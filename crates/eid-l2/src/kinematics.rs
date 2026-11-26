//! kinematics — фаза 12.5: ядро кинематики 5-осевых станков класса
//! **table-rotary-tilting** (наклоняемый стол + горизонтальный ротор,
//! смонтированный на столе): XYZAC (стол A + ротор C) и XYZBC (стол B +
//! ротор C). Forward (joints → декартовы координаты) и inverse (обратно).
//!
//! # Оракул формул (математические факты)
//!
//! LinuxCNC `src/emc/kinematics/trtfuncs.c` (автор формул Rudy du Preez,
//! 2016; GPL-2.0+), состояние master на 2026-11-26 — амендмент v0.1.5
//! дизайн-дока `EID-L2-DESIGN.md` (§12.5). Формулы перенесены как
//! **математические факты**: выражения выписаны самостоятельно на Rust
//! (C-код не копировался), каждая формула сверена член-в-член с оракулом
//! и дополнительно верифицирована тремя независимыми путями:
//! 1. круговой инвариант `forward∘inverse ≡ identity` на решётке кейсов
//!    (см. `tests/kinematics_tests.rs`);
//! 2. тождество матрицы поворота без офсетов: forward-матрица (строки —
//!    коэффициенты при joints) равна `Rz(con·c)·Rx(con·a)` для XYZAC и
//!    `Rz(con·c)·Ry(con·b)` для XYZBC в стандартных правых матрицах
//!    поворота — то же, что считают upstream-функции `xyzac_work_frame()`
//!    / `xyzbc_work_frame()` оракула;
//! 3. ручные зонд-кейсы (A=90° и т.п.).
//!
//! # Ограничение оракула: ротации «против конвенциональных»
//!
//! Заголовок trtfuncs.c: «The directions of the rotational axes are the
//! opposite of the conventional axis directions» (см.
//! <https://linuxcnc.org/docs/html/gcode/machining-center.html>).
//! Ограничение кодируется флагом-константой
//! [`DEFAULT_CONVENTIONAL_DIRECTIONS`] (= `false`, дефолт LinuxCNC) и
//! полем [`TrtParams::conventional`]: `false` → множитель `con = −1`
//! (ротации противоположны конвенциональным), `true` → `con = +1`.
//!
//! # Соглашения (как в оракуле)
//!
//! * Углы — в **градусах** и на входе, и на выходе (внутри — радианы,
//!   `f64::to_radians`, эквивалент `TO_RAD = π/180` оракула); линейные —
//!   единицы машины (мм, безразлично для формул).
//! * [`TrtParams::rot_point`] (x/y/z — пивот вращения) вычитается из
//!   joints до поворота и прибавляется к результату после.
//! * XYZAC: [`TrtParams::y_offset`] (`dy`) — смещение пивота оси A вдоль
//!   Y; [`TrtParams::z_offset`] (`dz`) — геометрия стола. В оракуле
//!   `dz = z-offset + tool-offset` (компенсация длины инструмента входит
//!   в dz ДО формул); в этом ядре нет отдельного поля tool-offset —
//!   вызывающая сторона складывает его в [`TrtParams::z_offset`] сама.
//! * XYZBC: [`TrtParams::x_offset`] (`dx`) — смещение пивота оси B вдоль
//!   X; тот же dz.
//! * Ротационный угол проходит насквозь без изменений (pass-through,
//!   без нормализации в [0;360)); forward заполняет неиспользуемый угол
//!   нулём (в оракуле буква вне `coordinates` ⇒ 0).
//! * Inverse — точная алгебраическая инверсия forward при любом `con`
//!   (круговой инвариант с допуском ~1e-15 относительных; в оракуле
//!   precomputed `dpx`/`dpz` XYZBC: `dpx = −cos b·dx + con·sin b·dz + dx`,
//!   `dpz = −con·sin b·dx − cos b·dz + dz`).

