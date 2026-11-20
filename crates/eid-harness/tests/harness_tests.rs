//! Интеграционные тесты eid-harness (Агент B, итерация 2):
//!  * semantic_diff — мутации: сдвиг позиции, feed в допуске/за допуском,
//!    unknown-vs-known, sweep 179.9 vs 180.1 (амплификация центра), M1-фильтрация,
//!    RAW payload, мета (units/path_mode/frame_preset), state-поля;
//!  * manifest::load — реальный corpus/manifest.toml (TOML, миграция R-1): 57 векторов (37 golden +
//!    20 негативных), пути существуют, notice/r4_expect по CORPUS §1.1;
//!  * runner — R1/R2 зелёные на всех 20 golden; диалектные проверки Pending
//!    без адаптеров (не crash); правило двух классов негативов и R4 —
//!    через mock-адаптеры и синтетический Manifest.

use eid_core::adapter::{Adapter, EidTextAdapter};
use eid_core::ast::{
    Coolant, Dir, Frame, Num, Op, PathMode, Point3, Program, SrcSpan, StartPoint, State, Units,
    Wcs,
};
use eid_core::diag::{Diagnostic, ErrorCode};
use eid_harness::diff::{semantic_diff, DiffConfig, Divergence};
use eid_harness::manifest::{self, FileEntry, Kind, Manifest, Outcome, Tier, Vector};
use eid_harness::runner::{self, Registry, Status};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ===========================================================================
// Хелперы
// ===========================================================================

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

fn load_manifest() -> Manifest {
    manifest::load(&corpus_dir().join("manifest.toml")).expect("manifest.toml loads")
}

fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tier_a/v0").join(name)
}

fn num(v: f64) -> Num {
    Num::new(format!("{v:.4}"), v)
}

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3 { x: num(x), y: num(y), z: num(z) }
}

fn src() -> SrcSpan {
    SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 }
}

fn state(feed: Option<f64>) -> State {
    State {
        feed: feed.map(num),
        spindle: eid_core::ast::Spindle::Cw(num(10000.0)),
        coolant: Coolant::default(),
        tool: None,
        frame: Frame::Wcs,
        wcs: Wcs::G54,
    }
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

/// Базовая программа: RAPID unknown → LINEAR → ARC(90° cw) → PEND.
fn base_ops() -> Vec<Op> {
    vec![
        Op::Rapid {
            start: StartPoint::Unknown,
            end: p3(0.0, 0.0, 5.0),
            state: state(None),
            src: src(),
            sub: None,
        },
        Op::Linear {
            start: StartPoint::Known(p3(0.0, 0.0, 5.0)),
            end: p3(10.0, 0.0, 5.0),
            state: state(Some(150.0)),
            src: src(),
            sub: None,
        },
        Op::Arc {
            start: StartPoint::Known(p3(10.0, 0.0, 5.0)),
            end: p3(15.0, 5.0, 5.0),
            center: p3(15.0, 0.0, 5.0),
            normal: p3(0.0, 0.0, 1.0),
            dir: Dir::Cw,
            sweep_deg: num(90.0),
            pitch: num(0.0),
            state: state(Some(100.0)),
            src: src(),
            sub: None,
        },
        Op::ProgramEnd { src: src() },
    ]
}

fn has(d: &[Divergence], p: &Divergence) -> bool {
    d.iter().any(|x| x == p)
}

fn tmp_file(tag: &str, text: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("eid_harness_it2_{tag}"));
    std::fs::write(&p, text).expect("write temp file");
    p
}

// ===========================================================================
// semantic_diff — мутации (задача 2)
// ===========================================================================

#[test]
fn diff_identical_is_empty() {
    let a = prog(base_ops());
    let b = prog(base_ops());
    assert!(semantic_diff(&a, &b, &DiffConfig::default()).is_empty());
}

#[test]
fn diff_position_shift_beyond_epsilon() {
    let mut b = prog(base_ops());
    if let Op::Linear { end, .. } = &mut b.ops[1] {
        *end = p3(10.01, 0.0, 5.0); // сдвиг 0.01 > ε_pos = max(0.001, 1e-7·L)
    }
    let d = semantic_diff(&prog(base_ops()), &b, &DiffConfig::default());
    assert!(has(&d, &Divergence::Geometry { index: 1, field: "end" }), "{d:?}");
    assert_eq!(d.len(), 1);
}

#[test]
fn diff_position_shift_within_epsilon() {
    let mut b = prog(base_ops());
    if let Op::Linear { end, .. } = &mut b.ops[1] {
        *end = p3(10.0005, 0.0, 5.0); // 0.0005 < ε_pos
    }
    assert!(semantic_diff(&prog(base_ops()), &b, &DiffConfig::default()).is_empty());
}

#[test]
fn diff_feed_beyond_and_within_tolerance() {
    let cfg = DiffConfig::default();
    // в допуске: 5e-4 < max(0.001, 1e-5·150)
    let mut within = prog(base_ops());
    if let Op::Linear { state, .. } = &mut within.ops[1] {
        state.feed = Some(num(150.0005));
    }
    assert!(semantic_diff(&prog(base_ops()), &within, &cfg).is_empty());
    // за допуском: 1.0 > 0.001
    let mut beyond = prog(base_ops());
    if let Op::Linear { state, .. } = &mut beyond.ops[1] {
        state.feed = Some(num(151.0));
    }
    let d = semantic_diff(&prog(base_ops()), &beyond, &cfg);
    assert!(has(&d, &Divergence::StateField { index: 1, field: "feed" }), "{d:?}");
    // none ↔ число — всегда расхождение (EQUIVALENCE §2)
    let mut none = prog(base_ops());
    if let Op::Linear { state, .. } = &mut none.ops[1] {
        state.feed = None;
    }
    let d = semantic_diff(&prog(base_ops()), &none, &cfg);
    assert!(has(&d, &Divergence::StateField { index: 1, field: "feed" }), "{d:?}");
}

#[test]
fn diff_unknown_vs_known() {
    let mut b = prog(base_ops());
    if let Op::Rapid { start, .. } = &mut b.ops[0] {
        *start = StartPoint::Known(p3(0.0, 0.0, 5.0));
    }
    let d = semantic_diff(&prog(base_ops()), &b, &DiffConfig::default());
    assert!(has(&d, &Divergence::UnknownVsKnown { index: 0 }), "{d:?}");
}

