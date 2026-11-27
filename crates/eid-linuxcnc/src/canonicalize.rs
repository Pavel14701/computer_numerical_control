//! Канонизация (Агент D, итерация 2): геометрия дуг (SPEC §6 — IJK/центры
//! G90.1/G91.1, R-форма, sweep/pitch, валидация N5a/N5b/N19) и числовые
//! помощники. Связывание стартов (Q11, точки разрыва (a)–(d)) — parser.rs
//! (последовательное состояние кадра), здесь — чистая математика.
//!
//! Интерпретации, зафиксированные в отчёте итерации 2:
//!  * normal дуги = +ось заявленной плоскости (SPEC §6): G17 → +Z (u=X, v=Y),
//!    G18 → +Y (u=Z, v=X — правая тройка), G19 → +X (u=Y, v=Z);
//!  * направление cw = отрицательное вращение вокруг normal (правая рука);
//!  * R-форма: центр — середина хорды ± h·перпендикуляр; знак выбирает дугу
//!    ≤180° (R>0) / >180° (R<0); R при start==end → E_INVALID_INPUT (N19);
//!    R+вне-плоскостное слово валидно (SPEC §6 v0.7: normal известен из
//!    плоскости, pitch — нормальная компонента хода, COVERAGE §6.9);
//!  * полное уравнение позиции SPEC §6.3 выполняется по построению
//!    (sweep вычисляется из геометрии);
//!  * вычисленные координаты/углы квантуются до 4 dp (fmt_num) — детерминизм.

use eid_core::ast::{Dir, Num, Point3};
use eid_core::emitter::fmt_num;

/// Плоскость парсера (G17/G18/G19): normal = +ось, (u, v) — правая пара,
/// индексы вектора: 0=X, 1=Y, 2=Z.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plane {
    G17,
    G18,
    G19,
}

/// (normal_axis, u_axis, v_axis): u × v = normal.
pub fn plane_def(p: Plane) -> (usize, usize, usize) {
    match p {
        // G17: X×Y=+Z; G18: Z×X=+Y; G19: Y×Z=+X
        Plane::G17 => (2, 0, 1),
        Plane::G18 => (1, 2, 0),
        Plane::G19 => (0, 1, 2),
    }
}

/// Num из вычисленного значения: 4 dp, "-0.0000" → "0.0000" (детерминизм).
pub fn num_snap(v: f64) -> Num {
    let t = fmt_num(v);
    Num::new(t.clone(), t.parse().unwrap_or(v))
}

pub fn p3_snap(v: [f64; 3]) -> Point3 {
    Point3::xyz(num_snap(v[0]), num_snap(v[1]), num_snap(v[2]))
}

/// Num-точка → [f64; 3] (f64-зеркало verbatim-значений).
pub fn to_f64(p: &Point3) -> [f64; 3] {
    [p.x.value, p.y.value, p.z.value]
}

/// Результат разрешения дуги.
pub struct ResolvedArc {
    pub center: [f64; 3],
    pub sweep_deg: f64,
    pub pitch: f64,
    /// Планиметрический радиус — для G93-разрешения feed (длина дуги §6).
    pub radius: f64,
}

/// Итог разрешения: оп или zero-sweep drop (SPEC §10.6 v0.12/v0.17, CORPUS #13).
pub enum ArcOutcome {
    /// Degenerate: end == start (в плоскости, явные или наследованные слова)
    /// И center == start (радиус 0) И pitch == 0 → нулевая дуга →
    /// drop+notice (v0.17, #15: rs274 — «Zero-radius arc»; полный круг —
    /// это center ≠ start, единственный drop-класс center-формы).
    ZeroSweep,
    Resolved(ResolvedArc),
}

/// Класс ошибки дуги.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcError {
    /// R при start == end — центр неоднозначен (N19, SPEC §6 v0.7/v0.12).
    RFullCircle,
    /// Планиметрическая хорда > 2R — невозможная дуга (N5b, SPEC §6).
    ChordTooLong,
    /// IJK: ρ_start ≠ ρ_end за пределом допуска (N5a).
    RadiusMismatch,
}

