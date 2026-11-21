//! Канонизатор Fanuc mill → EID/L1 AST (этап 1: модальности + движение).
//! Источники: fanuc_dialect_analysis.md v0.0 (контракт; §1.1 группы, §1.2 DPI,
//! §2.1 движение, §9.1 v0.1-скоуп), SPEC-EID-L1 v0.16 (§2 точки разрыва,
//! §4 state, §6 дуги, §8 RAW, §10 числа), паттерн — eid-grbl canonicalize.rs.
//!
//! DPI-политика (§1.2 ловушка №1, зафиксирована брифом): bare-integer (без
//! точки) в словах дробного домена X/Y/Z/R/Q/F/I/J/K трактуется по DPI=0 —
//! значение × least increment (IS-B: 0.001 mm при G21 / 0.0001" при G20) +
//! notice W_DPI_ASSUMED. Молчаливое чтение как целое запрещено (тихая ошибка
//! ×1000). Интерпретация — здесь, не в лексере: требует модального units.
//!
//! Центр дуги: I/J/K — инкрементальные от start ВСЕГДА (§2.1 отчёта),
//! независимо от G90/G91; G90.1 (редкая опция, группа 04) — абсолютный центр
//! как LinuxCNC. R-форма: знак R выбирает ≤180°/>180°; R при полной окружности
//! → E_INVALID_INPUT (SPEC §6(2), §6 отчёта).

use eid_core::ast::{Coolant, Dir, Frame, Num, Op, Point3, Program, Spindle, SrcSpan, StartPoint, State, Units, Wcs};
use eid_core::diag::{Diagnostic, ErrorCode, LimitInfo};
use crate::block::{parse_file, Block, Marker, ParsedFile};
use crate::lexer::LexErrorKind;
use crate::state::{plane_axes, plane_normal, GCode, MCode, ModalState};

// ---------------------------------------------------------------------------
// Notices (реплика паттерна eid-grbl canonicalize.rs; SPEC §3/§5.2/§6)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// rapid/linear: start == end (SPEC §3).
    ZeroLengthMotion,
    /// arc: start == end при sweep != 360° и pitch == 0 (SPEC §3/§6).
    ZeroLengthArc,
    /// Программа без M2/M30: неявный конец — ProgramEnd добавлен последним
    /// опом (SPEC §3 v0.16; анализ §4.4 — auto-PEND применим к Fanuc DNC).
    MissingProgramEnd,
    /// near-degenerate R-дуга (±0.12° вокруг 180°): амплитификация центра
    /// (EQUIVALENCE §4) — notice при живом опе.
    NearDegenerateRArc,
    /// W_DPI_ASSUMED (новый notice, анализ §1.2 ловушка №1): bare-integer в
    /// слове дробного домена прочитан по DPI=0 (× least increment).
    DpiAssumed,
}

#[derive(Debug, Clone)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
    /// Индекс опа в списке ops (для drop-notice — индекс, который оп ИМЕЛ БЫ).
    pub op_index: usize,
    pub op: Op,
}

/// Результат: программа + notices (drop/DPI/auto-PEND).
#[derive(Debug, Clone)]
pub struct ParseOut {
    pub program: Program,
    pub notices: Vec<Notice>,
}

// ---------------------------------------------------------------------------
// Числовые/диагностические хелперы
// ---------------------------------------------------------------------------

/// Квантование входа к 4dp (SPEC §10.6 — при построении Num).
fn quant(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// Вычисленное число → Num: 4 dp, трейлинг-нули нормализованы (SPEC §10.2).
pub(crate) fn num_f64(v: f64) -> Num {
    let mut s = format!("{:.4}", quant(v));
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".into();
    }
    Num::new(s, quant(v))
}

/// Слово с точкой → verbatim Num (SPEC §10.1), квантование значения §10.6.
fn verbatim(w: &crate::lexer::RawNum) -> Num {
    Num::new(w.text.clone(), quant(w.value))
}

/// DPI least increment по активным единицам (IS-B, §1.2): 0.001 mm / 0.0001".
fn least_increment(units: Units) -> f64 {
    match units {
        Units::Mm => 0.001,
        Units::In => 0.0001,
    }
}

fn span(file: &str, line: u32, len: usize) -> SrcSpan {
    SrcSpan { file: file.to_string(), line_start: line, col_start: 1, line_end: line, col_end: len as u32 + 1 }
}