#[test]
fn diff_m1_filtered_m0_counts() {
    // M1 (optional=true) ≡ отсутствие (Q8, EQUIVALENCE §5): лишний M1 → 0 диффов
    let mut a = prog(base_ops());
    a.ops.insert(2, Op::ProgramStop { optional: true, state: state(Some(150.0)), src: src() });
    assert!(semantic_diff(&a, &prog(base_ops()), &DiffConfig::default()).is_empty());
    // M0 (optional=false) — значим: OpCount + OpKind
    let mut c = prog(base_ops());
    c.ops.insert(2, Op::ProgramStop { optional: false, state: state(Some(150.0)), src: src() });
    let d = semantic_diff(&c, &prog(base_ops()), &DiffConfig::default());
    assert!(has(&d, &Divergence::OpCount { a: 5, b: 4 }), "{d:?}");
    assert!(has(&d, &Divergence::OpKind { index: 2 }), "{d:?}");
}

#[test]
fn diff_sweep_amplified_tolerance_rounded_center_pair_ok() {
    // EQUIVALENCE §4 v0.3 (СТОП-2 отчёта D): 179.4975 vs 179.4973 при R≈10
    // (геометрия golden #11: start (0,10,0), end (0.0175,-9.9998,0), центр
    // derived (−0.0351,0.0001,0)). Без члена deg(q·A/R): Δ = 0.0002° >
    // 1e-6·sweep = 1.795e-4° → ложнокрасный; с членом (deg(1e-4·228/10) ≈
    // 0.1307°) → ok.
    let mk = |sweep_val: f64| {
        prog(vec![
            Op::Rapid {
                start: StartPoint::Unknown,
                end: p3(0.0, 10.0, 0.0),
                state: state(None),
                src: src(),
                sub: None,
            },
            Op::Arc {
                start: StartPoint::Known(p3(0.0, 10.0, 0.0)),
                end: p3(0.0175, -9.9998, 0.0),
                center: p3(-0.0351, 0.0001, 0.0),
                normal: p3(0.0, 0.0, 1.0),
                dir: Dir::Cw,
                sweep_deg: Num::new(format!("{sweep_val:.4}"), sweep_val),
                pitch: num(0.0),
                state: state(Some(100.0)),
                src: src(),
                sub: None,
            },
            Op::ProgramEnd { src: src() },
        ])
    };
    assert!(semantic_diff(&mk(179.4975), &mk(179.4973), &DiffConfig::default()).is_empty());
}

#[test]
fn diff_sweep_half_degree_from_semicircle_diverges() {
    // 179.4975 vs 180.0 при том же округлённом центре (R≈10): AND-логика
    // (min по сторонам) — ε(179.4975) ≈ 0.1307° < Δ = 0.5025° → divergence,
    // ровно одна (sweep); центр/end идентичны → ok.
    let mk = |sweep_val: f64| {
        prog(vec![
            Op::Rapid {
                start: StartPoint::Unknown,
                end: p3(0.0, 10.0, 0.0),
                state: state(None),
                src: src(),
                sub: None,
            },
            Op::Arc {
                start: StartPoint::Known(p3(0.0, 10.0, 0.0)),
                end: p3(0.0175, -9.9998, 0.0),
                center: p3(-0.0351, 0.0001, 0.0),
                normal: p3(0.0, 0.0, 1.0),
                dir: Dir::Cw,
                sweep_deg: Num::new(format!("{sweep_val:.4}"), sweep_val),
                pitch: num(0.0),
                state: state(Some(100.0)),
                src: src(),
                sub: None,
            },
            Op::ProgramEnd { src: src() },
        ])
    };
    let d = semantic_diff(&mk(179.4975), &mk(180.0), &DiffConfig::default());
    assert_eq!(d, vec![Divergence::Geometry { index: 1, field: "sweep" }], "{d:?}");
    // Симметрия эквивалентности: дифф не зависит от порядка сторон.
    let d_rev = semantic_diff(&mk(180.0), &mk(179.4975), &DiffConfig::default());
    assert_eq!(d_rev, vec![Divergence::Geometry { index: 1, field: "sweep" }], "{d_rev:?}");
}

#[test]
fn diff_sweep_near_semicircle_both_sides_amplified_ok() {
    // Обе стороны near-180 (R=5, пер-сайдовые центры h = 5/tan(sweep/2)):
    // ε = deg(1e-4·A/5) ≈ 1.313° > Δ = 0.2° → ok (v0.3; прежний тест считал
    // это расхождением при допуске 1e-6·sweep).
    let mk = |sweep: f64, h: f64| {
        prog(vec![
            Op::Rapid {
                start: StartPoint::Unknown,
                end: p3(0.0, 0.0, 0.0),
                state: state(None),
                src: src(),
                sub: None,
            },
            Op::Arc {
                start: StartPoint::Known(p3(0.0, 0.0, 0.0)),
                end: p3(0.0, 10.0, 0.0),
                center: p3(h, 5.0, 0.0),
                normal: p3(0.0, 0.0, 1.0),
                dir: Dir::Cw,
                sweep_deg: Num::new(format!("{sweep:.4}"), sweep),
                pitch: num(0.0),
                state: state(Some(100.0)),
                src: src(),
                sub: None,
            },
            Op::ProgramEnd { src: src() },
        ])
    };
    let h_pos = 5.0 / ((179.9_f64 / 2.0).to_radians().tan());
    let h_neg = 5.0 / ((180.1_f64 / 2.0).to_radians().tan());
    assert!(semantic_diff(&mk(179.9, h_pos), &mk(180.1, h_neg), &DiffConfig::default()).is_empty());
    // Контроль центра: обычная дуга 90° (A ≈ 1.41), центр сдвинут на 0.5 →
    // center расходится (амплификация центра не резиновая).
    let mut c = prog(base_ops());
    if let Op::Arc { center, .. } = &mut c.ops[2] {
        *center = p3(15.5, 0.0, 5.0);
    }
    let d = semantic_diff(&prog(base_ops()), &c, &DiffConfig::default());
    assert!(has(&d, &Divergence::Geometry { index: 2, field: "center" }), "{d:?}");
    // Контроль sweep: обычная дуга 90°, sweep 90 → 91 (A(91) ≈ 1.44, R = 5:
    // ε ≈ 0.0016° < 1°) → sweep расходится.
    let mut s = prog(base_ops());
    if let Op::Arc { sweep_deg, .. } = &mut s.ops[2] {
        *sweep_deg = num(91.0);
    }
    let d = semantic_diff(&prog(base_ops()), &s, &DiffConfig::default());
    assert!(has(&d, &Divergence::Geometry { index: 2, field: "sweep" }), "{d:?}");
}

