//! Канонизатор Haas mill NGC → EID/L1 AST (этап 1).
//! Источники: haas_dialect_analysis.md v0.0 (контракт; §1.1 группы, §1.2 DPI
//! machinists notation + Setting 77, §2.1 движение + G01 C/R, §2.2 модальные,
//! §9.1 v0.1-скоуп, §10 реестр 34 `?`), SPEC-EID-L1 v0.18 (§2 точки разрыва,
//! §3 модель + полные круги v0.18, §4 state, §6 дуги, §8 RAW + §8.6
//! абсорбция v0.17), COVERAGE v0.9.2 Haas-колонка, паттерн — зеркала
//! eid-grbl/eid-linuxcnc/eid-fanuc canonicalize.
//!
//! Этап 1 покрывает (§9.1(1)–(3) контракта):
//! * лексика: EOB `;` — разделитель (ловушка №2), `( )`-комментарии,
//!   `%`, N-слова, `/` block delete → Raw{RUNTIME_STATE}, трейлинг-точка,
//!   целая часть >9 цифр → E_LIMIT_EXCEEDED, пробел-в-слове → строгий
//!   отказ `?` (контракт не специфицирует; реальный Haas-лексер не
//!   верифицирован);
//! * модальности: 01 (G0–G3), 02 (плоскости + валидация нормали), 03
//!   (G90/G91), 04 — В СЛОВАРЕ НЕТ, G90.1/G91.1 ABSORB-как-LinuxCNC `?`
//!   (дефолт — инкрементальный центр), 05 (G93/G94 ABSORB; G95 → Raw
//!   SEMANTICS), 06 (G20/G21; G70/G71/G72 → Raw-болтовые, НЕ units!),
//!   15 (G61 = exact_stop, G64 = bare blend — сдвиг значения vs LinuxCNC);
//! * движение: G0/G1/G2/G3 + модальное продолжение (голые координаты —
//!   `?` §1.3, продолжение группы 01 при активном режиме, отказ без него);
//!   IJK — инкрементальные от start ВСЕГДА (группы 04 нет); R-форма
//!   (знак ≤180°/>180°; R при полном круге → E_INVALID_INPUT); helical;
//!   **полный круг 360° по геометрии** — center-форма end==start при
//!   center≠start НЕЗАВИСИМО от end-слов (SPEC §3 v0.18); degenerate drop;
//!   валидация §6 (нормаль, радиусы, уравнение позиции); zero-length drop;
//!   G01 с C/R-словами (фаска/скругление) → Raw{SEMANTICS} (§2.1);
//! * DPI (§1.2 ловушка №1 — machinists notation, Setting 162 OFF):
//!   X/Y/Z/A/B/C/I/J/K/R/Q bare → × least increment (0.001 mm / 0.0001")
//!   с W_DPI_ASSUMED; F bare → × 0.0001 + notice с текстом Setting 77
//!   «Scale Integer F» (дефолт OFF: F12 = 0.0012 units/min — F-домен
//!   Haas ≠ Fanuc-ruling «целое mm/min verbatim»!); dotted → verbatim;
//!   P-dwell dot-rule (точка = сек, без = мс) — этап 2;
//! * §8.6 модальная абсорбция на RAW-строках (v0.17 — ВКЛЮЧЕНА СРАЗУ):
//!   units/path/F/S/M3-M5/M8-M9 + ABSORB-группы до push_raw;
//!   raw-motion-G (G28/G31/G12/G70/…) с осями — motion НЕ продолжается +
//!   notice W_MODAL_ON_RAW;
//! * структура: auto-PEND (W_MISSING_PROGRAM_END), хвост после PEND →
//!   Raw-хвост (subs-тела §4.4 — Haas-норма, как Fanuc), Macro B →
//!   line-level Raw{SYNTAX} (Fanuc-совместимый, §4.3).
//!
//! Известные ограничения этапа 1 (честно):
//! * Фреймы (WCS-слоты p=1..6 + G110–G112 → p=7..9, G52-ERR, G10 L2/L20/P0,
//!   G92/G53) — этап 2: G54–G59/G10/G92 встреченные на этапе 1 уходят в
//!   generic-ведро Raw{SYNTAX} provisional (не молча).
//! * Циклы ×Haas-семантика (L-повторы, K=min-пек G83, G73 Setting 22,
//!   P-dot-rule) — этап 2; G81–G89/G73 → generic provisional.
//! * M-словарь этапа 1 минимален (M2/M30, M3/M4/M5, M8/M9 для §8.6);
//!   вендорское ведро (M07-Shower, M13-тормоз, M29-relay, M97/M98/M99,
//!   M19) — этап 2.

use crate::block::{parse_file, Block, Marker, ParsedFile};
use crate::lexer::LexErrorKind;
use crate::state::{
    g_raw_disposition, plane_axes, plane_normal, GCode, MCode, ModalState, RAW_MOTION_G,
};
use eid_core::ast::{
    Coolant, Dir, Frame, Num, Op, PathMode, Point3, Program, Spindle, SrcSpan, StartPoint, State,
    Units, Wcs,
};
use eid_core::diag::{Diagnostic, ErrorCode, LimitInfo};
use std::cell::RefCell;