/// Диагностика с явным этапом: Parse — лексические пути, Verify —
/// канонизационные инварианты (interface v0.1.2, паттерн eid-grbl).
fn err_staged(
    stage: eid_core::diag::Stage,
    code: ErrorCode,
    msg: impl Into<String>,
    file: &str,
    line: u32,
    len: usize,
) -> Diagnostic {
    Diagnostic { code, message: msg.into(), src: Some(span(file, line, len)), options: Vec::new(), limit: None, stage }
}

fn err(code: ErrorCode, msg: impl Into<String>, file: &str, line: u32, len: usize) -> Diagnostic {
    err_staged(eid_core::diag::Stage::Verify, code, msg, file, line, len)
}

fn limit_err(
    threshold: &'static str,
    value: f64,
    actual: f64,
    msg: String,
    file: &str,
    line: u32,
) -> Diagnostic {
    Diagnostic {
        code: ErrorCode::LimitExceeded,
        message: msg,
        src: Some(span(file, line, 0)),
        options: Vec::new(),
        limit: Some(LimitInfo { threshold, threshold_value: value, actual }),
        stage: eid_core::diag::Stage::Verify,
    }
}

fn e3(p: &Point3) -> [f64; 3] {
    [p.x.value(), p.y.value(), p.z.value()]
}

fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3 { x: num_f64(x), y: num_f64(y), z: num_f64(z) }
}

fn wcs_of_default() -> Wcs {
    Wcs::G54 // этап 1: единственный фрейм — дефолтный G54 (фреймы — этап 2)
}

/// hex-кодировка байтов строки (grammar D9; payload RAW — этап 2, хелпер готов).
fn payload_hex(line: &str) -> String {
    let mut s = String::new();
    for b in line.as_bytes() {
        s.push_str(&format!("{:02X}", b));
    }
    s
}

// ---------------------------------------------------------------------------
// Ctx — модальное состояние (state.rs ModalState) + позиционное отслеживание
// ---------------------------------------------------------------------------

struct Ctx {
    file: String,
    modal: ModalState,
    feed: Option<Num>,       // модальный F (переживает G0, SPEC §4)
    prev_end: Option<Point3>, // end последнего motion-опа
    ops: Vec<Op>,
    notices: Vec<Notice>,
    /// W_DPI_ASSUMED-сообщения слов, прочитанных до построения опа (напр.
    /// голый F в кадре без движения): прицепляются к следующему опу.
    /// RefCell — dpi_num(&self) живёт рядом с disjoint-захватом ctx.file
    /// в diag-замыканиях (паттерн grbl-адаптера).
    pending_dpi: std::cell::RefCell<Vec<String>>,
    end_count: usize,
    end_is_last: bool,
    end_line: u32,
}

impl Ctx {
    fn state(&self) -> State {
        State {
            feed: self.feed.clone(),
            // Этап 1: шпиндель/СОЖ/инструмент не обрабатываются (M-коды — этап 2).
            spindle: Spindle::Off,
            coolant: Coolant::default(),
            tool: None,
            frame: Frame::Wcs,
            wcs: wcs_of_default(),
        }
    }

    /// Записать W_DPI_ASSUMED-сообщение (прицепится к следующему опу).
    fn note_dpi(&self, msg: String) {
        self.pending_dpi.borrow_mut().push(msg);
    }

    /// Слово дробного домена → Num: bare-integer → × least increment
    /// (DPI=0, IS-B) + W_DPI_ASSUMED; с точкой → verbatim (§1.2).
    fn dpi_num(&self, letter: char, w: &crate::lexer::RawNum) -> Num {
        if w.has_point {
            return verbatim(w);
        }
        let inc = least_increment(self.modal.units);
        let v = w.value * inc;
        let (u_str, inc_str) = match self.modal.units {
            Units::Mm => ("mm", "0.001"),
            Units::In => ("inch", "0.0001"),
        };
        self.note_dpi(format!(
            "W_DPI_ASSUMED: {}{} (bare integer) read as {} ({} × least increment {} = {}; DPI=0 default, IS-B {}; analysis §1.2)",
            letter,
            w.text,
            num_f64(v).text,
            w.text,
            inc_str,
            num_f64(v).text,
            u_str,
        ));
        num_f64(v)
    }

    fn flush_dpi(&mut self, op_index: usize, op: &Op) {
        let pending = std::mem::take(&mut *self.pending_dpi.borrow_mut());
        for msg in pending {
            self.notices.push(Notice {
                kind: NoticeKind::DpiAssumed,
                message: msg,
                op_index,
                op: op.clone(),
            });
        }
    }

