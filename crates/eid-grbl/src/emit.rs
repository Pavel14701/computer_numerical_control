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

use eid_core::ast::{Coolant, Op, Program, Spindle, StartPoint, State, Units, Wcs};
use eid_core::diag::{Diagnostic, ErrorCode};
use eid_core::emitter::fmt_num;
use crate::gcode::GCode;

/// Плоскость дуги по канонической нормали (SPEC §6: знак +ось; произвольная
/// нормаль — нарушение канонической формы → Err INVALID).
fn plane_of(normal: &[f64; 3]) -> Result<GCode, String> {
    let (x, y, z) = (normal[0], normal[1], normal[2]);
    match (x, y, z) {
        (v, 0.0, 0.0) if (v - 1.0).abs() < 1e-9 => Ok(GCode::PlaneYZ),
        (0.0, v, 0.0) if (v - 1.0).abs() < 1e-9 => Ok(GCode::PlaneZX),
        (0.0, 0.0, v) if (v - 1.0).abs() < 1e-9 => Ok(GCode::PlaneXY),
        _ => Err(format!("arc normal ({x},{y},{z}) is not a canonical +axis (SPEC §6)")),
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
        (Spindle::Cw(x), Spindle::Cw(y)) | (Spindle::Ccw(x), Spindle::Ccw(y)) => x.value() == y.value(),
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
    Diagnostic { code, message: msg.into(), src, options: Vec::new(), limit: None, stage: eid_core::diag::Stage::Emit }
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
    pub illegal: Vec<(&'static str, ErrorCode)>,
}

impl EmissionLegality {
    /// Код отказа по имени условия; None = условие не объявлено.
    pub fn code_of(&self, condition: &str) -> Option<ErrorCode> {
        self.illegal.iter().find(|(c, _)| *c == condition).map(|(_, code)| *code)
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

pub fn emit(program: &Program) -> Result<String, Vec<Diagnostic>> {
    // Каноническая форма до эмиссии (R1: parse(emit(x)) — тот же AST).
    eid_core::parser::validate(program)?;
    // Magic — ровно "EID/L1 v0.3" (grammar v0.3; SPEC §10.4: v0.2 невалиден).
    if program.eid_version != "EID/L1 v0.3" {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            format!("eid_version `{}` не поддерживается (ожидается EID/L1 v0.3)", program.eid_version),
            None,
        )]);
    }
    // Легальность эмиссии — против таблицы (COVERAGE §5), fail-fast до прохода.
    let table = crate::GrblAdapter::default().emission_legality();
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

    for op in &program.ops {
        emit_op(&mut ctx, op);
    }
    Ok(ctx.out)
}

/// Валидация легальности эмиссии против таблицы (fail-fast, COVERAGE §5).
/// Коды отказов берутся ИСКЛЮЧИТЕЛЬНО из таблицы — единый путь отказа.
fn validate_grbl(table: &EmissionLegality, program: &Program) -> Vec<Diagnostic> {
    // Условие обязано быть объявлено в таблице — иначе баг объявления, не паника пути.
    let code = |cond: &'static str| {
        table.code_of(cond).unwrap_or_else(|| {
            panic!("emission-legality: условие `{cond}` не объявлено в таблице")
        })
    };
    let mut v = Vec::new();
    let mut ends = 0usize;
    let mut last_is_end = false;
    for op in &program.ops {
        last_is_end = matches!(op, Op::ProgramEnd { .. });
        if last_is_end {
            ends += 1;
        }
        match op {
            Op::ToolChange { src, .. } => v.push(err(
                code("tool_change"),
                "tool_change: M6 outside GRBL 1.1 dictionary (COVERAGE §5; vector #27 r4_expect)",
                Some(src.clone()),
            )),
            Op::Linear { state, src, .. } => {
                check_motion_state(table, &mut v, state, src, true);
            }
            Op::Arc { start, normal, state, src, .. } => {
                check_motion_state(table, &mut v, state, src, true);
                if matches!(start, StartPoint::Unknown) {
                    v.push(err(
                        code("arc_unknown_start"),
                        "arc with unknown start — IJK offsets require numeric start (SPEC §2)",
                        Some(src.clone()),
                    ));
                }
                let n = [normal.x.value(), normal.y.value(), normal.z.value()];
                if let Err(m) = plane_of(&n) {
                    v.push(err(code("arc_normal_non_canonical"), m, Some(src.clone())));
                }
            }
            Op::Rapid { state, src, .. } => check_motion_state(table, &mut v, state, src, false),
            Op::Raw { payload_hex, src, .. } => {
                if payload_hex.len() % 2 != 0
                    || payload_hex.chars().any(|c| !c.is_ascii_hexdigit())
                {
                    v.push(err(
                        code("raw_payload_not_hex"),
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
                        code("wcs_outside_g54_g59"),
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
            code("program_end_missing"),
            "program_end missing: GRBL emission requires M2 (CORPUS n14)",
            None,
        ));
    } else if ends > 1 {
        v.push(err(code("program_end_duplicate"), "duplicate program_end (CORPUS n13)", None));
    } else if !last_is_end {
        v.push(err(
            code("program_end_not_last"),
            "program_end must be the last op (CORPUS n12)",
            None,
        ));
    }
    v
}

fn check_motion_state(
    table: &EmissionLegality,
    v: &mut Vec<Diagnostic>,
    state: &State,
    src: &eid_core::ast::SrcSpan,
    need_feed: bool,
) {
    if need_feed && state.feed.is_none() {
        v.push(err(
            table
                .code_of("motion_feed_none")
                .expect("emission-legality: условие `motion_feed_none` не объявлено в таблице"),
            "linear/arc with feed=none — canonical form violation (SPEC §3/§4)",
            Some(src.clone()),
        ));
    }
    if wcs_code(&state.wcs).is_empty() {
        v.push(err(
            table
                .code_of("wcs_outside_g54_g59")
                .expect("emission-legality: условие `wcs_outside_g54_g59` не объявлено в таблице"),
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

fn emit_op(ctx: &mut Ectx, op: &Op) {
    match op {
        Op::Rapid { end, state, .. } | Op::Linear { end, state, .. } => {
            let rapid = matches!(op, Op::Rapid { .. });
            ctx.state_prefix(state);
            let frame = if state.frame == eid_core::ast::Frame::Machine { "G53 " } else { "" };
            let mut line = format!(
                "{}{} X{} Y{} Z{}",
                frame,
                motion_code(op),
                fmt_num(end.x.value()),
                fmt_num(end.y.value()),
                fmt_num(end.z.value())
            );
            if !rapid {
                // F на linear/arc при изменении feed от предыдущего emitted-state
                // (модальный F; rapid без F — бриф п.4).
                let fv = state.feed.as_ref().map(|n| n.value());
                if fv != ctx.feed {
                    line.push_str(&format!(" F{}", fmt_num(fv.unwrap())));
                    ctx.feed = fv;
                }
            }
            ctx.line(&line);
            ctx.first_motion = false;
        }
        Op::Arc { start, end, center, normal, state, sweep_deg, pitch, .. } => {
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
            let s = start_pt(start);
            let c = [center.x.value(), center.y.value(), center.z.value()];
            let (w0, ax0, w1, ax1) = arc_pair(&plane);
            let offsets = format!(" {}{} {}{}", w0, fmt_num(c[ax0] - s[ax0]), w1, fmt_num(c[ax1] - s[ax1]));
            // Полное кольцо (sweep 360°, pitch 0) — БЕЗ end-слов (легальный
            // формат grbl): end-слова == start перепарсер трактовал бы как
            // вырожденную дугу (CORPUS #13-различитель); pitch ≠ 0 несёт Z.
            let full_circle = (sweep_deg.value() - 360.0).abs() < 1e-6 && pitch.value().abs() < 1e-9;
            let target = if full_circle {
                String::new()
            } else {
                format!(
                    " X{} Y{} Z{}",
                    fmt_num(end.x.value()),
                    fmt_num(end.y.value()),
                    fmt_num(end.z.value())
                )
            };
            let mut line = format!("{}{} {}", motion_code(op), target, offsets);
            let fv = state.feed.as_ref().map(|num| num.value());
            if fv != ctx.feed {
                line.push_str(&format!(" F{}", fmt_num(fv.unwrap())));
                ctx.feed = fv;
            }
            ctx.line(&line);
            ctx.first_motion = false;
        }
        Op::Dwell { seconds, state, .. } => {
            ctx.state_prefix(state);
            ctx.line(&format!("G4 P{}", fmt_num(seconds.value())));
        }
        Op::WcsDefine { p, origin, state, .. } => {
            ctx.state_prefix(state);
            ctx.line(&format!(
                "G10 L2 P{} X{} Y{} Z{}",
                p,
                fmt_num(origin.x.value()),
                fmt_num(origin.y.value()),
                fmt_num(origin.z.value())
            ));
        }
        Op::WcsSetHere { p, delta, state, .. } => {
            ctx.state_prefix(state);
            ctx.line(&format!(
                "G10 L20 P{} X{} Y{} Z{}",
                p,
                fmt_num(delta.x.value()),
                fmt_num(delta.y.value()),
                fmt_num(delta.z.value())
            ));
        }
        Op::ProgramStop { optional, state, .. } => {
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
