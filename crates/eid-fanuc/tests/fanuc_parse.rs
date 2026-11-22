//! Интеграционные тесты канонизатора Fanuc → EID/L1 (этап 1, Агент C).
//! Golden-пары (fanuc-текст → EID AST) по образцу eid-grbl tests/grbl_parse.rs;
//! допуски — EQUIVALENCE §4 (eps 0.001).
//!
//! ЧЕК-ЛИСТ этапа 1 (правило → тест):
//! LEXICAL: EOB-разделение → eob_full_line_program, lexer unit-тесты
//!   (12, src/lexer.rs); вложенный/незакрытый комментарий → lexer; % →
//!   percent_ignored; N-слова → n_word_line; трейлинг-точка → dpi_point…
//!   + lexer; целая часть >9 цифр → integer_overflow_limit.
//! MODAL (01–06): violation → modal_violation_invalid; G90/G91 →
//!   g91_incremental; плоскости → arc_g18/arc_g19; G90.1/G91.1 →
//!   arc_g90_1_absolute_center; G94/G95 → g95_raw_semantics; units G20/G21 +
//!   G70/G71 → units_alias_g70_g71, dpi_bare_integer_inch; смена units →
//!   unit_switch_mid_invalid.
//! MOTION: rapid/linear → eob_full_line_program; модальное продолжение →
//!   modal_continuation; голые координаты без motion → naked_coords_invalid;
//!   zero-length → zero_length_linear_drop.
//! ARC §6: IJK 3 плоскости × cw/ccw → arc_ijk_cw_helix / arc_ijk_ccw /
//!   arc_g18 / arc_g19; R± → arc_r_positive / arc_r_negative; R+pitch →
//!   arc_r_helix; helical → arc_ijk_cw_helix; полная окружность →
//!   full_circle / helical_full_turn; R-полный-круг → r_full_circle_invalid;
//!   радиус-мисс → radius_mismatch_invalid; хорда>2R → chord_gt_2r_invalid;
//!   перпендикулярная компонента → plane_perpendicular_invalid.
//! DPI (§1.2, W_DPI_ASSUMED): dpi_bare_integer_mm / _inch / dpi_point_not…
//!   / dpi_all_bare_arc / dpi_pending_f_flushes_to_next_op.
//! STRUCTURE: auto-PEND → auto_pend_missing_program_end; O-code → o_code_raw;
//!   вне словаря этапа 1 → unknown_g_raw_provisional; block delete →
//!   block_delete_raw_runtime; этап диагностики → stage_parse_vs_verify.
//!
//! ЧЕК-ЛИСТ этапа 2 (правило → тест):
//! FRAMES (2a): G54–G59 → wcs_select_g54_g59; точка разрыва (b) без origin →
//!   wcs_switch_unknown_without_origins; с origin (формула разности) →
//!   wcs_switch_with_origins; G10 L2 P1..6 abs/inc → g10_l2_wcs_define /
//!   g10_l2_incremental_record / partial-оси наследуют старый origin →
//!   g10_l2_partial_inherit; P0 → g10_l2_p0_raw_requires; L20 →
//!   g10_l20_raw_not_set_here (ловушка №3); L1 → g10_l1_table_raw;
//!   G11 → g11_cancel_ignored / g11_family_raw_inherit; G92 →
//!   g92_frame_preset_anchor; mid-program → g92_mid_program_invalid;
//!   G92 S → g92_s_clamp_raw; G52 → g52_hard_err (единственный ERR);
//!   G53 machine-frame → g53_machine_move / g53_first_move_unknown /
//!   g53_partial_end_invalid / g53_after_preset_invalid /
//!   g53_requires_explicit_motion / g53_arc_invalid; точка разрыва (d) →
//!   (в g53_machine_move); G54.1 Pn → g54_1_pn_raw; G92.1 → g92_1_reset_raw.
//! CYCLES (2b): §7.7 шаблон ×7 → cycle_g81_unroll_template /
//!   cycle_g82_dwell_p_ms / cycle_g83_peck_full_retract /
//!   cycle_g73_chip_break / cycle_g85_bore_feed_out /
//!   cycle_g86_spindle_restart / cycle_g89_bore_dwell; Raw ×5 →
//!   cycle_raw_family; K/L → cycle_k_repeat_g91_step; K0-in-G91 →
//!   cycle_k0_g91_raw; §7.6 → cycle_first_motion_invalid; G98 →
//!   cycle_g98_retract_initial; modal repeat → cycle_modal_repeat_xy;
//!   cancel → cycle_g0_cancels / cycle_g80_ignored; без R →
//!   cycle_missing_r_invalid; лимит → cycle_unroll_limit.
//! M-CODES (2c): M0/M1 → m0_m1_program_stop; M3/M4/M5+S →
//!   m3_m4_m5_spindle_state (голое S persist → m_bare_s_persists);
//!   M6+T → m6_tool_change; M6 без T → m6_without_t_invalid; одиночное T →
//!   t_alone_preload_raw; M7/M8/M9 → m7_m8_m9_coolant; M19/M29 →
//!   m19_m29_raw; M98/M99/M198 → m98_m99_m198_raw; generic →
//!   unknown_m_generic_raw.
//! MACRO B (2d): `#`/IF/GOTO/WHILE/DO/END → macro_b_hash_raw /
//!   macro_b_statements_raw (соседние статические ходы парсятся);
//!   G65/G66/G67 → macro_b_g65_family_raw; D9 round-trip (ruling 5) →
//!   d9_round_trip_raw_fragments / d9_round_trip_mixed_line.
//! COMPENSATIONS (2e): G43/G44 → g43_g44_raw_requires; G41/G42 →
//!   g41_g42_raw_semantics; семейства → g40_cancel_family_rule /
//!   g49_cancel_family_rule / h00_cancel_ignored.
//! G93 (2f): linear → g93_linear_feed_len_f; arc → g93_arc_feed_len_f.
//! DWELL G4: P мс → g4_p_ms_conversion; X/U сек → g4_x_u_seconds;
//!   U-исключение → g4_u_exception_elsewhere_raw; P0 → g4_p0_drop_notice.
//! PATH META: G61 = exact_stop (value shift) → g61_exact_stop_value_shift;
//!   G64 bare → g64_blend_bare; переключение → path_switch_mid_invalid.

use eid_core::ast::{Op, Point3, Program, StartPoint, Units};
use eid_core::diag::Diagnostic;
use eid_core::diag::{ErrorCode, Stage};
use eid_fanuc::canonicalize::{canonicalize, canonicalize_with_notices, NoticeKind};
use eid_fanuc::FanucAdapter;

const EPS: f64 = 0.001;

fn parse(src: &str) -> Result<Program, Vec<Diagnostic>> {
    canonicalize(src, "test.ngc")
}

fn err_code(r: Result<Program, Vec<Diagnostic>>) -> ErrorCode {
    let Err(d) = r else {
        panic!("ожидался Err")
    };
    d[0].code
}

fn err_stage(r: Result<Program, Vec<Diagnostic>>) -> Stage {
    let Err(d) = r else {
        panic!("ожидался Err")
    };
    d[0].stage
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
        Op::Rapid {
            start: StartPoint::Known(s),
            ..
        }
        | Op::Linear {
            start: StartPoint::Known(s),
            ..
        }
        | Op::Arc {
            start: StartPoint::Known(s),
            ..
        } => pt3(s),
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

fn notices_of(src: &str) -> (Program, Vec<eid_fanuc::canonicalize::Notice>) {
    let out = canonicalize_with_notices(src, "test.ngc").expect("ok");
    (out.program, out.notices)
}

// ---------------------------------------------------------------------------
// Golden-пары: программа → AST
// ---------------------------------------------------------------------------

#[test]
fn eob_full_line_program() {
    // Вся программа на ОДНОЙ физической строке: `;` делит блоки (§1.3).
    // GRBL-лексер с `;`-комментарием съел бы хвосты — здесь 4 блока.
    let p = parse("O0030; G21 G90 G94 G17; G0 X0. Y0. Z5.; G1 X10. F150.; M30;").expect("ok");
    assert_eq!(p.ops.len(), 4); // Raw(O) + Rapid + Linear + ProgramEnd
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    assert!(matches!(
        &p.ops[1],
        Op::Rapid {
            start: StartPoint::Unknown,
            ..
        }
    ));
    match &p.ops[1] {
        Op::Rapid { end, state, .. } => {
            assert_pt(end, 0.0, 0.0, 5.0);
            assert_eq!(state.wcs, eid_core::ast::Wcs::G54);
        }
        _ => unreachable!(),
    }
    // Связывание: start linear = end rapid'а (SPEC §2, обязанность канонизатора).
    assert_eq!(known(&p.ops[2]), [0.0, 0.0, 5.0]);
    match &p.ops[2] {
        Op::Linear { end, state, .. } => {
            assert_pt(end, 10.0, 0.0, 5.0);
            assert_eq!(feed_of(&p.ops[2]), Some(150.0));
            assert_eq!(state.frame, eid_core::ast::Frame::Wcs);
        }
        _ => unreachable!(),
    }
    assert!(matches!(&p.ops[3], Op::ProgramEnd { .. }));
    assert_eq!(p.units, Units::Mm);
    assert_eq!(p.path_mode, eid_core::ast::PathMode::ExactPath); // дефолт §3 v0.12
}

#[test]
fn multiline_traditional_program() {
    // Классический формат: блок на строку, завершённый `;`.
    // Этап 2: M5 → ABSORB (state.spindle = Off, дефолт) — Raw нет.
    let p =
        parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG1 X-10. Y10. F250.;\nM5;\nM30;\n").expect("ok");
    assert_eq!(p.ops.len(), 3); // Rapid, Linear, ProgramEnd
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    assert!(matches!(&p.ops[1], Op::Linear { .. }));
    assert!(matches!(&p.ops[2], Op::ProgramEnd { .. }));
    match &p.ops[1] {
        Op::Linear { end, state, .. } => {
            assert_pt(end, -10.0, 10.0, 5.0);
            assert_eq!(state.spindle, eid_core::ast::Spindle::Off);
        }
        _ => unreachable!(),
    }
}

#[test]
fn auto_pend_missing_program_end() {
    // SPEC §3 v0.16 / анализ §4.4: DNC-файл без M30 — неявный конец.
    let (p, notices) = notices_of("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG1 X10. F150.;\n");
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    assert!(matches!(&p.ops[1], Op::Linear { .. }));
    assert!(matches!(
        p.ops.last().expect("non-empty"),
        Op::ProgramEnd { .. }
    ));
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, NoticeKind::MissingProgramEnd);
    assert!(notices[0].message.contains("W_MISSING_PROGRAM_END"));
}

// ---------------------------------------------------------------------------
// DPI (анализ §1.2, ловушка №1): bare-integer → × least increment + notice
// ---------------------------------------------------------------------------

#[test]
fn dpi_bare_integer_mm() {
    // X25 в G21 = 0.025 mm (НЕ 25!) — DPI=0, IS-B (§1.2); молчаливое чтение
    // как целое запрещено (тихая ошибка ×1000).
    let (p, notices) = notices_of("G21 G90 G94 G17;\nG0 X25 Y0 Z0;\nM30;\n");
    match &p.ops[0] {
        Op::Rapid { end, .. } => assert_pt(end, 0.025, 0.0, 0.0),
        _ => panic!("op0 не Rapid"),
    }
    // Y0/Z0 тоже bare — uniform-политика: notice на каждое bare-слово.
    let dpi: Vec<_> = notices
        .iter()
        .filter(|n| n.kind == NoticeKind::DpiAssumed)
        .collect();
    assert!(dpi.len() >= 3, "X25/Y0/Z0 — три bare-слова");
    assert!(dpi.iter().any(|n| n.message.contains("W_DPI_ASSUMED")
        && n.message.contains("X25")
        && n.message.contains("0.025")));
}

#[test]
fn dpi_bare_integer_inch() {
    // IS-B inch: 25 × 0.0001" = 0.0025 (§1.2 least increment по units).
    let (p, notices) = notices_of("G20 G90 G94 G17;\nG0 X25 Y0 Z0;\nM30;\n");
    assert_eq!(p.units, Units::In);
    match &p.ops[0] {
        Op::Rapid { end, .. } => assert_pt(end, 0.0025, 0.0, 0.0),
        _ => panic!("op0 не Rapid"),
    }
    assert!(notices.iter().any(|n| n.message.contains("IS-B inch")));
}

#[test]
fn dpi_point_not_flagged() {
    // С точкой — verbatim (SPEC §10.1), без DPI-notice; трейлинг-точка легальна.
    let (_, notices) = notices_of("G21 G90 G94 G17;\nG0 X25. Y50. Z0.0001;\nM30;\n");
    assert!(notices.iter().all(|n| n.kind != NoticeKind::DpiAssumed));
    let p = parse("G21 G90 G94 G17;\nG0 X25. Y50. Z0.0001;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Rapid { end, .. } => assert_pt(end, 25.0, 50.0, 0.0001),
        _ => panic!("op0 не Rapid"),
    }
}