    /// push при живом опе (+ flush DPI-notice'ов на этот оп).
    fn push(&mut self, op: Op) {
        self.ops.push(op);
        let idx = self.ops.len() - 1;
        let op_ref = self.ops[idx].clone();
        self.flush_dpi(idx, &op_ref);
    }

    /// drop-with-notice (оп НЕ попадает в ops, SPEC §3) + flush DPI.
    fn push_drop(&mut self, op: Op, kind: NoticeKind, message: String) {
        let idx = self.ops.len();
        self.flush_dpi(idx, &op);
        self.notices.push(Notice { kind, message, op_index: idx, op });
    }

    /// notice при ЖИВОМ опе (near-degenerate R-arc) + flush DPI.
    fn push_keep(&mut self, op: Op, kind: NoticeKind, message: String) {
        let idx = self.ops.len();
        self.ops.push(op.clone());
        self.flush_dpi(idx, &op);
        self.notices.push(Notice { kind, message, op_index: idx, op });
    }

    fn raw_op(&self, b: &Block, code: ErrorCode, reason: &str) -> Op {
        Op::Raw {
            payload_hex: payload_hex(&b.raw),
            reason: reason.to_string(),
            code,
            src: span(&self.file, b.line_no, b.raw.len()),
        }
    }

    fn mark_end(&mut self, line: u32, is_last: bool, blen: usize) {
        self.end_count += 1;
        self.end_line = line;
        self.end_is_last = is_last;
        let op = Op::ProgramEnd { src: span(&self.file, line, blen) };
        self.push(op);
    }
}

// ---------------------------------------------------------------------------
// Входные точки
// ---------------------------------------------------------------------------

pub fn canonicalize(src: &str, file: &str) -> Result<Program, Vec<Diagnostic>> {
    canonicalize_with_notices(src, file).map(|o| o.program)
}

pub fn canonicalize_with_notices(src: &str, file: &str) -> Result<ParseOut, Vec<Diagnostic>> {
    // Лимит max_source_lines (COVERAGE §1.2).
    let line_count = src.lines().count() as f64;
    if line_count > 1_000_000.0 {
        return Err(vec![limit_err(
            "max_source_lines", 1_000_000.0, line_count,
            format!("file has {} lines, limit 1000000", src.lines().count()),
            file, 1,
        )]);
    }

    let parsed: ParsedFile = parse_file(src);
    // Лексические ошибки: мусор/комментарии → INVALID (Parse); переполнения —
    // LIMIT (§1.2: целая часть >9 цифр max_word_chars не ловит).
    if !parsed.lex_errors.is_empty() {
        return Err(parsed
            .lex_errors
            .iter()
            .map(|e| match &e.kind {
                LexErrorKind::WordTooLong => limit_err(
                    "max_word_chars", 32.0, 32.0,
                    "word value exceeds max_word_chars (32)".into(), file, e.line,
                ),
                LexErrorKind::IntOverflow { digits } => limit_err(
                    "max_integer_digits", 9.0, *digits as f64,
                    format!("integer part of {} digits exceeds max_integer_digits (9) — Fanuc range ±99999.9999 (analysis §1.2)", digits),
                    file, e.line,
                ),
                kind => err_staged(
                    eid_core::diag::Stage::Parse,
                    ErrorCode::InvalidInput,
                    format!("lexical error: {:?} at col {}", kind, e.col),
                    file, e.line, e.col as usize,
                ),
            })
            .collect());
    }

    let mut ctx = Ctx {
        file: file.to_string(),
        modal: ModalState::default(),
        feed: None,
        prev_end: None,
        ops: Vec::new(),
        notices: Vec::new(),
        pending_dpi: std::cell::RefCell::new(Vec::new()),
        end_count: 0,
        end_is_last: true,
        end_line: 0,
    };

    let n = parsed.blocks.len();
    for (i, b) in parsed.blocks.iter().enumerate() {
        process_block(&mut ctx, b, i + 1 == n)?;
    }
    finalize(&mut ctx, line_count as u32)
}

