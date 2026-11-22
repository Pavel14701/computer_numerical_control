//! Канонизатор GRBL 1.1 → EID/L1 AST (Агент C, этап 2).
//! Источники: SPEC v0.10 (§2 точки разрыва (a)–(d), §4 state, §5.3 machine-frame,
//! §6 дуги), COVERAGE §2 (диспозиции), CORPUS §1.2 (правило двух классов:
//! структурные нарушения → жёсткий Err; локальные конструкции → Raw-узел).
//! drop-with-notice — паттерн eid-core parser.rs (Notice, SPEC §3).

use crate::block::{Block, G10Kind, Marker, ParsedFile};
use crate::gcode::{GCode, MCode};
use eid_core::ast::{
    Coolant, Dir, Frame, Num, Op, Point3, Program, Spindle, SrcSpan, StartPoint, State, Units, Wcs,
};
use eid_core::diag::{Diagnostic, ErrorCode, LimitInfo};
use std::collections::HashSet;
use std::sync::Arc;

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
    /// Программа без M2/M30: неявный конец — ProgramEnd ДОБАВЛЕН последним
    /// опом (SPEC §3 v0.16 «отсутствие M2 = неявный конец, не отказ»;
    /// 18/32 Tier B — реальные посты не пишут program-end).
    MissingProgramEnd,
    /// Raw-motion-G (G28/G30/G28.1/G30.1/G38.x) с осевыми словами: модальное
    /// состояние абсорбировано, но motion-режим НЕ продолжается
    /// (SPEC §8.6 правило 6, v0.17 — критический аудит #33).
    ModalOnRaw,
    /// Блоки после program_end (M2/M30) — Raw-хвост: grbl-стрим ИСПОЛНЯЕТ
    /// хвост (source protocol.c:74-105 — главный цикл без проверки
    /// program_flow), rs274-файловый интерпретатор молча игнорирует
    /// (зонд 5: exit 0, канон обрывается на PROGRAM_END). Конструкция
    /// диалект-расходится по природе → кросс-диалект отказывает (R4).
    /// (SPEC §5.2 v0.20; core NoticeKind-супермножество v0.2 заморожен —
    /// на границе адаптера маппится в ModalOnRaw, семантика в message.)
    DialectDivergence,
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
    let mut s = format!("{v:.4}");
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

fn span(file: &Arc<str>, line: u32, len: usize) -> SrcSpan {
    SrcSpan {
        file: file.clone(),
        line_start: line,
        col_start: 1,
        line_end: line,
        col_end: len as u32 + 1,
    }
}