#[test]
fn dpi_all_bare_arc() {
    // Дуга со всеми bare-словами: геометрия — равномерное сжатие ×0.001
    // (X-5000→-5, Y5000→5, I0→0, J5000→5); F — ИСКЛЮЧЕНИЕ (ruling 2):
    // F-address IS-B инкремент = 1 mm/min (param 1422) → bare F = целое
    // mm/min verbatim (F100000 → 100000, НЕ 100).
    let (p, notices) =
        notices_of("G21 G90 G94 G17;\nG0 X0 Y0 Z0;\nG2 X-5000 Y5000 I0 J5000 F100000;\nM30;\n");
    match &p.ops[1] {
        Op::Arc {
            center,
            sweep_deg,
            dir,
            ..
        } => {
            assert_pt(center, 0.0, 5.0, 0.0);
            assert!(
                (sweep_deg.value() - 90.0).abs() <= EPS,
                "sweep {}",
                sweep_deg.value()
            );
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
        }
        _ => panic!("op1 не Arc"),
    }
    assert_eq!(feed_of(&p.ops[1]), Some(100000.0));
    let dpi = notices
        .iter()
        .filter(|n| n.kind == NoticeKind::DpiAssumed)
        .count();
    assert!(dpi >= 5, "все слова дуги bare: {dpi}");
}

#[test]
fn dpi_pending_f_flushes_to_next_op() {
    // F-only блок (нет движения): DPI-notice висит до следующего опа.
    // Ruling 2: bare F100 = 100 mm/min verbatim (IS-B F-инкремент 1 mm/min,
    // параметр 1422), НЕ ×0.001 — notice сохраняется.
    let (p, notices) = notices_of("G21 G90 G94;\nF100;\nG1 X5.;\nM2;\n");
    assert_eq!(p.ops.len(), 2); // Linear + ProgramEnd
    assert_eq!(feed_of(&p.ops[0]), Some(100.0));
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, NoticeKind::DpiAssumed);
    assert_eq!(notices[0].op_index, 0, "notice прицеплен к Linear");
    assert!(notices[0].message.contains("F100"));
    assert!(
        notices[0].message.contains("1422"),
        "F-address ruling 2 в тексте notice"
    );
}

// ---------------------------------------------------------------------------
// Дуги §6 — все формы
// ---------------------------------------------------------------------------

