//! Верификатор канонического AST EID/L1 — отдельный pass (MLIR-паттерн,
//! итерация 4). Вызывается на ЛЮБОМ этапе пайплайна: после parse, после
//! emit (по повторно разобранному тексту), standalone. parser.rs и
//! emitter.rs делегируют сюда — единственная копия правил валидации.
//!
//! Источники правил: SPEC-EID-L1 v0.15 (§3 инварианты, §5 опы, §6 каноническая
//! форма дуги, §2 принцип 2 / Q11 — точки разрыва и связывание), grammar.ebnf
//! v0.3 (D4–D8), допуски — EQUIVALENCE §4. Все диагностики pass'а помечены
//! [`Stage::Verify`] (interface v0.1.2).
//!
//! Два режима:
//!  * [`validate_with_notices`] — путь канонизации EID-текста: zero-length
//!    опы отбрасываются с notice (SPEC §3; CORPUS §4 — notice-файлы только
//!    R1-AST), остальные нарушения — диагностики;
//!  * [`validate`] — путь эмиссии/standalone: канонический AST обязан быть
//!    уже без zero-length (их отбросила канонизация) — наличие = диагностика.
//!
//! Замечания по интерпретации SPEC (утверждены оркестратором, SPEC v0.10):
//!  * точка разрыва (b) Q11 — критерий РАЗНОСТИ: start после смены wcs
//!    вычислим ⇔ ОБЕ системы (исходная и целевая) имеют in-text wcs_define
//!    того же P до точки хода; reading_new = prev_end + O_prev − O_new
//!    (EQUIVALENCE §1.3, Δ константен);
//!  * точки разрыва (c)/(d) Q11 — критерий вычислимости: machine-frame start
//!    вычислим ⇔ нет frame_preset (Δ=0) И origin активной WCS определён
//!    in-text (§5.3b; L20-происхождение origin не считается — M-производное);
//!    последующие machine-ходы цепляются: start = end предыдущего;
//!  * G10 L20 (wcs_set_here) задаёт чтение текущей позиции = delta, но
//!    машинное значение origin (O_p = M_here − delta) при M₀-неизвестности
//!    НЕ считается «известным из текста» для кросс-фреймовых преобразований;
//!  * N1 (E_RUNTIME_STATE за невыразимость machine-ходов, §5.3) — диспозиция
//!    канонизатора диалекта-источника; verify проверяет только каноническую
//!    форму (unknown в легальной точке разрыва валиден).

use crate::ast::*;
use crate::diag::{Diagnostic, ErrorCode, Stage};

// ===========================================================================
// Notices (drop-with-notice, SPEC §3 / §5.2 / §6). Notice — не Diagnostic:
// это не код ошибки, а флаг легального отброса (CORPUS §4: notice-файлы —
// только R1-AST).
// ===========================================================================

/// Класс отброшенного опа.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// rapid/linear: start == end (SPEC §3).
    ZeroLengthMotion,
    /// arc: start == end при sweep != 360° и pitch == 0 (SPEC §3/§6; включает
    /// sweep=0 — CORPUS вектор #13).
    ZeroLengthArc,
    /// dwell: seconds == 0 (SPEC §5.2).
    ZeroDwell,
}

/// Отброшенный оп + контекст отброса.
#[derive(Debug, Clone)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
    /// Индекс опа в исходном (до отбрасывания) списке `ops`.
    pub op_index: usize,
    /// Сам отброшенный оп («верни отброшенные»).
    pub op: Op,
}

/// Результат разбора: программа (zero-length отброшены) + notices.

// ===========================================================================
// Диагностика
// ===========================================================================

fn diag(code: ErrorCode, msg: impl Into<String>, src: Option<SrcSpan>) -> Diagnostic {
    Diagnostic {
        code,
        stage: Stage::Verify,
        message: msg.into(),
        src,
        options: Vec::new(),
        limit: None,
    }
}

