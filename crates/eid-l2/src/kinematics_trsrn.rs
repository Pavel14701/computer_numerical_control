//! kinematics_trsrn — фаза 12.5.4, модуль 2: кинематика **head-head
//! универсальной ортогональной головы (ν = 90°) + head-head nutating
//! (ν ∈ [0°, 90°])** — машина XYZ + A (стол-ротор) + B (secondary,
//! нутационный) + C (primary головы); пивоты Ly/Lz и смещения Dx/Dy
//! покрывают non-intersecting геометрию (She 2007 / Zeeko-класс).
//!
//! # Оракулы формул (математические факты)
//!
//! * LinuxCNC `src/hal/components/xyzacb_trsrn.comp` (автор D. Mueller;
//!   GPL-2.0+), master на 2026-11-26: TCP-inverse/-forward трансляция
//!   (j[0..2]) и матрицы Rp/Rs. Семантика букв — по INI/README
//!   sim-конфига `table-rotary_spindle-rotary-nutating/`:
//!   **A — table rotary, B — spindle secondary (нутационный), C — spindle
//!   primary** (заголовок comp «rotary table C» противоречит INI —
//!   авторитет INI + python-оракул; research §2.3 «⚠ сверить» разрешён
//!   по INI-семантике).
//! * НЕЗАВИСИМЫЙ python-оракул `tests/kins-twp/xyzacb/remap_funcs_twp.py`
//!   (in-tree тест LinuxCNC `tests/kins-twp/`): решатель углов θ1/θ2 по
//!   вектору оси инструмента + матрица Rs. README оракула: «python-
//!   математика написана НЕЗАВИСИМО от C-модуля… где расходятся — у
//!   одного из двух знак или порядок поворота неверен» — образец
//!   дисциплины «формулы ≠ код, сверка на сетке углов».
//!
//! Формулы выписаны самостоятельно на Rust (GPL-текст не копировался),
//! сверены член-в-член с обоими оракулами (знаки 1:1) и с research
//! `kinematics_research.md` §2.3 (его численная верификация: 799/799
//! ориентаций восстановлены точно при ν ∈ {30,45,60,90}).
//!
//! # Вращение TOOL-кинематики (work→machine)
//!
//! `R = Rp·Rs·Rtc`:
//! * `Rp = Rz(θ1)` — primary (голова, о machine Z);
//! * `Rs` — нутационный secondary (поворот θ2 о оси нутации `(0, Sv, Cv)`,
//!   ν — nut-angle): замкнутая форма comp совпадает с формулой Родрига
//!   (проверено тестом `trsrn_nutating_secondary_matches_rodrigues` —
//!   независимая She 2007-сверка: She & Chang 2007, IJMTM 47:537–545 —
//!   первоисточник нутационной головы, paywall, только ссылка);
//! * `Rtc = Rz(tc)` — виртуальный поворот о tool-Z (HAL `pre-rot`; в
//!   TCP-режиме tc не влияет на ось инструмента — Rz о ней инвариантен).
//!
//! **Вырождение ν = 90°** (research §2.3, проверено численно оракулом и
//! тестом `trsrn_nut90_degenerates_to_ry`): при Cv = 0 матрица Rs
//! покомпонентно равна `Ry(θ2)` — обычная ортогональная head-head голова
//! (Fidia/DMU 340P-класс).
//!
//! # Решатель углов (python-оракул, строки 144–199)
//!
//! По вектору оси инструмента `Kz = (Kzx, Kzy, Kzz)` (3-й столбец R):
//! * вертикаль `Kzz > 1 − ε` ⇒ θ1 = 0, θ2 = 0 (бесконечно решений — одно
//!   определено явно);
//! * достижимость `Kzz ≥ 2·Cv² − 1` (минимум s(θ2) по θ2), иначе None;
//! * `θ2 = acos(clamp((Kzz − Cv²)/(1 − Cv²)))` — главная ветвь θ2 ∈
//!   [0°, 180°]; альтернативная ветвь ±θ2 даёт тот же Kz;
//! * `θ1 = asin(clamp((p·Kzy − t·Kzx)/(t² + p²)))`, `p = Sv·Ss`,
//!   `t = Sv·Cv·(1 − Cs)` — главная ветвь θ1 ∈ [−90°, 90°]; альтернатива
//!   `π − θ1` даёт тот же Kz.
//!
//! `clamp` — `clamp_unit` оракула: значение в пределах `UNIT_EPSILON =
//! 1e-9` от ±1 защёлкивается в ±1 (ошибки округления матричных
//! произведений), иначе проходит насквозь (действительно недостижимые
//! ориентации остаются ошибкой).
//!
//! # Соглашения (как в оракуле)
//!
//! * Углы — в **градусах** на границе API; линейные — единицы машины.
//! * Геометрия (HAL-пины comp): `y-pivot` = Ly, `z-pivot` = Lz — пивоты
//!   головы; `x-offset` = Dx, `y-offset` = Dy — смещения;
//!   `y-rot-axis`/`z-rot-axis` — оси ротаций; `tool-offset-z` = Dt.
//!   Производные оракула: `Dray = y-rot-axis − (Dy + Ly)`,
//!   `Draz = z-rot-axis − Lz`.
//! * TCP-пара: [`trsrn_tcp_forward`] (joints → programmed TCP-позиция Q)
//!   и [`trsrn_tcp_inverse`] (Q → joints) — точная алгебраическая пара
//!   при фиксированных углах A/B/C (углы проходят насквозь).
//! * A проходит через Sw/Cw (стол), B — Ss/Cs, C — Sp/Cp.