/// Диагностика с явным этапом: Parse — лексические пути GRBL, Verify —
/// канонизационные инварианты (§6-геометрия, позиционные правила).
fn err_staged(
    stage: eid_core::diag::Stage,
    code: ErrorCode,
    msg: impl Into<String>,
    file: &Arc<str>,
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

fn err(
    code: ErrorCode,
    msg: impl Into<String>,
    file: &Arc<str>,
    line: u32,
    len: usize,
) -> Diagnostic {
    err_staged(eid_core::diag::Stage::Verify, code, msg, file, line, len)
}

fn limit_err(
    threshold: &'static str,
    value: f64,
    actual: f64,
    msg: String,
    file: &Arc<str>,
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

/// Осевые слова хода в блоке: XYZ — словарные слова (b.words); A/B/C/U/V/W —
/// rotary вне словаря (в unsupported_words — WordKind их не классифицирует).
/// По словам, а не по байтам raw-строки: буквы в тексте комментария
/// («(A : 0 ; C : 0 )» в ncm-файлах) движением не являются.
fn block_has_motion_axis_words(b: &Block) -> bool {
    ['X', 'Y', 'Z'].iter().any(|l| b.word(*l).is_some())
        || b.unsupported_words
            .iter()
            .any(|w| matches!(w.letter, 'A' | 'B' | 'C' | 'U' | 'V' | 'W'))
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

/// hex-кодировка байтов строки (grammar D9 v0.3: чётная длина, A–F uppercase);
/// байты verbatim включая line terminator (\r\n / \n).
fn payload_hex(line: &str) -> String {
    let mut s = String::new();
    for b in line.as_bytes() {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

/// Маркер Raw-носителя units-switch (SPEC §2.5 принцип 5, ревизия v0.21):
/// префикс зеркален linuxcnc-адаптеру (v021-носитель) — тесты обоих
/// адаптеров пинуют «units switch mid-program».
pub(crate) const UNITS_SWITCH_REASON_PREFIX: &str = "units switch mid-program";

/// Reason-текст носителя смены системы чтения (N7-снятие, v0.21).
const UNITS_SWITCH_REASON: &str = "units switch mid-program (SPEC §2.5 принцип 5, ревизия v0.21 — GRBL исполняет G20/G21 в любой позиции, gcode.c case 20/21: gc_state.units переключается mid-stream, внутренняя позиция — mm, конверсия чтений внутренняя): смена системы чтения; chaining через конверсию 25.4 В ПРЕДЕЛАХ ДОПУСКА (4dp-дрейф ≤2e-5 мм < ε_pos); опа units в AST v0.2 нет — носитель line-level Raw (байты lossless same-dialect, кросс-диалект — отказ, правило Raw)";

/// Фактор конверсии чтения из системы `from` в `to` (SPEC §2.5 v0.21):
/// value_in_to = value_in_from × factor (мм→дюйм ÷25.4, дюйм→мм ×25.4).
fn units_factor(from: Units, to: Units) -> f64 {
    match (from, to) {
        (Units::Mm, Units::In) => 1.0 / 25.4,
        (Units::In, Units::Mm) => 25.4,
        _ => 1.0,
    }
}

/// Чтение в новой системе: 4dp-квантование конверсии (num_f64) — дрейф
/// ≤2e-5 мм < ε_pos (SPEC §2.5 v0.21: конверсия «в пределах допуска»).
fn conv_reading(p: &Point3, factor: f64) -> Point3 {
    pt(
        p.x.value() * factor,
        p.y.value() * factor,
        p.z.value() * factor,
    )
}

// ---------------------------------------------------------------------------
// Ctx — модальное состояние канонизации (SPEC §4) + позиционное отслеживание
// ---------------------------------------------------------------------------

struct Ctx {
    file: Arc<str>,
    units: Units, // ТЕКУЩАЯ система чтения: G20/G21 переключают mid-stream (v0.21)
    /// Заголовок Program.units — первое ЧИСТОЕ вхождение G20/G21 (до ходов и
    /// чтений). Смена после ходов заголовок НЕ меняет (дефолт Mm честен —
    /// linuxcnc-зеркало v021: «Program.units остаётся дефолт mm»).
    program_units: Option<Units>,
    dist_abs: bool,        // G90=true / G91=false
    feed_inverse: bool,    // G93
    plane: GCode,          // G17/G18/G19 — модальное состояние парсера (SPEC §4)
    motion: Option<GCode>, // модальный motion (G80 → None)
    wcs: Wcs,
    selected_wcs: HashSet<u8>,      // §5.3(b): все выбранные до хода
    wcs_origins: Vec<(u8, Point3)>, // wcs_define (G10 L2) до текущей точки
    feed: Option<Num>,              // модальный F (переживает G0, SPEC §4)
    /// Провенанс: F-слово было в кадре-источнике текущего блока (v0.2
    /// волна 3, §2.4); сброс — в начале absorb_frame_state, установка —
    /// при абсорбции F; в снапшот — через state().
    feed_restated: bool,
    /// bare-T → ABSORB (SPEC §5.2 v0.20): grbl парсит T всегда
    /// (gcode.c:315-316 — только max-value проверка `value > MAX_TOOL_NUMBER`),
    /// исполняет молча (gcode.c:936 «[5. Select tool]: NOT SUPPORTED. Only
    /// tracks tool value» → `gc_state.tool = t`; lookup/M6/действия нет —
    /// зонд 6b). Аналог linuxcnc st.selected_tool: ВНУТРЕННЕЕ поле канонизатора
    /// (State.tool в AST — физический инструмент, замороженный интерфейс;
    /// grbl никогда не строит ToolChange-оп — смены не исполняет).
    selected_tool: Option<u32>,
    last_s: Option<Num>,
    spindle: Spindle,
    coolant: Coolant,
    anchor: Option<(Point3, Wcs)>, // пресетка (G92/L20 full-triple): reading старта
    frame_def_count: usize,        // записей фрейма (G92/L20): >0 → G53 = RUNTIME (§5.3a, N16)
    /// G92-пресеток в файле (Q5 ЗАКРЫТ v0.20: несколько G92 легальны);
    /// l20_count — G10 L20. Смешанная пара L20+G92 — по-прежнему Err
    /// (правило невзаимозаменяемости §5.3: разные узлы персистентности).
    g92_def_count: usize,
    l20_count: usize,
    /// Чтение-сдвиг mid-program G92 (SPEC §3 v0.20, зонды 1–3): накопленное
    /// изменение чтения ОДНОЙ И ТОЙ ЖЕ машинной точки относительно reading-
    /// фрейма prev_end; применяется в reading_now поверх базового чтения,
    /// сбрасывается в update_pos (после хода prev_end уже в новом фрейме).
    g92_shift: [f64; 3],
    /// Единицы ОПРЕДЕЛЕНИЯ пресетки (стартовая G92 / G10 L20): header
    /// g92_values хранится verbatim в этих единицах; anchor — чтение,
    /// конвертируется в текущие единицы при units-switch (v0.21).
    anchor_units: Units,
    prev_end: Option<Point3>, // end последнего motion-опа (в его reading)
    prev_is_machine: bool,    // последний motion был G53
    prev_wcs: Wcs,            // reading prev_end (wcs или Machine)
    pos_mach: Option<Point3>, // машинные координаты текущей позиции
    /// Chain-continuity через Raw-блоки (Q11-механизм; инвентаризация
    /// Tier A#3, wave 2 v0.21-кандидат): Raw-носитель с осевыми словами
    /// (G41/G43-строки с X/Y/Z, rotary-only `G0 A0.`) представляет РЕАЛЬНОЕ
    /// движение контроллера, которое AST L1 не выражает (split-parse
    /// запрещён как скрытая декомпозиция). Позиционная цепочка РАЗРЫВАЕТСЯ:
    /// следующий ход парсится от start=unknown — RAPID/LINEAR с полным end
    /// Ok от unknown; ARC / частичный end / G91 → честный E_RUNTIME_STATE
    /// вместо ложной «невозможной дуги / radius mismatch», валидировавшейся
    /// от СТАРОЙ позиции (comp311, placa, ncm_g41_d, ncm_g43_g69).
    /// Сбрасывается в update_pos — ход с явным end восстанавливает цепочку.
    /// prev_end/anchor при этом НЕ чистятся: разрыв «после genuinely-unknown»
    /// — не разрыв (a) «старт программы», дефолт-0 недоступен (SPEC §2 v0.11).
    pos_unknown: bool,
    /// На ТЕКУЩЕМ блоке была смена системы чтения (G20/G21 mid-stream,
    /// v0.21): блок уходит в line-level Raw-носитель (process_block 7b).
    /// Сброс — в absorb_frame_state (по кадру, как feed_restated).
    line_units_switch: bool,
    ops: Vec<Op>,
    notices: Vec<Notice>,
    units_established: bool, // первое слово G20/G21 устанавливает (SPEC §2.5, v0.21)
    g43_family_seen: bool,   // правило семейств §1.3 (G49): G43.1 в файле
    g92_values: Option<(Point3, Wcs)>, // Program.frame_preset — СТАРТОВАЯ пресетка
    end_count: usize,        // ProgramEnd-опов (M2/M30 после первого — Raw-хвост)
    end_seen: bool,          // первый M2/M30 обработан: хвост → Raw (SPEC §5.2 v0.20)
    tail_notice_sent: bool,  // notice диалект-расхождения — один на программу
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
            feed_restated: self.feed_restated,
        }
    }

    fn origin_of(&self, p: u8) -> Option<&Point3> {
        self.wcs_origins
            .iter()
            .find(|(q, _)| *q == p)
            .map(|(_, o)| o)
    }

    fn push(&mut self, op: Op) {
        self.ops.push(op);
    }

    /// drop-with-notice (оп НЕ попадает в ops, SPEC §3).
    fn push_drop(&mut self, op: Op, kind: NoticeKind, message: String) {
        let idx = self.ops.len();
        self.notices.push(Notice {
            kind,
            message,
            op_index: idx,
            op,
        });
    }

    /// notice при ЖИВОМ опе (near-degenerate R-arc, EQUIVALENCE §4).
    fn push_keep(&mut self, op: Op, kind: NoticeKind, message: String) {
        let idx = self.ops.len();
        self.ops.push(op.clone());
        self.notices.push(Notice {
            kind,
            message,
            op_index: idx,
            op,
        });
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
    // v0.2 (#21): один Arc<str> на прогон канонизации; span() клонирует хэндл
    // (refcount) — 0 аллокаций на оп.
    let file: Arc<str> = Arc::from(file);
    // Лимит max_source_lines (COVERAGE §1.2).
    let line_count = src.lines().count() as f64;
    if line_count > 1_000_000.0 {
        return Err(vec![limit_err(
            "max_source_lines",
            1_000_000.0,
            line_count,
            format!("file has {} lines, limit 1000000", src.lines().count()),
            &file,
            1,
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
                    "max_word_chars",
                    32.0,
                    32.0,
                    "word value exceeds max_word_chars (32)".into(),
                    &file,
                    e.line,
                ),
                kind => err_staged(
                    eid_core::diag::Stage::Parse,
                    ErrorCode::InvalidInput,
                    format!("lexical error: {:?} at col {}", kind, e.col),
                    &file,
                    e.line,
                    e.col as usize,
                ),
            })
            .collect());
    }

    let mut ctx = Ctx {
        file: file.clone(),
        units: Units::Mm,
        program_units: None,
        dist_abs: true,
        feed_inverse: false,
        plane: GCode::PlaneXY,
        motion: None,
        wcs: Wcs::G54,
        selected_wcs: HashSet::from([1]),
        wcs_origins: Vec::new(),
        feed: None,
        feed_restated: false,
        selected_tool: None,
        last_s: None,
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        anchor: None,
        frame_def_count: 0,
        g92_def_count: 0,
        l20_count: 0,
        g92_shift: [0.0; 3],
        anchor_units: Units::Mm,
        prev_end: None,
        prev_is_machine: false,
        prev_wcs: Wcs::G54,
        pos_mach: None,
        pos_unknown: false,
        line_units_switch: false,
        ops: Vec::new(),
        notices: Vec::new(),
        units_established: false,
        g43_family_seen: parsed
            .blocks
            .iter()
            .any(|b| b.g_codes.contains(&GCode::ToolLenDynamic)),
        g92_values: None,
        end_count: 0,
        end_seen: false,
        tail_notice_sent: false,
        end_line: 0,
    };

    for b in parsed.blocks.iter() {
        process_block(&mut ctx, b)?;
    }
    finalize(&mut ctx, line_count as u32)
}

/// Структурная проверка program_end (SPEC §5.2 v0.20): блоки после M2/M30 —
/// Raw-хвост (см. process_block), поэтому «не последний» (N12) и «дубль»
/// (N13) НЕ строят второй ProgramEnd-оп и больше не Err — парсер grbl-
/// стрима не останавливается (source protocol.c:74-105). Оба гейта
/// оставлены defensive: ProgramEnd-оп строится ровно один раз (end_seen),
/// end_count > 1 из parse недостижим. ОТСУТСТВИЕ M2/M30 → неявный конец:
/// ProgramEnd добавляется последним опом с notice (SPEC §3 v0.16;
/// 18/32 Tier B) + сборка Program.
fn finalize(ctx: &mut Ctx, line_count: u32) -> Result<ParseOut, Vec<Diagnostic>> {
    debug_assert!(
        ctx.end_count <= 1,
        "ProgramEnd-оп строится один раз (end_seen gate)"
    );
    if ctx.end_count > 1 {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "duplicate program end op (CORPUS n13; defensive — end_seen gate)",
            &ctx.file,
            ctx.end_line,
            0,
        )]);
    }
    if ctx.end_count == 0 {
        // SPEC §3 v0.16: отсутствие M2/M30 = неявный конец — авто-PEND
        // последним опом с notice (18/32 Tier B: реальные посты не пишут
        // program-end). src — весь файл (позиция вставки).
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
            "W_MISSING_PROGRAM_END: no M2/M30 — implicit program end appended (SPEC §3 v0.16; 18/32 Tier B)".into(),
        );
    }
    // Лимит max_ops_per_program (COVERAGE §1.2) — после авто-PEND (лимит
    // считается по ИТОГОВОМУ списку ops+notices).
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
        // grammar v0.3 — magic-строка (SPEC §10.4): изменение формата = новая версия.
        eid_version: "EID/L1 v0.3".to_string(),
        // Заголовок = первое ЧИСТОЕ вхождение G20/G21; без него — дефолт Mm
        // (дефолт GRBL честен: смена после ходов заголовок не меняет —
        // v0.21, linuxcnc-зеркало «st.units.unwrap_or(Mm)»).
        units: ctx.program_units.unwrap_or(Units::Mm),
        // SPEC §3 v0.12: отсутствие G61/G61.1/G64 → exact_path (дефолт);
        // G61 — единственный path-код словаря GRBL — тоже exact_path.
        path_mode: eid_core::ast::PathMode::ExactPath,
        frame_preset: ctx
            .g92_values
            .take()
            .map(|(values, wcs)| eid_core::ast::FramePreset { values, wcs }),
        ops: std::mem::take(&mut ctx.ops),
    };
    let notices = std::mem::take(&mut ctx.notices);
    Ok(ParseOut { program, notices })
}

