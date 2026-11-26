//! Тесты фазы 12.5: ядро кинематики table-rotary-tilting XYZAC/XYZBC
//! ([`eid_l2::kinematics`], порт формул LinuxCNC trtfuncs.c).
//!
//! Проверки:
//! 1. **Круговой инвариант** `forward∘inverse ≡ identity` (главная
//!    самопроверка): решётка углы A/B/C ∈ {0, ±30, ±90, 180, 359} ×
//!    позиции 0/±1e3 × офсеты 0/10/−5 × con ±1 — по 1568 кейсов на
//!    семейство (обе ноги: inv∘fwd и fwd∘inv), допуск 1e-9 относительный.
//! 2. Зонд-ручные кейсы: A=90°, C=0°, без офсетов — поворот вокруг X
//!    (дефолт con=−1: Y→−Z, Z→+Y; конвенциональный con=+1: Y→Z, Z→−Y) +
//!    ручные якоря с офсетами (посчитано вручную по формулам).
//! 3. Независимая сверка с учебными матрицами поворота: forward без
//!    офсетов ≡ `Rz(con·c)·Rx(con·a)` (XYZAC) / `Rz(con·c)·Ry(con·b)`
//!    (XYZBC) — то же тождество, что upstream `*_work_frame()`.
//! 4. Границы: вырожденный пивот (вся геометрия 0), двойной 360°
//!    (периодичность тригонометрии + точный pass-through углов).
//! 5. Точность: max |ошибка| кругового инварианта < 1e-9 абсолютных на
//!    единичных координатах.
//! 6. Дефолт-флаг «rotary axes opposite of conventional» (LinuxCNC
//!    `conventional-directions = false` ⇒ con = −1) — поведение.
//!
//! Отчёт: `cargo test -p eid-l2 --test kinematics_tests -- --nocapture`.

