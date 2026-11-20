//! Канонизатор GRBL 1.1 → EID/L1 AST (Агент C, этап 2).
//! Источники: SPEC v0.10 (§2 точки разрыва (a)–(d), §4 state, §5.3 machine-frame,
//! §6 дуги), COVERAGE §2 (диспозиции), CORPUS §1.2 (правило двух классов:
//! структурные нарушения → жёсткий Err; локальные конструкции → Raw-узел).
//! drop-with-notice — паттерн eid-core parser.rs (Notice, SPEC §3).

use std::collections::HashSet;
use eid_core::ast::{Coolant, Dir, Frame, Num, Op, Point3, Program, Spindle, SrcSpan, StartPoint, State, Units, Wcs};
use eid_core::diag::{Diagnostic, ErrorCode, LimitInfo};
use crate::block::{Block, G10Kind, Marker, ParsedFile};
use crate::gcode::{GCode, MCode};

// ---------------------------------------------------------------------------
// Notices (реплика паттерна eid-core parser.rs; SPEC §3/§5.2/§6)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// rapid/linear: start == end (SPEC §3).
    ZeroLengthMotion,
    /// arc: start == end при sweep != 360° и pitch == 0 (SPEC §3/§6).
    ZeroLengthArc,
    /// dwell: seconds == 0 (SPEC §5.2).
    ZeroDwell,
    /// R-форма около 180°: cos(sweep/2) < 1e-3 — амплитификация погрешности
    /// центра (EQUIVALENCE §4: notice, не отказ).
    NearDegenerateRArc,
}

#[derive(Debug, Clone)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
    /// Индекс опа в списке ops (для NearDegenerateRArc оп НЕ отброшен).
    pub op_index: usize,
    pub op: Op,
}

/// Результат: программа + notices (отброшенные zero-length / near-degenerate).
#[derive(Debug, Clone)]
pub struct ParseOut {
    pub program: Program,
    pub notices: Vec<Notice>,
}

// ---------------------------------------------------------------------------
// Числовые/диагностические хелперы
// ---------------------------------------------------------------------------

/// Вычисленное число → Num: 4 dp, трейлинг-нули нормализованы (SPEC §10.2).
pub(crate) fn num_f64(v: f64) -> Num {
    let mut s = format!("{:.4}", v);
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
    Num::new(s, v)
}

fn verbatim(n: &Num) -> Num {
    n.clone()
}

fn span(file: &str, line: u32, len: usize) -> SrcSpan {
    SrcSpan { file: file.to_string(), line_start: line, col_start: 1, line_end: line, col_end: len as u32 + 1 }
}

fn err(code: ErrorCode, msg: impl Into<String>, file: &str, line: u32, len: usize) -> Diagnostic {
    Diagnostic { code, message: msg.into(), src: Some(span(file, line, len)), options: Vec::new(), limit: None }
}

fn limit_err(threshold: &'static str, value: f64, actual: f64, msg: String, file: &str, line: u32) -> Diagnostic {
    Diagnostic {
        code: ErrorCode::LimitExceeded,
        message: msg,
        src: Some(span(file, line, 0)),
        options: Vec::new(),
        limit: Some(LimitInfo { threshold, threshold_value: value, actual }),
    }
}

fn e3(p: &Point3) -> [f64; 3] {
    [p.x.value(), p.y.value(), p.z.value()]
}

fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3 { x: num_f64(x), y: num_f64(y), z: num_f64(z) }
}

/// Плоскость → (axis_0, axis_1, axis_linear) — определение grbl gcode.c
/// (COVERAGE §6.11: G18 = ZX, G19 = YZ). Нормаль = +axis_linear (SPEC §6).
fn plane_axes(g: &GCode) -> (usize, usize, usize) {
    match g {
        GCode::PlaneXY => (0, 1, 2),
        GCode::PlaneZX => (2, 0, 1),
        GCode::PlaneYZ => (1, 2, 0),
        _ => (0, 1, 2),
    }
}

fn wcs_from_p(p: u8) -> Wcs {
    match p {
        1 => Wcs::G54,
        2 => Wcs::G55,
        3 => Wcs::G56,
        4 => Wcs::G57,
        5 => Wcs::G58,
        _ => Wcs::G59,
    }
}

fn wcs_to_p(w: &Wcs) -> u8 {
    match w {
        Wcs::G54 => 1,
        Wcs::G55 => 2,
        Wcs::G56 => 3,
        Wcs::G57 => 4,
        Wcs::G58 => 5,
        _ => 6,
    }
}

/// hex-кодировка байтов строки (grammar D9: чётная длина, A–F uppercase);
/// байты verbatim включая \r (CRLF fidelity).
fn payload_hex(line: &str) -> String {
    let mut s = String::new();
    for b in line.as_bytes() {
        s.push_str(&format!("{:02X}", b));
    }
    s
}

// ---------------------------------------------------------------------------
// Ctx — модальное состояние канонизации (SPEC §4) + позиционное отслеживание
// ---------------------------------------------------------------------------