// ---------------------------------------------------------------------------
// process_block — диспетчер одного кадра
// ---------------------------------------------------------------------------

fn process_block(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    // (0) Raw-хвост после program_end (SPEC §5.2 v0.20): grbl-стрим ИСПОЛНЯЕТ
    // блоки после M2/M30 — главный цикл protocol.c:74-105 читает строки и
    // зовёт gc_execute_line БЕЗ проверки program_flow; rs274-файловый
    // интерпретатор молча игнорирует хвост (зонд 5: exit 0, канон-поток
    // обрывается на PROGRAM_END). Прежний Err «program end is not the last
    // block» (CORPUS n12) СНЯТ: каждый блок хвоста — lossless Raw-оп
    // (same-dialect verbatim), program_end остаётся опом в последовательности
    // (не forced-last). Модальная абсорбция не нужна: легальных опов после
    // PEND не строится, хвост целиком Raw (модальный сброс grbl на M2/M30 —
    // gcode.c:1080-1128 «[F,S,T,H] do not reset» — не влияет на AST).
    // Notice диалект-расхождения — один на программу, на первом хвостовом
    // блоке; кросс-диалект для такой программы отказывает (правило Raw,
    // SPEC §8.2).
    if ctx.end_seen {
        let op = ctx.raw_op(
            b,
            &b.raw,
            ErrorCode::UnsupportedSemantics,
            "tail after program end: executed in grbl-stream semantics (protocol.c:74-105 — main loop without program_flow check), ignored in rs274-file semantics (probe 5: exit 0, canon stops at PROGRAM_END)",
        );
        if !ctx.tail_notice_sent {
            ctx.tail_notice_sent = true;
            ctx.push_keep(
                op,
                NoticeKind::DialectDivergence,
                "tail after program end: executed in grbl-stream semantics, ignored in rs274-file semantics — dialect-divergent, cross-dialect refuses (SPEC §5.2 v0.20)".into(),
            );
            return Ok(());
        }
        ctx.push(op);
        return Ok(());
    }

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
            format!("modal group violation in block: {group:?} after {g:?}"),
            &ctx.file,
            line,
            blen,
        )]);
    }

    // (2a) Модальная абсорбция ДО любых raw-возвратов (SPEC §8.6 правило 6,
    // v0.17 — критический аудит #33): строка, уходящая в Raw целиком,
    // обязана абсорбировать своё frame-level состояние до push_raw — иначе
    // следующий кадр парсится в потерянной модальности (пробой аудита:
    // `G91 G28 Z0` терял G91 → следующий `G0 X10 Y10` молча абсолютный).
    // Единицы — тоже абсорбция (§8.5: смена units — структурный класс,
    // жёсткий отказ). Для легальных кадров тот же код выполнял шаги
    // (5)/(8)–(9) прежней нумерации — порядок эффектов не изменился.
    absorb_frame_state(ctx, b)?;

    // (3) Слова вне словаря → Raw line-level. Оси A/B/C/U/V/W → SEMANTICS
    // (COVERAGE §4); G74/G84/G87/G88 → SEMANTICS (refuse-класс COVERAGE §2.5:
    // рулинг — класс определяется конструкцией-ИСТОЧНИКОМ, LinuxCNC-словарь
    // валиден, «синхронизация/реверс не представимы ходами»); прочие
    // (M6, G64, G90.1, H, ...) → SYNTAX (вне словаря GRBL).
    if !b.unsupported_words.is_empty() {
        let cycle_sem = b.unsupported_words.iter().any(|w| {
            w.letter == 'G'
                && w.value.value().fract() == 0.0
                && matches!(w.value.value() as i64, 74 | 76 | 84 | 87 | 88)
        });
        let axes_sem = !cycle_sem
            && b.unsupported_words
                .iter()
                .all(|w| matches!(w.letter, 'A' | 'B' | 'C' | 'U' | 'V' | 'W'));
        let (code, reason) = if cycle_sem {
            (
                ErrorCode::UnsupportedSemantics,
                "G74/G84/G87/G88: spindle-sync/reverse cycles not representable as moves (COVERAGE §2.5 refusal; ruling: class by SOURCE construct)",
            )
        } else if axes_sem {
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
        // §8.6 правило 6: структурная часть состояния (G92/G10) — тоже до
        // push_raw (вызов ТОЛЬКО на raw-ветках; легальный путь обрабатывает
        // G92/G10 в process_tail — двойная обработка недопустима).
        absorb_frame_structs(ctx, b)?;
        // Chain-break через Raw-блок (Ctx.pos_unknown, инвентаризация
        // Tier A#3): блок с осевыми словами — реальное движение контроллера
        // (`G41 G1 Y4` ведёт Y в 4; `G0 A0.` при RTCP двигает TCP), ход
        // непредставим → позиция после блока неизвестна. Без break следующий
        // ход валидировался бы от СТАРОЙ позиции → ложные «невозможная дуга
        // / radius mismatch» (зонд: с G41-блоком отказ, без — OK).
        if block_has_motion_axis_words(b) {
            ctx.pos_unknown = true;
        }
        ctx.push(op);
        return Ok(());
    }

    // (4) T-слово (SPEC §5.2 v0.20): bare-T → ABSORB в state.selected_tool
    // (absorb_frame_state (2a) — и на RAW-строках), Err «bare T word» СНЯТ.
    // Источник: gcode.c:315-316 — парсер принимает T всегда (только max-
    // проверка `value > MAX_TOOL_NUMBER`), gcode.c:936-937 «[5. Select tool]:
    // NOT SUPPORTED. Only tracks tool value» → `gc_state.tool = t` молча;
    // lookup/M6/исполнения смены нет (зонд 6b: T без lookup — ноль событий
    // канона). В AST tool_change НЕ строится: реальный grbl смену не
    // исполняет; M6 вне словаря GRBL (gcode.c:939 «[6. Change tool]: NOT
    // SUPPORTED») → строка с M6 уходит в Raw{SYNTAX} в (3) как прежде.

    // (5)→(2a) Единицы перенесены в absorb_frame_state (SPEC §8.6 правило 6,
    // аудит #33: units абсорбируются и на RAW-строках).

    // (6) Метаданные и IGNORED_SAFE-коды (COVERAGE §2.1/§2.2/§2.4).
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
        absorb_frame_structs(ctx, b)?;
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
        absorb_frame_structs(ctx, b)?;
        ctx.push(op);
        return Ok(());
    }

    // (7) RAW line-level по матрице COVERAGE §2.1/§2.3/§2.6. SPEC §8.6
    // (v0.17): raw-motion-G (G28/G30/G28.1/G30.1/G38.x) с осевыми словами —
    // модальное состояние уже абсорбировано в (2a); motion-режим НЕ
    // продолжается (ход не представим — «продолжение» молча
    // интерпретировало бы следующий X/Y-кадр в старом режиме) + notice
    // W_MODAL_ON_RAW.
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
                ErrorCode::RequiresMachineTable,
                "G92.1 resets the whole G92 offset register — restoring prior readings requires the absolute machine position, absent from AST (SPEC §3 v0.20 / Q4; probe 4: SET_G92_OFFSET(0,..) + return to pre-G92 readings)",
            ),
            _ => continue,
        };
        let op = ctx.raw_op(b, &b.raw, code, reason);
        absorb_frame_structs(ctx, b)?;
        let raw_motion_g = matches!(
            g,
            GCode::Home | GCode::SetHome | GCode::Home2 | GCode::SetHome2 | GCode::Probe(_)
        );
        let has_motion_words = ['X', 'Y', 'Z', 'I', 'J', 'K', 'R']
            .iter()
            .any(|l| b.word(*l).is_some());
        if raw_motion_g && has_motion_words {
            ctx.motion = None;
            ctx.push_keep(
                op,
                NoticeKind::ModalOnRaw,
                "W_MODAL_ON_RAW: G28/G30/G38.x — raw motion block with axis words; modal motion mode NOT continued (SPEC §8.6 rule 6, audit #33)".into(),
            );
            return Ok(());
        }
        ctx.push(op);
        return Ok(());
    }

    let _ = (line, blen);

    // (7b) Units-switch mid-program (SPEC §2.5 принцип 5, ревизия v0.21;
    // снятие N7 — см. absorb_frame_state): блок со сменой системы чтения —
    // line-level Raw-носитель (опа units в AST v0.2 нет), образец — G92-mid
    // Raw-носитель v0.20. После веток (3)/(6)/(7): там блок уже уходит в
    // Raw по СВОЕМУ основанию с полными байтами строки — units при этом
    // абсорбированы в (2a), отдельный носитель не нужен (lossless тот же).
    if ctx.line_units_switch {
        return units_switch_carrier(ctx, b);
    }
    process_tail(ctx, b)
}