const TAU: f64 = std::f64::consts::TAU;

/// sweep из полярных углов: cw — отрицательное направление вокруг normal
/// (правая рука); нормализация в (0°, 360°].
fn sweep_from_angles(a0: f64, a1: f64, dir: Dir) -> f64 {
    let raw = match dir {
        Dir::Cw => a0 - a1,
        Dir::Ccw => a1 - a0,
    };
    let s = (raw / TAU * 360.0).rem_euclid(360.0);
    if s <= 0.0 {
        360.0
    } else {
        s
    }
}

/// Планиметрическое равенство (точное на f64-зеркале: значения — verbatim
/// слова источника; полный круг по IJK — start == end).
fn planar_eq(a: [f64; 2], b: [f64; 2]) -> bool {
    a[0] == b[0] && a[1] == b[1]
}

fn dist2(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Разрешение дуги (SPEC §6).
///
/// * `start`, `end` — resolved абсолютные точки (входы квантованы, §10.6);
/// * `mode_abs` — G90.1 (абсолютные центры) / G91.1 = false (инкрементальные
///   от start) — ОБА режима, ABSORB (COVERAGE §2.2);
/// * `offsets` — значения I/J/K по осям X/Y/Z (None = слово не задано → 0);
/// * `r_word` — R-слово; при наличии — приоритет над IJK (интерпретация,
///   см. отчёт итерации 2);
/// * `explicit_end` — были ли слова осей в кадре. v0.17 (#15): критерий
///   полноты круга — геометрический (center vs start), форма команды НЕ
///   участвует; параметр сохранён в сигнатуре для симметрии с eid-grbl (R3).
///
/// sweep-выведение (v0.17, аудит #15 — зонд rs274): IJK-форма,
/// start == end (в плоскости):
///  * center ≠ start → sweep = 360° — НЕЗАВИСИМО от формы команды (явные
///    end-слова, квантовавшиеся в старт, или наследованные): реальный
///    контроллер (`G2 X10. Y0. I-5. J0` из (10,0) → `ARC_FEED(...,
///    rotation = -1, ...)`) и CAM-посты (Fusion/Mastercam) пишут end-слова
///    на полных кругах; pitch ≠ 0 — винтовой полный оборот;
///  * center == start (degenerate), pitch == 0 → [`ArcOutcome::ZeroSweep`]
///    → drop+notice у вызывающего (CORPUS #13 — единственный drop-класс
///    center-формы; форма команды не важна — rs274 отказывает обе);
///  * R-форма при start == end → `ArcError::RFullCircle` (N19) — без
///    изменений (центр неоднозначен).
#[allow(clippy::too_many_arguments)]
pub fn resolve_arc(
    start: &[f64; 3],
    end: &[f64; 3],
    plane: Plane,
    dir: Dir,
    mode_abs: bool,
    offsets: [Option<f64>; 3],
    r_word: Option<f64>,
    eps_pos: f64,
    _explicit_end: bool,
) -> Result<ArcOutcome, ArcError> {
    let (n_axis, u, v) = plane_def(plane);
    let s2 = [start[u], start[v]];
    let e2 = [end[u], end[v]];
    let pitch = end[n_axis] - start[n_axis]; // SPEC §6.2: (end−start)·normal

    if let Some(r) = r_word {
        let (center, rho) = resolve_r(plane, dir, s2, e2, r, start[n_axis], eps_pos)?;
        let a0 = (s2[1] - center[v]).atan2(s2[0] - center[u]);
        let a1 = (e2[1] - center[v]).atan2(e2[0] - center[u]);
        let sweep_deg = sweep_from_angles(a0, a1, dir);
        return Ok(ArcOutcome::Resolved(ResolvedArc {
            center,
            sweep_deg,
            pitch,
            radius: rho,
        }));
    }

    // --- IJK-форма ----------------------------------------------------------
    let offs = [
        offsets[0].unwrap_or(0.0),
        offsets[1].unwrap_or(0.0),
        offsets[2].unwrap_or(0.0),
    ];
    let mut center = [0.0f64; 3];
    center[u] = if mode_abs {
        offs[u]
    } else {
        start[u] + offs[u]
    };
    center[v] = if mode_abs {
        offs[v]
    } else {
        start[v] + offs[v]
    };
    center[n_axis] = start[n_axis]; // ось вращения — через start
    let rho_s = dist2(s2, [center[u], center[v]]);
    let rho_e = dist2(e2, [center[u], center[v]]);
    let tol_r = eps_pos.max(1e-6 * rho_s);
    if (rho_s - rho_e).abs() > tol_r {
        return Err(ArcError::RadiusMismatch); // N5a
    }

    if planar_eq(s2, e2) {
        // SPEC §3/§6 v0.17 (аудит #15, зонд rs274): полный круг 360° — по
        // геометрии (center ≠ start), не по форме команды; drop — только
        // degenerate-центр (радиус 0; rs274: «Zero-radius arc» — отказ на
        // обеих формах, у нас drop+notice) при pitch == 0.
        let degenerate_center = planar_eq(s2, [center[u], center[v]]);
        if pitch.abs() < 1e-9 && degenerate_center {
            return Ok(ArcOutcome::ZeroSweep); // §10.6 v0.12/v0.17, CORPUS #13
        }
        // Полный круг center-формой (SPEC §6 v0.12/v0.17, CORPUS #08/#15):
        // end наследуется или квантовался к старту; pitch ≠ 0 — винтовая.
        return Ok(ArcOutcome::Resolved(ResolvedArc {
            center,
            sweep_deg: 360.0,
            pitch,
            radius: rho_s,
        }));
    }

    let a0 = (s2[1] - center[v]).atan2(s2[0] - center[u]);
    let a1 = (e2[1] - center[v]).atan2(e2[0] - center[u]);
    let sweep_deg = sweep_from_angles(a0, a1, dir);
    Ok(ArcOutcome::Resolved(ResolvedArc {
        center,
        sweep_deg,
        pitch,
        radius: rho_s,
    }))
}

/// R-форма: центр из start, end, R, direction, normal (SPEC §6).
/// Возвращает (center, планиметрический радиус).
fn resolve_r(
    plane: Plane,
    dir: Dir,
    s2: [f64; 2],
    e2: [f64; 2],
    r: f64,
    n_coord: f64,
    eps_pos: f64,
) -> Result<([f64; 3], f64), ArcError> {
    let (n_axis, u, v) = plane_def(plane);

    // N19: R при start == end — центр неоднозначен (знак R не разрешает).
    if planar_eq(s2, e2) {
        return Err(ArcError::RFullCircle);
    }

    let d = [e2[0] - s2[0], e2[1] - s2[1]];
    let l = dist2([0.0; 2], d);
    let two_r = 2.0 * r.abs();
    // N5b: планиметрическая хорда > 2R — невозможная дуга (SPEC §6).
    // Около-2R (|L−2R| ≤ ε) трактуется как полуокружность (h → 0).
    if l > two_r + eps_pos {
        return Err(ArcError::ChordTooLong);
    }
    let half = l / 2.0;
    let h2 = r * r - half * half;
    let h = if h2 <= 0.0 { 0.0 } else { h2.sqrt() };
    let mid = [(s2[0] + e2[0]) / 2.0, (s2[1] + e2[1]) / 2.0];
    // Сторона перпендикуляра: R>0 — дуга ≤180°, R<0 — дуга >180°; cw/ccw
    // выбирает знак относительно rot_cw(d̂) = (d.v, −d.u)/|d|.
    let side = (if dir == Dir::Cw { 1.0 } else { -1.0 }) * r.signum();
    let dhat = [d[0] / l, d[1] / l];
    let perp_cw = [dhat[1], -dhat[0]];
    let c2 = [
        mid[0] + side * h * perp_cw[0],
        mid[1] + side * h * perp_cw[1],
    ];
    let mut center = [0.0f64; 3];
    center[u] = c2[0];
    center[v] = c2[1];
    center[n_axis] = n_coord; // ось вращения — через start
    Ok((center, r.abs()))
}