/// 3×3 матрица поворота (правые, столбцы-векторы; по строкам — коэффициенты).
pub type Mat3 = [[f64; 3]; 3];

/// Геометрия + нутация (аналог блока HAL-пинов оракула
/// `nut-angle`, `y-/z-pivot`, `x-/y-offset`, `y-/z-rot-axis`,
/// `tool-offset-z`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TrsrnParams {
    /// ν — nut-angle в градусах (домен машины/оракула 0..=90; при 90° —
    /// вырождение в ортогональную голову, при 0° — Rs ≡ Rz(θ2)).
    pub nut_angle: f64,
    /// Ly (HAL `y-pivot`) — пивот головы по Y.
    pub y_pivot: f64,
    /// Lz (HAL `z-pivot`) — пивот головы по Z.
    pub z_pivot: f64,
    /// Dx (HAL `x-offset`).
    pub x_offset: f64,
    /// Dy (HAL `y-offset`).
    pub y_offset: f64,
    /// HAL `y-rot-axis` (производная `Dray = y-rot-axis − (Dy + Ly)`).
    pub y_rot_axis: f64,
    /// HAL `z-rot-axis` (производная `Draz = z-rot-axis − Lz`).
    pub z_rot_axis: f64,
    /// Dt (HAL `tool-offset-z`) — вылет инструмента.
    pub tool_offset_z: f64,
}

/// Joints XYZABC-trsrn: X/Y/Z линейные + A (стол-ротор) + B (secondary,
/// нутационный) + C (primary); углы в градусах.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JointsTsrn {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub a: f64,
    pub b: f64,
    pub c: f64,
}

impl JointsTsrn {
    pub fn new(x: f64, y: f64, z: f64, a: f64, b: f64, c: f64) -> Self {
        Self { x, y, z, a, b, c }
    }
}

// ---------------------------------------------------------------------------
// Вращения: нутационный secondary + полная цепочка R = Rp·Rs·Rtc
// ---------------------------------------------------------------------------

/// Нутационный secondary `Rs` (замкнутая форма comp): поворот θ2 о оси
/// нутации `(0, Sv, Cv)`, где Sv/Cv — синус/косинус nut-angle ν. При
/// ν = 90° ≡ `Ry(θ2)`, при ν = 0° ≡ `Rz(θ2)` (обе вырожденности
/// проверены тестами). Углы в градусах.
pub fn nutating_secondary(nut_angle_deg: f64, theta2_deg: f64) -> Mat3 {
    let v = nut_angle_deg.to_radians();
    let th = theta2_deg.to_radians();
    let (sv, cv) = (v.sin(), v.cos());
    let (ss, cs) = (th.sin(), th.cos());
    let r = cs + sv * sv * (1.0 - cs);
    let s = cs + cv * cv * (1.0 - cs);
    let t = sv * cv * (1.0 - cs);
    [[cs, -cv * ss, sv * ss], [cv * ss, r, t], [-sv * ss, t, s]]
}