/// Носитель смены системы чтения (v0.21): Raw-оп с байтами строки +
/// chaining. Ход на switch-строке: grbl исполняет ВЕСЬ блок (gcode.c —
/// units в [2], motion в [10]) — цепочка чтений продолжается в НОВОЙ
/// системе: оп хода НЕ строится (байты строки в носителе), end
/// разрешается в новой системе и пишет prev_end (зеркало linuxcnc v021
/// «оп не строится — байты строки в носителе, носитель первым»). Ход,
/// end которого неразрешим (частичный/инкрементальный при неизвестном
/// чтении) или не исполняется (нет моды хода; G53 без G0/G1) — честный
/// разрыв цепочки: pos_unknown (Q11-механизм fc01bb9).
fn units_switch_carrier(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let op = ctx.raw_op(
        b,
        &b.raw,
        ErrorCode::UnsupportedSemantics,
        UNITS_SWITCH_REASON,
    );
    // §8.6 правило 6: структурная часть (G92/G10) — до push_raw, как на
    // прочих raw-ветках.
    absorb_frame_structs(ctx, b)?;
    // Мода хода: явная в кадре обновляет модальность (зеркало motion_block);
    // дальше — позиционный эффект хода (если он был).
    if let Some(m) = b.g_codes.iter().copied().find(|g| {
        matches!(
            g,
            GCode::Rapid | GCode::Linear | GCode::ArcCw | GCode::ArcCcw
        )
    }) {
        ctx.motion = Some(m);
    }
    let has_arc_words = ['I', 'J', 'K', 'R'].iter().any(|l| b.word(*l).is_some());
    if block_has_motion_axis_words(b) || has_arc_words {
        if b.g_codes.contains(&GCode::MachineFrame) {
            // G53 на switch-строке: machine-конец физичен (конверсии не
            // подлежит). G53 без G0/G1 grbl отказывает — ход не исполняется.
            if matches!(ctx.motion, Some(GCode::Rapid) | Some(GCode::Linear)) {
                match reading_now(ctx, true) {
                    Err(_) => ctx.pos_unknown = true, // §5.3-нарушение — ход в носителе
                    Ok(reading) => match resolve_end(ctx, b, &reading, false) {
                        Ok(Some(end)) => update_pos(ctx, true, end),
                        _ => ctx.pos_unknown = true,
                    },
                }
            }
        } else if ctx.motion.is_some() {
            // Err недостижим: WCS-ветка reading_now без §5.3-гейтов не
            // возвращает ошибок (они только в machine-ветке).
            let reading = reading_now(ctx, false).unwrap_or(None);
            match resolve_end(ctx, b, &reading, true) {
                Ok(Some(end)) => update_pos(ctx, false, end),
                _ => ctx.pos_unknown = true, // частичный/инкрементальный без чтения
            }
        }
        // motion None: слова без активной моды хода grbl отказывает —
        // позиция не меняется (pos_unknown не трогаем).
    }
    ctx.push(op);
    Ok(())
}