#[test]
fn diff_raw_payload_exact() {
    let raw = |payload: &str| {
        prog(vec![
            Op::Raw {
                payload_hex: payload.into(),
                reason: "r".into(),
                code: ErrorCode::UnsupportedSyntax,
                src: src(),
            },
            Op::ProgramEnd { src: src() },
        ])
    };
    assert!(semantic_diff(&raw("4142"), &raw("4142"), &DiffConfig::default()).is_empty());
    // reason/code — provenance, вне эквивалентности; payload — точное равенство
    let mut reason = raw("4142");
    if let Op::Raw { reason, .. } = &mut reason.ops[0] {
        *reason = "other".into();
    }
    assert!(semantic_diff(&raw("4142"), &reason, &DiffConfig::default()).is_empty());
    let d = semantic_diff(&raw("4142"), &raw("4143"), &DiffConfig::default());
    assert!(has(&d, &Divergence::RawMismatch { index: 0 }), "{d:?}");
}

#[test]
fn diff_meta_fields() {
    let cfg = DiffConfig::default();
    let mut b = prog(base_ops());
    b.units = Units::In;
    let d = semantic_diff(&prog(base_ops()), &b, &cfg);
    assert!(has(&d, &Divergence::MetaField { field: "units" }), "{d:?}");

    let mut c = prog(base_ops());
    c.path_mode = PathMode::ExactStop;
    let d = semantic_diff(&prog(base_ops()), &c, &cfg);
    assert!(has(&d, &Divergence::MetaField { field: "path_mode" }), "{d:?}");

    // Blend: kind-равенство; t — допуск позиции; None vs Some — расхождение
    let blend = |t: Option<f64>| {
        let mut p = prog(base_ops());
        p.path_mode = PathMode::Blend { t: t.map(num) };
        p
    };
    assert!(semantic_diff(&blend(None), &blend(None), &cfg).is_empty());
    let d = semantic_diff(&blend(None), &blend(Some(0.5)), &cfg);
    assert!(has(&d, &Divergence::MetaField { field: "path_mode" }), "{d:?}");
    let d = semantic_diff(&blend(Some(0.5)), &blend(Some(0.6)), &cfg);
    assert!(has(&d, &Divergence::MetaField { field: "path_mode.t" }), "{d:?}");
    assert!(semantic_diff(&blend(Some(0.5)), &blend(Some(0.5)), &cfg).is_empty());

    // frame_preset: присутствие / values (допуск позиции) / wcs (символ)
    let preset = |x: f64, wcs: Wcs| {
        let mut p = prog(base_ops());
        p.frame_preset = Some(eid_core::ast::FramePreset { values: p3(x, 0.0, 5.0), wcs });
        p
    };
    let d = semantic_diff(&prog(base_ops()), &preset(0.0, Wcs::G54), &cfg);
    assert!(has(&d, &Divergence::MetaField { field: "frame_preset" }), "{d:?}");
    let d = semantic_diff(&preset(0.0, Wcs::G54), &preset(0.01, Wcs::G54), &cfg);
    assert!(has(&d, &Divergence::MetaField { field: "frame_preset.values" }), "{d:?}");
    assert!(semantic_diff(&preset(0.0, Wcs::G54), &preset(0.0005, Wcs::G54), &cfg).is_empty());
    let d = semantic_diff(&preset(0.0, Wcs::G54), &preset(0.0, Wcs::G55), &cfg);
    assert!(has(&d, &Divergence::MetaField { field: "frame_preset.wcs" }), "{d:?}");
}

#[test]
fn diff_state_fields_exact_and_tolerant() {
    let cfg = DiffConfig::default();
    // wcs — точное (символ)
    let mut b = prog(base_ops());
    if let Op::Linear { state, .. } = &mut b.ops[1] {
        state.wcs = Wcs::G55;
    }
    let d = semantic_diff(&prog(base_ops()), &b, &cfg);
    assert!(has(&d, &Divergence::StateField { index: 1, field: "wcs" }), "{d:?}");

    // spindle rpm: max(0.01, 1e-6·10000) = 0.02 → 10000.5 расходится, 10000.01 нет
    let mut c = prog(base_ops());
    if let Op::Linear { state, .. } = &mut c.ops[1] {
        state.spindle = eid_core::ast::Spindle::Cw(num(10000.5));
    }
    let d = semantic_diff(&prog(base_ops()), &c, &cfg);
    assert!(has(&d, &Divergence::StateField { index: 1, field: "spindle" }), "{d:?}");
    let mut w = prog(base_ops());
    if let Op::Linear { state, .. } = &mut w.ops[1] {
        state.spindle = eid_core::ast::Spindle::Cw(num(10000.01));
    }
    assert!(semantic_diff(&prog(base_ops()), &w, &cfg).is_empty());
    // off ↔ cw — смена режима, всегда расхождение
    let mut off = prog(base_ops());
    if let Op::Linear { state, .. } = &mut off.ops[1] {
        state.spindle = eid_core::ast::Spindle::Off;
    }
    let d = semantic_diff(&prog(base_ops()), &off, &cfg);
    assert!(has(&d, &Divergence::StateField { index: 1, field: "spindle" }), "{d:?}");

    // dwell: max(0.0001, 1e-5·t): 1.00005 ok; 1.001 расходится
    let dwell = |t: f64| {
        prog(vec![
            Op::Dwell { seconds: num(t), state: state(Some(150.0)), src: src(), sub: None },
            Op::ProgramEnd { src: src() },
        ])
    };
    assert!(semantic_diff(&dwell(1.0), &dwell(1.00005), &cfg).is_empty());
    let d = semantic_diff(&dwell(1.0), &dwell(1.001), &cfg);
    assert!(has(&d, &Divergence::Geometry { index: 0, field: "seconds" }), "{d:?}");
}

