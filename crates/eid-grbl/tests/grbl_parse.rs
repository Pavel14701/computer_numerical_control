//! Интеграционные тесты канонизатора GRBL → EID/L1 (этап 2, Агент C).
//! Пилоты и негативы — corpus/tier_a/v0; допуски — EQUIVALENCE §4 (eps 0.001).

use eid_core::ast::{Op, Point3, Program, Spindle, StartPoint, Units, Wcs};
use eid_core::diag::ErrorCode;
use eid_grbl::canonicalize::{canonicalize, canonicalize_with_notices};

const EPS: f64 = 0.001;

fn parse(src: &str) -> Result<Program, Vec<Diagnostic>> {
    canonicalize(src, "test.ngc")
}

use eid_core::diag::Diagnostic;

fn err_code(r: Result<Program, Vec<Diagnostic>>) -> ErrorCode {
    let Err(d) = r else { panic!("ожидался Err") };
    d[0].code
}

fn pt3(p: &Point3) -> [f64; 3] {
    [p.x.value(), p.y.value(), p.z.value()]
}

fn assert_pt(p: &Point3, x: f64, y: f64, z: f64) {
    let v = pt3(p);
    assert!((v[0] - x).abs() <= EPS, "{v:?} vs ({x},{y},{z})");
    assert!((v[1] - y).abs() <= EPS, "{v:?} vs ({x},{y},{z})");
    assert!((v[2] - z).abs() <= EPS, "{v:?} vs ({x},{y},{z})");
}

fn known(op: &Op) -> [f64; 3] {
    match op {
        Op::Rapid { start: StartPoint::Known(s), .. }
        | Op::Linear { start: StartPoint::Known(s), .. }
        | Op::Arc { start: StartPoint::Known(s), .. } => pt3(s),
        other => panic!("start Unknown у {other:?}"),
    }
}

fn feed_of(op: &Op) -> Option<f64> {
    match op {
        Op::Rapid { state, .. } | Op::Linear { state, .. } | Op::Arc { state, .. } => {
            state.feed.as_ref().map(|n| n.value())
        }
        _ => None,
    }
}

fn corpus(rel: &str) -> String {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tier_a/v0");
    let p = base.join(rel);
    if p.is_file() {
        return std::fs::read_to_string(&p).expect("read");
    }
    let mut hit = None;
    for e in std::fs::read_dir(&p).expect(rel) {
        let f = e.expect("entry").path();
        if f.file_name().unwrap().to_string_lossy().ends_with(".grbl.ngc") {
            hit = Some(std::fs::read_to_string(&f).expect("read"));
        }
    }
    hit.expect("no .grbl.ngc in dir")
}

// ---------------------------------------------------------------------------
// Пилоты
// ---------------------------------------------------------------------------

#[test]
fn pilot_01_linear_box() {
    let p = parse(&corpus("01_linear_box")).expect("ok");
    // G0 + 4×G1 + ProgramEnd
    assert_eq!(p.ops.len(), 6);
    assert!(matches!(p.ops[0], Op::Rapid { .. }));
    for i in 1..=4 {
        assert!(matches!(p.ops[i], Op::Linear { .. }), "op{i} не Linear");
    }
    assert!(matches!(p.ops[5], Op::ProgramEnd { .. }));
    assert_eq!(p.units, Units::Mm);
    // state-снапшоты: 2-й linear (Y10 F250) несёт F250, coolant flood, шпиндель cw.
    let lin2 = match &p.ops[2] {
        Op::Linear { start, end, state, .. } => (start, end, state),
        _ => unreachable!(),
    };
    assert!(matches!(lin2.0, StartPoint::Known(_)));
    assert_eq!(feed_of(&p.ops[2]), Some(250.0));
    assert!(lin2.2.coolant.flood);
    assert_eq!(lin2.2.spindle, Spindle::Cw(eid_core::ast::Num::new("10000".into(), 10000.0)));
    assert_eq!(lin2.2.wcs, Wcs::G54);
    assert_eq!(lin2.2.frame, eid_core::ast::Frame::Wcs);
    assert_eq!(lin2.2.tool, None);
    // связывание: start первого linear = end rapid'а (0,0,5)
    assert!(matches!(&p.ops[0], Op::Rapid { start: StartPoint::Unknown, .. }));
    match &p.ops[0] {
        Op::Rapid { end, .. } => assert_pt(end, 0.0, 0.0, 5.0),
        _ => unreachable!(),
    }
    let _ = known(&p.ops[1]);
    assert_eq!(known(&p.ops[1]), [0.0, 0.0, 5.0]);
}