// ---------------------------------------------------------------------------
// Notices (реплика паттерна eid-grbl/eid-linuxcnc/eid-fanuc; SPEC §3/§6)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// rapid/linear: start == end (SPEC §3).
    ZeroLengthMotion,
    /// arc: start == end при sweep != 360° и pitch == 0 (SPEC §3/§6).
    ZeroLengthArc,
    /// near-degenerate R-дуга (±0.12° вокруг 180°): амплитификация центра
    /// (EQUIVALENCE §4) — notice при живом опе.
    NearDegenerateRArc,
    /// Программа без M2/M30: неявный конец — ProgramEnd добавлен последним
    /// опом (SPEC §3 v0.16; контракт §4.4 — DNC-файлы Haas без M30).
    MissingProgramEnd,
    /// W_DPI_ASSUMED (анализ §1.2 ловушка №1): bare-integer в слове дробного
    /// домена прочитан по machinists notation (× least increment, Setting 162
    /// OFF); F — × least increment (Setting 77 «Scale Integer F» OFF).
    DpiAssumed,
    /// W_MODAL_ON_RAW (SPEC §8.6 правило 6, v0.17 — критический аудит #33):
    /// raw-motion-G (G28/G31/G12/G70…) с осями — модальное состояние
    /// абсорбировано, но motion-режим НЕ продолжается (ход не представим).
    ModalOnRaw,
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

/// DPI least increment по активным единицам (§1.2: least input increment
/// 0.001 mm / 0.0001" — совпадает с Fanuc IS-B; таблицы Setting 77).
fn least_increment(units: Units) -> f64 {
    match units {
        Units::Mm => 0.001,
        Units::In => 0.0001,
    }
}

fn span(file: &str, line: u32, len: usize) -> SrcSpan {
    SrcSpan {
        file: file.to_string(),
        line_start: line,
        col_start: 1,
        line_end: line,
        col_end: len as u32 + 1,
    }
}

/// Диагностика с явным этапом: Parse — лексические пути, Verify —
/// канонизационные инварианты (interface v0.1.2, паттерн eid-grbl/fanuc).
fn err_staged(
    stage: eid_core::diag::Stage,
    code: ErrorCode,
    msg: impl Into<String>,
    file: &str,
    line: u32,
    len: usize,
) -> Diagnostic {
    Diagnostic {
        code,
        message: msg.into(),
        src: Some(span(file, line, len)),
        options: Vec::new(),
        limit: None,
        stage,
    }
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
        limit: Some(LimitInfo {
            threshold,
            threshold_value: value,
            actual,
        }),
        stage: eid_core::diag::Stage::Verify,
    }
}

fn e3(p: &Point3) -> [f64; 3] {
    [p.x.value(), p.y.value(), p.z.value()]
}

fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3 {
        x: num_f64(x),
        y: num_f64(y),
        z: num_f64(z),
    }
}

fn p3(v: [f64; 3]) -> Point3 {
    pt(v[0], v[1], v[2])
}