struct Ctx {
    file: String,
    units: Units,           // G20/G21 (дефолт Mm = дефолт GRBL)
    dist_abs: bool,         // G90=true / G91=false
    feed_inverse: bool,     // G93
    plane: GCode,           // G17/G18/G19 — модальное состояние парсера (SPEC §4)
    motion: Option<GCode>,  // модальный motion (G80 → None)
    wcs: Wcs,
    selected_wcs: HashSet<u8>,      // §5.3(b): все выбранные до хода
    wcs_origins: Vec<(u8, Point3)>, // wcs_define (G10 L2) до текущей точки
    feed: Option<Num>,      // модальный F (переживает G0, SPEC §4)
    last_s: Option<Num>,
    spindle: Spindle,
    coolant: Coolant,
    anchor: Option<(Point3, Wcs)>,  // пресетка (G92/L20 full-triple): reading старта
    frame_def_count: usize,         // G92/L20 — не более одной пресетки (N17)
    prev_end: Option<Point3>,       // end последнего motion-опа (в его reading)
    prev_is_machine: bool,          // последний motion был G53
    prev_wcs: Wcs,                  // reading prev_end (wcs или Machine)
    pos_mach: Option<Point3>,       // машинные координаты текущей позиции
    ops: Vec<Op>,
    notices: Vec<Notice>,
    path_exact: bool,               // G61
    units_mm_seen_motion: usize,    // счётчик motion-опов
    g43_family_seen: bool,          // правило семейств §1.3 (G49): G43.1 в файле
    g92_values: Option<(Point3, Wcs)>, // для Program.frame_preset (только G92)
    end_count: usize,               // M2/M30
    end_is_last: bool,
    end_line: u32,
}

impl Ctx {
    fn state(&self, frame: Frame) -> State {
        State {
            feed: self.feed.clone(),
            spindle: self.spindle.clone(),
            coolant: self.coolant,
            tool: None, // GRBL: M6/T вне словаря — инструмент не устанавливается
            frame,
            wcs: self.wcs,
        }
    }

    fn origin_of(&self, p: u8) -> Option<&Point3> {
        self.wcs_origins.iter().find(|(q, _)| *q == p).map(|(_, o)| o)
    }

    fn push(&mut self, op: Op) {
        self.ops.push(op);
    }

    /// drop-with-notice (оп НЕ попадает в ops, SPEC §3).
    fn push_drop(&mut self, op: Op, kind: NoticeKind, message: String) {
        let idx = self.ops.len();
        self.notices.push(Notice { kind, message, op_index: idx, op });
    }

    /// notice при ЖИВОМ опе (near-degenerate R-arc, EQUIVALENCE §4).
    fn push_keep(&mut self, op: Op, kind: NoticeKind, message: String) {
        let idx = self.ops.len();
        self.ops.push(op.clone());
        self.notices.push(Notice { kind, message, op_index: idx, op });
    }

    fn line(&self, b: &Block) -> u32 {
        b.line_no
    }