fn motion_block(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    if let Some(m) = b.g_codes.iter().copied().find(|g| {
        matches!(
            g,
            GCode::Rapid | GCode::Linear | GCode::ArcCw | GCode::ArcCcw
        )
    }) {
        ctx.motion = Some(m);
    }
    let g53 = b.g_codes.contains(&GCode::MachineFrame);
    let has_axis = b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some();
    let has_arc_words = b.word('I').is_some()
        || b.word('J').is_some()
        || b.word('K').is_some()
        || b.word('R').is_some();

    if g53 && !matches!(ctx.motion, Some(GCode::Rapid) | Some(GCode::Linear)) {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "G53 requires G0/G1 in the same block (grbl gcode.c)",
            &ctx.file,
            line,
            blen,
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
    // §5.3-нарушения (пресетка/не-in-text origins) внутри reading_now → Err;
    // легальный случай None — первый machine-ход = точка разрыва (c) v0.10:
    // RAPID получает start=unknown (CORPUS #37), последующие цепляются.
    let reading = reading_now(ctx, g53)?;

    if matches!(mode, GCode::ArcCw | GCode::ArcCcw) {
        return arc_block(ctx, b, mode, reading);
    }

    // Цель: слова G90/G91 + наследование осей (SPEC §2: частичный end при
    // неизвестном reading — E_RUNTIME_STATE; дефолт-0 — только для WCS-старта (a)).
    let end =
        resolve_end(ctx, b, &reading, !g53)?.expect("end known: reading known or full G90 triple");

    let frame = if g53 { Frame::Machine } else { Frame::Wcs };
    let start = reading
        .clone()
        .map(StartPoint::Known)
        .unwrap_or(StartPoint::Unknown);

    // zero-length (start == end) → drop + notice (SPEC §3).
    if let StartPoint::Known(s) = &start {
        if len3(&sub3(&end, s)) < 1e-9 {
            let op = if mode == GCode::Rapid {
                Op::Rapid {
                    start: start.clone(),
                    end: end.clone(),
                    state: ctx.state(frame),
                    src: span(&ctx.file, line, blen),
                    sub: None,
                }
            } else {
                Op::Linear {
                    start: start.clone(),
                    end: end.clone(),
                    state: ctx.state(frame),
                    src: span(&ctx.file, line, blen),
                    sub: None,
                }
            };
            ctx.push_drop(
                op,
                NoticeKind::ZeroLengthMotion,
                "zero-length motion dropped (start == end, SPEC §3)".into(),
            );
            update_pos(ctx, g53, end);
            return Ok(());
        }
    }

    // feed: G0 не требует; linear без F → INVALID (SPEC §4). G93 → len × F.
    let op = if mode == GCode::Rapid {
        // G93 не применяется к rapid (rapid не ходит по feed) — F как есть.
        Op::Rapid {
            start,
            end: end.clone(),
            state: ctx.state(frame),
            src: span(&ctx.file, line, blen),
            sub: None,
        }
    } else {
        let f = ctx.feed.clone().ok_or_else(|| {
            vec![err(
                ErrorCode::InvalidInput,
                "linear move before any F word (SPEC §4)",
                &ctx.file,
                line,
                blen,
            )]
        })?;
        let feed = resolved_feed(ctx, &start, &end, f, line, blen)?;
        let mut st = ctx.state(frame);
        st.feed = Some(feed);
        Op::Linear {
            start,
            end: end.clone(),
            state: st,
            src: span(&ctx.file, line, blen),
            sub: None,
        }
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
                &ctx.file,
                0,
                0,
            )]);
        }
        for p in &ctx.selected_wcs {
            if ctx.origin_of(*p).is_none() {
                return Err(vec![err(
                    ErrorCode::RuntimeState,
                    format!("machine-frame move: WCS P{p} selected but not wcs_define'd in text (SPEC §5.3b; CORPUS n1)"),
                    &ctx.file, 0, 0,
                )]);
            }
        }
        // §5.3-инварианты текста проверены выше; позиционно цепочка разорвана
        // Raw-носителем с осевыми словами (Ctx.pos_unknown) → unknown.
        if ctx.pos_unknown {
            return Ok(None);
        }
        return Ok(machine_start(ctx));
    }
    // Chain-break через Raw-блок (Q11): чтение неизвестно до явного end.
    if ctx.pos_unknown {
        return Ok(None);
    }
    let base = if ctx.prev_end.is_none() {
        // (a) первый motion-оп: якорь = frame_preset / wcs_set_here (той же WCS).
        ctx.anchor
            .as_ref()
            .filter(|(_, w)| *w == ctx.wcs)
            .map(|(p, _)| p.clone())
    } else if ctx.prev_is_machine {
        // (d) первый WCS-ход после machine-ходов: reading = M − O_wcs − Δ.
        let o = ctx.origin_of(wcs_to_p(&ctx.wcs));
        match (ctx.pos_mach.clone(), o) {
            (Some(m), Some(o)) => Some(sub3(&m, o)),
            _ => None,
        }
    } else if ctx.prev_wcs == ctx.wcs {
        ctx.prev_end.clone()
    } else {
        // (b) смена wcs: обе системы in-text → reading_new = prev_end + O_prev − O_new
        // (регистр G92 сокращается: prev_end несёт его в своём reading, а новое
        // чтение той же машины — тоже — зонды 2/8: глобальный регистр поверх WCS).
        let o_prev = ctx.origin_of(wcs_to_p(&ctx.prev_wcs));
        let o_cur = ctx.origin_of(wcs_to_p(&ctx.wcs));
        match (ctx.prev_end.clone(), o_prev, o_cur) {
            (Some(e), Some(a), Some(c)) => Some(sub3(&add3(&e, a), c)),
            _ => None,
        }
    };
    // SPEC §3 v0.20 (зонды 1–3): mid-program G92 ребейзит чтение той же
    // машинной точки; g92_shift — накопленное изменение с фрейма prev_end.
    // Сдвиг обнуляется в update_pos (после хода prev_end в новом фрейме).
    Ok(base.map(|p| {
        let mut v = e3(&p);
        for (i, s) in v.iter_mut().enumerate() {
            *s -= ctx.g92_shift[i];
        }
        pt(v[0], v[1], v[2])
    }))
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
/// end наследуют дефолт контроллера (0) в активном фрейме (allow_controller_default);
/// после genuinely-неизвестных позиций ((b)–(d), вкл. machine-ходы) частичный
/// end → E_RUNTIME_STATE.
fn resolve_end(
    ctx: &Ctx,
    b: &Block,
    reading: &Option<Point3>,
    allow_controller_default: bool,
) -> Result<Option<Point3>, Vec<Diagnostic>> {
    let rv = reading.as_ref().map(e3);
    // Разрыв цепочки Raw-носителем (pos_unknown) — НЕ точка разрыва (a):
    // дефолт-0 контроллера применим только на старте программы, до ходов.
    let at_start =
        allow_controller_default && reading.is_none() && ctx.prev_end.is_none() && !ctx.pos_unknown;
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
                                &ctx.file,
                                b.line_no,
                                b.raw.len(),
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
                    &ctx.file,
                    b.line_no,
                    b.raw.len(),
                )])
            } else {
                unreachable!("reading known ⇒ все оси разрешены")
            }
        }
    }
}

/// G94: F как есть; G93: feed = len × F (только для linear/arc, SPEC §4).
fn resolved_feed(
    ctx: &Ctx,
    start: &StartPoint,
    end: &Point3,
    f: Num,
    line: u32,
    blen: usize,
) -> Result<Num, Vec<Diagnostic>> {
    if !ctx.feed_inverse {
        return Ok(f);
    }
    match start {
        StartPoint::Known(s) => Ok(num_f64(len3(&sub3(end, s)) * f.value())),
        StartPoint::Unknown => Err(vec![err(
            ErrorCode::RuntimeState,
            "G93 requires known trajectory length (start unknown)",
            &ctx.file,
            line,
            blen,
        )]),
    }
}

/// Дуга G2/G3: IJK (инкрементальные offsets от старта, in-plane пара по
/// плоскости; COVERAGE §6.11) или R-форма (SPEC §6: знак + ≤180°, − >180°).
/// Вне-плоскостное слово → pitch. Валидация §6 → E_INVALID_INPUT (N5a/N5b/N19).
fn arc_block(
    ctx: &mut Ctx,
    b: &Block,
    mode: GCode,
    reading: Option<Point3>,
) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let inval = |msg: String| err(ErrorCode::InvalidInput, msg, &ctx.file, line, blen);
    // ARC с start=unknown — E_RUNTIME_STATE (геометрия дуги требует числового
    // старта, SPEC §2).
    let start = reading.ok_or_else(|| {
        vec![err(
            ErrorCode::RuntimeState,
            "arc with unknown start (SPEC §2)",
            &ctx.file,
            line,
            blen,
        )]
    })?;
    let end = resolve_end(ctx, b, &Some(start.clone()), true)?
        .expect("end known: reading known or full G90 triple");

    let (a0, a1, al) = plane_axes(&ctx.plane);
    let s = e3(&start);
    let e = e3(&end);
    let pitch = e[al] - s[al]; // вне-плоскостная компонента хода (винт)
    let dir = if mode == GCode::ArcCw {
        Dir::Cw
    } else {
        Dir::Ccw
    };
    let normal = match ctx.plane {
        GCode::PlaneXY => [0.0, 0.0, 1.0],
        GCode::PlaneZX => [0.0, 1.0, 0.0],
        _ => [1.0, 0.0, 0.0],
    };

    // Центр: in-plane пара; linear-компонента = start (круг в плоскости старта).
    let mut c = s;
    let r_val = b.word('R').map(|n| n.value());
    // v0.21 (инвентаризация §5, класс #9): дуга-продолжение БЕЗ offset-слов
    // (ни I/J/K, ни R в блоке) — прежняя подстановка нулевых offsets давала
    // ложный «arc radius mismatch» (ρ_start 0 vs ρ_end ≠ 0) вместо честной
    // причины. Реальный GRBL = error 2 STATUS_GCODE_ARC_OFFSETS_MISSING:
    // offsets обязательны в КАЖДОМ дуговом блоке (модального «продолжения
    // offset'ов» предыдущей дуги у grbl нет), LinuxCNC: «I/J/K word missing
    // with G2/G3» — оба контроллера отказывают → TRUE-ERR. Формулировка
    // зеркальна linuxcnc-адаптеру (v021-фикс, тест-пин на подстроку).
    // Частичная пара (например, только I) сохраняет прежнюю семантику
    // «отсутствующая ось = 0» (COVERAGE §6.11).
    if r_val.is_none() && b.word('I').is_none() && b.word('J').is_none() && b.word('K').is_none() {
        return Err(vec![inval(
            "дуга без offset-слов: в дуговом блоке нет ни I/J/K, ни R — GRBL error 2 (ARC_OFFSETS_MISSING: offsets обязательны в каждом дуговом блоке), LinuxCNC: „I/J/K word missing with G2/G3“".into(),
        )]);
    }
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
                return Err(vec![inval(format!(
                    "arc chord {chord_plane:.4} > 2R {}: impossible arc (SPEC §6; CORPUS n5b)",
                    r.abs()
                ))]);
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
            if r < 0.0 {
                two_pi - sw
            } else {
                sw
            }
        }
        // IJK: полярные углы от точного центра; совпадение в плоскости = 360°.
        None => {
            let a_s = (s[a1] - c[a1]).atan2(s[a0] - c[a0]);
            let a_e = (e[a1] - c[a1]).atan2(e[a0] - c[a0]);
            let mut sweep_ccw = (a_e - a_s).rem_euclid(two_pi);
            if sweep_ccw < 1e-9 {
                sweep_ccw = two_pi; // полная окружность (SPEC §6, ловушка (а))
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

    // zero-length / full circle (SPEC §3/§6 v0.17, аудит #15 — зонд rs274
    // «Zero-radius arc» / ARC_FEED rotation=-1): полнота круга — ГЕОМЕТРИЯ
    // (center ≠ start при start == end в плоскости), НЕЗАВИСИМО от формы
    // команды: явные end-слова, квантовавшиеся в старт (§10.6), и
    // наследованный end (нет слов) дают ОДИНАКОВО 360° (CORPUS #13 v0.6.7 —
    // диспозиция drop → 360°-дуга; CAM-посты пишут end-слова на полных
    // кругах). Drop — единственный класс center-формы: вырожденный центр
    // (center == start, радиус 0) при pitch == 0 — ОБЕ формы end-слов
    // (rs274 отказывает «Zero-radius arc»); pitch ≠ 0 + degenerate —
    // 360-винтовая r=0, остаётся (как D/fanuc). Проверка ПОСЛЕ квантования
    // входа.
    let rho_s = ((s[a0] - c[a0]).powi(2) + (s[a1] - c[a1]).powi(2)).sqrt();
    let rho_e = ((e[a0] - c[a0]).powi(2) + (e[a1] - c[a1]).powi(2)).sqrt();
    if chord_plane < 1e-9 && pitch.abs() < 1e-9 && rho_s < 1e-9 {
        let f = ctx.feed.clone().unwrap_or_else(|| num_f64(0.0));
        let mut st = ctx.state(Frame::Wcs);
        st.feed = Some(f);
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
            "zero-radius arc dropped (degenerate center == start, end == start, pitch == 0; SPEC §3/§6 v0.17; CORPUS #13)".into(),
        );
        update_pos(ctx, false, end);
        return Ok(());
    }
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
        vec![err(
            ErrorCode::InvalidInput,
            "arc move before any F word (SPEC §4)",
            &ctx.file,
            line,
            blen,
        )]
    })?;
    let feed = if ctx.feed_inverse {
        num_f64(arc_len * f.value())
    } else {
        f
    };
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
            "near-degenerate R-arc (±0.12° around 180°): center amplification (EQUIVALENCE §4)"
                .into(),
        );
    } else {
        ctx.push(op);
    }
    update_pos(ctx, false, end);
    Ok(())
}