/// hex-кодировка байтов строки (grammar D9; payload — точная вырезка +
/// терминатор строки, ruling 5).
fn payload_hex(line: &str) -> String {
    let mut s = String::new();
    for b in line.as_bytes() {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

// ---------------------------------------------------------------------------
// Ctx — модальное состояние (state.rs ModalState) + resolved-state
// ---------------------------------------------------------------------------

/// Чтение позиции последнего motion-опа (точки разрыва (a)–(d), SPEC §2).
/// Этап 1: только WCS-фрейм (G53/G92/G10/переключения WCS — этап 2), поле
/// framewcs сохранено для зеркальности этапа 2.
#[derive(Debug, Clone, Copy)]
struct Reading {
    pos: [f64; 3],
    frame: Frame,
    wcs: Wcs,
}

/// Результат связывания старта (SPEC §2 принцип 2).
#[derive(Debug, Clone, Copy)]
enum Linked {
    /// start вычислим.
    Known([f64; 3]),
    /// Точка разрыва (a): первый motion-оп без якоря — start Unknown, но
    /// отсутствующие оси end наследуют дефолт контроллера 0 (§2 v0.11).
    UnknownAtStart,
}

struct Ctx {
    file: String,
    modal: ModalState,
    feed: Option<Num>, // модальный F (переживает G0, SPEC §4)
    spindle: Spindle,
    coolant: Coolant,
    tool: Option<u32>,  // этап 2 (M6+T); всегда None на этапе 1
    last_s: Option<Num>, // голое S — сохранение оборотов (§3 контракта)
    prev: Option<Reading>, // end последнего motion-опа
    saw_motion: bool,
    ops: Vec<Op>,
    notices: Vec<Notice>,
    /// W_DPI_ASSUMED-сообщения слов, прочитанных до построения опа: прицепляются
    /// к следующему опу (в т.ч. к raw-опу — §8.6).
    pending_dpi: RefCell<Vec<String>>,
    end_count: usize,
    end_is_last: bool,
    end_line: u32,
}

impl Ctx {
    fn state(&self) -> State {
        State {
            feed: self.feed.clone(),
            spindle: self.spindle.clone(),
            coolant: self.coolant,
            tool: self.tool,
            frame: Frame::Wcs, // G53 — этап 2
            wcs: self.modal.wcs,
        }
    }

    /// Записать W_DPI_ASSUMED-сообщение (прицепится к следующему опу).
    fn note_dpi(&self, msg: String) {
        self.pending_dpi.borrow_mut().push(msg);
    }

    /// Слово дробного домена (X/Y/Z/A/B/C/I/J/K/R/Q/E) → Num: bare-integer →
    /// × least increment (machinists notation, Setting 162 OFF) + W_DPI_ASSUMED;
    /// с точкой → verbatim (§1.2 ловушка №1 — направление обратное Fanuc-DPI-
    /// гипотезе, но политика Fanuc §9.1(1) переиспользуется).
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
            "W_DPI_ASSUMED: {}{} (bare integer) read as {} ({} × least increment {} = {}; machinists notation default, Setting 162 Default To Float OFF, least input increment {}; analysis §1.2)",
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

    /// F-слово → Num — HAAS-ПРАВИЛО (≠ Fanuc ruling 2!): bare-integer →
    /// × 0.0001 (Setting 77 «Scale Integer F» дефолт OFF: `F12` =
    /// 0.0012 units/min — фиксированный F-домен, НЕ unit-scaled ×least-
    /// increment); dotted-F → verbatim. Fanuc-политика «bare F = целое
    /// mm/min verbatim (параметр 1422)» НЕ переносится: F-домен у Haas
    /// другой (§1.2: «Setting 77 — F12 = 0.0012 units/min; ON: F12 = 12.0»;
    /// бриф этапа 1: «Haas F = ×0.0001 при Setting 77 OFF»).
    fn feed_num(&self, w: &crate::lexer::RawNum) -> Num {
        if w.has_point {
            return verbatim(w);
        }
        const F_SCALE: f64 = 0.0001;
        let v = w.value * F_SCALE;
        self.note_dpi(format!(
            "W_DPI_ASSUMED: F{} (bare integer) read as {} units/min ({} × 0.0001 = {}; Setting 77 Scale Integer F default OFF — F12 = 0.0012 units/min; Haas F domain differs from Fanuc integer-mm/min ruling; analysis §1.2)",
            w.text,
            num_f64(v).text,
            w.text,
            num_f64(v).text,
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
        self.notices.push(Notice {
            kind,
            message,
            op_index: idx,
            op,
        });
    }

    /// notice при ЖИВОМ опе (near-degenerate R-arc, auto-PEND) + flush DPI.
    fn push_keep(&mut self, op: Op, kind: NoticeKind, message: String) {
        let idx = self.ops.len();
        self.ops.push(op.clone());
        self.flush_dpi(idx, &op);
        self.notices.push(Notice {
            kind,
            message,
            op_index: idx,
            op,
        });
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
        let op = Op::ProgramEnd {
            src: span(&self.file, line, blen),
        };
        self.push(op);
    }

    /// Связывание старта (SPEC §2 принцип 2). Этап 1: только WCS-фрейм;
    /// точки разрыва (b)/(d)/machine — этап 2 (фреймы), поэтому отказов
    /// связывания нет — сигнатура без Result.
    fn link_start(&self) -> Linked {
        match self.prev {
            // Точка разрыва (a): якоря нет (frame_preset/G10 — этап 2).
            None => Linked::UnknownAtStart,
            Some(Reading { pos, .. }) => Linked::Known(pos),
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
            "max_source_lines",
            1_000_000.0,
            line_count,
            format!("file has {} lines, limit 1000000", src.lines().count()),
            file,
            1,
        )]);
    }

    let parsed: ParsedFile = parse_file(src);
    // Лексические ошибки: мусор/комментарии → INVALID (Parse); переполнения —
    // LIMIT (§1.2: целая часть >9 цифр max_word_chars не ловит); пробел-в-
    // слове — строгий отказ `?` (контракт не специфицирует).
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
                    format!("integer part of {digits} digits exceeds max_integer_digits (9) — Haas range ±99999.9999-class `?` (analysis §1.2; registry §10(31))"),
                    file, e.line,
                ),
                LexErrorKind::SpaceInWord { letter } => err_staged(
                    eid_core::diag::Stage::Parse,
                    ErrorCode::InvalidInput,
                    format!(
                        "space between word letter '{letter}' and value — strict reject: Haas contract does not specify (`?` COVERAGE §6 registry; real Haas lexer unverified; analysis §1.3)"
                    ),
                    file, e.line, e.col as usize,
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
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        tool: None,
        last_s: None,
        prev: None,
        saw_motion: false,
        ops: Vec::new(),
        notices: Vec::new(),
        pending_dpi: RefCell::new(Vec::new()),
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

/// Структурная проверка program_end (SPEC §3 + контракт §4.4):
/// хвост после первого program_end — line-level Raw-хвост
/// {E_UNSUPPORTED_SYNTAX} (тела subs/main+subs в одном файле — норма Haas,
/// §4.4; осознанное расхождение с grbl n12 — зеркало Fanuc ruling 1),
/// НЕ жёсткий Err. Дубль M2/M30 — defensive-guard. Отсутствие M2/M30 →
/// auto-PEND + notice.
fn finalize(ctx: &mut Ctx, line_count: u32) -> Result<ParseOut, Vec<Diagnostic>> {
    if ctx.end_count > 1 {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "duplicate program end: more than one M2/M30 (SPEC §3; analysis §4.4)",
            &ctx.file,
            ctx.end_line,
            0,
        )]);
    }
    let _ = ctx.end_is_last; // Haas: хвост → Raw (subs-тела §4.4), не Err
    if ctx.end_count == 0 {
        // SPEC §3 v0.16 / контракт §4.4: DNC-файлы часто без M30 — авто-
        // вставка ProgramEnd последним опом + notice.
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
            "max_ops_per_program",
            200_000.0,
            ops_n,
            format!("program yields {} ops, limit 200000", ops_n as u64),
            &ctx.file,
            1,
        )]);
    }
    let program = Program {
        // grammar v0.3 — magic-строка (SPEC §10.4).
        eid_version: "EID/L1 v0.3".to_string(),
        units: ctx.modal.units,
        // SPEC §3 v0.12: отсутствие G61/G64 → exact_path (дефолт);
        // G61 = exact_stop (value shift, COVERAGE §2.2).
        path_mode: ctx.modal.path.clone().unwrap_or(PathMode::ExactPath),
        // frame_preset (G92) — этап 2.
        frame_preset: None,
        ops: std::mem::take(&mut ctx.ops),
    };
    let notices = std::mem::take(&mut ctx.notices);
    Ok(ParseOut { program, notices })
}

