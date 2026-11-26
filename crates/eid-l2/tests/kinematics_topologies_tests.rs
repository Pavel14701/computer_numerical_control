//! Тесты фазы 12.5.4: оставшиеся топологии кинематики —
//! [`eid_l2::kinematics_tdr`] (TT-2 cradle/trunnion XYZAB),
//! [`eid_l2::kinematics_trsrn`] (head-head ν=90° + nutating),
//! [`eid_l2::kinematics_maxkins`] (head-table intersecting + UVW),
//! [`eid_l2::kinematics_5axis`] (gantry/bridge mill XYZBCW).
//!
//! Методика (прецедент kins-twp / паттерн 12.5.1 `kinematics_tests.rs`):
//! 1. **Круговой инвариант** `forward∘inverse ≡ identity` на решётке
//!    кейсов (≥ 64 на топологию, обе ноги inv∘fwd и fwd∘inv, допуск 1e-9
//!    относительный);
//! 2. **Независимая матричная сверка** с учебными поворотами (тождество
//!    коэффициентов forward);
//! 3. **Ручные зонд-кейсы** (hand-computed golden: как trt A=90°);
//! 4. **Квирки оракула зафиксированы** (tdr: forward не зависит от dx —
//!    код-квирк воспроизведён 1:1; maxkins: con-дефолт = нестандартные
//!    направления; 5axis: W-конвенция «negative increases radial motion»);
//! 5. **Вырожденности**: trsrn ν=90° ≡ Ry(θ2), ν=0° ≡ Rz(θ2), maxkins
//!    b=0 ≡ trt C-вращение; She 2007-сверка nutating-формулы (Родригес).

use eid_l2::kinematics::{xyzbc_forward, JointsBC, TrtParams};
use eid_l2::kinematics_5axis::{
    fiveaxis_forward, fiveaxis_inverse, tool_frame_machine, FiveAxisParams, JointsXyzbcw,
    PoseXyzbcw,
};
use eid_l2::kinematics_maxkins::{
    maxkins_forward, maxkins_inverse, JointsMaxkins, MaxkinsParams,
    DEFAULT_CONVENTIONAL_DIRECTIONS as MAXKINS_DEFAULT_CON,
};
use eid_l2::kinematics_tdr::{tdr_forward, tdr_inverse, JointsTdr, TdrParams};
use eid_l2::kinematics_trsrn::{
    nutating_secondary, tool_rotation, trsrn_angles_from_tool_axis, trsrn_tcp_forward,
    trsrn_tcp_inverse, JointsTsrn, Mat3, TrsrnParams,
};

/// Решётка углов (градусы): 0, ±30, ±90, 180, 359.
const ANGLES: [f64; 7] = [0.0, 30.0, -30.0, 90.0, -90.0, 180.0, 359.0];

/// Решётка позиций joints: нуль, крупная (±1e3), средняя отрицательная,
/// единичная.
const POSITIONS: [[f64; 3]; 4] = [
    [0.0, 0.0, 0.0],
    [1.0e3, -1.0e3, 0.0],
    [-500.0, 250.0, -750.0],
    [1.0, 2.0, 3.0],
];

/// Относительная близость: |a−b| ≤ rtol·max(1, |a|, |b|).
fn close_rel(a: f64, b: f64, rtol: f64) -> bool {
    (a - b).abs() <= rtol * a.abs().max(b.abs()).max(1.0)
}

fn assert_cart_close(got: [f64; 3], want: [f64; 3], tol: f64, ctx: &str) {
    for i in 0..3 {
        assert!(
            close_rel(got[i], want[i], tol),
            "{ctx}: comp[{i}] = {} != {}",
            got[i],
            want[i]
        );
    }
}

// ===========================================================================
// Общие матричные помощники (учебные правые матрицы — независимая сверка)
// ===========================================================================

fn rx(deg: f64) -> Mat3 {
    let r = deg.to_radians();
    let (s, c) = (r.sin(), r.cos());
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}

fn ry(deg: f64) -> Mat3 {
    let r = deg.to_radians();
    let (s, c) = (r.sin(), r.cos());
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}