#[test]
fn arc_ijk_cw_helix() {
    // IJK инкрементальные от start (§2.1), helical Z→pitch, G17 → normal +Z.
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X-5. Y5. Z2. I0. J5. F100.;\nM30;\n")
        .expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Rapid {
            start: StartPoint::Unknown,
            ..
        }
    ));
    match &p.ops[1] {
        Op::Arc {
            start,
            end,
            center,
            normal,
            dir,
            sweep_deg,
            pitch,
            state,
            ..
        } => {
            assert_eq!(known(&p.ops[1]), [0.0, 0.0, 0.0]);
            assert!(matches!(start, StartPoint::Known(_)));
            assert_pt(end, -5.0, 5.0, 2.0);
            assert_pt(center, 0.0, 5.0, 0.0);
            assert_pt(normal, 0.0, 0.0, 1.0);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
            assert!((pitch.value() - 2.0).abs() <= EPS);
            assert_eq!(state.feed.as_ref().map(|n| n.value()), Some(100.0));
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_ijk_ccw() {
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG3 X5. Y-5. Z-2. I5. J0. F100.;\nM30;\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            normal,
            dir,
            sweep_deg,
            pitch,
            ..
        } => {
            assert_pt(center, 5.0, 0.0, 0.0);
            assert_pt(normal, 0.0, 0.0, 1.0);
            assert_eq!(*dir, eid_core::ast::Dir::Ccw);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
            assert!((pitch.value() + 2.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_g18() {
    // G18 = ZX-плоскость: пара I(X)+K(Z), нормаль +Y, linear Y → pitch.
    let p = parse("G21 G90 G94 G18;\nG0 X0. Y0. Z0.;\nG2 X5. Y2. Z-5. I5. K0. F100.;\nM30;\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            normal,
            sweep_deg,
            pitch,
            ..
        } => {
            assert_pt(center, 5.0, 0.0, 0.0);
            assert_pt(normal, 0.0, 1.0, 0.0);
            assert!(
                (sweep_deg.value() - 90.0).abs() <= EPS,
                "sweep {}",
                sweep_deg.value()
            );
            assert!((pitch.value() - 2.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_g19() {
    // G19 = YZ: пара J(Y)+K(Z), нормаль +X, linear X.
    let p = parse("G21 G90 G94 G19;\nG0 X0. Y0. Z0.;\nG3 X2. Y5. Z-5. J5. K0. F100.;\nM30;\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            normal,
            sweep_deg,
            ..
        } => {
            assert_pt(center, 0.0, 5.0, 0.0);
            assert_pt(normal, 1.0, 0.0, 0.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_r_positive() {
    // R>0 → дуга ≤180°: из (0,10) в (10,0) по R10 — четверть вокруг (0,0).
    let p =
        parse("G21 G90 G94 G17;\nG0 X0. Y10. Z0.;\nG2 X10. Y0. R10. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            sweep_deg,
            pitch,
            ..
        } => {
            assert_pt(center, 0.0, 0.0, 0.0);
            assert!(
                (sweep_deg.value() - 90.0).abs() <= EPS,
                "sweep {}",
                sweep_deg.value()
            );
            assert!(pitch.value().abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_r_negative() {
    // R<0 → дуга >180° (знак R выбирает сторону; SPEC §6).
    let p =
        parse("G21 G90 G94 G17;\nG0 X0. Y10. Z0.;\nG2 X10. Y0. R-10. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center, sweep_deg, ..
        } => {
            assert_pt(center, 10.0, 10.0, 0.0);
            assert!(
                (sweep_deg.value() - 270.0).abs() <= EPS,
                "sweep {}",
                sweep_deg.value()
            );
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_r_helix() {
    // R + винтовая компонента валидна (SPEC §6.1): Z вне плоскости → pitch.
    let p =
        parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X8. Y0. Z2. R5. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            pitch,
            sweep_deg,
            ..
        } => {
            assert_pt(center, 4.0, -3.0, 0.0);
            assert!((pitch.value() - 2.0).abs() <= EPS);
            assert!(
                (sweep_deg.value() - 106.26).abs() <= 0.01,
                "sweep {}",
                sweep_deg.value()
            );
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn full_circle() {
    // Полная окружность: start==end (end-слова отсутствуют) + center≠start →
    // sweep 360° (§6 отчёта; SPEC §6 ловушка (а)).
    let (p, notices) = notices_of("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 I-5. F100.;\nM30;\n");
    assert!(
        notices.iter().all(|n| n.kind != NoticeKind::ZeroLengthArc),
        "полное кольцо — не zero-length drop"
    );
    match &p.ops[1] {
        Op::Arc {
            end,
            center,
            sweep_deg,
            pitch,
            ..
        } => {
            assert_pt(end, 0.0, 0.0, 0.0);
            assert_pt(center, -5.0, 0.0, 0.0);
            assert!(
                (sweep_deg.value() - 360.0).abs() <= EPS,
                "sweep {}",
                sweep_deg.value()
            );
            assert!(pitch.value().abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn helical_full_turn() {
    // Винтовой полный оборот: in-plane start==end, Z-слово → pitch 5.
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 I-5. Z5. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc {
            sweep_deg, pitch, ..
        } => {
            assert!((sweep_deg.value() - 360.0).abs() <= EPS);
            assert!((pitch.value() - 5.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_g90_1_absolute_center() {
    // G90.1 (группа 04, опция): I/J — АБСОЛЮТНЫЙ центр, как LinuxCNC
    // (анализ §2.2; инкрементальная интерпретация дала бы центр (10,5) и
    // radius-mismatch → Err, поэтому позитивный тест доказывает G90.1-путь).
    let p = parse(
        "G21 G90 G94 G17 G90.1;\nG0 X10. Y0. Z0.;\nG2 X-5. Y-5. I0. J5. F100.;\nG91.1;\nM30;\n",
    )
    .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center, sweep_deg, ..
        } => {
            assert_pt(center, 0.0, 5.0, 0.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
    // G91.1 возвращает инкрементальный режим — следующая дуга ок.
    let p = parse(
        "G21 G90 G94 G17;\nG90.1;\nG91.1;\nG0 X0. Y0. Z0.;\nG2 X-5. Y5. I0. J5. F100.;\nM30;\n",
    )
    .expect("ok");
    match &p.ops[1] {
        Op::Arc { center, .. } => assert_pt(center, 0.0, 5.0, 0.0),
        _ => panic!("оп не Arc"),
    }
}

// ---------------------------------------------------------------------------
// Дуги §6 — негативы
// ---------------------------------------------------------------------------

#[test]
fn r_full_circle_invalid() {
    // R при полной окружности → E_INVALID_INPUT (SPEC §6(2), §6 отчёта:
    // оба контроллера alarm'ят R-полный-круг; центр неоднозначен).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 R5. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X0. Y0. R5. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn radius_mismatch_invalid() {
    // ρ_start=5, ρ_end=|(3,4)−(0,5)|=3.16 → невозможная дуга (SPEC §6).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X3. Y4. I0. J5. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn chord_gt_2r_invalid() {
    // Хорда 20 > 2R=10 — невозможная дуга (SPEC §6).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X20. Y0. R5. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn plane_perpendicular_invalid() {
    // Валидация нормали (SPEC §4/§6): перпендикулярная компонента (center −
    // start) обязана быть ≈0 — K≠0 в G17 / J≠0 в G18 → E_INVALID_INPUT.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X-5. Y5. K2. I0. J5. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G18;\nG0 X0. Y0. Z0.;\nG2 X5. Y2. Z-5. I5. J1. K0. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

// ---------------------------------------------------------------------------
// Движение: модальность, chaining, zero-length
// ---------------------------------------------------------------------------

#[test]
fn modal_continuation() {
    // Адресный язык Fanuc (§2.7): X без G-кода = продолжение группы 01.
    let p = parse("G21 G90 G94 G17;\nG1 X5. F100.;\nX10.;\nZ2.;\nM30;\n").expect("ok");
    assert_eq!(p.ops.len(), 4); // 3 Linear + ProgramEnd
    assert!(matches!(&p.ops[1], Op::Linear { .. }));
    match &p.ops[2] {
        Op::Linear { end, .. } => assert_pt(end, 10.0, 0.0, 2.0),
        _ => unreachable!(),
    }
    // chaining: start опа = end предыдущего; старт первого — Unknown (точка
    // разрыва (a), SPEC §2).
    assert!(matches!(
        &p.ops[0],
        Op::Linear {
            start: StartPoint::Unknown,
            ..
        }
    ));
    assert_eq!(known(&p.ops[1]), [5.0, 0.0, 0.0]);
    assert_eq!(known(&p.ops[2]), [10.0, 0.0, 0.0]);
}

#[test]
fn naked_coords_no_motion_invalid() {
    // Голые координаты без активного motion-кода (старт программы) → INVALID.
    assert_eq!(
        err_code(parse("G21 G90 G94 G17;\nX5. Y5.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn zero_length_linear_drop() {
    // start == end → drop + notice (SPEC §3).
    let (_, notices) =
        notices_of("G21 G90 G94 G17;\nG0 X5. Y0. Z0.;\nG1 X5. Y0. Z0. F100.;\nM30;\n");
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, NoticeKind::ZeroLengthMotion);
}

#[test]
fn g91_incremental() {
    // G91 на старте: база = дефолт контроллера 0 (SPEC §2 v0.11); start
    // Unknown (точка разрыва (a)), end Known.
    let p = parse("G91 G17;\nG1 X1. Y2. F100.;\nX1. Y1.;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Linear {
            start: StartPoint::Unknown,
            ..
        }
    ));
    match &p.ops[0] {
        Op::Linear { end, .. } => assert_pt(end, 1.0, 2.0, 0.0),
        _ => unreachable!(),
    }
    // Второй инкрементальный ход цепляется: (1,2) + (1,1) = (2,3).
    match &p.ops[1] {
        Op::Linear { end, .. } => assert_pt(end, 2.0, 3.0, 0.0),
        _ => unreachable!(),
    }
}

#[test]
fn linear_without_feed_invalid() {
    // linear/arc до первого F → E_INVALID_INPUT (SPEC §3).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG1 X5.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
    // G0 без F легален (rapid не ходит по feed).
    parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n").expect("rapid без F легален");
}

#[test]
fn modal_violation_invalid() {
    // §1.1: один код на группу на блок.
    assert_eq!(
        err_code(parse("G21 G90 G94 G17;\nG0 G1 X5. F100.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
    // Несколько РАЗНЫХ групп в блоке легальны.
    parse("G90 G17 G21 G94 G0 X0. Y0. Z0.;\nM30;\n").expect("разные группы легальны");
}

// ---------------------------------------------------------------------------
// Модальности 02–06
// ---------------------------------------------------------------------------

#[test]
fn unit_switch_mid_invalid() {
    // SPEC §2.5: одна система единиц на программу.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG20;\nG0 X2. Y1. Z1.;\nM30;\n"
        )),
        ErrorCode::UnsupportedSemantics
    );
}

#[test]
fn units_alias_g70_g71() {
    // Псевдонимы units (§2.2): G70 → inch, G71 → mm; распознавать, не Raw.
    let p = parse("G70 G90 G94 G17;\nG0 X25. Y0. Z0.;\nM30;\n").expect("ok");
    assert_eq!(p.units, Units::In);
    let p = parse("G71 G90 G94 G17;\nG0 X25. Y0. Z0.;\nM30;\n").expect("ok");
    assert_eq!(p.units, Units::Mm);
}

#[test]
fn g95_raw_semantics() {
    // G94 ABSORB; G95 → Raw{E_UNSUPPORTED_SEMANTICS} (§2.2: feed/rev требует
    // rpm, зависящего от override).
    let out = canonicalize_with_notices("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n", "t.ngc")
        .expect("ok");
    assert!(out.program.ops.iter().all(|o| !matches!(o, Op::Raw { .. })));
    let out = canonicalize_with_notices("G21 G90 G95 G17;\nG0 X1. Y1. Z1.;\nM30;\n", "t.ngc")
        .expect("Ok + Raw");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
}

// ---------------------------------------------------------------------------
// Лексика/структура файла (§1.3)
// ---------------------------------------------------------------------------

#[test]
fn percent_ignored() {
    // `%` — IGNORED_SAFE (§1.3): нет опов, нет диагностики.
    let (p, notices) = notices_of("%\nG21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n%\n");
    assert_eq!(p.ops.len(), 2); // Rapid + ProgramEnd
    assert!(notices.is_empty());
}

#[test]
fn n_word_line() {
    // N-слова — IGNORED_SAFE (§1.3).
    let p =
        parse("N10 G21 G90 G94 G17;\nN20 G0 X1. Y1. Z1.;\nN30 G1 X5. F100.;\nM30;\n").expect("ok");
    assert_eq!(p.ops.len(), 3);
}

#[test]
fn empty_blocks_and_comments_skipped() {
    // Пустые блоки — пропуск без диагностики (§1.3); комментарии — аннотации.
    let (p, notices) = notices_of(
        ";;\n(cStartup)G21 G90 G94 G17;;\nG0 X1. Y1. Z1.;\n(face) G1 X5. F100. (pass);\nM30;\n",
    );
    assert_eq!(p.ops.len(), 3); // Rapid + Linear + ProgramEnd
    assert!(notices.is_empty());
}

#[test]
fn o_code_raw() {
    // O-код → Raw{E_UNSUPPORTED_SYNTAX} (§1.3, SPEC §1.1 v0.16) — финальная
    // диспозиция; программа продолжается.
    let out =
        canonicalize_with_notices("O0030;\nG21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n", "o.ngc")
            .expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    assert!(matches!(&out.program.ops[1], Op::Rapid { .. }));
}

#[test]
fn unknown_g_raw_provisional() {
    // Этап 2: G43/H → финальная диспозиция Raw{REQUIRES_MACHINE_TABLE}
    // (COVERAGE §2.4, TLO из таблицы; не provisional).
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17;\nG43 H2;\nG0 X1. Y1. Z1.;\nM30;\n",
        "g.ngc",
    )
    .expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
    assert!(matches!(&out.program.ops[1], Op::Rapid { .. }));
    // Голое S — легальное сохранение оборотов (§3 отчёта), не Raw.
    let out =
        canonicalize_with_notices("S5000;\nG21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n", "s.ngc")
            .expect("ok");
    assert!(out
        .program
        .ops
        .iter()
        .take(1)
        .all(|o| !matches!(o, Op::Raw { .. })));
    assert_eq!(out.program.ops.len(), 2);
}

#[test]
fn block_delete_raw_runtime() {
    // `/` block delete → Raw{E_RUNTIME_STATE} (§1.3, как LinuxCNC COVERAGE §4).
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17;\n/ G1 X5. F100.;\nG0 X1. Y1. Z1.;\nM30;\n",
        "b.ngc",
    )
    .expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::RuntimeState,
            ..
        }
    ));
}

#[test]
fn stage_parse_vs_verify() {
    // interface v0.1.2: lexical → Stage::Parse; канон-инвариант → Stage::Verify.
    let d = parse("G21;\nG0 X1. Y1. Z1.;\nG1 &;\nM30;\n").unwrap_err();
    assert_eq!(d[0].code, ErrorCode::InvalidInput);
    assert_eq!(err_stage(Err(d)), Stage::Parse);
    let d = parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG20;\nG0 X2. Y1. Z1.;\nM30;\n").unwrap_err();
    assert_eq!(d[0].code, ErrorCode::UnsupportedSemantics);
    assert_eq!(err_stage(Err(d)), Stage::Verify);
}

#[test]
fn integer_overflow_limit() {
    // §1.2: целая часть >9 цифр → E_LIMIT_EXCEEDED (max_word_chars не ловит).
    let d = parse("G21 G90;\nG0 X1234567890. Y0. Z0.;\nM30;\n").unwrap_err();
    assert_eq!(d[0].code, ErrorCode::LimitExceeded);
    assert_eq!(
        d[0].limit.as_ref().map(|l| l.threshold),
        Some("max_integer_digits")
    );
    assert_eq!(d[0].limit.as_ref().map(|l| l.actual), Some(10.0));
    // 9 цифр легальны.
    parse("G21 G90 G94 G17;\nG0 X123456789. Y0. Z0.;\nM30;\n").expect("9 цифр легальны");
}

#[test]
fn subs_tail_after_program_end_is_raw_not_err() {
    // Анализ §4.4/§7.3: main M30 + тела subs O#### в том же файле — норма
    // Fanuc → хвост после PEND — line-level Raw{SYNTAX}, НЕ жёсткий Err
    // (расхождение с grbl n12 задокументировано контрактом). Motion-опы
    // после PEND не строятся.
    let p = parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\nO0100;\nG1 X-10. F200.;\nM99;\n")
        .expect("Ok (хвост → Raw)");
    // ops: Rapid, ProgramEnd, Raw(O), Raw(G1-тело subs), Raw(M99)
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    assert!(matches!(&p.ops[1], Op::ProgramEnd { .. }));
    assert!(matches!(
        &p.ops[2],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    assert!(matches!(
        &p.ops[3],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    assert!(matches!(
        &p.ops[4],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    assert_eq!(p.ops.len(), 5);
}

#[test]
fn lowercase_and_whitespace_tolerance() {
    // Регистр и whitespace между словами — легальны (паттерн grbl gcode.c).
    let p =
        parse("g21 g90 g94 g17;\ng0 x0. y0. z0.;\ng2 x-5. y5. i0. j5. f100.;\nm30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, .. } => assert_pt(center, 0.0, 5.0, 0.0),
        _ => panic!("op1 не Arc"),
    }
}

// ===========================================================================
// ЭТАП 2a — Фреймы
// ===========================================================================

fn spindle_of(op: &Op) -> Option<&eid_core::ast::Spindle> {
    match op {
        Op::Rapid { state, .. } | Op::Linear { state, .. } | Op::Arc { state, .. } => {
            Some(&state.spindle)
        }
        _ => None,
    }
}

fn wcs_of_op(op: &Op) -> Option<eid_core::ast::Wcs> {
    match op {
        Op::Rapid { state, .. } | Op::Linear { state, .. } | Op::Arc { state, .. } => {
            Some(state.wcs)
        }
        _ => None,
    }
}

#[allow(dead_code)] // хелпер этапа 2 — используется точечно; предупреждение чистоты сборки
fn frame_of_op(op: &Op) -> Option<eid_core::ast::Frame> {
    match op {
        Op::Rapid { state, .. } | Op::Linear { state, .. } | Op::Arc { state, .. } => {
            Some(state.frame)
        }
        _ => None,
    }
}

fn end_of(op: &Op) -> [f64; 3] {
    match op {
        Op::Rapid { end, .. } | Op::Linear { end, .. } => pt3(end),
        other => panic!("у опа нет end: {other:?}"),
    }
}

fn raw_reason(p: &Program, idx: usize) -> String {
    match &p.ops[idx] {
        Op::Raw { reason, code, .. } => format!("{code:?}: {reason}"),
        other => panic!("op{idx} не Raw: {other:?}"),
    }
}

fn hex_decode(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn wcs_select_g54_g59() {
    // G54–G59 → state.wcs p=1..6 (ABSORB, выбор — не запись).
    let p = parse(
        "G21 G90 G94 G17;\nG54;\nG0 X1. Y1. Z1.;\nG55;\nG0 X2. Y2. Z2.;\nG59;\nG0 X3. Y3. Z3.;\nM30;\n",
    )
    .expect("ok");
    // G54-блок без координат — опов не даёт; WCS видна в state ходов.
    assert_eq!(wcs_of_op(&p.ops[0]), Some(eid_core::ast::Wcs::G54));
    assert_eq!(wcs_of_op(&p.ops[1]), Some(eid_core::ast::Wcs::G55));
    assert_eq!(wcs_of_op(&p.ops[2]), Some(eid_core::ast::Wcs::G59));
    // Смена WCS — точка разрыва (b) без origin → start Unknown.
    assert!(matches!(
        &p.ops[1],
        Op::Rapid {
            start: StartPoint::Unknown,
            ..
        }
    ));
}

#[test]
fn wcs_switch_unknown_without_origins() {
    // Точка разрыва (b): ни одна из систем не определена in-text →
    // reading_new неизвестен; полный end легален.
    let p =
        parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG55;\nG0 X1. Y2. Z3.;\nM30;\n").expect("ok");
    // ops: Rapid(G54, start Unknown — разрыв (a)), Rapid(G55, start Unknown —
    // разрыв (b)), PEND.
    assert_eq!(end_of(&p.ops[0]), [10.0, 10.0, 5.0]);
    match (&p.ops[0], &p.ops[1]) {
        (Op::Rapid { start: s0, .. }, Op::Rapid { start: s1, .. }) => {
            assert!(matches!(s0, StartPoint::Unknown));
            assert!(
                matches!(s1, StartPoint::Unknown),
                "разрыв (b): reading неизвестен"
            );
        }
        _ => panic!("ожидались 2 Rapid"),
    }
}

#[test]
fn wcs_switch_with_origins() {
    // Точка разрыва (b): обе системы in-text → reading = prev + O_prev − O_new.
    // O54 = (0,0,0), O55 = (100,0,0): reading_G55 = (10,10,5) − (100,0,0).
    let p = parse(
        "G21 G90 G94 G17;\nG10 L2 P1 X0. Y0. Z0.;\nG10 L2 P2 X100. Y0. Z0.;\nG0 X10. Y10. Z5.;\nG55;\nG0 X1. Y2. Z3.;\nM30;\n",
    )
    .expect("ok");
    // ops: WcsDefine×2, Rapid, Rapid, PEND.
    assert!(matches!(&p.ops[0], Op::WcsDefine { p: 1, .. }));
    assert!(matches!(&p.ops[1], Op::WcsDefine { p: 2, .. }));
    assert_eq!(
        known(&p.ops[3]),
        [-90.0, 10.0, 5.0],
        "reading вычислен из разности origin"
    );
    match &p.ops[3] {
        Op::Rapid { end, .. } => assert_pt(end, 1.0, 2.0, 3.0),
        _ => panic!("op3 не Rapid"),
    }
}

#[test]
fn g10_l2_wcs_define() {
    // G10 L2 P1–P6 → wcs_define (NODE, §5.3 SPEC); абсолютная запись.
    let p = parse("G21 G90 G94 G17;\nG10 L2 P3 X5. Y6. Z7.;\nM30;\n").expect("ok");
    assert!(matches!(&p.ops[0], Op::WcsDefine { p: 3, .. }));
    match &p.ops[0] {
        Op::WcsDefine { origin, .. } => assert_pt(origin, 5.0, 6.0, 7.0),
        _ => unreachable!(),
    }
    // P7–P9 — вне Fanuc-домена 1..6 → Raw{SYNTAX}.
    let out = canonicalize_with_notices("G10 L2 P7 X1. Y1. Z1.;\nM30;\n", "t.ngc").expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
}

#[test]
fn g10_l2_incremental_record() {
    // G91 в G10-кадре — инкремент записи: origin = old + delta (§5.3 SPEC);
    // отсутствующий old = 0 (интерпретация — паритет LinuxCNC-адаптера).
    let p = parse(
        "G21 G90 G94 G17;\nG10 L2 P1 X10. Y10. Z10.;\nG91;\nG10 L2 P1 X5. Y5. Z5.;\nG90;\nM30;\n",
    )
    .expect("ok");
    match (&p.ops[0], &p.ops[1]) {
        (Op::WcsDefine { origin: o1, .. }, Op::WcsDefine { origin: o2, .. }) => {
            assert_pt(o1, 10.0, 10.0, 10.0);
            assert_pt(o2, 15.0, 15.0, 15.0);
        }
        _ => panic!("ожидались 2 WcsDefine"),
    }
}

#[test]
fn g10_l2_partial_raw_requires() {
    // Partial G10 L2 (не все X/Y/Z) → Raw{REQUIRES_MACHINE_TABLE}: невыписанные
    // оси Fanuc сохраняет неизменными — значение из таблицы станка, вне текста
    // (ruling брифа этапа 2a; расхождение с «1:1 LinuxCNC» COVERAGE §2.3 —
    // в отчёт этапа 2).
    for cmd in ["G10 L2 P2 X9.;", "G10 L2 P2 X9. Y9.;", "G10 L2 P2;"] {
        let out = canonicalize_with_notices(&format!("{cmd}\nM30;\n"), "t.ngc").expect("ok");
        assert!(
            matches!(
                &out.program.ops[0],
                Op::Raw {
                    code: ErrorCode::RequiresMachineTable,
                    ..
                }
            ),
            "{cmd}"
        );
    }
}

#[test]
fn g10_l2_p0_raw_requires() {
    // G10 L2 P0 — external/common offset → Raw{E_REQUIRES_MACHINE_TABLE}.
    let out = canonicalize_with_notices("G10 L2 P0 X1. Y1. Z1.;\nM30;\n", "t.ngc").expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
    assert!(raw_reason(&out.program, 0).contains("external"));
}

#[test]
fn g10_l20_raw_not_set_here() {
    // Ловушка №3: G10 L20 Fanuc = запись origin G54.1 Pn, НЕ wcs_set_here.
    // Маппинг L20 → WcsSetHere ЗАПРЕЩЁН (COVERAGE §2.3).
    let out = canonicalize_with_notices("G10 L20 P1 X1. Y1. Z1.;\nM30;\n", "t.ngc").expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
    assert!(raw_reason(&out.program, 0).contains("NOT wcs_set_here"));
    assert!(!out
        .program
        .ops
        .iter()
        .any(|o| matches!(o, Op::WcsSetHere { .. })));
}

#[test]
fn g10_l1_table_raw() {
    // G10 L1/L10/L11/L50+ — таблицы инструментов/параметров → Raw{REQUIRES}.
    for l in ["L1", "L10", "L11", "L50"] {
        let out = canonicalize_with_notices(&format!("G10 {l} P1 X1. Y1. Z1.;\nM30;\n"), "t.ngc")
            .expect("ok");
        assert!(matches!(
            &out.program.ops[0],
            Op::Raw {
                code: ErrorCode::RequiresMachineTable,
                ..
            }
        ));
    }
}

#[test]
fn g11_cancel_ignored() {
    // G11 (cancel G10-режима) без RAW G10-форм в файле — IGNORED_SAFE.
    let (p, notices) = notices_of("G10 L2 P1 X1. Y1. Z1.;\nG11;\nM30;\n");
    assert_eq!(p.ops.len(), 2); // WcsDefine + ProgramEnd
    assert!(notices.is_empty());
}

#[test]
fn g11_family_raw_inherit() {
    // Семейство G11: RAW G10-форма (L20) в файле → G11 → Raw{SYNTAX}
    // (raw-наследование, COVERAGE §1.3).
    let p = parse("G10 L20 P1 X1. Y1. Z1.;\nG11;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
    assert!(matches!(
        &p.ops[1],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
}

#[test]
fn g92_frame_preset_anchor() {
    // G92 → frame_preset (META): reading первой привязки = значения пресетки.
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17;\nG92 X5. Y5. Z5.;\nG1 X10. F100.;\nM30;\n",
        "t.ngc",
    )
    .expect("ok");
    let pr = out.program.frame_preset.as_ref().expect("frame_preset");
    assert_pt(&pr.values, 5.0, 5.0, 5.0);
    assert_eq!(pr.wcs, eid_core::ast::Wcs::G54);
    match &out.program.ops[0] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_pt(s, 5.0, 5.0, 5.0);
            assert_pt(end, 10.0, 5.0, 5.0);
        }
        _ => panic!("op0 не Linear с Known start"),
    }
}

#[test]
fn g92_mid_program_invalid() {
    // Позиционное ограничение §3 SPEC: G92 после motion — отказ.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG92 X0. Y0. Z0.;\nM30;\n"
        )),
        ErrorCode::UnsupportedSemantics
    );
}

#[test]
fn g92_s_clamp_raw() {
    // G92 S — кламп оборотов: тот же адрес, другой word → Raw{SEMANTICS}
    // (НЕ frame_preset; COVERAGE §2.6).
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17;\nG92 S1500;\nG0 X1. Y1. Z1.;\nM30;\n",
        "t.ngc",
    )
    .expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    assert!(raw_reason(&out.program, 0).contains("clamp"));
    assert!(out.program.frame_preset.is_none(), "S-слово не пресетка");
}

#[test]
fn g52_hard_err() {
    // G52 — ЕДИНСТВЕННЫЙ жёсткий отказ Fanuc (SPEC §8.5; COVERAGE §2.3):
    // смена фрейма mid-program — raw-сохранение исказило бы последующие ходы.
    let d = parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG52 X5. Y5.;\nG0 X2. Y2. Z2.;\nM30;\n")
        .unwrap_err();
    assert_eq!(d[0].code, ErrorCode::UnsupportedSemantics);
    assert!(d[0].message.contains("G52"));
    // G52 как первый кадр — тоже Err (структурный класс, не raw).
    let d = parse("G52 X0. Y0. Z0.;\nM30;\n").unwrap_err();
    assert_eq!(d[0].code, ErrorCode::UnsupportedSemantics);
}

#[test]
fn g53_machine_move() {
    // G53 machine-frame ход (правило §5.3): origin in-text → start вычислим
    // (M = W + O_wcs); точка разрыва (d) — обратный WCS-ход из M.
    let p = parse(
        "G21 G90 G94 G17;\nG10 L2 P1 X0. Y0. Z0.;\nG0 X10. Y10. Z5.;\nG53 G0 X5. Y5. Z-100.;\nG0 X7. Y7. Z7.;\nM30;\n",
    )
    .expect("ok");
    // ops: WcsDefine, Rapid(wcs), Rapid(machine), Rapid(wcs), PEND.
    match &p.ops[2] {
        Op::Rapid { end, state, .. } => {
            assert_eq!(known(&p.ops[2]), [10.0, 10.0, 5.0], "M = W + O_wcs");
            assert_pt(end, 5.0, 5.0, -100.0);
            assert_eq!(state.frame, eid_core::ast::Frame::Machine);
        }
        _ => panic!("op2 не machine Rapid"),
    }
    // Точка разрыва (d): reading_wcs = M − O_wcs − Δ = (5,5,−100).
    match &p.ops[3] {
        Op::Rapid { state, .. } => {
            assert_eq!(state.frame, eid_core::ast::Frame::Wcs);
            assert_eq!(known(&p.ops[3]), [5.0, 5.0, -100.0]);
        }
        _ => panic!("op3 не wcs Rapid"),
    }
}

#[test]
fn g53_first_move_unknown() {
    // Первое движение в machine-frame: origin in-text, M₀ внешняя — точка
    // разрыва (c): start Unknown, полный end обязателен.
    let p =
        parse("G21 G90 G94 G17;\nG10 L2 P1 X1. Y2. Z3.;\nG53 G0 X0. Y0. Z0.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Rapid {
            start, end, state, ..
        } => {
            assert!(matches!(start, StartPoint::Unknown));
            assert_pt(end, 0.0, 0.0, 0.0);
            assert_eq!(state.frame, eid_core::ast::Frame::Machine);
        }
        _ => panic!("op1 не machine Rapid"),
    }
    // Origin активной WCS не определён → E_RUNTIME_STATE (§5.3b).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17;\nG53 G0 X0. Y0. Z0.;\nM30;\n")),
        ErrorCode::RuntimeState
    );
}

#[test]
fn g53_partial_end_invalid() {
    // Machine-ходы — только полные end (нуль не наследуется, SPEC §2 v0.13).
    let p =
        parse("G21 G90 G94 G17;\nG10 L2 P1 X0. Y0. Z0.;\nG0 X1. Y1. Z1.;\nG53 G0 Z-100.;\nM30;\n");
    assert_eq!(err_code(p), ErrorCode::RuntimeState);
}

#[test]
fn g53_after_preset_invalid() {
    // §5.3(a): machine-ход при активном frame_preset — невыразим.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG92 X5. Y5. Z5.;\nG53 G0 X0. Y0. Z0.;\nM30;\n"
        )),
        ErrorCode::RuntimeState
    );
}

#[test]
fn g53_requires_explicit_motion() {
    // Fanuc: G53 обязан быть в одном кадре с G0/G1; дуги невыразимы.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG10 L2 P1 X0. Y0. Z0.;\nG1 X5. F100.;\nG53 X0. Y0. Z-1.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG10 L2 P1 X0. Y0. Z0.;\nG0 X5. Y5. Z5.;\nG53 G2 X0. Y0. Z-1. I5.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
    // `G53;` без осей — no-op (без требования G0/G1).
    let p = parse("G21 G90 G94 G17;\nG53;\nG0 X1. Y1. Z1.;\nM30;\n").expect("ok");
    assert_eq!(p.ops.len(), 2);
}

#[test]
fn g54_1_pn_raw() {
    // G54.1 Pn — доп. WCS вне wcs-enum L1 → Raw{SEMANTICS} (COVERAGE §2.3).
    let out = canonicalize_with_notices("G54.1 P5;\nM30;\n", "t.ngc").expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    assert!(raw_reason(&out.program, 0).contains("G54.1"));
}

#[test]
fn g92_1_reset_raw() {
    // G92.1/.2/.3 → Raw{SEMANTICS} (Q4; COVERAGE §2.3).
    for code in ["G92.1", "G92.2", "G92.3"] {
        let out = canonicalize_with_notices(&format!("{code};\nM30;\n"), "t.ngc").expect("ok");
        assert!(
            matches!(
                &out.program.ops[0],
                Op::Raw {
                    code: ErrorCode::UnsupportedSemantics,
                    ..
                }
            ),
            "{code}"
        );
    }
}

// ===========================================================================
// ЭТАП 2b — Циклы (Fanuc-семантика §5 отчёта; канон-шаблон §7.7 SPEC)
// ===========================================================================

#[test]
fn cycle_g81_unroll_template() {
    // §7.7 шаблон: traverse (на текущем Z) → approach (R) → feed (Z_bottom)
    // → retract (дефолт G99 §7.5a → R). F кадра — на ВСЕ производные опы.
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG81 X20. Y20. Z-5. R2. F100.;\nM30;\n")
        .expect("ok");
    // ops: Rapid(G0), traverse, approach, feed, retract, PEND = 6.
    assert_eq!(p.ops.len(), 6);
    assert_eq!(
        end_of(&p.ops[1]),
        [20.0, 20.0, 5.0],
        "traverse не сливается с подходом"
    );
    assert_eq!(
        end_of(&p.ops[2]),
        [20.0, 20.0, 2.0],
        "approach — отдельный оп"
    );
    assert_eq!(end_of(&p.ops[3]), [20.0, 20.0, -5.0], "feed на дно");
    assert_eq!(end_of(&p.ops[4]), [20.0, 20.0, 2.0], "retract G99 → R");
    // F на все производные опы + sub-нумерация развёртки.
    for i in 1..5 {
        assert_eq!(feed_of(&p.ops[i]), Some(100.0), "op{i}");
        let sub = match &p.ops[i] {
            Op::Rapid { sub, .. } | Op::Linear { sub, .. } => *sub,
            _ => panic!("op{i} не движение"),
        };
        assert_eq!(sub, Some(i as u32), "sub-нумерация op{i}");
    }
    // Связывание: start traverse = end G0.
    assert_eq!(known(&p.ops[1]), [10.0, 10.0, 5.0]);
}

#[test]
fn cycle_g82_dwell_p_ms() {
    // G82: P dwell — bare P = ЦЕЛЫЕ МИЛЛИСЕКУНДЫ (§1.2 Fanuc) → секунды;
    // dotted P (сломанная Fanuc-форма) — секунды verbatim (v0.18 зеркало D:
    // rs274/LinuxCNC-совместимое чтение, golden #21 P1.0 → 1.0 s).
    let p = parse(
        "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG81 X20. Y20. Z-5. R2. F100.;\nG82 X30. Y20. Z-6. R2. P500;\nM30;\n",
    )
    .expect("ok");
    // G81: 4 опа (1..4); G82: traverse + [approach — ZERO-LENGTH (cz == 2 ==
    // r_cur после отвода G81) → drop+notice, этап 3/SPEC §3] + drill + dwell
    // + retract = 4 опа (5..8) — dwell между feed и retract.
    assert!(
        matches!(&p.ops[7], Op::Dwell { seconds, .. } if (seconds.value() - 0.5).abs() <= EPS),
        "bare P500 → 0.5 s"
    );
    assert_eq!(end_of(&p.ops[8]), [30.0, 20.0, 2.0], "retract после dwell");
    // dotted P → секунды verbatim (P1.0 = 1.0 s, не 1 мс).
    let p =
        parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG82 X20. Y20. Z-5. R2. P1.0 F100.;\nM30;\n")
            .expect("ok");
    assert!(
        matches!(&p.ops[4], Op::Dwell { seconds, .. } if (seconds.value() - 1.0).abs() <= EPS),
        "dotted P1.0 → 1.0 s verbatim"
    );
    // G82 без P — сломанный вход (паритет LinuxCNC-адаптера).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG82 X20. Y20. Z-5. R2. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn cycle_g83_peck_full_retract() {
    // G83: полный отвод к R после каждого пика; подход rapid на δ над
    // последней глубиной (δ = 0.254mm unit-scaled, SPEC §7.4a v0.18 —
    // зонд rs274; прежде 0.5mm v0.14). R=2, Z=-10, Q=3:
    // пеки d_k = max(-10, 2−3k): -1, -4, -7, -10.
    let p =
        parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG83 X20. Y20. Z-10. R2. Q3. F100.;\nM30;\n")
            .expect("ok");
    // G0 + traverse + approach + [feed, rapid(R), rapid(d+δ)] × 3 +
    // feed(-10) + retract(R) + PEND = 15.
    assert_eq!(p.ops.len(), 15);
    let seq = [
        -1.0f64, 2.0, -0.746, -4.0, 2.0, -3.746, -7.0, 2.0, -6.746, -10.0, 2.0,
    ];
    for (i, z) in seq.iter().enumerate() {
        let got = end_of(&p.ops[3 + i])[2];
        assert!((got - z).abs() <= EPS, "op{} z {got} vs {z}", 3 + i);
    }
    // Пики — feed; отводы/подходы — rapid.
    assert!(matches!(&p.ops[3], Op::Linear { .. }));
    assert!(matches!(&p.ops[4], Op::Rapid { .. }));
    assert!(
        matches!(&p.ops[12], Op::Linear { .. }),
        "последний пек — feed"
    );
    assert!(matches!(&p.ops[13], Op::Rapid { .. }), "retract");
}

#[test]
fn cycle_g73_chip_break() {
    // G73 (SPEC §7.4a v0.18 — зеркало D, probe g73_r2): депеки R−kQ вниз до
    // z_bottom (расписание как G83); после каждого промежуточного пека ОДИН
    // rapid на d+δ (δ = 0.254mm); финал на R (G99). Модели «de-retract
    // min(R, d+Q) + отдельный подход» в интерпретаторе НЕТ.
    // R=2, Z=-10, Q=2: d_k = max(-10, 2−2k): 0, -2, -4, -6, -8, -10.
    let p =
        parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG73 X20. Y20. Z-10. R2. Q2. F100.;\nM30;\n")
            .expect("ok");
    // G0 + traverse + approach + [feed(d_k), rapid(d+0.254)] × 5 + feed(-10)
    // + retract(R=2) + PEND = 16 (прежде 21 — −5 опов).
    assert_eq!(p.ops.len(), 16);
    assert_eq!(end_of(&p.ops[3]), [20.0, 20.0, 0.0], "первый пик d1=0");
    assert_eq!(
        end_of(&p.ops[4]),
        [20.0, 20.0, 0.254],
        "отвод d1+δ=0.254 (ОДИН ход)"
    );
    assert_eq!(end_of(&p.ops[5]), [20.0, 20.0, -2.0], "второй пик");
    assert_eq!(end_of(&p.ops[6]), [20.0, 20.0, -1.746], "отвод d2+δ");
    assert_eq!(
        end_of(&p.ops[13]),
        [20.0, 20.0, -10.0],
        "последний пик d6=z_bottom"
    );
    assert_eq!(end_of(&p.ops[14]), [20.0, 20.0, 2.0], "финал на R (G99)");
}

#[test]
fn cycle_g73_probe_structure_r2_q2() {
    // Зонд-структура 1:1 (probe g73_r2, бриф зеркала v0.18):
    // R2 Q2 Z−5 → R5,R2,F0,R0.254,F−2,R−1.746,F−4,R−3.746,F−5,R2.
    // (R5 = traverse на текущем Z, R2 = подход к R-плоскости.)
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG73 X10. Y10. Z-5. R2. Q2. F100.;\nM30;\n")
        .expect("ok");
    let seq = [5.0, 2.0, 0.0, 0.254, -2.0, -1.746, -4.0, -3.746, -5.0, 2.0];
    // G0 + 10 ходов развёртки + PEND.
    assert_eq!(p.ops.len(), seq.len() + 2, "10 ходов зонда + G0 + PEND");
    for (i, z) in seq.iter().enumerate() {
        let got = end_of(&p.ops[1 + i])[2];
        assert!((got - z).abs() <= EPS, "op{} z {got} vs {z}", 1 + i);
    }
}

#[test]
fn cycle_g85_bore_feed_out() {
    // G85: отвод на ПОДАЧЕ (linear, не rapid).
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG85 X20. Y20. Z-5. R2. F100.;\nM30;\n")
        .expect("ok");
    assert_eq!(p.ops.len(), 6);
    assert!(matches!(&p.ops[3], Op::Linear { .. }), "feed на дно");
    assert!(
        matches!(&p.ops[4], Op::Linear { .. }),
        "отвод на подаче — linear"
    );
    assert_eq!(end_of(&p.ops[4]), [20.0, 20.0, 2.0]);
}

#[test]
fn cycle_g86_spindle_restart() {
    // G86 (SPEC §7.4a v0.18 — зеркало D): P ОБЯЗАТЕЛЕН (rs274 exit 1 без
    // него); развёртка FEED → DWELL(P) → STOP_SPINDLE → RAPID-out → рестарт
    // (Q9) в state последующих опов.
    let p = parse(
        "G21 G90 G94 G17;\nS1000 M3;\nG0 X10. Y10. Z5.;\nG86 X20. Y20. Z-5. R2. P500 F100.;\nG1 X30. F200.;\nM30;\n",
    )
    .expect("ok");
    // ops: G0, traverse, approach, feed(Cw), DWELL(0.5s), rapid-out(Off),
    // linear(Cw), PEND = 8.
    assert_eq!(p.ops.len(), 8);
    match &p.ops[3] {
        Op::Linear { state, .. } => {
            assert!(
                matches!(state.spindle, eid_core::ast::Spindle::Cw(ref n) if (n.value() - 1000.0).abs() <= EPS)
            );
        }
        _ => panic!("op3 не Linear"),
    }
    match &p.ops[4] {
        Op::Dwell { seconds, state, .. } => {
            // bare P500 = 500 мс = 0.5 s; dwell ДО останова шпинделя (Cw).
            assert!((seconds.value() - 0.5).abs() <= EPS);
            assert!(matches!(state.spindle, eid_core::ast::Spindle::Cw(_)));
        }
        other => panic!("op4 не Dwell: {other:?}"),
    }
    match &p.ops[5] {
        Op::Rapid { state, .. } => {
            assert_eq!(
                state.spindle,
                eid_core::ast::Spindle::Off,
                "rapid-out при off"
            );
        }
        _ => panic!("op5 не Rapid"),
    }
    match &p.ops[6] {
        Op::Linear { state, .. } => {
            assert!(
                matches!(state.spindle, eid_core::ast::Spindle::Cw(ref n) if (n.value() - 1000.0).abs() <= EPS),
                "рестарт в исходном направлении+оборотах (Q9)"
            );
        }
        _ => panic!("op6 не Linear"),
    }
    // G86 без P — отказ (rs274-сообщение зеркалируется дословно).
    let Err(d) =
        parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG86 X20. Y20. Z-5. R2. F100.;\nM30;\n")
    else {
        panic!("ожидался Err");
    };
    assert_eq!(d[0].code, ErrorCode::InvalidInput);
    assert_eq!(d[0].message, "Dwell time p word missing with g86");
}

#[test]
fn cycle_g89_bore_dwell() {
    // G89: dwell + отвод на подаче.
    let p =
        parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG89 X20. Y20. Z-5. R2. P100 F100.;\nM30;\n")
            .expect("ok");
    assert_eq!(p.ops.len(), 7); // G0 + traverse/approach/feed/dwell/feed-out + PEND
    assert!(matches!(&p.ops[4], Op::Dwell { .. }));
    assert!(matches!(&p.ops[5], Op::Linear { .. }), "feed-out");
    // G89 без P — сломанный вход.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG89 X20. Y20. Z-5. R2. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn cycle_raw_family() {
    // G84/G74/G76/G87/G88 → Raw{SEMANTICS} (синхронизация/ориентация/ручное;
    // G76 — двойной отказ: M19-класс + параметр 5101; COVERAGE §2.5).
    for (code, marker) in [
        ("G84", "G84"),
        ("G74", "G74"),
        ("G76", "5101"),
        ("G87", "G87"),
        ("G88", "manual"),
    ] {
        let out = canonicalize_with_notices(
            &format!(
                "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\n{code} X20. Y20. Z-5. R2. F100.;\nM30;\n"
            ),
            "t.ngc",
        )
        .expect("Ok + Raw");
        assert!(
            matches!(
                &out.program.ops[1],
                Op::Raw {
                    code: ErrorCode::UnsupportedSemantics,
                    ..
                }
            ),
            "{code} → Raw SEMANTICS"
        );
        assert!(raw_reason(&out.program, 1).contains(marker), "{code}");
    }
}

#[test]
fn cycle_k_repeat_g91_step() {
    // G91 + K: X/Y-слова — шаг, K — число отверстий; R — инкремент от
    // initial Z (каждая итерация r_cur += R); Z — инкремент от R.
    let p = parse(
        "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG91;\nG81 X5. Y2. Z-15. R-3. K3. F100.;\nG90;\nM30;\n",
    )
    .expect("ok");
    // 3 итерации × 4 опа = 12 + G0 + PEND = 14.
    assert_eq!(p.ops.len(), 14);
    // Итерация 1: r_cur = 5 + (−3) = 2; zb = 2 − 15 = −13; xy = (15, 12).
    assert_eq!(end_of(&p.ops[1]), [15.0, 12.0, 5.0], "traverse iter1");
    assert_eq!(end_of(&p.ops[2]), [15.0, 12.0, 2.0], "approach iter1");
    assert_eq!(end_of(&p.ops[3]), [15.0, 12.0, -13.0], "дно iter1");
    assert_eq!(end_of(&p.ops[4]), [15.0, 12.0, 2.0], "retract iter1");
    // Итерация 2: r_cur = −1; zb = −16; xy = (20, 14).
    assert_eq!(
        end_of(&p.ops[5]),
        [20.0, 14.0, 2.0],
        "traverse iter2 на текущем Z"
    );
    assert_eq!(end_of(&p.ops[6]), [20.0, 14.0, -1.0], "approach iter2");
    assert_eq!(end_of(&p.ops[7]), [20.0, 14.0, -16.0], "дно iter2");
    // Итерация 3: r_cur = −4; zb = −19; xy = (25, 16).
    assert_eq!(end_of(&p.ops[9]), [25.0, 16.0, -1.0], "traverse iter3");
    assert_eq!(end_of(&p.ops[10]), [25.0, 16.0, -4.0], "approach iter3");
    assert_eq!(end_of(&p.ops[11]), [25.0, 16.0, -19.0], "дно iter3");
}

#[test]
fn cycle_k0_g91_raw() {
    // K0/L0 в G91 — спец-семантика «движение по одной оси» → Raw{SEMANTICS}
    // (§5.2; COVERAGE §2.5; `?` реестр §6.17; разворачивание не делается).
    for w in ["K0", "L0"] {
        let out = canonicalize_with_notices(
            &format!("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG91;\nG81 X5. Y2. Z-15. R-3. {w} F100.;\nG90;\nM30;\n"),
            "t.ngc",
        )
        .expect("Ok + Raw");
        assert!(
            matches!(
                &out.program.ops[1],
                Op::Raw {
                    code: ErrorCode::UnsupportedSemantics,
                    ..
                }
            ),
            "{w} in G91 → Raw"
        );
    }
    // K0 в G90 — легально: 0 повторов (детерминированный no-op, паритет
    // LinuxCNC L0). RULING оркестратора (этап 3, п.3): K0-in-G90 = no-op
    // ПРИНЯТ как паритет L0; COVERAGE `?`-реестр пополняет оркестратор.
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG81 X5. Y2. Z-15. R-3. K0 F100.;\nM30;\n")
        .expect("K0 в G90 легален");
    assert_eq!(p.ops.len(), 2); // G0 + PEND
}

#[test]
fn cycle_first_motion_invalid() {
    // §7.6: цикл как первый motion-оп — позиция не определена.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG81 X5. Y5. Z-5. R2. F100.;\nM30;\n"
        )),
        ErrorCode::RuntimeState
    );
    // После позиционно-неопределённого участка (разрыв (b)) — тоже отказ.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG55;\nG81 X5. Y5. Z-5. R2. F100.;\nM30;\n"
        )),
        ErrorCode::RuntimeState
    );
}

#[test]
fn cycle_g98_retract_initial() {
    // G98 — отвод в initial Z (требует known-position §7.6); G99 → R — дефолт
    // §7.5a (покрыт в cycle_g81_unroll_template).
    let p =
        parse("G21 G90 G94 G17;\nG98;\nG0 X10. Y10. Z5.;\nG81 X20. Y20. Z-5. R2. F100.;\nM30;\n")
            .expect("ok");
    assert_eq!(end_of(&p.ops[4]), [20.0, 20.0, 5.0], "retract в initial Z");
}

#[test]
fn cycle_modal_repeat_xy() {
    // Модальность группы 09: G81-блок без X/Y — параметр-кадр (без
    // сверления, §5.1); X/Y-блоки без G8x — повтор; Z обновляется.
    let p = parse(
        "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG81 Z-5. R2. F100.;\nX20.;\nX30. Y-10. Z-6.;\nG80;\nG1 X40. F250.;\nM30;\n",
    )
    .expect("ok");
    // G0; отверстие1 × 4; отверстие2 × 3 (approach — ZERO-LENGTH: cz == 2 ==
    // r_cur после отвода отв.1 → drop+notice, этап 3/SPEC §3); linear; PEND
    // = 10 (G81-параметр-кадр без опов).
    assert_eq!(p.ops.len(), 10);
    // Отверстие 1 (X20): traverse (10,10,5)→(20,10,5).
    assert_eq!(end_of(&p.ops[1]), [20.0, 10.0, 5.0]);
    assert_eq!(end_of(&p.ops[3]), [20.0, 10.0, -5.0]);
    // Отверстие 2 (X30 Y-10, Z обновлён −6).
    assert_eq!(
        end_of(&p.ops[5]),
        [30.0, -10.0, 2.0],
        "traverse на retract Z"
    );
    assert_eq!(end_of(&p.ops[6]), [30.0, -10.0, -6.0], "обновлённый Z");
    assert_eq!(end_of(&p.ops[7]), [30.0, -10.0, 2.0], "retract");
    // После G80 X/Y = линейный ход (группа 01 модальна, цикл отменён).
    assert!(matches!(&p.ops[8], Op::Linear { .. }));
    assert_eq!(end_of(&p.ops[8]), [40.0, -10.0, 2.0]);
}

#[test]
fn cycle_g91_z_step_repeat() {
    // §5.1: в G91 модальный повтор триггерится X/Y/Z-словом (Z-шаг глубины
    // между отверстиями); в G90 Z-only блок — обновление параметров.
    let p = parse(
        "G21 G90 G94 G17;
G0 X10. Y10. Z5.;
G91;
G81 X0. Y0. Z-5. R-3. F100.;
Z-5.;
G90;
M30;
",
    )
    .expect("ok");
    // G91: X0 Y0 → traverse нулевой (hole на той же X/Y) → drop+notice
    // (этап 3/SPEC §3); на отверстие: approach + drill + retract × 2 + G0 +
    // PEND = 8; второе — глубже на Z-шаг.
    assert_eq!(p.ops.len(), 8);
    // iter1: r_cur = 2, zb = −3; iter2: r_cur = −1, zb = −6.
    assert_eq!(end_of(&p.ops[1]), [10.0, 10.0, 2.0], "approach iter1");
    assert_eq!(end_of(&p.ops[2]), [10.0, 10.0, -3.0], "дно iter1");
    assert_eq!(end_of(&p.ops[3]), [10.0, 10.0, 2.0], "retract iter1");
    assert_eq!(
        end_of(&p.ops[4]),
        [10.0, 10.0, -1.0],
        "approach iter2 (r_cur −1)"
    );
    assert_eq!(end_of(&p.ops[5]), [10.0, 10.0, -6.0], "дно iter2 (Z-шаг)");
}

#[test]
fn cycle_g0_cancels() {
    // Fanuc: явный код группы 01 отменяет активный цикл.
    let p = parse(
        "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG81 X20. Y20. Z-5. R2. F100.;\nG0 Z10.;\nX30.;\nM30;\n",
    )
    .expect("ok");
    // ops: G0, 4×цикл, G0 Z10, Rapid X30 (не повтор!), PEND = 8.
    assert_eq!(p.ops.len(), 8);
    assert!(matches!(&p.ops[6], Op::Rapid { .. }));
    assert_eq!(
        end_of(&p.ops[6]),
        [30.0, 20.0, 10.0],
        "G0 Z10 сохраняет X/Y отверстия"
    );
}

#[test]
fn cycle_g80_ignored() {
    // G80 — IGNORED_SAFE (COVERAGE §2.1); «голая» строка после G80 без
    // motion-моды — INVALID.
    let (p, notices) = notices_of(
        "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG81 X20. Y20. Z-5. R2. F100.;\nG80;\nM30;\n",
    );
    assert!(notices.is_empty());
    assert!(!p.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
    assert_eq!(
        err_code(parse("G80;\nX5. Y5.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn cycle_missing_r_invalid() {
    // R/Z обязательны; Q пекерующих > 0; F модальна (сломанный вход).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG81 X20. Y20. Z-5. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG81 X20. Y20. R2. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG83 X20. Y20. Z-10. R2. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG83 X20. Y20. Z-10. R2. Q0. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG81 X20. Y20. Z-5. R2.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn cycle_unroll_limit() {
    // COVERAGE §1.2 max_unroll_iterations = 10 000: пек-оценка > лимита —
    // E_LIMIT_EXCEEDED (guard транслятора; Q=0.001 × ход 12мм → 12 000 пиков).
    let d =
        parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG83 X20. Y20. Z-10. R2. Q0.001 F100.;\nM30;\n")
            .unwrap_err();
    assert_eq!(d[0].code, ErrorCode::LimitExceeded);
    assert_eq!(
        d[0].limit.as_ref().map(|l| l.threshold),
        Some("max_unroll_iterations")
    );
}

// ===========================================================================
// ЭТАП 2c — M-коды
// ===========================================================================

#[test]
fn m0_m1_program_stop() {
    let p =
        parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM0;\nM1;\nG1 X5. F100.;\nM30;\n").expect("ok");
    // ops: Rapid, Stop(M0), Stop(M1), Linear, PEND.
    assert_eq!(p.ops.len(), 5);
    match &p.ops[1] {
        Op::ProgramStop {
            optional, state, ..
        } => {
            assert!(!optional, "M0 — без optional");
            assert_eq!(state.wcs, eid_core::ast::Wcs::G54);
        }
        _ => panic!("op1 не ProgramStop"),
    }
    match &p.ops[2] {
        Op::ProgramStop { optional, .. } => assert!(optional, "M1 — optional (Q8)"),
        _ => panic!("op2 не ProgramStop"),
    }
}

#[test]
fn m3_m4_m5_spindle_state() {
    // M3/M4/M5 + S → state.spindle (ABSORB, без опов).
    let p = parse(
        "G21 G90 G94 G17;\nS1000 M3;\nG0 X1. Y1. Z1.;\nM4;\nG0 X2. Y2. Z2.;\nM5;\nG0 X3. Y3. Z3.;\nM30;\n",
    )
    .expect("ok");
    // ops: 3 Rapid + PEND (M-кадры без осей — ABSORB, опов не дают).
    assert!(
        matches!(spindle_of(&p.ops[0]).unwrap(), eid_core::ast::Spindle::Cw(ref n) if (n.value() - 1000.0).abs() <= EPS)
    );
    assert!(
        matches!(
            spindle_of(&p.ops[1]).unwrap(),
            eid_core::ast::Spindle::Ccw(_)
        ),
        "M4"
    );
    assert!(
        matches!(spindle_of(&p.ops[2]).unwrap(), eid_core::ast::Spindle::Off),
        "M5"
    );
}

#[test]
fn m_bare_s_persists() {
    // Голое S — сохранение оборотов (§3 отчёта): M3 после S2000 → Cw(2000).
    let p = parse("G21 G90 G94 G17;\nS2000;\nM3;\nG0 X1. Y1. Z1.;\nM30;\n").expect("ok");
    assert!(
        matches!(spindle_of(&p.ops[0]).unwrap(), eid_core::ast::Spindle::Cw(ref n) if (n.value() - 2000.0).abs() <= EPS)
    );
    // S при работающем шпинделе — обновление оборотов.
    let p = parse("G21 G90 G94 G17;\nS1000 M3;\nS1500;\nG0 X1. Y1. Z1.;\nM30;\n").expect("ok");
    assert!(
        matches!(spindle_of(&p.ops[0]).unwrap(), eid_core::ast::Spindle::Cw(ref n) if (n.value() - 1500.0).abs() <= EPS)
    );
}

#[test]
fn m6_tool_change() {
    // Fanuc-идиома `T05 M06` → ToolChange (state.tool установлен).
    let p = parse("G21 G90 G94 G17;\nT5 M6;\nG0 X1. Y1. Z1.;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::ToolChange { tool, state, .. } => {
            assert_eq!(*tool, 5);
            assert_eq!(state.tool, Some(5));
        }
        _ => panic!("op0 не ToolChange"),
    }
    // Повторная смена видна в следующем опе.
    let p = parse("G21 G90 G94 G17;\nT5 M6;\nT7 M6;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::ToolChange { tool, .. } => assert_eq!(*tool, 7),
        _ => panic!("op1 не ToolChange"),
    }
}

#[test]
fn m6_without_t_invalid() {
    assert_eq!(
        err_code(parse("G21 G90 G94 G17;\nM6;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn t_alone_preload_raw() {
    // Одиночное T — предвыбор (modal next-tool) → Raw{SEMANTICS}
    // (COVERAGE §3; кандидат `selected_tool`).
    let out = canonicalize_with_notices("T5;\nG21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n", "t.ngc")
        .expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    assert!(raw_reason(&out.program, 0).contains("preload"));
    // H/D-слова вне G43/G41-кадров — значение из таблицы станка (COVERAGE §4).
    let out = canonicalize_with_notices("H2;\nM30;\n", "h.ngc").expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
}

#[test]
fn m7_m8_m9_coolant() {
    let p = parse(
        "G21 G90 G94 G17;\nM8;\nG0 X1. Y1. Z1.;\nM7;\nG0 X2. Y2. Z2.;\nM9;\nG0 X3. Y3. Z3.;\nM30;\n",
    )
    .expect("ok");
    let c = match &p.ops[0] {
        Op::Rapid { state, .. } => state.coolant,
        _ => panic!(),
    };
    assert!(c.flood && !c.mist, "M8 — flood");
    let c = match &p.ops[1] {
        Op::Rapid { state, .. } => state.coolant,
        _ => panic!(),
    };
    assert!(c.flood && c.mist, "M7 (mist) поверх M8");
    let c = match &p.ops[2] {
        Op::Rapid { state, .. } => state.coolant,
        _ => panic!(),
    };
    assert!(!c.flood && !c.mist, "M9 — всё off");
}

#[test]
fn m19_m29_raw() {
    // M19 (ориентация, энкодер) / M29 (rigid tap) → Raw{SEMANTICS}.
    for (code, marker) in [("M19", "orientation"), ("M29", "rigid")] {
        let out = canonicalize_with_notices(&format!("{code};\nM30;\n"), "t.ngc").expect("ok");
        assert!(
            matches!(
                &out.program.ops[0],
                Op::Raw {
                    code: ErrorCode::UnsupportedSemantics,
                    ..
                }
            ),
            "{code}"
        );
        assert!(raw_reason(&out.program, 0).contains(marker));
    }
}

#[test]
fn m98_m99_m198_raw() {
    // Подпрограммы → Raw{SYNTAX} (Macro B-механика; Q10 — ревизия по корпусу).
    for code in ["M98 P0010;", "M99;", "M198 P1234;"] {
        let out = canonicalize_with_notices(&format!("{code}\nM30;\n"), "t.ngc").expect("ok");
        assert!(
            matches!(
                &out.program.ops[0],
                Op::Raw {
                    code: ErrorCode::UnsupportedSyntax,
                    ..
                }
            ),
            "{code}"
        );
    }
}

#[test]
fn unknown_m_generic_raw() {
    // Builder-specific M (M64 и пр.) → generic-ведро Raw{SYNTAX} + текст кода
    // («не молча», COVERAGE §3).
    let out = canonicalize_with_notices("M64;\nM30;\n", "t.ngc").expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    assert!(raw_reason(&out.program, 0).contains("M64"));
    // Неизвестный G — тот же generic-ведро.
    let out = canonicalize_with_notices("G88.8;\nM30;\n", "t.ngc").expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    assert!(raw_reason(&out.program, 0).contains("G88.8"));
}

// ===========================================================================
// ЭТАП 2d — Macro B (line-level Raw{SYNTAX}; COVERAGE §4)
// ===========================================================================

#[test]
fn macro_b_hash_raw() {
    // `#`-переменная/присваивание → line-level Raw{SYNTAX}; лексика макро
    // (`=`, `+`) не роняет лексер.
    let out = canonicalize_with_notices("#100 = #100 + 1;\nM30;\n", "t.ngc").expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    assert!(raw_reason(&out.program, 0).contains("Macro B"));
}

#[test]
fn macro_b_statements_raw() {
    // IF/GOTO/THEN/WHILE/DO/END → Raw; соседние статические ходы парсятся
    // как обычно (правило двух классов; точки разрыва → unknown).
    let p = parse(
        "IF [#1 GT 5] GOTO 10;\nG21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nWHILE [#100 LT 10] DO 1;\nG1 X5. F100.;\nEND 1;\nM30;\n",
    )
    .expect("Ok + Raw-строки");
    // ops: Raw(IF), Rapid, Raw(WHILE), Linear, Raw(END), PEND — порядок
    // кадров источника; статические ходы парсятся сквозь Raw-строки.
    assert_eq!(p.ops.len(), 6);
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    assert!(matches!(&p.ops[1], Op::Rapid { .. }));
    assert!(matches!(&p.ops[2], Op::Raw { .. }), "WHILE");
    assert!(matches!(&p.ops[3], Op::Linear { .. }));
    assert!(matches!(&p.ops[4], Op::Raw { .. }), "END");
}

#[test]
fn macro_b_g65_family_raw() {
    // G65/G66/G66.1/G67 → Raw{SYNTAX} (вызовы макросов — Macro B).
    for code in ["G65 P1000 L2;", "G66 P1000;", "G66.1 X1. Y1.;", "G67;"] {
        let out = canonicalize_with_notices(&format!("{code}\nM30;\n"), "t.ngc").expect("ok");
        assert!(
            matches!(
                &out.program.ops[0],
                Op::Raw {
                    code: ErrorCode::UnsupportedSyntax,
                    ..
                }
            ),
            "{code}"
        );
    }
}

#[test]
fn d9_round_trip_raw_fragments() {
    // Ruling 5 (golden): `G43 H2; T5;` на одной строке → 2 Raw-опа;
    // конкатенация payload'ов + терминатор = исходная строка байт-в-байт.
    let src = "G43 H2; T5;\n";
    let out = canonicalize_with_notices(src, "t.ngc").expect("ok");
    // 2 Raw-опа + auto-PEND (нет M30 в golden-строке ruling 5).
    assert_eq!(out.program.ops.len(), 3);
    let mut recon = Vec::new();
    for op in &out.program.ops {
        match op {
            Op::Raw { payload_hex, .. } => recon.extend(hex_decode(payload_hex)),
            Op::ProgramEnd { .. } => {}
            other => panic!("ожидался Raw/PEND: {other:?}"),
        }
    }
    assert_eq!(String::from_utf8(recon).unwrap(), src);
    // CRLF-файл: терминатор \r\n сохранён в payload последнего фрагмента.
    let src = "G43 H2; T5;\r\n";
    let out = canonicalize_with_notices(src, "t.ngc").expect("ok");
    let mut recon = Vec::new();
    for op in &out.program.ops {
        if let Op::Raw { payload_hex, .. } = op {
            recon.extend(hex_decode(payload_hex));
        }
    }
    assert_eq!(String::from_utf8(recon).unwrap(), src);
}

#[test]
fn d9_round_trip_mixed_line() {
    // Смешанная строка: Raw-фрагмент сохраняет ТОЧНО свои байты (`G43 H2;`),
    // соседний оп — линейный ход (ruling 7: split-parse запрещён —
    // координаты Raw-кадра не декомпозируются).
    let src = "G43 H2; G1 X5. F100.;\n";
    let out = canonicalize_with_notices(src, "t.ngc").expect("ok");
    match &out.program.ops[0] {
        Op::Raw {
            payload_hex, code, ..
        } => {
            assert_eq!(*code, ErrorCode::RequiresMachineTable);
            assert_eq!(
                String::from_utf8(hex_decode(payload_hex)).unwrap(),
                "G43 H2;"
            );
        }
        other => panic!("op0 не Raw: {other:?}"),
    }
    assert!(matches!(&out.program.ops[1], Op::Linear { .. }));
}

// ===========================================================================
// ЭТАП 2e — Компенсации (семейства COVERAGE §1.3, Fanuc-составы)
// ===========================================================================

#[test]
fn g43_g44_raw_requires() {
    // G43/G44 (+G43.1) — TLO из таблицы станка → Raw{REQUIRES}.
    for code in ["G43 H2;", "G44 H3;", "G43.1 Z5.;"] {
        let out = canonicalize_with_notices(&format!("{code}\nM30;\n"), "t.ngc").expect("ok");
        assert!(
            matches!(
                &out.program.ops[0],
                Op::Raw {
                    code: ErrorCode::RequiresMachineTable,
                    ..
                }
            ),
            "{code}"
        );
    }
    // G43.4/.5 — TCP (5-осевое) → Raw{SEMANTICS}.
    for code in ["G43.4;", "G43.5;"] {
        let out = canonicalize_with_notices(&format!("{code}\nM30;\n"), "t.ngc").expect("ok");
        assert!(
            matches!(
                &out.program.ops[0],
                Op::Raw {
                    code: ErrorCode::UnsupportedSemantics,
                    ..
                }
            ),
            "{code}"
        );
    }
}

#[test]
fn g41_g42_raw_semantics() {
    // G41/G42 (+D) и семья (G38/G39/G41.1) → Raw{SEMANTICS}.
    for code in ["G41 D5;", "G42 D5;", "G41.1 X1. Y1.;", "G38;", "G39;"] {
        let out = canonicalize_with_notices(&format!("{code}\nM30;\n"), "t.ngc").expect("ok");
        assert!(
            matches!(
                &out.program.ops[0],
                Op::Raw {
                    code: ErrorCode::UnsupportedSemantics,
                    ..
                }
            ),
            "{code}"
        );
    }
}

#[test]
fn g40_cancel_family_rule() {
    // Правило семейств §1.3: G40 без активных кодов семьи — IGNORED_SAFE;
    // с G41 в файле — Raw{SEMANTICS} (raw-наследование).
    let (p, notices) = notices_of("G21 G90 G94 G17;\nG40;\nG0 X1. Y1. Z1.;\nM30;\n");
    assert!(!p.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
    assert!(notices.is_empty());
    let p = parse("G21 G90 G94 G17;\nG41 D5;\nG40;\nG0 X1. Y1. Z1.;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    assert!(
        matches!(
            &p.ops[1],
            Op::Raw {
                code: ErrorCode::UnsupportedSemantics,
                ..
            }
        ),
        "G40 → наследование семьи"
    );
}

#[test]
fn g49_cancel_family_rule() {
    // Семья G49 Fanuc-состав {G43, G43.1, G43.4, G43.5, G44}: G49 без семьи —
    // IGNORED_SAFE; с G43 — Raw{REQUIRES}.
    let (p, notices) = notices_of("G21 G90 G94 G17;\nG49;\nG0 X1. Y1. Z1.;\nM30;\n");
    assert!(!p.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
    assert!(notices.is_empty());
    let p = parse("G21 G90 G94 G17;\nG43 H2;\nG49;\nG0 X1. Y1. Z1.;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
    assert!(
        matches!(
            &p.ops[1],
            Op::Raw {
                code: ErrorCode::RequiresMachineTable,
                ..
            }
        ),
        "G49 → наследование семьи"
    );
}

#[test]
fn h00_cancel_ignored() {
    // H00 = cancel TLO (COVERAGE §2.4) — легальное поглощение без G43-семьи.
    let (p, notices) = notices_of("G21 G90 G94 G17;\nH00;\nG0 X1. Y1. Z1.;\nM30;\n");
    assert!(!p.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
    assert!(notices.is_empty());
}

// ===========================================================================
// ЭТАП 2f — G93 ABSORB (ruling 4)
// ===========================================================================

#[test]
fn g93_linear_feed_len_f() {
    // G93 inverse-time: feed = len × F (SPEC §4); эмиссия всегда G94.
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG93;\nG1 X30. F2.;\nM30;\n").expect("ok");
    // len = 30, F = 2 1/min → feed = 60 mm/min.
    assert_eq!(feed_of(&p.ops[1]), Some(60.0));
    // G94 возвращает обычную семантику.
    let p = parse(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG93;\nG1 X30. F2.;\nG94;\nG1 X10. F120.;\nM30;\n",
    )
    .expect("ok");
    // ops: Rapid, Linear(G93), Linear(G94), PEND.
    assert_eq!(feed_of(&p.ops[2]), Some(120.0));
    // G93 с неизвестным стартом — E_RUNTIME_STATE (разрешение требует len).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17;\nG93;\nG1 X30. F2.;\nM30;\n")),
        ErrorCode::RuntimeState
    );
}

#[test]
fn g93_arc_feed_len_f() {
    // G93 дуга: len = sqrt((sweep·ρ)² + pitch²) (SPEC §4/§6).
    // Четверть R=5: len = π/2·5 ≈ 7.85398; F=4 → feed ≈ 31.4159.
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG93;\nG2 X-5. Y5. I0. J5. F4.;\nM30;\n")
        .expect("ok");
    let f = feed_of(&p.ops[1]).expect("feed");
    assert!((f - 31.4159).abs() <= 0.001, "feed {f}");
    // Винтовая: pitch добавляется квадратично (полный оборот R=5, pitch 4).
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG93;\nG2 X0. Y0. Z4. I-5. F4.;\nM30;\n")
        .expect("ok");
    let f = feed_of(&p.ops[1]).expect("feed");
    let expected = 4.0 * ((2.0 * std::f64::consts::PI * 5.0).powi(2) + 16.0f64).sqrt();
    assert!((f - expected).abs() <= 0.01, "feed {f} vs {expected}");
}

// ===========================================================================
// G4 dwell (P — целые мс; X/U — секунды; §1.2/§2.7)
// ===========================================================================

#[test]
fn g4_p_ms_conversion() {
    // `G04 P500` = 0.5 s (P — целые мс, decimal point запрещён Fanuc).
    let p =
        parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG4 P500;\nG1 X5. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Dwell { seconds, .. } => {
            assert!((seconds.value() - 0.5).abs() <= EPS, "{}", seconds.value());
            assert_eq!(seconds.text, "0.5");
        }
        _ => panic!("op1 не Dwell"),
    }
}

#[test]
fn g4_x_u_seconds() {
    // X/U-форма — секунды float (X/U — дробный домен: bare X5 → 0.005 s по
    // DPI-политике, notice сохраняется).
    let p = parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG4 X2.;\nG4 U1.5;\nM30;\n").expect("ok");
    assert!(matches!(&p.ops[1], Op::Dwell { seconds, .. } if (seconds.value() - 2.0).abs() <= EPS));
    assert!(matches!(&p.ops[2], Op::Dwell { seconds, .. } if (seconds.value() - 1.5).abs() <= EPS));
    let (p, notices) = notices_of("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG4 X5;\nM30;\n");
    assert!(
        matches!(&p.ops[1], Op::Dwell { seconds, .. } if (seconds.value() - 0.005).abs() <= EPS)
    );
    assert!(notices
        .iter()
        .any(|n| n.kind == NoticeKind::DpiAssumed && n.message.contains("X5")));
    // Несколько dwell-слов / ни одного / отрицательный — сломанный вход.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG4 P100. X1.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG4;\nM30;\n")),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG4 P-100;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn g4_u_exception_elsewhere_raw() {
    // Исключение §4.1: U в G04 = dwell-секунды; U вне G4 — Raw{SEMANTICS}
    // (вне-скоуп ось, этап 12).
    let out = canonicalize_with_notices("U1.;\nM30;\n", "t.ngc").expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
}

#[test]
fn g4_p0_drop_notice() {
    // `G04 P0` — no-op → drop + notice (SPEC §5.2 dwell=0).
    let (p, notices) =
        notices_of("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG4 P0;\nG1 X5. F100.;\nM30;\n");
    assert_eq!(p.ops.len(), 3); // Rapid, Linear, PEND (Dwell отброшен)
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, NoticeKind::ZeroLengthDwell);
}

// ===========================================================================
// Path META (группа 13; Fanuc value shift — G61 = exact_stop)
// ===========================================================================

#[test]
fn g61_exact_stop_value_shift() {
    // Fanuc G61 = exact STOP (останов в углу), НЕ exact_path как LinuxCNC
    // (сдвиг значения; COVERAGE §2.2).
    let out = canonicalize_with_notices("G61;\nM30;\n", "t.ngc").expect("ok");
    assert_eq!(out.program.path_mode, eid_core::ast::PathMode::ExactStop);
    let out = canonicalize_with_notices("G61.1;\nM30;\n", "t.ngc").expect("ok");
    assert_eq!(out.program.path_mode, eid_core::ast::PathMode::ExactStop);
}

#[test]
fn g64_blend_bare() {
    // Fanuc G64 без P-квантификатора → bare blend.
    let out = canonicalize_with_notices("G64;\nM30;\n", "t.ngc").expect("ok");
    assert_eq!(
        out.program.path_mode,
        eid_core::ast::PathMode::Blend { t: None }
    );
    // P с G64 — без потребителя → сломанный вход (Fanuc alarm).
    assert_eq!(
        err_code(parse("G64 P100.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn path_switch_mid_invalid() {
    // path_mode глобален (SPEC §3): G61 → G64 в середине — отказ.
    assert_eq!(
        err_code(parse(
            "G61;\nG21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG64;\nG0 X2. Y2. Z2.;\nM30;\n"
        )),
        ErrorCode::UnsupportedSemantics
    );
}

// ===========================================================================
// Этап 3 (зеркала SPEC v0.17 + rulings оркестратора)
// ===========================================================================

/// Хелпер: end-точка опа → [f64; 3].
fn end_of3(op: &Op) -> [f64; 3] {
    match op {
        Op::Rapid { end, .. } | Op::Linear { end, .. } | Op::Arc { end, .. } => pt3(end),
        other => panic!("нет end у {other:?}"),
    }
}

#[test]
fn g38_probing_family_raw() {
    // RULING (этап 3, п.2): G38.2–.5 — пробинг, класс = G31/G37
    // (COVERAGE §2.1: «пробинг — этап 12») → Raw{E_UNSUPPORTED_SEMANTICS}.
    for code in ["G38.2", "G38.3", "G38.4", "G38.5"] {
        let out = canonicalize_with_notices(
            &format!("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\n{code} Z-1. F100.;\nM30;\n"),
            "t.ngc",
        )
        .expect("Ok + Raw");
        assert!(
            matches!(
                &out.program.ops[1],
                Op::Raw {
                    code: ErrorCode::UnsupportedSemantics,
                    ..
                }
            ),
            "{code} → Raw SEMANTICS (probing)"
        );
    }
}

#[test]
fn full_circle_explicit_end_words_360() {
    // ЗЕРКАЛО SPEC §3 v0.17 (аудит #15, зонд rs274): center-форма с
    // center ≠ start при start == end = полный круг 360° НЕЗАВИСИМО от
    // явных end-слов. Проб: `G2 X10. Y0. I-5. J0` из (10,0) → sweep=360
    // (критерий «форма команды» v0.13 отбросил бы дугу — ошибка).
    let (p, notices) =
        notices_of("G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG2 X10. Y0. I-5. J0. F100.;\nM30;\n");
    assert!(
        notices.iter().all(|n| n.kind != NoticeKind::ZeroLengthArc),
        "полное кольцо с явными end-словами — НЕ zero-length drop"
    );
    match &p.ops[1] {
        Op::Arc {
            end,
            center,
            sweep_deg,
            pitch,
            ..
        } => {
            assert_pt(end, 10.0, 0.0, 0.0);
            assert_pt(center, 5.0, 0.0, 0.0);
            assert!(
                (sweep_deg.value() - 360.0).abs() <= EPS,
                "sweep {}",
                sweep_deg.value()
            );
            assert!(pitch.value().abs() <= EPS);
        }
        _ => panic!("оп не Arc"),
    }
    // Helical-вариант с явным end-словом вне плоскости: 360° + pitch.
    let (p, notices) =
        notices_of("G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG2 X10. Y0. Z5. I-5. J0. F100.;\nM30;\n");
    assert!(notices.iter().all(|n| n.kind != NoticeKind::ZeroLengthArc));
    match &p.ops[1] {
        Op::Arc {
            sweep_deg, pitch, ..
        } => {
            assert!((sweep_deg.value() - 360.0).abs() <= EPS);
            assert!((pitch.value() - 5.0).abs() <= EPS);
        }
        _ => panic!("оп не Arc"),
    }
}

#[test]
fn zero_length_arc_degenerate_center_still_dropped() {
    // SPEC §3 v0.17, вторая половина правила: end-слова, квантованные в
    // старт, при center == start (вырожденное кольцо радиуса 0) → drop.
    let (p, notices) =
        notices_of("G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG2 X10. Y0. I0. J0. F100.;\nM30;\n");
    assert!(
        notices.iter().any(|n| n.kind == NoticeKind::ZeroLengthArc),
        "вырожденный центр (I0 J0) при end==start — zero-length drop"
    );
    assert_eq!(p.ops.len(), 2); // Rapid + PEND (дуга отброшена)
}

#[test]
fn modal_absorption_g91_g28_incremental_next() {
    // ЗЕРКАЛО SPEC §8.6 правило 6 (v0.17, критический аудит #33): пробой —
    // `G91 G28 Z0` терял G91 (raw-возврат ДО абсорбции) → следующий
    // `G0 X10 Y10` молча АБСОЛЮТНЫЙ. Теперь G91 абсорбируется до push_raw:
    // следующий ход инкрементальный: (20,20,10) + (10,10,0) = (30,30,10).
    let p = parse("G21 G90 G94 G17;\nG0 X20. Y20. Z10.;\nG91 G28 Z0;\nG0 X10. Y10.;\nM30;\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        } => {}
        other => panic!("ops[1] не Raw(G28): {other:?}"),
    }
    match &p.ops[2] {
        Op::Rapid { .. } => {
            assert_eq!(
                known(&p.ops[2]),
                [20.0, 20.0, 10.0],
                "связывание с последнего хода"
            );
            assert_eq!(
                end_of3(&p.ops[2]),
                [30.0, 30.0, 10.0],
                "инкрементальная интерпретация"
            );
        }
        other => panic!("ops[2] не Rapid: {other:?}"),
    }
}

#[test]
fn modal_on_raw_motion_not_continued_notice() {
    // SPEC §8.6 (v0.17): raw-motion-G (G84…) с осями — модальное состояние
    // абсорбируется, motion-режим НЕ продолжается + notice W_MODAL_ON_RAW.
    let (_, notices) =
        notices_of("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG84 X20. Y10. Z-15. R3. F100.;\nM30;\n");
    assert!(
        notices.iter().any(|n| n.kind == NoticeKind::ModalOnRaw),
        "нет W_MODAL_ON_RAW на raw-motion-кадре"
    );
    // Явный G0 ПОСЛЕ raw-motion-G — легален (режим переустановлен),
    // старт связывается с последнего motion-опа.
    let p = parse(
        "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG84 X20. Y10. Z-15. R3. F100.;\nG0 X30. Y30.;\nM30;\n",
    )
    .expect("ok");
    assert_eq!(known(&p.ops[2]), [10.0, 10.0, 5.0]);
    // Голые координаты ПОСЛЕ raw-motion-G без переустановки режима —
    // честный отказ (режим не продолжается молча).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG84 X20. Y10. Z-15. R3. F100.;\nX30. Y30.;\nM30;\n",
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn modal_absorption_units_wcs_feed_on_raw() {
    // SPEC §8.6: units/wcs/feed/coolant абсорбируются и на raw-кадрах.
    // (a) G20 на raw-кадре БЕЗ предыдущего units-слова — устанавливает inch
    // (SPEC §2.5: первое вхождение устанавливает; дефолт mm — не «уже установлено»).
    let out = canonicalize_with_notices("G43 H2 G20;\nM30;\n", "t.ngc").expect("Ok + Raw");
    assert_eq!(
        out.program.units,
        Units::In,
        "G20 на raw-кадре абсорбирован"
    );
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
    // (b) G55 на raw-кадре — wcs абсорбирован → следующий оп несёт G55
    // (полный end: точка разрыва (b) без origin'ов — start Unknown легален).
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG28 G55;\nG0 X20. Y20. Z5.;\nM30;\n")
        .expect("ok");
    match &p.ops[2] {
        Op::Rapid { state, .. } => assert_eq!(state.wcs, eid_core::ast::Wcs::G55),
        other => panic!("ops[2] не Rapid: {other:?}"),
    }
    // (c) F на raw-кадре (G43) — модальная подача установлена.
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG43 H2 F500.;\nG1 X20. Y20.;\nM30;\n")
        .expect("ok");
    match &p.ops[2] {
        Op::Linear { state, .. } => {
            assert!((state.feed.as_ref().expect("feed").value() - 500.0).abs() <= EPS);
        }
        other => panic!("ops[2] не Linear: {other:?}"),
    }
    // (d) M8 на raw-кадре — СОЖ абсорбирована.
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG43 H2 M8;\nG1 X20. Y20. F100.;\nM30;\n")
        .expect("ok");
    match &p.ops[2] {
        Op::Linear { state, .. } => assert!(state.coolant.flood, "M8 на raw-кадре абсорбирован"),
        other => panic!("ops[2] не Linear: {other:?}"),
    }
    // (e) Смена units на raw-кадре ПОСЛЕ установленной системы —
    // структурный класс (§8.5), жёсткий отказ.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG43 H2 G20;\nG0 X20. Y20.;\nM30;\n"
        )),
        ErrorCode::UnsupportedSemantics
    );
}

#[test]
fn g92_and_g10_absorb_on_raw_frames() {
    // SPEC §8.6: G92/G10 в списке абсорбции. G92 X.. на кадре с raw-кодом —
    // пресетка абсорбируется; G10 L2 полная форма на raw-кадре обновляет
    // таблицу origin (связывание последующих WCS-переходов).
    let (p, _) = notices_of("G21 G90 G94 G17;\nG92 X0. Y0. Z50. G43 H1;\nM30;\n");
    let preset = p
        .frame_preset
        .as_ref()
        .expect("G92 на raw-кадре абсорбирован");
    assert_pt(&preset.values, 0.0, 0.0, 50.0);
    // G10 L2 P2 (= origin G55) полная форма на raw-кадре + легальная P1
    // (origin G54) + WCS-переход (формула разности):
    // чтение в G55 = pos_wcs54 + O54 − O55 = (10,10,5) + 0 − (5,0,0) = (5,10,5).
    let p = parse(
        "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG10 L2 P2 X5. Y0. Z0. G28;\nG10 L2 P1 X0. Y0. Z0.;\nG55;\nG0 X30. Y30.;\nM30;\n",
    )
    .expect("ok");
    match &p.ops[1] {
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        } => {}
        other => panic!("ops[1] не Raw(G28): {other:?}"),
    }
    assert!(
        matches!(&p.ops[2], Op::WcsDefine { p: 1, .. }),
        "легальная L2 P1 — узел"
    );
    match &p.ops[3] {
        Op::Rapid { .. } => {
            let s = known(&p.ops[3]);
            assert!(
                (s[0] - 5.0).abs() <= EPS,
                "start {s:?} = prev(10,10,5) + O54(0) − O55(5,0,0)"
            );
            assert!((s[1] - 10.0).abs() <= EPS);
        }
        other => panic!("ops[3] не Rapid: {other:?}"),
    }
}

// ===========================================================================
// Зеркало SPEC §7.4a v0.18 — главный конфирм (бриф п.6)
// ===========================================================================

#[test]
fn mirror_v018_golden_cross_check_19_20_21() {
    // fanuc-парсер обязан давать на v0-файлах #19/#20/#21 те же AST, что
    // golden.eid (regen от D, rs274-конформный, commit 4705480) —
    // semantic_diff = 0. Golden-файлы НЕ трогаются (общие, уже v0.18);
    // источники — linuxcnc-диалект, значения с десятичными точками →
    // fanuc-DPI verbatim; dotted P → секунды verbatim (v0.18-правило).
    use eid_core::adapter::{Adapter, EidTextAdapter};
    use eid_harness::diff::{semantic_diff, DiffConfig};
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tier_a/v0");
    for (dir, src_name) in [
        ("19_peck_G83_retract", "peck_G83_retract.linuxcnc.ngc"),
        ("20_chipbreak_G73", "chipbreak_G73.linuxcnc.ngc"),
        ("21_bore_G86_restart", "bore_G86_restart.linuxcnc.ngc"),
    ] {
        let source =
            std::fs::read_to_string(root.join(dir).join(src_name)).expect("v0 source readable");
        let golden =
            std::fs::read_to_string(root.join(dir).join("golden.eid")).expect("golden readable");
        let a = FanucAdapter::new()
            .parse(&source, src_name)
            .expect("fanuc parse ok");
        let b = EidTextAdapter::new()
            .parse(&golden, "golden.eid")
            .expect("golden parses");
        let d = semantic_diff(&a, &b, &DiffConfig::default());
        assert!(d.is_empty(), "{dir}: divergences: {d:?}");
    }
}

// ===========================================================================
// Мутационный раунд v0.17/v0.18 (агент D): связывание старта с ненулевыми
// origin (точки разрыва (b)/(d)), частичный G10 L2, G93 3D-длина, zero-arc
// с end-словами, четверть-дуга (§6-уравнение позиции), sub-нумерация развёртки.
// ===========================================================================

#[test]
fn mround_wcs_switch_with_origins_nonzero() {
    // Точка разрыва (b) с НЕНУЛЕВЫМИ origin: reading = prev + O_prev − O_new
    // = (10,10,5) + (10,20,30) − (1,2,3) = (19,28,32).
    let p = parse(
        "G21 G90 G94 G17;\nG10 L2 P1 X10. Y20. Z30.;\nG10 L2 P2 X1. Y2. Z3.;\nG0 X10. Y10. Z5.;\nG55;\nG0 X1. Y2. Z3.;\nM30;\n",
    )
    .expect("ok");
    assert_eq!(
        known(&p.ops[3]),
        [19.0, 28.0, 32.0],
        "start = prev_end + O54 − O55"
    );
}

#[test]
fn mround_g53_machine_nonzero_origin() {
    // §5.3 с ненулевым O_wcs: machine-ход из WCS — start = W + O = (20,30,35);
    // обратный WCS-ход (точка разрыва (d)) — reading = M − O = (−5,−15,−130).
    let p = parse(
        "G21 G90 G94 G17;\nG10 L2 P1 X10. Y20. Z30.;\nG0 X10. Y10. Z5.;\nG53 G0 X5. Y5. Z-100.;\nG0 X7. Y7. Z7.;\nM30;\n",
    )
    .expect("ok");
    assert_eq!(known(&p.ops[2]), [20.0, 30.0, 35.0], "M = W + O_wcs");
    assert_eq!(known(&p.ops[3]), [-5.0, -15.0, -130.0], "reading = M − O_wcs");
}

#[test]
fn mround_g92_preset_cross_wcs_with_origins() {
    // Точка разрыва (b) для пресетки: reading = preset + O_preset − O_current
    // = (5,5,5) + (1,1,1) − (2,2,2) = (4,4,4).
    let p = parse(
        "G21 G90 G94 G17;\nG92 X5. Y5. Z5.;\nG10 L2 P1 X1. Y1. Z1.;\nG10 L2 P2 X2. Y2. Z2.;\nG55;\nG1 X10. F100.;\nM30;\n",
    )
    .expect("ok");
    assert_eq!(
        known(&p.ops[2]),
        [4.0, 4.0, 4.0],
        "start = preset + O54 − O55"
    );
}

#[test]
fn mround_g93_linear_feed_len_3d() {
    // G93 inverse-time: 3D-диагональ от ненулевого старта — len = |Δ| = 5,
    // F = 1 → feed = 5 (ранее тесты были осе-выровнены — мутанты len-формулы
    // выживали).
    let p = parse("G21 G90 G94 G17;\nG0 X1. Y2. Z3.;\nG93;\nG1 X4. Y6. Z3. F1.;\nM30;\n").expect("ok");
    assert_eq!(feed_of(&p.ops[1]), Some(5.0));
}

#[test]
fn mround_zero_arc_end_words_degenerate_center_dropped() {
    // SPEC §3 v0.17: явные end-слова + center == start (rho = 0) →
    // zero-length drop, а НЕ полный круг (различитель от full-circle).
    let (_p, notices) = notices_of(
        "G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG1 X10. Y0. F100.;\nG2 X10. Y0. I0. J0.;\nM30;\n",
    );
    assert!(
        notices.iter().any(|n| n.kind == NoticeKind::ZeroLengthArc),
        "zero-arc с end-словами обязан быть dropped: {notices:?}"
    );
}

#[test]
fn mround_arc_quarter_ccw_ok() {
    // Четверть CCW R=10 из (10,0) в (0,10), центр (0,0) — валидная дуга:
    // §6-уравнение позиции сходится, sweep 90° (ранее тесты дуг были
    // полуокружностями — sin=0 маскировал мутантов Rodrigues).
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG3 X0. Y10. I-10. J0. F100.;\nM30;\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc { center, sweep_deg, .. } => {
            assert_pt(center, 0.0, 0.0, 0.0);
            assert!(
                (sweep_deg.value() - 90.0).abs() <= 1e-6,
                "sweep {}",
                sweep_deg.value()
            );
        }
        other => panic!("ops[1] не Arc: {other:?}"),
    }
}

#[test]
fn mround_g83_unroll_subs_sequence() {
    // Развёртка G83: sub производных опов — последовательность 1,2,3,… без
    // повторов (F-правило sub=1 в эмиссии опирается на нумерацию).
    // Начальный Z ВЫШЕ R-плоскости — иначе подход к R zero-length и sub
    // потребляется на dropped-опе (дыра в нумерации — отдельное наблюдение).
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nF100.;\nG83 X5. Z-10. Q3. R2.;\nM30;\n")
        .expect("ok");
    let mut subs = Vec::new();
    for op in &p.ops {
        if let Op::Rapid { sub, .. } | Op::Linear { sub, .. } | Op::Dwell { sub, .. } = op {
            if let Some(s) = sub {
                subs.push(*s);
            }
        }
    }
    assert_eq!(subs, Vec::<u32>::from_iter(1..=subs.len() as u32),
        "sub-нумерация развёртки — плотная последовательность с 1: {subs:?}");
    assert!(subs.len() >= 8, "ожидалась многопековая развёртка: {subs:?}");
}