/// Матрица Rz (правая, градусы) — локальный помощник (стандартная).
fn rz(deg: f64) -> Mat3 {
    let r = deg.to_radians();
    let (s, c) = (r.sin(), r.cos());
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

fn mat_mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut o = [[0.0f64; 3]; 3];
    for (i, row) in o.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    o
}

/// Полная цепочка вращения TOOL-кинематики work→machine:
/// `R = Rz(θ1)·Rs(ν, θ2)·Rz(tc)`. Углы в градусах (θ1 — primary,
/// θ2 — secondary/нутация, tc — виртуальный `pre-rot` о tool-Z; на ось
/// инструмента tc не влияет — 3-й столбец от него не зависит).
pub fn tool_rotation(nut_angle_deg: f64, theta1_deg: f64, theta2_deg: f64, tc_deg: f64) -> Mat3 {
    let rp = rz(theta1_deg);
    let rs = nutating_secondary(nut_angle_deg, theta2_deg);
    let rtc = rz(tc_deg);
    mat_mul(&rp, &mat_mul(&rs, &rtc))
}

/// `clamp_unit` python-оракула (UNIT_EPSILON = 1e-9): значение в пределах
/// 1e-9 от ±1 защёлкивается в ±1 (сглаживание ошибок округления
/// матричных произведений), иначе — насквозь.
fn clamp_unit(v: f64) -> f64 {
    const UNIT_EPSILON: f64 = 1e-9;
    if v > 1.0 - UNIT_EPSILON {
        1.0
    } else if v < -(1.0 - UNIT_EPSILON) {
        -1.0
    } else {
        v
    }
}

/// Решатель углов primary/secondary по вектору оси инструмента
/// `kz = (Kzx, Kzy, Kzz)` (единичный; 3-й столбец [`tool_rotation`]).
/// Возвращает `(theta1_deg, theta2_deg)` — ГЛАВНЫЕ ветви
/// (θ1 ∈ [−90°, 90°], θ2 ∈ [0°, 180°]; альтернативные ±θ2 и π−θ1 дают
/// тот же Kz). `None` — ориентация недостижима (`Kzz < 2·Cv² − 1`).
/// Вертикальный инструмент (`Kzz > 1 − ε`) ⇒ `(0, 0)` — бесконечно
/// решений, одно определено явно (конвенция python-оракула).
pub fn trsrn_angles_from_tool_axis(nut_angle_deg: f64, kz: [f64; 3]) -> Option<(f64, f64)> {
    let v = nut_angle_deg.to_radians();
    let (sv, cv) = (v.sin(), v.cos());
    let kzz = kz[2];

    // Вертикаль: бесконечно решений — одно явно (θ1 = θ2 = 0).
    if kzz > 1.0 - 1e-9 {
        return Some((0.0, 0.0));
    }
    // Достижимость: нутация ограничивает отрицательный наклон
    // (минимум s(θ2) = 2·Cv² − 1 при Cs = −1).
    if kzz < 2.0 * cv * cv - 1.0 {
        return None;
    }
    let arg2 = clamp_unit((kzz - cv * cv) / (1.0 - cv * cv));
    let theta2 = arg2.acos();
    let (ss, cs) = (theta2.sin(), theta2.cos());
    let p = sv * ss;
    let t = sv * cv * (1.0 - cs);
    let denom = t * t + p * p;
    if denom == 0.0 {
        // Защитный guard (недостижим на проверенных путях: Sv = 0 отсечён
        // проверкой достижимости, θ2 = 0 — вертикальной ветвью).
        return None;
    }
    let arg1 = clamp_unit((p * kz[1] - t * kz[0]) / denom);
    let theta1 = arg1.asin();
    Some((theta1.to_degrees(), theta2.to_degrees()))
}

// ---------------------------------------------------------------------------
// TCP-пара: forward (joints → Q) и inverse (Q → joints) при фиксированных
// углах A/B/C (comp xyzacb_trsrn: TCP-inverse строки 239–274 + TCP-forward)
// ---------------------------------------------------------------------------