#[test]
fn pilot_02_rapid_reposition() {
    let p = parse(&corpus("02_rapid_reposition")).expect("ok");
    // 8 ходов + ProgramEnd
    assert_eq!(p.ops.len(), 9);
    let rapids = p.ops.iter().filter(|o| matches!(o, Op::Rapid { .. })).count();
    let linears = p.ops.iter().filter(|o| matches!(o, Op::Linear { .. })).count();
    assert_eq!(rapids, 6);
    assert_eq!(linears, 2);
    assert_eq!(feed_of(&p.ops[2]), Some(80.0)); // G1 X5 F80
}

#[test]
fn pilot_03_feed_survives_rapid() {
    let p = parse(&corpus("03_feed_survives_rapid")).expect("ok");
    assert_eq!(p.ops.len(), 4); // G1, G0, G1, M2
    // Ключевой вектор: rapid несёт feed=500 (G0 не сбрасывает, SPEC §4).
    assert_eq!(feed_of(&p.ops[1]), Some(500.0));
    assert!(matches!(&p.ops[1], Op::Rapid { .. }));
    assert_eq!(feed_of(&p.ops[2]), Some(500.0));
}

#[test]
fn pilot_36_preset_g54_then_switch() {
    let p = parse(&corpus("36_preset_G54_then_switch")).expect("ok");
    // frame_preset{X0 Y0 Z5, wcs=G54} — узел Program
    let fp = p.frame_preset.as_ref().expect("frame_preset есть");
    assert_pt(&fp.values, 0.0, 0.0, 5.0);
    assert_eq!(fp.wcs, Wcs::G54);
    // op0 G0: start Known(0,0,5) — якорь пресетки; op1 G1 X10: chaining.
    assert_eq!(known(&p.ops[0]), [0.0, 0.0, 5.0]);
    assert_eq!(known(&p.ops[1]), [0.0, 0.0, 0.0]);
    // op2/op4 — смена wcs без in-text origins → точки разрыва (b): Unknown.
    assert!(matches!(&p.ops[2], Op::Rapid { start: StartPoint::Unknown, .. }));
    assert!(matches!(&p.ops[4], Op::Rapid { start: StartPoint::Unknown, .. }));
    // op3/op5 — chaining внутри той же WCS.
    assert_eq!(known(&p.ops[3]), [10.0, 0.0, 5.0]);
    assert_eq!(known(&p.ops[5]), [20.0, 0.0, 0.0]);
    // wcs в state: op1 — G54, op3 — G55, op5 — G54.
    let wcs_of = |i: usize| match &p.ops[i] {
        Op::Rapid { state, .. } | Op::Linear { state, .. } => state.wcs,
        _ => unreachable!(),
    };
    assert_eq!(wcs_of(1), Wcs::G54);
    assert_eq!(wcs_of(3), Wcs::G55);
    assert_eq!(wcs_of(5), Wcs::G54);
}