#[test]
fn diff_op_kind_mismatch() {
    let mut b = prog(base_ops());
    b.ops.swap(1, 2); // LINEAR vs ARC на позиции 1
    let d = semantic_diff(&prog(base_ops()), &b, &DiffConfig::default());
    assert!(has(&d, &Divergence::OpKind { index: 1 }), "{d:?}");
}

#[test]
fn diff_src_and_sub_are_provenance() {
    // src/sub не входят в эквивалентность (EQUIVALENCE §3)
    let mut b = prog(base_ops());
    if let Op::Linear { src, sub, .. } = &mut b.ops[1] {
        *src = SrcSpan { file: "other.nc".into(), line_start: 42, col_start: 1, line_end: 42, col_end: 9 };
        *sub = Some(3);
    }
    assert!(semantic_diff(&prog(base_ops()), &b, &DiffConfig::default()).is_empty());
}

// ===========================================================================
// manifest::load (задача 1)
// ===========================================================================

#[test]
fn manifest_loads_40_vectors_20_goldens() {
    // Динамические счётчики + sanity-минимумы: добавление векторов не ломает
    // фикстуру (инвариант — позитив с golden, негатив без; минимумы v0: >= 37
    // golden, >= 20 негативов).
    let m = load_manifest();
    let goldens = m.vectors.iter().filter(|v| v.golden.is_some()).count();
    let negatives = m.vectors.iter().filter(|v| v.golden.is_none()).count();
    assert!(goldens >= 37, "sanity: золотых минимум 37, сейчас {goldens}");
    assert!(negatives >= 20, "sanity: негативов минимум 20, сейчас {negatives}");
    assert_eq!(m.vectors.len(), goldens + negatives);
    // все пути файлов существуют (load также валидирует — двойная проверка)
    for v in &m.vectors {
        assert!(v.files.iter().all(|f| f.path.is_file()), "vector {}: file missing", v.id);
        if let Some(g) = &v.golden {
            assert!(g.is_file(), "vector {}: golden missing", v.id);
        }
    }
    // тир/kind/исходы
    for v in &m.vectors {
        assert!(matches!(v.tier, Tier::A));
        match v.kind {
            Kind::Positive => assert!(v.golden.is_some(), "positive {} без golden", v.id),
            Kind::Negative => assert!(v.golden.is_none(), "negative {} с golden", v.id),
        }
    }
}

#[test]
fn manifest_notice_and_r4_expect_per_corpus_1_1() {
    let m = load_manifest();
    let v = |id: &str| m.vectors.iter().find(|v| v.id == id).unwrap_or_else(|| panic!("no {id}"));
    // notice: true — только #13 (CORPUS §1.1)
    assert_eq!(
        m.vectors.iter().filter(|v| v.notice).map(|v| v.id.clone()).collect::<Vec<_>>(),
        vec!["13".to_string(), "43".to_string()]
    );
    // r4_expect непустой: #27 (grbl E_UNSUPPORTED_SYNTAX) и #47 (обе цели E_RAW_UNTRANSLATABLE)
    let v27 = v("27");
    assert_eq!(v27.r4_expect.get("grbl-1.1"), Some(&Outcome::Code(ErrorCode::UnsupportedSyntax)));
    let v47 = v("47");
    assert_eq!(v47.r4_expect.get("grbl-1.1"), Some(&Outcome::Code(ErrorCode::RawUntranslatable)));
    assert_eq!(v47.r4_expect.get("linuxcnc"), Some(&Outcome::Code(ErrorCode::RawUntranslatable)));
    // N3 — line-level негатив: Ok-исход + expected_diags (правило двух классов)
    let n3 = v("N3");
    assert!(matches!(n3.files[0].expect, Outcome::Ok));
    assert_eq!(n3.files[0].expected_diags, vec![ErrorCode::UnsupportedSyntax]);
    // N11 — Err-класс с CORPUS v0.5.2 (подпрограмма структурна: после M98
    // поток управления расходится): expect = код отказа, expected_diags пуст.
    let n11 = v("N11");
    assert_eq!(n11.files[0].expect, Outcome::Code(ErrorCode::UnsupportedSyntax));
    assert!(n11.files[0].expected_diags.is_empty());
    // Класс-списки CORPUS §1.2 v0.5.2: Err = {N1,N2,N4,N5a,N5b,N6,N7,N11,N12,
    // N13,N14,N15,N16,N17,N19}; line-level = {N3,N8,N9,N10,N18} — сверка
    // манифеста со списком (loader не знает о классах — проверяем форму).
    let err_class = ["N1", "N2", "N4", "N5a", "N5b", "N6", "N7", "N11", "N12", "N13", "N14", "N15", "N16", "N17", "N19"];
    let line_class = ["N3", "N8", "N9", "N10", "N18"];
    for id in err_class {
        let vv = v(id);
        for f in &vv.files {
            assert!(
                matches!(f.expect, Outcome::Code(_)) && f.expected_diags.is_empty(),
                "{id}: Err-класс, файл {:?}", f.path
            );
        }
    }
    for id in line_class {
        let vv = v(id);
        for f in &vv.files {
            assert!(
                matches!(f.expect, Outcome::Ok) && !f.expected_diags.is_empty(),
                "{id}: line-level класс, файл {:?}", f.path
            );
        }
    }
    // новые векторы присутствуют с ожидаемыми диалектами
    assert_eq!(v("47").files.len(), 1, "raw_crlf_fidelity — только GRBL-файл");
    assert_eq!(v("47").files[0].dialect, "grbl-1.1");
    assert_eq!(v("05").files.len(), 2);
    assert_eq!(v("13").files.len(), 2);
    // r4 warning-цели (CORPUS §1.1 #40/#41/#45): grbl = warning, не отказ
    assert_eq!(v("40").r4_expect.get("grbl-1.1"), Some(&Outcome::Warning));
    assert_eq!(v("41").r4_expect.get("grbl-1.1"), Some(&Outcome::Warning));
    assert_eq!(v("45").r4_expect.get("grbl-1.1"), Some(&Outcome::Warning));
}