fn op_src(op: &Op) -> Option<SrcSpan> {
    match op {
        Op::Rapid { src, .. }
        | Op::Linear { src, .. }
        | Op::Arc { src, .. }
        | Op::Dwell { src, .. }
        | Op::ToolChange { src, .. }
        | Op::ProgramStop { src, .. }
        | Op::ProgramEnd { src }
        | Op::WcsDefine { src, .. }
        | Op::WcsSetHere { src, .. }
        | Op::Raw { src, .. } => Some(src.clone()),
    }
}

/// Номер системы p (1..9) ↔ имя WCS: G54=1 … G59=6, G59.1=7, G59.2=8, G59.3=9.
pub fn wcs_to_p(w: Wcs) -> u8 {
    match w {
        Wcs::G54 => 1,
        Wcs::G55 => 2,
        Wcs::G56 => 3,
        Wcs::G57 => 4,
        Wcs::G58 => 5,
        Wcs::G59 => 6,
        Wcs::G59_1 => 7,
        Wcs::G59_2 => 8,
        Wcs::G59_3 => 9,
    }
}

// ===========================================================================
// Публичный API pass'а
// ===========================================================================

/// Отчёт validate_with_notices: выжившие опы, notices (отброшенное),
/// диагностики (пусто = валидно).
#[derive(Debug, Clone)]
pub struct VerifyReport {
    /// Опы, выжившие после отбрасывания zero-length (исходный порядок).
    pub kept_ops: Vec<Op>,
    /// Отброшенные опы (drop-with-notice, SPEC §3/§5.2/§6).
    pub notices: Vec<Notice>,
    /// Нарушения канонической формы (Stage::Verify).
    pub diagnostics: Vec<Diagnostic>,
}

/// Верификация с семантикой канонизации: zero-length опы отбрасываются с
/// notice, остальные нарушения — диагностики. Путь parse/канонизации.
pub fn validate_with_notices(program: &Program) -> VerifyReport {
    let (kept_ops, notices, diagnostics) = run(program, false);
    VerifyReport { kept_ops, notices, diagnostics }
}

/// Верификация канонической формы (путь эмиссии/standalone): zero-length
/// опы в AST = диагностика («канонизация обязана была отбросить», SPEC §3).
pub fn validate(program: &Program) -> Vec<Diagnostic> {
    run(program, true).2
}

// ===========================================================================
// Валидация (SPEC §3 + D4–D8 + §6) — собирает все ошибки
// ===========================================================================

fn is_motion(op: &Op) -> bool {
    matches!(op, Op::Rapid { .. } | Op::Linear { .. } | Op::Arc { .. })
}

fn nval(n: &Num) -> f64 {
    n.value
}

/// Точное равенство значений (f64-зеркало; verbatim-текст гарантируется
/// эмиттером — SPEC §10.1). -0.0 == 0.0.
fn p3_eq(a: &Point3, b: &Point3) -> bool {
    nval(&a.x) == nval(&b.x) && nval(&a.y) == nval(&b.y) && nval(&a.z) == nval(&b.z)
}

fn motion_parts(op: &Op) -> Option<(&StartPoint, &Point3, &State)> {
    match op {
        Op::Rapid { start, end, state, .. }
        | Op::Linear { start, end, state, .. }
        | Op::Arc { start, end, state, .. } => Some((start, end, state)),
        _ => None,
    }
}