    fn raw_op(&self, b: &Block, raw_line: &str, code: ErrorCode, reason: &str) -> Op {
        Op::Raw {
            payload_hex: payload_hex(raw_line),
            reason: reason.to_string(),
            code,
            src: span(&self.file, b.line_no, raw_line.len()),
        }
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

    let parsed: ParsedFile = crate::block::parse_file(src);
    // Лексические ошибки: мусор → E_INVALID_INPUT; длинное слово → LIMIT (§1.2).
    if !parsed.lex_errors.is_empty() {
        return Err(parsed
            .lex_errors
            .iter()
            .map(|e| match e.kind {
                crate::lexer::LexErrorKind::WordTooLong => limit_err(
                    "max_word_chars", 32.0, 32.0,
                    "word value exceeds max_word_chars (32)".into(), file, e.line,
                ),
                kind => err(
                    ErrorCode::InvalidInput,
                    format!("lexical error: {:?} at col {}", kind, e.col),
                    file, e.line, e.col as usize,
                ),
            })
            .collect());
    }

    let mut ctx = Ctx {
        file: file.to_string(),
        units: Units::Mm,
        dist_abs: true,
        feed_inverse: false,
        plane: GCode::PlaneXY,
        motion: None,
        wcs: Wcs::G54,
        selected_wcs: HashSet::from([1]),
        wcs_origins: Vec::new(),
        feed: None,
        last_s: None,
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        anchor: None,
        frame_def_count: 0,
        prev_end: None,
        prev_is_machine: false,
        prev_wcs: Wcs::G54,
        pos_mach: None,
        ops: Vec::new(),
        notices: Vec::new(),
        path_exact: false,
        units_mm_seen_motion: 0,
        g43_family_seen: parsed
            .blocks
            .iter()
            .any(|b| b.g_codes.contains(&GCode::ToolLenDynamic)),
        g92_values: None,
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

/// Структурная проверка program_end (N12/N13/N14 → Err INVALID) + сборка Program.
fn finalize(ctx: &mut Ctx, line_count: u32) -> Result<ParseOut, Vec<Diagnostic>> {
    // Лимит max_ops_per_program (COVERAGE §1.2).
    let ops_n = (ctx.ops.len() + ctx.notices.len()) as f64;
    if ops_n > 200_000.0 {
        return Err(vec![limit_err(
            "max_ops_per_program", 200_000.0, ops_n,
            format!("program yields {} ops, limit 200000", ops_n as u64),
            &ctx.file, 1,
        )]);
    }
    if ctx.end_count == 0 {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "program end missing: no M2/M30 as the last block (CORPUS n14)",
            &ctx.file, line_count.max(1), 0,
        )]);
    }
    if ctx.end_count > 1 {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "duplicate program end: more than one M2/M30 (CORPUS n13)",
            &ctx.file, ctx.end_line, 0,
        )]);
    }
    if !ctx.end_is_last {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "program end is not the last block (CORPUS n12)",
            &ctx.file, ctx.end_line, 0,
        )]);
    }
    let program = Program {
        eid_version: "EID/L1 v0.2".to_string(),
        units: ctx.units,
        path_mode: if ctx.path_exact {
            eid_core::ast::PathMode::ExactPath
        } else {
            eid_core::ast::PathMode::Blend { t: None }
        },
        frame_preset: ctx.g92_values.take().map(|(values, wcs)| eid_core::ast::FramePreset { values, wcs }),
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

    // (1) Строковые маркеры.
    match &b.marker {
        Some(Marker::Percent) => return Ok(()), // IGNORED_SAFE (COVERAGE §4)
        Some(Marker::ConfigLine(_)) => {
            let op = ctx.raw_op(
                b,
                &b.raw,
                ErrorCode::UnsupportedSyntax,
                "$-config line: machine configuration, not a program (COVERAGE §4; CORPUS n3)",
            );
            ctx.push(op);
            return Ok(());
        }
        Some(Marker::BlockDelete) => {
            // Реестр COVERAGE §6.3 открыт: исполнение зависит от переключателя
            // станка → временная диспозиция Raw{E_RUNTIME_STATE} (отчёт этапа 2).
            let op = ctx.raw_op(
                b,
                &b.raw,
                ErrorCode::RuntimeState,
                "block delete '/': execution depends on machine switch (COVERAGE §6.3)",
            );
            ctx.push(op);
            return Ok(());
        }
        None => {}
    }

    // (2) Нарушение модальной группы — сломанный вход (grbl: group violation).
    if let Some((group, g)) = b.modal_violation {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            format!("modal group violation in block: {:?} after {:?}", group, g),
            &ctx.file, line, blen,
        )]);
    }

    // (3) Слова вне словаря → Raw line-level. Оси A/B/C/U/V/W → SEMANTICS
    // (COVERAGE §4); прочие (M6, G64, G90.1, H, ...) → SYNTAX (вне словаря GRBL).
    if !b.unsupported_words.is_empty() {
        let axes_sem =
            b.unsupported_words.iter().all(|w| matches!(w.letter, 'A' | 'B' | 'C' | 'U' | 'V' | 'W'));
        let (code, reason) = if axes_sem {
            (
                ErrorCode::UnsupportedSemantics,
                "rotary/4th-axis words A/B/C/U/V/W — outside L1 (COVERAGE §4)",
            )
        } else {
            (
                ErrorCode::UnsupportedSyntax,
                "word outside GRBL 1.1 dictionary (COVERAGE §2/§3, N/A)",
            )
        };
        let op = ctx.raw_op(b, &b.raw, code, reason);
        ctx.push(op);
        return Ok(());
    }

    // (4) T без M6: M6 в словаре GRBL нет → любое T одиночное (COVERAGE §3).
    if b.word('T').is_some() {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "bare T word: no tool change semantics in GRBL (COVERAGE §3)",
            &ctx.file, line, blen,
        )]);
    }

    // (5) Единицы: ровно одна система на программу (SPEC §2.5, CORPUS n7).
    for g in &b.g_codes {
        if matches!(g, GCode::UnitsIn | GCode::UnitsMm) {
            let u = if *g == GCode::UnitsIn { Units::In } else { Units::Mm };
            if u != ctx.units {
                return Err(vec![err(
                    ErrorCode::UnsupportedSemantics,
                    "unit switch mid-program: one unit system per program (SPEC §2.5)",
                    &ctx.file, line, blen,
                )]);
            }
        }
    }

    // (6) Метаданные и IGNORED_SAFE-коды (COVERAGE §2.1/§2.2/§2.4).
    if b.g_codes.contains(&GCode::PathExact) {
        ctx.path_exact = true; // META(exact_path); Q3 — поведение сверяет этап 3
    }
    if b.g_codes.contains(&GCode::CycleCancel) {
        ctx.motion = None; // G80: cancel cycle-моды; голые координаты дадут INVALID
    }
    // Правило семейств §1.3: G49 → Raw, только если семейство G43 в файле;
    // семейство G40 в GRBL пусто → G40 всегда IGNORED_SAFE.
    if b.g_codes.contains(&GCode::ToolLenDynamic) {
        let op = ctx.raw_op(
            b,
            &b.raw,
            ErrorCode::UnsupportedSemantics,
            "G43.1 dynamic TLO — value not representable as motion state (COVERAGE §2.4)",
        );
        ctx.push(op);
        return Ok(());
    }
    if b.g_codes.contains(&GCode::ToolLenOff) && ctx.g43_family_seen {
        let op = ctx.raw_op(
            b,
            &b.raw,
            ErrorCode::UnsupportedSemantics,
            "G49 cancels present G43-family compensation — family rule (COVERAGE §1.3)",
        );
        ctx.push(op);
        return Ok(());
    }

    // (7) RAW line-level по матрице COVERAGE §2.1/§2.3/§2.6.
    for g in &b.g_codes {
        let (code, reason): (ErrorCode, &str) = match g {
            GCode::Home | GCode::SetHome | GCode::Home2 | GCode::SetHome2 => (
                ErrorCode::RequiresMachineTable,
                "G28/G30 home position depends on machine table (COVERAGE §2.3; Q1)",
            ),
            GCode::Probe(_) => (
                ErrorCode::UnsupportedSemantics,
                "probing G38.x — not representable as L1 moves (COVERAGE §2.1)",
            ),
            GCode::ResetFramePreset => (
                ErrorCode::UnsupportedSemantics,
                "G92.1 resets offset origins — semantics outside L1 (COVERAGE §2.3; Q4)",
            ),
            _ => continue,
        };
        let op = ctx.raw_op(b, &b.raw, code, reason);
        ctx.push(op);
        return Ok(());
    }

    let _ = (is_last, line, blen);
    process_tail(ctx, b, is_last)
}