#[test]
fn manifest_load_rejects_missing_path() {
    let dir = std::env::temp_dir().join("eid_harness_it2_manifest_bad");
    std::fs::create_dir_all(&dir).unwrap();
    let mf = dir.join("manifest.toml");
    std::fs::write(
        &mf,
        "corpus_version = \"t\"
[[vectors]]
id = \"1\"
name = \"x\"
tier = \"A\"
kind = \"positive\"
golden = \"nope/golden.eid\"
",
    )
    .unwrap();
    let err = manifest::load(&mf).unwrap_err();
    assert!(err.contains("does not exist"), "{err}");
    // invalid expect-строка: путь файла существует, строка "banana" — не исход
    std::fs::write(&dir.join("real.nc"), "mock").unwrap();
    std::fs::write(
        &mf,
        "corpus_version = \"t\"
[[vectors]]
id = \"1\"
name = \"x\"
tier = \"A\"
kind = \"positive\"
[[vectors.files]]
path = \"real.nc\"
dialect = \"grbl-1.1\"
expect = \"banana\"
",
    )
    .unwrap();
    let err = manifest::load(&mf).unwrap_err();
    assert!(err.contains("invalid expect"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

// ===========================================================================
// runner на реальном корпусе (задача 3: DoD — R1 зелёный на 20 goldens)
// ===========================================================================

#[test]
fn runner_r1_r2_green_on_all_20_goldens() {
    let m = load_manifest();
    let reg = Registry::new();
    let rep = runner::run(&m, &reg);
    assert!(rep.is_green(), "Fail'ы: {:?}", rep.vectors.iter()
        .flat_map(|v| v.checks.iter().filter(|c| matches!(c.status, Status::Fail(_))))
        .collect::<Vec<_>>());
    let golden_count = m.vectors.iter().filter(|v| v.golden.is_some()).count();
    assert!(golden_count >= 37, "sanity: golden минимум 37, сейчас {golden_count}");
    assert_eq!(rep.checks("R1").len(), golden_count, "R1 по одному на golden (динамически)");
    assert!(rep.checks("R1").iter().all(|c| c.status == Status::Pass), "R1 все Pass");
    assert_eq!(rep.checks("R2").len(), golden_count);
    assert!(rep.checks("R2").iter().all(|c| c.status == Status::Pass), "R2 все Pass");
    // диалектные проверки без адаптеров — Pending, не crash
    assert!(!rep.checks("parse").is_empty());
    assert!(rep.checks("parse").iter().all(|c| matches!(c.status, Status::Pending(_))));
    assert!(rep.checks("R3-golden").iter().all(|c| matches!(c.status, Status::Pending(_))));
    // R4 — динамически: по одной проверке на каждую цель из r4_expect манифеста
    let r4_expected_total: usize =
        m.vectors.iter().map(|v| v.r4_expect.len()).sum();
    let r4: Vec<_> = rep.checks("R4");
    assert_eq!(r4.len(), r4_expected_total, "R4 = сумма целей r4_expect (динамически)");
    assert!(r4_expected_total >= 5, "sanity: r4-целей минимум 5, сейчас {r4_expected_total}");
    // Пустой registry: все R4 — Pending (нет адаптеров), не crash.
    assert_eq!(r4.iter().filter(|c| matches!(c.status, Status::Pending(_))).count(), r4.len(),
        "без адаптеров все R4 Pending");
}

#[test]
fn golden_47_raw_payload_byte_exact() {
    let eid = EidTextAdapter::new();
    let path = golden_path("47_raw_crlf_fidelity/golden.eid");
    let text = std::fs::read_to_string(&path).unwrap();
    let p = eid.parse(&text, "golden47").unwrap();
    let payload = p.ops.iter().find_map(|o| match o {
        Op::Raw { payload_hex, code, .. } => Some((payload_hex.clone(), *code)),
        _ => None,
    });
    let (hex, code) = payload.expect("RAW op in golden 47");
    // '$$' + CRLF — вычислено реально: 24 24 0D 0A
    assert_eq!(hex, "24240D0A");
    assert_eq!(code, ErrorCode::UnsupportedSyntax);
    let emitted = eid.emit(&p).unwrap();
    assert_eq!(emitted, text, "emit(golden 47) байт-в-байт (payload не нормализуется, D9)");
    let p2 = eid.parse(&emitted, "golden47").unwrap();
    assert!(semantic_diff(&p, &p2, &DiffConfig::default()).is_empty());
}

#[test]
fn golden_13_is_canonical_without_dropped_op() {
    let eid = EidTextAdapter::new();
    let path = golden_path("13_arc_degenerate_zero_sweep/golden.eid");
    let text = std::fs::read_to_string(&path).unwrap();
    let p = eid.parse(&text, "golden13").unwrap();
    assert!(eid.take_notices().is_empty(), "golden AST каноничен: нулевой оп уже отброшен");
    assert_eq!(p.ops.len(), 3, "RAPID + LINEAR + PEND (дуга отсутствует)");
    assert!(matches!(p.ops[0], Op::Rapid { .. }));
    assert!(matches!(p.ops[1], Op::Linear { .. }));
    let emitted = eid.emit(&p).unwrap();
    assert_eq!(emitted, text);
}

#[test]
fn golden_10_is_inch() {
    let eid = EidTextAdapter::new();
    let text = std::fs::read_to_string(golden_path("10_arc_helix_inch/golden.eid")).unwrap();
    let p = eid.parse(&text, "golden10").unwrap();
    assert!(matches!(p.units, Units::In));
    let emitted = eid.emit(&p).unwrap();
    assert_eq!(emitted, text);
}

#[test]
fn all_goldens_roundtrip_diff_zero() {
    let eid = EidTextAdapter::new();
    let m = load_manifest();
    let cfg = DiffConfig::default();
    for v in m.vectors.iter().filter(|v| v.golden.is_some()) {
        let g = v.golden.as_ref().unwrap();
        let text = std::fs::read_to_string(g).unwrap();
        let p1 = eid.parse(&text, &v.id).expect("golden parses");
        assert!(eid.take_notices().is_empty(), "golden {} каноничен", v.id);
        let emitted = eid.emit(&p1).expect("golden emits");
        let p2 = eid.parse(&emitted, &v.id).expect("re-parses");
        assert!(
            semantic_diff(&p1, &p2, &cfg).is_empty(),
            "golden {}: parse(emit(parse)) diverges",
            v.id
        );
    }
}

// ===========================================================================
// runner: правило двух классов + R4 через mock-адаптеры (синтетический manifest)
// ===========================================================================

#[derive(Clone)]
enum Mock {
    /// Структурный негатив: parse отказывает с кодом.
    Refuse(ErrorCode),
    /// Line-level негатив: parse Ok, RAW-узел с кодом.
    OkWithRaw(ErrorCode),
    /// Parse Ok, чистая минимальная программа (только PEND).
    OkClean,
    /// parse/emit через eid parser/emitter (для R1-text) под чужим именем.
    RoundTrip,
}

struct MockAdapter {
    name: &'static str,
    behavior: Mock,
}

impl MockAdapter {
    fn new(name: &'static str, behavior: Mock) -> Self {
        Self { name, behavior }
    }
}

fn diag(code: ErrorCode) -> Diagnostic {
    Diagnostic { code, message: "mock refusal".into(), src: None, options: vec![], limit: None }
}

fn mock_program(raw: Option<ErrorCode>) -> Program {
    let s = src();
    let mut ops = Vec::new();
    if let Some(code) = raw {
        ops.push(Op::Raw {
            payload_hex: "4142".into(),
            reason: "mock raw".into(),
            code,
            src: s.clone(),
        });
    }
    ops.push(Op::ProgramEnd { src: s });
    Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: PathMode::ExactPath,
        frame_preset: None,
        ops,
    }
}

impl Adapter for MockAdapter {
    fn dialect_name(&self) -> &'static str {
        self.name
    }
    fn parse(&self, s: &str, f: &str) -> Result<Program, Vec<Diagnostic>> {
        match &self.behavior {
            Mock::Refuse(code) => Err(vec![diag(*code)]),
            Mock::OkWithRaw(code) => Ok(mock_program(Some(*code))),
            Mock::OkClean => Ok(mock_program(None)),
            Mock::RoundTrip => eid_core::parser::parse_with_notices(s, f).map(|o| o.program),
        }
    }
    fn emit(&self, p: &Program) -> Result<String, Vec<Diagnostic>> {
        match &self.behavior {
            Mock::Refuse(code) => Err(vec![diag(*code)]),
            Mock::RoundTrip => eid_core::emitter::emit(p),
            // «Эмитит успешно, но текст не парсится обратно» — для Fail-путей R1-text
            _ => Ok("EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET none\nOP 001 PEND\nEND\n".to_string()),
        }
    }
}

const MINIMAL_GOLDEN: &str = "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET none\nOP 001 PEND\nEND\n";

fn vec_negative(id: &str, file: &Path, expect: Outcome, diags: Vec<ErrorCode>) -> Vector {
    Vector {
        id: id.into(),
        name: "mock".into(),
        tier: Tier::A,
        kind: Kind::Negative,
        golden: None,
        files: vec![FileEntry {
            path: file.to_path_buf(),
            dialect: "mock".into(),
            expect,
            expected_diags: diags,
            sha256: None,
        }],
        r4_expect: BTreeMap::new(),
        notice: false,
    }
}

#[test]
fn runner_two_class_negatives() {
    let f = tmp_file("neg.txt", "mock input");
    let mut reg = Registry::new();
    reg.register(Arc::new(MockAdapter::new("mock", Mock::Refuse(ErrorCode::RuntimeState))));
    reg.register(Arc::new(MockAdapter::new("mock-raw-sem", Mock::OkWithRaw(ErrorCode::UnsupportedSemantics))));
    reg.register(Arc::new(MockAdapter::new("mock-clean", Mock::OkClean)));

    // Структурный: Err(code == expect) → Pass
    let m1 = Manifest {
        corpus_version: "t".into(),
        vectors: vec![vec_negative("S1", &f, Outcome::Code(ErrorCode::RuntimeState), vec![])],
    };
    // Структурный: Ok там, где ждём отказ → Fail
    let m2 = Manifest {
        corpus_version: "t".into(),
        vectors: vec![vec_negative(
            "S2",
            &f,
            Outcome::Code(ErrorCode::RuntimeState),
            vec![],
        )],
    };
    // Line-level: Ok + RAW{code} ∈ expected_diags → Pass
    let m3 = Manifest {
        corpus_version: "t".into(),
        vectors: vec![vec_negative(
            "L1",
            &f,
            Outcome::Ok,
            vec![ErrorCode::UnsupportedSemantics],
        )],
    };
    // Line-level: Ok, но RAW-код не покрывает expected_diags → Fail
    let m4 = Manifest {
        corpus_version: "t".into(),
        vectors: vec![vec_negative(
            "L2",
            &f,
            Outcome::Ok,
            vec![ErrorCode::UnsupportedSemantics],
        )],
    };

    // point registry entries at the right behaviors per manifest
    let mut m1 = m1;
    m1.vectors[0].files[0].dialect = "mock".into();
    let mut m3 = m3;
    m3.vectors[0].files[0].dialect = "mock-raw-sem".into();

    let rep1 = runner::run(&m1, &reg);
    assert!(rep1.is_green(), "{:?}", rep1.vectors);

    // m2: тот же диалект "mock" (Refuse) — но ждём Ok → Fail... строим отдельно
    let mut reg2 = Registry::new();
    reg2.register(Arc::new(MockAdapter::new("mock", Mock::OkClean)));
    let rep2 = runner::run(&m2, &reg2);
    assert!(!rep2.is_green());
    assert!(matches!(rep2.vectors[0].checks[0].status, Status::Fail(_)));

    let rep3 = runner::run(&m3, &reg);
    assert!(rep3.is_green(), "{:?}", rep3.vectors);

    let mut m4 = m4;
    m4.vectors[0].files[0].dialect = "mock-clean".into(); // OkClean: нет RAW-кода
    let rep4 = runner::run(&m4, &reg);
    assert!(!rep4.is_green());
    match &rep4.vectors[0].checks[0].status {
        Status::Fail(msg) => assert!(msg.contains("missing"), "{msg}"),
        s => panic!("expected Fail, got {s:?}"),
    }
    std::fs::remove_file(&f).ok();
}

#[test]
fn runner_positive_r3_and_r1_text_via_mock() {
    let golden = tmp_file("golden_min.eid", MINIMAL_GOLDEN);
    let raw_file = tmp_file(
        "raw.eid",
        "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET none\nOP 001 RAW reason=\"mock\" code=E_UNSUPPORTED_SYNTAX src=0.1-0.0 payload=414243\nOP 002 PEND\nEND\n",
    );
    let mut reg = Registry::new();
    // RoundTrip-адаптер: parse/emit через eid parser/emitter под именем "mock-eid"
    reg.register(Arc::new(MockAdapter::new("mock-eid", Mock::RoundTrip)));
    reg.register(Arc::new(MockAdapter::new("mock-diverge", Mock::OkClean)));

    let mk = |id: &str, dialect: &'static str, file: &Path, golden: &Path, notice: bool| Vector {
        id: id.into(),
        name: "mock".into(),
        tier: Tier::A,
        kind: Kind::Positive,
        golden: Some(golden.to_path_buf()),
        files: vec![FileEntry {
            path: file.to_path_buf(),
            dialect: dialect.into(),
            expect: Outcome::Ok,
            expected_diags: vec![],
            sha256: None,
        }],
        r4_expect: BTreeMap::new(),
        notice,
    };

    // R3: parse(mock-eid файл) ≡ golden → Pass (минимальная программа)
    let v1 = mk("P1", "mock-eid", &golden, &golden, false);
    let m1 = Manifest { corpus_version: "t".into(), vectors: vec![v1] };
    let rep1 = runner::run(&m1, &reg);
    assert!(rep1.is_green(), "{:?}", rep1.vectors);
    assert!(rep1.checks("R3-golden").iter().all(|c| c.status == Status::Pass), "{:?}", rep1.checks("R3-golden"));
    assert!(rep1.checks("R1").iter().all(|c| c.status == Status::Pass));

    // R3 Fail: parse даёт RAW-оп, golden — нет
    let v2 = mk("P2", "mock-eid", &raw_file, &golden, false);
    let m2 = Manifest { corpus_version: "t".into(), vectors: vec![v2] };
    let rep2 = runner::run(&m2, &reg);
    assert!(!rep2.is_green());
    let r3 = rep2.checks("R3-golden")[0].clone();
    assert!(matches!(r3.status, Status::Fail(_)), "{r3:?}");

    // R1-text: RAW-файл, same-dialect round-trip payload байт-в-байт → Pass
    let v3 = mk("P3", "mock-eid", &raw_file, &raw_file, false);
    let m3 = Manifest { corpus_version: "t".into(), vectors: vec![v3] };
    let rep3 = runner::run(&m3, &reg);
    let r1t = rep3.checks("R1-text");
    assert_eq!(r1t.len(), 1);
    assert!(matches!(r1t[0].status, Status::Pass), "{:?}", r1t[0]);

    // R1-text исключён при notice: true (CORPUS §4) → Skipped
    let v4 = mk("P4", "mock-eid", &raw_file, &raw_file, true);
    let m4 = Manifest { corpus_version: "t".into(), vectors: vec![v4] };
    let rep4 = runner::run(&m4, &reg);
    let r1t = rep4.checks("R1-text");
    assert_eq!(r1t.len(), 1);
    assert!(matches!(r1t[0].status, Status::Skipped(_)), "{:?}", r1t[0]);

    std::fs::remove_file(&golden).ok();
    std::fs::remove_file(&raw_file).ok();
}

