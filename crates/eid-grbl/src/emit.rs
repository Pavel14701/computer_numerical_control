//! Эмиттер GRBL 1.1 (Агент C, итерация 3) — EID/L1 AST → текст GRBL.
//! Правила: COVERAGE §5 (дуги — in-plane пары IJK; tool_change → SYNTAX;
//! G92 первым блоком после выборки preset.wcs — EQUIVALENCE §1.4, без
//! «запекания» в G10), R-форма не эмитится никогда (EQUIVALENCE §4).
//! Числа — eid_core::emitter::fmt_num: фиксированные 4 dp, trailing zeros
//! сохраняются (EQUIVALENCE §1.4: два адаптера на одном EID — идентичный блок).
//!
//! RAW: payload-hex декодируется в исходные байты строки ВКЛЮЧАЯ line
//! terminator (grammar D9 v0.3: \r\n / \n) → verbatim (same-dialect fidelity,
//! байт-в-байт). КРОСС-ДИАЛЕКТНЫЙ ОТКАЗ E_RAW_UNTRANSLATABLE —
//! НЕ ЗДЕСЬ: это правило translate-пути (Агент E, итерация 3–5) — программа
//! с RAW-опами при переводе в другой диалект отклоняется целиком ДО эмиссии
//! (SPEC §8.2); эмиттер same-dialect только выводит payload как был.

use crate::gcode::GCode;
use eid_core::ast::{Coolant, Num, Op, Point3, Program, Spindle, StartPoint, State, Units, Wcs};
use eid_core::diag::{Diagnostic, ErrorCode};
use eid_core::emitter::fmt_num;

/// Плоскость дуги по канонической нормали (SPEC §6: знак +ось; произвольная
/// нормаль — нарушение канонической формы → Err INVALID).
fn plane_of(normal: &[f64; 3]) -> Result<GCode, String> {
    let (x, y, z) = (normal[0], normal[1], normal[2]);
    match (x, y, z) {
        (v, 0.0, 0.0) if (v - 1.0).abs() < 1e-9 => Ok(GCode::PlaneYZ),
        (0.0, v, 0.0) if (v - 1.0).abs() < 1e-9 => Ok(GCode::PlaneZX),
        (0.0, 0.0, v) if (v - 1.0).abs() < 1e-9 => Ok(GCode::PlaneXY),
        _ => Err(format!(
            "arc normal ({x},{y},{z}) is not a canonical +axis (SPEC §6)"
        )),
    }
}

/// In-plane пара offsets: (слово, ось) — I/J/K literal X/Y/Z (COVERAGE §6.11):
/// G17 → I+J, G18 → I+K, G19 → J+K. Порядок эмиссии — слово за словом.
fn arc_pair(plane: &GCode) -> (&'static str, usize, &'static str, usize) {
    match plane {
        GCode::PlaneZX => ("I", 0, "K", 2), // G18: I(X) + K(Z)
        GCode::PlaneYZ => ("J", 1, "K", 2), // G19: J(Y) + K(Z)
        _ => ("I", 0, "J", 1),              // G17: I(X) + J(Y)
    }
}

fn wcs_code(w: &Wcs) -> &'static str {
    match w {
        Wcs::G54 => "G54",
        Wcs::G55 => "G55",
        Wcs::G56 => "G56",
        Wcs::G57 => "G57",
        Wcs::G58 => "G58",
        Wcs::G59 => "G59",
        // G59.1/.2/.3 вне словаря GRBL (COVERAGE §2.3) — канонизатор GRBL их
        // не производит; кросс-адаптерный AST → отказ эмиссии.
        _ => "",
    }
}

struct Ectx {
    out: String,
    plane: GCode,
    wcs: Wcs,
    feed: Option<f64>,
    spindle: Spindle,
    coolant: Coolant,
    first_motion: bool,
}

fn spindle_eq(a: &Spindle, b: &Spindle) -> bool {
    match (a, b) {
        (Spindle::Off, Spindle::Off) => true,
        (Spindle::Cw(x), Spindle::Cw(y)) | (Spindle::Ccw(x), Spindle::Ccw(y)) => {
            x.value() == y.value()
        }
        _ => false,
    }
}

