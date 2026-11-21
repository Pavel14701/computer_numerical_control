//! Интеграционные тесты канонизатора GRBL → EID/L1 (этап 2, Агент C).
//! Пилоты и негативы — corpus/tier_a/v0; допуски — EQUIVALENCE §4 (eps 0.001).
//!
//! ЧЕК-ЛИСТ покрытия GRBL-колонки COVERAGE (spec_cov_*; правило → тест):
//! LEXICAL: UnexpectedChar→errors(lexer), unclosed-paren→errors(lexer),
//!   word-without-number→errors(lexer), exponent→spec_cov_lex_exponent,
//!   word>32→errors(lexer), $$/$110=/$garbage→corpus_47+spec_cov_lex_dollar,
//!   N-word→lexer, %→markers, block-delete→lexer+markers.
//! MODAL-CONFLICT (→INVALID): motion→modal_violation_detected,
//!   plane/distance/feed/units/wcs/path→spec_cov_modal_conflicts; solo —
//!   spec_cov_modal_solo.
//! ARC §6: IJK G17/18/19×cw/ccw→spec_cov_arc_planes_ijk, helix±→pilot_46(+)/
//!   spec_cov_arc_planes_ijk(−), R±→pilot_11+spec_cov_arc_r_negative,
//!   R+pitch→spec_cov_arc_r_pitch, radius-miss→n5a, chord>2R→n5b,
//!   R-full-circle→n19, rotation-eq→позитивы всех дуг (проходят §6.3).
//! CYCLES §7 (GRBL: циклы N/A словаря): G81/G83+L→spec_cov_cycles_raw_syntax,
//!   G84 → spec_cov_cycles_raw_syntax (матрица N/A→SYNTAX; SEMANTICS —
//!   LinuxCNC-диспозиция, расхождение с буквой брифа — отчёт), G80-naked→
//!   unit_g80_naked_coords_invalid; разворот/L-лимит — D-зона.
//! FRAME/WCS §5.3: G10L2/L20→g10_l20_and_l2+pilot_36, L20-mid→n15,
//!   double-preset→n17, G92-mid→n2, G92-partial→spec_cov_frame_g92_partial,
//!   G92.1→spec_cov_frame_g92_1_raw, G53-conditions→#37+n1+n16,
//!   G53-without-motion→spec_cov_frame_g53_no_motion, wcs-solo→spec_cov_frame_wcs_solo.
//! STATE-EMIT: coolant-переходы→coolant_29(#29)+grbl_emit spec_cov_emit_coolant,
//!   spindle→grbl_emit spec_cov_emit_spindle, feed-переживает-G0→pilot_03,
//!   partial-end-default-0→unit_partial_end_at_start.
//! EMIT-LEGALITY (8 условий)→grbl_emit spec_cov_emit_legality_*; tool_change→#27.

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
fn negative_n12_13_program_end_structure() {
    // Дубль/не-последний — структурные → Err INVALID (SPEC §3).
    assert_eq!(err_code(parse(&corpus("n12_program_end_not_last"))), ErrorCode::InvalidInput);
    assert_eq!(err_code(parse(&corpus("n13_program_end_duplicate"))), ErrorCode::InvalidInput);
}