#[test]
fn runner_r4_emission_expectations() {
    let golden = tmp_file("r4_golden.eid", MINIMAL_GOLDEN);
    let f = tmp_file("r4_input.txt", "mock");
    let mut reg = Registry::new();
    reg.register(Arc::new(MockAdapter::new("mock-refuse", Mock::Refuse(ErrorCode::UnsupportedSyntax))));
    reg.register(Arc::new(MockAdapter::new("mock-clean", Mock::OkClean)));

    let mk_r4 = |id: &str, dialect: &str, expect: Outcome| Vector {
        id: id.into(),
        name: "r4".into(),
        tier: Tier::A,
        kind: Kind::Positive,
        golden: Some(golden.clone()),
        files: vec![FileEntry {
            path: f.clone(),
            dialect: "unused".into(), // нет такого адаптера → parse Pending
            expect: Outcome::Ok,
            expected_diags: vec![],
            sha256: None,
        }],
        r4_expect: [(dialect.to_string(), expect)].into_iter().collect(),
        notice: false,
    };

    // Эмиссия отказывает ожидаемым кодом → Pass (#27-диспозиция)
    let m1 = Manifest {
        corpus_version: "t".into(),
        vectors: vec![mk_r4("R4a", "mock-refuse", Outcome::Code(ErrorCode::UnsupportedSyntax))],
    };
    let rep1 = runner::run(&m1, &reg);
    let r4 = rep1.checks("R4");
    assert_eq!(r4.len(), 1);
    assert!(matches!(r4[0].status, Status::Pass), "{:?}", r4[0]);

    // Эмиссия успешна, а ждали отказ → Fail
    let m2 = Manifest {
        corpus_version: "t".into(),
        vectors: vec![mk_r4("R4b", "mock-clean", Outcome::Code(ErrorCode::UnsupportedSyntax))],
    };
    let rep2 = runner::run(&m2, &reg);
    let r4 = rep2.checks("R4");
    assert!(matches!(r4[0].status, Status::Fail(_)), "{:?}", r4[0]);

    // r4_expect у негатива (без golden AST) → Skipped, не crash (N3-диспозиция)
    let mut neg = vec_negative("R4n", &f, Outcome::Code(ErrorCode::RuntimeState), vec![]);
    neg.r4_expect.insert("mock-refuse".to_string(), Outcome::Code(ErrorCode::RawUntranslatable));
    neg.files[0].dialect = "mock-refuse".into();
    let m3 = Manifest { corpus_version: "t".into(), vectors: vec![neg] };
    let rep3 = runner::run(&m3, &reg);
    let r4 = rep3.checks("R4");
    assert_eq!(r4.len(), 1);
    assert!(matches!(r4[0].status, Status::Skipped(_)), "{:?}", r4[0]);

    // Нет адаптера для цели r4_expect → Pending
    let m4 = Manifest {
        corpus_version: "t".into(),
        vectors: vec![mk_r4("R4p", "absent-dialect", Outcome::Ok)],
    };
    let rep4 = runner::run(&m4, &reg);
    let r4 = rep4.checks("R4");
    assert!(matches!(r4[0].status, Status::Pending(_)), "{:?}", r4[0]);

    std::fs::remove_file(&golden).ok();
    std::fs::remove_file(&f).ok();
}