fn motion_block(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    if let Some(m) = b.g_codes.iter().copied().find(|g| {
        matches!(g, GCode::Rapid | GCode::Linear | GCode::ArcCw | GCode::ArcCcw)
    }) {
        ctx.motion = Some(m);
    }
    let g53 = b.g_codes.contains(&GCode::MachineFrame);
    let has_axis = b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some();
    let has_arc_words = b.word('I').is_some() || b.word('J').is_some() || b.word('K').is_some() || b.word('R').is_some();

    if g53 && !matches!(ctx.motion, Some(GCode::Rapid) | Some(GCode::Linear)) {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "G53 requires G0/G1 in the same block (grbl gcode.c)",
            &ctx.file, line, blen,
        )]);
    }
    let mode = match ctx.motion {
        Some(m) => m,
        None if has_axis || has_arc_words => {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                "naked coordinate words: no active motion mode after G80 (COVERAGE §2.1; CORPUS n6)",
                &ctx.file, line, blen,
            )]);
        }
        None => return Ok(()),
    };
    if !has_axis && !has_arc_words {
        return Ok(()); // кадр без целевых слов — движения нет
    }

    // Чтение текущей позиции в целевом фрейме (точки разрыва (a)–(d), SPEC §2).
    let reading = reading_now(ctx, g53)?;
    if g53 && reading.is_none() {
        return Err(vec![err(
            ErrorCode::RuntimeState,
            "machine-frame move: start not statically known (SPEC §5.3; CORPUS n1/n16)",
            &ctx.file, line, blen,
        )]);
    }

    if matches!(mode, GCode::ArcCw | GCode::ArcCcw) {
        return arc_block(ctx, b, mode, reading);
    }

    // Цель: слова G90/G91 + наследование осей (SPEC §2: частичный end при
    // неизвестном reading — E_RUNTIME_STATE).
    let end = resolve_end(ctx, b, &reading)?
        .expect("end known: reading known or full G90 triple");

    let frame = if g53 { Frame::Machine } else { Frame::Wcs };
    let start = reading.clone().map(StartPoint::Known).unwrap_or(StartPoint::Unknown);

    // zero-length (start == end) → drop + notice (SPEC §3).
    if let StartPoint::Known(s) = &start {
        if len3(&sub3(&end, s)) < 1e-9 {
            let op = if mode == GCode::Rapid {
                Op::Rapid { start: start.clone(), end: end.clone(), state: ctx.state(frame), src: span(&ctx.file, line, blen), sub: None }
            } else {
                Op::Linear { start: start.clone(), end: end.clone(), state: ctx.state(frame), src: span(&ctx.file, line, blen), sub: None }
            };
            ctx.push_drop(op, NoticeKind::ZeroLengthMotion, "zero-length motion dropped (start == end, SPEC §3)".into());
            update_pos(ctx, g53, end);
            return Ok(());
        }
    }

    // feed: G0 не требует; linear без F → INVALID (SPEC §4). G93 → len × F.
    let op = if mode == GCode::Rapid {
        // G93 не применяется к rapid (rapid не ходит по feed) — F как есть.
        Op::Rapid { start, end: end.clone(), state: ctx.state(frame), src: span(&ctx.file, line, blen), sub: None }
    } else {
        let f = ctx.feed.clone().ok_or_else(|| {
            vec![err(ErrorCode::InvalidInput, "linear move before any F word (SPEC §4)", &ctx.file, line, blen)]
        })?;
        let feed = resolved_feed(ctx, &start, &end, f, line, blen)?;
        let mut st = ctx.state(frame);
        st.feed = Some(feed);
        Op::Linear { start, end: end.clone(), state: st, src: span(&ctx.file, line, blen), sub: None }
    };
    ctx.push(op);
    update_pos(ctx, g53, end);
    Ok(())
}

