//! kinematics_tdr — фаза 12.5.4, модуль 1: кинематика **table-table TT-2
//! «cradle/trunnion»** (качающийся стол B снизу о machine-Y, ротор A сверху
//! на нём; деталь на A — Hermle-класс). Forward (joints → декартова поза)
//! и inverse (обратно) для XYZAB-tdr.
//!
//! # Оракул формул (математические факты)
//!
//! LinuxCNC `src/hal/components/xyzab_tdr_kins.comp` (автор D. Mueller;
//! GPL-2.0+), состояние master на 2026-11-26; sim-оракул
//! `configs/sim/axis/vismach/5axis/table-dual-rotary/`. Формулы перенесены
//! как **математические факты**: выражения выписаны самостоятельно на Rust
//! (GPL-текст не копировался), каждая формула сверена член-в-член с
//! comp-источником (знаки/порядок операций 1:1) и с research
//! `kinematics_research.md` §2.1 (его численная верификация: fwd∘inv ≤
//! 1.14e-13 на 1000 случайных конфигураций с dx/dz ≠ 0). Дополнительная
//! независимая академическая сверка: CAD&Applications 16(6) 2019,
//! Lin & Bohez, с.1192, Eqs (5)–(11) — та же композиция Ry(B)·Rx(A)
//! (их знаковая конвенция синусов противоположна — их примечание §2.2.2).
//!
//! # Квирк оракула: dx АЛГЕБРАИЧЕСКИ ИНЕРТЕН (воспроизведён 1:1)
//!
//! В comp `dx` (`geometry[P_XO]`) читается ТОЛЬКО в inverse (forward его
//! не видит вовсе — часть квирка, как в research §2.1). Но и в inverse dx
//! СОКРАЩАЕТСЯ: `qx = X − xr − dx` даёт множитель `−cb·dx` в j[0]
//! (`−sb·dx` в j[2]), а компенсационные члены `+cb·dx`/`+sb·dx`
//! гасят его ТОЧНО при любых углах. Итого `dx` — полностью инертный
//! параметр comp «как написано» (сильнее, чем формулировка research
//! «dx — офсет именно inverse-стороны»: РЕАЛЬНОСТЬ ПОБЕЖДАЕТ, проверено
//! алгебраически и тестом `tdr_dx_is_algebraically_inert` в
//! `tests/kinematics_topologies_tests.rs`). Это код-квирк оракула, НЕ
//! ошибка порта — формулы воспроизводятся 1:1, чтобы semantic_diff с
//! оракулом замыкался. Семантика буквы `dx` (лицо стола A) ИНАЯ, чем
//! `x_offset` trt-порта ([`crate::kinematics::TrtParams::x_offset`] —
//! пивот оси B в XYZBC-trt, там dx значим).
//!
//! # Геометрия (README sim-конфига)
//!
//! * [`TdrParams::rot_point`] — пивот A/B-сборки (HAL-пины
//!   `x-/y-/z-rot-point`); при dx/dz ≠ 0 rot-point остаётся на оси B.
//! * `dz` ([`TdrParams::z_offset`], HAL `z-offset`) и `dx`
//!   ([`TdrParams::x_offset`], HAL `x-offset`) — офсеты от rot-point до
//!   центра лица стола A.
//! * `dt` ([`TdrParams::tool_offset_z`]) = `tool.tran.z` оракула — вылет
//!   инструмента по Z; в comp это ОТДЕЛЬНОЕ слагаемое (не сумма в dz, в
//!   отличие от trt-порта, где tool-offset складывался вызывающей
//!   стороной в dz).
//!
//! # Матричная семантика (факт коэффициентов forward)
//!
//! Линейная часть forward (без пивота/офсетов) равна **Rx(a)·Ry(b)** в
//! стандартных правых матрицах (проверено покомпонентно и тестом
//! `tdr_forward_matches_textbook_matrices`):
//! `[[cb, 0, sb], [sa·sb, ca, −sa·cb], [−ca·sb, sa, ca·cb]]`.
//! B — нижняя/несущая ось (внутренний сомножитель), A — верхний ротор.
//! (research §1/§2.1 описывает вложение физической цепочки записью
//! Ry(b)·Rx(a); коэффициентная матрица кода — Rx(a)·Ry(b) — РЕАЛЬНОСТЬ
//! ПОБЕЖДАЕТ, для пары fwd/inv это эквивалентно.)
//!
//! # Соглашения (как в оракуле)
//!
//! * Углы — в **градусах** на границе API, внутри радианы
//!   (`f64::to_radians` ≡ `TO_RAD = π/180` оракула); линейные — единицы
//!   машины (мм). Знаки — как в comp, флага `conventional-directions` в
//!   comp НЕТ (в отличие от trt/maxkins).
//! * Углы A/B проходят насквозь без нормализации (pass-through); буква C
//!   вне `coordinates` ⇒ forward заполняет [`Pose6::c`] нулём.
//! * Inverse — точная алгебраическая инверсия forward (круговой инвариант
//!   с допуском ~1e-13, подтверждён решёткой в
//!   `tests/kinematics_topologies_tests.rs`).