fn run(program: &Program, reject_zero_length: bool) -> (Vec<Op>, Vec<Notice>, Vec<Diagnostic>) {
    let mut errors: Vec<Diagnostic> = Vec::new();
    let ops = &program.ops;

    // --- §3: program_end ровно один раз, последний; в минимальной программе —
    //     единственный; опов после него нет. --------------------------------
    if ops.is_empty() {
        errors.push(diag(
            ErrorCode::InvalidInput,
            "program has no ops: program_end (PEND) required exactly once as the last op (SPEC §3)",
            None,
        ));
    }
    let end_count = ops.iter().filter(|o| matches!(o, Op::ProgramEnd { .. })).count();
    if end_count == 0 && !ops.is_empty() {
        errors.push(diag(
            ErrorCode::InvalidInput,
            "missing program_end (PEND) — required exactly once, last (SPEC §3)",
            None,
        ));
    }
    if end_count > 1 {
        for o in ops.iter().filter(|o| matches!(o, Op::ProgramEnd { .. })) {
            errors.push(diag(
                ErrorCode::InvalidInput,
                "duplicate program_end (SPEC §3: ровно один PEND)",
                op_src(o),
            ));
        }
    }
    if let Some(last) = ops.last() {
        if !matches!(last, Op::ProgramEnd { .. }) {
            errors.push(diag(
                ErrorCode::InvalidInput,
                "program_end must be the last op (SPEC §3)",
                op_src(last),
            ));
        }
    }

    // --- D5: FRAME_PRESET/WCS_SET_HERE — только до первого motion-опа.
    //     FRAME_PRESET — поле заголовка (позиция структурная); WCS_SET_HERE —
    //     оп, проверяем по текстовому порядку. -------------------------------
    let first_motion_raw = ops.iter().position(is_motion);
    for (j, o) in ops.iter().enumerate() {
        if let Op::WcsSetHere { .. } = o {
            if let Some(fm) = first_motion_raw {
                if j > fm {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        "WCS_SET_HERE after first motion op (D5, SPEC §3/§5.3: only a preset before the first motion)",
                        op_src(o),
                    ));
                }
            }
        }
    }

    // --- D4 / D7 (домены, не зависящие от отбрасывания) ---------------------
    for o in ops.iter() {
        match o {
            Op::ToolChange { tool, state, .. } => {
                // D4: значение op-поля tool= равно state-полю tool=
                if state.tool != Some(*tool) {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!("TOOL_CHANGE tool={tool} != state tool={} (D4)", state.tool.map(|t| t.to_string()).unwrap_or_else(|| "none".into())),
                        op_src(o),
                    ));
                }
            }
            Op::WcsDefine { p, .. } | Op::WcsSetHere { p, .. } => {
                if *p < 1 || *p > 9 {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!("p={} out of domain 1..9 (D7: G54..G59.3)", p),
                        op_src(o),
                    ));
                }
            }
            Op::Dwell { seconds, .. } => {
                if nval(seconds) < 0.0 {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!("dwell seconds={} < 0 (D7, SPEC §5.2)", seconds.text),
                        op_src(o),
                    ));
                }
            }
            _ => {}
        }
    }

    // --- Отбрасывание zero-length (Parse: drop+notice; Emit: ошибка) --------
    let mut kept: Vec<Op> = Vec::new();
    let mut notices: Vec<Notice> = Vec::new();
    for (i, o) in ops.iter().enumerate() {
        let zero: Option<(NoticeKind, String)> = match o {
            Op::Rapid { start, end, .. } | Op::Linear { start, end, .. } => {
                if let StartPoint::Known(s) = start {
                    if p3_eq(s, end) {
                        Some((NoticeKind::ZeroLengthMotion, "zero-length motion dropped (start == end, SPEC §3)".into()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Op::Arc { start, end, sweep_deg, pitch, .. } => {
                // start == end при sweep != 360° и pitch == 0 — нулевой ход,
                // не дуга (SPEC §3/§6; sweep=0 сюда же — CORPUS #13).
                if let StartPoint::Known(s) = start {
                    if p3_eq(s, end) && nval(sweep_deg) != 360.0 && nval(pitch) == 0.0 {
                        Some((NoticeKind::ZeroLengthArc, "zero-length arc dropped (start == end, sweep != 360°, pitch == 0; SPEC §3/§6)".into()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Op::Dwell { seconds, .. } => {
                if nval(seconds) == 0.0 {
                    Some((NoticeKind::ZeroDwell, "zero dwell dropped (seconds == 0, SPEC §5.2)".into()))
                } else {
                    None
                }
            }
            _ => None,
        };
        match zero {
            Some((kind, message)) => {
                if reject_zero_length {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!(
                            "{} must be dropped by canonicalization before emission (SPEC §3)",
                            message
                        ),
                        op_src(o),
                    ));
                    kept.push(o.clone());
                } else {
                    notices.push(Notice { kind, message, op_index: i, op: o.clone() });
                }
            }
            None => kept.push(o.clone()),
        }
    }

    // --- Проверки по выжившим опам ------------------------------------------
    // D7-домены дуги; ARC с unknown-стартом; feed для linear/arc (SPEC §3:
    // требование относится к опам в итоговом AST, т.е. после отбрасывания).
    for o in kept.iter() {
        match o {
            Op::Arc { start, sweep_deg, normal, state, .. } => {
                if matches!(start, StartPoint::Unknown) {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        "arc with start=unknown is invalid (SPEC §2 Q11: arc requires a numeric start)",
                        op_src(o),
                    ));
                }
                let s = nval(sweep_deg);
                if s <= 0.0 || s > 360.0 {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!("sweep={} out of domain (0°, 360°] (D7, SPEC §6)", sweep_deg.text),
                        op_src(o),
                    ));
                }
                let len = vlen(&vec_vals(normal));
                if (len - 1.0).abs() > 0.001 {
                    // 0.001 — допуск 4dp-квантования компонент единичного вектора (D7)
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!("|normal| = {len:.6} != 1 (D7)"),
                        op_src(o),
                    ));
                }
                if state.feed.is_none() {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        "arc with feed=none (SPEC §3: linear/arc in the final AST require modal feed)",
                        op_src(o),
                    ));
                }
            }
            Op::Linear { state, .. } => {
                if state.feed.is_none() {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        "linear with feed=none (SPEC §3: linear/arc in the final AST require modal feed)",
                        op_src(o),
                    ));
                }
            }
            _ => {}
        }
    }

    // --- §6: геометрическая валидация выживших дуг --------------------------
    let eps_pos = eps_for(&kept);
    for o in kept.iter() {
        if let Op::Arc { start: StartPoint::Known(s), end, center, normal, dir, sweep_deg, pitch, .. } = o {
            check_arc_geometry(o, s, end, center, normal, *dir, nval(sweep_deg), nval(pitch), eps_pos, &mut errors);
        }
    }

    // --- Связывание start (SPEC §2 принцип 2, Q11; v0.9 обязанность) --------
    check_linking(program, &kept, &mut errors);

    (kept, notices, errors)
}

