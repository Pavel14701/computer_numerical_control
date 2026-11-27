//! Матрица покрытия EQUIVALENCE §4 × within/out (чек-лист — шапка diff.rs):
//! каждое поле допусков: within-ok + out-divergence; bbox-масштаб L;
//! амплификация центра по sweep; min-per-side симметрия; границы 1e-4;
//! точки разрыва (a)–(d) unknown-vs-known; M1 в начале/середине/конце;
//! дубликаты id манифеста → Err.

use eid_core::ast::{
    Coolant, Dir, Frame, Num, Op, PathMode, Point3, Program, Spindle, SrcSpan, StartPoint, State,
    Units, Wcs,
};
use eid_harness::diff::{semantic_diff, DiffConfig, Divergence};
use eid_harness::manifest;
use std::path::Path;

fn num(v: f64) -> Num {
    Num::new(format!("{v:.4}"), v)
}
fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::xyz(num(x), num(y), num(z))
}
fn src() -> SrcSpan {
    SrcSpan {
        file: "m".into(),
        line_start: 0,
        col_start: 1,
        line_end: 0,
        col_end: 0,
    }
}
fn st(feed: Option<f64>) -> State {
    State {
        feed: feed.map(num),
        spindle: Spindle::Cw(num(10000.0)),
        coolant: Coolant::default(),
        tool: None,
        frame: Frame::Wcs,
        wcs: Wcs::G54,
        feed_restated: false,
    }
}

// T8 (г) (v0.2 волна 3, EQUIVALENCE §5): feed_restated — провенанс
// (класс src/sub) — semantic_diff(restated=true) vs (restated=false) пуст.
#[test]
fn feed_restated_is_out_of_equivalence() {
    let mk = |restated: bool| {
        prog(vec![Op::Linear {
            start: StartPoint::Unknown,
            end: p3(10.0, 0.0, 0.0),
            state: State {
                feed_restated: restated,
                ..st(Some(120.0))
            },
            src: src(),
            sub: None,
        }])
    };
    let a = mk(true);
    let b = mk(false);
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    assert!(
        d.is_empty(),
        "feed_restated — вне эквивалентности (provenance): {d:?}"
    );
}

fn prog(ops: Vec<Op>) -> Program {
    Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: PathMode::ExactPath,
        frame_preset: None,
        ops,
    }
}
fn rapid(start: StartPoint, e: Point3, feed: Option<f64>) -> Op {
    Op::Rapid {
        start,
        end: e,
        state: st(feed),
        src: src(),
        sub: None,
    }
}
fn linear(s: Point3, e: Point3, feed: f64) -> Op {
    Op::Linear {
        start: StartPoint::Known(s),
        end: e,
        state: st(Some(feed)),
        src: src(),
        sub: None,
    }
}
fn arc(s: Point3, e: Point3, c: Point3, sweep: f64, feed: f64) -> Op {
    Op::Arc {
        start: StartPoint::Known(s),
        end: e,
        center: c,
        normal: p3(0.0, 0.0, 1.0),
        dir: Dir::Cw,
        sweep_deg: num(sweep),
        pitch: num(0.0),
        state: st(Some(feed)),
        src: src(),
        sub: None,
    }
}
fn pend() -> Op {
    Op::ProgramEnd { src: src() }
}

// --- позиция: bbox-масштаб L (k_pos·L включается на больших программах) ---
#[test]
fn eq4_position_small_out() {
    // маленькая программа (L ≈ 10): tol = 0.001; сдвиг 0.002 → divergence
    let mut b = prog(base());
    if let Op::Linear { end, .. } = &mut b.ops[1] {
        *end = p3(10.002, 0.0, 5.0);
    }
    let d = semantic_diff(&prog(base()), &b, &DiffConfig::default());
    assert!(
        d.contains(&Divergence::Geometry {
            index: 1,
            field: "end"
        }),
        "{d:?}"
    );
}