impl Ectx {
    /// Смена состояния перед опом (порядок grbl: wcs → spindle → coolant).
    fn state_prefix(&mut self, st: &State) {
        // WCS явно перед первым motion-опом и при смене (COVERAGE §5; бриф п.3).
        if self.first_motion || st.wcs != self.wcs {
            self.out.push_str(wcs_code(&st.wcs));
            self.out.push('\n');
            self.wcs = st.wcs;
        }
        if !spindle_eq(&st.spindle, &self.spindle) {
            match &st.spindle {
                Spindle::Off => self.out.push_str("M5\n"),
                Spindle::Cw(n) => self.out.push_str(&format!("M3 S{}\n", fmt_num(n.value()))),
                Spindle::Ccw(n) => self.out.push_str(&format!("M4 S{}\n", fmt_num(n.value()))),
            }
            self.spindle = st.spindle.clone();
        }
        if st.coolant != self.coolant {
            // Дельта coolant (CORPUS #29): ЛЮБОЕ изменение поля даёт M-коды.
            // Переход к none (из flood / mist / flood+mist) → M9; включения —
            // M7/M8; ЛЮБОЕ выключение оси при живой второй (частичное) —
            // M9 (гасит обе) + повтор оставшейся (GRBL не умеет точечно).
            let prev = self.coolant;
            let new = st.coolant;
            if !new.flood && !new.mist {
                self.out.push_str("M9\n");
            } else if (prev.flood && !new.flood) || (prev.mist && !new.mist) {
                self.out.push_str("M9\n");
                if new.mist {
                    self.out.push_str("M7\n");
                }
                if new.flood {
                    self.out.push_str("M8\n");
                }
            } else {
                if new.mist && !prev.mist {
                    self.out.push_str("M7\n");
                }
                if new.flood && !prev.flood {
                    self.out.push_str("M8\n");
                }
            }
            self.coolant = new;
        }
    }

    fn line(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push('\n');
    }
}

fn err(code: ErrorCode, msg: impl Into<String>, src: Option<eid_core::ast::SrcSpan>) -> Diagnostic {
    Diagnostic {
        code,
        message: msg.into(),
        src,
        options: Vec::new(),
        limit: None,
        stage: eid_core::diag::Stage::Emit,
    }
}

// ---------------------------------------------------------------------------
// EmissionLegality — ConversionTarget-паттерн (MLIR): легальность эмиссии в
// GRBL — ДАННЫЕ, не размазанные проверки. Единый источник кодов отказа emit:
// валидатор проверяет условия и берёт ErrorCode ТОЛЬКО из таблицы
// (COVERAGE §5, GRBL-колонка). E_RAW_UNTRANSLATABLE в таблицу НЕ входит —
// это translate-путь (Агент E, SPEC §8.2), не same-dialect эмиссия.
// ---------------------------------------------------------------------------

/// Статическая таблица легальности эмиссии GRBL 1.1.
#[derive(Debug, Clone, PartialEq)]
pub struct EmissionLegality {
    /// Виды опов, представимые ходами GRBL (имена = op_kind()).
    pub legal: Vec<&'static str>,
    /// Условия отказа: (имя условия, код класса легальности).
    pub illegal: Vec<(GrblEmissionCondition, ErrorCode)>,
    /// Легально-но-потерьные условия: (условие, текст Q3-warning). Эмиттер
    /// пишет текст в warnings-буфер адаптера (take_warnings), отказом НЕ
    /// является (COVERAGE §6.1/§5: warning-цель по manifest r4_expect).
    pub warnings: Vec<(&'static str, &'static str)>,
}

/// Условие отказа легальности эмиссии в GRBL 1.1 (типизированное, v0.2
/// волна 4 — INTERFACE-V0.2 §2.5 В1: enum вместо &'static str; код —
/// ТОТАЛЬНАЯ функция `code()`, паник-точек «таблица и эмиттер разошлись»
/// на emit-пути нет; согласие таблицы — тест `legality_table_complete`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrblEmissionCondition {
    /// `tool_change` — M6 вне словаря GRBL 1.1 (#27).
    ToolChange,
    /// `wcs_outside_g54_g59` — G59.x вне словаря GRBL.
    WcsOutsideG54G59,
    /// `motion_feed_none` — канон-форма §3/§4.
    MotionFeedNone,
    /// `arc_unknown_start` — IJK требует числового старта.
    ArcUnknownStart,
    /// `arc_normal_non_canonical` — нормаль не единичная +ось (§6).
    ArcNormalNonCanonical,
    /// `raw_payload_not_hex` — payload не чётный hex (grammar D9 v0.3).
    RawPayloadNotHex,
    /// `program_end_missing` — структура (CORPUS n14).
    ProgramEndMissing,
    /// `program_end_duplicate` — структура (CORPUS n13).
    ProgramEndDuplicate,
    /// `program_end_not_last` — НЕ-Raw оп после program_end (CORPUS n12;
    /// SPEC §5.2 v0.20: Raw-хвост после PEND легален — grbl исполняет).
    ProgramEndNotLast,
}