// ---------------------------------------------------------------------------
// process_block — диспетчер одного кадра
// ---------------------------------------------------------------------------

/// План кадра (этап 1: только движение группы 01; dwell/frame/cycle — этап 2).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Plan {
    Motion,
    Noop,
}

#[allow(clippy::too_many_lines)]
fn process_block(ctx: &mut Ctx, b: &Block, is_last: bool) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    // (0) Хвост после program_end (§4.4 контракта): main + subs в одном
    // файле — норма Haas → line-level Raw{SYNTAX}, НЕ Err (зеркало Fanuc
    // ruling 1; осознанное расхождение с grbl n12); motion-опы после PEND
    // не строятся (SPEC §3). Хвост НЕ абсорбируется (программа структурно
    // закончена; G92 из sub-тела исказил бы frame_preset — этап 2).
    if ctx.end_count > 0 {
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSyntax,
            "block after program end: subprogram tail preserved raw (analysis §4.4; COVERAGE §4 O-words)",
        );
        ctx.push(op);
        return Ok(());
    }

    // (1) Строковые маркеры.
    match &b.marker {
        Some(Marker::Percent) => return Ok(()), // IGNORED_SAFE (§1.3)
        Some(Marker::BlockDelete) => {
            // §1.3: исполнение зависит от переключателя станка —
            // RAW(E_RUNTIME_STATE), как LinuxCNC/Fanuc-строки COVERAGE §4.
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

    // (2) Macro B (§4.3, COVERAGE §4): `#`-слова / `[ ]`-выражения /
    // letter-run statement'ы → весь блок line-level Raw{SYNTAX} (Haas Macro B
    // Fanuc-совместимый; параметрика = L2, этап 12; декомпозиция запрещена).
    if b.macro_b {
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSyntax,
            "Macro B construct (#-variable / []-expression / IF-GOTO-WHILE-DO-END) — parametric, line-level Raw (analysis §4.3; COVERAGE §4)",
        );
        ctx.push(op);
        return Ok(());
    }

    // (3) Нарушение модальной группы (§1.1: один код на группу на блок) —
    // сломанный вход.
    if let Some((group, g)) = b.modal_violation {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            format!("modal group violation in block: {group:?} after {g:?}"),
            &ctx.file,
            line,
            blen,
        )]);
    }

    // (3a) Модальная абсорбция ДО любых raw-возвратов (SPEC §8.6 правило 6,
    // v0.17 — критический аудит #33; ВКЛЮЧЕНА СРАЗУ по брифу этапа 1):
    // строка, уходящая в Raw целиком, обязана абсорбировать frame-level
    // состояние до push_raw — иначе следующий кадр парсится в потерянной
    // модальности (пробой аудита: `G91 G28 Z0` терял G91 → следующий
    // `G0 X10 Y10` молча абсолютный).
    absorb_frame_state(ctx, b)?;

    // (4) G95 — детект для RAW (§2.2: feed/rev требует rpm, зависящего от
    // override; статическая эквивалентность не гарантируется; кандидат
    // ABSORB при статическом S — ревизия по корпусу).
    if b.g_codes.contains(&GCode::FeedPerRev) {
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSemantics,
            "G95 feed-per-rev: resolution needs spindle rpm (override-dependent) — not representable (analysis §2.2; COVERAGE §2.2)",
        );
        ctx.push(op);
        return Ok(());
    }

    // (5) G/M-коды вне enum: raw-таблица (фиксированные COVERAGE-коды §2/§3)
    // первым приоритетом, затем generic-ведро Raw{SYNTAX} + диагностика текста
    // (молчаливый пропуск запрещён, COVERAGE §2.7). SPEC §8.6: raw-motion-G
    // (G28/G31/G12/G70…) с осями — модальное состояние уже абсорбировано в
    // (3a); motion-режим НЕ продолжается + notice W_MODAL_ON_RAW.
    for w in &b.unsupported_words {
        if w.letter == 'G' {
            if let Some((code, reason)) = g_raw_disposition(w.num.value) {
                let t = (w.num.value * 10.0).round() as i64;
                let has_motion_words = ['X', 'Y', 'Z', 'I', 'J', 'K', 'R']
                    .iter()
                    .any(|l| b.word(*l).is_some());
                if has_motion_words && RAW_MOTION_G.contains(&t) {
                    ctx.modal.motion = None;
                    let op = ctx.raw_op(b, code, reason);
                    ctx.push_keep(
                        op,
                        NoticeKind::ModalOnRaw,
                        format!(
                            "W_MODAL_ON_RAW: G{} — raw motion block with axis words; modal motion mode NOT continued (SPEC §8.6 rule 6, audit #33)",
                            w.num.text
                        ),
                    );
                    return Ok(());
                }
                let op = ctx.raw_op(b, code, reason);
                ctx.push(op);
                return Ok(());
            }
        }
    }
    if let Some(w) = b.unsupported_words.first() {
        let val = w.num.text.as_str();
        let reason = if w.letter == 'G' {
            format!(
                "G-code G{val}: outside stage-1 Haas mill dictionary — frames/cycles/vendor buckets land stage 2 (COVERAGE §2.7 generic bucket; not silent)"
            )
        } else {
            format!(
                "M-code M{val}: outside stage-1 M dictionary — vendor M-bucket disposition stage 2 (COVERAGE §3 generic bucket; not silent)"
            )
        };
        let op = ctx.raw_op(b, ErrorCode::UnsupportedSyntax, &reason);
        ctx.push(op);
        return Ok(());
    }

    // (6) План кадра (этап 1: движение группы 01; dwell/frame/cycle — этап 2).
    let has_axis = b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some();
    let has_arc_words = b.word('I').is_some()
        || b.word('J').is_some()
        || b.word('K').is_some()
        || b.word('R').is_some()
        || b.word('C').is_some(); // C в G01 = фаска (Haas-специфика, §2.1)
    let explicit_motion = b.motion();
    let plan = if explicit_motion.is_some()
        || (ctx.modal.motion.is_some() && (has_axis || has_arc_words))
    {
        Plan::Motion
    } else if has_axis || has_arc_words {
        // Голые координаты без активного motion-кода — сломанный вход
        // (§1.3 «легальны — продолжение группы 01 `?`»: продолжение при
        // активном режиме выше; без режима — отказ, паритет Fanuc §2.7).
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "naked coordinate words: no active motion mode (analysis §1.3 `?`)",
            &ctx.file,
            line,
            blen,
        )]);
    } else {
        Plan::Noop
    };
    let motion_mode = explicit_motion.or(ctx.modal.motion);

    // (7) Слова без потребителя — молчаливый пропуск запрещён (COVERAGE §4).
    // G01 C/R (фаска/скругление) → Raw{SEMANTICS} (§2.1); context-слова
    // (P/Q/L/I/J/K/R) без контекста — сломанный вход (Err).
    if let Some((code, reason)) = unused_word_diagnostic(b, plan, motion_mode) {
        if code == ErrorCode::InvalidInput {
            return Err(vec![err(code, reason, &ctx.file, line, blen)]);
        }
        let op = ctx.raw_op(b, code, &reason);
        ctx.push(op);
        return Ok(());
    }

    // (8) Движение.
    if plan == Plan::Motion {
        motion_block(ctx, b)?;
    }

    // (9) M2/M30 → ProgramEnd (после motion — порядок grbl #9; SPEC §5.2).
    if b.m_codes.contains(&MCode::ProgramEnd) {
        ctx.mark_end(line, is_last, blen);
    }
    Ok(())
}