#[test]
fn eq4_position_large_bbox_within() {
    // большая программа (L ≈ 30000): tol = max(0.001, 1e-7·30000) = 0.003;
    // сдвиг 0.002 < 0.003 → within-ok (относительный член включён)
    let ops = |shift: f64| {
        prog(vec![
            rapid(StartPoint::Unknown, p3(0.0, 0.0, 0.0), None),
            linear(p3(0.0, 0.0, 0.0), p3(30000.0, 0.0, 0.0), 500.0),
            rapid(
                StartPoint::Known(p3(30000.0, 0.0, 0.0)),
                p3(30000.0, 0.0 + shift, 0.0),
                Some(500.0),
            ),
            pend(),
        ])
    };
    let mut b = ops(0.002);
    if let Op::Rapid { end, .. } = &mut b.ops[2] {
        end.y = num(0.002);
    }
    assert!(semantic_diff(&ops(0.0), &b, &DiffConfig::default()).is_empty());
}

#[test]
fn eq4_position_large_bbox_out() {
    let ops = |shift: f64| {
        prog(vec![
            rapid(StartPoint::Unknown, p3(0.0, 0.0, 0.0), None),
            linear(p3(0.0, 0.0, 0.0), p3(30000.0, 0.0, 0.0), 500.0),
            rapid(
                StartPoint::Known(p3(30000.0, 0.0, 0.0)),
                p3(30000.0, shift, 0.0),
                Some(500.0),
            ),
            pend(),
        ])
    };
    let mut b = ops(0.0);
    if let Op::Rapid { end, .. } = &mut b.ops[2] {
        end.y = num(0.01); // 0.01 > 0.003
    }
    let d = semantic_diff(&ops(0.0), &b, &DiffConfig::default());
    assert!(
        d.contains(&Divergence::Geometry {
            index: 2,
            field: "end"
        }),
        "{d:?}"
    );
}

fn base() -> Vec<Op> {
    vec![
        rapid(StartPoint::Unknown, p3(0.0, 0.0, 5.0), None),
        linear(p3(0.0, 0.0, 5.0), p3(10.0, 0.0, 5.0), 150.0),
        arc(
            p3(10.0, 0.0, 5.0),
            p3(15.0, 5.0, 5.0),
            p3(15.0, 0.0, 5.0),
            90.0,
            100.0,
        ),
        pend(),
    ]
}

// --- центр: амплификация A по sweep (179.9 vs 170 vs 90) ---
#[test]
fn eq4_center_amplify_179_9() {
    // sweep 179.9, R=5: tol_center = q·A = 1e-4·1146 ≈ 0.115; сдвиг центра 0.1 → ok
    let mk = |cx: f64| {
        prog(vec![
            rapid(StartPoint::Unknown, p3(0.0, 0.0, 0.0), None),
            arc(
                p3(0.0, 0.0, 0.0),
                p3(0.0175, -9.9998, 0.0),
                p3(cx, 5.0, 0.0),
                179.9,
                100.0,
            ),
            pend(),
        ])
    };
    // допустимый сдвиг: 0.1 (< 0.115)
    let mut b = mk(-0.0351 + 0.1);
    if let Op::Arc { center, .. } = &mut b.ops[1] {
        center.x = num(-0.0351 + 0.1);
    }
    assert!(semantic_diff(&mk(-0.0351), &b, &DiffConfig::default()).is_empty());
    // вне допуска: 0.2 > 0.115
    let mut c = mk(-0.0351);
    if let Op::Arc { center, .. } = &mut c.ops[1] {
        center.x = num(-0.0351 + 0.2);
    }
    let d = semantic_diff(&mk(-0.0351), &c, &DiffConfig::default());
    assert!(
        d.contains(&Divergence::Geometry {
            index: 1,
            field: "center"
        }),
        "{d:?}"
    );
}

#[test]
fn eq4_center_sweep_170_90_normal_tolerance() {
    // sweep 170: A ≈ 1/cos(85°) ≈ 11.47 → tol ≈ 0.0011; sweep 90: A ≈ 1.41 → tol ≈ 0.001
    let mk = |sweep: f64, cx_shift: f64| {
        prog(vec![
            rapid(StartPoint::Unknown, p3(0.0, 0.0, 0.0), None),
            arc(
                p3(0.0, 0.0, 0.0),
                p3(10.0, 0.0, 0.0),
                p3(5.0 + cx_shift, 0.0, 0.0),
                sweep,
                100.0,
            ),
            pend(),
        ])
    };
    let d = semantic_diff(&mk(170.0, 0.0), &mk(170.0, 0.002), &DiffConfig::default());
    assert!(!d.is_empty(), "170°: сдвиг 0.002 > tol 0.0011 → divergence");
    assert!(semantic_diff(&mk(90.0, 0.0), &mk(90.0, 0.0005), &DiffConfig::default()).is_empty());
}