impl GrblEmissionCondition {
    /// Каноническое имя условия (сохранено 1:1 со строковой таблицы —
    /// его читают тесты и COVERAGE §5; manifest r4_expect хранит КОДЫ).
    pub fn as_str(self) -> &'static str {
        match self {
            GrblEmissionCondition::ToolChange => "tool_change",
            GrblEmissionCondition::WcsOutsideG54G59 => "wcs_outside_g54_g59",
            GrblEmissionCondition::MotionFeedNone => "motion_feed_none",
            GrblEmissionCondition::ArcUnknownStart => "arc_unknown_start",
            GrblEmissionCondition::ArcNormalNonCanonical => "arc_normal_non_canonical",
            GrblEmissionCondition::RawPayloadNotHex => "raw_payload_not_hex",
            GrblEmissionCondition::ProgramEndMissing => "program_end_missing",
            GrblEmissionCondition::ProgramEndDuplicate => "program_end_duplicate",
            GrblEmissionCondition::ProgramEndNotLast => "program_end_not_last",
        }
    }

    /// Код класса отказа — ТОТАЛЬНАЯ функция (исчерпывающий match; классы —
    /// правило COVERAGE §5: вне словаря цели → E_UNSUPPORTED_SYNTAX;
    /// нарушение канон-формы AST → E_INVALID_INPUT).
    pub fn code(self) -> ErrorCode {
        use eid_core::diag::ErrorCode::{InvalidInput as INVAL, UnsupportedSyntax as SYN};
        match self {
            GrblEmissionCondition::ToolChange => SYN, // M6 вне словаря GRBL (#27)
            GrblEmissionCondition::WcsOutsideG54G59 => SYN, // G59.x вне словаря GRBL
            GrblEmissionCondition::MotionFeedNone => INVAL, // канон-форма §3/§4
            GrblEmissionCondition::ArcUnknownStart => INVAL, // IJK требует числового старта
            GrblEmissionCondition::ArcNormalNonCanonical => INVAL, // нормаль не +ось (§6)
            GrblEmissionCondition::RawPayloadNotHex => INVAL, // grammar D9 v0.3
            GrblEmissionCondition::ProgramEndMissing => INVAL, // структура (CORPUS n14)
            GrblEmissionCondition::ProgramEndDuplicate => INVAL, // n13
            GrblEmissionCondition::ProgramEndNotLast => INVAL, // n12
        }
    }
}

impl EmissionLegality {
    /// Текст Q3-warning по имени условия; None = не объявлено.
    pub fn warning_of(&self, condition: &str) -> Option<&'static str> {
        self.warnings
            .iter()
            .find(|(c, _)| *c == condition)
            .map(|(_, text)| *text)
    }

    /// Все коды отказа таблицы (без дубликатов).
    pub fn codes(&self) -> Vec<ErrorCode> {
        let mut v: Vec<ErrorCode> = self.illegal.iter().map(|(_, c)| *c).collect();
        v.sort_by_key(|c| c.as_str());
        v.dedup();
        v
    }

    /// Вид опа легален?
    pub fn is_legal_kind(&self, kind: &str) -> bool {
        self.legal.contains(&kind)
    }
}

/// Имя вида опа — ключ таблицы легальности (и словарь manifest r4_expect).
pub fn op_kind(op: &Op) -> &'static str {
    match op {
        Op::Rapid { .. } => "rapid",
        Op::Linear { .. } => "linear",
        Op::Arc { .. } => "arc",
        Op::Dwell { .. } => "dwell",
        Op::ToolChange { .. } => "tool_change",
        Op::ProgramStop { .. } => "program_stop",
        Op::ProgramEnd { .. } => "program_end",
        Op::WcsDefine { .. } => "wcs_define",
        Op::WcsSetHere { .. } => "wcs_set_here",
        Op::Raw { .. } => "raw",
    }
}

/// Эмиссия без warnings-буфера (совместимость; предупреждения теряются).
pub fn emit(program: &Program) -> Result<String, Vec<Diagnostic>> {
    let mut sink = Vec::new();
    emit_with_warnings(program, &mut sink)
}