/// После motion-опа: prev_end/reading/машинная позиция. g92_shift сбрасывается:
/// end записан УЖЕ в пост-G92 reading-фрейме (SPEC §3 v0.20 chaining —
/// следующий ход цепляется от нового чтения).
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
    ctx.g92_shift = [0.0; 3];
    ctx.pos_unknown = false; // ход с явным end восстановил цепочку (Q11)
}

/// Якорь-согласование units-switch (v0.21; linuxcnc-зеркало
/// `convert_units_anchor`): конверсия чтения применяется и к ХРАНИМОМУ end
/// последнего motion-опа — пара `start_{i+1} == end_i` (замороженный
/// core-verify, SPEC §2 v0.9 «точное равенство») обязана остаться бит-в-бит
/// после конверсии, иначе «start != expected» на validate-пути CLI/emit.
/// Дуга: center/start — та же геометрия (§6), равномерный скейл; каскад
/// назад по бит-цепочке (start дуги == end предыдущего опа — скейлится и
/// он), до первого rapid/linear (их start — literal-пара с нескошенным end).
/// Вызывается ТОЛЬКО когда последний motion — WCS (machine-конец физичен,
/// конверсии не подлежит).
fn convert_units_anchor(ctx: &mut Ctx, factor: f64) {
    let mut idx = match ctx
        .ops
        .iter()
        .rposition(|op| matches!(op, Op::Rapid { .. } | Op::Linear { .. } | Op::Arc { .. }))
    {
        Some(i) => i,
        None => return, // ходов нет — якоря нет
    };
    let mut need_end = true;
    loop {
        // Каскадная связь с предыдущим опом — ДО масштабирования: start дуги
        // был бит-равен end предыдущего опа?
        let cascade = match &ctx.ops[idx] {
            Op::Arc {
                start: StartPoint::Known(p),
                ..
            } if idx > 0
                && matches!(
                    &ctx.ops[idx - 1],
                    Op::Rapid { .. } | Op::Linear { .. } | Op::Arc { .. }
                ) =>
            {
                let e = match &ctx.ops[idx - 1] {
                    Op::Rapid { end, .. } | Op::Linear { end, .. } | Op::Arc { end, .. } => e3(end),
                    _ => unreachable!("проверено выше"),
                };
                e == [p.x.value(), p.y.value(), p.z.value()]
            }
            _ => false,
        };
        {
            let op = &mut ctx.ops[idx];
            if need_end {
                if let Some(end) = match op {
                    Op::Rapid { end, .. } | Op::Linear { end, .. } | Op::Arc { end, .. } => {
                        Some(end)
                    }
                    _ => None,
                } {
                    let c = conv_reading(end, factor);
                    *end = c;
                }
            }
            if let Op::Arc { start, center, .. } = op {
                let cc = conv_reading(center, factor);
                *center = cc;
                if let StartPoint::Known(p) = start {
                    let sc = conv_reading(p, factor);
                    *p = sc;
                }
            }
        }
        if cascade && idx > 0 {
            idx -= 1;
            need_end = true;
        } else {
            return;
        }
    }
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

/// Модальная абсорбция кадра (SPEC §8.6 правило 6, v0.17 — аудит #33):
/// вызывается ДО любых raw-возвратов process_block — строка, уходящая в Raw
/// целиком, обязана обновить frame-level состояние до push_raw, иначе
/// следующий кадр парсится в потерянной модальности (пробой: `G91 G28 Z0`
/// терял G91 → следующий `G0 X10 Y10` молча абсолютный). Для легальных
/// кадров — тот же код, что прежние шаги (5)/(8)/(9) process_tail: единицы
/// (одна система на программу, SPEC §2.5), выбор
/// WCS/плоскости/дистанции/feeder-режима (COVERAGE §2.2/§2.3), затем —
/// порядок grbl gcode.c: feed → spindle → coolant. Motion-G-коды кадра
/// здесь НЕ обновляют ctx.motion: raw-motion-G не продолжается
/// (W_MODAL_ON_RAW на raw-ветке), легальный путь обновляет в motion_block.
fn absorb_frame_state(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    // Провенанс F-кадра — по кадру (v0.2 волна 3): сброс на начале блока.
    ctx.feed_restated = false;
    // Маркер носителя units-switch — по кадру (v0.21): сброс на начале блока.
    ctx.line_units_switch = false;

    // Единицы — ревизия SPEC §2.5 принцип 5 v0.21 (инвентаризация Tier C,
    // снятие N7): реальный grbl исполняет G20/G21 в ЛЮБОЙ позиции (gcode.c
    // case 20/21 — gc_state.units переключается mid-stream; внутренняя
    // позиция хранится в mm, конверсия чтений внутренняя) — прежний Err
    // «одна система на программу» был тем же недоразумением, что G92-mid/
    // bare-T (директива ревью). Первое ЧИСТОЕ вхождение (до ходов/чтений)
    // устанавливает заголовок Program.units (inch-файлы легальны, CORPUS
    // #10); дальнейшие G20/G21 = смена системы чтения, число переключений
    // НЕ ограничено. Chaining: чтение после переключения = прежнее
    // физическое положение в НОВОЙ системе — конверсия 25.4 В ПРЕДЕЛАХ
    // ДОПУСКА (4dp-дрейф ≤2e-5 мм < ε_pos). Носитель смены — line-level
    // Raw (process_block 7b; опа units в AST v0.2 нет). Повтор той же
    // системы — легальный no-op.
    for g in &b.g_codes {
        if matches!(g, GCode::UnitsIn | GCode::UnitsMm) {
            let u = if *g == GCode::UnitsIn {
                Units::In
            } else {
                Units::Mm
            };
            // Чистое первое вхождение: ни ходов, ни чтений ещё нет —
            // конверсии нечего делать, заголовок = система (linuxcnc-
            // зеркало v021: «чистое первое вхождение — смена чтения не
            // нужна»). Пресетка (anchor) или разрыв цепочки делают
            // вхождение НЕ чистым: чтение существует (в старой системе).
            let pure_first = !ctx.units_established
                && ctx.prev_end.is_none()
                && ctx.anchor.is_none()
                && !ctx.pos_unknown
                && ctx.g92_shift == [0.0; 3];
            if u != ctx.units {
                if pure_first {
                    ctx.program_units = Some(u);
                } else {
                    // Смена системы чтения (вкл. ПЕРВОЕ вхождение после
                    // mm-дефолтных ходов: Program.units остаётся дефолт Mm —
                    // заголовок честен). Конверсия цепочки чтений: prev_end
                    // (WCS-чтение; machine-конец физичен — конверсии не
                    // подлежит, linuxcnc-зеркало), anchor-пресетка и
                    // накопленный g92_shift — всё в СТАРОЙ системе.
                    let factor = units_factor(ctx.units, u);
                    if !ctx.prev_is_machine {
                        if let Some(e) = ctx.prev_end.take() {
                            ctx.prev_end = Some(conv_reading(&e, factor));
                        }
                        // Якорь-согласование core-verify (linuxcnc-зеркало
                        // v021-fix, N7/eid verify): конверсия применяется и к
                        // ХРАНИМОМУ end последнего motion-опа (пара
                        // start_{i+1} == end_i обязана остаться бит-в-бит,
                        // иначе «start != expected» на validate-пути CLI/emit).
                        convert_units_anchor(ctx, factor);
                    }
                    if let Some((a, w)) = ctx.anchor.take() {
                        ctx.anchor = Some((conv_reading(&a, factor), w));
                    }
                    for s in ctx.g92_shift.iter_mut() {
                        *s *= factor;
                    }
                    ctx.anchor_units = u;
                    ctx.line_units_switch = true;
                }
                ctx.units = u;
            } else if pure_first {
                // Первое вхождение системы ПО УМОЛЧАНИЮ (G21 при mm-дефолте):
                // заголовок = система, смены чтения нет.
                ctx.program_units = Some(u);
            }
            ctx.units_established = true;
        }
    }

    // Выбор WCS/плоскости/дистанции/feeder-режима — ABSORB (COVERAGE §2.2/§2.3).
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

    // Порядок grbl gcode.c: [5] T (§5.2 v0.20 ABSORB) → [7] spindle → [8] coolant.
    // T-проверки зеркалят gcode.c:315-316 ТОЧНО: только не-отрицательность и
    // `value > MAX_TOOL_NUMBER` (255, gcode.c:28) → FAIL(STATUS_GCODE_MAX_
    // VALUE_EXCEEDED); дробное усекается как `int_value = trunc(value)`
    // (gcode.c:132). ПОСЛЕ проверок — молча в state (gcode.c:936), без
    // lookup и без построения опа (смену grbl не исполняет — зонд 6b).
    if let Some(t) = b.word('T') {
        let v = t.value();
        if v < 0.0 {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                format!("T word value {v}: negative tool number (grbl gcode.c:315)"),
                &ctx.file,
                line,
                blen,
            )]);
        }
        if v > 255.0 {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                format!("T word value {v} exceeds MAX_TOOL_NUMBER 255 (grbl gcode.c:316, STATUS_GCODE_MAX_VALUE_EXCEEDED)"),
                &ctx.file,
                line,
                blen,
            )]);
        }
        // Повторное T просто обновляет state (`gc_state.tool = t` на каждом
        // исполняемом блоке — gcode.c:936); видимого следа в AST нет:
        // State.tool — ФИЗИЧЕСКИЙ инструмент (SPEC §4), grbl смен не делает.
        ctx.selected_tool = Some(v as u32);
    }
    if let Some(f) = b.word('F') {
        ctx.feed = Some(verbatim(f));
        // F-слово было в кадре (restated-F, SPEC §7.4a): маркер кадра важен
        // независимо от того, изменилось ли ЗНАЧЕНИЕ модального feed.
        ctx.feed_restated = true;
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
                let s = s_word
                    .clone()
                    .or_else(|| ctx.last_s.clone())
                    .unwrap_or_else(|| num_f64(0.0));
                ctx.spindle = Spindle::Cw(s);
            }
            MCode::SpindleCcw => {
                let s = s_word
                    .clone()
                    .or_else(|| ctx.last_s.clone())
                    .unwrap_or_else(|| num_f64(0.0));
                ctx.spindle = Spindle::Ccw(s);
            }
            MCode::SpindleOff => ctx.spindle = Spindle::Off,
            MCode::MistOn => ctx.coolant.mist = true,
            MCode::FloodOn => ctx.coolant.flood = true,
            MCode::CoolantOff => ctx.coolant = Coolant::default(),
            _ => {}
        }
    }
    Ok(())
}