// ===========================================================================
// §6 — геометрия дуги (допуски — EQUIVALENCE §4)
// ===========================================================================

type V3 = (f64, f64, f64);

fn vec_vals(p: &Point3) -> V3 {
    (nval(&p.x), nval(&p.y), nval(&p.z))
}

fn p3_vals(p: &Point3) -> V3 {
    (nval(&p.x), nval(&p.y), nval(&p.z))
}

fn vlen(v: &V3) -> f64 {
    v.0.hypot(v.1).hypot(v.2)
}

fn vsub(a: &V3, b: &V3) -> V3 {
    (a.0 - b.0, a.1 - b.1, a.2 - b.2)
}

fn vadd(a: &V3, b: &V3) -> V3 {
    (a.0 + b.0, a.1 + b.1, a.2 + b.2)
}

fn vscale(a: &V3, k: f64) -> V3 {
    (a.0 * k, a.1 * k, a.2 * k)
}

fn vdot(a: &V3, b: &V3) -> f64 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

fn vcross(a: &V3, b: &V3) -> V3 {
    (a.1 * b.2 - a.2 * b.1, a.2 * b.0 - a.0 * b.2, a.0 * b.1 - a.1 * b.0)
}

fn vdist(a: &V3, b: &V3) -> f64 {
    vlen(&vsub(a, b))
}

