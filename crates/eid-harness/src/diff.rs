//! semantic_diff — замороженный интерфейс v0.1.
//! Допуски — EQUIVALENCE §4 (единственный источник коэффициентов; меняются
//! только ревизией контракта, не кодом и не профилем).

use eid_core::ast::{FramePreset, Op, PathMode, Point3, Program, Spindle, StartPoint, State};

#[derive(Debug, Clone)]
pub struct DiffConfig {
    pub eps_abs: f64,   // 0.001 — floor от 4dp-квантования ×10
    pub k_pos: f64,     // 1e-7 (относительный член, включается при L > 10 м)
    pub q_arc: f64,     // 1e-4 (шаг 4dp источника — член амплификации центра)
    pub a_cap: f64,     // 1000 — cap 1/|cos(sweep/2)| (ревью §4; cos, не sin)
    pub k_r: f64,       // 1e-6
    pub feed_rel: f64,  // 1e-5
    pub rpm_rel: f64,   // 1e-6
    pub dwell_rel: f64, // 1e-5
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            eps_abs: 0.001,
            k_pos: 1e-7,
            q_arc: 1e-4,
            a_cap: 1000.0,
            k_r: 1e-6,
            feed_rel: 1e-5,
            rpm_rel: 1e-6,
            dwell_rel: 1e-5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Divergence {
    OpCount { a: usize, b: usize },
    OpKind { index: usize },
    Geometry { index: usize, field: &'static str },
    StateField { index: usize, field: &'static str },
    MetaField { field: &'static str },
    /// unknown vs known — расхождение (EQUIVALENCE §3 v0.2); R3 красный.
    UnknownVsKnown { index: usize },
    RawMismatch { index: usize },
}

/// L2-дифф на composed-фрейме (EQUIVALENCE §1.3, §3). Пустой вектор = эквивалентны.
/// Вне эквивалентности: src/sub (provenance), аннотации, формат чисел, M1, M2/M30,
/// физические WCS origins (EQUIVALENCE §5).
///
/// Правила:
///  * `ProgramStop { optional: true }` (M1) считается отсутствующим (Q8) — оба
///    списка фильтруются до сравнения; `index` в [`Divergence`] — индекс в
///    отфильтрованном списке;
///  * сравнение посортное; при разной длине — `OpCount` + сравнение общего
///    префикса;
///  * числа — на f64-зеркале (EQUIVALENCE §4), verbatim-текст не сравнивается;
///  * `k_r` (радиус, EQUIVALENCE §4) в диффе не участвует: радиус — производная
///    start/center, и «радиус из центра» дал бы ложносрабатывание на легальных
///    околополуокружностях (допуск центра q·A легализует разницу центров ~0.1,
///    которая в радиус не транслируется — δR входит в центр, делённый на cos).
///    Радиусная валидация — парсер (SPEC §6.1);
///  * normal/dir — часть полной спецификации дуги (EQUIVALENCE §3).
pub fn semantic_diff(a: &Program, b: &Program, cfg: &DiffConfig) -> Vec<Divergence> {
    let mut d = Vec::new();

    // --- мета: units, path_mode, frame_preset (EQUIVALENCE §3.1) -------------
    if a.units != b.units {
        d.push(Divergence::MetaField { field: "units" });
    }
    diff_path_mode(&a.path_mode, &b.path_mode, cfg, &mut d);
    let eps = eps_pos(a, b, cfg);
    diff_frame_preset(a.frame_preset.as_ref(), b.frame_preset.as_ref(), eps, &mut d);

    // --- опы: M1-фильтрация, посортное сравнение ------------------------------
    let fa = filter_m1(&a.ops);
    let fb = filter_m1(&b.ops);
    if fa.len() != fb.len() {
        d.push(Divergence::OpCount { a: fa.len(), b: fb.len() });
    }
    for (i, (oa, ob)) in fa.iter().zip(fb.iter()).enumerate() {
        diff_op(oa, ob, i, cfg, eps, &mut d);
    }
    d
}

pub fn asts_equal(a: &Program, b: &Program, cfg: &DiffConfig) -> bool {
    semantic_diff(a, b, cfg).is_empty()
}

// ===========================================================================
// Мета
// ===========================================================================

/// path_mode: kind-равенство (ExactPath/ExactStop/Blend — задача 2); Blend t —
/// допуск позиции (ε_abs — метаполе вне геометрии программы).
fn diff_path_mode(a: &PathMode, b: &PathMode, cfg: &DiffConfig, d: &mut Vec<Divergence>) {
    match (a, b) {
        (PathMode::Blend { t: ta }, PathMode::Blend { t: tb }) => match (ta, tb) {
            (None, None) => {}
            (Some(x), Some(y)) => {
                if !num_close(x.value(), y.value(), cfg.eps_abs) {
                    d.push(Divergence::MetaField { field: "path_mode.t" });
                }
            }
            _ => d.push(Divergence::MetaField { field: "path_mode" }),
        },
        _ if std::mem::discriminant(a) != std::mem::discriminant(b) => {
            d.push(Divergence::MetaField { field: "path_mode" })
        }
        _ => {}
    }
}

/// frame_preset: сравнивается как узел (EQUIVALENCE §1.3.4) — values с допуском
/// позиции, wcs — символ.
fn diff_frame_preset(
    a: Option<&FramePreset>,
    b: Option<&FramePreset>,
    eps: f64,
    d: &mut Vec<Divergence>,
) {
    match (a, b) {
        (None, None) => {}
        (Some(_), None) | (None, Some(_)) => {
            d.push(Divergence::MetaField { field: "frame_preset" })
        }
        (Some(x), Some(y)) => {
            if x.wcs != y.wcs {
                d.push(Divergence::MetaField { field: "frame_preset.wcs" });
            }
            if !p3_close(&x.values, &y.values, eps) {
                d.push(Divergence::MetaField { field: "frame_preset.values" });
            }
        }
    }
}

// ===========================================================================
// Опы
// ===========================================================================

/// M1 ≡ отсутствие (Q8, EQUIVALENCE §5): optional-stop выфильтровывается из
/// обоих списков до сравнения.
fn filter_m1(ops: &[Op]) -> Vec<&Op> {
    ops.iter()
        .filter(|o| !matches!(o, Op::ProgramStop { optional: true, .. }))
        .collect()
}

fn diff_op(oa: &Op, ob: &Op, i: usize, cfg: &DiffConfig, eps: f64, d: &mut Vec<Divergence>) {
    if std::mem::discriminant(oa) != std::mem::discriminant(ob) {
        d.push(Divergence::OpKind { index: i });
        return;
    }
    match (oa, ob) {
        (Op::Rapid { .. }, Op::Rapid { .. }) | (Op::Linear { .. }, Op::Linear { .. }) => {
            diff_motion(i, oa, ob, cfg, eps, d);
        }
        (Op::Arc { .. }, Op::Arc { .. }) => diff_arc(i, oa, ob, cfg, eps, d),
        (
            Op::Dwell { seconds: sa, state: st_a, .. },
            Op::Dwell { seconds: sb, state: st_b, .. },
        ) => {
            // dwell: max(0.0001, dwell_rel·t) (EQUIVALENCE §4)
            let tol = 1e-4_f64.max(cfg.dwell_rel * sa.value().abs().max(sb.value().abs()));
            if !num_close(sa.value(), sb.value(), tol) {
                d.push(Divergence::Geometry { index: i, field: "seconds" });
            }
            diff_state(i, st_a, st_b, cfg, d);
        }
        (
            Op::ToolChange { tool: ta, state: st_a, .. },
            Op::ToolChange { tool: tb, state: st_b, .. },
        ) => {
            if ta != tb {
                d.push(Divergence::StateField { index: i, field: "tool" });
            }
            diff_state(i, st_a, st_b, cfg, d);
        }
        (
            Op::ProgramStop { optional: oa_, state: st_a, .. },
            Op::ProgramStop { optional: ob_, state: st_b, .. },
        ) => {
            // M1 отфильтрован; здесь оба optional=false (M0) — иначе OpKind.
            if oa_ != ob_ {
                d.push(Divergence::StateField { index: i, field: "optional" });
            }
            diff_state(i, st_a, st_b, cfg, d);
        }
        (Op::ProgramEnd { .. }, Op::ProgramEnd { .. }) => {}
        (
            Op::WcsDefine { p: pa, origin: ga, state: st_a, .. },
            Op::WcsDefine { p: pb, origin: gb, state: st_b, .. },
        ) => {
            if pa != pb {
                d.push(Divergence::StateField { index: i, field: "p" });
            }
            if !p3_close(ga, gb, eps) {
                d.push(Divergence::Geometry { index: i, field: "origin" });
            }
            diff_state(i, st_a, st_b, cfg, d);
        }
        (
            Op::WcsSetHere { p: pa, delta: ga, state: st_a, .. },
            Op::WcsSetHere { p: pb, delta: gb, state: st_b, .. },
        ) => {
            if pa != pb {
                d.push(Divergence::StateField { index: i, field: "p" });
            }
            if !p3_close(ga, gb, eps) {
                d.push(Divergence::Geometry { index: i, field: "delta" });
            }
            diff_state(i, st_a, st_b, cfg, d);
        }
        (
            Op::Raw { payload_hex: ha, .. },
            Op::Raw { payload_hex: hb, .. },
        ) => {
            // RAW — payload_hex точное равенство (EQUIVALENCE §3, D9 lossless).
            if ha != hb {
                d.push(Divergence::RawMismatch { index: i });
            }
            // reason/code — provenance диагностики; payload — семантика.
        }
        // Одинаковые discriminant'ы покрыты выше.
        _ => unreachable!("op variants covered"),
    }
}

fn diff_motion(i: usize, oa: &Op, ob: &Op, cfg: &DiffConfig, eps: f64, d: &mut Vec<Divergence>) {
    let (sa, ea, st_a) = motion_parts(oa);
    let (sb, eb, st_b) = motion_parts(ob);
    diff_start(i, sa, sb, eps, d);
    if !p3_close(ea, eb, eps) {
        d.push(Divergence::Geometry { index: i, field: "end" });
    }
    diff_state(i, st_a, st_b, cfg, d);
}

fn diff_arc(i: usize, oa: &Op, ob: &Op, cfg: &DiffConfig, eps: f64, d: &mut Vec<Divergence>) {
    let (
        Op::Arc { start: sa, end: ea, center: ca, normal: na, dir: da, sweep_deg: swa, pitch: pia, state: st_a, .. },
        Op::Arc { start: sb, end: eb, center: cb, normal: nb, dir: db, sweep_deg: swb, pitch: pib, state: st_b, .. },
    ) = (oa, ob) else { unreachable!("arc pair") };

    diff_start(i, sa, sb, eps, d);
    if !p3_close(ea, eb, eps) {
        d.push(Divergence::Geometry { index: i, field: "end" });
    }
    // center: ε = max(ε_pos, q·A), A = 1/max(|cos(sweep/2)|, 1e-3), cap a_cap
    // (EQUIVALENCE §4; cos, не sin — ревью §4).
    let sweep_max = swa.value().abs().max(swb.value().abs());
    let a_fac = amp_factor(sweep_max).min(cfg.a_cap);
    let eps_center = eps.max(cfg.q_arc * a_fac);
    if !p3_close(ca, cb, eps_center) {
        d.push(Divergence::Geometry { index: i, field: "center" });
    }
    if !p3_close(na, nb, eps) {
        d.push(Divergence::Geometry { index: i, field: "normal" });
    }
    if da != db {
        d.push(Divergence::Geometry { index: i, field: "dir" });
    }
    // sweep: max(1e-4, 1e-6·sweep), включая sweep=360°.
    if !num_close(swa.value(), swb.value(), 1e-4_f64.max(1e-6 * sweep_max)) {
        d.push(Divergence::Geometry { index: i, field: "sweep" });
    }
    // pitch = ε_pos (длина).
    if !num_close(pia.value(), pib.value(), eps) {
        d.push(Divergence::Geometry { index: i, field: "pitch" });
    }
    diff_state(i, st_a, st_b, cfg, d);
}

/// Усиление R→центр у околополуокружностей (EQUIVALENCE §4):
/// A(sweep) = 1/max(|cos(sweep/2)|, 1e-3) — cos, не sin (ревью §4).
fn amp_factor(sweep_deg: f64) -> f64 {
    let half = (sweep_deg / 2.0).to_radians();
    1.0 / half.cos().abs().max(1e-3)
}

/// start: unknown ≡ unknown; unknown vs known — расхождение (§3 v0.2);
/// known vs known — допуск позиции.
fn diff_start(i: usize, sa: &StartPoint, sb: &StartPoint, eps: f64, d: &mut Vec<Divergence>) {
    match (sa, sb) {
        (StartPoint::Known(x), StartPoint::Known(y)) => {
            if !p3_close(x, y, eps) {
                d.push(Divergence::Geometry { index: i, field: "start" });
            }
        }
        (StartPoint::Unknown, StartPoint::Unknown) => {}
        _ => d.push(Divergence::UnknownVsKnown { index: i }),
    }
}

fn diff_state(i: usize, a: &State, b: &State, cfg: &DiffConfig, d: &mut Vec<Divergence>) {
    match (&a.feed, &b.feed) {
        (None, None) => {}
        (Some(x), Some(y)) => {
            // feed: max(0.001, feed_rel·feed); none ↔ число — всегда расхождение.
            let tol = cfg.eps_abs.max(cfg.feed_rel * x.value().abs().max(y.value().abs()));
            if !num_close(x.value(), y.value(), tol) {
                d.push(Divergence::StateField { index: i, field: "feed" });
            }
        }
        _ => d.push(Divergence::StateField { index: i, field: "feed" }),
    }
    match (&a.spindle, &b.spindle) {
        (Spindle::Off, Spindle::Off) => {}
        (Spindle::Cw(x), Spindle::Cw(y)) | (Spindle::Ccw(x), Spindle::Ccw(y)) => {
            // rpm: max(0.01, rpm_rel·rpm)
            let tol = 0.01_f64.max(cfg.rpm_rel * x.value().abs().max(y.value().abs()));
            if !num_close(x.value(), y.value(), tol) {
                d.push(Divergence::StateField { index: i, field: "spindle" });
            }
        }
        _ => d.push(Divergence::StateField { index: i, field: "spindle" }),
    }
    // coolant / tool / wcs / frame — точные (EQUIVALENCE §2).
    if a.coolant != b.coolant {
        d.push(Divergence::StateField { index: i, field: "coolant" });
    }
    if a.tool != b.tool {
        d.push(Divergence::StateField { index: i, field: "tool" });
    }
    if a.frame != b.frame {
        d.push(Divergence::StateField { index: i, field: "frame" });
    }
    if a.wcs != b.wcs {
        d.push(Divergence::StateField { index: i, field: "wcs" });
    }
}

// ===========================================================================
// Числа/точки
// ===========================================================================

/// L — диагональ bbox всех endpoint'ов motion-опов ОБЕИХ программ (EQUIVALENCE §4)
/// → ε_pos = max(ε_abs, k_pos·L).
fn eps_pos(a: &Program, b: &Program, cfg: &DiffConfig) -> f64 {
    let mut min = (f64::MAX, f64::MAX, f64::MAX);
    let mut max = (f64::MIN, f64::MIN, f64::MIN);
    let mut any = false;
    let mut take = |v: (f64, f64, f64)| {
        any = true;
        min.0 = min.0.min(v.0);
        min.1 = min.1.min(v.1);
        min.2 = min.2.min(v.2);
        max.0 = max.0.max(v.0);
        max.1 = max.1.max(v.1);
        max.2 = max.2.max(v.2);
    };
    for p in [a, b] {
        for o in filter_m1(&p.ops) {
            if let Op::Rapid { start, end, .. }
            | Op::Linear { start, end, .. }
            | Op::Arc { start, end, .. } = o
            {
                if let StartPoint::Known(k) = start {
                    take(p3_vals(k));
                }
                take(p3_vals(end));
            }
        }
    }
    let l = if any {
        ((max.0 - min.0).powi(2) + (max.1 - min.1).powi(2) + (max.2 - min.2).powi(2)).sqrt()
    } else {
        0.0
    };
    cfg.eps_abs.max(cfg.k_pos * l)
}

fn motion_parts(op: &Op) -> (&StartPoint, &Point3, &State) {
    match op {
        Op::Rapid { start, end, state, .. }
        | Op::Linear { start, end, state, .. }
        | Op::Arc { start, end, state, .. } => (start, end, state),
        _ => unreachable!("motion_parts on non-motion op"),
    }
}

fn num_close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn p3_close(a: &Point3, b: &Point3, tol: f64) -> bool {
    num_close(a.x.value(), b.x.value(), tol)
        && num_close(a.y.value(), b.y.value(), tol)
        && num_close(a.z.value(), b.z.value(), tol)
}

fn p3_vals(p: &Point3) -> (f64, f64, f64) {
    (p.x.value(), p.y.value(), p.z.value())
}