#[test]
fn spec_v016_auto_pend_missing_program_end() {
    // SPEC §3 v0.16 (18/32 Tier B): программа без M2/M30 ВАЛИДА — неявный
    // конец: ProgramEnd добавлен последним опом + notice W_MISSING_PROGRAM_END.
    let out = canonicalize_with_notices(
        &corpus("n14_program_end_missing/program_end_missing.grbl.ngc"),
        "n14.ngc",
    )
    .expect("n14: Ok (auto-PEND, не отказ)");
    // ops: RAPID, LINEAR, ProgramEnd(авто) — последний оп именно ProgramEnd.
    assert!(matches!(&out.program.ops[0], Op::Rapid { .. }));
    assert!(matches!(&out.program.ops[1], Op::Linear { .. }));
    assert!(matches!(&out.program.ops.last().expect("non-empty"), Op::ProgramEnd { .. }));
    assert_eq!(out.notices.len(), 1);
    assert_eq!(
        out.notices[0].kind,
        eid_grbl::canonicalize::NoticeKind::MissingProgramEnd
    );
    assert!(out.notices[0].message.contains("W_MISSING_PROGRAM_END"));
    // Негатив: дубль M2 по-прежнему Err (структура ≠ отсутствие конца).
    assert_eq!(err_code(parse(&corpus("n13_program_end_duplicate"))), ErrorCode::InvalidInput);
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
fn spec_v0106_quantized_end_with_center_full_circle_360() {
    // CORPUS #13 (SPEC §3/§6 v0.17, аудит #15 — зонд rs274 ARC_FEED
    // rotation=-1): end X0.00004 квантуется к старту (§10.6), но
    // center ≠ start (I5 J0) → ПОЛНЫЙ КРУГ 360° НЕЗАВИСИМО от явных
    // end-слов; дуга сохранена, notice снят (CORPUS v0.6.7 golden regen).
    let out = canonicalize_with_notices(
        &corpus("13_arc_degenerate_zero_sweep/arc_degenerate_zero_sweep.grbl.ngc"),
        "13.ngc",
    )
    .expect("ok");
    assert_eq!(out.notices.len(), 0, "диспозиция drop снята: 360°-дуга жива");
    // G0, ARC-360, G1 X10, M2 → 4 опа (golden).
    assert_eq!(out.program.ops.len(), 4);
    match &out.program.ops[1] {
        Op::Arc { center, sweep_deg, pitch, dir, .. } => {
            assert_pt(center, 5.0, 0.0, 0.0);
            assert!((sweep_deg.value() - 360.0).abs() <= EPS, "sweep {}", sweep_deg.value());
            assert!(pitch.value().abs() <= EPS);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
        }
        _ => panic!("op1 не Arc"),
    }
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

// ===========================================================================
// SPEC §3 v0.17 (аудит #15): полный круг / вырожденный центр
// ===========================================================================

#[test]
fn audit15_full_circle_explicit_end_quantized_cw_ccw_and_helix() {
    // Зонд rs274: `G2 X10. Y0. I-5. J0` из (10,0) → полный оборот 360°
    // НЕЗАВИСИМО от явных end-слов (квантовались в старт); G3 — тоже 360°.
    let p = parse("G21 G90 G94 G17\nG0 X10.0 Y0.0 Z0.0\nG2 X10.0 Y0.0 I-5.0 J0.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, sweep_deg, pitch, dir, .. } => {
            assert_pt(center, 5.0, 0.0, 0.0);
            assert!((sweep_deg.value() - 360.0).abs() <= EPS, "cw sweep {}", sweep_deg.value());
            assert!(pitch.value().abs() <= EPS);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
        }
        _ => panic!("op1 не Arc"),
    }
    let p = parse("G21 G90 G94 G17\nG0 X10.0 Y0.0 Z0.0\nG3 X10.0 Y0.0 I-5.0 J0.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { sweep_deg, dir, .. } => {
            assert!((sweep_deg.value() - 360.0).abs() <= EPS, "ccw sweep {}", sweep_deg.value());
            assert_eq!(*dir, eid_core::ast::Dir::Ccw);
        }
        _ => panic!("op1 не Arc"),
    }
    // Частично-явный end: одно Y-слово, квантовавшееся в старт, — тоже 360°
    // (критерий геометрический: center ≠ start, а не «есть ли end-слова»).
    let p = parse("G21 G90 G94 G17\nG0 X10.0 Y0.0 Z0.0\nG2 Y0.0 I-5.0 J0.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { sweep_deg, .. } => {
            assert!((sweep_deg.value() - 360.0).abs() <= EPS, "partial-end sweep {}", sweep_deg.value());
        }
        _ => panic!("op1 не Arc"),
    }
    // Винтовой полный оборот с явными end-словами: 360° + pitch (Z2) —
    // не drop (pitch ≠ 0), ход сохранён.
    let p = parse("G21 G90 G94 G17\nG0 X10.0 Y0.0 Z0.0\nG2 X10.0 Y0.0 Z2.0 I-5.0 J0.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { sweep_deg, pitch, .. } => {
            assert!((sweep_deg.value() - 360.0).abs() <= EPS);
            assert!((pitch.value() - 2.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn audit15_degenerate_center_zero_radius_dropped_both_forms() {
    // Вырожденный центр (I0 J0 → center == start) при end == start (в
    // плоскости) и pitch == 0 → drop+notice ОБЕ формы end-слов (зонд
    // rs274: «Zero-radius arc» — отказ на обеих формах, у нас drop+notice).
    // Форма 1: явные end-слова, совпавшие со стартом.
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG0 X5.0 Y5.0 Z0.0\nG2 X5.0 Y5.0 I0.0 J0.0 F100.0\nM2\n",
        "dg1.ngc",
    )
    .expect("ok");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(out.notices[0].kind, eid_grbl::canonicalize::NoticeKind::ZeroLengthArc);
    assert!(out.notices[0].message.contains("zero-radius arc"), "{}", out.notices[0].message);
    // Форма 2: наследованный end (нет end-слов) — тоже drop (обе формы).
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG0 X5.0 Y5.0 Z0.0\nG2 I0.0 J0.0 F100.0\nM2\n",
        "dg2.ngc",
    )
    .expect("ok");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(out.notices[0].kind, eid_grbl::canonicalize::NoticeKind::ZeroLengthArc);
    // Вырожденный + pitch ≠ 0 → 360-винтовая r=0 — остаётся (D/fanuc).
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG0 X5.0 Y5.0 Z0.0\nG2 X5.0 Y5.0 Z2.0 I0.0 J0.0 F100.0\nM2\n",
        "dg3.ngc",
    )
    .expect("ok");
    assert_eq!(out.notices.len(), 0);
    match &out.program.ops[1] {
        Op::Arc { center, sweep_deg, pitch, .. } => {
            assert_pt(center, 5.0, 5.0, 0.0);
            assert!((sweep_deg.value() - 360.0).abs() <= EPS);
            assert!((pitch.value() - 2.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
    // R-форма при start == end — по-прежнему Err (центр неоднозначен, N19).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17\nG0 X5.0 Y5.0 Z0.0\nG2 X5.0 Y5.0 R5.0 F100.0\nM2\n")),
        ErrorCode::InvalidInput
    );
}

// ===========================================================================
// Аудит #33 (SPEC §8.6 правило 6, v0.17): модальная абсорбция на RAW-строках
// ===========================================================================

#[test]
fn audit33_g91_absorbed_on_raw_line_incremental_continues() {
    // Пробой: `G91 G28 Z0` — G28 ушёл в Raw, G91 терялся → следующий
    // `G0 X10 Y10` молча парсился АБСОЛЮТНЫМ. После фикса G91 абсорбирован:
    // end = стартовая точка + (10,10) инкрементально.
    let src = "G21 G90 G94 G17\nG0 X5.0 Y5.0 Z2.0\nG91 G28 Z0\nG0 X10.0 Y10.0\nM2\n";
    let out = canonicalize_with_notices(src, "a33.ngc").expect("parses Ok (G28 — line-level raw)");
    assert!(
        out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })),
        "строка G28 — RAW-оп"
    );
    assert!(
        out.notices.iter().any(|n| matches!(n.kind, eid_grbl::canonicalize::NoticeKind::ModalOnRaw)
            && n.message.contains("W_MODAL_ON_RAW")),
        "ожидался W_MODAL_ON_RAW-notice, получено: {:?}",
        out.notices.iter().map(|n| &n.message).collect::<Vec<_>>()
    );
    let ends: Vec<[f64; 3]> = out
        .program
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Rapid { end, .. } => Some(pt3(end)),
            _ => None,
        })
        .collect();
    assert_eq!(ends.len(), 2);
    for (i, want) in [5.0, 5.0, 2.0].into_iter().enumerate() {
        assert!((ends[0][i] - want).abs() <= EPS, "end0[{}] = {} vs {want}", i, ends[0][i]);
    }
    assert_eq!(
        ends[1],
        [15.0, 15.0, 2.0],
        "G91 абсорбирован: end = (5,5,2) + (10,10) инкрементально (было (10,10,0) — тихое искажение)"
    );
}