/// Масштаб L (диагональ bbox всех endpoint'ов motion-опов, EQUIVALENCE §4) →
/// ε_pos = max(0.001, 1e-7·L).
fn eps_for(ops: &[Op]) -> f64 {
    let mut min = (f64::MAX, f64::MAX, f64::MAX);
    let mut max = (f64::MIN, f64::MIN, f64::MIN);
    let mut any = false;
    let mut take = |v: V3| {
        any = true;
        min.0 = min.0.min(v.0); min.1 = min.1.min(v.1); min.2 = min.2.min(v.2);
        max.0 = max.0.max(v.0); max.1 = max.1.max(v.1); max.2 = max.2.max(v.2);
    };
    for o in ops {
        if let Some((start, end, _)) = motion_parts(o) {
            if let StartPoint::Known(k) = start {
                take(p3_vals(k));
            }
            take(p3_vals(end));
        }
    }
    let l = if any {
        vlen(&(max.0 - min.0, max.1 - min.1, max.2 - min.2))
    } else {
        0.0
    };
    (0.001_f64).max(1e-7 * l)
}

/// Rodrigues: поворот v вокруг единичной оси n на угол θ (ccw — положительный).
fn rotate(v: &V3, n: &V3, theta: f64) -> V3 {
    let (s, c) = theta.sin_cos();
    let nxv = vcross(n, v);
    let nv = vdot(n, v);
    vadd(
        &vadd(
            &vscale(v, c),
            &vscale(&nxv, s),
        ),
        &vscale(n, nv * (1.0 - c)),
    )
}

#[allow(clippy::too_many_arguments)]
fn check_arc_geometry(
    op: &Op,
    start: &Point3,
    end: &Point3,
    center: &Point3,
    normal: &Point3,
    dir: Dir,
    sweep: f64,
    pitch: f64,
    eps_pos: f64,
    errors: &mut Vec<Diagnostic>,
) {
    let src = op_src(op);
    let s = p3_vals(start);
    let e = p3_vals(end);
    let c = p3_vals(center);
    let n_raw = vec_vals(normal);
    let nlen = vlen(&n_raw);
    let n = vscale(&n_raw, 1.0 / nlen); // |n|≈1 проверен доменом (D7)

    // 1. Планиметрический радиус: ρ_start ≈ ρ_end (проекция ⟂ normal).
    let planar = |d: &V3| -> V3 { vsub(d, &vscale(&n, vdot(&n, d))) };
    let rho_s = vlen(&planar(&vsub(&s, &c)));
    let rho_e = vlen(&planar(&vsub(&e, &c)));
    let tol_r = eps_pos.max(1e-6 * rho_s);
    if (rho_s - rho_e).abs() > tol_r {
        errors.push(diag(
            ErrorCode::InvalidInput,
            format!(
                "arc radius mismatch: |start| = {rho_s:.6}, |end| = {rho_e:.6}, tol = {tol_r:.6} (SPEC §6.1)"
            ),
            src.clone(),
        ));
    }

    // 2. Нормальная компонента хорды ≈ pitch.
    let chord_n = vdot(&vsub(&e, &s), &n);
    if (chord_n - pitch).abs() > eps_pos {
        errors.push(diag(
            ErrorCode::InvalidInput,
            format!(
                "arc chord·normal = {chord_n:.6} != pitch = {pitch:.6} (tol {eps_pos:.6}, SPEC §6.2)"
            ),
            src.clone(),
        ));
    }

    // 3. Полное уравнение позиции: end ≈ center + Rot(n, dir, sweep)·(start−center) + pitch·n.
    let theta = sweep.to_radians() * if dir == Dir::Cw { -1.0 } else { 1.0 };
    let rotated = rotate(&vsub(&s, &c), &n, theta);
    let expected = vadd(&vadd(&c, &rotated), &vscale(&n, pitch));
    let d = vdist(&expected, &e);
    if d > eps_pos {
        errors.push(diag(
            ErrorCode::InvalidInput,
            format!(
                "arc endpoint inconsistent with (center, normal, dir, sweep, pitch): |Δ| = {d:.6} > {eps_pos:.6} (SPEC §6.3)"
            ),
            src,
        ));
    }
}