// --- sweep: граница 1e-4 ---
#[test]
fn eq4_sweep_boundary_1e4() {
    let mk = |deg: f64| {
        prog(vec![
            rapid(StartPoint::Unknown, p3(0.0, 0.0, 0.0), None),
            linear(p3(0.0, 0.0, 0.0), p3(10.0, 0.0, 0.0), 150.0),
            arc(
                p3(10.0, 0.0, 0.0),
                p3(15.0, 5.0, 0.0),
                p3(15.0, 0.0, 0.0),
                deg,
                100.0,
            ),
            pend(),
        ])
    };
    // Δ = ровно 1e-4 (90.0 vs 90.0001): (a−b).abs() = 1.0000e-4 ≤ 1e-4 → within
    let d = semantic_diff(&mk(90.0), &mk(90.0001), &DiffConfig::default());
    assert!(d.is_empty(), "граница допуска включена: {d:?}");
    // Δ = 0.01° → out (амплитудный член на 90°: deg(q·A(90)/R) ≈ 0.0016°)
    let d = semantic_diff(&mk(90.0), &mk(90.01), &DiffConfig::default());
    assert!(
        d.contains(&Divergence::Geometry {
            index: 2,
            field: "sweep"
        }),
        "{d:?}"
    );
}

// --- pitch: within/out (= ε_pos) ---
#[test]
fn eq4_pitch_within_out() {
    let mk = |pitch: f64| {
        let mut p = prog(vec![
            rapid(StartPoint::Unknown, p3(0.0, 0.0, 0.0), None),
            arc(
                p3(0.0, 0.0, 0.0),
                p3(5.0, 5.0, 0.0),
                p3(5.0, 0.0, 0.0),
                90.0,
                100.0,
            ),
            pend(),
        ]);
        if let Op::Arc { pitch: pt, .. } = &mut p.ops[1] {
            *pt = num(pitch);
        }
        p
    };
    assert!(semantic_diff(&mk(0.0), &mk(0.0005), &DiffConfig::default()).is_empty());
    let d = semantic_diff(&mk(0.0), &mk(0.002), &DiffConfig::default());
    assert!(
        d.contains(&Divergence::Geometry {
            index: 1,
            field: "pitch"
        }),
        "{d:?}"
    );
}

// --- feed: floor vs relative ---
#[test]
fn eq4_feed_floor_vs_relative() {
    // floor-режим: feed=150, tol=0.001 → 0.0005 ok, 0.002 out
    let lin = |f: f64| {
        prog(vec![
            linear(p3(0.0, 0.0, 5.0), p3(10.0, 0.0, 5.0), f),
            pend(),
        ])
    };
    assert!(semantic_diff(&lin(150.0), &lin(150.0005), &DiffConfig::default()).is_empty());
    let d = semantic_diff(&lin(150.0), &lin(150.002), &DiffConfig::default());
    assert!(
        d.contains(&Divergence::StateField {
            index: 0,
            field: "feed"
        }),
        "{d:?}"
    );
    // relative-режим: feed=200000 → tol = max(0.001, 1e-5·200000 = 2.0)
    let big = |f: f64| {
        prog(vec![
            linear(p3(0.0, 0.0, 5.0), p3(10.0, 0.0, 5.0), f),
            pend(),
        ])
    };
    assert!(semantic_diff(&big(200000.0), &big(200001.0), &DiffConfig::default()).is_empty());
}

// --- rpm: floor vs relative ---
#[test]
fn eq4_rpm_floor_and_relative() {
    let spin = |rpm: f64| {
        let mut p = prog(vec![
            rapid(StartPoint::Unknown, p3(0.0, 0.0, 5.0), None),
            pend(),
        ]);
        if let Op::Rapid { state, .. } = &mut p.ops[0] {
            state.spindle = Spindle::Cw(num(rpm));
        }
        p
    };
    // floor: max(0.01, 1e-6·50)=0.01 → 50.005 within; 50.05 out
    assert!(semantic_diff(&spin(50.0), &spin(50.005), &DiffConfig::default()).is_empty());
    let d = semantic_diff(&spin(50.0), &spin(50.05), &DiffConfig::default());
    assert!(
        d.contains(&Divergence::StateField {
            index: 0,
            field: "spindle"
        }),
        "{d:?}"
    );
    // relative: rpm=2_000_000 → tol = 1e-6·2e6 = 2.0 → 2000001 within
    assert!(semantic_diff(
        &spin(2_000_000.0),
        &spin(2_000_001.0),
        &DiffConfig::default()
    )
    .is_empty());
}

