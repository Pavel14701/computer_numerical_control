//! kinematics_5axis — фаза 12.5.4, модуль 4: кинематика **gantry / bridge
//! mill с вращающейся головой XYZBCW** (B-наклон + C-азимут на голове,
//! W — виртуальная ось вдоль инструмента, складывающаяся в XYZ-джоинты).
//!
//! # Оракул формул (математические факты)
//!
//! LinuxCNC `src/emc/kinematics/5axiskins.c` (автор C. Radek; GPL-2.0+),
//! master на 2026-11-26; sim-оракул
//! `configs/sim/axis/vismach/5axis/bridgemill/` (5axis.ini,
//! MACHINE_MOVES_IN_MACHINE_FRAME=1; demo `g532-fused-orient-move.ngc`,
//! `g536-orient-then-move.ngc` — живой rs274-оракул TWP/RTCP-цепочки
//! G68.2/G53.2/G53.6 верхних слоёв). Формулы перенесены как
//! **математические факты**: выражения выписаны самостоятельно на Rust
//! (GPL-текст не копировался), сверены член-в-член с C-источником
//! (знаки/порядок 1:1) и с research `kinematics_research.md` §2.4 (его
//! численная верификация: fwd∘inv ≤ 5.7e-14 при b ∈ (5°, 175°), c —
//! полный круг). Самый богатый модуль семейства: якобиан, frames,
//! честная W-ось.
//!
//! # Сферическая форма (полярная)
//!
//! C — азимут (о Z, theta), B — полярный угол от −Z (в оракуле phi =
//! `180 − B`: «the direction of the tilt axis is the opposite of the
//! conventional axis direction»). Радиус до кончика
//! `R = pivot_length + tool_length + w`; s2r-вызов оракула
//! `s2r(R, c, 180 − b)` разворачивается в
//! `(R·sin b·cos c, R·sin b·sin c, −R·cos b)` (подтверждено комментарием
//! якобиана оракула). Позиция:
//! * forward: `X = JX + r.x; Y = JY + r.y; Z = JZ + pivot + tool + r.z`;
//! * inverse: точное вычитание тех же слагаемых.
//!
//! Знак W оракула: «Negative values increase tool radial motion» —
//! положительное w укорачивает вылет кончика из пивота (r = радиус до
//! кончика), виртуальная ось складывается в XYZ-джоинты.
//!
//! # Tool-frame (machine)
//!
//! `Rz(C)·Ry(−B)` — «the tilt left handed about y» (комментарий оракула);
//! 3-й столбец = ось инструмента от кончика к державке:
//! `(−sb·cc, −sb·sc, cb)`. Work-frame = identity (голова несёт
//! инструмент, стол деталь не вращает — `kinsIdentityFrame` оракула).
//!
//! # Соглашения (как в оракуле)
//!
//! * Углы — в **градусах** на границе API (`TO_RAD = π/180` внутри);
//!   линейные — единицы машины.
//! * Углы B/C и W проходят насквозь без нормализации (pass-through).
//! * Inverse — точная алгебраическая инверсия forward (особых точек нет:
//!   sin/cos/hypot без делений).

/// 3×3 матрица поворота (правые, столбцы-векторы; по строкам — коэффициенты).
pub type Mat3 = [[f64; 3]; 3];

/// Геометрия станка gantry/bridge mill (аналог HAL-пинов `pivot-length`,
/// `tool-length` оракула; дефолт pivot оракула 250 — машина-специфично).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FiveAxisParams {
    /// Длина пивота головы (HAL `pivot-length`).
    pub pivot_length: f64,
    /// Длина инструмента (HAL `tool-length`).
    pub tool_length: f64,
}

/// Joints XYZBCW: X/Y/Z линейные + B (наклон головы) + C (азимут) + W
/// (виртуальная ось вдоль инструмента); углы в градусах.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JointsXyzbcw {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub b: f64,
    pub c: f64,
    pub w: f64,
}

/// Поза XYZBCW (те же буквы: B/C/W проходят насквозь — как в оракуле
/// `pos->b = joints[JB]` и т.д.).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PoseXyzbcw {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub b: f64,
    pub c: f64,
    pub w: f64,
}

/// Сферическое преобразование оракула `s2r(r, t, p)`: t — theta (азимут
/// проекции на XY), p — phi (угол от оси Z); углы в градусах, конверсия
/// внутри (как в оракуле: `t = TO_RAD*t; p = TO_RAD*p` до sin/cos).
fn s2r(r: f64, t_deg: f64, p_deg: f64) -> [f64; 3] {
    let t = t_deg.to_radians();
    let p = p_deg.to_radians();
    [r * p.sin() * t.cos(), r * p.sin() * t.sin(), r * p.cos()]
}

/// Радиус-вектор пивот → кончик (вызов оракула 1:1:
/// `s2r(pivot + tool + w, c, 180 − b)` ⇒ `(R·sin b·cos c, R·sin b·sin c,
/// −R·cos b)`).
fn head_vector(p: &FiveAxisParams, b_deg: f64, c_deg: f64, w: f64) -> [f64; 3] {
    s2r(p.pivot_length + p.tool_length + w, c_deg, 180.0 - b_deg)
}

/// XYZBCW forward: joints → декартова поза (формулы 5axiskins.c
/// `kinematicsForward`, факт).
pub fn fiveaxis_forward(p: &FiveAxisParams, j: &JointsXyzbcw) -> PoseXyzbcw {
    let r = head_vector(p, j.b, j.c, j.w);
    PoseXyzbcw {
        x: j.x + r[0],
        y: j.y + r[1],
        z: j.z + p.pivot_length + p.tool_length + r[2],
        b: j.b,
        c: j.c,
        w: j.w,
    }
}

/// XYZBCW inverse: декартова поза → joints (формулы 5axiskins.c
/// `kinematicsInverse`, факт — точное вычитание).
pub fn fiveaxis_inverse(p: &FiveAxisParams, pose: &PoseXyzbcw) -> JointsXyzbcw {
    let r = head_vector(p, pose.b, pose.c, pose.w);
    JointsXyzbcw {
        x: pose.x - r[0],
        y: pose.y - r[1],
        z: pose.z - p.pivot_length - p.tool_length - r[2],
        b: pose.b,
        c: pose.c,
        w: pose.w,
    }
}

/// Tool-frame в machine-координатах: `Rz(C)·Ry(−B)` (матрица оракула 1:1,
/// «the tilt left handed about y»); 3-й столбец — ось инструмента от
/// кончика к державке `(−sb·cc, −sb·sc, cb)`. Углы в градусах.
pub fn tool_frame_machine(b_deg: f64, c_deg: f64) -> Mat3 {
    let b = b_deg.to_radians();
    let c = c_deg.to_radians();
    let (sb, cb) = (b.sin(), b.cos());
    let (sc, cc) = (c.sin(), c.cos());
    [
        [cb * cc, -sc, -sb * cc],
        [cb * sc, cc, -sb * sc],
        [sb, 0.0, cb],
    ]
}