/// Дефолт флага «конвенциональные направления ротаций» — как пин
/// `conventional-directions` оракула (trtfuncs.c): **false** ⇒ `con = −1`,
/// т.е. ротационные оси направлены противоположно конвенциональным
/// (ограничение оракула, см. модульную документацию).
pub const DEFAULT_CONVENTIONAL_DIRECTIONS: bool = false;

/// Геометрия станка + флаг направления ротаций (аналог блока HAL-пинов
/// `x-/y-/z-rot-point`, `x-/y-/z-offset`, `conventional-directions`
/// оракула; tool-offset входит в [`z_offset`](TrtParams::z_offset)).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TrtParams {
    /// Пивот вращения `[x, y, z]` (точка, вокруг которой вращается стол).
    pub rot_point: [f64; 3],
    /// `dy` — смещение пивота оси A вдоль Y (XYZAC).
    pub y_offset: f64,
    /// `dx` — смещение пивота оси B вдоль X (XYZBC).
    pub x_offset: f64,
    /// `dz` — геометрия стола по Z; в оракуле `z-offset + tool-offset`
    /// (длину инструмента прибавляет вызывающая сторона).
    pub z_offset: f64,
    /// `false` (дефолт LinuxCNC) ⇒ `con = −1`: ротации противоположны
    /// конвенциональным; `true` ⇒ `con = +1`.
    pub conventional: bool,
}

impl TrtParams {
    /// Геометрия с явно заданными полями; `conventional` — флаг направлений
    /// ротаций (см. [`DEFAULT_CONVENTIONAL_DIRECTIONS`]).
    pub fn new(
        rot_point: [f64; 3],
        y_offset: f64,
        x_offset: f64,
        z_offset: f64,
        conventional: bool,
    ) -> Self {
        Self {
            rot_point,
            y_offset,
            x_offset,
            z_offset,
            conventional,
        }
    }

    /// Множитель направления ротаций `con = ±1` оракула: `+1` при
    /// конвенциональных направлениях, `−1` при дефолтных (противоположных).
    fn con(&self) -> f64 {
        if self.conventional {
            1.0
        } else {
            -1.0
        }
    }
}

/// Декартова поза + углы A/B/C в градусах (подмножество EmcPose оракула,
/// нужное для XYZAC/XYZBC).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pose6 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Угол A (градусы; для XYZBC — 0, см. [`xyzbc_forward`]).
    pub a: f64,
    /// Угол B (градусы; для XYZAC — 0, см. [`xyzac_forward`]).
    pub b: f64,
    pub c: f64,
}

impl Pose6 {
    pub fn new(x: f64, y: f64, z: f64, a: f64, b: f64, c: f64) -> Self {
        Self { x, y, z, a, b, c }
    }
}

/// Joints XYZAC: X/Y/Z линейные + A (стол) + C (ротор), углы в градусах.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Joints5 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub a: f64,
    pub c: f64,
}

impl Joints5 {
    pub fn new(x: f64, y: f64, z: f64, a: f64, c: f64) -> Self {
        Self { x, y, z, a, c }
    }
}

/// Joints XYZBC: X/Y/Z линейные + B (стол) + C (ротор), углы в градусах.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JointsBC {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub b: f64,
    pub c: f64,
}

impl JointsBC {
    pub fn new(x: f64, y: f64, z: f64, b: f64, c: f64) -> Self {
        Self { x, y, z, b, c }
    }
}

/// XYZAC forward: joints → декартова поза (формулы trtfuncs.c
/// `xyzac_forward`, факт; X = JX−xr, Y = JY−dy−yr, Z = JZ−dz−zr, углы в
/// градусах).
pub fn xyzac_forward(p: &TrtParams, j: &Joints5) -> Pose6 {
    let con = p.con();
    let (xr, yr, zr) = (p.rot_point[0], p.rot_point[1], p.rot_point[2]);
    let (dy, dz) = (p.y_offset, p.z_offset);
    let a = j.a.to_radians();
    let c = j.c.to_radians();
    let (ca, sa) = (a.cos(), a.sin());
    let (cc, sc) = (c.cos(), c.sin());

    let x = j.x - xr;
    let y = j.y - dy - yr;
    let z = j.z - dz - zr;

    Pose6 {
        x: cc * x - con * sc * ca * y + sc * sa * z - con * sc * dy + xr,
        y: con * sc * x + cc * ca * y - con * cc * sa * z + cc * dy + yr,
        z: con * sa * y + ca * z + dz + zr,
        a: j.a,
        b: 0.0,
        c: j.c,
    }
}