use eid_l2::kinematics::{
    xyzac_forward, xyzac_inverse, xyzbc_forward, xyzbc_inverse, Joints5, JointsBC, Pose6,
    TrtParams, DEFAULT_CONVENTIONAL_DIRECTIONS,
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

/// Решётка геометрий: (пивот xr/yr/zr, офсет стола, dz) — 0 / 10 / −5 /
/// смешанная. Для XYZAC офсет = dy, для XYZBC = dx.
const GEOMETRIES: [[f64; 5]; 4] = [
    [0.0, 0.0, 0.0, 0.0, 0.0],
    [10.0, 10.0, 10.0, 10.0, 10.0],
    [-5.0, -5.0, -5.0, -5.0, -5.0],
    [0.0, 10.0, -5.0, 10.0, -5.0],
];

/// Единичные координаты для теста абсолютной точности.
const UNIT_POSITIONS: [[f64; 3]; 7] = [
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
    [1.0, 1.0, 1.0],
    [-1.0, 1.0, -1.0],
    [-1.0, -1.0, -1.0],
    [0.5, -0.25, 0.75],
];

/// Относительная близость: |a−b| ≤ rtol·max(1, |a|, |b|).
fn close_rel(a: f64, b: f64, rtol: f64) -> bool {
    (a - b).abs() <= rtol * a.abs().max(b.abs()).max(1.0)
}

/// Тройка декартовых координат близка к pose (абсолютный допуск — кейсы
/// с единичными величинами; ошибки кругового инварианта ~1e-14).
fn assert_cart_close(x: f64, y: f64, z: f64, pose: &Pose6, tol: f64, ctx: &str) {
    assert!(close_rel(pose.x, x, tol), "{ctx}: pose.x={} != {x}", pose.x);
    assert!(close_rel(pose.y, y, tol), "{ctx}: pose.y={} != {y}", pose.y);
    assert!(close_rel(pose.z, z, tol), "{ctx}: pose.z={} != {z}", pose.z);
}

/// Джоинты XYZAC близки (абсолютный допуск).
fn assert_joints5_close(j0: &Joints5, j1: &Joints5, tol: f64, ctx: &str) {
    assert!(
        close_rel(j1.x, j0.x, tol) && close_rel(j1.y, j0.y, tol) && close_rel(j1.z, j0.z, tol),
        "{ctx}: joints ({}, {}, {}) != ({}, {}, {})",
        j1.x,
        j1.y,
        j1.z,
        j0.x,
        j0.y,
        j0.z
    );
}

/// Джоинты XYZBC близки (абсолютный допуск).
fn assert_jointsbc_close(j0: &JointsBC, j1: &JointsBC, tol: f64, ctx: &str) {
    assert!(
        close_rel(j1.x, j0.x, tol) && close_rel(j1.y, j0.y, tol) && close_rel(j1.z, j0.z, tol),
        "{ctx}: joints ({}, {}, {}) != ({}, {}, {})",
        j1.x,
        j1.y,
        j1.z,
        j0.x,
        j0.y,
        j0.z
    );
}

// ===========================================================================
// 1. Круговой инвариант — решётка кейсов (обе ноги, оба семейства)
// ===========================================================================

#[test]
fn circular_invariant_lattice_xyzac() {
    let mut cases = 0usize;
    let mut max_err = 0.0f64;
    for &conventional in &[false, true] {
        for g in &GEOMETRIES {
            let p = TrtParams::new([g[0], g[1], g[2]], g[3], 0.0, g[4], conventional);
            for &a in &ANGLES {
                for &c in &ANGLES {
                    for pos in &POSITIONS {
                        let j0 = Joints5::new(pos[0], pos[1], pos[2], a, c);
                        let ctx =
                            format!("xyzac con={conventional} g={g:?} a={a} c={c} pos={pos:?}");
                        // нога 1: forward → inverse
                        let pose0 = xyzac_forward(&p, &j0);
                        let j1 = xyzac_inverse(&p, &pose0);
                        // углы — точный pass-through
                        assert_eq!(j1.a, j0.a, "{ctx}: A не проходит насквозь");
                        assert_eq!(j1.c, j0.c, "{ctx}: C не проходит насквозь");
                        for (v0, v1) in [(j0.x, j1.x), (j0.y, j1.y), (j0.z, j1.z)] {
                            let err = (v1 - v0).abs();
                            max_err = max_err.max(err);
                            assert!(
                                close_rel(v1, v0, 1e-9),
                                "{ctx}: inv∘fwd err={err} ({v0} → {v1})"
                            );
                        }
                        // нога 2: inverse → forward
                        let pose1 = xyzac_forward(&p, &j1);
                        assert_eq!(pose1.a, pose0.a, "{ctx}: A второго круга");
                        assert_eq!(pose1.c, pose0.c, "{ctx}: C второго круга");
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
    assert!(cases >= 64, "решётка XYZAC слишком мала: {cases}");
    println!("xyzac круговой инвариант: {cases} кейсов, max err = {max_err:e}");
}

#[test]
fn circular_invariant_lattice_xyzbc() {
    let mut cases = 0usize;
    let mut max_err = 0.0f64;
    for &conventional in &[false, true] {
        for g in &GEOMETRIES {
            let p = TrtParams::new([g[0], g[1], g[2]], 0.0, g[3], g[4], conventional);
            for &b in &ANGLES {
                for &c in &ANGLES {
                    for pos in &POSITIONS {
                        let j0 = JointsBC::new(pos[0], pos[1], pos[2], b, c);
                        let ctx =
                            format!("xyzbc con={conventional} g={g:?} b={b} c={c} pos={pos:?}");
                        let pose0 = xyzbc_forward(&p, &j0);
                        let j1 = xyzbc_inverse(&p, &pose0);
                        assert_eq!(j1.b, j0.b, "{ctx}: B не проходит насквозь");
                        assert_eq!(j1.c, j0.c, "{ctx}: C не проходит насквозь");
                        for (v0, v1) in [(j0.x, j1.x), (j0.y, j1.y), (j0.z, j1.z)] {
                            let err = (v1 - v0).abs();
                            max_err = max_err.max(err);
                            assert!(
                                close_rel(v1, v0, 1e-9),
                                "{ctx}: inv∘fwd err={err} ({v0} → {v1})"
                            );
                        }
                        let pose1 = xyzbc_forward(&p, &j1);
                        assert_eq!(pose1.b, pose0.b, "{ctx}: B второго круга");
                        assert_eq!(pose1.c, pose0.c, "{ctx}: C второго круга");
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
    assert!(cases >= 64, "решётка XYZBC слишком мала: {cases}");
    println!("xyzbc круговой инвариант: {cases} кейсов, max err = {max_err:e}");
}

// ===========================================================================
// 2. Зонд-ручные кейсы (A=90°, поворот вокруг X; якоря с офсетами)
// ===========================================================================

#[test]
fn probe_a90_default_con_maps_y_to_minus_z() {
    // Дефолт LinuxCNC (con=false ⇒ con=−1): ротации ПРОТИВ конвенциональных.
    // Поворот вокруг X на 90°: конвенционально Y→Z, Z→−Y; здесь наоборот:
    // Y→−Z, Z→+Y.
    let p = TrtParams::default();
    // точка на оси Y → −Z
    let r = xyzac_forward(&p, &Joints5::new(0.0, 1.0, 0.0, 90.0, 0.0));
    assert_cart_close(0.0, 0.0, -1.0, &r, 1e-9, "Y=1 при A=90 con=false");
    // точка на оси Z → +Y
    let r = xyzac_forward(&p, &Joints5::new(0.0, 0.0, 1.0, 90.0, 0.0));
    assert_cart_close(0.0, 1.0, 0.0, &r, 1e-9, "Z=1 при A=90 con=false");
    // якорь на ненулевых joints: (100,200,300) → (100, 300, −200)
    let r = xyzac_forward(&p, &Joints5::new(100.0, 200.0, 300.0, 90.0, 0.0));
    assert_cart_close(100.0, 300.0, -200.0, &r, 1e-9, "(100,200,300) A=90");
    // и круг замыкается
    let back = xyzac_inverse(&p, &r);
    assert_joints5_close(
        &Joints5::new(100.0, 200.0, 300.0, 90.0, 0.0),
        &back,
        1e-9,
        "probe inv",
    );
}

#[test]
fn probe_a90_conventional_maps_y_to_plus_z() {
    // conventional=true ⇒ con=+1: конвенциональные направления Y→Z, Z→−Y.
    let p = TrtParams::new([0.0; 3], 0.0, 0.0, 0.0, true);
    let r = xyzac_forward(&p, &Joints5::new(0.0, 1.0, 0.0, 90.0, 0.0));
    assert_cart_close(0.0, 0.0, 1.0, &r, 1e-9, "Y=1 при A=90 con=+1");
    let r = xyzac_forward(&p, &Joints5::new(0.0, 0.0, 1.0, 90.0, 0.0));
    assert_cart_close(0.0, -1.0, 0.0, &r, 1e-9, "Z=1 при A=90 con=+1");
}

#[test]
fn xyzac_hand_computed_anchor_with_y_offset() {
    // Ручной подсчёт по формулам (con=−1, xr=yr=zr=dz=0, dy=10,
    // A=90°, C=90°, joints (100,200,300)):
    //   X=100, Y=200−10=190, Z=300; ca=0, sa=1, cc=0, sc=1;
    //   x = 0·100 − (−1)·1·0·190 + 1·1·300 − (−1)·1·10 = 310
    //   y = (−1)·1·100 + 0 − 0 + 0·10             = −100
    //   z = (−1)·1·190 + 0·300 + 0                = −190
    let p = TrtParams::new([0.0; 3], 10.0, 0.0, 0.0, false);
    let r = xyzac_forward(&p, &Joints5::new(100.0, 200.0, 300.0, 90.0, 90.0));
    assert_cart_close(310.0, -100.0, -190.0, &r, 1e-9, "xyzac якорь dy=10");
    // inverse возвращает исходные joints
    let back = xyzac_inverse(&p, &r);
    assert_joints5_close(
        &Joints5::new(100.0, 200.0, 300.0, 90.0, 90.0),
        &back,
        1e-9,
        "якорь inv",
    );
}

#[test]
fn xyzbc_hand_computed_anchor_with_x_offset() {
    // Ручной подсчёт (con=−1, xr=yr=zr=dz=0, dx=10, B=90°, C=0°,
    // joints (100,200,300)):
    //   X=100−10=90, Y=200, Z=300; sb=1, cb=0, cc=1, sc=0;
    //   x = 1·0·90 − 0 + (−1)·1·1·300 + 1·10 = −290
    //   y = 0 + 1·200 + 0 + 0                = 200
    //   z = −(−1)·1·90 + 0·300 + 0           = 90
    let p = TrtParams::new([0.0; 3], 0.0, 10.0, 0.0, false);
    let r = xyzbc_forward(&p, &JointsBC::new(100.0, 200.0, 300.0, 90.0, 0.0));
    assert_cart_close(-290.0, 200.0, 90.0, &r, 1e-9, "xyzbc якорь dx=10");
    let back = xyzbc_inverse(&p, &r);
    assert_jointsbc_close(
        &JointsBC::new(100.0, 200.0, 300.0, 90.0, 0.0),
        &back,
        1e-9,
        "якорь inv",
    );
}

// ===========================================================================
// 3. Независимая сверка с учебными матрицами поворота
// ===========================================================================

type Mat3 = [[f64; 3]; 3];

/// Матрица Rx (правая, градусы).
fn rx(deg: f64) -> Mat3 {
    let r = deg.to_radians();
    let (s, c) = (r.sin(), r.cos());
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}

/// Матрица Ry (правая, градусы).
fn ry(deg: f64) -> Mat3 {
    let r = deg.to_radians();
    let (s, c) = (r.sin(), r.cos());
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}

/// Матрица Rz (правая, градусы).
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

#[test]
fn forward_matches_textbook_matrices_xyzac() {
    // Тождество (проверено алгебраически и совпадает с коэффициентами
    // upstream xyzac_work_frame): без пивота/офсетов forward-матрица = M =
    // Rz(con·c)·Rx(con·a). Т.е. programmed-угол при con=false вращает
    // физическую ось в противоположную сторону.
    let mut cases = 0usize;
    for &conventional in &[false, true] {
        let s = if conventional { 1.0 } else { -1.0 };
        let p = params_with_con(conventional);
        for &a in &ANGLES {
            for &c in &ANGLES {
                let m = mm(&rz(s * c), &rx(s * a));
                for pt in &MATRIX_POINTS {
                    let expect = mv(&m, *pt);
                    let got = xyzac_forward(&p, &Joints5::new(pt[0], pt[1], pt[2], a, c));
                    assert_cart_close(expect[0], expect[1], expect[2], &got, 1e-9, "mat xyzac");
                    cases += 1;
                }
            }
        }
    }
    assert!(cases >= 64, "матричная решётка XYZAC мала: {cases}");
    println!("xyzac матричная сверка: {cases} кейсов");
}

#[test]
fn forward_matches_textbook_matrices_xyzbc() {
    // Аналогично: M = Rz(con·c)·Ry(con·b) (совпадает с xyzbc_work_frame).
    let mut cases = 0usize;
    for &conventional in &[false, true] {
        let s = if conventional { 1.0 } else { -1.0 };
        let p = params_with_con(conventional);
        for &b in &ANGLES {
            for &c in &ANGLES {
                let m = mm(&rz(s * c), &ry(s * b));
                for pt in &MATRIX_POINTS {
                    let expect = mv(&m, *pt);
                    let got = xyzbc_forward(&p, &JointsBC::new(pt[0], pt[1], pt[2], b, c));
                    assert_cart_close(expect[0], expect[1], expect[2], &got, 1e-9, "mat xyzbc");
                    cases += 1;
                }
            }
        }
    }
    assert!(cases >= 64, "матричная решётка XYZBC мала: {cases}");
    println!("xyzbc матричная сверка: {cases} кейсов");
}

// ===========================================================================
// 4. Границы: вырожденный пивот, двойной 360°
// ===========================================================================

#[test]
fn degenerate_zero_geometry_round_trip() {
    // Вся геометрия 0 (пивот в нуле, офсеты 0) — чистый поворот; круг
    // обязан замыкаться, включая « problematic » углы 90/359/180.
    let p = TrtParams::default();
    let mut cases = 0usize;
    for &a in &ANGLES {
        for &c in &ANGLES {
            for pos in &POSITIONS {
                let j0 = Joints5::new(pos[0], pos[1], pos[2], a, c);
                let j1 = xyzac_inverse(&p, &xyzac_forward(&p, &j0));
                assert_joints5_close(&j0, &j1, 1e-9, "degenerate xyzac");
                let jb0 = JointsBC::new(pos[0], pos[1], pos[2], a, c);
                let jb1 = xyzbc_inverse(&p, &xyzbc_forward(&p, &jb0));
                assert_jointsbc_close(&jb0, &jb1, 1e-9, "degenerate xyzbc");
                cases += 1;
            }
        }
    }
    assert!(cases >= 64, "degenerate решётка мала: {cases}");
}

#[test]
fn double_360_periodicity_and_passthrough() {
    // 360°/720°: тригонометрия периодична ⇒ та же декартова поза, что при
    // 0°; углы при этом проходят насквозь БЕЗ нормализации (точно).
    let p = TrtParams::new([5.0, 10.0, -5.0], 10.0, 10.0, 10.0, false);

    let j_ref = Joints5::new(100.0, 200.0, 300.0, 0.0, 0.0);
    let pose_ref = xyzac_forward(&p, &j_ref);
    // двойной 360° на C и на A одновременно:
    let j720 = Joints5::new(100.0, 200.0, 300.0, 720.0, 720.0);
    let pose720 = xyzac_forward(&p, &j720);
    assert_cart_close(
        pose_ref.x,
        pose_ref.y,
        pose_ref.z,
        &pose720,
        1e-9,
        "A=C=720 vs 0",
    );
    let back = xyzac_inverse(&p, &pose720);
    assert_eq!(back.a, 720.0, "A=720 не нормализуется");
    assert_eq!(back.c, 720.0, "C=720 не нормализуется");
    assert_joints5_close(&j720, &back, 1e-9, "inv(A=C=720)");
    // одиночный 360° на C (при A=45°):
    let pose360 = xyzac_forward(&p, &Joints5::new(100.0, 200.0, 300.0, 45.0, 360.0));
    let pose_a45 = xyzac_forward(&p, &Joints5::new(100.0, 200.0, 300.0, 45.0, 0.0));
    assert_cart_close(
        pose_a45.x,
        pose_a45.y,
        pose_a45.z,
        &pose360,
        1e-9,
        "C=360 vs 0",
    );
    assert_eq!(xyzac_inverse(&p, &pose360).c, 360.0);

    // XYZBC: то же для B.
    let jb_ref = JointsBC::new(100.0, 200.0, 300.0, 0.0, 30.0);
    let poseb_ref = xyzbc_forward(&p, &jb_ref);
    let jb720 = JointsBC::new(100.0, 200.0, 300.0, 720.0, 30.0);
    let poseb720 = xyzbc_forward(&p, &jb720);
    assert_cart_close(
        poseb_ref.x,
        poseb_ref.y,
        poseb_ref.z,
        &poseb720,
        1e-9,
        "B=720 vs 0",
    );
    let bback = xyzbc_inverse(&p, &poseb720);
    assert_eq!(bback.b, 720.0, "B=720 не нормализуется");
    assert_jointsbc_close(&jb720, &bback, 1e-9, "inv(B=720)");
}

// ===========================================================================
// 5. Точность: max |ошибка| кругового инварианта < 1e-9 (единичные коорд.)
// ===========================================================================

#[test]
fn circular_invariant_max_abs_error_below_1e9_on_unit_coords() {
    let mut max_err = 0.0f64;
    for &conventional in &[false, true] {
        for g in &GEOMETRIES {
            let pa = TrtParams::new([g[0], g[1], g[2]], g[3], 0.0, g[4], conventional);
            let pb = TrtParams::new([g[0], g[1], g[2]], 0.0, g[3], g[4], conventional);
            for &a in &ANGLES {
                for &c in &ANGLES {
                    for pos in &UNIT_POSITIONS {
                        // XYZAC, обе ноги
                        let j0 = Joints5::new(pos[0], pos[1], pos[2], a, c);
                        let j1 = xyzac_inverse(&pa, &xyzac_forward(&pa, &j0));
                        for (v0, v1) in [(j0.x, j1.x), (j0.y, j1.y), (j0.z, j1.z)] {
                            max_err = max_err.max((v1 - v0).abs());
                        }
                        // XYZBC, обе ноги
                        let jb0 = JointsBC::new(pos[0], pos[1], pos[2], a, c);
                        let jb1 = xyzbc_inverse(&pb, &xyzbc_forward(&pb, &jb0));
                        for (v0, v1) in [(jb0.x, jb1.x), (jb0.y, jb1.y), (jb0.z, jb1.z)] {
                            max_err = max_err.max((v1 - v0).abs());
                        }
                    }
                }
            }
        }
    }
    println!("круговой инвариант (единичные координаты): max abs err = {max_err:e}");
    assert!(
        max_err < 1e-9,
        "max |ошибка| кругового инварианта {max_err:e} ≥ 1e-9"
    );
}

// ===========================================================================
// 6. Дефолт-флаг «opposite of conventional» + pass-through углов
// ===========================================================================

#[test]
fn default_params_match_linuxcnc_opposite_convention() {
    // Константа-флаг оракула: conventional-directions по умолчанию false
    // (ротации противоположны конвенциональным ⇒ con = −1). Пин-контракт:
    // Default связан с константой, а её значение закреплено поведенчески
    // ниже (дефолт даёт повороты «против конвенциональных»).
    let p = TrtParams::default();
    assert_eq!(p.conventional, DEFAULT_CONVENTIONAL_DIRECTIONS);
    assert_eq!(
        p,
        TrtParams {
            rot_point: [0.0; 3],
            y_offset: 0.0,
            x_offset: 0.0,
            z_offset: 0.0,
            conventional: DEFAULT_CONVENTIONAL_DIRECTIONS,
        }
    );
    assert!(!p.conventional);
    // Поведение дефолта: A=90° переводит Y в −Z (НЕ в +Z, как было бы при
    // конвенциональных направлениях).
    let r = xyzac_forward(&p, &Joints5::new(0.0, 1.0, 0.0, 90.0, 0.0));
    assert_cart_close(0.0, 0.0, -1.0, &r, 1e-9, "дефолт Y→−Z");
    // Явный conventional=true переворачивает знак.
    let pc = TrtParams::new([0.0; 3], 0.0, 0.0, 0.0, true);
    let rc = xyzac_forward(&pc, &Joints5::new(0.0, 1.0, 0.0, 90.0, 0.0));
    assert_cart_close(0.0, 0.0, 1.0, &rc, 1e-9, "conventional Y→+Z");
}

#[test]
fn angles_pass_through_unmodified_and_unused_is_zero() {
    let p = TrtParams::new([1.0, 2.0, 3.0], 10.0, -5.0, 7.5, false);
    for &a in &ANGLES {
        for &c in &ANGLES {
            // XYZAC: A/C проходят точно; B в позе = 0 (буква вне coordinates).
            let ja = Joints5::new(10.0, 20.0, 30.0, a, c);
            let pose = xyzac_forward(&p, &ja);
            assert_eq!(pose.a, ja.a);
            assert_eq!(pose.c, ja.c);
            assert_eq!(pose.b, 0.0, "неиспользуемый B должен быть 0");
            let back = xyzac_inverse(&p, &pose);
            assert_eq!(back.a, pose.a);
            assert_eq!(back.c, pose.c);

            // XYZBC: B/C проходят точно; A в позе = 0.
            let jb = JointsBC::new(10.0, 20.0, 30.0, a, c);
            let poseb = xyzbc_forward(&p, &jb);
            assert_eq!(poseb.b, jb.b);
            assert_eq!(poseb.c, jb.c);
            assert_eq!(poseb.a, 0.0, "неиспользуемый A должен быть 0");
            let bback = xyzbc_inverse(&p, &poseb);
            assert_eq!(bback.b, poseb.b);
            assert_eq!(bback.c, poseb.c);
        }
    }
    // Углы за пределами оборота тоже проходят без нормализации.
    let pose = xyzac_forward(&p, &Joints5::new(0.0, 0.0, 0.0, 720.0, -90.0));
    assert_eq!(pose.a, 720.0);
    assert_eq!(pose.c, -90.0);
}

// ---------------------------------------------------------------------------
// Тестовая фабрика: дефолтная (нулевая) геометрия с явным флагом
// направлений ротаций (inherent-impl для чужого типа в тест-крейте
// запрещён, поэтому свободная функция).
// ---------------------------------------------------------------------------
fn params_with_con(conventional: bool) -> TrtParams {
    TrtParams {
        conventional,
        ..TrtParams::default()
    }
}
