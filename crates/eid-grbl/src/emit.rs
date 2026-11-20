//! Эмиттер GRBL 1.1 (Агент C, итерация 3) — EID/L1 AST → текст GRBL.
//! Правила: COVERAGE §5 (дуги — in-plane пары IJK; tool_change → SYNTAX;
//! G92 первым блоком после выборки preset.wcs — EQUIVALENCE §1.4, без
//! «запекания» в G10), R-форма не эмитится никогда (EQUIVALENCE §4).
//! Числа — eid_core::emitter::fmt_num: фиксированные 4 dp, trailing zeros
//! сохраняются (EQUIVALENCE §1.4: два адаптера на одном EID — идентичный блок).
//!
//! RAW: payload-hex декодируется в исходные байты → verbatim (same-dialect
//! fidelity, CRLF байт-в-байт). КРОСС-ДИАЛЕКТНЫЙ ОТКАЗ E_RAW_UNTRANSLATABLE —
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
            let prev = self.coolant;
            let new = st.coolant;
            if new.mist == prev.mist && new.flood != prev.flood {
                if new.flood {
                    self.out.push_str("M8\n");
                } else {
                    self.out.push_str("M9\n");
                }
            } else if new.mist != prev.mist || new.flood != prev.flood {
                // Частичное выключение одной из двух выключается только M9
                // (гасит обе) с повторным включением оставшейся.
                if prev.flood && !new.flood && new.mist {
                    self.out.push_str("M9\nM8\n"); // флад гаснет→нет… (mist-единств.)
                } else {
                    if new.mist && !prev.mist {
                        self.out.push_str("M7\n");
                    }
                    if new.flood && !prev.flood {
                        self.out.push_str("M8\n");
                    }
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
    Diagnostic { code, message: msg.into(), src, options: Vec::new(), limit: None }
}

pub fn emit(program: &Program) -> Result<String, Vec<Diagnostic>> {
    // Каноническая форма до эмиссии (R1: parse(emit(x)) — тот же AST).
    eid_core::parser::validate(program)?;
    if program.eid_version != "EID/L1 v0.2" {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            format!("eid_version `{}` не поддерживается (ожидается EID/L1 v0.2)", program.eid_version),
            None,
        )]);
    }
    // GRBL-специфичная валидация (COVERAGE §5) — fail-fast до прохода.
    let problems = validate_grbl(program);
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

/// GRBL-специфичная валидация канонической формы (fail-fast, COVERAGE §5).
fn validate_grbl(program: &Program) -> Vec<Diagnostic> {
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
                ErrorCode::UnsupportedSyntax,
                "tool_change: M6 outside GRBL 1.1 dictionary (COVERAGE §5; vector #27 r4_expect)",
                Some(src.clone()),
            )),
            Op::Linear { state, src, .. } => {
                check_motion_state(&mut v, state, src, true);
            }
            Op::Arc { start, normal, state, src, .. } => {
                check_motion_state(&mut v, state, src, true);
                if matches!(start, StartPoint::Unknown) {
                    v.push(err(
                        ErrorCode::InvalidInput,
                        "arc with unknown start — IJK offsets require numeric start (SPEC §2)",
                        Some(src.clone()),
                    ));
                }
                let n = [normal.x.value(), normal.y.value(), normal.z.value()];
                if let Err(m) = plane_of(&n) {
                    v.push(err(ErrorCode::InvalidInput, m, Some(src.clone())));
                }
            }
            Op::Rapid { state, src, .. } => check_motion_state(&mut v, state, src, false),
            Op::Raw { payload_hex, src, .. } => {
                if payload_hex.len() % 2 != 0
                    || payload_hex.chars().any(|c| !c.is_ascii_hexdigit())
                {
                    v.push(err(
                        ErrorCode::InvalidInput,
                        "raw payload_hex is not valid byte hex (grammar D9)",
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
                        ErrorCode::UnsupportedSyntax,
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
            ErrorCode::InvalidInput,
            "program_end missing: GRBL emission requires M2 (CORPUS n14)",
            None,
        ));
    } else if ends > 1 {
        v.push(err(ErrorCode::InvalidInput, "duplicate program_end (CORPUS n13)", None));
    } else if !last_is_end {
        v.push(err(ErrorCode::InvalidInput, "program_end must be the last op (CORPUS n12)", None));
    }
    v
}

fn check_motion_state(
    v: &mut Vec<Diagnostic>,
    state: &State,
    src: &eid_core::ast::SrcSpan,
    need_feed: bool,
) {
    if need_feed && state.feed.is_none() {
        v.push(err(
            ErrorCode::InvalidInput,
            "linear/arc with feed=none — canonical form violation (SPEC §3/§4)",
            Some(src.clone()),
        ));
    }
    if wcs_code(&state.wcs).is_empty() {
        v.push(err(
            ErrorCode::UnsupportedSyntax,
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
        Op::Arc { start, end, center, normal, state, .. } => {
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
            let mut line = format!(
                "{} X{} Y{} Z{} {}{} {}{}",
                motion_code(op),
                fmt_num(end.x.value()),
                fmt_num(end.y.value()),
                fmt_num(end.z.value()),
                w0,
                fmt_num(c[ax0] - s[ax0]),
                w1,
                fmt_num(c[ax1] - s[ax1])
            );
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
            // same-dialect fidelity: исходные байты строки verbatim (CRLF —
            // байт-в-байт; hex — grammar D9).
            let bytes = decode_hex(payload_hex);
            ctx.out.push_str(&String::from_utf8_lossy(&bytes));
            ctx.out.push('\n');
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
