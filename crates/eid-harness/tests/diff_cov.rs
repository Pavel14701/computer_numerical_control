//! Покрытие diff.rs (итерация 5, cargo-mutants): planimetric_radius —
//! граница допуска радиуса (max(ε_pos, k_R·R)) различает in/out-tolerance.

use eid_core::ast::{
    Dir, Frame, Num, Op, Point3, Program, Spindle, SrcSpan, StartPoint, State, Units, Wcs,
};
use eid_harness::diff::{semantic_diff, DiffConfig, Divergence};

fn n(v: f64) -> Num {
    Num::new(format!("{v}"), v)
}

fn arc_prog(center_x: f64) -> Program {
    Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: eid_core::ast::PathMode::ExactPath,
        frame_preset: None,
        ops: vec![Op::Arc {
            start: StartPoint::Known(Point3 {
                x: n(0.0),
                y: n(0.0),
                z: n(0.0),
            }),
            end: Point3 {
                x: n(5.0),
                y: n(5.0),
                z: n(0.0),
            },
            center: Point3 {
                x: n(center_x),
                y: n(5.0),
                z: n(0.0),
            },
            normal: Point3 {
                x: n(0.0),
                y: n(0.0),
                z: n(1.0),
            },
            dir: Dir::Cw,
            sweep_deg: n(90.0),
            pitch: n(0.0),
            state: State {
                feed: Some(n(100.0)),
                spindle: Spindle::Off,
                coolant: Default::default(),
                tool: None,
                frame: Frame::Wcs,
                wcs: Wcs::G54,
            },
            src: SrcSpan {
                file: "t".into(),
                line_start: 1,
                col_start: 1,
                line_end: 1,
                col_end: 2,
            },
            sub: None,
        }],
    }
}

#[test]
fn planimetric_radius_tolerance_boundary() {
    // ρ_start(A)=5 vs ρ_start(B)=5.0009: разница 0.0009 < 0.001 (ε) → эквивалентны;
    // ρ(B')=5.002: разница 0.002 > 0.001 → Geometry-дивергенция.
    // Ловит ±-мутанты planimetric_radius (формула (s−c)² в осевом квадрате).
    let a = arc_prog(0.0);
    let b_in = arc_prog(0.0009);
    let b_out = arc_prog(0.002);
    let d_in = semantic_diff(&a, &b_in, &DiffConfig::default());
    assert!(
        d_in.iter().all(|d| !matches!(
            d,
            Divergence::Geometry {
                field: "center",
                ..
            }
        )),
        "in-tolerance центр не должен дивергировать: {d_in:?}"
    );
    let d_out = semantic_diff(&a, &b_out, &DiffConfig::default());
    assert!(
        d_out.iter().any(|d| matches!(
            d,
            Divergence::Geometry {
                field: "center",
                ..
            }
        )),
        "out-tolerance центр обязан дивергировать: {d_out:?}"
    );
}

#[test]
fn planimetric_radius_self_consistency_boundary() {
    // Собственная консистентность дуги: ρ_end = |(end−center)| на границе
    // радиусного допуска (max(ε_pos, k_R·R) = 0.001 при R=5):
    // сдвиг конца 0.0009 — в допуске (эквивалентны), 0.002 — дивергенция.
    // Ловит ±-мутанты planimetric_radius напрямую (без center-компенсации).
    let a = arc_prog(0.0);
    let mk = |ex: f64| {
        let mut p = arc_prog(0.0);
        if let Op::Arc { end, .. } = &mut p.ops[0] {
            end.x = n(ex);
        }
        p
    };
    let b_in = mk(5.0009);
    let d_in = semantic_diff(&a, &b_in, &DiffConfig::default());
    assert!(d_in.is_empty(), "радиус в допуске: {d_in:?}");
    let b_out = mk(5.002);
    let d_out = semantic_diff(&a, &b_out, &DiffConfig::default());
    assert!(
        !d_out.is_empty(),
        "радиус вне допуска обязан дивергировать: {d_out:?}"
    );
}

#[test]
fn asts_equal_direct_true_and_false() {
    // Прямой контракт asts_equal: идентичные AST → true; сдвиг feed → false.
    let a = arc_prog(0.0);
    let mut b = arc_prog(0.0);
    if let Op::Arc { state, .. } = &mut b.ops[0] {
        state.feed = Some(n(150.0));
    }
    assert!(eid_harness::diff::asts_equal(
        &a,
        &a.clone(),
        &DiffConfig::default()
    ));
    assert!(!eid_harness::diff::asts_equal(
        &a,
        &b,
        &DiffConfig::default()
    ));
}