// ===========================================================================
// Связывание start — SPEC §2 принцип 2 / Q11, v0.9 (точки разрыва (a)–(d))
// ===========================================================================

/// Ожидаемый start: точное значение (verbatim-цепочка), значение с допуском
/// (вычислено через origin-арифметику) или обязательный unknown.
#[derive(Debug, Clone)]
enum StartExpect {
    Exact(Point3),
    Tol(Point3),
    UnknownRequired,
}

struct PrevMotion {
    end: Point3,
    frame: Frame,
    wcs: Wcs,
}

fn p3_build(v: V3) -> Point3 {
    let mk = |x: f64| Num::new(crate::emitter::fmt_num(x), x);
    Point3 { x: mk(v.0), y: mk(v.1), z: mk(v.2) }
}

#[allow(clippy::too_many_arguments)]
fn expect_start(
    is_first: bool,
    frame: Frame,
    wcs: Wcs,
    preset: Option<&FramePreset>,
    set_here: &[Option<Point3>; 10],
    known_origin: &[Option<Point3>; 10],
    prev: Option<&PrevMotion>,
) -> StartExpect {
    let op_o = known_origin[wcs_to_p(wcs) as usize].clone();
    if !is_first {
        let prev = prev.expect("prev exists when not first");
        let prev_o = known_origin[wcs_to_p(prev.wcs) as usize].clone();
        return match (prev.frame, frame) {
            (Frame::Machine, Frame::Machine) => StartExpect::Exact(prev.end.clone()),
            (Frame::Wcs, Frame::Wcs) if prev.wcs == wcs => StartExpect::Exact(prev.end.clone()),
            (Frame::Wcs, Frame::Wcs) => match (&prev_o, &op_o) {
                // start = prev_end + O_prev − O_new (Δ константен, EQUIVALENCE §1.3)
                (Some(o_prev), Some(o_new)) => StartExpect::Tol(p3_build(vadd(
                    &p3_vals(&prev.end),
                    &vsub(&p3_vals(o_prev), &p3_vals(o_new)),
                ))),
                _ => StartExpect::UnknownRequired, // (b): разность origin'ов не вычислима
            },
            (Frame::Wcs, Frame::Machine) => {
                // M = reading + O_wcs + S; S = 0 только без frame_preset (SPEC §5.3)
                if preset.is_none() {
                    if let Some(op_) = &prev_o {
                        StartExpect::Tol(p3_build(vadd(&p3_vals(&prev.end), &p3_vals(op_))))
                    } else {
                        StartExpect::UnknownRequired
                    }
                } else {
                    StartExpect::UnknownRequired // (c): M₀ внешний
                }
            }
            (Frame::Machine, Frame::Wcs) => {
                // reading = M − O_wcs − S
                if preset.is_none() {
                    if let Some(on) = &op_o {
                        StartExpect::Tol(p3_build(vsub(&p3_vals(&prev.end), &p3_vals(on))))
                    } else {
                        StartExpect::UnknownRequired // (d)
                    }
                } else {
                    StartExpect::UnknownRequired // (d): Δ содержит M₀
                }
            }
        };
    }

    // Первый (выживший) motion-оп программы.
    if frame == Frame::Machine {
        // (c): стартовая позиция станка M₀ внешняя
        return StartExpect::UnknownRequired;
    }
    let sh = set_here[wcs_to_p(wcs) as usize].clone();
    match (sh, preset) {
        // G92 задаёт Δ = M₀ − O₀ − preset ≠ 0 → L20-чтение = delta − Δ содержит M₀
        (Some(_), Some(_)) => StartExpect::UnknownRequired,
        // (a)-якорь L20 (без пресетки): текущая позиция читается как delta
        (Some(delta), None) => StartExpect::Exact(delta),
        (None, Some(pr)) => {
            if pr.wcs == wcs {
                // (a)-якорь G92 в той же системе: reading = preset (golden 36)
                StartExpect::Exact(pr.values.clone())
            } else if known_origin[wcs_to_p(pr.wcs) as usize].is_some() && op_o.is_some() {
                // reading = preset + O₀ − O_wcs (обе системы определены в тексте)
                let o0 = known_origin[wcs_to_p(pr.wcs) as usize].clone().unwrap();
                StartExpect::Tol(p3_build(vadd(&p3_vals(&pr.values), &vsub(&p3_vals(&o0), &p3_vals(&op_o.unwrap())))))
            } else {
                // (b): смена на систему, разность origin'ов которой не вычислима
                StartExpect::UnknownRequired
            }
        }
        (None, None) => StartExpect::UnknownRequired, // (a): без якоря
    }
}