/// Эмиссия + Q3-warnings (потери метаданных при целевой эмиссии).
pub fn emit_with_warnings(
    program: &Program,
    warnings: &mut Vec<String>,
) -> Result<String, Vec<Diagnostic>> {
    // Каноническая форма до эмиссии (R1: parse(emit(x)) — тот же AST).
    // SPEC §5.2 v0.20: Raw-хвост после program_end легален (grbl-стрим
    // исполняет блоки после M2/M30 — protocol.c:74-105), program_end не
    // forced-last. Замороженный core-инвариант «program_end must be the
    // last op» (eid-core verify.rs — вне зоны адаптера, SPEC §3 как
    // написано) к Raw-хвосту НЕ применяется: диагностика отбрасывается,
    // только когда ВСЕ опы после последнего PEND — Raw (иначе канон-форма
    // нарушена по-настоящему). Строка-фильтр — из замороженного core;
    // если core сменит текст, фильтр перестанет срабатывать →
    // v020_tail_emit_same_dialect_honest_bytes упадёт громко (сигнал к
    // синхронизации с интерфейсом).
    if let Err(mut ds) = eid_core::parser::validate(program) {
        let tail_all_raw = program
            .ops
            .iter()
            .rposition(|op| matches!(op, Op::ProgramEnd { .. }))
            .map(|i| {
                program.ops[i + 1..]
                    .iter()
                    .all(|op| matches!(op, Op::Raw { .. }))
            })
            .unwrap_or(true);
        if tail_all_raw {
            ds.retain(|d| !d.message.contains("program_end must be the last op"));
        }
        if !ds.is_empty() {
            return Err(ds);
        }
    }
    // Magic — ровно "EID/L1 v0.3" (grammar v0.3; SPEC §10.4: v0.2 невалиден).
    if program.eid_version != "EID/L1 v0.3" {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            format!(
                "eid_version `{}` не поддерживается (ожидается EID/L1 v0.3)",
                program.eid_version
            ),
            None,
        )]);
    }
    // Легальность эмиссии — против таблицы (COVERAGE §5), fail-fast до прохода.
    let table = crate::GrblAdapter::new().emission_legality();
    let problems = validate_grbl(&table, program);
    if !problems.is_empty() {
        return Err(problems);
    }

    let mut ctx = Ectx {
        out: String::new(),
        plane: GCode::PlaneXY,
        wcs: Wcs::G54,
        feed: None,
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        first_motion: true,
    };

    // Q3-warning: PATH_MODE не выражается в GRBL-шапке — потеря метаданных
    // фиксируется в warnings-буфере адаптера (не отказ; manifest r4_expect).
    let pm_cond = match program.path_mode {
        eid_core::ast::PathMode::ExactPath => Some("path_mode_exact_path"),
        eid_core::ast::PathMode::ExactStop => Some("path_mode_exact_stop"),
        eid_core::ast::PathMode::Blend { .. } => Some("path_mode_blend"),
    };
    if let Some(text) = pm_cond.and_then(|c| table.warning_of(c)) {
        warnings.push(text.to_string());
    }

    // Заголовок — всегда явно (бриф п.1).
    ctx.line(match program.units {
        Units::Mm => "G21 G90 G94",
        Units::In => "G20 G90 G94",
    });

    // frame_preset — первым блоком после заголовка (EQUIVALENCE §1.4, порядок
    // строгий: выборка preset.wcs, затем G92; без «запекания» в G10).
    if let Some(fp) = &program.frame_preset {
        ctx.line(wcs_code(&fp.wcs));
        ctx.line(&format!(
            "G92 X{} Y{} Z{}",
            fmt_num(fp.values.x.value()),
            fmt_num(fp.values.y.value()),
            fmt_num(fp.values.z.value())
        ));
    }

    // Обратные факторы units-switch (v0.21, N7-снятие): маркеры G20/G21 в
    // Raw-носителях идут в поток verbatim, репарс повторяет конверсию от тех
    // же литералов — координаты восстанавливаются до-конверсными (R1).
    let inv = build_units_inverse(program);
    for (i, op) in program.ops.iter().enumerate() {
        emit_op(&mut ctx, op, i, &inv);
    }
    Ok(ctx.out)
}

// ===========================================================================
// Units-switch anchor: обратная конверсия на эмиссии (v0.21, N7-снятие;
// linuxcnc-зеркало `UnitsInverse`). Парсер при смене системы чтения
// конвертирует ХРАНИМЫЙ end последнего motion-опа
// (canonicalize::convert_units_anchor) — иначе core-verify видит разрыв
// пары `start_{i+1} == end_i` («start != expected», N7/eid verify). Эмиттер
// обязан ВОССТАНОВИТЬ до-конверсные литералы: маркеры G20/G21 идут в потоке
// verbatim (D9), репарс повторяет конверсию от тех же литералов — R1
// round-trip по AST. Последовательности факторов применяются в обратном
// порядке (парсер конвертировал по маркерам вперёд, с 4dp-квантованием на
// каждом шаге — реплей симметричен).
// ===========================================================================