/// Где сработал G92 (SPEC §3 v0.20): стартовая зона (пресетка header-поля)
/// или mid-program (chaining + оп у legal-пути).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum G92Where {
    Startup,
    Mid,
}

/// Общая семантика G92 (SPEC §3 v0.20 — зонды 1–3: сдвиг последующих
/// координат; per-axis: неуказанные оси чтение НЕ меняют; replace-по-
/// пересчёту от текущей машины; глобальный регистр поверх WCS — зонд 8).
/// Вызывается из legal-пути (process_g92) и raw-пути (absorb_frame_structs);
/// различие только в построении опа (legal строит на Mid, raw — нет).
/// Диспозиции:
/// * стартовая зона (до первого motion-опа), ПЕРВАЯ G92 — СТАРТОВАЯ
///   пресетка: full-triple обязателен — частичный → RUNTIME (незаданные
///   оси — неизвестное стартовое чтение; header-поле Program.frame_preset —
///   полный triple, back-compat goldens не изменяются);
/// * стартовая зона, ПОВТОРНАЯ G92 — per-axis слияние в ту же пресетку
///   (машина ещё не двигалась → пересчёт вырождается в merge; зонд 3:
///   неуказанные оси сохраняют прежнее чтение);
/// * MID-программа — chaining: чтение последующих ходов = новое по заданным
///   осям (g92_shift, зонды 1–2). Блок без XYZ-слов (`G92 E0` PrusaSlicer)
///   регистр XYZ не трогает (grbl gcode.c:561 `if (!axis_words) FAIL` —
///   строка вне словаря уходит Raw в (3)) — ни shift, ни счётчики.
///
/// Смешанные пресетки G10 L20 → G92 — по-прежнему Err SEMANTICS (§5.3
/// правило невзаимозаменяемости: разные узлы персистентности; Q5 закрывает
/// ТОЛЬКО несколько G92).
fn apply_g92(ctx: &mut Ctx, b: &Block) -> Result<G92Where, Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    if ctx.l20_count > 0 {
        return Err(vec![err(
            ErrorCode::UnsupportedSemantics,
            "G92 after G10 L20: mixed preset constructs (SPEC §5.3 non-interchangeability; CORPUS n17-class)",
            &ctx.file,
            line,
            blen,
        )]);
    }
    let words = [(0usize, 'X'), (1, 'Y'), (2, 'Z')];
    if ctx.prev_end.is_none() {
        // СТАРТОВАЯ зона.
        if ctx.g92_def_count == 0 {
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
            ctx.anchor = Some((values, ctx.wcs));
            ctx.anchor_units = ctx.units; // слова кадра — в текущей системе
        } else {
            // Повторная G92 до ходов: per-axis merge пресетки (зонд 3:
            // неуказанные оси чтение не меняют; машина не двигалась, поэтому
            // replace-по-пересчёту (зонд 2) == merge заданных осей).
            // Merge — по ЧТЕНИЮ: anchor живёт в ТЕКУЩИХ единицах чтения
            // (v0.21: units могли переключиться после первой пресетки —
            // anchor конвертируется из anchor_units; указанные оси — слова
            // кадра в текущей системе). Header g92_values — verbatim в
            // единицах ОПРЕДЕЛЕНИЯ (anchor_units): указанные оси
            // конвертируются из текущих, неуказанные остаются verbatim.
            let mut header = ctx.g92_values.clone().expect("g92_def_count > 0");
            let mut reading = ctx.anchor.clone().expect("g92_def_count > 0 → anchor");
            let f_cur_to_def = units_factor(ctx.units, ctx.anchor_units);
            for (i, l) in words {
                if let Some(v) = b.word(l) {
                    let n = verbatim(v); // в ТЕКУЩИХ единицах чтения
                    match i {
                        0 => {
                            header.0.x = num_f64(n.value() * f_cur_to_def);
                            reading.0.x = n;
                        }
                        1 => {
                            header.0.y = num_f64(n.value() * f_cur_to_def);
                            reading.0.y = n;
                        }
                        _ => {
                            header.0.z = num_f64(n.value() * f_cur_to_def);
                            reading.0.z = n;
                        }
                    }
                }
            }
            header.1 = ctx.wcs;
            reading.1 = ctx.wcs;
            ctx.g92_values = Some(header);
            ctx.anchor_units = ctx.units;
            ctx.anchor = Some(reading);
        }
    } else {
        // MID-программа (зонды 1–3): rebase чтения последующих ходов.
        // shift[i] += R[i] − v[i] относительно reading-фрейма prev_end
        // (обновляется в update_pos: после хода prev_end уже в новом
        // фрейме). Чтение неизвестно → задать его из G92 нельзя (Point3
        // all-or-nothing) — хвост цепляется как прежде, блок несёт Raw.
        if let Some(r) = reading_now(ctx, false)? {
            let rv = e3(&r);
            let mut touched = false;
            for (i, l) in words {
                if let Some(v) = b.word(l) {
                    ctx.g92_shift[i] += rv[i] - v.value();
                    touched = true;
                }
            }
            if touched {
                ctx.g92_def_count += 1;
                ctx.frame_def_count += 1; // §5.3(a): G53 после пресетки — RUNTIME (N16)
            }
        }
        return Ok(G92Where::Mid);
    }
    ctx.g92_def_count += 1;
    ctx.frame_def_count += 1;
    Ok(G92Where::Startup)
}