fn modal_group_is_motion(g: GCode) -> bool {
    matches!(
        g,
        GCode::Rapid | GCode::Linear | GCode::ArcCw | GCode::ArcCcw
    )
}

// ---------------------------------------------------------------------------
// Модальная абсорбция на RAW-строках (SPEC §8.6 правило 6, v0.17 — аудит #33)
// ---------------------------------------------------------------------------

/// Абсорбция frame-level состояния кадра (SPEC §8.6 правило 6). Вызывается ДО
/// любых raw-возвратов: строка, уходящая в Raw целиком, обязана абсорбировать
/// модальные группы (plane/distance/arcdist/feedmode/units/path), F/S
/// (spindle, last_s), M3/M4/M5, M8/M9 — иначе следующий кадр парсится в
/// потерянной модальности (пробой аудита: `G91 G28 Z0` терял G91 → следующий
/// `G0 X10 Y10` молча абсолютный — точное нарушение правила).
/// Для легальных кадров функция выполняет units-проверку, ABSORB, path,
/// F/S/M-состояние (этап 2 добавит G92/G10-структуры — absorb_frame_structs).
fn absorb_frame_state(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    // (a) Единицы: ровно одна система на программу (SPEC §2.5) — и на
    // RAW-кадре (§8.5: смена units — структурный класс, жёсткий отказ).
    for g in &b.g_codes {
        if let Some(u) = g.units() {
            if u != ctx.modal.units {
                if ctx.modal.units_established {
                    return Err(vec![err(
                        ErrorCode::UnsupportedSemantics,
                        "unit switch mid-program: one unit system per program (SPEC §2.5)",
                        &ctx.file,
                        line,
                        blen,
                    )]);
                }
                ctx.modal.units = u;
            }
            ctx.modal.units_established = true;
        }
    }

    // (b) ABSORB модальных групп (01–06; units выше). Явный код группы 01
    // отменяет активный цикл (группа 09 — этап 2).
    for g in &b.g_codes {
        ctx.modal.apply(*g);
    }

    // (c) Path-режим (группа 15): глобален (§3); переключение — отказ (и на
    // RAW-кадре — метаданные программы).
    for g in &b.g_codes {
        if let Some(pm) = g.path_mode() {
            match &ctx.modal.path {
                None => ctx.modal.path = Some(pm),
                Some(old) if *old != pm => {
                    return Err(vec![err(
                        ErrorCode::UnsupportedSemantics,
                        "path mode switch mid-program — one mode per program (SPEC §3)",
                        &ctx.file,
                        line,
                        blen,
                    )]);
                }
                _ => {} // повтор того же режима — легальный no-op
            }
        }
    }

    // (d) F — модальная подача (HAAS-правило: bare F → × least increment +
    // notice Setting 77; notice прицепится к raw-опу через pending_dpi).
    // S — обороты (голое S — сохранение, §3 контракта).
    if let Some(f) = b.word('F') {
        let num = ctx.feed_num(f);
        ctx.feed = Some(num);
    }
    if let Some(s) = b.word('S') {
        let num = verbatim(s);
        match ctx.spindle.clone() {
            Spindle::Off => ctx.last_s = Some(num),
            Spindle::Cw(_) => ctx.spindle = Spindle::Cw(num),
            Spindle::Ccw(_) => ctx.spindle = Spindle::Ccw(num),
        }
    }

    // (e) M3/M4/M5 → шпиндель; M8/M9 → СОЖ (M07 Shower — этап 2, НЕ mist).
    for m in &b.m_codes {
        match m {
            MCode::SpindleCw | MCode::SpindleCcw => {
                let n = match b.word('S') {
                    Some(s) => verbatim(s),
                    None => ctx.last_s.clone().unwrap_or_else(|| num_f64(0.0)),
                };
                ctx.spindle = if *m == MCode::SpindleCw {
                    Spindle::Cw(n)
                } else {
                    Spindle::Ccw(n)
                };
            }
            MCode::SpindleOff => ctx.spindle = Spindle::Off,
            MCode::CoolantFlood => ctx.coolant.flood = true,
            MCode::CoolantOff => ctx.coolant = Coolant::default(),
            MCode::ProgramEnd => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Слова без потребителя (молчаливый пропуск запрещён, COVERAGE §4)
// ---------------------------------------------------------------------------

/// Feature-слова вне скоупа → (RAW-код, причина); context-слова без контекста →
/// (E_INVALID_INPUT, причина) — сломанный вход (паритет Fanuc/LinuxCNC).
/// HAAS-СПЕЦИФИКА (§2.1): C/R в G01 = фаска/скругление угла (lookahead-
/// декомпозиция, этап 12) → Raw{SEMANTICS}, НЕ InvalidInput.
fn unused_word_diagnostic(
    b: &Block,
    plan: Plan,
    motion_mode: Option<GCode>,
) -> Option<(ErrorCode, String)> {
    let mut consumed: Vec<char> = vec!['F', 'S'];
    match plan {
        Plan::Motion => {
            consumed.extend(['X', 'Y', 'Z']);
            if matches!(motion_mode, Some(GCode::ArcCw) | Some(GCode::ArcCcw)) {
                consumed.extend(['I', 'J', 'K', 'R']);
            }
        }
        Plan::Noop => {}
    }
    for letter in ['T', 'H', 'D', 'U', 'W', 'A', 'B', 'C', 'E', 'O', 'P', 'Q', 'R', 'I', 'J', 'K', 'L']
    {
        if b.word(letter).is_none() || consumed.contains(&letter) {
            continue;
        }
        // G01 C/R — фаска/скругление угла (Haas-специфика §2.1): R в G01 —
        // радиус скругления (контекстное слово), C — фаска; lookahead-деко-
        // мпозиция следующего хода — этап 12 → Raw{SEMANTICS}.
        if matches!(plan, Plan::Motion)
            && motion_mode == Some(GCode::Linear)
            && (letter == 'C' || letter == 'R')
        {
            return Some((
                ErrorCode::UnsupportedSemantics,
                format!(
                    "{letter} word in G01 — corner chamfer/rounding between this and next move, needs lookahead decomposition (analysis §2.1; stage-12 candidate)"
                ),
            ));
        }
        return Some(match letter {
            'T' => (
                ErrorCode::UnsupportedSemantics,
                "T word alone — modal next-tool preload (SMTC), outside L1; candidate `selected_tool` (COVERAGE §3)".to_string(),
            ),
            'H' | 'D' => (
                ErrorCode::RequiresMachineTable,
                format!("{letter} offset number — machine table value (COVERAGE §4 H/D words)"),
            ),
            'U' | 'W' => (
                ErrorCode::UnsupportedSemantics,
                format!("{letter} word — Setting 162 mentions U/W (lathe legacy `?`), mill vocabulary stage 12 (COVERAGE §4; registry §10(22))"),
            ),
            'A' | 'B' | 'C' => (
                ErrorCode::UnsupportedSemantics,
                format!("{letter} rotary axis — stage 12 (COVERAGE §4; A in G12/G13/G70 context is part of raw generator line)"),
            ),
            'E' => (
                ErrorCode::UnsupportedSyntax,
                "E word (G187 accuracy) — vendor smoothness mode, stage-12 candidate (COVERAGE §2.7)".to_string(),
            ),
            'O' => (
                ErrorCode::UnsupportedSyntax,
                "O word — program number / subprogram structure, outside L1 (COVERAGE §4)".to_string(),
            ),
            _ => (
                ErrorCode::InvalidInput,
                format!("{letter} word without consumer in block (arc/G10/G4-cycle context expected; P dwell dot-rule — stage 2)"),
            ),
        });
    }
    None
}

// ---------------------------------------------------------------------------
// Движение (G0–G3 / модальное продолжение; G53 machine-frame — этап 2)
// ---------------------------------------------------------------------------

/// Целевая точка хода: слова (G90 abs / G91 inc) + база связывания.
/// SPEC §2 v0.11: в точке разрыва (a) отсутствующие оси end наследуют дефолт
/// контроллера (0); частичный end при unknown-базе — E_RUNTIME_STATE.
fn resolve_end(ctx: &Ctx, b: &Block, base: Option<[f64; 3]>) -> Result<Point3, Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let mut v: [Option<Num>; 3] = std::array::from_fn(|_| None);
    for (i, w) in [(0usize, 'X'), (1, 'Y'), (2, 'Z')] {
        v[i] = match b.word(w) {
            Some(n) => {
                let num = ctx.dpi_num(w, n);
                if ctx.modal.dist_abs {
                    Some(num)
                } else {
                    let base = base.ok_or_else(|| {
                        vec![err(
                            ErrorCode::RuntimeState,
                            "incremental move requires known current position (SPEC §2)",
                            &ctx.file,
                            line,
                            blen,
                        )]
                    })?;
                    Some(num_f64(base[i] + num.value()))
                }
            }
            None => match base {
                Some(bb) => Some(num_f64(bb[i])),
                None => {
                    return Err(vec![err(
                        ErrorCode::RuntimeState,
                        "partial end inherits unknown at frame break point (SPEC §2)",
                        &ctx.file,
                        line,
                        blen,
                    )]);
                }
            },
        };
    }
    match (v[0].take(), v[1].take(), v[2].take()) {
        (Some(x), Some(y), Some(z)) => Ok(Point3 { x, y, z }),
        _ => unreachable!("end fully resolved: base known or all axes explicit"),
    }
}

fn motion_block(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    let has_axis = b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some();
    let has_arc_words = b.word('I').is_some()
        || b.word('J').is_some()
        || b.word('K').is_some()
        || b.word('R').is_some();
    if !has_axis && !has_arc_words {
        return Ok(()); // кадр без целевых слов — движения нет
    }

    let linked = ctx.link_start();
    let base = match linked {
        Linked::Known(v) => Some(v),
        Linked::UnknownAtStart => Some([0.0; 3]), // дефолт контроллера (§2 v0.11)
    };
    let start_point = match linked {
        Linked::Known(v) => StartPoint::Known(p3(v)),
        Linked::UnknownAtStart => StartPoint::Unknown,
    };

    let mode = b.motion().or(ctx.modal.motion).expect("plan Motion ⇒ mode");

    if matches!(mode, GCode::ArcCw | GCode::ArcCcw) {
        return arc_block(ctx, b, mode, base);
    }

    let end = resolve_end(ctx, b, base)?;

    // zero-length (start == end) → drop + notice (SPEC §3).
    if let StartPoint::Known(s) = &start_point {
        if len3(&sub3(&end, s)) < 1e-9 {
            let op = if mode == GCode::Rapid {
                Op::Rapid {
                    start: start_point.clone(),
                    end: end.clone(),
                    state: ctx.state(),
                    src: span(&ctx.file, line, blen),
                    sub: None,
                }
            } else {
                Op::Linear {
                    start: start_point.clone(),
                    end: end.clone(),
                    state: ctx.state(),
                    src: span(&ctx.file, line, blen),
                    sub: None,
                }
            };
            ctx.push_drop(
                op,
                NoticeKind::ZeroLengthMotion,
                "zero-length motion dropped (start == end, SPEC §3)".into(),
            );
            ctx.prev = Some(Reading {
                pos: e3(&end),
                frame: Frame::Wcs,
                wcs: ctx.modal.wcs,
            });
            ctx.saw_motion = true;
            return Ok(());
        }
    }

    // feed: G0 не требует; linear без F → INVALID (SPEC §3/§4).
    let op = if mode == GCode::Rapid {
        Op::Rapid {
            start: start_point.clone(),
            end: end.clone(),
            state: ctx.state(),
            src: span(&ctx.file, line, blen),
            sub: None,
        }
    } else {
        if ctx.feed.is_none() {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                "linear move before any F word (SPEC §3/§4)",
                &ctx.file,
                line,
                blen,
            )]);
        }
        let mut st = ctx.state();
        if ctx.modal.feed_inverse {
            // G93 (ABSORB, §2.2): feed = len·F (SPEC §4); требует known start.
            let s = match &start_point {
                StartPoint::Known(s) => e3(s),
                StartPoint::Unknown => {
                    return Err(vec![err(
                        ErrorCode::RuntimeState,
                        "G93 feed resolution requires known start (SPEC §4)",
                        &ctx.file,
                        line,
                        blen,
                    )]);
                }
            };
            let f = ctx.feed.clone().expect("feed checked above");
            let len = {
                let d = [
                    end.x.value() - s[0],
                    end.y.value() - s[1],
                    end.z.value() - s[2],
                ];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
            };
            st.feed = Some(num_f64(len * f.value()));
        }
        Op::Linear {
            start: start_point,
            end: end.clone(),
            state: st,
            src: span(&ctx.file, line, blen),
            sub: None,
        }
    };
    ctx.push(op);
    ctx.prev = Some(Reading {
        pos: e3(&end),
        frame: Frame::Wcs,
        wcs: ctx.modal.wcs,
    });
    ctx.saw_motion = true;
    Ok(())
}

