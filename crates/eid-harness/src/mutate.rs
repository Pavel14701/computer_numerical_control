//! mutate.rs — операторы M1–M17 (CORPUS §2.1): валидный AST/текст →
//! мутированная программа + ожидаемая диспозиция (spека: Ok / notice /
//! Err-код; AnyErr — допустимое семейство кодов при расхождении уровней
//! канонизатора/диалекта). Диалект-уровневые операторы (M8/M9/M10/M15)
//! применяются к linuxcnc-тексту (apply_dialect), текстовые (M12/M17) —
//! к eid-тексту (apply_text).

use eid_core::ast::*;
use eid_core::diag::{Diagnostic, ErrorCode};
use eid_core::parser;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Expect {
    /// parse Ok, без notice.
    Ok,
    /// parse Ok + drop-with-notice.
    Notice,
    /// parse Err с этим кодом.
    Err(ErrorCode),
    /// parse Err с любым из кодов семейства (уровневые расхождения
    /// канонизатора/диалекта — классификация §9).
    AnyErr(&'static [ErrorCode]),
    /// Ok ИЛИ emit-отказ E_INVALID_INPUT (Verify-этап) — ruling refinement:
    /// инварианты Verify работают, отказ = подтверждение, не находка (M1/M5/M7/M11/M13/M14).
    OkOrVerifyErr,
}

impl Expect {
    /// Сверка фактического исхода eid-core parse (Ok+notices / Err-коды).
    pub fn check_eid(&self, result: &Result<parser::ParseOutcome, Vec<Diagnostic>>) -> bool {
        match result {
            Err(diags) => match self {
                Expect::Err(c) => diags.iter().any(|d| d.code == *c),
                Expect::AnyErr(cs) => diags.iter().any(|d| cs.contains(&d.code)),
                _ => false,
            },
            Ok(out) => match self {
                Expect::Ok | Expect::OkOrVerifyErr => true,
                Expect::Notice => !out.notices.is_empty(),
                Expect::Err(_) | Expect::AnyErr(_) => false,
            },
        }
    }

    /// Сверка фактического исхода диалектного адаптера (без notices).
    pub fn check_dialect(&self, result: &Result<Program, Vec<Diagnostic>>) -> bool {
        match result {
            Err(diags) => match self {
                Expect::Err(c) => diags.iter().any(|d| d.code == *c),
                Expect::AnyErr(cs) => diags.iter().any(|d| cs.contains(&d.code)),
                _ => false,
            },
            Ok(_) => matches!(self, Expect::Ok | Expect::OkOrVerifyErr),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Expect::Ok => "Ok".into(),
            Expect::Notice => "Ok + notice".into(),
            Expect::Err(c) => format!("Err({})", c.as_str()),
            Expect::AnyErr(cs) => {
                format!(
                    "Err({})",
                    cs.iter().map(|c| c.as_str()).collect::<Vec<_>>().join("|")
                )
            }
            Expect::OkOrVerifyErr => "Ok или Verify-отказ E_INVALID_INPUT".into(),
        }
    }
}

pub const OPERATORS: [&str; 17] = [
    "M1", "M2", "M3", "M4", "M5", "M6", "M7", "M8", "M9", "M10", "M11", "M12", "M13", "M14", "M15",
    "M16", "M17",
];

/// Первый arc.
fn first_arc(p: &mut Program) -> Option<&mut Op> {
    p.ops.iter_mut().find(|o| matches!(o, Op::Arc { .. }))
}

fn num(v: f64) -> Num {
    Num::new(format!("{v:.4}"), v)
}

/// AST-уровневые операторы. None — неприменимо (нет дуг/линейных и т.п.).
pub fn apply(id: &str, p: &Program) -> Option<(Program, Expect)> {
    let mut p = p.clone();
    let expect = match id {
        "M1" => {
            // sweep→0+ε при start==end → drop+notice (SPEC §3/§6, #13)
            let a = first_arc(&mut p)?;
            if let Op::Arc {
                end,
                start,
                sweep_deg,
                pitch,
                ..
            } = a
            {
                if let StartPoint::Known(s) = start {
                    *end = s.clone();
                }
                *sweep_deg = num(0.0001);
                *pitch = num(0.0);
            }
            Expect::OkOrVerifyErr
        }
        "M2" => {
            // sweep→360 при start==end → полный круг (SPEC §6).
            // Если дуга связанная (не первая), end := start ломает цепочку
            // последующих опов → легитимный Verify-отказ (инвариант §4
            // связывания работает) — мутант-зонд инварианта, не баг (ruling).
            let a = first_arc(&mut p)?;
            if let Op::Arc {
                end,
                start,
                sweep_deg,
                ..
            } = a
            {
                if let StartPoint::Known(s) = start {
                    *end = s.clone();
                }
                *sweep_deg = num(360.0);
            }
            Expect::OkOrVerifyErr
        }
        "M3" => {
            // sweep→near-180 с пересчитанным центром (амплификация, #11)
            let sx = match p.ops.iter().find_map(|o| match o {
                Op::Arc { start, .. } => Some(start.clone()),
                _ => None,
            }) {
                Some(StartPoint::Known(s)) => s.clone(),
                _ => return None,
            };
            let a = first_arc(&mut p)?;
            if let Op::Arc {
                end,
                center,
                dir,
                sweep_deg,
                ..
            } = a
            {
                let (cx, cy) = crate::generator::arc_center(
                    sx.x.value(),
                    sx.y.value(),
                    end.x.value(),
                    end.y.value(),
                    179.9,
                    *dir,
                );
                *center = Point3 {
                    x: num(cx),
                    y: num(cy),
                    z: sx.z.clone(),
                };
                *sweep_deg = num(179.9);
            }
            Expect::Ok
        }
        "M4" => {
            // R-form near-degenerate: sweep в cap-зоне ±0.12° вокруг 180°
            let a = first_arc(&mut p)?;
            if let Op::Arc {
                start,
                end,
                center,
                dir,
                sweep_deg,
                ..
            } = a
            {
                if let StartPoint::Known(s) = start {
                    let (cx, cy) = crate::generator::arc_center(
                        s.x.value(),
                        s.y.value(),
                        end.x.value(),
                        end.y.value(),
                        179.88,
                        *dir,
                    );
                    *center = Point3 {
                        x: num(cx),
                        y: num(cy),
                        z: s.z.clone(),
                    };
                }
                *sweep_deg = num(179.88);
            }
            Expect::Ok
        }
        "M5" => {
            // feed→none на linear/arc → Err(E_INVALID_INPUT) (SPEC §4)
            let lin = p.ops.iter_mut().find(|o| matches!(o, Op::Linear { .. }))?;
            if let Op::Linear { state, .. } = lin {
                state.feed = None;
            }
            Expect::OkOrVerifyErr
        }
        "M6" => {
            // feed re-set: смена F' на первом линейном ходе → Ok (modal)
            let lin = p.ops.iter_mut().find(|o| matches!(o, Op::Linear { .. }))?;
            if let Op::Linear { state, .. } = lin {
                state.feed = state.feed.take().map(|f| num(f.value() * 1.5 + 7.0));
            }
            Expect::Ok
        }
        "M7" => {
            // unknown-start инъекция только на СВЯЗАННОМ ходе (refinement):
            // пара соседних motion-опов (frame=Wcs, одна WCS, обе Known) —
            // связывание обязывает Known; Unknown здесь = Err(E_INVALID_INPUT).
            // В точках разрыва (a)-(d) Unknown легален — инъекция туда = шум.
            let mut pair: Option<usize> = None;
            let mut prev: Option<(Frame, Wcs)> = None;
            for (idx, o) in p.ops.iter().enumerate() {
                let (frame, wcs, known) = match o {
                    Op::Rapid { start, state, .. }
                    | Op::Linear { start, state, .. }
                    | Op::Arc { start, state, .. } => (
                        state.frame,
                        state.wcs,
                        matches!(start, StartPoint::Known(_)),
                    ),
                    _ => continue,
                };
                if let Some((pf, pw)) = prev {
                    if known && frame == Frame::Wcs && pf == Frame::Wcs && pw == wcs {
                        pair = Some(idx);
                        break;
                    }
                }
                prev = Some((frame, wcs));
            }
            let idx = pair?;
            match &mut p.ops[idx] {
                Op::Rapid { start, .. } | Op::Linear { start, .. } | Op::Arc { start, .. } => {
                    *start = StartPoint::Unknown
                }
                _ => {}
            }
            Expect::OkOrVerifyErr
        }
        "M11" => {
            // zero-length + пресетка: L20-якорь + linear start==end → notice
            let i_motion = p
                .ops
                .iter()
                .position(|o| matches!(o, Op::Rapid { .. } | Op::Linear { .. } | Op::Arc { .. }))?;
            let wcs = match p.ops.first() {
                Some(Op::Rapid { state, .. }) => state.wcs,
                _ => Wcs::G54,
            };
            p.ops.insert(
                i_motion,
                Op::WcsSetHere {
                    p: 1,
                    delta: Point3 {
                        x: num(0.0),
                        y: num(0.0),
                        z: num(5.0),
                    },
                    state: State {
                        feed: None,
                        spindle: Spindle::Off,
                        coolant: Coolant::default(),
                        tool: None,
                        frame: Frame::Wcs,
                        wcs,
                    },
                    src: SrcSpan {
                        file: "m11".into(),
                        line_start: 0,
                        col_start: 1,
                        line_end: 0,
                        col_end: 0,
                    },
                },
            );
            // сделать первый motion zero-length (linear)
            let after = i_motion + 1;
            if matches!(p.ops.get(after), Some(Op::Rapid { .. })) {
                let (start, end) = match &p.ops[after] {
                    Op::Rapid { start, end, .. } => (start.clone(), end.clone()),
                    _ => unreachable!(),
                };
                p.ops[after] = Op::Linear {
                    start,
                    end,
                    state: State {
                        feed: Some(num(150.0)),
                        spindle: Spindle::Cw(num(8000.0)),
                        coolant: Coolant::default(),
                        tool: None,
                        frame: Frame::Wcs,
                        wcs,
                    },
                    src: SrcSpan {
                        file: "m11".into(),
                        line_start: 0,
                        col_start: 1,
                        line_end: 0,
                        col_end: 0,
                    },
                    sub: None,
                };
            }
            Expect::OkOrVerifyErr
        }
        "M13" => {
            // plane orphan: нормаль чужой плоскости → радиус-валидация §6.1
            let a = first_arc(&mut p)?;
            if let Op::Arc { normal, .. } = a {
                *normal = Point3 {
                    x: num(0.0),
                    y: num(1.0),
                    z: num(0.0),
                };
            }
            Expect::OkOrVerifyErr
        }
        "M14" => {
            // centers swap: центр сдвинут на 2·вектор (инкрементальное прочтение)
            // → невозможная дуга (ловушка #39/#48)
            let sx = match p.ops.iter().find_map(|o| match o {
                Op::Arc { start, .. } => Some(start.clone()),
                _ => None,
            }) {
                Some(StartPoint::Known(s)) => s.x.value(),
                _ => return None,
            };
            let a = first_arc(&mut p)?;
            if let Op::Arc { center, .. } = a {
                let cx = center.x.value();
                center.x = num(sx + 2.0 * (cx - sx));
            }
            Expect::OkOrVerifyErr
        }
        "M16" => {
            // M1-инъекция: STOP optional=yes после первого опа → Ok (Q8)
            let at = 1.min(p.ops.len());
            p.ops.insert(
                at,
                Op::ProgramStop {
                    optional: true,
                    state: State {
                        feed: None,
                        spindle: Spindle::Off,
                        coolant: Coolant::default(),
                        tool: None,
                        frame: Frame::Wcs,
                        wcs: Wcs::G54,
                    },
                    src: SrcSpan {
                        file: "m16".into(),
                        line_start: 0,
                        col_start: 1,
                        line_end: 0,
                        col_end: 0,
                    },
                },
            );
            Expect::Ok
        }
        _ => return None, // M8/M9/M10/M15 — диалект-уровневые (apply_dialect); M12/M17 — текстовые
    };
    Some((p, expect))
}

/// Диалект-уровневые операторы: linuxcnc-текст → мутированный текст + ожидание.
pub fn apply_dialect(id: &str, text: &str) -> Option<(String, Expect)> {
    let mut t = text.to_string();
    let expect = match id {
        "M8" => {
            // units-смена mid: G20 после первого хода
            let i = t
                .lines()
                .position(|l| l.starts_with("G0 ") || l.starts_with("G1 "))?;
            let mut lines: Vec<&str> = t.lines().collect();
            lines.insert(i + 1, "G20");
            t = lines.join("\n");
            Expect::AnyErr(&[ErrorCode::UnsupportedSemantics, ErrorCode::InvalidInput])
        }
        "M9" => {
            // G92 mid: после первого хода
            let i = t
                .lines()
                .position(|l| l.starts_with("G0 ") || l.starts_with("G1 "))?;
            let mut lines: Vec<&str> = t.lines().collect();
            lines.insert(i + 1, "G92 X0.0000 Y0.0000 Z0.0000");
            t = lines.join("\n");
            Expect::AnyErr(&[ErrorCode::UnsupportedSemantics, ErrorCode::InvalidInput])
        }
        "M10" => {
            // double G92: два подряд в начале
            let i = t
                .lines()
                .position(|l| l.starts_with("M3") || l.starts_with("G0 "))?;
            let mut lines: Vec<&str> = t.lines().collect();
            lines.insert(i, "G92 X0.0000 Y0.0000 Z0.0000");
            lines.insert(i + 1, "G92 X1.0000 Y1.0000 Z1.0000");
            t = lines.join("\n");
            Expect::AnyErr(&[ErrorCode::UnsupportedSemantics, ErrorCode::InvalidInput])
        }
        "M15" => {
            // wcs-switch без origin + частичный end → Err(E_RUNTIME_STATE)
            let i = t.lines().position(|l| l.starts_with("G0 "))?;
            let mut lines: Vec<&str> = t.lines().collect();
            lines.insert(i + 1, "G55");
            lines.insert(i + 2, "G1 X20.0000 F150.0");
            t = lines.join("\n");
            Expect::Err(ErrorCode::RuntimeState)
        }
        _ => return None,
    };
    Some((t, expect))
}

/// Текстовые операторы: eid-текст → мутированный текст + ожидание.
pub fn apply_text(id: &str, text: &str) -> Option<(String, Expect)> {
    let mut t = text.to_string();
    let expect = match id {
        "M12" => {
            // CRLF-микс: первый перевод строки после заголовка -> \r\n
            let nl = t.find('\n')?;
            let nl2 = t[nl + 1..].find('\n')? + nl + 1;
            t.insert(nl2, '\r');
            Expect::Ok
        }
        "M17" => {
            // raw-payload терминатор: hex payload += "0A" (LF-терминатор)
            let i = t.find("payload=")?;
            let hex_start = i + "payload=".len();
            let hex_end = t[hex_start..]
                .find(|c: char| !(c.is_ascii_digit() || ('A'..='F').contains(&c)))
                .map(|k| hex_start + k)
                .unwrap_or(t.len());
            t.insert_str(hex_end, "0A");
            Expect::Ok
        }
        _ => return None,
    };
    Some((t, expect))
}