#[test]
fn audit33_units_absorbed_on_raw_line_g20_g43() {
    // `G20 G43 H2 X5.` — G43/H вне словаря GRBL → RAW, но G20 абсорбирован:
    // units = inch, следующая строка читается в inch (до фикса units терялись).
    let out = canonicalize_with_notices("G20 G43 H2 X5.\nG1 X1.0 F10.0\nM2\n", "a33u.ngc")
        .expect("parses Ok");
    assert_eq!(out.program.units, Units::In, "G20 абсорбирован с RAW-строки");
    assert!(out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
}

#[test]
fn audit33_feed_spindle_absorbed_on_raw_line() {
    // F/S/M3 на RAW-строке (`G43 H2 F240.0 M3 S6000`) абсорбируются:
    // следующий linear несёт feed=240 и шпиндель Cw(6000) без своих слов.
    let src = "G21 G90 G94 G17\nG43 H2 F240.0 M3 S6000\nG1 X10.0\nM2\n";
    let out = canonicalize_with_notices(src, "a33fs.ngc").expect("parses Ok");
    assert!(out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
    match out.program.ops.iter().find(|o| matches!(o, Op::Linear { .. })) {
        Some(Op::Linear { state, .. }) => {
            assert_eq!(state.feed.as_ref().map(|f| f.value()), Some(240.0), "F240 абсорбирован");
            assert!(
                matches!(&state.spindle, Spindle::Cw(n) if n.value() == 6000.0),
                "M3 S6000 абсорбирован: {state:?}"
            );
        }
        other => panic!("ожидался Linear: {other:?}"),
    }
}

#[test]
fn audit33_w_modal_on_raw_notice_on_raw_motion_g() {
    // Raw-motion-G (`G28 X5.0`) — notice W_MODAL_ON_RAW; модальный motion
    // (G1) НЕ продолжается: следующий голый X-кадр → INVALID (нет активного
    // motion-режима), не молчаливое продолжение G1.
    let src = "G21 G90 G94 G17\nG92 X0 Y0 Z0\nG1 X5.0 F100.0\nG28 X5.0\nX8.0\nM2\n";
    assert_eq!(err_code(canonicalize(src, "a33w.ngc")), ErrorCode::InvalidInput);
    // Без голого кадра — Ok + notice:
    let src2 = "G21 G90 G94 G17\nG92 X0 Y0 Z0\nG1 X5.0 F100.0\nG28 X5.0\nM2\n";
    let out2 = canonicalize_with_notices(src2, "a33w2.ngc").expect("Ok");
    assert!(
        out2.notices.iter().any(|n| matches!(n.kind, eid_grbl::canonicalize::NoticeKind::ModalOnRaw)),
        "{:?}",
        out2.notices.iter().map(|n| &n.message).collect::<Vec<_>>()
    );
    // G28 без осевых слов — notice НЕ эмитится (нет motion-неоднозначности).
    let src3 = "G21 G90 G94 G17\nG92 X0 Y0 Z0\nG28\nM2\n";
    let out3 = canonicalize_with_notices(src3, "a33w3.ngc").expect("Ok");
    assert!(
        out3.notices.iter().all(|n| !matches!(n.kind, eid_grbl::canonicalize::NoticeKind::ModalOnRaw)),
        "{:?}",
        out3.notices.iter().map(|n| &n.message).collect::<Vec<_>>()
    );
}

#[test]
fn audit33_g92_preset_absorbed_on_raw_line() {
    // G92 на RAW-строке абсорбируется: frame_preset задан, следующий ход
    // связывается с якоря пресетки (start known).
    let src = "G21 G90 G94 G17\nG92 X0.0 Y0.0 Z5.0 G28\nG0 X1.0 Y1.0 Z6.0\nM2\n";
    let out = canonicalize_with_notices(src, "a33g92.ngc").expect("parses Ok");
    let fp = out.program.frame_preset.as_ref().expect("G92 абсорбирован: пресетка задана");
    assert_pt(&fp.values, 0.0, 0.0, 5.0);
    assert_eq!(fp.wcs, Wcs::G54);
    assert!(out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
    match out
        .program
        .ops
        .iter()
        .find(|o| matches!(o, Op::Rapid { .. }))
        .expect("rapid")
    {
        Op::Rapid { start: StartPoint::Known(s), end, .. } => {
            assert_pt(s, 0.0, 0.0, 5.0);
            assert_pt(end, 1.0, 1.0, 6.0);
        }
        other => panic!("{other:?}"),
    }
    // Структурные инварианты G92 работают и на raw-кадре: второй пресет → Err.
    let dup = "G21 G90 G94 G17\nG92 X0.0 Y0.0 Z5.0 G28\nG92 X1.0 Y1.0 Z1.0 G28\nM2\n";
    assert_eq!(err_code(canonicalize(dup, "a33g92b.ngc")), ErrorCode::UnsupportedSemantics);
}

#[test]
fn audit33_g10_l2_absorbed_on_raw_line_origin_table_updated() {
    // G10 L2 на RAW-строке: состояние origin абсорбировано (оп НЕ строится —
    // кадр представлен Raw-опом), смена wcs использует абсорбированный
    // origin при связывании следующего хода.
    let src = "G21 G90 G94 G17\nG10 L2 P1 X0.0 Y0.0 Z0.0\nG0 X5.0 Y0.0 Z0.0\nG10 L2 P2 X10.0 Y0.0 Z0.0 G28\nG55\nG0 X5.0 Y0.0 Z0.0\nM2\n";
    let out = canonicalize_with_notices(src, "a33g10.ngc").expect("parses Ok");
    assert!(
        out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })),
        "строка G28 — RAW-оп"
    );
    assert!(
        !out.program.ops.iter().any(|o| matches!(o, Op::WcsDefine { p: 2, .. })),
        "raw-кадр не эмитит WcsDefine(p=2) (p=2 ушёл в состояние)"
    );
    // Легальная строка P1 — по-прежнему WcsDefine-оп.
    assert!(
        out.program.ops.iter().any(|o| matches!(o, Op::WcsDefine { p: 1, .. })),
        "легальная G10 L2 P1 строка — WcsDefine-оп"
    );
    // Связывание: start = prev_end(5) + O_P1(0) − O_P2(10) = −5 —
    // origin P2 ДОСТУПЕН (до фикса G10 с raw-строки терялся → разрыв (b)).
    match out
        .program
        .ops
        .iter()
        .find(|o| matches!(o, Op::Rapid { start: StartPoint::Known(_), .. }))
        .expect("Known-rapid")
    {
        Op::Rapid { start: StartPoint::Known(s), .. } => assert_pt(s, -5.0, 0.0, 0.0),
        other => panic!("{other:?}"),
    }
}

