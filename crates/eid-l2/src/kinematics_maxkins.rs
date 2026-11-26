//! kinematics_maxkins — фаза 12.5.4, модуль 3: кинематика **head-table
//! intersecting** (B-качание на шпинделе над C-столом, пивоты на одной
//! вертикали — DMU 50-класс; вариант max5) + U/V/W-салазки.
//!
//! # Оракул формул (математические факты)
//!
//! LinuxCNC `src/emc/kinematics/maxkins.c` (автор C. Radek; GPL-2.0+),
//! master на 2026-11-26; sim-оракул
//! `configs/sim/axis/vismach/5axis/max5/` (maxkins.ini + max5gui.py).
//! Формулы перенесены как **математические факты**: выражения выписаны
//! самостоятельно на Rust (GPL-текст не копировался), сверены член-в-член
//! с C-источником (знаки/порядок 1:1) и с research
//! `kinematics_research.md` §2.2 (его численная верификация: fwd∘inv ≤
//! 4.3e-14 при con = −1, pivot+tool+w, u/v ≠ 0). Head-table вариант
//! Haas/Fanuc-стандарта — частный случай при u = v = w = 0 (u/v/w-ветки
//! выкидываются конфигурацией, формулы те же).
//!
//! # Геометрия оракула
//!
//! Голова (B, о machine Y, направление НЕстандартное — HAL-пин
//! `conventional-directions`, множитель `con = ±1`) висит на Z-салазках и
//! НЕ вращается с C-столом; C о machine Z через machine origin (полярная
//! форма: `ρ = hypot(mx,my)`, `θ = atan2(my,mx) ± rad(c)`). Джоинты
//! 0..8 = **X Y Z A B C U V W** по порядку (комментарий maxkins.c); A в
//! формулах не участвует — pass-through (как в оракуле). W входит в
//! радиус `R = pivot_length + tool_length + w`; U/V — сдвижки, вращаемые
//! с головой (xv = u·cos b по X, zv = u·sin b и v — по Y до поворота C).
//!
//! # Соглашения (как в оракуле)
//!
//! * Углы — в **градусах** на границе API (`d2r = π/180` оракула);
//!   линейные — единицы машины.
//! * [`DEFAULT_CONVENTIONAL_DIRECTIONS`] = `false` (дефолт оракула) ⇒
//!   `con = −1`; `true` ⇒ `con = +1` (макро `CON(p)` maxkins.c: любое
//!   ненулевое значение пина = конвенциональные направления).
//! * Forward: `θ = atan2(my,mx) + rad(c)`, inverse: `− rad(c)`; углы
//!   A/B/C и u/v/w проходят насквозь (pass-through) без нормализации.
//! * Inverse — точная алгебраическая инверсия forward (гипотенуза/угол
//!   сохраняются: `hypot` и `atan2` взаимно однозначны на паре (mx,my)).

/// Дефолт флага «конвенциональные направления ротаций» — как пин
/// `conventional-directions` оракула (maxkins.c): **false** ⇒ `con = −1`
/// (направление оси B нестандартное).
pub const DEFAULT_CONVENTIONAL_DIRECTIONS: bool = false;

/// Геометрия станка head-table intersecting (аналог блока HAL-пинов
/// `pivot-length`, `tool-length`, `conventional-directions` оракула).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MaxkinsParams {
    /// Длина пивота головы (HAL `pivot-length`; дефолт maxkins.c 0.666 —
    /// машина-специфично, в профиль/параметры не зашивается).
    pub pivot_length: f64,
    /// Длина инструмента (HAL `tool-length`).
    pub tool_length: f64,
    /// `false` (дефолт оракула) ⇒ `con = −1`; `true` ⇒ `con = +1`.
    pub conventional: bool,
}

impl MaxkinsParams {
    /// Множитель направления `con = ±1` (макро `CON(p)` оракула).
    fn con(&self) -> f64 {
        if self.conventional {
            1.0
        } else {
            -1.0
        }
    }
}

/// Joints maxkins: 9 букв X Y Z A B C U V W (порядок оракула «joints 0..8
/// are X..W»); углы в градусах. A в формулах не участвует — pass-through.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JointsMaxkins {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub u: f64,
    pub v: f64,
    pub w: f64,
}

/// Поза maxkins: те же 9 букв (pass-through A/B/C/U/V/W в обе стороны —
/// как в оракуле `pos->a = joints[3]; …`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PoseMaxkins {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub u: f64,
    pub v: f64,
    pub w: f64,
}

/// maxkins forward: joints → декартова поза (формулы maxkins.c
/// `max_forward`, факт; полярная форма C-поворота).
pub fn maxkins_forward(p: &MaxkinsParams, j: &JointsMaxkins) -> PoseMaxkins {
    let con = p.con();
    let b = j.b.to_radians();
    let (sb, cb) = (b.sin(), b.cos());
    let rad_c = j.c.to_radians();
    let big_r = p.pivot_length + p.tool_length + j.w;

    // Поправка головы B (радиус от пивота до кончика по наклонённой оси).
    let zb = big_r * cb;
    let xb = big_r * sb;
    // Поправка U-салазок (едет вдоль наклонённой оси головы).
    let zv = j.u * sb;
    let xv = j.u * cb;
    // Сдвиг до поворота C (V — чистый Y-сдвиг).
    let mx = j.x - con * xb - xv;
    let my = j.y - j.v;
    let xyr = mx.hypot(my);
    let xytheta = my.atan2(mx) + rad_c;

    PoseMaxkins {
        x: xyr * xytheta.cos(),
        y: xyr * xytheta.sin(),
        z: j.z - zb - con * zv + p.pivot_length + p.tool_length,
        a: j.a,
        b: j.b,
        c: j.c,
        u: j.u,
        v: j.v,
        w: j.w,
    }
}

/// maxkins inverse: декартова поза → joints (формулы maxkins.c
/// `max_inverse`, факт; `θ = atan2(Y,X) − rad(c)` — обратный поворот C).
pub fn maxkins_inverse(p: &MaxkinsParams, pose: &PoseMaxkins) -> JointsMaxkins {
    let con = p.con();
    let b = pose.b.to_radians();
    let (sb, cb) = (b.sin(), b.cos());
    let rad_c = pose.c.to_radians();
    let big_r = p.pivot_length + p.tool_length + pose.w;

    let zb = big_r * cb;
    let xb = big_r * sb;
    let zv = pose.u * sb;
    let xv = pose.u * cb;
    let xyr = pose.x.hypot(pose.y);
    let xytheta = pose.y.atan2(pose.x) - rad_c;

    JointsMaxkins {
        x: xyr * xytheta.cos() + con * xb + xv,
        y: xyr * xytheta.sin() + pose.v,
        z: pose.z + zb + con * zv - p.pivot_length - p.tool_length,
        a: pose.a,
        b: pose.b,
        c: pose.c,
        u: pose.u,
        v: pose.v,
        w: pose.w,
    }
}