/// Текущее чтение позиции в целевом фрейме хода (None = точка разрыва, SPEC §2).
fn reading_now(ctx: &Ctx, g53: bool) -> Result<Option<Point3>, Vec<Diagnostic>> {
    if g53 {
        // (c) §5.3: (a) нет позиционно-относительных фреймовых операций;
        // (b) каждая WCS, выбранная до хода, определена in-text до хода.
        if ctx.frame_def_count > 0 {
            return Err(vec![err(
                ErrorCode::RuntimeState,
                "machine-frame move after frame_preset/wcs_set_here (SPEC §5.3a; CORPUS n16)",
                &ctx.file, 0, 0,
            )]);
        }
        for p in &ctx.selected_wcs {
            if ctx.origin_of(*p).is_none() {
                return Err(vec![err(
                    ErrorCode::RuntimeState,
                    format!("machine-frame move: WCS P{} selected but not wcs_define'd in text (SPEC §5.3b; CORPUS n1)", p),
                    &ctx.file, 0, 0,
                )]);
            }
        }
        return Ok(machine_start(ctx));
    }
    if ctx.prev_end.is_none() {
        // (a) первый motion-оп: якорь = frame_preset / wcs_set_here (той же WCS).
        return Ok(ctx.anchor.as_ref().filter(|(_, w)| *w == ctx.wcs).map(|(p, _)| p.clone()));
    }
    if ctx.prev_is_machine {
        // (d) первый WCS-ход после machine-ходов: reading = M − O_wcs − Δ.
        let o = ctx.origin_of(wcs_to_p(&ctx.wcs));
        return Ok(match (ctx.pos_mach.clone(), o) {
            (Some(m), Some(o)) => Some(sub3(&m, o)),
            _ => None,
        });
    }
    if ctx.prev_wcs == ctx.wcs {
        return Ok(ctx.prev_end.clone());
    }
    // (b) смена wcs: обе системы in-text → reading_new = prev_end + O_prev − O_new.
    let o_prev = ctx.origin_of(wcs_to_p(&ctx.prev_wcs));
    let o_cur = ctx.origin_of(wcs_to_p(&ctx.wcs));
    Ok(match (ctx.prev_end.clone(), o_prev, o_cur) {
        (Some(e), Some(a), Some(c)) => Some(sub3(&add3(&e, a), c)),
        _ => None,
    })
}

/// Старт machine-хода: цепочка с предыдущего machine-хода либо prev_end + O_wcs.
fn machine_start(ctx: &Ctx) -> Option<Point3> {
    if ctx.prev_is_machine {
        return ctx.prev_end.clone();
    }
    let e = ctx.prev_end.as_ref()?;
    let o = ctx.origin_of(wcs_to_p(&ctx.prev_wcs))?;
    Some(add3(e, o))
}

/// Целевая точка хода: слова (G90 abs / G91 inc) + наследование reading.
/// SPEC §2 v0.11: в точке разрыва (a) — старт программы — отсутствующие оси
/// end наследуют дефолт контроллера (0) в активном фрейме; после
/// genuinely-неизвестных позиций ((b)–(d)) частичный end → E_RUNTIME_STATE.
fn resolve_end(ctx: &Ctx, b: &Block, reading: &Option<Point3>) -> Result<Option<Point3>, Vec<Diagnostic>> {
    let rv = reading.as_ref().map(|p| e3(p));
    let at_start = reading.is_none() && ctx.prev_end.is_none();
    let mut v: [Option<Num>; 3] = std::array::from_fn(|_| None);
    for (i, w) in [(0usize, 'X'), (1, 'Y'), (2, 'Z')] {
        v[i] = match b.word(w) {
            Some(n) => {
                if ctx.dist_abs {
                    Some(verbatim(n))
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
                    Some(num_f64(base[i] + n.value()))
                }
            }
            None => match rv {
                Some(r) => Some(num_f64(r[i])),
                None if at_start => Some(num_f64(0.0)), // дефолт контроллера
                None => None,
            },
        };
    }
    match (v[0].take(), v[1].take(), v[2].take()) {
        (Some(x), Some(y), Some(z)) => Ok(Some(Point3 { x, y, z })),
        _ => {
            if reading.is_none() {
                Err(vec![err(
                    ErrorCode::RuntimeState,
                    "partial end with unknown reading inherits unknown (SPEC §2)",
                    &ctx.file, b.line_no, b.raw.len(),
                )])
            } else {
                unreachable!("reading known ⇒ все оси разрешены")
            }
        }
    }
}

/// G94: F как есть; G93: feed = len × F (только для linear/arc, SPEC §4).
fn resolved_feed(ctx: &Ctx, start: &StartPoint, end: &Point3, f: Num, line: u32, blen: usize) -> Result<Num, Vec<Diagnostic>> {
    if !ctx.feed_inverse {
        return Ok(f);
    }
    match start {
        StartPoint::Known(s) => Ok(num_f64(len3(&sub3(end, s)) * f.value())),
        StartPoint::Unknown => Err(vec![err(
            ErrorCode::RuntimeState,
            "G93 requires known trajectory length (start unknown)",
            &ctx.file, line, blen,
        )]),
    }
}