/// Модель целевого адаптера для R4-негативов: эмиссия программы с RAW-опом
/// отказывает E_RAW_UNTRANSLATABLE (SPEC §8.2; CORPUS v0.5.1 — N3/#47),
/// без RAW — успешна.
struct RawRefuseAdapter;

impl Adapter for RawRefuseAdapter {
    fn dialect_name(&self) -> &'static str {
        "mock-rawrefuse"
    }
    fn parse(&self, _s: &str, _f: &str) -> Result<Program, Vec<Diagnostic>> {
        Ok(mock_program(None))
    }
    fn emit(&self, p: &Program) -> Result<String, Vec<Diagnostic>> {
        if p.ops.iter().any(|o| matches!(o, Op::Raw { .. })) {
            Err(vec![diag(ErrorCode::RawUntranslatable)])
        } else {
            Ok(MINIMAL_GOLDEN.to_string())
        }
    }
}

#[test]
fn runner_r4_negative_file_as_source() {
    // CORPUS v0.5.1: негатив с r4_expect — parse(файл) → Ok → эмиссия AST
    // в каждую цель → ожидаемый код. Заменяет прежний Skipped при golden=null.
    let f = tmp_file("r4neg.txt", "mock raw input");
    let mut reg = Registry::new();
    reg.register(Arc::new(MockAdapter::new("mock-raw-sem", Mock::OkWithRaw(ErrorCode::UnsupportedSemantics))));
    reg.register(Arc::new(MockAdapter::new("mock-clean", Mock::OkClean)));
    reg.register(Arc::new(RawRefuseAdapter));

    let mk_neg = |dialect: &str, expect: Outcome, diags: Vec<ErrorCode>| Vector {
        id: "RN".into(),
        name: "r4neg".into(),
        tier: Tier::A,
        kind: Kind::Negative,
        golden: None,
        files: vec![FileEntry {
            path: f.clone(),
            dialect: dialect.into(),
            expect,
            expected_diags: diags,
            sha256: None,
        }],
        r4_expect: [("mock-rawrefuse".to_string(), Outcome::Code(ErrorCode::RawUntranslatable))]
            .into_iter()
            .collect(),
        notice: false,
    };

    // Line-level негатив (Ok + RAW в AST), v0.5.3: same-dialect цель → Skipped
    // (RAW-эмиссия легальна, verbatim), кросс-цель → ожидаемый код отказа.
    let mut neg = mk_neg("mock-raw-sem", Outcome::Ok, vec![ErrorCode::UnsupportedSemantics]);
    neg.r4_expect.insert("mock-raw-sem".to_string(), Outcome::Code(ErrorCode::RawUntranslatable));
    let m1 = Manifest { corpus_version: "t".into(), vectors: vec![neg] };
    let rep1 = runner::run(&m1, &reg);
    let r4 = rep1.checks("R4");
    assert_eq!(r4.len(), 2, "{:?}", rep1.vectors);
    // same-dialect (source mock-raw-sem → цель mock-raw-sem): Skipped/not-applicable.
    let same = r4.iter().find(|c| c.target.ends_with("→mock-raw-sem")).expect("same-dialect check");
    assert!(matches!(same.status, Status::Skipped(_)), "{:?}", same);
    // кросс (source mock-raw-sem → цель mock-rawrefuse): Pass — E_RAW_UNTRANSLATABLE.
    let cross = r4.iter().find(|c| c.target.ends_with("→mock-rawrefuse")).expect("cross check");
    assert!(matches!(cross.status, Status::Pass), "{:?}", cross);
    // Источник — файл, не golden.
    assert!(
        !cross.target.starts_with("golden"),
        "{}",
        cross.target
    );

    // Тот же негатив, но эмиссия успешна (AST без RAW) → Fail.
    let m2 = Manifest {
        corpus_version: "t".into(),
        vectors: vec![mk_neg("mock-clean", Outcome::Ok, vec![])],
    };
    let rep2 = runner::run(&m2, &reg);
    let r4 = rep2.checks("R4");
    assert_eq!(r4.len(), 1);
    match &r4[0].status {
        Status::Fail(msg) => assert!(msg.contains("expected emit refusal E_RAW_UNTRANSLATABLE"), "{msg}"),
        s => panic!("expected Fail, got {s:?}"),
    }

    // Структурный негатив с r4_expect: нет Ok-источника (parse = Err) → Skipped.
    let mut nstruct = vec_negative("RS", &f, Outcome::Code(ErrorCode::RuntimeState), vec![]);
    nstruct.files[0].dialect = "mock-refuse".into();
    nstruct.r4_expect.insert("mock-rawrefuse".to_string(), Outcome::Code(ErrorCode::RawUntranslatable));
    let m3 = Manifest { corpus_version: "t".into(), vectors: vec![nstruct] };
    let mut reg3 = Registry::new();
    reg3.register(Arc::new(MockAdapter::new("mock-refuse", Mock::Refuse(ErrorCode::RuntimeState))));
    reg3.register(Arc::new(RawRefuseAdapter));
    let rep3 = runner::run(&m3, &reg3);
    let r4 = rep3.checks("R4");
    assert_eq!(r4.len(), 1);
    assert!(matches!(r4[0].status, Status::Skipped(_)), "{:?}", r4[0]);

    std::fs::remove_file(&f).ok();
}

#[test]
fn registry_basics() {
    let mut reg = Registry::new();
    assert!(reg.get("mock").is_none());
    reg.register(Arc::new(MockAdapter::new("mock", Mock::OkClean)));
    assert!(reg.get("mock").is_some());
    assert_eq!(reg.dialects(), vec!["mock"]);
}