/// Структурная проверка program_end (SPEC §3 + анализ §4.4/§7.3):
/// Fanuc-политика хвоста — ВСЁ после первого program_end — line-level
/// Raw-хвост {E_UNSUPPORTED_SYNTAX} (тела subs O#### в том же файле —
/// норма Fanuc), НЕ жёсткий Err (в отличие от grbl n12: raw-сохранение
/// хвоста честно, motion-опы после PEND не строятся — process_block
/// отсекает по end_count). Дубль M2/M30 — defensive-guard (недостижим
/// при tail-политике). Отсутствие M2/M30 → auto-PEND + notice (v0.16).
fn finalize(ctx: &mut Ctx, line_count: u32) -> Result<ParseOut, Vec<Diagnostic>> {
    if ctx.end_count > 1 {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "duplicate program end: more than one M2/M30 (SPEC §3; analysis §4.4)",
            &ctx.file, ctx.end_line, 0,
        )]);
    }
    let _ = ctx.end_is_last; // Fanuc: хвост → Raw (см. process_block), не Err
    if ctx.end_count == 0 {
        // SPEC §3 v0.16 / анализ §4.4: DNC-файлы Fanuc часто без M30 —
        // авто-вставка ProgramEnd последним опом + notice.
        let op = Op::ProgramEnd {
            src: SrcSpan {
                file: ctx.file.clone(),
                line_start: line_count.max(1),
                col_start: 1,
                line_end: line_count.max(1),
                col_end: 1,
            },
        };
        ctx.push_keep(
            op,
            NoticeKind::MissingProgramEnd,
            "W_MISSING_PROGRAM_END: no M2/M30 — implicit program end appended (SPEC §3 v0.16; analysis §4.4)".into(),
        );
    }
    // Лимит max_ops_per_program (COVERAGE §1.2) — по итоговому ops+notices.
    let ops_n = (ctx.ops.len() + ctx.notices.len()) as f64;
    if ops_n > 200_000.0 {
        return Err(vec![limit_err(
            "max_ops_per_program", 200_000.0, ops_n,
            format!("program yields {} ops, limit 200000", ops_n as u64),
            &ctx.file, 1,
        )]);
    }
    let program = Program {
        // grammar v0.3 — magic-строка (SPEC §10.4).
        eid_version: "EID/L1 v0.3".to_string(),
        units: ctx.modal.units,
        // SPEC §3 v0.12: отсутствие G61/G64 → exact_path (дефолт);
        // path-коды Fanuc (G61/G64) — этап 2/3.
        path_mode: eid_core::ast::PathMode::ExactPath,
        frame_preset: None, // G92 — этап 2
        ops: std::mem::take(&mut ctx.ops),
    };
    let notices = std::mem::take(&mut ctx.notices);
    Ok(ParseOut { program, notices })
}

// ---------------------------------------------------------------------------
// process_block — диспетчер одного кадра
// ---------------------------------------------------------------------------