/// Дуга G2/G3: IJK (инкрементальные offsets от старта, in-plane пара по
/// плоскости; COVERAGE §6.11) или R-форма (SPEC §6: знак + ≤180°, − >180°).
/// Вне-плоскостное слово → pitch. Валидация §6 → E_INVALID_INPUT (N5a/N5b/N19).
fn arc_block(ctx: &mut Ctx, b: &Block, mode: GCode, reading: Option<Point3>) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let inval = |msg: String| err(ErrorCode::InvalidInput, msg, &ctx.file, line, blen);
    // ARC с start=unknown — E_RUNTIME_STATE (геометрия дуги требует числового
    // старта, SPEC §2).
    let start = reading.ok_or_else(|| {
        vec![err(ErrorCode::RuntimeState, "arc with unknown start (SPEC §2)", &ctx.file, line, blen)]
    })?;
    let end = resolve_end(ctx, b, &Some(start.clone()))?
        .expect("end known: reading known or full G90 triple");

    let (a0, a1, al) = plane_axes(&ctx.plane);
    let s = e3(&start);
    let e = e3(&end);
    let pitch = e[al] - s[al]; // вне-плоскостная компонента хода (винт)
    let dir = if mode == GCode::ArcCw { Dir::Cw } else { Dir::Ccw };
    let normal = match ctx.plane {
        GCode::PlaneXY => [0.0, 0.0, 1.0],
        GCode::PlaneZX => [0.0, 1.0, 0.0],
        _ => [1.0, 0.0, 0.0],
    };

    // Центр: in-plane пара; linear-компонента = start (круг в плоскости старта).
    let mut c = s;
    let r_val = b.word('R').map(|n| n.value());
    let d0 = e[a0] - s[a0];
    let d1 = e[a1] - s[a1];
    let chord_plane = (d0 * d0 + d1 * d1).sqrt();
    match r_val {
        Some(r) => {
            // R при start == end в плоскости — центр неоднозначен (SPEC §6.2; N19).
            if chord_plane < 1e-9 {
                return Err(vec![inval(
                    "R-arc with coincident in-plane start/end: ambiguous center (SPEC §6; CORPUS n19)".into(),
                )]);
            }
            // Хорда > 2R — невозможная дуга (допуск радиуса EQUIVALENCE §4; N5b).
            let tol_r = 0.001f64.max(1e-6 * r.abs());
            let h2 = 4.0 * r * r - d0 * d0 - d1 * d1; // = (2R)² − c²
            if h2 < -tol_r * 4.0 * r.abs().max(1.0) {
                return Err(vec![inval(
                    format!("arc chord {chord_plane:.4} > 2R {}: impossible arc (SPEC §6; CORPUS n5b)", r.abs()),
                )]);
            }
            // Формула LinuxCNC/grbl: центр = середина хорды ± h·p̂; знак из
            // направления и знака R (проверено на пилоте 46: центр (0,5,0)).
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
            // IJK — literal X/Y/Z offsets (COVERAGE §6.11); отсутствующие = 0.
            let offs = [
                b.word('I').map(|n| n.value()).unwrap_or(0.0),
                b.word('J').map(|n| n.value()).unwrap_or(0.0),
                b.word('K').map(|n| n.value()).unwrap_or(0.0),
            ];
            c[a0] = s[a0] + offs[a0];
            c[a1] = s[a1] + offs[a1];
        }
    }

    // Sweep: угол от старта к цели в направлении дуги (SPEC §6, (0°, 360°]).
    let two_pi = 2.0 * std::f64::consts::PI;
    // SPEC §10.5 v0.11 — single-final-rounding: производные считаются в f64
    // от сырых входов; округление к 4dp — однократно (num_f64 в Num).
    let sweep = match r_val {
        // R-форма: из хорды и |R| (закон хорд): 2·asin(c/2R) — ≤180° при R>0,
        // дополнение до 360° при R<0; cw/ccw выбирает сторону центра, не величину.
        Some(r) => {
            let sw = 2.0 * (chord_plane / (2.0 * r.abs())).clamp(0.0, 1.0).asin();
            if r < 0.0 { two_pi - sw } else { sw }
        }
        // IJK: полярные углы от точного центра; совпадение в плоскости = 360°.
        None => {
            let a_s = (s[a1] - c[a1]).atan2(s[a0] - c[a0]);
            let a_e = (e[a1] - c[a1]).atan2(e[a0] - c[a0]);
            let mut sweep_ccw = (a_e - a_s).rem_euclid(two_pi);
            if sweep_ccw < 1e-9 {
                sweep_ccw = two_pi; // полная окружность (SPEC §6, ловушка (а))
            }
            let sw = if mode == GCode::ArcCw { two_pi - sweep_ccw } else { sweep_ccw };
            if sw < 1e-9 { two_pi } else { sw }
        }
    };

    // zero-length arc: радиус 0, хорда 0, pitch 0 → drop + notice (SPEC §3/§6).
    let rho_s = ((s[a0] - c[a0]).powi(2) + (s[a1] - c[a1]).powi(2)).sqrt();
    let rho_e = ((e[a0] - c[a0]).powi(2) + (e[a1] - c[a1]).powi(2)).sqrt();
    let mut near_degenerate = false;
    if b.word('R').is_some() && (180.0 - sweep.to_degrees()).abs() < 0.114 {
        near_degenerate = true; // |cos(sweep/2)| < 1e-3 (EQUIVALENCE §4)
    }

    // Валидация §6: планиметрический радиус ρ_start ≈ ρ_end (N5a).
    let tol_r = 0.001f64.max(1e-6 * rho_s.max(rho_e));
    if (rho_s - rho_e).abs() > tol_r {
        return Err(vec![inval(format!(
            "arc radius mismatch: ρ_start {rho_s:.4} vs ρ_end {rho_e:.4} (SPEC §6; CORPUS n5a)"
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

    let arc_len = ((rho_s * sweep).powi(2) + pitch * pitch).sqrt();
    let f = ctx.feed.clone().ok_or_else(|| {
        vec![err(ErrorCode::InvalidInput, "arc move before any F word (SPEC §4)", &ctx.file, line, blen)]
    })?;
    let feed = if ctx.feed_inverse { num_f64(arc_len * f.value()) } else { f };
    let mut st = ctx.state(Frame::Wcs);
    st.feed = Some(feed);
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
    update_pos(ctx, false, end);
    Ok(())
}

/// После motion-опа: prev_end/reading/машинная позиция.
fn update_pos(ctx: &mut Ctx, g53: bool, end: Point3) {
    if g53 {
        ctx.pos_mach = Some(end.clone());
    } else if let Some(o) = ctx.origin_of(wcs_to_p(&ctx.wcs)) {
        ctx.pos_mach = Some(add3(&end, o));
    } else {
        ctx.pos_mach = None;
    }
    ctx.prev_end = Some(end);
    ctx.prev_is_machine = g53;
    ctx.prev_wcs = ctx.wcs;
}

fn add3(a: &Point3, b: &Point3) -> Point3 {
    let [x, y, z] = e3(a);
    let [u, v, w] = e3(b);
    pt(x + u, y + v, z + w)
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

fn process_tail(ctx: &mut Ctx, b: &Block, is_last: bool) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    // (8) Выбор WCS/плоскости/дистанции/feeder-режима — ABSORB (COVERAGE §2.2/§2.3).
    for g in &b.g_codes {
        match g {
            GCode::Wcs(p) => {
                ctx.wcs = wcs_from_p(*p);
                ctx.selected_wcs.insert(*p);
            }
            GCode::PlaneXY | GCode::PlaneZX | GCode::PlaneYZ => ctx.plane = *g,
            GCode::DistAbs => ctx.dist_abs = true,
            GCode::DistInc => ctx.dist_abs = false,
            GCode::FeedInverse => ctx.feed_inverse = true,
            GCode::FeedPerMin => ctx.feed_inverse = false,
            _ => {}
        }
    }

    // (9) Порядок grbl gcode.c: feed → spindle → coolant.
    if let Some(f) = b.word('F') {
        ctx.feed = Some(verbatim(f));
    }
    let s_word = b.word('S').map(verbatim);
    if let Some(s) = &s_word {
        ctx.last_s = Some(s.clone());
        if let Spindle::Cw(_) = ctx.spindle {
            ctx.spindle = Spindle::Cw(s.clone());
        }
        if let Spindle::Ccw(_) = ctx.spindle {
            ctx.spindle = Spindle::Ccw(s.clone());
        }
    }
    for m in &b.m_codes {
        match m {
            MCode::SpindleCw => {
                let s = s_word.clone().or_else(|| ctx.last_s.clone()).unwrap_or_else(|| num_f64(0.0));
                ctx.spindle = Spindle::Cw(s);
            }
            MCode::SpindleCcw => {
                let s = s_word.clone().or_else(|| ctx.last_s.clone()).unwrap_or_else(|| num_f64(0.0));
                ctx.spindle = Spindle::Ccw(s);
            }
            MCode::SpindleOff => ctx.spindle = Spindle::Off,
            MCode::MistOn => ctx.coolant.mist = true,
            MCode::FloodOn => ctx.coolant.flood = true,
            MCode::CoolantOff => ctx.coolant = Coolant::default(),
            _ => {}
        }
    }

    // (10) Фреймовые записи (SPEC §5.3) — до motion (порядок grbl #5).
    if b.g_codes.contains(&GCode::SetCoordSystem) {
        return process_g10(ctx, b);
    }
    if b.g_codes.contains(&GCode::FramePreset) {
        return process_g92(ctx, b);
    }

    // (11) Dwell G4 P (сек, COVERAGE §7.3): ==0 → drop+notice; <0 → INVALID.
    if b.g_codes.contains(&GCode::Dwell) {
        let p = b.word('P').ok_or_else(|| {
            vec![err(ErrorCode::InvalidInput, "G4 requires P word (seconds)", &ctx.file, line, blen)]
        })?;
        let sec = p.value();
        let op = Op::Dwell { seconds: verbatim(p), state: ctx.state(Frame::Wcs), src: span(&ctx.file, line, blen), sub: None };
        if sec < 0.0 {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                "G4 P < 0: negative dwell (SPEC §5.2)",
                &ctx.file, line, blen,
            )]);
        }
        if sec == 0.0 {
            ctx.push_drop(op, NoticeKind::ZeroDwell, "zero dwell dropped (seconds == 0, SPEC §5.2)".into());
        } else {
            ctx.push(op);
        }
        return Ok(());
    }

    // (12) Motion (порции 5–6) — G53/G0/G1/G2/G3, модальное продолжение.
    motion_block(ctx, b)?;

    // (13) M0/M1/M2/M30 — после motion (порядок grbl #8/#9).
    let mut saw_end = false;
    for m in &b.m_codes {
        match m {
            MCode::ProgramStop | MCode::OptionalStop => {
                let op = Op::ProgramStop {
                    optional: *m == MCode::OptionalStop,
                    state: ctx.state(Frame::Wcs),
                    src: span(&ctx.file, line, blen),
                };
                ctx.push(op);
            }
            MCode::ProgramEnd => saw_end = true,
            _ => {}
        }
    }
    if saw_end {
        ctx.end_count += 1;
        ctx.end_line = line;
        ctx.end_is_last = is_last;
        ctx.ops.push(Op::ProgramEnd { src: span(&ctx.file, line, blen) });
    }
    Ok(())
}

/// G10 L2 → WcsDefine (любая позиция, SPEC §5.3); L20 → WcsSetHere (пресетка).
fn process_g10(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let bad_p = || {
        err(ErrorCode::InvalidInput, "G10 requires P word, 1..=6 (GRBL WCS count)", &ctx.file, line, blen)
    };
    let p: u8 = match b.word('P') {
        Some(n) if n.value() >= 1.0 && n.value() <= 6.0 && n.value().fract() == 0.0 => n.value() as u8,
        _ => return Err(vec![bad_p()]),
    };
    match b.g10_kind() {
        Some(G10Kind::Define) => {
            // Частичная запись origin сохраняет старые значения таблицы →
            // значение зависит от таблицы станка → Raw (COVERAGE §1.1).
            let (x, y, z) = match (b.word('X'), b.word('Y'), b.word('Z')) {
                (Some(x), Some(y), Some(z)) => (verbatim(x), verbatim(y), verbatim(z)),
                _ => {
                    let op = ctx.raw_op(
                        b,
                        &b.raw,
                        ErrorCode::RequiresMachineTable,
                        "G10 L2 with partial origin keeps old table values (COVERAGE §1.1)",
                    );
                    ctx.push(op);
                    return Ok(());
                }
            };
            let origin = Point3 { x, y, z };
            let op = Op::WcsDefine {
                p,
                origin: origin.clone(),
                state: ctx.state(Frame::Wcs),
                src: span(&ctx.file, line, blen),
            };
            ctx.push(op);
            ctx.wcs_origins.push((p, origin));
            Ok(())
        }
        Some(G10Kind::SetHere) => {
            // Позиционно-зависим: только пресетка до первого motion-опа (SPEC §5.3).
            if ctx.prev_end.is_some() {
                return Err(vec![err(
                    ErrorCode::UnsupportedSemantics,
                    "G10 L20 after motion: position-dependent op outside the preset zone (SPEC §5.3; CORPUS n15)",
                    &ctx.file, line, blen,
                )]);
            }
            if ctx.frame_def_count >= 1 {
                return Err(vec![err(
                    ErrorCode::UnsupportedSemantics,
                    "second frame preset (G92/G10 L20) in one program (CORPUS n17)",
                    &ctx.file, line, blen,
                )]);
            }
            // Частичный delta оставляет ось в неизвестном reading → RUNTIME.
            let (x, y, z) = match (b.word('X'), b.word('Y'), b.word('Z')) {
                (Some(x), Some(y), Some(z)) => (verbatim(x), verbatim(y), verbatim(z)),
                _ => {
                    return Err(vec![err(
                        ErrorCode::RuntimeState,
                        "G10 L20 partial delta: origin depends on unknown current position (SPEC §5.3)",
                        &ctx.file, line, blen,
                    )]);
                }
            };
            let delta = Point3 { x, y, z };
            let op = Op::WcsSetHere {
                p,
                delta: delta.clone(),
                state: ctx.state(Frame::Wcs),
                src: span(&ctx.file, line, blen),
            };
            ctx.push(op);
            ctx.frame_def_count += 1;
            ctx.anchor = Some((delta, wcs_from_p(p)));
            Ok(())
        }
        None => {
            // L отсутствует или не 2/20 — вне словаря GRBL (Raw line-level).
            let op = ctx.raw_op(
                b,
                &b.raw,
                ErrorCode::UnsupportedSyntax,
                "G10 requires L2/L20 in GRBL (COVERAGE §2.3)",
            );
            ctx.push(op);
            Ok(())
        }
    }
}

/// G92 → frame_preset: только пресетка до первого motion-опа; full-triple.
fn process_g92(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    if ctx.prev_end.is_some() {
        return Err(vec![err(
            ErrorCode::UnsupportedSemantics,
            "G92 after motion: frame_preset only as startup preset (SPEC §5.3; CORPUS n2)",
            &ctx.file, line, blen,
        )]);
    }
    if ctx.frame_def_count >= 1 {
        return Err(vec![err(
            ErrorCode::UnsupportedSemantics,
            "second frame preset (G92/G10 L20) in one program (CORPUS n17)",
            &ctx.file, line, blen,
        )]);
    }
    let (x, y, z) = match (b.word('X'), b.word('Y'), b.word('Z')) {
        (Some(x), Some(y), Some(z)) => (verbatim(x), verbatim(y), verbatim(z)),
        _ => {
            return Err(vec![err(
                ErrorCode::RuntimeState,
                "partial G92: unspecified axes keep unknown startup position reading (SPEC §5.3)",
                &ctx.file, line, blen,
            )]);
        }
    };
    let values = Point3 { x, y, z };
    ctx.g92_values = Some((values.clone(), ctx.wcs));
    ctx.frame_def_count += 1;
    ctx.anchor = Some((values, ctx.wcs));
    Ok(())
}