/// Дуга G2/G3 (§2.1 контракта, SPEC §6): IJK — инкрементальные offsets от
/// start ВСЕГДА (группы 04 в словаре Haas НЕТ — официальный ман: «I J K
/// values define the relative X Y Z distances from the starting point»);
/// G90.1 `?` (ABSORB-как-LinuxCNC ruling этапа 1) — абсолютный центр.
/// R-форма: знак R выбирает ≤180°/>180°; R при полной окружности →
/// E_INVALID_INPUT (§2.1: «R при полной окружности невалидна»); R+вне-
/// плоскостное слово (helical R-форма) — допустимо, `?` §10(24) (SPEC §6
/// v0.7 правило 1: нормаль известна из плоскости). Helical — вне-плоскостное
/// осевое слово → pitch. **Полное кольцо = IJK без end-слов** (официальный
/// ман: «Defining only the I J K values… will cut a circle»); ПО ГЕОМЕТРИИ:
/// center-форма end==start при center≠start → sweep 360° НЕЗАВИСИМО от
/// наличия явных end-слов (SPEC §3 v0.18); вырожденный центр (center ==
/// start) при end == start → drop+notice. Валидация нормали по активной
/// плоскости (SPEC §4/§6): перпендикулярная компонента (center − start) ≈ 0,
/// иначе E_INVALID_INPUT.
#[allow(clippy::too_many_lines)]
fn arc_block(
    ctx: &mut Ctx,
    b: &Block,
    mode: GCode,
    base: Option<[f64; 3]>,
) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let inval = |msg: String| err(ErrorCode::InvalidInput, msg, &ctx.file, line, blen);
    // ARC с start=unknown — E_RUNTIME_STATE (геометрия требует числового
    // старта, SPEC §2).
    let start = base.ok_or_else(|| {
        vec![err(
            ErrorCode::RuntimeState,
            "arc with unknown start (SPEC §2)",
            &ctx.file,
            line,
            blen,
        )]
    })?;
    let end = resolve_end(ctx, b, Some(start))?;

    let (a0, a1, al) = plane_axes(ctx.modal.plane);
    let s = start;
    let e = e3(&end);
    let pitch = e[al] - s[al]; // вне-плоскостная компонента хода (винт)
    let dir = if mode == GCode::ArcCw {
        Dir::Cw
    } else {
        Dir::Ccw
    };
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
            // (SPEC §6(2); контракт §2.1: «R при полной окружности невалидна»;
            // полный круг — только IJK-формой).
            if chord_plane < 1e-9 {
                return Err(vec![inval(
                    "R-arc with coincident in-plane start/end: ambiguous center — full circle only via IJK (SPEC §6; analysis §2.1)".into(),
                )]);
            }
            // Хорда > 2R — невозможная дуга (SPEC §6).
            let tol_r = 0.001f64.max(1e-6 * r.abs());
            let h2 = 4.0 * r * r - d0 * d0 - d1 * d1;
            if h2 < -tol_r * 4.0 * r.abs().max(1.0) {
                return Err(vec![inval(format!(
                    "arc chord {chord_plane:.4} > 2R {}: impossible arc (SPEC §6)",
                    r.abs()
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
            // Инкрементальные от start ВСЕГДА (группы 04 нет; §1.1/§2.1);
            // G90.1 `?` — ABSORB-как-LinuxCNC (ruling этапа 1).
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
                // G90.1 `?` — вне словаря Haas; ABSORB-как-LinuxCNC ruling.
                c[a0] = off_vals[a0];
                c[a1] = off_vals[a1];
            } else {
                // Дефолт Haas: инкремент от start ВСЕГДА, независимо от
                // G90/G91 (§2.1 контракта).
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
            if r < 0.0 {
                two_pi - sw
            } else {
                sw
            }
        }
        // IJK: полярные углы от центра; совпадение в плоскости = 360°
        // (**полный круг ПО ГЕОМЕТРИИ**: center ≠ start + start == end —
        // 360° НЕЗАВИСИМО от наличия явных end-слов, SPEC §3 v0.18; контракт
        // §2.1 «полное кольцо = IJK без end-слов» — частный случай).
        None => {
            let a_s = (s[a1] - c[a1]).atan2(s[a0] - c[a0]);
            let a_e = (e[a1] - c[a1]).atan2(e[a0] - c[a0]);
            let mut sweep_ccw = (a_e - a_s).rem_euclid(two_pi);
            if sweep_ccw < 1e-9 {
                sweep_ccw = two_pi;
            }
            let sw = if mode == GCode::ArcCw {
                two_pi - sweep_ccw
            } else {
                sweep_ccw
            };
            if sw < 1e-9 {
                two_pi
            } else {
                sw
            }
        }
    };

    // zero-length arc (SPEC §3 v0.17): нулевой ход = end-слова, квантованные
    // до стартовой точки (§10.6), при center == start (вырожденное кольцо
    // радиуса 0) → drop+notice. Center-форма с center ≠ start при
    // start == end = полный круг 360° НЕЗАВИСИМО от формы команды (v0.18;
    // CAM-посты систематически пишут end-слова на полных кругах).
    let rho_s = ((s[a0] - c[a0]).powi(2) + (s[a1] - c[a1]).powi(2)).sqrt();
    let rho_e = ((e[a0] - c[a0]).powi(2) + (e[a1] - c[a1]).powi(2)).sqrt();
    let explicit_end = b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some();
    if explicit_end && chord_plane < 1e-9 && pitch.abs() < 1e-9 && rho_s < 1e-9 {
        let mut st = ctx.state();
        st.feed = ctx.feed.clone();
        let op = Op::Arc {
            start: StartPoint::Known(pt(s[0], s[1], s[2])),
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
        ctx.prev = Some(Reading {
            pos: e,
            frame: Frame::Wcs,
            wcs: ctx.modal.wcs,
        });
        ctx.saw_motion = true;
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
    let eps_pos = 0.001f64.max(1e-7 * len3(&sub3(&pt(e[0], e[1], e[2]), &pt(s[0], s[1], s[2]))));
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
            &ctx.file,
            line,
            blen,
        )]);
    }
    let mut st = ctx.state();
    if ctx.modal.feed_inverse {
        // G93: feed = len(траектории)·F; длина винтовой дуги —
        // sqrt((sweep·ρ)² + pitch²) (SPEC §4, §6).
        let f = ctx.feed.clone().expect("feed checked above");
        let sweep_rad = sweep;
        let len = ((sweep_rad * rho_s).powi(2) + pitch.powi(2)).sqrt();
        st.feed = Some(num_f64(len * f.value()));
    } else {
        st.feed = ctx.feed.clone();
    }
    let op = Op::Arc {
        start: StartPoint::Known(pt(s[0], s[1], s[2])),
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
            "near-degenerate R-arc (±0.12° around 180°): center amplification (EQUIVALENCE §4)"
                .into(),
        );
    } else {
        ctx.push(op);
    }
    ctx.prev = Some(Reading {
        pos: e,
        frame: Frame::Wcs,
        wcs: ctx.modal.wcs,
    });
    ctx.saw_motion = true;
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