fn check_linking(program: &Program, kept: &[Op], errors: &mut Vec<Diagnostic>) {
    let mut known_origin: [Option<Point3>; 10] = Default::default();
    let mut set_here: [Option<Point3>; 10] = Default::default();
    let mut prev: Option<PrevMotion> = None;
    let mut saw_motion = false;

    for o in kept.iter() {
        match o {
            Op::WcsDefine { p, origin, .. } => {
                if (*p as usize) < known_origin.len() {
                    known_origin[*p as usize] = Some(origin.clone());
                }
            }
            Op::WcsSetHere { p, delta, .. } => {
                if (*p as usize) < set_here.len() {
                    set_here[*p as usize] = Some(delta.clone());
                }
            }
            _ if is_motion(o) => {
                let (start, end, state) = motion_parts(o).expect("motion op");
                let expect = expect_start(
                    !saw_motion,
                    state.frame,
                    state.wcs,
                    program.frame_preset.as_ref(),
                    &set_here,
                    &known_origin,
                    prev.as_ref(),
                );
                match (start, expect) {
                    (StartPoint::Unknown, StartExpect::UnknownRequired) => {}
                    (StartPoint::Unknown, StartExpect::Exact(_) | StartExpect::Tol(_)) => {
                        errors.push(diag(
                            ErrorCode::InvalidInput,
                            "start=unknown outside frame-break points (SPEC §2 v0.9: канонизатор обязан вычислить start = end предыдущего motion-опа)",
                            op_src(o),
                        ));
                    }
                    (StartPoint::Known(_v), StartExpect::UnknownRequired) => {
                        errors.push(diag(
                            ErrorCode::InvalidInput,
                            "start must be unknown here: позиция не вычислима из текста (точка разрыва фрейма, SPEC §2 Q11)",
                            op_src(o),
                        ));
                    }
                    (StartPoint::Known(v), StartExpect::Exact(exp)) => {
                        if !p3_eq(v, &exp) {
                            errors.push(diag(
                                ErrorCode::InvalidInput,
                                format!(
                                    "start {} != expected {} (точное равенство: start = end предыдущего motion-опа / якорь, SPEC §2 v0.9)",
                                    fmt_point(v), fmt_point(&exp)
                                ),
                                op_src(o),
                            ));
                        }
                    }
                    (StartPoint::Known(v), StartExpect::Tol(exp)) => {
                        let d = vdist(&p3_vals(v), &p3_vals(&exp));
                        if d > 0.001 {
                            // ε_abs из EQUIVALENCE §4 (floor 4dp-эмиссии с запасом ×10)
                            errors.push(diag(
                                ErrorCode::InvalidInput,
                                format!(
                                    "start {} != frame-adjusted previous end {} (|Δ| = {d:.6} > 0.001, SPEC §2 v0.9)",
                                    fmt_point(v), fmt_point(&exp)
                                ),
                                op_src(o),
                            ));
                        }
                    }
                }
                saw_motion = true;
                prev = Some(PrevMotion { end: end.clone(), frame: state.frame, wcs: state.wcs });
            }
            _ => {}
        }
    }
}

fn fmt_point(p: &Point3) -> String {
    format!("({} {} {})", p.x.text, p.y.text, p.z.text)
}