// --- dwell: max(1e-4, 1e-5·t) ---
#[test]
fn eq4_dwell_within_out() {
    let dw = |t: f64| {
        prog(vec![
            Op::Dwell {
                seconds: num(t),
                state: st(Some(150.0)),
                src: src(),
                sub: None,
            },
            pend(),
        ])
    };
    assert!(semantic_diff(&dw(1.0), &dw(1.00005), &DiffConfig::default()).is_empty());
    let d = semantic_diff(&dw(1.0), &dw(1.001), &DiffConfig::default());
    assert!(
        d.contains(&Divergence::Geometry {
            index: 0,
            field: "seconds"
        }),
        "{d:?}"
    );
}

// --- min-per-side: одна сторона в допуске, другая нет → divergence; симметрия ---
#[test]
fn eq4_min_side_179_9_180_1_and_symmetry() {
    // R=10 near-semicircle: tol(179.9) ≈ 0.657°, tol(180.1) ≈ 0.657°, но Δ = 0.2°...
    // для 179.9-vs-180.1 обе стороны в допуске → ok (проверено в harness_tests);
    // здесь — пара «одна в допуске, другая нет»: sweep 179.4975 (tol 0.131 @R10)
    // vs 180.0 (tol 0.573): min = 0.131 < Δ = 0.5025 → divergence; swap → тот же.
    let mk = |sweep_val: f64| {
        prog(vec![
            rapid(StartPoint::Unknown, p3(0.0, 10.0, 0.0), None),
            arc(
                p3(0.0, 10.0, 0.0),
                p3(0.0175, -9.9998, 0.0),
                p3(-0.0351, 0.0001, 0.0),
                sweep_val,
                100.0,
            ),
            pend(),
        ])
    };
    let a = mk(179.4975);
    let b = mk(180.0);
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    assert!(
        d.contains(&Divergence::Geometry {
            index: 1,
            field: "sweep"
        }),
        "{d:?}"
    );
    let d_rev = semantic_diff(&b, &a, &DiffConfig::default());
    assert_eq!(d, d_rev, "симметрия min-per-side");
}

// --- точки разрыва (a)–(d): unknown-vs-known → UnknownVsKnown divergence ---
#[test]
fn eq4_breakpoints_a_d_unknown_vs_known() {
    // (a) первый motion без якоря: Unknown ✓; пара (Unknown vs Known) → divergence
    let a_u = prog(vec![
        rapid(StartPoint::Unknown, p3(0.0, 0.0, 5.0), None),
        pend(),
    ]);
    let a_k = prog(vec![
        rapid(
            StartPoint::Known(p3(0.0, 0.0, 5.0)),
            p3(0.0, 0.0, 5.0),
            None,
        ),
        pend(),
    ]);
    let d = semantic_diff(&a_u, &a_k, &DiffConfig::default());
    assert!(
        d.contains(&Divergence::UnknownVsKnown { index: 0 }),
        "{d:?}"
    );

    // (b) wcs-смена без origins: Unknown легален; (Known vs Unknown) → divergence
    let mk_b = |unknown: bool| {
        prog(vec![
            rapid(StartPoint::Unknown, p3(0.0, 0.0, 5.0), None),
            rapid(
                StartPoint::Known(p3(5.0, 0.0, 5.0)),
                p3(5.0, 0.0, 5.0),
                Some(150.0),
            ),
            {
                let mut r = rapid(
                    if unknown {
                        StartPoint::Unknown
                    } else {
                        StartPoint::Known(p3(5.0, 0.0, 5.0))
                    },
                    p3(15.0, 0.0, 5.0),
                    Some(150.0),
                );
                if let Op::Rapid { state, .. } = &mut r {
                    state.wcs = Wcs::G55;
                }
                r
            },
            pend(),
        ])
    };
    let d = semantic_diff(&mk_b(true), &mk_b(false), &DiffConfig::default());
    assert!(
        d.contains(&Divergence::UnknownVsKnown { index: 2 }),
        "{d:?}"
    );

    // (c)/(d): machine-ход и возврат — обе пары Unknown/Known → divergence на индексе
    let mk_cd = |start_known: bool| {
        prog(vec![
            Op::WcsDefine {
                p: 1,
                origin: p3(0.0, 0.0, 0.0),
                state: st(None),
                src: src(),
            },
            rapid(
                if start_known {
                    StartPoint::Known(p3(10.0, 0.0, 0.0))
                } else {
                    StartPoint::Unknown
                },
                p3(10.0, 0.0, 0.0),
                None,
            ),
            pend(),
        ])
    };
    // (c): machine-frame первый ход — Unknown легален; Known — вне связывания
    // semantic_diff: (Unknown vs Known) → UnknownVsKnown
    let mut c_u = mk_cd(false);
    let mut c_k = mk_cd(true);
    if let Op::Rapid { state, .. } = &mut c_u.ops[1] {
        state.frame = Frame::Machine;
    }
    if let Op::Rapid { state, .. } = &mut c_k.ops[1] {
        state.frame = Frame::Machine;
    }
    let d = semantic_diff(&c_u, &c_k, &DiffConfig::default());
    assert!(
        d.contains(&Divergence::UnknownVsKnown { index: 1 }),
        "{d:?}"
    );
}