/// XYZAC inverse: декартова поза → joints (формулы trtfuncs.c
/// `xyzac_inverse`, факт; X' = x−xr, Y' = y−yr, Z' = z−zr).
pub fn xyzac_inverse(p: &TrtParams, pose: &Pose6) -> Joints5 {
    let con = p.con();
    let (xr, yr, zr) = (p.rot_point[0], p.rot_point[1], p.rot_point[2]);
    let (dy, dz) = (p.y_offset, p.z_offset);
    let a = pose.a.to_radians();
    let c = pose.c.to_radians();
    let (ca, sa) = (a.cos(), a.sin());
    let (cc, sc) = (c.cos(), c.sin());

    let x = pose.x - xr;
    let y = pose.y - yr;
    let z = pose.z - zr;

    Joints5 {
        x: cc * x + con * sc * y + xr,
        y: -con * sc * ca * x + cc * ca * y + con * sa * z - ca * dy - con * sa * dz + dy + yr,
        z: sc * sa * x - con * cc * sa * y + ca * z + con * sa * dy - ca * dz + dz + zr,
        a: pose.a,
        c: pose.c,
    }
}

/// XYZBC forward: joints → декартова поза (формулы trtfuncs.c
/// `xyzbc_forward`, факт; X = JX−dx−xr, Y = JY−yr, Z = JZ−dz−zr).
pub fn xyzbc_forward(p: &TrtParams, j: &JointsBC) -> Pose6 {
    let con = p.con();
    let (xr, yr, zr) = (p.rot_point[0], p.rot_point[1], p.rot_point[2]);
    let (dx, dz) = (p.x_offset, p.z_offset);
    let b = j.b.to_radians();
    let c = j.c.to_radians();
    let (cb, sb) = (b.cos(), b.sin());
    let (cc, sc) = (c.cos(), c.sin());

    let x = j.x - dx - xr;
    let y = j.y - yr;
    let z = j.z - dz - zr;

    Pose6 {
        x: cc * cb * x - con * sc * y + con * cc * sb * z + cc * dx + xr,
        y: con * sc * cb * x + cc * y + sc * sb * z + con * sc * dx + yr,
        z: -con * sb * x + cb * z + dz + zr,
        a: 0.0,
        b: j.b,
        c: j.c,
    }
}

/// XYZBC inverse: декартова поза → joints (формулы trtfuncs.c
/// `xyzbc_inverse`, факт; precomputed `dpx`/`dpz` — офсеты, «видимые» с
/// наклонённого стола: тот же поворот, что применяет forward).
pub fn xyzbc_inverse(p: &TrtParams, pose: &Pose6) -> JointsBC {
    let con = p.con();
    let (xr, yr, zr) = (p.rot_point[0], p.rot_point[1], p.rot_point[2]);
    let (dx, dz) = (p.x_offset, p.z_offset);
    let b = pose.b.to_radians();
    let c = pose.c.to_radians();
    let (cb, sb) = (b.cos(), b.sin());
    let (cc, sc) = (c.cos(), c.sin());

    // dpx/dpz: офсеты dx/dz, повёрнутые на −B (в системе стола).
    let dpx = -cb * dx + con * sb * dz + dx;
    let dpz = -con * sb * dx - cb * dz + dz;

    let x = pose.x - xr;
    let y = pose.y - yr;
    let z = pose.z - zr;

    JointsBC {
        x: cc * cb * x + con * sc * cb * y - con * sb * z + dpx + xr,
        y: -con * sc * x + cc * y + yr,
        z: con * cc * sb * x + sc * sb * y + cb * z + dpz + zr,
        b: pose.b,
        c: pose.c,
    }
}