use crate::kinematics::Pose6;

/// Геометрия станка TT-2 «cradle/trunnion» (аналог блока HAL-пинов
/// `x-/y-/z-rot-point`, `x-offset`, `z-offset` + `tool.tran.z` оракула).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TdrParams {
    /// Пивот A/B-сборки `[x, y, z]` (при dx/dz ≠ 0 остаётся на оси B).
    pub rot_point: [f64; 3],
    /// `dx` (HAL `x-offset`) — офсет rot-point → лицо стола A по X.
    /// **Квирк оракула**: алгебраически ИНЕРТЕН — forward его не читает,
    /// а в inverse компенсационные члены гасят `qx`-сдвиг точно (см.
    /// модульную документацию). Хранится/передаётся 1:1 с comp.
    pub x_offset: f64,
    /// `dz` (HAL `z-offset`) — офсет rot-point → лицо стола A по Z.
    pub z_offset: f64,
    /// `dt` = `tool.tran.z` оракула — вылет инструмента по Z (отдельное
    /// слагаемое, НЕ сумма в [`z_offset`](TdrParams::z_offset)).
    pub tool_offset_z: f64,
}

impl TdrParams {
    /// Геометрия с явно заданными полями (rot-point, dx, dz, dt).
    pub fn new(rot_point: [f64; 3], x_offset: f64, z_offset: f64, tool_offset_z: f64) -> Self {
        Self {
            rot_point,
            x_offset,
            z_offset,
            tool_offset_z,
        }
    }
}

/// Joints XYZAB-tdr: X/Y/Z линейные + A (верхний ротор, j[3]) + B
/// (качающийся стол, j[4]); углы в градусах.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JointsTdr {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub a: f64,
    pub b: f64,
}

impl JointsTdr {
    pub fn new(x: f64, y: f64, z: f64, a: f64, b: f64) -> Self {
        Self { x, y, z, a, b }
    }
}

/// XYZAB-tdr forward: joints → декартова поза (формулы comp
/// `xyzab_tdr_kins.comp`, факт; **dx в forward не входит** — квирк
/// оракула, см. модульную документацию).
pub fn tdr_forward(p: &TdrParams, j: &JointsTdr) -> Pose6 {
    let (xr, yr, zr) = (p.rot_point[0], p.rot_point[1], p.rot_point[2]);
    let (dz, dt) = (p.z_offset, p.tool_offset_z);
    let a = j.a.to_radians();
    let b = j.b.to_radians();
    let (sa, ca) = (a.sin(), a.cos());
    let (sb, cb) = (b.sin(), b.cos());

    let px = j.x - xr;
    let py = j.y - yr;
    let pz = j.z - zr - dt;

    Pose6 {
        x: cb * px + sb * pz + xr,
        y: sa * sb * px + ca * py - cb * sa * pz + sa * dz + yr,
        z: -ca * sb * px + sa * py + ca * cb * pz - ca * dz + zr + dz + dt,
        a: j.a,
        b: j.b,
        c: 0.0,
    }
}

/// XYZAB-tdr inverse: декартова поза → joints (формулы comp, факт; `dx`
/// входит только здесь — qx = X − xr − dx, и сокращается в паре fwd∘inv).
pub fn tdr_inverse(p: &TdrParams, pose: &Pose6) -> JointsTdr {
    let (xr, yr, zr) = (p.rot_point[0], p.rot_point[1], p.rot_point[2]);
    let (dx, dz, dt) = (p.x_offset, p.z_offset, p.tool_offset_z);
    let a = pose.a.to_radians();
    let b = pose.b.to_radians();
    let (sa, ca) = (a.sin(), a.cos());
    let (sb, cb) = (b.sin(), b.cos());

    let qx = pose.x - xr - dx;
    let qy = pose.y - yr;
    let qz = pose.z - zr - dz - dt;

    JointsTdr {
        x: cb * qx + sa * sb * qy - ca * sb * qz + cb * dx - sb * dz + xr,
        y: ca * qy + sa * qz + yr,
        z: sb * qx - sa * cb * qy + ca * cb * qz + sb * dx + cb * dz + zr + dt,
        a: pose.a,
        b: pose.b,
    }
}