/// Структурная часть абсорбции (SPEC §8.6: G92/G10 в списке абсорбции) —
/// вызывается ТОЛЬКО перед raw-возвратами process_block. Легальный путь
/// обрабатывает G92/G10 в process_tail (10) — двойная обработка
/// недопустима (задвоение wcs_origins). G92 — apply_g92 (v0.20: стартовая
/// пресетка / per-axis merge / mid-chaining), оп не строится: кадр
/// представлен Raw-опом. G10 — только ПОЛНАЯ легальная форма L2
/// (P 1..=6 целое, XYZ полные) → обновление таблицы origin; RAW-формы
/// (L20/частичные/сломанные) таблицу не трогают — значение из машины,
/// вне текста.
fn absorb_frame_structs(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    // G92: общая семантика с legal-путём (SPEC §3 v0.20); на raw-кадре
    // пресетка/mid-chaining абсорбируются без построения опа.
    if b.g_codes.contains(&GCode::FramePreset) {
        apply_g92(ctx, b)?;
    }

    // G10 L2 ПОЛНАЯ форма → обновление таблицы origin (§8.6: G10 в списке
    // абсорбции). G10-обновление idempotent по форме (те же слова), но
    // вызывается только на raw-ветке — задвоения wcs_origins нет.
    if b.g_codes.contains(&GCode::SetCoordSystem) && b.g10_kind() == Some(G10Kind::Define) {
        if let Some(p) = match b.word('P') {
            Some(n) if n.value() >= 1.0 && n.value() <= 6.0 && n.value().fract() == 0.0 => {
                Some(n.value() as u8)
            }
            _ => None,
        } {
            if let (Some(x), Some(y), Some(z)) = (b.word('X'), b.word('Y'), b.word('Z')) {
                let origin = Point3 {
                    x: verbatim(x),
                    y: verbatim(y),
                    z: verbatim(z),
                };
                ctx.wcs_origins.push((p, origin));
            }
        }
    }

    Ok(())
}

fn process_tail(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    // (8)/(9) — модальные группы + F/S/M3-5/M7-9 — перенесены в
    // absorb_frame_state (process_block (2a), SPEC §8.6 правило 6, #33).

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
            vec![err(
                ErrorCode::InvalidInput,
                "G4 requires P word (seconds)",
                &ctx.file,
                line,
                blen,
            )]
        })?;
        let sec = p.value();
        let op = Op::Dwell {
            seconds: verbatim(p),
            state: ctx.state(Frame::Wcs),
            src: span(&ctx.file, line, blen),
            sub: None,
        };
        if sec < 0.0 {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                "G4 P < 0: negative dwell (SPEC §5.2)",
                &ctx.file,
                line,
                blen,
            )]);
        }
        if sec == 0.0 {
            ctx.push_drop(
                op,
                NoticeKind::ZeroDwell,
                "zero dwell dropped (seconds == 0, SPEC §5.2)".into(),
            );
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
        // SPEC §5.2 v0.20: ProgramEnd-оп строится РОВНО ОДИН раз (первый
        // M2/M30); последующие блоки уходят в Raw-хвост в process_block (0)
        // — grbl-стрим продолжает исполнение (protocol.c:74-105), поэтому
        // «M2 не последний» больше не Err (CORPUS n12 снят).
        ctx.end_count += 1;
        ctx.end_seen = true;
        ctx.end_line = line;
        ctx.ops.push(Op::ProgramEnd {
            src: span(&ctx.file, line, blen),
        });
    }
    Ok(())
}

/// G10 L2 → WcsDefine (любая позиция, SPEC §5.3); L20 → WcsSetHere (пресетка).
fn process_g10(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let bad_p = || {
        err(
            ErrorCode::InvalidInput,
            "G10 requires P word, 1..=6 (GRBL WCS count)",
            &ctx.file,
            line,
            blen,
        )
    };
    let p: u8 = match b.word('P') {
        Some(n) if n.value() >= 1.0 && n.value() <= 6.0 && n.value().fract() == 0.0 => {
            n.value() as u8
        }
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
            // Позиционно-зависим: только пресетка до первого motion-опа (SPEC §5.3);
            // единственная L20 (смешанная L20+G92 — Err в apply_g92).
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
                    &ctx.file,
                    line,
                    blen,
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
            ctx.l20_count += 1;
            ctx.anchor = Some((delta, wcs_from_p(p)));
            ctx.anchor_units = ctx.units; // delta — verbatim в текущей системе
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

/// G92 → frame_preset (SPEC §3 v0.20, Q4-частично/Q5-закрыт):
/// стартовая зона — header-поле Program.frame_preset (первая G92 full-triple,
/// повторные — per-axis merge; back-compat goldens не изменяются);
/// mid-program — chaining чтения (apply_g92) + Raw-оп как lossless-носитель
/// блока. Носитель Raw, а не узел frame_preset: grammar/AST v0.3 НЕ имеют
/// mid-program frame_preset-опa (eid-core заморожен, Op 10 вариантов);
/// семантика исполнена в каноническом состоянии полностью — при разморозке
/// интерфейса узел заменяется 1:1, до тех пор кросс-диалект отказывает
/// (R4-правило Raw, консервативно: зонды показывают одинаковый эффект
/// G92 в rs274 и grbl, но R4 отказ безопасен).
fn process_g92(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let mid = apply_g92(ctx, b)?;
    if mid == G92Where::Mid {
        // Без XYZ-слов (`G92 E0` PrusaSlicer) регистр XYZ не тронут (grbl
        // gcode.c:561 `if (!axis_words) FAIL` — у источника строка вне
        // словаря); reason не должен называть это frame_preset-семантикой.
        let has_xyz = b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some();
        let reason = if has_xyz {
            "G92 mid-program: frame_preset (SPEC §3 v0.20; probes 1-3: per-axis assignment, replace-by-recompute, global register over WCS) — L1 v0.3 AST has no mid-program frame_preset op; block carried Raw losslessly, reading chain rebased in canonical state; cross-dialect refuses (R4) until the interface gains the op"
        } else {
            "G92 mid-program without XYZ axis words — XYZ offset register untouched (grbl gcode.c:561: `if (!axis_words) FAIL`); block carried Raw losslessly"
        };
        let op = ctx.raw_op(b, &b.raw, ErrorCode::UnsupportedSemantics, reason);
        ctx.push(op);
    }
    Ok(())
}