// ===========================================================================
// (перенесено) CORPUS #37
// ===========================================================================

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
// ПОКРЫТИЕ spec_cov_* — GRBL-колонка COVERAGE (см. чек-лист в шапке)
// ---------------------------------------------------------------------------

#[test]
fn spec_cov_lex_exponent_is_bad_number() {
    // Экспонента: read_number читает "1", 'e' → буква E (вне словаря слов) →
    // whole-line Raw{SYNTAX} (E не в A/B/C/U/V/W — правило unsupported_words).
    let out = canonicalize_with_notices("G21\nG0 X1e5\nM2\n", "e.ngc").expect("Ok + Raw");
    assert!(matches!(&out.program.ops[0], Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }));
}

#[test]
fn spec_cov_lex_stage_parse_vs_canonical_verify() {
    // interface v0.1.2: lexical-диагностика → Stage::Parse;
    // канонизационный инвариант (unit-switch §2.5) → Stage::Verify.
    use eid_core::diag::Stage;
    let d = parse("G21 G90 G94 G17\nG0 X1 Y1 Z1\nG1 &\nM2\n").unwrap_err();
    assert_eq!(d[0].code, ErrorCode::InvalidInput);
    assert_eq!(d[0].stage, Stage::Parse, "lexical должен быть Parse");
    let d = parse(&corpus("n7_unit_switch_mid/unit_switch_mid.grbl.ngc")).unwrap_err();
    assert_eq!(d[0].code, ErrorCode::UnsupportedSemantics);
    assert_eq!(d[0].stage, Stage::Verify, "канон-инвариант должен быть Verify");
}