#[derive(Debug, Default)]
struct UnitsInverse {
    /// op индекс → факторы для end (rapid/linear/arc).
    end: std::collections::HashMap<usize, Vec<f64>>,
    /// op индекс → факторы для start+center (arc, §6-геометрия единая).
    geo: std::collections::HashMap<usize, Vec<f64>>,
}

fn is_motion_op(op: &Op) -> bool {
    matches!(op, Op::Rapid { .. } | Op::Linear { .. } | Op::Arc { .. })
}

fn motion_end_vals(op: &Op) -> Option<[f64; 3]> {
    match op {
        Op::Rapid { end, .. } | Op::Linear { end, .. } | Op::Arc { end, .. } => {
            Some([end.x.value(), end.y.value(), end.z.value()])
        }
        _ => None,
    }
}

impl UnitsInverse {
    /// Обратное значение Num: факторы в обратном порядке, 4dp-квантование на
    /// шаге (зеркало parse-конверсии conv_reading/num_f64). Пустой/отсутствующий
    /// список — как есть.
    fn num_at(&self, key: &std::collections::HashMap<usize, Vec<f64>>, idx: usize, n: &Num) -> Num {
        match key.get(&idx) {
            Some(invs) if !invs.is_empty() => Self::apply(invs, n),
            _ => n.clone(),
        }
    }

    fn pt_at(
        &self,
        key: &std::collections::HashMap<usize, Vec<f64>>,
        idx: usize,
        p: &Point3,
    ) -> Point3 {
        Point3 {
            x: self.num_at(key, idx, &p.x),
            y: self.num_at(key, idx, &p.y),
            z: self.num_at(key, idx, &p.z),
        }
    }

    fn apply(invs: &[f64], n: &Num) -> Num {
        let mut v = n.value;
        for g in invs.iter().rev() {
            v = crate::canonicalize::num_f64(v * g).value();
        }
        crate::canonicalize::num_f64(v)
    }
}

/// Система, объявленная в декодированной строке маркера (последний G20/G21
/// в блоке; grbl-словарь через crate::block::parse_file).
fn marker_mode(payload_hex: &str) -> Option<Units> {
    let bytes: Option<Vec<u8>> = (0..payload_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&payload_hex[i..i + 2], 16).ok())
        .collect();
    let line = String::from_utf8(bytes?).ok()?;
    let parsed = crate::block::parse_file(&line);
    parsed.blocks.iter().find_map(|b| {
        b.g_codes.iter().find_map(|g| match g {
            GCode::UnitsIn => Some(Units::In),
            GCode::UnitsMm => Some(Units::Mm),
            _ => None,
        })
    })
}

/// Построение обратных факторов по маркерам UNITS_SWITCH_REASON_PREFIX
/// (зеркало parser-конверсии: якорь = последний motion-оп до маркера,
/// каскад назад по бит-цепочке дуг; порядок маркеров — потоковый).
fn build_units_inverse(program: &Program) -> UnitsInverse {
    let mut inv = UnitsInverse::default();
    let mut mode = program.units;
    for (i, op) in program.ops.iter().enumerate() {
        let Op::Raw {
            payload_hex,
            reason,
            ..
        } = op
        else {
            continue;
        };
        if !reason.contains(crate::canonicalize::UNITS_SWITCH_REASON_PREFIX) {
            continue;
        }
        let Some(new_mode) = marker_mode(payload_hex) else {
            continue;
        };
        if new_mode == mode {
            continue; // no-op на репарсе — конверсии не было (зеркало parse)
        }
        let f = if new_mode == Units::In {
            1.0 / 25.4
        } else {
            25.4
        };
        let g = 1.0 / f;
        // Якорь = последний motion-оп до маркера; каскад назад по бит-цепочке
        // (равенство бит инвариантно к равномерному скейлу — связность та же,
        // что на parse-конверсии convert_units_anchor).
        let mut cur = (0..i).rev().find(|&j| is_motion_op(&program.ops[j]));
        let mut need_end = true;
        while let Some(ci) = cur {
            if need_end {
                inv.end.entry(ci).or_default().push(g);
            }
            let cascade = match &program.ops[ci] {
                Op::Arc {
                    start: StartPoint::Known(p),
                    ..
                } => {
                    inv.geo.entry(ci).or_default().push(g);
                    match ci.checked_sub(1).map(|j| &program.ops[j]) {
                        Some(prev) if is_motion_op(prev) => {
                            matches!(
                                motion_end_vals(prev),
                                Some(e) if e == [p.x.value(), p.y.value(), p.z.value()]
                            )
                        }
                        _ => false,
                    }
                }
                _ => false,
            };
            cur = if cascade { Some(ci - 1) } else { None };
            need_end = true;
        }
        mode = new_mode;
    }
    inv
}