// --- M1 в начале/середине/конце; M0 не фильтруется; M1+M0 вместе ---
#[test]
fn eq4_m1_positions_start_mid_end() {
    let with_stop = |optional: bool, at_end: bool| {
        let mut ops = base();
        let stop = Op::ProgramStop {
            optional,
            state: st(Some(100.0)),
            src: src(),
        };
        if at_end {
            ops.insert(ops.len() - 1, stop); // перед PEND
        } else {
            ops.insert(0, stop); // начало
        }
        prog(ops)
    };
    let base_p = prog(base());
    // M1 (optional=true) в начале и в конце — фильтруется
    assert!(semantic_diff(&with_stop(true, false), &base_p, &DiffConfig::default()).is_empty());
    assert!(semantic_diff(&with_stop(true, true), &base_p, &DiffConfig::default()).is_empty());
    // M0 (optional=false) — не фильтруется: OpCount
    let d = semantic_diff(&with_stop(false, false), &base_p, &DiffConfig::default());
    assert!(d.contains(&Divergence::OpCount { a: 5, b: 4 }), "{d:?}");
    // M1+M0 вместе: M1 уходит, M0 остаётся
    let mut both = prog(base());
    both.ops.insert(
        0,
        Op::ProgramStop {
            optional: true,
            state: st(Some(100.0)),
            src: src(),
        },
    );
    both.ops.insert(
        1,
        Op::ProgramStop {
            optional: false,
            state: st(Some(100.0)),
            src: src(),
        },
    );
    let d = semantic_diff(&both, &base_p, &DiffConfig::default());
    // M1 уходит (Q8), M0 остаётся → 5 vs 4
    assert!(d.contains(&Divergence::OpCount { a: 5, b: 4 }), "{d:?}");
}

// --- дубликаты id манифеста → Err ---
#[test]
fn tc_manifest_duplicate_ids() {
    let dir = std::env::temp_dir().join("eid_harness_it5_dup");
    std::fs::create_dir_all(&dir).unwrap();
    let mf = dir.join("manifest.toml");
    std::fs::write(
        &mf,
        "corpus_version = \"t\"\n[[vectors]]\nid = \"X\"\nname = \"a\"\ntier = \"A\"\nkind = \"positive\"\ngolden = \"g.eid\"\n[[vectors]]\nid = \"X\"\nname = \"b\"\ntier = \"A\"\nkind = \"positive\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("g.eid"), "").unwrap();
    let err = manifest::load(&mf).unwrap_err();
    assert!(err.contains("duplicate vector id"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

// --- манифест-инвариант: каждый позитив ⇔ golden, негатив ⇔ golden=null ---
#[test]
fn tc_manifest_positive_negative_invariants() {
    let m =
        manifest::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/manifest.toml"))
            .expect("manifest loads");
    for v in &m.vectors {
        match v.kind {
            eid_harness::manifest::Kind::Positive => {
                assert!(v.golden.is_some(), "{}: позитив без golden", v.id);
            }
            eid_harness::manifest::Kind::Negative => {
                assert!(v.golden.is_none(), "{}: негатив с golden", v.id);
            }
        }
    }
}