#[test]
fn spec_cov_frame_l20_then_g92_double_preset() {
    // Смешанные пресетки: G10 L20 → G92 — второй пресет → Err SEMANTICS
    // (count-гейт ветки G10 L20; CORPUS n17-класс).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17
G10 L20 P1 X1.0 Y2.0 Z3.0
G92 X0.0 Y0.0 Z5.0
G0 X1 Y1 Z1
M2
")),
        ErrorCode::UnsupportedSemantics
    );
}

#[test]
fn spec_cov_lex_dollar_variants() {
    // Любая $-строка — конфигурация станка → Raw{SYNTAX}: $$, $110=, $-мусор.
    for src in ["$$", "$garbage", "$-5=1"] {
        let out = canonicalize_with_notices(&format!("{src}\nG21 G90 G94 G17\nG0 X1 Y1 Z1\nM2\n"), "d.ngc")
            .expect("Ok + Raw");
        assert!(
            matches!(&out.program.ops[0], Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }),
            "{src}: ops[0] не Raw{{SYNTAX}}"
        );
    }
}

#[test]
fn spec_cov_modal_conflicts() {
    // Конфликт двух кодов одной группы в кадре → Err INVALID (grbl group violation).
    // motion покрыт modal_violation_detected; здесь остальные 6 групп.
    let cases = [
        "G17 G18",            // plane
        "G90 G91",            // distance
        "G93 G94",            // feed
        "G20 G21",            // units (G20 устанавливает, G21 в том же кадре — смена)
        "G54 G55",            // wcs
        "G61 G61",            // path
    ];
    for c in cases {
        assert_eq!(
            parse(&format!("G0 X1 Y1 Z1\n{c}\nM2\n")).unwrap_err()[0].code,
            ErrorCode::InvalidInput,
            "конфликт `{c}` не отловлен"
        );
    }
}

#[test]
fn spec_cov_modal_solo() {
    // Каждый код каждой группы легален solo (семь групп). G20/G21-соло-набор
    // ограничен одной системой единиц (SPEC §2.5) — смена проверена отдельно.
    let src = "G21 G90 G94 G17 G54 G61\nG0 X1 Y1 Z1\nG55\nG0 X2 Y1 Z1\nG91\nG93\nG1 X1.0 Y1.0 Z1.0 F2.0\nG94\nG56\nG18\nG19\nM2\n";
    let p = parse(src).expect("solo-коды легальны");
    assert!(!p.ops.is_empty());
}