#[test]
fn pilot_46_arc_r_helix() {
    let p = parse(&corpus("46_arc_R_helix")).expect("ok");
    assert_eq!(p.ops.len(), 3); // G0, G2, M2
    match &p.ops[1] {
        Op::Arc { start, end, center, normal, dir, sweep_deg, pitch, .. } => {
            assert_eq!(known(&p.ops[1]), [0.0, 0.0, 0.0]);
            assert!(matches!(start, StartPoint::Known(_)));
            assert_pt(end, -5.0, 5.0, 2.0);
            assert_pt(center, 0.0, 5.0, 0.0); // R-декод (формула grbl/LinuxCNC)
            assert_pt(normal, 0.0, 0.0, 1.0); // G17 → +Z
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS, "sweep {}", sweep_deg.value());
            assert!((pitch.value() - 2.0).abs() <= EPS, "pitch {}", pitch.value());
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn pilot_11_arc_r_semicircle() {
    // SPEC v0.11: golden перегенерирован — центр/sweep ВЫВЕДЕНЫ из 4dp-концов
    // по §6 (single-final-rounding §10.5): center (-0.0351, 0.0001, 0),
    // sweep 179.4975 (sweep R-формы — из хорды и |R|, не от округлённого центра).
    let out = canonicalize_with_notices(&corpus("11_arc_R_semicircle"), "11.ngc").expect("ok");
    let p = out.program;
    assert_eq!(p.ops.len(), 5); // G0, G0, G2, G0, ProgramEnd
    let arc = &p.ops[2];
    match arc {
        Op::Arc { center, sweep_deg, dir, pitch, normal, .. } => {
            assert_pt(center, -0.0351, 0.0001, 0.0);
            assert!((sweep_deg.value() - 179.4975).abs() <= 0.001, "sweep {}", sweep_deg.value());
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
            assert!((pitch.value()).abs() <= EPS);
            assert_pt(normal, 0.0, 0.0, 1.0);
        }
        _ => panic!("op2 не Arc"),
    }
    // Радиус-валидация прошла: |start − center| в плоскости = R = 10.
    let s = known(arc);
    let c = [center_val(arc).0, center_val(arc).1];
    let rho = ((s[0] - c[0]).powi(2) + (s[1] - c[1]).powi(2)).sqrt();
    assert!((rho - 10.0).abs() <= EPS, "ρ_start {rho}");
}

fn center_val(op: &Op) -> (f64, f64) {
    match op {
        Op::Arc { center, .. } => (center.x.value(), center.y.value()),
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Негативы (правило двух классов, CORPUS §1.2 / SPEC §8 v0.10)
// ---------------------------------------------------------------------------

#[test]
fn negative_n2_mid_g92_is_semantics() {
    assert_eq!(err_code(parse(&corpus("n2_mid_G92"))), ErrorCode::UnsupportedSemantics);
}

#[test]
fn negative_n3_config_is_ok_with_raw() {
    let p = parse(&corpus("n3_raw_vendor_block")).expect("n3: Ok + Raw-узел");
    assert!(matches!(&p.ops[0], Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }));
    // Остальное — обычная программа.
    assert!(matches!(&p.ops[1], Op::Rapid { .. }));
    assert!(matches!(&p.ops[2], Op::Linear { .. }));
    assert!(matches!(&p.ops[3], Op::ProgramEnd { .. }));
}

#[test]
fn negative_n5_arcs_invalid() {
    assert_eq!(err_code(parse(&corpus("n5a_arc_radius_mismatch"))), ErrorCode::InvalidInput);
    assert_eq!(err_code(parse(&corpus("n5b_arc_chord_gt_2R"))), ErrorCode::InvalidInput);
}

#[test]
fn negative_n7_unit_switch_semantics() {
    assert_eq!(err_code(parse(&corpus("n7_unit_switch_mid"))), ErrorCode::UnsupportedSemantics);
}

#[test]
fn negative_n12_13_14_program_end_structure() {
    assert_eq!(err_code(parse(&corpus("n12_program_end_not_last"))), ErrorCode::InvalidInput);
    assert_eq!(err_code(parse(&corpus("n13_program_end_duplicate"))), ErrorCode::InvalidInput);
    assert_eq!(err_code(parse(&corpus("n14_program_end_missing"))), ErrorCode::InvalidInput);
}

#[test]
fn negative_n15_n16_n17_frames() {
    assert_eq!(err_code(parse(&corpus("n15_wcs_set_here_mid"))), ErrorCode::UnsupportedSemantics);
    assert_eq!(err_code(parse(&corpus("n16_g53_after_preset"))), ErrorCode::RuntimeState);
    assert_eq!(err_code(parse(&corpus("n17_multiple_preset_at_start"))), ErrorCode::UnsupportedSemantics);
}

#[test]
fn negative_n19_r_full_circle_invalid() {
    assert_eq!(err_code(parse(&corpus("n19_arc_R_full_circle"))), ErrorCode::InvalidInput);
}

// ---------------------------------------------------------------------------
// SPEC v0.12: дефолт path_mode, установление единиц, квантование входа
// ---------------------------------------------------------------------------

#[test]
fn spec_v012_path_mode_default_exact_path() {
    // SPEC §3 v0.12: отсутствие G61/G61.1/G64 → exact_path.
    let p = parse(&corpus("01_linear_box/linear_box.grbl.ngc")).expect("ok");
    assert_eq!(p.path_mode, eid_core::ast::PathMode::ExactPath);
}

#[test]
fn spec_v012_first_units_word_establishes() {
    // CORPUS #10: G20 первым словом — легальный inch-файл (не смена единиц).
    let p = parse(&corpus("10_arc_helix_inch/arc_helix_inch.grbl.ngc")).expect("ok");
    assert_eq!(p.units, Units::In);
    match &p.ops[1] {
        Op::Arc { pitch, sweep_deg, .. } => {
            assert!((pitch.value() - 0.1).abs() <= EPS, "pitch {}", pitch.value());
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
    // n7 (G21 в шапке → G20 после) по-прежнему Err SEMANTICS.
    assert_eq!(err_code(parse(&corpus("n7_unit_switch_mid"))), ErrorCode::UnsupportedSemantics);
}

#[test]
fn spec_v0106_input_quantization_drops_near_zero_arc() {
    // CORPUS #13: end 0.00004 → 0.0000 (§10.6) → zero-length arc → drop+notice.
    let out = canonicalize_with_notices(
        &corpus("13_arc_degenerate_zero_sweep/arc_degenerate_zero_sweep.grbl.ngc"),
        "13.ngc",
    )
    .expect("ok");
    // G0, [дуга отброшена], G1 X10, M2 → 3 опа.
    assert_eq!(out.program.ops.len(), 3);
    assert_eq!(out.notices.len(), 1);
    assert_eq!(out.notices[0].kind, eid_grbl::canonicalize::NoticeKind::ZeroLengthArc);
}

#[test]
fn corpus_47_dollar_dollar_line_is_raw() {
    // CORPUS #47: голое `$$` (dump settings) → ConfigLine → Raw{SYNTAX}.
    let out = canonicalize_with_notices(
        &corpus("47_raw_crlf_fidelity/raw_crlf_fidelity.grbl.ngc"),
        "47.ngc",
    )
    .expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }
    ));
    // Полная CRLF-программа продолжает разбираться: Rapid/Linear/…/M2.
    assert!(matches!(&out.program.ops[1], Op::Rapid { .. }));
}

#[test]
fn corpus_08_full_circle_without_end_words_kept() {
    // CORPUS #08: «G2 I-5.0 J0.0» без end-слов — настоящее полное кольцо 360°.
    let out = canonicalize_with_notices(
        &corpus("08_arc_full_circle/arc_full_circle.grbl.ngc"),
        "08.ngc",
    )
    .expect("ok");
    assert_eq!(out.notices.len(), 0);
    match &out.program.ops[1] {
        Op::Arc { sweep_deg, .. } => {
            assert!((sweep_deg.value() - 360.0).abs() <= EPS, "sweep {}", sweep_deg.value());
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn corpus_37_g53_known_origins_breakpoint_c() {
    // CORPUS #37 (SPEC §2(c) v0.10): первый machine-ход при выполненных §5.3 —
    // точка разрыва: RAPID start=unknown (не отказ); последующие цепляются.
    let out = canonicalize_with_notices(&corpus("37_g53_known_origins/g53_known_origins.grbl.ngc"), "37.ngc")
        .expect("parse Ok");
    let ops = &out.program.ops;
    // ops: [WcsDefine, Rapid(G53), Linear(G53), ProgramEnd]
    assert_eq!(ops.len(), 4);
    match &ops[1] {
        Op::Rapid { start: StartPoint::Unknown, end, state, .. } => {
            assert_pt(end, 10.0, 0.0, 0.0);
            assert_eq!(state.frame, eid_core::ast::Frame::Machine);
        }
        _ => panic!("op1 не machine Rapid"),
    }
    // Второй machine-ход цепляется от первого.
    match &ops[2] {
        Op::Linear { state, .. } => {
            assert_eq!(known(&ops[2]), [10.0, 0.0, 0.0]);
            assert_eq!(state.frame, eid_core::ast::Frame::Machine);
            assert_eq!(state.feed.as_ref().map(|n| n.value()), Some(150.0));
        }
        _ => panic!("op2 не machine Linear"),
    }
    // N1 (без origins) остаётся Err RUNTIME — §5.3b-диспозиция диалекта.
    assert_eq!(err_code(parse(&corpus("n1_g53_no_origins/g53_no_origins.grbl.ngc"))), ErrorCode::RuntimeState);
}

// ---------------------------------------------------------------------------
// Юнит: модальные режимы и связывание (SPEC §2/§4)
// ---------------------------------------------------------------------------

#[test]
fn unit_g93_inverse_time_resolution() {
    // G93: feed = len(траектория) × F (SPEC §4): len 10 × F2 = 20.
    let p = parse("G21 G90 G93 G17\nG92 X0 Y0 Z0\nG1 X10.0 F2.0\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Linear { state, .. } => {
            let f = state.feed.as_ref().expect("feed есть").value();
            assert!((f - 20.0).abs() <= EPS, "G93 feed {f} != 20");
        }
        _ => panic!("op0 не Linear"),
    }
}

#[test]
fn unit_g91_incremental_resolution() {
    let p = parse("G21 G91 G17\nG92 X1 Y2 Z3\nG1 X1.0 Y1.0 Z1.0 F100.0\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Linear { start, end, .. } => {
            assert_eq!(known(&p.ops[0]), [1.0, 2.0, 3.0]); // якорь G92
            assert_pt(end, 2.0, 3.0, 4.0); // инкремент разрешён в абсолют
            let _ = start;
        }
        _ => panic!("op0 не Linear"),
    }
}

#[test]
fn unit_zero_length_linear_dropped_with_notice() {
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG92 X0 Y0 Z0\nG1 X0.0 Y0.0 Z0.0 F100.0\nM2\n",
        "z.ngc",
    )
    .expect("ok");
    // Linear отброшен; остаются только ProgramEnd.
    assert!(out.program.ops.iter().all(|o| matches!(o, Op::ProgramEnd { .. })));
    assert_eq!(out.notices.len(), 1);
    assert_eq!(out.notices[0].kind, eid_grbl::canonicalize::NoticeKind::ZeroLengthMotion);
}

#[test]
fn unit_cross_wcs_linking_arithmetic() {
    // (b): обе системы in-text → start = prev_end + O_prev − O_new.
    let src = "G21 G90 G94 G17\nG10 L2 P1 X0 Y0 Z0\nG10 L2 P2 X10 Y0 Z0\nG54\nG0 X5.0 Y0 Z0\nG55\nG0 X5.0 Y0 Z0\nM2\n";
    let p = parse(src).expect("ok");
    // ops: [WcsDefine, WcsDefine, Rapid, Rapid, ProgramEnd]
    assert!(matches!(&p.ops[2], Op::Rapid { start: StartPoint::Unknown, .. })); // (a): без якоря
    assert_eq!(known(&p.ops[3]), [-5.0, 0.0, 0.0]); // 5 + O1(0) − O2(10)
}

#[test]
fn unit_partial_end_at_start_inherits_zero() {
    // SPEC §2 v0.11: старт программы — отсутствующие оси = дефолт 0.
    let p = parse("G21 G90 G94 G17\nG0 X10.0\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Rapid { end, .. } => assert_pt(end, 10.0, 0.0, 0.0),
        _ => panic!("op0 не Rapid"),
    }
}

#[test]
fn unit_partial_end_after_unknown_breakpoint_is_runtime() {
    // (b) без in-text origins → reading Unknown; частичный end → RUNTIME.
    let src = "G21 G90 G94 G17\nG0 X5.0 Y0 Z0\nG55\nG0 X6.0\nM2\n";
    assert_eq!(err_code(parse(src)), ErrorCode::RuntimeState);
}

#[test]
fn unit_g80_naked_coords_invalid() {
    // Голые координаты после G80 без motion-кода → INVALID (COVERAGE §2.1).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17\nG0 X1 Y1 Z1\nG80\nX2.0 Y2.0 Z2.0\nM2\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn unit_modal_motion_continuation() {
    // Без G80 голые координаты наследуют модальный motion (grbl).
    let p = parse("G21 G90 G94 G17\nG92 X0 Y0 Z0\nG1 X5.0 F100.0\nX10.0\nM2\n").expect("ok");
    assert!(matches!(&p.ops[1], Op::Linear { .. }));
    match &p.ops[1] {
        Op::Linear { end, .. } => assert_pt(end, 10.0, 0.0, 0.0),
        _ => unreachable!(),
    }
}

#[test]
fn unit_t_word_and_modal_violation_invalid() {
    assert_eq!(err_code(parse("G21 G90 G17\nT1\nM2\n")), ErrorCode::InvalidInput);
    assert_eq!(err_code(parse("G21 G90 G17\nG0 G1 X5.0\nM2\n")), ErrorCode::InvalidInput);
}

#[test]
fn unit_dwell_and_stops() {
    let p = parse("G21 G90 G17\nG4 P1.5\nM1\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Dwell { seconds, .. } => assert!((seconds.value() - 1.5).abs() <= EPS),
        _ => panic!("op0 не Dwell"),
    }
    match &p.ops[1] {
        Op::ProgramStop { optional, .. } => assert!(optional),
        _ => panic!("op1 не ProgramStop"),
    }
    // G4 P0 → drop+notice; G4 P<0 → INVALID.
    let out = canonicalize_with_notices("G21 G90 G17\nG4 P0\nM2\n", "d.ngc").expect("ok");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(err_code(parse("G21 G90 G17\nG4 P-1.0\nM2\n")), ErrorCode::InvalidInput);
}

#[test]
fn unit_g53_machine_frame_and_limits() {
    // (c): machine-ход вычислим только с in-text origin (§5.3) → N1-класс.
    assert_eq!(
        err_code(parse("G21 G90 G94 G17\nG0 X1 Y1 Z1\nG53 G0 X0.0 Y0.0 Z-5.0\nM2\n")),
        ErrorCode::RuntimeState
    );
    // С in-text origin: machine-ход ок; (d) первый WCS-ход после — Known.
    let src = "G21 G90 G94 G17\nG10 L2 P1 X0 Y0 Z0\nG0 X1.0 Y0 Z0\nG53 G0 X0.0 Y0.0 Z-5.0\nG0 X1.0 Y0 Z0\nM2\n";
    let p = parse(src).expect("ok");
    // ops: [WcsDefine, Rapid(wcs), Rapid(machine), Rapid(wcs), ProgramEnd]
    match &p.ops[2] {
        Op::Rapid { state, end, .. } => {
            assert_eq!(state.frame, eid_core::ast::Frame::Machine);
            assert_pt(end, 0.0, 0.0, -5.0);
        }
        _ => panic!("op2 не machine Rapid"),
    }
    // (d): start = M_after − O_wcs = (0,0,−5) − (0,0,0).
    assert_eq!(known(&p.ops[3]), [0.0, 0.0, -5.0]);
}