fn rz(deg: f64) -> Mat3 {
    let r = deg.to_radians();
    let (s, c) = (r.sin(), r.cos());
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

fn mm(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut o = [[0.0f64; 3]; 3];
    for (i, row) in o.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    o
}

fn mv(m: &Mat3, v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// Опорные точки для матричной сверки (величины ≤ 3 ⇒ абс. ошибки ~1e-15).
const MATRIX_POINTS: [[f64; 3]; 4] = [
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
    [1.5, -2.5, 3.0],
];

// ===========================================================================
// МОДУЛЬ 1: kinematics_tdr — TT-2 cradle/trunnion XYZAB
// (оракул xyzab_tdr_kins.comp; квирк: forward не зависит от dx)
// ===========================================================================

/// Решётка геометрий tdr: (xr, yr, zr, dx, dz, dt) — нулевая / крупная /
/// отрицательная / смешанная.
const TDR_GEOMETRIES: [[f64; 6]; 4] = [
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [10.0, 10.0, 10.0, 10.0, 10.0, 10.0],
    [-5.0, -5.0, -5.0, -5.0, -5.0, -5.0],
    [0.0, 10.0, -5.0, 2.0, 10.0, 3.0],
];

fn tdr_params(g: &[f64; 6]) -> TdrParams {
    TdrParams::new([g[0], g[1], g[2]], g[3], g[4], g[5])
}

#[test]
fn tdr_circular_invariant_lattice() {
    let mut cases = 0usize;
    let mut max_err = 0.0f64;
    for g in &TDR_GEOMETRIES {
        let p = tdr_params(g);
        for &a in &ANGLES {
            for &b in &ANGLES {
                for pos in &POSITIONS {
                    let j0 = JointsTdr::new(pos[0], pos[1], pos[2], a, b);
                    let ctx = format!("tdr g={g:?} a={a} b={b} pos={pos:?}");
                    // нога 1: forward → inverse
                    let pose0 = tdr_forward(&p, &j0);
                    let j1 = tdr_inverse(&p, &pose0);
                    assert_eq!(j1.a, j0.a, "{ctx}: A не проходит насквозь");
                    assert_eq!(j1.b, j0.b, "{ctx}: B не проходит насквозь");
                    for (v0, v1) in [(j0.x, j1.x), (j0.y, j1.y), (j0.z, j1.z)] {
                        let err = (v1 - v0).abs();
                        max_err = max_err.max(err);
                        assert!(
                            close_rel(v1, v0, 1e-9),
                            "{ctx}: inv∘fwd err={err} ({v0} → {v1})"
                        );
                    }
                    // нога 2: inverse → forward
                    let pose1 = tdr_forward(&p, &j1);
                    assert_eq!(pose1.a, pose0.a, "{ctx}: A второго круга");
                    assert_eq!(pose1.b, pose0.b, "{ctx}: B второго круга");
                    assert_eq!(pose1.c, 0.0, "{ctx}: C (вне coordinates) = 0");
                    for (v0, v1) in [(pose0.x, pose1.x), (pose0.y, pose1.y), (pose0.z, pose1.z)] {
                        let err = (v1 - v0).abs();
                        max_err = max_err.max(err);
                        assert!(
                            close_rel(v1, v0, 1e-9),
                            "{ctx}: fwd∘inv err={err} ({v0} → {v1})"
                        );
                    }
                    cases += 1;
                }
            }
        }
    }
    assert!(cases >= 64, "решётка tdr слишком мала: {cases}");
    println!("tdr круговой инвариант: {cases} кейсов, max err = {max_err:e}");
}

#[test]
fn tdr_forward_matches_textbook_matrices() {
    // Тождество коэффициентов forward (без геометрии): M = Rx(a)·Ry(b) —
    // B (несущая, нижняя ось) — внутренний сомножитель, A (верхний ротор) —
    // внешний. Факт кода оракула; research §2.1 описывает вложение цепочки
    // записью Ry(b)·Rx(a) — коэффициентная матрица РЕАЛЬНОСТИ = Rx(a)·Ry(b)
    // (см. doc модуля; на пару fwd/inv не влияет).
    let p = TdrParams::default();
    let mut cases = 0usize;
    for &a in &ANGLES {
        for &b in &ANGLES {
            let m = mm(&rx(a), &ry(b));
            for pt in &MATRIX_POINTS {
                let want = mv(&m, *pt);
                let pose = tdr_forward(&p, &JointsTdr::new(pt[0], pt[1], pt[2], a, b));
                assert_cart_close(
                    [pose.x, pose.y, pose.z],
                    want,
                    1e-9,
                    &format!("tdr mat a={a} b={b} pt={pt:?}"),
                );
                cases += 1;
            }
        }
    }
    assert!(cases >= 64, "матричная решётка tdr мала: {cases}");
    println!("tdr матричная сверка: {cases} кейсов");
}

#[test]
fn tdr_hand_computed_anchors() {
    // (1) b=90°, a=0, без геометрии, joints (100,200,300): sb=1, cb=0 ⇒
    //     X = pz = 300, Y = py = 200, Z = −px = −100.
    let p = TdrParams::default();
    let r = tdr_forward(&p, &JointsTdr::new(100.0, 200.0, 300.0, 0.0, 90.0));
    assert_cart_close([r.x, r.y, r.z], [300.0, 200.0, -100.0], 1e-9, "tdr b=90");
    // (2) a=90°, b=0: X = px, Y = −pz, Z = py ⇒ (100, −300, 200).
    let r = tdr_forward(&p, &JointsTdr::new(100.0, 200.0, 300.0, 90.0, 0.0));
    assert_cart_close([r.x, r.y, r.z], [100.0, -300.0, 200.0], 1e-9, "tdr a=90");
    // (3) Полная геометрия: rot_point [10,20,30], dx=2, dz=5, dt=7,
    //     a=b=90°, joints (100,200,300). Ручной подсчёт по формулам:
    //     px=90, py=180, pz=263 ⇒ X=273, Y=115, Z=222; inverse возвращает
    //     joints (проверено тем же кругом ниже).
    let pg = TdrParams::new([10.0, 20.0, 30.0], 2.0, 5.0, 7.0);
    let rg = tdr_forward(&pg, &JointsTdr::new(100.0, 200.0, 300.0, 90.0, 90.0));
    assert_cart_close(
        [rg.x, rg.y, rg.z],
        [273.0, 115.0, 222.0],
        1e-9,
        "tdr якорь геом",
    );
    let back = tdr_inverse(&pg, &rg);
    assert_cart_close(
        [back.x, back.y, back.z],
        [100.0, 200.0, 300.0],
        1e-9,
        "tdr якорь inv",
    );
    assert_eq!((back.a, back.b), (90.0, 90.0));
}

#[test]
fn tdr_dx_is_algebraically_inert() {
    // Квирк оракула (xyzab_tdr_kins.comp), ЗАФИКСИРОВАН полностью:
    // forward dx не читает вовсе; в inverse `qx = X − xr − dx` даёт
    // множитель −cb·dx (−sb·dx в j[2]), который компенсационные члены
    // +cb·dx (+sb·dx) гасят ТОЧНО при любых углах ⇒ dx — полностью
    // инертный параметр comp «как написано» (сильнее формулировки
    // research §2.1 «dx — офсет именно inverse-стороны»: РЕАЛЬНОСТЬ
    // ПОБЕЖДАЕТ). Круг замыкается при ЛЮБОМ dx; результаты обеих сторон
    // совпадают (в пределах FP-округления ~ulp от порядка слагаемых).
    let p0 = TdrParams::new([10.0, 20.0, 30.0], 0.0, 5.0, 7.0);
    let p1 = TdrParams::new([10.0, 20.0, 30.0], 123.0, 5.0, 7.0);
    for &a in &ANGLES {
        for &b in &ANGLES {
            let j0 = JointsTdr::new(100.0, 200.0, 300.0, a, b);
            // forward: байт-в-байт одинаков (dx не читается вовсе).
            let r0 = tdr_forward(&p0, &j0);
            let r1 = tdr_forward(&p1, &j0);
            assert_eq!(r0, r1, "a={a} b={b}: forward зависит от dx (квирк сломан)");
            // inverse: алгебраически тот же результат (FP-расхождение
            // ~ulp допуском 1e-9), круг замыкается на обоих параметрах.
            let back0 = tdr_inverse(&p0, &r0);
            let back1 = tdr_inverse(&p1, &r0);
            assert_cart_close(
                [back1.x, back1.y, back1.z],
                [back0.x, back0.y, back0.z],
                1e-9,
                &format!("a={a} b={b}: inverse не инертен к dx"),
            );
            for back in [&back0, &back1] {
                assert_cart_close(
                    [back.x, back.y, back.z],
                    [j0.x, j0.y, j0.z],
                    1e-9,
                    &format!("a={a} b={b}: круг с dx≠0"),
                );
            }
        }
    }
}

#[test]
fn tdr_angles_passthrough_and_no_normalization() {
    let p = TdrParams::new([1.0, 2.0, 3.0], 2.0, 5.0, 7.0);
    let j = JointsTdr::new(10.0, 20.0, 30.0, 720.0, -90.0);
    let pose = tdr_forward(&p, &j);
    assert_eq!(pose.a, 720.0, "A не нормализуется");
    assert_eq!(pose.b, -90.0, "B не нормализуется");
    let back = tdr_inverse(&p, &pose);
    assert_eq!((back.a, back.b), (720.0, -90.0));
    // 360° периодичность декартовой части.
    let r0 = tdr_forward(&p, &JointsTdr::new(10.0, 20.0, 30.0, 45.0, 0.0));
    let r360 = tdr_forward(&p, &JointsTdr::new(10.0, 20.0, 30.0, 45.0, 360.0));
    assert_cart_close(
        [r360.x, r360.y, r360.z],
        [r0.x, r0.y, r0.z],
        1e-9,
        "tdr B=360 vs 0",
    );
}

// ===========================================================================
// МОДУЛЬ 2: kinematics_trsrn — head-head ν=90° + nutating (ν 0–90°)
// (оракулы xyzacb_trsrn.comp + НЕЗАВИСИМЫЙ python kins-twp)
// ===========================================================================

/// Решётка геометрий trsrn: (nut, Ly, Lz, Dx, Dy, y-rot-axis, z-rot-axis,
/// Dt) — нулевая / сходящаяся (пересекающиеся пивоты) / полная
/// non-intersecting (пивоты смещены от осей ротаций).
const TRSRN_GEOMETRIES: [[f64; 8]; 3] = [
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [90.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [45.0, 20.0, 110.0, 5.0, -7.0, 13.0, 95.0, 30.0],
];

fn trsrn_params(g: &[f64; 8]) -> TrsrnParams {
    TrsrnParams {
        nut_angle: g[0],
        y_pivot: g[1],
        z_pivot: g[2],
        x_offset: g[3],
        y_offset: g[4],
        y_rot_axis: g[5],
        z_rot_axis: g[6],
        tool_offset_z: g[7],
    }
}

#[test]
fn trsrn_tcp_circular_invariant_lattice() {
    let mut cases = 0usize;
    let mut max_err = 0.0f64;
    for g in &TRSRN_GEOMETRIES {
        let p = trsrn_params(g);
        for &a in &ANGLES {
            for &b in &ANGLES {
                for &c in &ANGLES {
                    for pos in &POSITIONS {
                        let j0 = JointsTsrn::new(pos[0], pos[1], pos[2], a, b, c);
                        let ctx = format!("trsrn g={g:?} a={a} b={b} c={c} pos={pos:?}");
                        // нога 1: forward → inverse
                        let pose0 = trsrn_tcp_forward(&p, &j0);
                        let j1 = trsrn_tcp_inverse(&p, &pose0);
                        assert_eq!(
                            (j1.a, j1.b, j1.c),
                            (j0.a, j0.b, j0.c),
                            "{ctx}: углы не проходят насквозь"
                        );
                        for (v0, v1) in [(j0.x, j1.x), (j0.y, j1.y), (j0.z, j1.z)] {
                            let err = (v1 - v0).abs();
                            max_err = max_err.max(err);
                            assert!(
                                close_rel(v1, v0, 1e-9),
                                "{ctx}: inv∘fwd err={err} ({v0} → {v1})"
                            );
                        }
                        // нога 2: inverse → forward
                        let pose1 = trsrn_tcp_forward(&p, &j1);
                        assert_eq!(
                            (pose1.a, pose1.b, pose1.c),
                            (pose0.a, pose0.b, pose0.c),
                            "{ctx}: углы второго круга"
                        );
                        for (v0, v1) in [(pose0.x, pose1.x), (pose0.y, pose1.y), (pose0.z, pose1.z)]
                        {
                            let err = (v1 - v0).abs();
                            max_err = max_err.max(err);
                            assert!(
                                close_rel(v1, v0, 1e-9),
                                "{ctx}: fwd∘inv err={err} ({v0} → {v1})"
                            );
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert!(cases >= 64, "решётка trsrn слишком мала: {cases}");
    println!("trsrn круговой инвариант: {cases} кейсов, max err = {max_err:e}");
}

#[test]
fn trsrn_nutating_secondary_matches_rodrigues() {
    // Независимая She 2007-сверка (Rodrigues): Rs(ν, θ2) — поворот θ2 о
    // оси нутации k = (0, Sv, Cv) по формуле Родрига
    // R = cosθ·I + sinθ·[k]× + (1−cosθ)·kkᵀ. Сетка ν × θ2.
    let nuts = [0.0f64, 15.0, 30.0, 45.0, 60.0, 90.0];
    let thetas = [0.0f64, 30.0, 90.0, 135.0, 180.0, -90.0];
    let mut cases = 0usize;
    for &nut in &nuts {
        let v = nut.to_radians();
        let (sv, cv) = (v.sin(), v.cos());
        for &th2 in &thetas {
            let t = th2.to_radians();
            let (s, c) = (t.sin(), t.cos());
            // Rodrigues о оси (0, Sv, Cv).
            let mut want = [[0.0f64; 3]; 3];
            for (i, row) in want.iter_mut().enumerate() {
                for (j, cell) in row.iter_mut().enumerate() {
                    *cell = if i == j { c } else { 0.0 };
                }
            }
            let k = [0.0f64, sv, cv];
            for i in 0..3 {
                for j in 0..3 {
                    // [k]×: [[0,−kz,ky],[kz,0,−kx],[−ky,kx,0]]
                    let skew = match (i, j) {
                        (0, 1) => -k[2],
                        (0, 2) => k[1],
                        (1, 0) => k[2],
                        (1, 2) => -k[0],
                        (2, 0) => -k[1],
                        (2, 1) => k[0],
                        _ => 0.0,
                    };
                    want[i][j] += s * skew + (1.0 - c) * k[i] * k[j];
                }
            }
            let got = nutating_secondary(nut, th2);
            for i in 0..3 {
                for j in 0..3 {
                    assert!(
                        (got[i][j] - want[i][j]).abs() < 1e-12,
                        "ν={nut} θ2={th2} [{i}][{j}]: {} != {}",
                        got[i][j],
                        want[i][j]
                    );
                }
            }
            // Ортогональность R·Rᵀ = I.
            for i in 0..3 {
                for j in 0..3 {
                    let dot = (0..3).map(|kk| got[i][kk] * got[j][kk]).sum::<f64>();
                    let want_dot = if i == j { 1.0 } else { 0.0 };
                    assert!(
                        (dot - want_dot).abs() < 1e-12,
                        "ν={nut} θ2={th2}: не ортогональна [{i}][{j}]"
                    );
                }
            }
            cases += 1;
        }
    }
    assert!(cases >= 24, "Rodrigues-решётка мала: {cases}");
    println!("trsrn Rodrigues/She-сверка: {cases} кейсов");
}

#[test]
fn trsrn_nut90_degenerates_to_ry() {
    // Research §2.3: при ν = 90° (Cv = 0) Rs покомпонентно ≡ Ry(θ2) —
    // обычная ортогональная head-head голова (проверено оракулом численно,
    // здесь — независимо на решётке).
    for &th2 in &ANGLES {
        let got = nutating_secondary(90.0, th2);
        let want = ry(th2);
        for i in 0..3 {
            for j in 0..3 {
                assert!(
                    (got[i][j] - want[i][j]).abs() < 1e-12,
                    "ν=90 θ2={th2} [{i}][{j}]: {} != {}",
                    got[i][j],
                    want[i][j]
                );
            }
        }
    }
}

#[test]
fn trsrn_nut0_degenerates_to_rz() {
    // ν = 0° (Sv = 0): ось нутации = (0,0,1) ⇒ Rs ≡ Rz(θ2) (второй
    // документированный вырожденный предел nutating-формулы).
    for &th2 in &ANGLES {
        let got = nutating_secondary(0.0, th2);
        let want = rz(th2);
        for i in 0..3 {
            for j in 0..3 {
                assert!(
                    (got[i][j] - want[i][j]).abs() < 1e-12,
                    "ν=0 θ2={th2} [{i}][{j}]: {} != {}",
                    got[i][j],
                    want[i][j]
                );
            }
        }
    }
}

#[test]
fn trsrn_angle_solver_round_trip_grid() {
    // Python-оракул: Kz = 3-й столбец R = Rp·Rs·Rtc (от tc НЕ зависит);
    // solver восстанавливает (θ1, θ2) главные ветви точно. Сетка
    // ν ∈ {30,45,60,90} × θ1 ∈ [−90°,90°] × θ2 ∈ [0°,180°] × tc ∈ {0,37}.
    let nuts = [30.0f64, 45.0, 60.0, 90.0];
    let th1s = [-80.0f64, -30.0, 0.0, 30.0, 80.0];
    let th2s = [10.0f64, 45.0, 90.0, 135.0, 170.0];
    let mut cases = 0usize;
    let mut recovered = 0usize;
    for &nut in &nuts {
        for &th1 in &th1s {
            for &th2 in &th2s {
                for &tc in &[0.0f64, 37.0] {
                    let r = tool_rotation(nut, th1, th2, tc);
                    let kz = [r[0][2], r[1][2], r[2][2]];
                    // Единичность 3-го столбца (ось инструмента).
                    let len = (kz[0] * kz[0] + kz[1] * kz[1] + kz[2] * kz[2]).sqrt();
                    assert!(
                        (len - 1.0).abs() < 1e-12,
                        "ν={nut} θ1={th1} θ2={th2}: |Kz|={len}"
                    );
                    let (got1, got2) = trsrn_angles_from_tool_axis(nut, kz)
                        .unwrap_or_else(|| panic!("ν={nut} θ1={th1} θ2={th2}: недостижима"));
                    assert!(
                        (got1 - th1).abs() < 1e-9 && (got2 - th2).abs() < 1e-9,
                        "ν={nut} tc={tc}: ({th1},{th2}) → ({got1},{got2})"
                    );
                    recovered += 1;
                    cases += 1;
                }
            }
        }
    }
    assert!(cases >= 64, "solver-решётка мала: {cases}");
    assert_eq!(cases, recovered, "все ориентации должны восстанавливаться");
    println!("trsrn solver round-trip: {cases}/{recovered} ориентаций");
}

#[test]
fn trsrn_angle_solver_vertical_unreachable_and_branches() {
    // Вертикаль: Kzz > 1 − ε ⇒ (0, 0) при ЛЮБОМ ν (бесконечно решений,
    // одно явно — конвенция python-оракула).
    for &nut in &[0.0f64, 30.0, 45.0, 60.0, 90.0] {
        assert_eq!(
            trsrn_angles_from_tool_axis(nut, [0.0, 0.0, 1.0]),
            Some((0.0, 0.0)),
            "ν={nut}: вертикаль"
        );
    }
    // Недостижимость: Kzz < 2·Cv² − 1 ⇒ None. При ν=45°: Cv² = 0.5,
    // порог 0 ⇒ всё ниже горизонта недостижимо (нутация ограничивает
    // отрицательный наклон).
    assert_eq!(trsrn_angles_from_tool_axis(45.0, [0.0, 0.0, -0.5]), None);
    assert_eq!(trsrn_angles_from_tool_axis(45.0, [0.0, 0.0, -1.0]), None);
    // ν=90°: порог 2·0 − 1 = −1 ⇒ вся сфера достижима (ортогональная
    // голова наклоняется на ±180°).
    assert!(trsrn_angles_from_tool_axis(90.0, [0.0, 0.0, -1.0]).is_some());
    // Граница достижимости: ровно на пороге (θ2 = 180°) — достижима.
    let cv = 45.0f64.to_radians().cos();
    let edge = 2.0 * cv * cv - 1.0;
    assert!(trsrn_angles_from_tool_axis(45.0, [0.0, 0.0, edge]).is_some());
    // Альтернативные ветви (python-оракул: ±θ2 и π−θ1 — те же кандидаты):
    // (а) π−θ1-зеркало даёт ТОТ ЖЕ Kzz-сектор и solver возвращает главную
    //     ветвь θ1 (sin зеркала = sin θ1);
    let (th1, th2, tc, nut) = (30.0f64, 60.0, 0.0, 45.0);
    let kz_main = {
        let r = tool_rotation(nut, th1, th2, tc);
        [r[0][2], r[1][2], r[2][2]]
    };
    let kz_mirror = {
        let r = tool_rotation(nut, 180.0 - th1, th2, tc);
        [r[0][2], r[1][2], r[2][2]]
    };
    // Kz зеркала = Rz(180°−θ1)·v против Rz(θ1)·v — разные векторы, но
    // solver восстанавливает из обоих ОДНУ главную ветвь θ1.
    let got_main = trsrn_angles_from_tool_axis(nut, kz_main).unwrap();
    let got_mirror = trsrn_angles_from_tool_axis(nut, kz_mirror).unwrap();
    assert!(
        (got_main.0 - th1).abs() < 1e-9 && (got_main.1 - th2).abs() < 1e-9,
        "главная ветвь: {got_main:?} != ({th1},{th2})"
    );
    assert!(
        (got_mirror.0 - th1).abs() < 1e-9,
        "π−θ1-ветвь: solver даёт главную θ1 ({})",
        got_mirror.0
    );
    // (б) ±θ2-кандидаты: одинаковый Kzz (acos-главная ветвь [0°,180°]).
    let kz_plus = { tool_rotation(nut, th1, th2, tc)[2][2] };
    let kz_minus = { tool_rotation(nut, th1, -th2, tc)[2][2] };
    assert!(
        (kz_plus - kz_minus).abs() < 1e-12,
        "±θ2: Kzz должен совпадать ({kz_plus} vs {kz_minus})"
    );
}

#[test]
fn trsrn_hand_computed_anchors() {
    // (1) Нулевая геометрия, нулевые углы: тождество (Px,Py,Pz → Q).
    let p0 = TrsrnParams::default();
    let r = trsrn_tcp_forward(&p0, &JointsTsrn::new(100.0, 200.0, 300.0, 0.0, 0.0, 0.0));
    assert_cart_close(
        [r.x, r.y, r.z],
        [100.0, 200.0, 300.0],
        1e-12,
        "trsrn тождество",
    );
    // (2) Нулевая геометрия, ν=90°, a=90° (стол), b=30° (secondary),
    //     c=90° (primary), joints (100,200,300): Sv=1, Cv=0 ⇒ r=s=Cs,
    //     t=0, SvSs=Ss, CvSs=0; Cw=Sw=... a=90°: Sw=1, Cw=0; c=90°:
    //     Sp=1, Cp=0. Ручной подсчёт: Q = (100, −300, 200).
    let p90 = TrsrnParams {
        nut_angle: 90.0,
        ..TrsrnParams::default()
    };
    let r = trsrn_tcp_forward(
        &p90,
        &JointsTsrn::new(100.0, 200.0, 300.0, 90.0, 30.0, 90.0),
    );
    assert_cart_close(
        [r.x, r.y, r.z],
        [100.0, -300.0, 200.0],
        1e-12,
        "trsrn якорь ν=90",
    );
    let back = trsrn_tcp_inverse(&p90, &r);
    assert_cart_close(
        [back.x, back.y, back.z],
        [100.0, 200.0, 300.0],
        1e-12,
        "trsrn якорь inv",
    );
    // (3) Тот же кейс при ν=45°: Sw=1, Cw=0; Sp=1, Cp=0; ν=45°: Sv=Cv=√2/2,
    //     b=30°: Ss=0.5, Cs=√3/2. Qy: −Cw·(...) = 0; (Draz−Pz)·Sw = −300·1;
    //     прочие члены с Cp/Cw/Dx/Dy/Ly = 0 ⇒ Qy = −300. Qz: −Cp·Dy·Sw=0;
    //     −Cw(...)=0; −(...)·dtlz=0; (...)·Ly=0; −(Dray−Py)·Sw = +200;
    //     Draz=Dt=Lz=0 ⇒ Qz = 200. Qx = 100 (все члены нулевые кроме Px).
    //     Тот же результат, что и ν=90° — трансляция при нулевой геометрии
    //     не зависит от ν (нутация входит только через геометрию головы).
    let p45 = TrsrnParams {
        nut_angle: 45.0,
        ..TrsrnParams::default()
    };
    let r45 = trsrn_tcp_forward(
        &p45,
        &JointsTsrn::new(100.0, 200.0, 300.0, 90.0, 30.0, 90.0),
    );
    assert_cart_close(
        [r45.x, r45.y, r45.z],
        [r.x, r.y, r.z],
        1e-12,
        "trsrn ν-инвариантность при нулевой геометрии",
    );
}

// ===========================================================================
// МОДУЛЬ 3: kinematics_maxkins — head-table intersecting (DMU 50-класс)
// (оракул maxkins.c; joints 0..8 = X Y Z A B C U V W)
// ===========================================================================

/// Решётка геометрий maxkins: (pivot, tool, u, v, w) — нулевая / полная
/// (голова + салазки) / только-голова (Haas/Fanuc-вариант: u=v=w=0) /
/// отрицательный вылет.
const MAXKINS_GEOMETRIES: [[f64; 5]; 4] = [
    [0.0, 0.0, 0.0, 0.0, 0.0],
    [250.0, 120.0, 30.0, -20.0, 15.0],
    [100.0, 50.0, 0.0, 0.0, 0.0],
    [66.6, 10.0, -40.0, 25.0, -5.0],
];

fn maxkins_joints(pos: [f64; 3], a: f64, b: f64, c: f64, g: &[f64; 5]) -> JointsMaxkins {
    JointsMaxkins {
        x: pos[0],
        y: pos[1],
        z: pos[2],
        a,
        b,
        c,
        u: g[2],
        v: g[3],
        w: g[4],
    }
}

#[test]
fn maxkins_circular_invariant_lattice() {
    let mut cases = 0usize;
    let mut max_err = 0.0f64;
    for &conventional in &[false, true] {
        for g in &MAXKINS_GEOMETRIES {
            let p = MaxkinsParams {
                pivot_length: g[0],
                tool_length: g[1],
                conventional,
            };
            for &b in &ANGLES {
                for &c in &ANGLES {
                    for pos in &POSITIONS {
                        let j0 = maxkins_joints(*pos, 0.0, b, c, g);
                        let ctx =
                            format!("maxkins con={conventional} g={g:?} b={b} c={c} pos={pos:?}");
                        // нога 1: forward → inverse
                        let pose0 = maxkins_forward(&p, &j0);
                        let j1 = maxkins_inverse(&p, &pose0);
                        assert_eq!(
                            (j1.a, j1.b, j1.c, j1.u, j1.v, j1.w),
                            (j0.a, j0.b, j0.c, j0.u, j0.v, j0.w),
                            "{ctx}: ротации/UVW не проходят насквозь"
                        );
                        for (v0, v1) in [(j0.x, j1.x), (j0.y, j1.y), (j0.z, j1.z)] {
                            let err = (v1 - v0).abs();
                            max_err = max_err.max(err);
                            assert!(
                                close_rel(v1, v0, 1e-9),
                                "{ctx}: inv∘fwd err={err} ({v0} → {v1})"
                            );
                        }
                        // нога 2: inverse → forward
                        let pose1 = maxkins_forward(&p, &j1);
                        for (v0, v1) in [(pose0.x, pose1.x), (pose0.y, pose1.y), (pose0.z, pose1.z)]
                        {
                            let err = (v1 - v0).abs();
                            max_err = max_err.max(err);
                            assert!(
                                close_rel(v1, v0, 1e-9),
                                "{ctx}: fwd∘inv err={err} ({v0} → {v1})"
                            );
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert!(cases >= 64, "решётка maxkins слишком мала: {cases}");
    println!("maxkins круговой инвариант: {cases} кейсов, max err = {max_err:e}");
}

#[test]
fn maxkins_hand_computed_anchors() {
    // Ручной якорь (полярная форма): pivot=100, tool=10, w=5 ⇒ R=115;
    // con=−1 (дефолт), u=30, v=20; b=90° (zb=0, xb=115), c=90°;
    // joints (200, 50, 300):
    //   mx = 200 − (−1)·115 − 30·cos90° = 315; my = 50 − 20 = 30;
    //   θ = atan2(30, 315) + 90°; X = −ρ·sin θ0 = −my = −30;
    //   Y = ρ·cos θ0 = mx = 315; zv = 30·1 = 30;
    //   Z = 300 − 0 − (−1)·30 + 110 = 440.
    let p = MaxkinsParams {
        pivot_length: 100.0,
        tool_length: 10.0,
        conventional: false,
    };
    let j = JointsMaxkins {
        x: 200.0,
        y: 50.0,
        z: 300.0,
        a: 0.0,
        b: 90.0,
        c: 90.0,
        u: 30.0,
        v: 20.0,
        w: 5.0,
    };
    let r = maxkins_forward(&p, &j);
    assert_cart_close(
        [r.x, r.y, r.z],
        [-30.0, 315.0, 440.0],
        1e-9,
        "maxkins якорь",
    );
    let back = maxkins_inverse(&p, &r);
    assert_cart_close(
        [back.x, back.y, back.z],
        [200.0, 50.0, 300.0],
        1e-9,
        "maxkins якорь inv",
    );
    // Нулевые углы, Haas-вариант (u=v=w=0, pivot=tool=100): тождество
    // с вертикальной головой — Z = JZ (−w = 0).
    let ph = MaxkinsParams {
        pivot_length: 100.0,
        tool_length: 100.0,
        conventional: true,
    };
    let r0 = maxkins_forward(
        &ph,
        &JointsMaxkins {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            a: 0.0,
            b: 0.0,
            c: 0.0,
            u: 0.0,
            v: 0.0,
            w: 0.0,
        },
    );
    assert_cart_close(
        [r0.x, r0.y, r0.z],
        [1.0, 2.0, 3.0],
        1e-12,
        "maxkins вертикальная голова",
    );
}

#[test]
fn maxkins_degenerate_b0_matches_trt_c_rotation() {
    // Research §1 verdict (в) для maxkins: «degenerate-сверка с
    // trt-вращением». При b=0, u=v=w=0, con=+1 голова вертикальна и
    // forward = поворот XY о machine Z на +c + Z-сдвиг −w — ТОТ ЖЕ
    // эффект, что C-ротор trt-порта при conventional=true и нулевой
    // геометрии (pose = Rz(c)·Rx(0)·joints). Сверяем позы на решётке.
    let pm = MaxkinsParams {
        pivot_length: 0.0,
        tool_length: 0.0,
        conventional: true,
    };
    let pt = TrtParams::new([0.0; 3], 0.0, 0.0, 0.0, true);
    let mut cases = 0usize;
    for &c in &ANGLES {
        for pos in &POSITIONS {
            let jm = maxkins_joints(*pos, 0.0, 0.0, c, &[0.0; 5]);
            let rm = maxkins_forward(&pm, &jm);
            let rt = xyzbc_forward(&pt, &JointsBC::new(pos[0], pos[1], pos[2], 0.0, c));
            assert_cart_close(
                [rm.x, rm.y, rm.z],
                [rt.x, rt.y, rt.z],
                1e-9,
                &format!("maxkins(b=0) vs trt(b=0) c={c} pos={pos:?}"),
            );
            cases += 1;
        }
    }
    assert!(cases >= 28, "degenerate-сверка мала: {cases}");
}

#[test]
fn maxkins_default_con_directions_matches_oracle() {
    // Пин-контракт оракула: conventional-directions дефолт false
    // (MAXKINS_DEFAULT_CON, значение зафиксировано в src) ⇒ con=−1
    // (направление оси B нестандартное). Поведенческая фиксация ниже:
    // при b=90° (R>0, c=0, u=v=w=0) con=−1 ⇒ mx = JX + R.
    let def = MaxkinsParams {
        pivot_length: 100.0,
        tool_length: 0.0,
        conventional: MAXKINS_DEFAULT_CON,
    };
    let r = maxkins_forward(
        &def,
        &JointsMaxkins {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            a: 0.0,
            b: 90.0,
            c: 0.0,
            u: 0.0,
            v: 0.0,
            w: 0.0,
        },
    );
    // Z = JZ − zb + pivot + tool = 0 − 0 + 100 (постоянный сдвиг головы).
    assert_cart_close(
        [r.x, r.y, r.z],
        [100.0, 0.0, 100.0],
        1e-9,
        "дефолт con=−1: X=JX+R",
    );
    // Явный conventional=true переворачивает: X = JX − R (Z тот же: zv=0).
    let conv = MaxkinsParams {
        conventional: true,
        ..def
    };
    let rc = maxkins_forward(
        &conv,
        &JointsMaxkins {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            a: 0.0,
            b: 90.0,
            c: 0.0,
            u: 0.0,
            v: 0.0,
            w: 0.0,
        },
    );
    assert_cart_close(
        [rc.x, rc.y, rc.z],
        [-100.0, 0.0, 100.0],
        1e-9,
        "con=+1: X=JX−R",
    );
}

// ===========================================================================
// МОДУЛЬ 4: kinematics_5axis — gantry/bridge mill XYZBCW
// (оракул 5axiskins.c; W — виртуальная ось вдоль инструмента)
// ===========================================================================

/// Решётка геометрий 5axis: (pivot, tool).
const FIVEAXIS_GEOMETRIES: [[f64; 2]; 2] = [[0.0, 0.0], [250.0, 120.0]];

/// W-решётка (включая конвенцию оракула «negative values increase tool
/// radial motion» — отрицательный W удлиняет вылет).
const W_VALUES: [f64; 4] = [0.0, 10.0, -5.0, 100.0];

/// B-решётка 5axis (полный полярный диапазон: B — угол от −Z).
const B_ANGLES: [f64; 7] = [0.0, 30.0, 60.0, 90.0, 120.0, 150.0, 180.0];

fn fiveaxis_joints(pos: [f64; 3], b: f64, c: f64, w: f64) -> JointsXyzbcw {
    JointsXyzbcw {
        x: pos[0],
        y: pos[1],
        z: pos[2],
        b,
        c,
        w,
    }
}

#[test]
fn fiveaxis_circular_invariant_lattice() {
    let mut cases = 0usize;
    let mut max_err = 0.0f64;
    for g in &FIVEAXIS_GEOMETRIES {
        let p = FiveAxisParams {
            pivot_length: g[0],
            tool_length: g[1],
        };
        for &b in &B_ANGLES {
            for &c in &ANGLES {
                for &w in &W_VALUES {
                    for pos in &POSITIONS {
                        let j0 = fiveaxis_joints(*pos, b, c, w);
                        let ctx = format!("5axis g={g:?} b={b} c={c} w={w} pos={pos:?}");
                        // нога 1: forward → inverse
                        let pose0 = fiveaxis_forward(&p, &j0);
                        let j1 = fiveaxis_inverse(&p, &pose0);
                        assert_eq!(
                            (j1.b, j1.c, j1.w),
                            (j0.b, j0.c, j0.w),
                            "{ctx}: B/C/W не проходят насквозь"
                        );
                        for (v0, v1) in [(j0.x, j1.x), (j0.y, j1.y), (j0.z, j1.z)] {
                            let err = (v1 - v0).abs();
                            max_err = max_err.max(err);
                            assert!(
                                close_rel(v1, v0, 1e-9),
                                "{ctx}: inv∘fwd err={err} ({v0} → {v1})"
                            );
                        }
                        // нога 2: inverse → forward
                        let pose1 = fiveaxis_forward(&p, &j1);
                        for (v0, v1) in [(pose0.x, pose1.x), (pose0.y, pose1.y), (pose0.z, pose1.z)]
                        {
                            let err = (v1 - v0).abs();
                            max_err = max_err.max(err);
                            assert!(
                                close_rel(v1, v0, 1e-9),
                                "{ctx}: fwd∘inv err={err} ({v0} → {v1})"
                            );
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert!(cases >= 64, "решётка 5axis слишком мала: {cases}");
    println!("5axis круговой инвариант: {cases} кейсов, max err = {max_err:e}");
}

#[test]
fn fiveaxis_hand_computed_anchors() {
    // (1) Нулевая геометрия, нулевые углы: тождество (w=0 ⇒ r=0).
    let p0 = FiveAxisParams::default();
    let r = fiveaxis_forward(&p0, &fiveaxis_joints([100.0, 200.0, 300.0], 0.0, 0.0, 0.0));
    assert_cart_close(
        [r.x, r.y, r.z],
        [100.0, 200.0, 300.0],
        1e-12,
        "5axis тождество",
    );
    // (2) pivot=100, tool=10, w=5 ⇒ R=115; b=90°, c=0°: r = (115, 0, 0)
    //     (s2r(115, 0, 90)); joints (200,50,300) → (315, 50, 410).
    let p = FiveAxisParams {
        pivot_length: 100.0,
        tool_length: 10.0,
    };
    let r = fiveaxis_forward(&p, &fiveaxis_joints([200.0, 50.0, 300.0], 90.0, 0.0, 5.0));
    assert_cart_close(
        [r.x, r.y, r.z],
        [315.0, 50.0, 410.0],
        1e-9,
        "5axis якорь b=90 c=0",
    );
    // (3) b=90°, c=90°: r = (115·cos90°, 115·sin90°, 0) = (0, 115, 0)
    //     → (200, 165, 410).
    let r = fiveaxis_forward(&p, &fiveaxis_joints([200.0, 50.0, 300.0], 90.0, 90.0, 5.0));
    assert_cart_close(
        [r.x, r.y, r.z],
        [200.0, 165.0, 410.0],
        1e-9,
        "5axis якорь b=90 c=90",
    );
    // (4) b=0°, c=0°, w=10, R=120: r = (0, 0, −120) → Z = 300 + 110 − 120
    //     = 290 (конвенция W: положительный w УКОРАЧИВАЕТ вылет кончика
    //     вниз; «negative values increase tool radial motion» оракула).
    let r = fiveaxis_forward(&p, &fiveaxis_joints([200.0, 50.0, 300.0], 0.0, 0.0, 10.0));
    assert_cart_close(
        [r.x, r.y, r.z],
        [200.0, 50.0, 290.0],
        1e-9,
        "5axis W-конвенция",
    );
    // Круг на якорях.
    for (b, c, w) in [(90.0f64, 0.0, 5.0), (90.0, 90.0, 5.0), (0.0, 0.0, 10.0)] {
        let pose = fiveaxis_forward(&p, &fiveaxis_joints([200.0, 50.0, 300.0], b, c, w));
        let back = fiveaxis_inverse(&p, &pose);
        assert_cart_close(
            [back.x, back.y, back.z],
            [200.0, 50.0, 300.0],
            1e-9,
            &format!("5axis якорь inv b={b} c={c} w={w}"),
        );
    }
}

#[test]
fn fiveaxis_tool_frame_matches_rz_ry_product() {
    // Tool-frame оракула: Rz(C)·Ry(−B) («tilt left handed about y»);
    // 3-й столбец = ось инструмента от кончика к державке
    // (−sb·cc, −sb·sc, cb). Независимая сверка с учебными матрицами.
    let mut cases = 0usize;
    for &b in &B_ANGLES {
        for &c in &ANGLES {
            let got = tool_frame_machine(b, c);
            let want = mm(&rz(c), &ry(-b));
            for i in 0..3 {
                for j in 0..3 {
                    assert!(
                        (got[i][j] - want[i][j]).abs() < 1e-12,
                        "b={b} c={c} [{i}][{j}]: {} != {}",
                        got[i][j],
                        want[i][j]
                    );
                }
            }
            // 3-й столбец — единичный.
            let col = [got[0][2], got[1][2], got[2][2]];
            let len = (col[0] * col[0] + col[1] * col[1] + col[2] * col[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-12, "b={b} c={c}: |col3|={len}");
            cases += 1;
        }
    }
    assert!(cases >= 49, "tool-frame решётка мала: {cases}");
    // Вырожденный якорь: b=c=0 ⇒ единичная матрица (вертикальный шпиндель).
    let ident = tool_frame_machine(0.0, 0.0);
    for (i, row) in ident.iter().enumerate() {
        for (j, cell) in row.iter().enumerate() {
            assert!((*cell - if i == j { 1.0 } else { 0.0 }).abs() < 1e-12);
        }
    }
}

#[test]
fn fiveaxis_inverse_jacobian_matches_c_source() {
    // Якобиан ОРАКУЛА (5axiskins.c fiveaxis_jacobian) — производные
    // INVERSE-отображения (J = P − r) по (X,Y,Z,B,C,W); сверка с
    // численным центральным дифференцированием порта (независимая
    // верификация формул §2.4):
    //   dJx/db = −R·cb·cc·d2r   dJx/dc =  R·sb·sc·d2r   dJx/dw = −sb·cc
    //   dJy/db = −R·cb·sc·d2r   dJy/dc = −R·sb·cc·d2r   dJy/dw = −sb·sc
    //   dJz/db = −R·sb·d2r                              dJz/dw =  cb
    //   линейные диагонали = 1.
    let p = FiveAxisParams {
        pivot_length: 250.0,
        tool_length: 120.0,
    };
    let d2r = std::f64::consts::PI / 180.0;
    let h = 1e-5; // шаг центральной разности (углы в градусах)
    let mut cases = 0usize;
    for &(b, c, w) in &[
        (30.0f64, 40.0f64, 5.0f64),
        (90.0f64, 0.0f64, 0.0f64),
        (135.0f64, -120.0f64, -10.0f64),
    ] {
        let big_r = p.pivot_length + p.tool_length + w;
        let (sb, cb) = (b.to_radians().sin(), b.to_radians().cos());
        let (sc, cc) = (c.to_radians().sin(), c.to_radians().cos());
        // Численные производные inverse по b/c/w.
        let dpose = |db: f64, dc: f64, dw: f64| -> [f64; 3] {
            let q = PoseXyzbcw {
                x: 10.0,
                y: -20.0,
                z: 30.0,
                b: b + db,
                c: c + dc,
                w: w + dw,
            };
            let j = fiveaxis_inverse(&p, &q);
            [j.x, j.y, j.z]
        };
        let _base = dpose(0.0, 0.0, 0.0);
        let d_dbb = {
            let plus = dpose(h, 0.0, 0.0);
            let minus = dpose(-h, 0.0, 0.0);
            [
                (plus[0] - minus[0]) / (2.0 * h),
                (plus[1] - minus[1]) / (2.0 * h),
                (plus[2] - minus[2]) / (2.0 * h),
            ]
        };
        let d_dc = {
            let plus = dpose(0.0, h, 0.0);
            let minus = dpose(0.0, -h, 0.0);
            [
                (plus[0] - minus[0]) / (2.0 * h),
                (plus[1] - minus[1]) / (2.0 * h),
                (plus[2] - minus[2]) / (2.0 * h),
            ]
        };
        let d_dw = {
            let plus = dpose(0.0, 0.0, h);
            let minus = dpose(0.0, 0.0, -h);
            [
                (plus[0] - minus[0]) / (2.0 * h),
                (plus[1] - minus[1]) / (2.0 * h),
                (plus[2] - minus[2]) / (2.0 * h),
            ]
        };
        // Аналитические строки оракула.
        let want_bb = [
            -big_r * cb * cc * d2r,
            -big_r * cb * sc * d2r,
            -big_r * sb * d2r,
        ];
        let want_c = [big_r * sb * sc * d2r, -big_r * sb * cc * d2r, 0.0];
        let want_w = [-sb * cc, -sb * sc, cb];
        for i in 0..3 {
            assert!(
                (d_dbb[i] - want_bb[i]).abs() < 1e-6,
                "b={b} c={c} w={w}: dJ[{i}]/db = {} != {}",
                d_dbb[i],
                want_bb[i]
            );
            assert!(
                (d_dc[i] - want_c[i]).abs() < 1e-6,
                "b={b} c={c} w={w}: dJ[{i}]/dc = {} != {}",
                d_dc[i],
                want_c[i]
            );
            assert!(
                (d_dw[i] - want_w[i]).abs() < 1e-6,
                "b={b} c={c} w={w}: dJ[{i}]/dw = {} != {}",
                d_dw[i],
                want_w[i]
            );
        }
        cases += 1;
    }
    assert!(cases >= 3, "якобиан-сверка мала: {cases}");
}