#[test]
fn spec_cov_arc_planes_ijk() {
    // IJK: 3 плоскости × cw/ccw × винтовая ±. Центр = старт + пара осей
    // (I/J/K literal X/Y/Z); направление и 90°-сектор согласованы по геометрии.
    // G17 cw (+винт): из (0,0,0) в (−5,5,2), центр (0,5,0).
    let p = parse("G21 G90 G94 G17\nG0 X0 Y0 Z0\nG2 X-5.0 Y5.0 Z2.0 I0.0 J5.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, pitch, dir, sweep_deg, normal, .. } => {
            assert_pt(center, 0.0, 5.0, 0.0);
            assert!((pitch.value() - 2.0).abs() <= EPS);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
            assert_pt(normal, 0.0, 0.0, 1.0);
        }
        _ => panic!("op1 не Arc"),
    }
    // G17 ccw (−винт): центр (5,0,0), e=(5,−5,−2).
    let p = parse("G21 G90 G94 G17\nG0 X0 Y0 Z0\nG3 X5.0 Y-5.0 Z-2.0 I5.0 J0.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, pitch, dir, sweep_deg, normal, .. } => {
            assert_pt(center, 5.0, 0.0, 0.0);
            assert!((pitch.value() + 2.0).abs() <= EPS);
            assert_eq!(*dir, eid_core::ast::Dir::Ccw);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
            assert_pt(normal, 0.0, 0.0, 1.0);
        }
        _ => panic!("op1 не Arc"),
    }
    // G18 cw: пара I(X)+K(Z), нормаль +Y, linear=Y (pitch 2); 90° CW сектор.
    let p = parse("G21 G90 G94 G18\nG0 X0 Y0 Z0\nG2 X5.0 Y2.0 Z-5.0 I5.0 K0.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, normal, sweep_deg, pitch, .. } => {
            assert_pt(center, 5.0, 0.0, 0.0);
            assert_pt(normal, 0.0, 1.0, 0.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS, "sweep {}", sweep_deg.value());
            assert!((pitch.value() - 2.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
    // G19 ccw: пара J(Y)+K(Z), нормаль +X, linear=X; 90° CCW сектор.
    let p = parse("G21 G90 G94 G19\nG0 X0 Y0 Z0\nG3 X2.0 Y5.0 Z-5.0 J5.0 K0.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, normal, sweep_deg, .. } => {
            assert_pt(center, 0.0, 5.0, 0.0);
            assert_pt(normal, 1.0, 0.0, 0.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn spec_cov_arc_r_negative_and_pitch() {
    // R<0 → большая дуга (>180°), центр на другой стороне хорды.
    let p = parse("G21 G90 G94 G17\nG0 X0 Y10 Z0\nG2 X10.0 Y0.0 R-10.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, sweep_deg, .. } => {
            assert_pt(center, 10.0, 10.0, 0.0);
            assert!((sweep_deg.value() - 270.0).abs() <= EPS, "sweep {}", sweep_deg.value());
        }
        _ => panic!("op1 не Arc"),
    }
    // R + pitch: винтовая R-форма (SPEC §6.1) — Z-слово вне плоскости = pitch.
    let p = parse("G21 G90 G94 G17\nG0 X0 Y0 Z0\nG2 X8.0 Y0.0 Z2.0 R5.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, pitch, sweep_deg, .. } => {
            assert_pt(center, 4.0, -3.0, 0.0);
            assert!((pitch.value() - 2.0).abs() <= EPS);
            assert!((sweep_deg.value() - 106.26).abs() <= 0.01, "sweep {}", sweep_deg.value());
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn spec_cov_cycles_raw_classes() {
    // COVERAGE §2.5 + рулинг оркестратора: G81/G83 — N/A словаря GRBL как
    // источника → Raw{SYNTAX}; G74/G84/G87/G88 — refuse-класс конструкции-
    // источника (LinuxCNC-словарь валиден, «не представимо ходами») →
    // Raw{SEMANTICS}. Разворот/L-лимит — D-зона.
    let cases = [
        ("G81 X1.0 Y2.0 Z-3.0 R1.0", ErrorCode::UnsupportedSyntax),
        ("G83 X1.0 Y2.0 Z-5.0 Q1.0 L3", ErrorCode::UnsupportedSyntax),
        ("G84 X1.0 Y2.0 Z-5.0", ErrorCode::UnsupportedSemantics),
        ("G74 X1.0 Y2.0 Z-5.0", ErrorCode::UnsupportedSemantics),
        ("G87 X1.0 Y2.0 Z-5.0", ErrorCode::UnsupportedSemantics),
        ("G88 X1.0 Y2.0 Z-5.0", ErrorCode::UnsupportedSemantics),
        ("G76 X1.0 Y2.0 Z-5.0", ErrorCode::UnsupportedSemantics), // токарный цикл резьбы (§1.2/§2.5)
    ];
    for (g, code) in cases {
        let out = canonicalize_with_notices(&format!("G21 G90 G94 G17\n{g}\nF100.0\nM2\n"), "c.ngc")
            .expect("Ok + Raw");
        assert!(
            matches!(&out.program.ops[0], Op::Raw { code: expected, .. } if *expected == code),
            "{g}: ожидался {code:?}"
        );
    }
}

#[test]
fn spec_cov_frame_g92_partial_and_g92_1() {
    // Частичный G92 → RUNTIME (незаданные оси — неизвестное чтение, §5.3).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17\nG92 X0.0 Y0.0\nG0 X1 Y1 Z1\nM2\n")),
        ErrorCode::RuntimeState
    );
    // G92.1 → Raw{SEMANTICS} (COVERAGE §2.3, Q4).
    let out = canonicalize_with_notices("G21 G90 G94 G17\nG92.1\nG0 X1 Y1 Z1\nM2\n", "g.ngc").expect("Ok");
    assert!(matches!(&out.program.ops[0], Op::Raw { code: ErrorCode::UnsupportedSemantics, .. }));
}

#[test]
fn spec_cov_frame_g53_no_motion() {
    // G53 без G0/G1 в кадре → INVALID (grbl gcode.c).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17\nG53\nG0 X1 Y1 Z1\nM2\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn spec_cov_frame_wcs_solo() {
    // G54..G59 соло — легальный выбор, state.wcs переключается по опам.
    let src = "G21 G90 G94 G17\nG54\nG0 X1 Y1 Z1\nG55\nG0 X2 Y1 Z1\nG56\nG0 X3 Y1 Z1\nG57\nG0 X4 Y1 Z1\nG58\nG0 X5 Y1 Z1\nG59\nG0 X6 Y1 Z1\nM2\n";
    let p = parse(src).expect("ok");
    let wcs_of = |i: usize| match &p.ops[i] {
        Op::Rapid { state, .. } => state.wcs,
        _ => unreachable!(),
    };
    let expected = [Wcs::G54, Wcs::G55, Wcs::G56, Wcs::G57, Wcs::G58, Wcs::G59];
    for (i, w) in expected.iter().enumerate() {
        assert_eq!(wcs_of(i), *w, "оп {i}");
    }
}

#[test]
fn spec_cov_arc_nonzero_start_exact() {
    // Ненулевой старт на любых осях: арифметика центров/ρ/pitch не вырождается
    // (± -мутанты arc_block ловятся точными ассертами).
    // G17 cw 120°: s=(3,7,1), c=(3,2,1), e = c + Rot_cw120(0,5) = (7.33,−0.5,1).
    let p = parse("G21 G90 G94 G17\nG0 X3.0 Y7.0 Z1.0\nG2 X7.3300 Y-0.5000 I0.0 J-5.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, sweep_deg, pitch, .. } => {
            assert_pt(center, 3.0, 2.0, 1.0);
            assert!((sweep_deg.value() - 120.0).abs() <= 0.01, "sweep {}", sweep_deg.value());
            assert!(pitch.value().abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn spec_cov_arc_r_nonzero_start() {
    // R-форма с ненулевым d0/d1 и pitch: p̂/sgn/h арифметика не вырождается.
    // s=(3,7,1), e=(−1,10,4), R5 cw: центр (3.598, 11.964), sweep 60°, pitch 3.
    let p = parse("G21 G90 G94 G17\nG0 X3.0 Y7.0 Z1.0\nG2 X-1.0 Y10.0 Z4.0 R5.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, sweep_deg, pitch, .. } => {
            assert_pt(center, 3.598, 11.964, 1.0);
            assert!((sweep_deg.value() - 60.0).abs() <= 0.01, "sweep {}", sweep_deg.value());
            assert!((pitch.value() - 3.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn spec_cov_arc_g93_arc_len_and_linear_diagonal() {
    // G93: arc_len = ρ·sweep (дуга) и len = √(x²+y²+z²) (диагональ) —
    // формулы длины ловятся точными значениями feed.
    let p = parse("G21 G90 G93 G17\nG92 X0 Y0 Z0\nG2 X-5.0 Y5.0 Z0.0 I0.0 J5.0 F2.0\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Arc { state, .. } => {
            let f = state.feed.as_ref().expect("feed").value();
            // quarter circle ρ=5: len = 5·π/2 ≈ 7.854; feed = 2·len ≈ 15.708.
            assert!((f - 15.708).abs() <= 0.01, "G93 arc feed {f}");
        }
        _ => panic!("op0 не Arc"),
    }
    let p = parse("G21 G90 G93 G17\nG92 X0 Y0 Z0\nG1 X10.0 Y20.0 F2.0\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Linear { state, .. } => {
            let f = state.feed.as_ref().expect("feed").value();
            assert!((f - 2.0 * 500.0f64.sqrt()).abs() <= 0.01, "G93 diagonal feed {f}");
        }
        _ => panic!("op0 не Linear"),
    }
}

#[test]
fn spec_cov_frame_g28_g38_raw() {
    // COVERAGE §2.1/§2.3: G28 → Raw{MACHINE_TABLE}; G38.2 → Raw{SEMANTICS}.
    let out = canonicalize_with_notices("G21 G90 G94 G17\nG28\nG0 X1 Y1 Z1\nM2\n", "a.ngc").expect("Ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw { code: ErrorCode::RequiresMachineTable, .. }
    ));
    let out = canonicalize_with_notices("G21 G90 G94 G17\nG38.2 X1.0\nF50.0\nM2\n", "b.ngc").expect("Ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw { code: ErrorCode::UnsupportedSemantics, .. }
    ));
}

#[test]
fn spec_cov_frame_g10_p_variants() {
    // P вне 1..=6 / нецелое → Err INVALID (guard process_g10);
    // частичный origin L2 → Raw{MACHINE_TABLE} (таблица хранит старое).
    for p_word in ["P0", "P7", "P2.5", "P-1"] {
        assert_eq!(
            err_code(parse(&format!(
                "G21 G90 G94 G17\nG10 L2 {p_word} X1.0 Y2.0 Z3.0\nG0 X1 Y1 Z1\nM2\n"
            ))),
            ErrorCode::InvalidInput,
            "{p_word}"
        );
    }
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG10 L2 P1 X1.0 Y2.0\nG0 X1 Y1 Z1\nM2\n",
        "p.ngc",
    )
    .expect("Ok + Raw");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw { code: ErrorCode::RequiresMachineTable, .. }
    ));
}

#[test]
fn spec_cov_frame_g91_at_start_and_breakpoint() {
    // G91 на старте: база = дефолт 0 (SPEC §2 v0.11); G91 в точке разрыва (b)
    // → RUNTIME (инкремент от неизвестного чтения).
    let p = parse("G21 G91 G17\nG1 X1.0 Y2.0 F100.0\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Linear { end, .. } => assert_pt(end, 1.0, 2.0, 0.0),
        _ => panic!("op0 не Linear"),
    }
    let src = "G21 G90 G17\nG0 X5.0 Y0 Z0\nG55\nG91\nG1 X1.0 Y1.0 Z1.0\nM2\n";
    assert_eq!(err_code(parse(src)), ErrorCode::RuntimeState);
}

#[test]
fn spec_cov_frame_dist_feed_resets() {
    // DistAbs/FeedPerMin соло-сброс мод: G91→G90 абсолют; G93→G94 — F как есть.
    let src = "G21 G90 G94 G17\nG92 X10 Y10 Z10\nG91\nG1 X1.0 Y0 Z0 F100.0\nG90\nG94\nG1 X20.0 Y10.0 Z10.0 F200.0\nM2\n";
    let p = parse(src).expect("ok");
    match &p.ops[0] {
        Op::Linear { end, .. } => assert_pt(end, 11.0, 10.0, 10.0),
        _ => panic!("op0 не Linear"),
    }
    match &p.ops[1] {
        Op::Linear { state, end, .. } => {
            // G90: абсолют X20 (не 21 инкрементально); G94: F как есть (не len×F).
            assert_pt(end, 20.0, 10.0, 10.0);
            assert_eq!(state.feed.as_ref().map(|n| n.value()), Some(200.0));
        }
        _ => panic!("op1 не Linear"),
    }
}

#[test]
fn spec_cov_coolant_ops_state() {
    // MistOn/CoolantOff соло-руки: M7 → mist в state; M9 → none.
    let p = parse("G21 G90 G94 G17\nM7\nG0 X1 Y1 Z1\nM9\nG0 X2 Y1 Z1\nM2\n").expect("ok");
    let cool_of = |i: usize| match &p.ops[i] {
        Op::Rapid { state, .. } => (state.coolant.flood, state.coolant.mist),
        _ => unreachable!(),
    };
    assert_eq!(cool_of(0), (false, true));
    assert_eq!(cool_of(1), (false, false));
}

#[test]
fn spec_cov_limit_max_source_lines() {
    // COVERAGE §1.2: ровно 1_000_000 строк — Ok; 1_000_001 — Err LIMIT.
    let ok_src = format!("{}M2\n", "G21\n".repeat(999_999));
    assert_eq!(ok_src.lines().count(), 1_000_000);
    parse(&ok_src).expect("1M строк легальны");
    let over = format!("{}M2\n", "G21\n".repeat(1_000_000));
    let d = parse(&over).unwrap_err();
    assert_eq!(d[0].code, ErrorCode::LimitExceeded);
    assert_eq!(d[0].limit.as_ref().map(|l| l.threshold), Some("max_source_lines"));
}

#[test]
fn spec_cov_limit_max_ops_boundary() {
    // COVERAGE §1.2: сумма ops+notices; граница 200_000: снизу Ok, +1 → Err.
    // Ходы различны (нулевые rapid'ы давали бы лишние notices); финальный
    // zero-length linear даёт notice без опа — проверяет ±-мутант суммы.
    let base = "G21 G90 G94 G17\nG92 X0 Y0 Z0\n";
    let ok_moves: String = (0..199_998).map(|i| format!("G0 X{i} Y0 Z0\n")).collect();
    let ok_src = format!("{base}{ok_moves}G1 X199997 Y0 Z0 F100.0\nM2\n");
    // ops: 199_998 rapid + 1 ProgramEnd = 199_999; notices: 1 → сумма 200_000.
    parse(&ok_src).expect("граница снизу легальна");
    let over_moves: String = (0..199_999).map(|i| format!("G0 X{i} Y0 Z0\n")).collect();
    let over_src = format!("{base}{over_moves}G1 X199998 Y0 Z0 F100.0\nM2\n");
    let d = parse(&over_src).unwrap_err();
    assert_eq!(d[0].code, ErrorCode::LimitExceeded);
    assert_eq!(d[0].limit.as_ref().map(|l| l.threshold), Some("max_ops_per_program"));
}

#[test]
fn spec_cov_arc_near_degenerate_r_notice() {
    // EQUIVALENCE §4: |cos(sweep/2)| < 1e-3 (±0.12° около 180°) → notice
    // NearDegenerateRArc, дуга ОСТАЁТСЯ (не отказ, не drop).
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG92 X0 Y10 Z0\nG2 X0.0175 Y-9.99999 R10.0 F100.0\nM2\n",
        "nd.ngc",
    )
    .expect("ok");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(
        out.notices[0].kind,
        eid_grbl::canonicalize::NoticeKind::NearDegenerateRArc
    );
    assert!(matches!(&out.program.ops[0], Op::Arc { .. }));
}

#[test]
fn spec_cov_lex_whitespace_and_word_boundary() {
    // Ведущие/множественные/хвостовые пробелы и табы; граница max_word_chars
    // (31 цифра Ok / 32 WordTooLong); col в ошибке (мутанты col-арифметики).
    let toks = eid_grbl::lexer::lex_line("  G1  X5\tY6  ", 1).expect("ok");
    assert_eq!(toks.len(), 3);
    let toks = eid_grbl::lexer::lex_line("G 1 X 5", 2).expect("ok");
    assert_eq!(toks.len(), 2); // G1 + X5
    let ok = eid_grbl::lexer::lex_line(&format!("X{}", "1".repeat(31)), 3).expect("31 цифра легальна");
    assert_eq!(ok.len(), 1);
    let e = eid_grbl::lexer::lex_line(&format!("X{}", "1".repeat(32)), 4).unwrap_err();
    assert_eq!(e.kind, eid_grbl::lexer::LexErrorKind::WordTooLong);
    assert_eq!(e.line, 4);
    let e = eid_grbl::lexer::lex_line("G1 &", 5).unwrap_err();
    assert_eq!(e.col, 4, "col ошибки: позиция &");
}

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