/// TCP-forward: joints → programmed TCP-позиция (формулы comp
/// `xyzacb_trsrn`, факт). Углы A (стол), B (secondary), C (primary)
/// проходят насквозь. `Dray = y-rot-axis − (Dy + Ly)`,
/// `Draz = z-rot-axis − Lz` — производные геометрии оракула.
pub fn trsrn_tcp_forward(p: &TrsrnParams, j: &JointsTsrn) -> crate::kinematics::Pose6 {
    let v = p.nut_angle.to_radians();
    let (sv, cv) = (v.sin(), v.cos());
    let a = j.a.to_radians();
    let b = j.b.to_radians();
    let c = j.c.to_radians();
    let (sw, cw) = (a.sin(), a.cos()); // стол A
    let ss = b.sin(); // secondary B
    let (sp, cp) = (c.sin(), c.cos()); // primary C
    let svss = sv * ss;
    let cvss = cv * ss;
    let cs = b.cos();
    let r = cs + sv * sv * (1.0 - cs);
    let s = cs + cv * cv * (1.0 - cs);
    let t = sv * cv * (1.0 - cs);
    let ly = p.y_pivot;
    let lz = p.z_pivot;
    let (dx, dy) = (p.x_offset, p.y_offset);
    let dray = p.y_rot_axis - (dy + ly);
    let draz = p.z_rot_axis - lz;
    let dt = p.tool_offset_z;
    let dtlz = dt + lz;

    crate::kinematics::Pose6 {
        x: -(cp * svss - sp * t) * dtlz - cp * dx + (cp * cvss + sp * r) * ly + dy * sp + dx + j.x,
        y: -cp * cw * dy
            - cw * dx * sp
            - cw * (dray - j.y)
            - (cw * sp * svss + cp * cw * t - sw * s) * dtlz
            + (cvss * cw * sp - cp * cw * r + sw * t) * ly
            + (draz - j.z) * sw
            + dray
            + dy
            + ly,
        z: -cp * dy * sw
            - dx * sp * sw
            - cw * (draz - j.z)
            - (sp * svss * sw + cp * sw * t + cw * s) * dtlz
            + (cvss * sp * sw - cp * sw * r - cw * t) * ly
            - (dray - j.y) * sw
            + draz
            + dt
            + lz,
        a: j.a,
        b: j.b,
        c: j.c,
    }
}

/// TCP-inverse: programmed TCP-позиция → joints (формулы comp, факт;
/// строки 239–274). Углы проходят насквозь.
pub fn trsrn_tcp_inverse(p: &TrsrnParams, pose: &crate::kinematics::Pose6) -> JointsTsrn {
    let v = p.nut_angle.to_radians();
    let (sv, cv) = (v.sin(), v.cos());
    let a = pose.a.to_radians();
    let b = pose.b.to_radians();
    let c = pose.c.to_radians();
    let (sw, cw) = (a.sin(), a.cos()); // стол A
    let ss = b.sin(); // secondary B
    let (sp, cp) = (c.sin(), c.cos()); // primary C
    let svss = sv * ss;
    let cvss = cv * ss;
    let cs = b.cos();
    let r = cs + sv * sv * (1.0 - cs);
    let s = cs + cv * cv * (1.0 - cs);
    let t = sv * cv * (1.0 - cs);
    let ly = p.y_pivot;
    let lz = p.z_pivot;
    let (dx, dy) = (p.x_offset, p.y_offset);
    let dray = p.y_rot_axis - (dy + ly);
    let draz = p.z_rot_axis - lz;
    let dt = p.tool_offset_z;
    let dtlz = dt + lz;

    JointsTsrn {
        x: (cp * svss - sp * t) * dtlz + cp * dx - (cp * cvss + sp * r) * ly - dy * sp - dx
            + pose.x,
        y: cp * dy + dx * sp - cw * (dray + dy + ly - pose.y) + (sp * svss + cp * t) * dtlz
            - (cvss * sp - cp * r) * ly
            - (draz + dt + lz - pose.z) * sw
            + dray,
        z: dtlz * s + ly * t - cw * (draz + dt + lz - pose.z)
            + (dray + dy + ly - pose.y) * sw
            + draz,
        a: pose.a,
        b: pose.b,
        c: pose.c,
    }
}