fn process_block(ctx: &mut Ctx, b: &Block, is_last: bool) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    // (0) Fanuc-политика хвоста (анализ §4.4/§7.3): блоки после program_end —
    // тела subs (O####) → line-level Raw{SYNTAX}, НЕ Err (в отличие от grbl
    // n12); motion-опы после PEND не строятся (SPEC §3: PEND — последний оп).
    if ctx.end_count > 0 {
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSyntax,
            "block after program end: subprogram tail preserved raw (analysis §4.4/§7.3; O-structure refined in stage 2)",
        );
        ctx.push(op);
        return Ok(());
    }

    // (1) Строковые маркеры.
    match &b.marker {
        Some(Marker::Percent) => return Ok(()), // IGNORED_SAFE (§1.3)
        Some(Marker::BlockDelete) => {
            // §1.3: исполнение зависит от переключателя станка —
            // RAW(E_RUNTIME_STATE), как LinuxCNC-строка COVERAGE §4.
            let op = ctx.raw_op(
                b,
                ErrorCode::RuntimeState,
                "block delete '/': execution depends on machine switch (analysis §1.3)",
            );
            ctx.push(op);
            return Ok(());
        }
        None => {}
    }

    // (2) Нарушение модальной группы (§1.1: один код на группу на блок) —
    // сломанный вход.
    if let Some((group, g)) = b.modal_violation {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            format!("modal group violation in block: {:?} after {:?}", group, g),
            &ctx.file, line, blen,
        )]);
    }

    // (3) Слова вне словаря → line-level Raw (молчаливый пропуск запрещён).
    if !b.unsupported_words.is_empty() {
        let has_o = b.unsupported_words.iter().any(|w| w.letter == 'O');
        let (code, reason) = if has_o {
            (
                ErrorCode::UnsupportedSyntax,
                "O-code: program number / subprogram definition — outside L1 (analysis §1.3, SPEC §1.1 v0.16)",
            )
        } else {
            (
                ErrorCode::UnsupportedSyntax,
                "word outside stage-1 dictionary — provisional RAW; disposition per analysis §2/§3 refined in stage 2/3",
            )
        };
        let op = ctx.raw_op(b, code, reason);
        ctx.push(op);
        return Ok(());
    }

    // (4) Единицы: ровно одна система на программу (SPEC §2.5). Первое слово
    // G20/G21 (или псевдоним G70/G71, §2.2) устанавливает; повторная смена —
    // отказ (анализ: одно переключение на программу).
    for g in &b.g_codes {
        if let Some(u) = g.units() {
            if u != ctx.modal.units {
                if ctx.modal.units_established {
                    return Err(vec![err(
                        ErrorCode::UnsupportedSemantics,
                        "unit switch mid-program: one unit system per program (SPEC §2.5)",
                        &ctx.file, line, blen,
                    )]);
                }
                ctx.modal.units = u;
            }
            ctx.modal.units_established = true;
        }
    }

    // (5) G95 — детект для RAW (§2.2: feed/rev требует rpm, зависящего от
    // override; статическая эквивалентность не гарантируется).
    if b.g_codes.contains(&GCode::FeedPerRev) {
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSemantics,
            "G95 feed-per-rev: resolution needs spindle rpm (override-dependent) — not representable (analysis §2.2)",
        );
        ctx.push(op);
        return Ok(());
    }

    // (6) ABSORB модальных групп (01–05, units уже проверены в (4)).
    for g in &b.g_codes {
        ctx.modal.apply(*g);
    }

    // (7) Слова: F — модальная подача (DPI-домен, §1.2); S — этап 2
    // (голое S — легальное сохранение оборотов, §3 отчёта).
    if let Some(f) = b.word('F') {
        let num = ctx.dpi_num('F', f);
        ctx.feed = Some(num);
    }

    // (8) Движение (G0–G3 / модальное продолжение, §2.7).
    motion_block(ctx, b)?;

    // (9) M2/M30 → ProgramEnd (после motion — порядок grbl #9; SPEC §5.2).
    if b.m_codes.contains(&MCode::ProgramEnd) {
        ctx.mark_end(line, is_last, blen);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Движение
// ---------------------------------------------------------------------------

/// Текущее чтение позиции (SPEC §2). Этап 1: фреймов нет (G54/G10/G92/G53 —
/// этап 2), единственная точка разрыва — (a) первый motion-оп без якоря.
fn reading_now(ctx: &Ctx) -> Option<Point3> {
    ctx.prev_end.clone()
}

/// Целевая точка хода: слова (G90 abs / G91 inc) + наследование reading.
/// SPEC §2 v0.11: в точке разрыва (a) отсутствующие оси end наследуют дефолт
/// контроллера (0); после genuinely-неизвестных позиций — E_RUNTIME_STATE
/// (этап 2: точки разрыва (b)–(d)).
fn resolve_end(
    ctx: &Ctx,
    b: &Block,
    reading: &Option<Point3>,
) -> Result<Point3, Vec<Diagnostic>> {
    let rv = reading.as_ref().map(|p| e3(p));
    let at_start = reading.is_none() && ctx.prev_end.is_none();
    let mut v: [Option<Num>; 3] = std::array::from_fn(|_| None);
    for (i, w) in [(0usize, 'X'), (1, 'Y'), (2, 'Z')] {
        v[i] = match b.word(w) {
            Some(n) => {
                let num = ctx.dpi_num(w, n);
                if ctx.modal.dist_abs {
                    Some(num)
                } else {
                    let base = match rv {
                        Some(r) => r,
                        None if at_start => [0.0, 0.0, 0.0],
                        None => {
                            return Err(vec![err(
                                ErrorCode::RuntimeState,
                                "incremental move requires known current position (SPEC §2)",
                                &ctx.file, b.line_no, b.raw.len(),
                            )]);
                        }
                    };
                    Some(num_f64(base[i] + num.value()))
                }
            }
            None => match rv {
                Some(r) => Some(num_f64(r[i])),
                None if at_start => Some(num_f64(0.0)), // дефолт контроллера (§2 v0.11)
                None => {
                    return Err(vec![err(
                        ErrorCode::RuntimeState,
                        "partial end with unknown reading inherits unknown (SPEC §2)",
                        &ctx.file, b.line_no, b.raw.len(),
                    )]);
                }
            },
        };
    }
    match (v[0].take(), v[1].take(), v[2].take()) {
        (Some(x), Some(y), Some(z)) => Ok(Point3 { x, y, z }),
        _ => unreachable!("stage 1: reading known ⇒ все оси разрешены"),
    }
}

fn motion_block(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let has_axis = b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some();
    let has_arc_words = b.word('I').is_some() || b.word('J').is_some() || b.word('K').is_some() || b.word('R').is_some();

    // Голые координаты без motion-кода: модальное продолжение группы 01
    // (§2.7 отчёта — адресный язык); без активного motion — INVALID.
    let mode = match ctx.modal.motion {
        Some(m) => m,
        None if has_axis || has_arc_words => {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                "naked coordinate words: no active motion mode (analysis §2.7)",
                &ctx.file, line, blen,
            )]);
        }
        None => return Ok(()),
    };
    if !has_axis && !has_arc_words {
        return Ok(()); // кадр без целевых слов — движения нет
    }

    let reading = reading_now(ctx);

    if matches!(mode, GCode::ArcCw | GCode::ArcCcw) {
        return arc_block(ctx, b, mode, reading);
    }

    let end = resolve_end(ctx, b, &reading)?;
    let start = reading.map(StartPoint::Known).unwrap_or(StartPoint::Unknown);

    // zero-length (start == end) → drop + notice (SPEC §3).
    if let StartPoint::Known(s) = &start {
        if len3(&sub3(&end, s)) < 1e-9 {
            let op = if mode == GCode::Rapid {
                Op::Rapid { start: start.clone(), end: end.clone(), state: ctx.state(), src: span(&ctx.file, line, blen), sub: None }
            } else {
                Op::Linear { start: start.clone(), end: end.clone(), state: ctx.state(), src: span(&ctx.file, line, blen), sub: None }
            };
            ctx.push_drop(op, NoticeKind::ZeroLengthMotion, "zero-length motion dropped (start == end, SPEC §3)".into());
            ctx.prev_end = Some(end);
            return Ok(());
        }
    }

    // feed: G0 не требует; linear без F → INVALID (SPEC §3/§4).
    let op = if mode == GCode::Rapid {
        Op::Rapid { start, end: end.clone(), state: ctx.state(), src: span(&ctx.file, line, blen), sub: None }
    } else {
        if ctx.feed.is_none() {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                "linear move before any F word (SPEC §3/§4)",
                &ctx.file, line, blen,
            )]);
        }
        Op::Linear { start, end: end.clone(), state: ctx.state(), src: span(&ctx.file, line, blen), sub: None }
    };
    ctx.push(op);
    ctx.prev_end = Some(end);
    Ok(())
}