/// Валидация легальности эмиссии против таблицы (fail-fast, COVERAGE §5).
/// Коды отказов берутся ИСКЛЮЧИТЕЛЬНО из таблицы — единый путь отказа.
fn validate_grbl(table: &EmissionLegality, program: &Program) -> Vec<Diagnostic> {
    // v0.2 волна 4 (§2.5 В1): код отказа — тотальный `cond.code()` на enum;
    // таблица — данные для интроспекции/тестов (согласие — тест
    // legality_table_complete). Паник-точек на emit-пути нет.
    let code = |cond: GrblEmissionCondition| cond.code();
    let mut v = Vec::new();
    let mut ends = 0usize;
    let mut end_idx: Option<usize> = None;
    for (i, op) in program.ops.iter().enumerate() {
        if matches!(op, Op::ProgramEnd { .. }) {
            ends += 1;
            end_idx = Some(i);
        }
        match op {
            Op::ToolChange { src, .. } => v.push(err(
                code(GrblEmissionCondition::ToolChange),
                "tool_change: M6 outside GRBL 1.1 dictionary (COVERAGE §5; vector #27 r4_expect)",
                Some(src.clone()),
            )),
            Op::Linear { state, src, .. } => {
                check_motion_state(table, &mut v, state, src, true);
            }
            Op::Arc {
                start,
                normal,
                state,
                src,
                ..
            } => {
                check_motion_state(table, &mut v, state, src, true);
                if matches!(start, StartPoint::Unknown) {
                    v.push(err(
                        code(GrblEmissionCondition::ArcUnknownStart),
                        "arc with unknown start — IJK offsets require numeric start (SPEC §2)",
                        Some(src.clone()),
                    ));
                }
                let n = [normal.x.value(), normal.y.value(), normal.z.value()];
                if let Err(m) = plane_of(&n) {
                    v.push(err(
                        code(GrblEmissionCondition::ArcNormalNonCanonical),
                        m,
                        Some(src.clone()),
                    ));
                }
            }
            Op::Rapid { state, src, .. } => check_motion_state(table, &mut v, state, src, false),
            Op::Raw {
                payload_hex, src, ..
            } => {
                if payload_hex.len() % 2 != 0 || payload_hex.chars().any(|c| !c.is_ascii_hexdigit())
                {
                    v.push(err(
                        code(GrblEmissionCondition::RawPayloadNotHex),
                        "raw payload_hex is not valid byte hex (grammar D9 v0.3)",
                        Some(src.clone()),
                    ));
                }
            }
            Op::Dwell { state, src, .. }
            | Op::WcsDefine { state, src, .. }
            | Op::WcsSetHere { state, src, .. }
            | Op::ProgramStop { state, src, .. } => {
                if wcs_code(&state.wcs).is_empty() {
                    v.push(err(
                        code(GrblEmissionCondition::WcsOutsideG54G59),
                        "state.wcs outside GRBL dictionary (G59.x; COVERAGE §2.3)",
                        Some(src.clone()),
                    ));
                }
            }
            _ => {}
        }
    }
    if ends == 0 {
        v.push(err(
            code(GrblEmissionCondition::ProgramEndMissing),
            "program_end missing: GRBL emission requires M2 (CORPUS n14)",
            None,
        ));
    } else if ends > 1 {
        v.push(err(
            code(GrblEmissionCondition::ProgramEndDuplicate),
            "duplicate program_end (CORPUS n13)",
            None,
        ));
    } else {
        // SPEC §5.2 v0.20: блоки после M2/M30 — Raw-хвост (grbl-стрим
        // исполняет: protocol.c:74-105), program_end НЕ forced-last.
        // Same-dialect эмиссия хранит байты честно (payload verbatim) —
        // хвост из Raw-опов легален. НЕ-Raw оп после PEND — нарушение
        // канон-формы (никакой канонизатор его не строит) → отказ n12.
        let tail_raw = end_idx
            .and_then(|i| program.ops.get(i + 1..))
            .is_none_or(|tail| tail.iter().all(|op| matches!(op, Op::Raw { .. })));
        if !tail_raw {
            v.push(err(
                code(GrblEmissionCondition::ProgramEndNotLast),
                "non-raw op after program_end — canonical form violation (CORPUS n12; SPEC §5.2 v0.20: raw tail is legal)",
                None,
            ));
        }
    }
    v
}