/// Дуга G2/G3 (§2.1 отчёта, SPEC §6): IJK — инкрементальные offsets от start
/// ВСЕГДА (независимо от G90/G91); G90.1 (группа 04, опция) — абсолютный
/// центр как LinuxCNC. R-форма: знак R выбирает ≤180°/>180°; R при полной
/// окружности → E_INVALID_INPUT. Helical — вне-плоскостное осевое слово →
/// pitch; полная окружность (нет end-слов + center ≠ start) → sweep 360°.
/// Валидация нормали по активной плоскости (SPEC §4/§6): перпендикулярная
/// компонента (center − start) ≈ 0, иначе E_INVALID_INPUT.
fn arc_block(ctx: &mut Ctx, b: &Block, mode: GCode, reading: Option<Point3>) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let inval = |msg: String| err(ErrorCode::InvalidInput, msg, &ctx.file, line, blen);
    // ARC с start=unknown — E_RUNTIME_STATE (геометрия требует числового
    // старта, SPEC §2).
    let start = reading.ok_or_else(|| {
        vec![err(ErrorCode::RuntimeState, "arc with unknown start (SPEC §2)", &ctx.file, line, blen)]
    })?;
    let end = resolve_end(ctx, b, &Some(start.clone()))?;

    let (a0, a1, al) = plane_axes(ctx.modal.plane);
    let s = e3(&start);
    let e = e3(&end);
    let pitch = e[al] - s[al]; // вне-плоскостная компонента хода (винт)
    let dir = if mode == GCode::ArcCw { Dir::Cw } else { Dir::Ccw };
    let normal = plane_normal(ctx.modal.plane);

    // Центр: in-plane пара; линейная компонента = start (круг в плоскости старта).
    let mut c = s;
    let r_val = b.word('R').map(|n| ctx.dpi_num('R', n)).map(|n| n.value());
    let d0 = e[a0] - s[a0];
    let d1 = e[a1] - s[a1];
    let chord_plane = (d0 * d0 + d1 * d1).sqrt();
    match r_val {
        Some(r) => {
            // R при совпадающих in-plane start/end — центр неоднозначен
            // (SPEC §6(2); R-полный круг — alarm обоих контроллеров; §6 отчёта).
            if chord_plane < 1e-9 {
                return Err(vec![inval(
                    "R-arc with coincident in-plane start/end: ambiguous center — full circle only via IJK (SPEC §6; analysis §6)".into(),
                )]);
            }
            // Хорда > 2R — невозможная дуга (SPEC §6).
            let tol_r = 0.001f64.max(1e-6 * r.abs());
            let h2 = 4.0 * r * r - d0 * d0 - d1 * d1;
            if h2 < -tol_r * 4.0 * r.abs().max(1.0) {
                return Err(vec![inval(format!(
                    "arc chord {chord_plane:.4} > 2R {}: impossible arc (SPEC §6)", r.abs()
                ))]);
            }
            // Формула LinuxCNC/grbl: центр = середина хорды ± h·p̂; знак
            // направления + знак R (≤180° / >180°).
            let mut sgn = if mode == GCode::ArcCw { -1.0 } else { 1.0 };
            if r < 0.0 {
                sgn = -sgn;
            }
            let h = h2.max(0.0).sqrt() / 2.0;
            let (px, py) = (-d1 / chord_plane, d0 / chord_plane);
            c[a0] = (e[a0] + s[a0]) / 2.0 + sgn * h * px;
            c[a1] = (e[a1] + s[a1]) / 2.0 + sgn * h * py;
        }
        None => {
            // IJK: пара осей плоскости; буквенные соответствия I↔X, J↔Y, K↔Z.
            let mut off_vals = [0.0f64; 3];
            for (i, letter) in ['I', 'J', 'K'].into_iter().enumerate() {
                if let Some(n) = b.word(letter) {
                    off_vals[i] = ctx.dpi_num(letter, n).value();
                }
            }
            // Валидация нормали (SPEC §4/§6): перпендикулярная компонента
            // (center − start) ≈ 0 → вне-плоскостное слово обязано быть ~0.
            if off_vals[al].abs() > 1e-9 {
                return Err(vec![inval(format!(
                    "perpendicular arc-center component {}{} in plane {}: center must lie in the start plane (SPEC §4/§6 plane validation)",
                    ['I', 'J', 'K'][al], num_f64(off_vals[al]).text, ['X', 'Y', 'Z'][al],
                ))]);
            }
            if ctx.modal.arc_center_abs {
                // G90.1: абсолютный центр (как LinuxCNC; анализ §2.2 —
                // по умолчанию Fanuc инкрементальный, G90.1 — опция).
                c[a0] = off_vals[a0];
                c[a1] = off_vals[a1];
            } else {
                // G91.1-эквивалент (дефолт Fanuc): инкремент от start ВСЕГДА,
                // независимо от G90/G91 (§2.1 отчёта).
                c[a0] = s[a0] + off_vals[a0];
                c[a1] = s[a1] + off_vals[a1];
            }
        }
    }

    // Sweep: угол от старта к цели в направлении дуги (SPEC §6, (0°, 360°]).
    // SPEC §10.5 — single-final-rounding: производные в f64 от сырых входов,
    // округление к 4dp однократно (num_f64).
    let two_pi = 2.0 * std::f64::consts::PI;
    let sweep = match r_val {
        // R-форма: из хорды и |R| (закон хорд): 2·asin(c/2R) — ≤180° при R>0,
        // дополнение до 360° при R<0.
        Some(r) => {
            let sw = 2.0 * (chord_plane / (2.0 * r.abs())).clamp(0.0, 1.0).asin();
            if r < 0.0 { two_pi - sw } else { sw }
        }
        // IJK: полярные углы от центра; совпадение в плоскости = 360°
        // (полная окружность: нет end-слов + center ≠ start, §6 отчёта).
        None => {
            let a_s = (s[a1] - c[a1]).atan2(s[a0] - c[a0]);
            let a_e = (e[a1] - c[a1]).atan2(e[a0] - c[a0]);
            let mut sweep_ccw = (a_e - a_s).rem_euclid(two_pi);
            if sweep_ccw < 1e-9 {
                sweep_ccw = two_pi;
            }
            let sw = if mode == GCode::ArcCw { two_pi - sweep_ccw } else { sweep_ccw };
            if sw < 1e-9 { two_pi } else { sw }
        }
    };

    // zero-length arc (SPEC §3/§6): end-слова, квантованные до стартовой
    // точки (§10.6), — нулевой ход, не полное кольцо (§6: 360° — только
    // наследованный end / отсутствие end-слов).
    let rho_s = ((s[a0] - c[a0]).powi(2) + (s[a1] - c[a1]).powi(2)).sqrt();
    let rho_e = ((e[a0] - c[a0]).powi(2) + (e[a1] - c[a1]).powi(2)).sqrt();
    let explicit_end = b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some();
    if explicit_end && chord_plane < 1e-9 && pitch.abs() < 1e-9 {
        let mut st = ctx.state();
        st.feed = ctx.feed.clone();
        let op = Op::Arc {
            start: StartPoint::Known(start),
            end: end.clone(),
            center: pt(c[0], c[1], c[2]),
            normal: pt(normal[0], normal[1], normal[2]),
            dir,
            sweep_deg: num_f64(sweep.to_degrees()),
            pitch: num_f64(pitch),
            state: st,
            src: span(&ctx.file, line, blen),
            sub: None,
        };
        ctx.push_drop(
            op,
            NoticeKind::ZeroLengthArc,
            "zero-length arc dropped (end quantizes to start, sweep != 360°, pitch == 0; SPEC §3/§6)".into(),
        );
        ctx.prev_end = Some(end);
        return Ok(());
    }
    let mut near_degenerate = false;
    if b.word('R').is_some() && (180.0 - sweep.to_degrees()).abs() < 0.114 {
        near_degenerate = true; // |cos(sweep/2)| < 1e-3 (EQUIVALENCE §4)
    }

    // Валидация §6: планиметрический радиус ρ_start ≈ ρ_end.
    let tol_r = 0.001f64.max(1e-6 * rho_s.max(rho_e));
    if (rho_s - rho_e).abs() > tol_r {
        return Err(vec![inval(format!(
            "arc radius mismatch: ρ_start {rho_s:.4} vs ρ_end {rho_e:.4} (SPEC §6)"
        ))]);
    }
    // Уравнение позиции: end ≈ c + Rot(n, dir, sweep)·(start − c) + pitch·n.
    let v = {
        let th = if mode == GCode::ArcCw { -sweep } else { sweep };
        let rv = [s[0] - c[0], s[1] - c[1], s[2] - c[2]];
        let cos = th.cos();
        let sin = th.sin();
        let nv = [rv[0] * normal[0] + rv[1] * normal[1] + rv[2] * normal[2]];
        let cr = [
            normal[1] * rv[2] - normal[2] * rv[1],
            normal[2] * rv[0] - normal[0] * rv[2],
            normal[0] * rv[1] - normal[1] * rv[0],
        ];
        [
            rv[0] * cos + cr[0] * sin + normal[0] * nv[0] * (1.0 - cos),
            rv[1] * cos + cr[1] * sin + normal[1] * nv[0] * (1.0 - cos),
            rv[2] * cos + cr[2] * sin + normal[2] * nv[0] * (1.0 - cos),
        ]
    };
    let eps_pos = 0.001f64.max(1e-7 * len3(&sub3(&end, &start)));
    let mut resid2 = 0.0;
    for i in 0..3 {
        let pred = c[i] + v[i] + pitch * normal[i];
        resid2 += (e[i] - pred).powi(2);
    }
    if resid2.sqrt() > eps_pos {
        return Err(vec![inval(format!(
            "arc rotation-equation residual {:.4} > {eps_pos:.4} (SPEC §6.3)",
            resid2.sqrt()
        ))]);
    }

    if ctx.feed.is_none() {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "arc move before any F word (SPEC §3/§4)",
            &ctx.file, line, blen,
        )]);
    }
    let mut st = ctx.state();
    st.feed = ctx.feed.clone();
    let op = Op::Arc {
        start: StartPoint::Known(start),
        end: end.clone(),
        center: pt(c[0], c[1], c[2]),
        normal: pt(normal[0], normal[1], normal[2]),
        dir,
        sweep_deg: num_f64(sweep.to_degrees()),
        pitch: num_f64(pitch),
        state: st,
        src: span(&ctx.file, line, blen),
        sub: None,
    };
    if near_degenerate {
        ctx.push_keep(
            op,
            NoticeKind::NearDegenerateRArc,
            "near-degenerate R-arc (±0.12° around 180°): center amplification (EQUIVALENCE §4)".into(),
        );
    } else {
        ctx.push(op);
    }
    ctx.prev_end = Some(end);
    Ok(())
}

fn sub3(a: &Point3, b: &Point3) -> Point3 {
    let [x, y, z] = e3(a);
    let [u, v, w] = e3(b);
    pt(x - u, y - v, z - w)
}

fn len3(d: &Point3) -> f64 {
    let [x, y, z] = e3(d);
    (x * x + y * y + z * z).sqrt()
}