fn check_motion_state(
    _table: &EmissionLegality,
    v: &mut Vec<Diagnostic>,
    state: &State,
    src: &eid_core::ast::SrcSpan,
    need_feed: bool,
) {
    if need_feed && state.feed.is_none() {
        v.push(err(
            GrblEmissionCondition::MotionFeedNone.code(),
            "linear/arc with feed=none — canonical form violation (SPEC §3/§4)",
            Some(src.clone()),
        ));
    }
    if wcs_code(&state.wcs).is_empty() {
        v.push(err(
            GrblEmissionCondition::WcsOutsideG54G59.code(),
            "state.wcs outside GRBL dictionary (G59.x; COVERAGE §2.3)",
            Some(src.clone()),
        ));
    }
}

fn motion_code(op: &Op) -> &'static str {
    match op {
        Op::Rapid { .. } => "G0",
        Op::Linear { .. } => "G1",
        Op::Arc { dir, .. } => match dir {
            eid_core::ast::Dir::Cw => "G2",
            eid_core::ast::Dir::Ccw => "G3",
        },
        _ => "",
    }
}

/// decode_hex — payload валидирован (grammar D9); не-hex уже отбракован.
fn decode_hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(0))
        .collect()
}

fn emit_op(ctx: &mut Ectx, op: &Op, idx: usize, inv: &UnitsInverse) {
    match op {
        Op::Rapid {
            end, state, sub, ..
        }
        | Op::Linear {
            end, state, sub, ..
        } => {
            ctx.state_prefix(state);
            let frame = if state.frame == eid_core::ast::Frame::Machine {
                "G53 "
            } else {
                ""
            };
            // units-switch anchor (v0.21): end хранится в ТЕКУЩЕЙ системе
            // чтения — восстановление до-конверсных литералов (R1).
            let end = inv.pt_at(&inv.end, idx, end);
            let mut line = format!(
                "{}{} X{} Y{} Z{}",
                frame,
                motion_code(op),
                fmt_num(end.x.value()),
                fmt_num(end.y.value()),
                fmt_num(end.z.value())
            );
            // F-дельта ВКЛЮЧАЯ rapid (§7.7-1 развёртки: traverse/approach несут
            // modal feed кадра; "G0 F.." легален в GRBL — устанавливает F).
            // Без F на rapid парс-бэк даёт feed=none ≠ golden (#17–#26).
            // ПЛЮС первый производный оп кадра цикла (sub=1): F переизлучается
            // даже при неизменном значении (SPEC §7.4a v0.18 — зеркало
            // linuxcnc feed_word: rs274 ассертит SET_FEED_RATE на каждом
            // кадре с F-словом; маркер — sub=Some(1), обычные кадры sub=None).
            let fv = state.feed.as_ref().map(|n| n.value());
            if let Some(fv) = fv {
                let changed = Some(fv) != ctx.feed;
                let first_derived = *sub == Some(1);
                // v0.2 волна 3 (SPEC §7.4a): restated-F теперь из AST.
                let restated = state.feed_restated;
                if changed || first_derived || restated {
                    line.push_str(&format!(" F{}", fmt_num(fv)));
                    ctx.feed = Some(fv);
                }
            }
            ctx.line(&line);
            ctx.first_motion = false;
        }
        Op::Arc {
            start,
            end,
            center,
            normal,
            state,
            sweep_deg,
            pitch,
            sub,
            ..
        } => {
            ctx.state_prefix(state);
            // Плоскость из канонической нормали; G17/18/19 при смене (бриф п.1).
            let n = [normal.x.value(), normal.y.value(), normal.z.value()];
            let plane = plane_of(&n).expect("нормаль провалидирована");
            if plane != ctx.plane {
                ctx.line(match plane {
                    GCode::PlaneZX => "G18",
                    GCode::PlaneYZ => "G19",
                    _ => "G17",
                });
                ctx.plane = plane;
            }
            // I/J/K инкрементальные от СТАРТА: offsets = center − start
            // (in-plane пара: G17→I,J; G18→I,K; G19→J,K; R никогда).
            // units-switch anchor (v0.21): center/start дуги — та же
            // геометрия §6, равномерный скейл → обратное восстановление
            // обоих (geo-факторы), offsets инвариантны.
            let s = match start {
                StartPoint::Known(p) => {
                    let p_inv = inv.pt_at(&inv.geo, idx, p);
                    [p_inv.x.value(), p_inv.y.value(), p_inv.z.value()]
                }
                StartPoint::Unknown => [0.0, 0.0, 0.0],
            };
            let c_pt = inv.pt_at(&inv.geo, idx, center);
            let c = [c_pt.x.value(), c_pt.y.value(), c_pt.z.value()];
            let end_inv = inv.pt_at(&inv.end, idx, end);
            let (w0, ax0, w1, ax1) = arc_pair(&plane);
            let offsets = format!(
                " {}{} {}{}",
                w0,
                fmt_num(c[ax0] - s[ax0]),
                w1,
                fmt_num(c[ax1] - s[ax1])
            );
            // Полное кольцо (sweep 360°, pitch 0) — БЕЗ end-слов (легальный
            // формат grbl): end-слова == start перепарсер трактовал бы как
            // вырожденную дугу (CORPUS #13-различитель); pitch ≠ 0 несёт Z.
            let full_circle =
                (sweep_deg.value() - 360.0).abs() < 1e-6 && pitch.value().abs() < 1e-9;
            let target = if full_circle {
                String::new()
            } else {
                format!(
                    " X{} Y{} Z{}",
                    fmt_num(end_inv.x.value()),
                    fmt_num(end_inv.y.value()),
                    fmt_num(end_inv.z.value())
                )
            };
            // target и offsets несут ведущий пробел; полный круг — target
            // пуст → одинарный пробел после G2/G3 (CORPUS #13 regen v1).
            let mut line = format!("{}{}{}", motion_code(op), target, offsets);
            // F при смене значения ИЛИ на первом производном опе кадра цикла
            // (sub=1) — SPEC §7.4a v0.18 (зеркало linuxcnc feed_word);
            // feed=none на дуге невозможен (motion_feed_none в валидации).
            let fv = state.feed.as_ref().map(|num| num.value());
            // v0.2 волна 3: restated-F из AST (зеркало линейного правила).
            if fv != ctx.feed || *sub == Some(1) || state.feed_restated {
                if let Some(fv) = fv {
                    line.push_str(&format!(" F{}", fmt_num(fv)));
                    ctx.feed = Some(fv);
                }
            }
            ctx.line(&line);
            ctx.first_motion = false;
        }
        Op::Dwell { seconds, state, .. } => {
            ctx.state_prefix(state);
            ctx.line(&format!("G4 P{}", fmt_num(seconds.value())));
        }
        Op::WcsDefine {
            p, origin, state, ..
        } => {
            ctx.state_prefix(state);
            ctx.line(&format!(
                "G10 L2 P{} X{} Y{} Z{}",
                p,
                fmt_num(origin.x.value()),
                fmt_num(origin.y.value()),
                fmt_num(origin.z.value())
            ));
        }
        Op::WcsSetHere {
            p, delta, state, ..
        } => {
            ctx.state_prefix(state);
            ctx.line(&format!(
                "G10 L20 P{} X{} Y{} Z{}",
                p,
                fmt_num(delta.x.value()),
                fmt_num(delta.y.value()),
                fmt_num(delta.z.value())
            ));
        }
        Op::ProgramStop {
            optional, state, ..
        } => {
            ctx.state_prefix(state);
            ctx.line(if *optional { "M1" } else { "M0" });
        }
        Op::ProgramEnd { .. } => ctx.line("M2"),
        Op::Raw { payload_hex, .. } => {
            // same-dialect fidelity: исходные байты строки verbatim ВКЛЮЧАЯ
            // терминатор (grammar D9 v0.3: \r\n / \n байт-в-байт). Внешний '\n'
            // не добавляется, если терминатор уже в payload.
            let bytes = decode_hex(payload_hex);
            let text = String::from_utf8_lossy(&bytes).to_string();
            ctx.out.push_str(&text);
            if !text.ends_with('\n') {
                ctx.out.push('\n');
            }
        }
        Op::ToolChange { .. } => unreachable!("tool_change отбракован валидацией"),
    }
}

fn start_pt(s: &StartPoint) -> [f64; 3] {
    match s {
        StartPoint::Known(p) => [p.x.value(), p.y.value(), p.z.value()],
        StartPoint::Unknown => [0.0, 0.0, 0.0],
    }
}
