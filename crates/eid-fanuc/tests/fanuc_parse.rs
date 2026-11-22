//! Интеграционные тесты канонизатора Fanuc → EID/L1 (этап 1, Агент C).
//! Golden-пары (fanuc-текст → EID AST) по образцу eid-grbl tests/grbl_parse.rs;
//! допуски — EQUIVALENCE §4 (eps 0.001).
//!
//! ЧЕК-ЛИСТ этапа 1 (правило → тест):
//! LEXICAL: EOB-разделение → eob_full_line_program, lexer unit-тесты
//!   (12, src/lexer.rs); вложенный/незакрытый комментарий → lexer; % →
//!   percent_ignored; N-слова → n_word_line; трейлинг-точка → dpi_point…
//!   + lexer; целая часть >9 цифр → integer_overflow_limit.
//!  MODAL (01–06): violation → modal_violation_invalid; G90/G91 →
//!   g91_incremental; плоскости → arc_g18/arc_g19; G90.1/G91.1 →
//!   arc_g90_1_absolute_center; G94/G95 → g95_raw_semantics; units G20/G21 +
//!   G70/G71 → units_alias_g70_g71, dpi_bare_integer_inch; смена units →
//!   unit_switch_mid_invalid.
//!  MOTION: rapid/linear → eob_full_line_program; модальное продолжение →
//!   modal_continuation; голые координаты без motion → naked_coords_invalid;
//!   zero-length → zero_length_linear_drop.
//!  ARC §6: IJK 3 плоскости × cw/ccw → arc_ijk_cw_helix / arc_ijk_ccw /
//!   arc_g18 / arc_g19; R± → arc_r_positive / arc_r_negative; R+pitch →
//!   arc_r_helix; helical → arc_ijk_cw_helix; полная окружность →
//!   full_circle / helical_full_turn; R-полный-круг → r_full_circle_invalid;
//!   радиус-мисс → radius_mismatch_invalid; хорда>2R → chord_gt_2r_invalid;
//!   перпендикулярная компонента → plane_perpendicular_invalid.
//!  DPI (§1.2, W_DPI_ASSUMED): dpi_bare_integer_mm / _inch / dpi_point_not…
//!   / dpi_all_bare_arc / dpi_pending_f_flushes_to_next_op.
//!  STRUCTURE: auto-PEND → auto_pend_missing_program_end; O-code → o_code_raw;
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
//!   g92_frame_preset_anchor; mid-program → v020_g92_mid_* (v0.20: chaining
//!   + Raw-носитель; стартовый merge; G53-N16);
//!   G92 S → g92_s_clamp_raw; G52 → g52_hard_err (единственный ERR);
//!   G53 machine-frame → g53_machine_move / g53_first_move_unknown /
//!   g53_partial_end_invalid / g53_after_preset_invalid /
//!   g53_requires_explicit_motion / g53_arc_invalid; точка разрыва (d) →
//!   (в g53_machine_move); G54.1 Pn → g54_1_pn_raw; G92.1/.2/.3 →
//!   g92_1_reset_raw (v0.20: Raw{REQUIRES_MACHINE_TABLE}, зонд 4).
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
// doc-чек-лист в прежнем формате (продолжения строк) — вне скоупа INTERFACE v0.2;
// lint отключён точечно, форматирование не трогаем.
#![allow(clippy::doc_lazy_continuation)]

use eid_core::ast::{Dir, Frame, Op, Point3, Program, Spindle, StartPoint, Units};
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
fn v020_tail_notice_dialect_divergence_text() {
    // SPEC §5.2 v0.20 (зонд 5): tail-reason согласован с notice-текстом
    // диалект-расхождения — rs274-файловый интерпретатор молча игнорирует
    // хвост, grbl-стрим исполняет; для fanuc-хвоста-subs семантика та же
    // lossless (носитель Raw, motion-опы после PEND не строятся).
    let p = parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\nO0100;\nM99;\n").expect("Ok");
    let raw = p
        .ops
        .iter()
        .find(|o| matches!(o, Op::Raw { .. }))
        .expect("хвост — Raw");
    match raw {
        Op::Raw { reason, .. } => {
            assert!(
                reason.contains("ignored in rs274-file semantics"),
                "{reason}"
            );
            assert!(
                reason.contains("executed in grbl-stream semantics"),
                "{reason}"
            );
            assert!(reason.contains("dialect-divergent"), "{reason}");
            assert!(reason.contains("lossless"), "{reason}");
        }
        _ => unreachable!(),
    }
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
fn g10_l10_r_tool_table_raw() {
    // COVERAGE §6.71: R в официальном Fanuc G10 L10 = geometry-значение
    // (L10 = длина-geometry/H, L12 = диаметр-geometry/D — R-словом).
    // Frame/DataRecord-план потребляет P/L/R (паритет eid-haas) → строка
    // доходит до шага (17) → Raw{REQUIRES_MACHINE_TABLE}, НЕ Err на шаге (14)
    // (BEHAVIOR-CHANGE: прежде Err(INVALID) «R word without consumer» —
    // ревью-одобрено, официальный формат vs недосмотр).
    let out =
        canonicalize_with_notices("G10 L10 G90 P5 R250.;\nM30;\n", "t.ngc").expect("полный кадр");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
    assert!(raw_reason(&out.program, 0).contains("table record"));
}

#[test]
fn g10_l2_broken_p_fractional_unaffected() {
    // Негатив-регресс §6.71: сломанная форма L2 P1.9 (P вне целого домена
    // 1..6) — фикс потребления P/L/R её disposition НЕ меняет (шаг (17):
    // Raw{SYNTAX} «outside ... domain», как и до фикса).
    let out = canonicalize_with_notices("G10 L2 P1.9;\nM30;\n", "t.ngc").expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    assert!(raw_reason(&out.program, 0).contains("outside"));
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
fn v020_g92_mid_golden_chain_new_reading() {
    // SPEC §3 v0.20 (коммит c76ed9a; зонды 1–3 target/probe/g92): mid-program
    // G92 — валидный frame_preset; ограничение «только startup» снято
    // (бывший Err SEMANTICS «G92 after motion»). Golden зонда 1:
    // `G0 X10 Y10 / G92 X0 Y0 / G1 X5` → linear с НОВЫМ чтением
    // (start = (0,0,0), end = (5,0,0)); AST-носитель блока — Raw
    // (grammar v0.3 не имеет mid-program frame_preset-опa; chaining — в
    // каноническом состоянии).
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17;\nG0 X10. Y10.;\nG92 X0. Y0.;\nG1 X5. F100.;\nM30;\n",
        "t.ngc",
    )
    .expect("G92-mid → Ok (v0.20)");
    let p = out.program;
    assert!(
        p.frame_preset.is_none(),
        "header — только СТАРТОВАЯ пресетка"
    );
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    match &p.ops[1] {
        Op::Raw { reason, code, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(reason.starts_with("G92 mid-program"), "{reason}");
            assert!(reason.contains("frame_preset"), "{reason}");
        }
        other => panic!("op1 не Raw-носитель G92: {other:?}"),
    }
    match &p.ops[2] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_pt(s, 0.0, 0.0, 0.0); // новое чтение после G92 X0 Y0
            assert_pt(end, 5.0, 0.0, 0.0);
        }
        other => panic!("op2 не Linear: {other:?}"),
    }
    assert!(matches!(&p.ops[3], Op::ProgramEnd { .. }));
}

#[test]
fn v020_g92_mid_twice_replace_by_recompute() {
    // Зонд 2 (Q5 закрыт v0.20): повторная G92 ЗАМЕНЯЕТ регистр по пересчёту
    // от текущего чтения (не аккумуляция). Без промежуточного хода: чтение
    // (10,0,0); G92 X0 → 0; G92 X5 → 5 (replace, не 0−5); G1 X8 — start = 5.
    let p = parse("G21 G90 G94 G17;\nG0 X10.;\nG92 X0.;\nG92 X5.;\nG1 X8. F100.;\nM30;\n")
        .expect("два G92 подряд → Ok (Q5 закрыт)");
    let raws = p.ops.iter().filter(|o| matches!(o, Op::Raw { .. })).count();
    assert_eq!(raws, 2, "несколько G92 = несколько опов-носителей");
    match &p.ops[3] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_pt(s, 5.0, 0.0, 0.0);
            assert_pt(end, 8.0, 0.0, 0.0);
        }
        other => panic!("op3 не Linear: {other:?}"),
    }
}

#[test]
fn v020_g92_mid_per_axis_y_reading_unchanged() {
    // Зонд 3: per-axis assignment — `G92 X0` меняет чтение ТОЛЬКО X;
    // Y-чтение не меняется (старт хода Y = 10 — прежняя система).
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y10.;\nG92 X0.;\nG1 X5. Y5. F100.;\nM30;\n")
        .expect("per-axis G92 → Ok");
    match &p.ops[2] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_pt(s, 0.0, 10.0, 0.0);
            assert_pt(end, 5.0, 5.0, 0.0);
        }
        other => panic!("op2 не Linear: {other:?}"),
    }
}

#[test]
fn v020_g92_mid_wcs_switch_unknown_start() {
    // Зонд 8: регистр G92 глобален поверх WCS; после mid-G92 смена G54→G55
    // без in-text origin'ов → честный unknown-start (rs274 пересчитал бы от
    // машины, машины в AST нет). Полный end легален (точка разрыва).
    let p =
        parse("G21 G90 G94 G17;\nG54 G0 X10. Y0. Z0.;\nG92 X0.;\nG55;\nG0 X5. Y0. Z0.;\nM30;\n")
            .expect("WCS-переключение после mid-G92 → Ok");
    match &p.ops[2] {
        Op::Rapid {
            start: StartPoint::Unknown,
            end,
            state,
            ..
        } => {
            assert_pt(end, 5.0, 0.0, 0.0);
            assert_eq!(state.wcs, eid_core::ast::Wcs::G55);
        }
        other => panic!("op2 не Rapid с unknown-start: {other:?}"),
    }
}

#[test]
fn v020_g92_mid_wcs_switch_with_origins_formula_holds() {
    // Зонд 8b: Δ (регистр G92) сокращается в разности чтений — формула (b)
    // reading_new = prev + O_prev − O_new точна и после mid-G92: rebase X→0,
    // O54=(1,1,1), O55=(2,2,2) → start (−1,−1,−1).
    let p = parse(
        "G21 G90 G94 G17;\nG54 G0 X10. Y0. Z0.;\nG92 X0.;\nG10 L2 P1 X1. Y1. Z1.;\nG10 L2 P2 X2. Y2. Z2.;\nG55;\nG0 X5. Y0. Z0.;\nM30;\n",
    )
    .expect("формула (b) с in-text origins после mid-G92 → Ok");
    // ops: Rapid, Raw(G92), WcsDefine ×2, Rapid(G55), PEND.
    match &p.ops[4] {
        Op::Rapid {
            start: StartPoint::Known(s),
            ..
        } => assert_pt(s, -1.0, -1.0, -1.0),
        other => panic!("op4 не Rapid с Known start: {other:?}"),
    }
}

#[test]
fn v020_g92_mid_second_startup_g92_merges_per_axis() {
    // Q5 закрыт v0.20: вторая G92 В СТАРТОВОЙ ЗОНЕ — per-axis merge пресетки
    // (зонд 3: неуказанные оси чтение не меняют; машина не двигалась).
    let p = parse("G21 G90 G94 G17;\nG92 X5. Y5. Z5.;\nG92 X10.;\nG1 X15. F100.;\nM30;\n")
        .expect("второй G92 в стартовой зоне → Ok (merge)");
    let pr = p.frame_preset.as_ref().expect("стартовая пресетка");
    assert_pt(&pr.values, 10.0, 5.0, 5.0);
    match &p.ops[0] {
        Op::Linear {
            start: StartPoint::Known(s),
            ..
        } => assert_pt(s, 10.0, 5.0, 5.0),
        other => panic!("op0 не Linear: {other:?}"),
    }
}

#[test]
fn v020_g92_mid_g53_after_mid_g92_runtime() {
    // N16 (§5.3a): G53 machine-ход при активном регистре G92 — RUNTIME
    // (чтение машинной позиции с активным Δ вне AST; зеркало grbl n16).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG92 X0. Y0. Z0.;\nG53 G0 X0. Y0. Z0.;\nM30;\n"
        )),
        ErrorCode::RuntimeState
    );
}

#[test]
fn v020_g92_mid_on_raw_block_carries_marker() {
    // mid-G92 на raw-кадре (`G92 X2. G95` после motion): rebase применён
    // (Fanuc исполняет G92 в блоке), маркер в reason Raw-носителя — по нему
    // strip start-диагностик эмиссии вооружается.
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG92 X2. G95\nG0 X5. Y0. Z0.;\nM30;\n",
        "t.ngc",
    )
    .expect("mid-G92 на raw-кадре → Ok");
    let p = out.program;
    let raw = p
        .ops
        .iter()
        .find(|o| matches!(o, Op::Raw { .. }))
        .expect("G95-кадр — Raw-носитель");
    match raw {
        Op::Raw { reason, .. } => {
            assert!(
                reason.contains("G92 mid-program"),
                "маркер rebase в reason носителя: {reason}"
            );
        }
        _ => unreachable!(),
    }
    // chaining: чтение после G92 X2 от (10,0,0) = 2 → start следующего хода 2.
    match &p.ops[2] {
        Op::Rapid {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_pt(s, 2.0, 0.0, 0.0);
            assert_pt(end, 5.0, 0.0, 0.0);
        }
        other => panic!("op2 не Rapid: {other:?}"),
    }
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
    // G92.1/.2/.3 → Raw{E_REQUIRES_MACHINE_TABLE} (v0.20 SEMANTICS→TABLE,
    // зонд 4: G92.1 обнуляет ВЕСЬ регистр; возврат прежних чтений требует
    // абсолютной машинной позиции, которой в AST нет; COVERAGE §2.3).
    for code in ["G92.1", "G92.2", "G92.3"] {
        let out = canonicalize_with_notices(&format!("{code};\nM30;\n"), "t.ngc").expect("ok");
        match &out.program.ops[0] {
            Op::Raw {
                code: c, reason, ..
            } => {
                assert_eq!(*c, ErrorCode::RequiresMachineTable, "{code}: {reason}");
                if code == "G92.1" {
                    assert!(reason.contains("machine position"), "{code}: {reason}");
                }
            }
            other => panic!("{code}: не Raw: {other:?}"),
        }
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
            .expect("fanuc parse ok")
            .program;
        let b = EidTextAdapter::new()
            .parse(&golden, "golden.eid")
            .expect("golden parses")
            .program;
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
    assert_eq!(
        known(&p.ops[3]),
        [-5.0, -15.0, -130.0],
        "reading = M − O_wcs"
    );
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
    let p =
        parse("G21 G90 G94 G17;\nG0 X1. Y2. Z3.;\nG93;\nG1 X4. Y6. Z3. F1.;\nM30;\n").expect("ok");
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
        Op::Arc {
            center, sweep_deg, ..
        } => {
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
    assert_eq!(
        subs,
        Vec::<u32>::from_iter(1..=subs.len() as u32),
        "sub-нумерация развёртки — плотная последовательность с 1: {subs:?}"
    );
    assert!(
        subs.len() >= 8,
        "ожидалась многопековая развёртка: {subs:?}"
    );
}

// ===========================================================================
// МУТ-РАУНД 2 (fanuc): батчи mround2_* — кластеры baseline 76.4%
// (canonicalize.rs process_block/absorb §8.6, R-формы, циклы, лимиты,
// state.rs g_raw_disposition-таблица).
// ===========================================================================

// ---------------------------------------------------------------------------
// §8.6: raw-motion-G с осями/без осей (W_MODAL_ON_RAW)
// ---------------------------------------------------------------------------

#[test]
fn mround2_modal_on_raw_g28_no_axes() {
    // G28 БЕЗ осевых слов: raw, но notice W_MODAL_ON_RAW НЕ выдаётся,
    // модальность сохранена (G91 живёт → следующий ход инкрементальный).
    let (p, ns) = notices_of("G0 X0. Y0. Z0.\nG91\nG28\nG0 X10. Y0. Z0.\nM30\n");
    assert!(
        ns.iter().all(|n| n.kind != NoticeKind::ModalOnRaw),
        "G28 без осей не должен ронять motion-режим: {ns:?}"
    );
    let rap = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Rapid { .. }))
        .nth(1)
        .expect("rapid после raw");
    assert_eq!(known(&rap), [0.0, 0.0, 0.0]);
    match rap {
        Op::Rapid { end, .. } => assert_pt(&end, 10.0, 0.0, 0.0),
        _ => unreachable!(),
    }
}

#[test]
fn mround2_modal_on_raw_g28_with_axes_notice() {
    // G28 С осевым словом: raw + W_MODAL_ON_RAW (SPEC §8.6 правило 6).
    let (_, ns) = notices_of("G0 X0. Y0. Z0.\nG91\nG28 Z0.\nM30\n");
    assert!(
        ns.iter().any(|n| n.kind == NoticeKind::ModalOnRaw),
        "нет W_MODAL_ON_RAW: {ns:?}"
    );
}

#[test]
fn mround2_modal_on_raw_g28_cancels_cycle() {
    // Активный цикл + raw-motion-G с осями: cycle=None; следующий X/Y-кадр
    // без motion-моды — naked coords Err (повтора цикла быть не должно).
    let r = parse("G0 X0. Y0. Z5.\nG81 X10. Z-5. R2. F100.\nG28 Z0.\nX20. Y20.\nM30\n");
    assert_eq!(err_code(r), ErrorCode::InvalidInput);
}

// ---------------------------------------------------------------------------
// §2.4/§1.3: H/D-слова, H00-cancel
// ---------------------------------------------------------------------------

#[test]
fn mround2_h00_cancel_with_g49_family_raw_reason() {
    // H00 при живой G43/G44-семье в файле → Raw{RT} с причиной про H00.
    let p = parse("G43 H1 Z5.\nG0 X1. Y1. Z1.\nH0\nM30\n").expect("ok");
    let raw = p
        .ops
        .iter()
        .filter(|o| {
            matches!(
                o,
                Op::Raw {
                    code: ErrorCode::RequiresMachineTable,
                    ..
                }
            )
        })
        .count();
    assert!(raw >= 2, "G43-строка и H00 обязаны быть Raw RT: {raw}");
    let h00 = p.ops.iter().find_map(|o| match o {
        Op::Raw { reason, .. } if reason.contains("H00") => Some(()),
        _ => None,
    });
    assert!(h00.is_some(), "нет H00-cancel raw");
}

#[test]
fn mround2_h00_with_g43_same_block_is_g43_not_h00() {
    // G43+H0 в ОДНОМ кадре: это активация TLO (raw по g_raw_disposition),
    // а НЕ H00-cancel — причина обязана быть про G43 (различитель
    // h_cancel-условия «в кадре есть G-код семьи G49»).
    let p = parse("G43 H0 X5. Y5. Z5.\nM30\n").expect("ok");
    match p.ops.first().expect("raw op") {
        Op::Raw { reason, code, .. } => {
            assert_eq!(*code, ErrorCode::RequiresMachineTable);
            assert!(
                reason.contains("G43 tool length offset"),
                "причина должна быть G43-TLO, не H00-cancel: {reason}"
            );
        }
        other => panic!("ожидался Raw: {other:?}"),
    }
}

#[test]
fn mround2_h_offset_word_raw() {
    // H≠0 без G43 — offset-номер из таблицы станка → Raw{RT}.
    let p = parse("G0 X1. Y1. Z1.\nH2\nM30\n").expect("ok");
    assert!(matches!(
        p.ops.iter().find(|o| matches!(o, Op::Raw { .. })),
        Some(Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        })
    ));
    // D без G41-контекста → Raw{RT}.
    let p = parse("G0 X1. Y1. Z1.\nD7\nM30\n").expect("ok");
    assert!(matches!(
        p.ops.iter().find(|o| matches!(o, Op::Raw { .. })),
        Some(Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        })
    ));
}

// ---------------------------------------------------------------------------
// §3: T-домен при M6
// ---------------------------------------------------------------------------

#[test]
fn mround2_t_domain_boundaries() {
    // T вне домена 0..9999 (дробное/отрицательное/переполнение) → Err.
    assert_eq!(err_code(parse("T2.5 M6\nM30\n")), ErrorCode::InvalidInput);
    assert_eq!(err_code(parse("T-1 M6\nM30\n")), ErrorCode::InvalidInput);
    assert_eq!(err_code(parse("T10000 M6\nM30\n")), ErrorCode::InvalidInput);
    // Границы домена легальны: T0 и T9999.
    let p = parse("T0 M6\nM30\n").expect("T0 ok");
    assert!(p
        .ops
        .iter()
        .any(|o| matches!(o, Op::ToolChange { tool: 0, .. })));
    parse("T9999 M6\nM30\n").expect("T9999 ok");
    // M6 без T → Err.
    assert_eq!(err_code(parse("M6\nM30\n")), ErrorCode::InvalidInput);
}

#[test]
fn mround2_t_alone_raw() {
    // Одиночное T — предвыбор (modal next-tool) → Raw{SEMANTICS}.
    let p = parse("G0 X1. Y1. Z1.\nT05\nM30\n").expect("ok");
    assert!(matches!(
        p.ops.iter().find(|o| matches!(o, Op::Raw { .. })),
        Some(Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        })
    ));
}

// ---------------------------------------------------------------------------
// Модальные группы: cancel цикла, path-repeat, units
// ---------------------------------------------------------------------------

#[test]
fn mround2_nonmotion_g_does_not_cancel_cycle() {
    // Не-motion G-код НЕ отменяет активный цикл: X/Y-кадр после него —
    // повтор (отверстие), а не naked-coords Err.
    // (a) G99 — группа 10:
    let p = parse("G0 X0. Y0. Z5.\nG81 X10. Z-5. R2. F100.\nG99\nX20. Y20.\nM30\n")
        .expect("ok: цикл пережил G99");
    let lins = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Linear { .. }))
        .count();
    assert_eq!(lins, 2, "два сверления (до и после G99)");
    // (b) G55 (группа 12) с определёнными обеими origin — связывание
    // Known, повтор легален:
    let p = parse(
        "G10 L2 P1 X0. Y0. Z0.\nG10 L2 P2 X0. Y0. Z0.\nG0 X0. Y0. Z5.\nG81 X10. Z-5. R2. F100.\nG55\nX20. Y20.\nM30\n",
    )
    .expect("ok: цикл пережил G55");
    let lins = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Linear { .. }))
        .count();
    assert_eq!(lins, 2, "два сверления (до и после G55)");
}

#[test]
fn mround2_path_same_mode_repeat_ok() {
    // Повтор того же path-режима — легальный no-op (не «switch» → Err).
    parse("G61\nG0 X1. Y1. Z1.\nG61\nG1 X2. Y2. Z2. F100.\nM30\n").expect("повтор G61 ok");
    parse("G64\nG0 X1. Y1. Z1.\nG64\nG1 X2. Y2. Z2. F100.\nM30\n").expect("повтор G64 ok");
    assert_eq!(
        err_code(parse("G61\nG0 X1. Y1. Z1.\nG64\nM30\n")),
        ErrorCode::UnsupportedSemantics
    );
}

#[test]
fn mround2_units_same_word_repeat_ok() {
    // Повтор слова units с ТЕМ ЖЕ значением — не «unit switch mid-program».
    parse("G21\nG21\nG0 X1. Y1. Z1.\nM30\n").expect("повтор G21 ok");
    parse("G20\nG70\nG0 X1. Y1. Z1.\nM30\n").expect("G20 затем G70 (тот же inch) ok");
}

// ---------------------------------------------------------------------------
// §8.6 абсорбция структур (G92/G10) на raw-кадрах
// ---------------------------------------------------------------------------

#[test]
fn mround2_g10_absorb_increment_on_raw() {
    // G10 L2 ПОЛНАЯ форма на raw-кадре (через G95): таблица origin
    // обновляется инкрементально (G91: old+v), затем G53-связывание видит
    // новое значение (reading_M = W + O_wcs).
    let p = parse(
        "G0 X1. Y1. Z1.\nG10 L2 P2 X7. Y7. Z7.\nG91\nG10 L2 P2 X3. Y3. Z3. G95\nG90 G55\nG0 X2. Y2. Z2.\nG53 G0 X100. Y100. Z100.\nM30\n",
    )
    .expect("ok");
    let g53 = p
        .ops
        .iter()
        .find(|o| matches!(o, Op::Rapid { state, .. } if state.frame == Frame::Machine))
        .expect("G53 rapid");
    // start = reading_G55 (2,2,2) + O_g55 (7+3, ...) = (12,12,12).
    assert_eq!(known(&g53), [12.0, 12.0, 12.0]);
}

#[test]
fn mround2_g10_absorb_partial_raw_no_update() {
    // Частичная G10 L2 на raw-кадре: origin НЕ обновляется (значение осей
    // из таблицы станка, вне текста). X-only и XY-варианты.
    for cmd in ["G10 L2 P2 X3. G95", "G10 L2 P2 X3. Y3. G95"] {
        let p = parse(&format!(
            "G10 L2 P2 X7. Y7. Z7.\nG0 X1. Y1. Z1.\nG91\n{cmd}\nG90 G55\nG0 X2. Y2. Z2.\nG53 G0 X100. Y100. Z100.\nM30\n"
        ))
        .expect("ok");
        let g53 = p
            .ops
            .iter()
            .find(|o| matches!(o, Op::Rapid { state, .. } if state.frame == Frame::Machine))
            .expect("G53 rapid");
        // start = (2,2,2) + (7,7,7) — частичная форма таблицу не трогает.
        assert_eq!(known(&g53), [9.0, 9.0, 9.0]);
        let _ = cmd;
    }
}

#[test]
fn mround2_g10_absorb_broken_l_no_update() {
    // L вне домена (L2.5 — усечённо «L2», но значение дробное) на raw-кадре:
    // сломанная форма таблицу не обновляет.
    let p = parse(
        "G10 L2 P2 X7. Y7. Z7.\nG0 X1. Y1. Z1.\nG91\nG10 L2.5 P2 X3. Y3. Z3. G95\nG90 G55\nG0 X2. Y2. Z2.\nG53 G0 X100. Y100. Z100.\nM30\n",
    )
    .expect("ok");
    let g53 = p
        .ops
        .iter()
        .find(|o| matches!(o, Op::Rapid { state, .. } if state.frame == Frame::Machine))
        .expect("G53 rapid");
    // L2.5 — сломанная форма: таблица не обновлена → start (2,2,2)+(7,7,7).
    assert_eq!(known(&g53), [9.0, 9.0, 9.0]);
}

#[test]
fn mround2_g92_absorb_on_raw_block() {
    // G92 (X-only) на raw-кадре (через G95): пресетка абсорбируется →
    // следующий ход связывается от пресетки.
    let p = parse("G91\nG92 X5. G95\nG0 X2. Y0. Z0.\nM30\n").expect("ok");
    let mv = p
        .ops
        .iter()
        .find(|o| matches!(o, Op::Rapid { .. }))
        .expect("ход после raw-G92");
    // Пресетка (5,0,0) — якорь точки разрыва (a); G91 абсорбирован →
    // X2 инкрементальный → end (7,0,0).
    assert_eq!(known(&mv), [5.0, 0.0, 0.0]);
    match mv {
        Op::Rapid { end, .. } => assert_pt(&end, 7.0, 0.0, 0.0),
        _ => unreachable!(),
    }
}

#[test]
fn mround2_g92_second_in_absorb_ok_v020() {
    // v0.20 (Q5 закрыт): второй G92 в стартовой зоне на raw-кадре — per-axis
    // merge пресетки (как на легальном пути); G92 после motion на raw-кадре —
    // mid-chaining + маркер в reason носителя (см. v020_g92_mid_on_raw_block).
    let p = parse("G92 X1. Y1. Z1.\nG92 X2. Y2. Z2. G95\nM30\n").expect("Ok (merge, v0.20)");
    let pr = p.frame_preset.as_ref().expect("стартовая пресетка (merge)");
    assert_pt(&pr.values, 2.0, 2.0, 2.0);
    // G92 после motion на raw-кадре → Ok (chaining, зонд 1; Err снят v0.20).
    parse("G0 X1. Y1. Z1.\nG92 X2. Y2. Z2. G95\nM30\n").expect("Ok (mid-chaining, v0.20)");
}

// ---------------------------------------------------------------------------
// Провенанс F-кадра (v0.2 волна 3): feed_restated
// ---------------------------------------------------------------------------

#[test]
fn mround2_feed_restated_flag_ops() {
    // F-слово в кадре → feed_restated=true на опе кадра; кадр без F — false.
    let p = parse("G1 X1. Y0. Z0. F100.\nG1 X2. Y0. Z0.\nM30\n").expect("ok");
    let lins: Vec<&Op> = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Linear { .. }))
        .collect();
    assert_eq!(lins.len(), 2);
    for (i, o) in lins.iter().enumerate() {
        let st = match o {
            Op::Linear { state, .. } => state,
            _ => unreachable!(),
        };
        assert_eq!(st.feed_restated, i == 0, "linear #{i} feed_restated");
    }
}

#[test]
fn mround2_feed_restated_cycle_first_derived_only() {
    // Развёртка: флаг кадра несёт ТОЛЬКО первый производный (sub == 1).
    let (p, _) = notices_of("G0 X0. Y0. Z5.\nG81 X10. Z-2. R1. F50.\nM30\n");
    for o in &p.ops {
        if let Op::Rapid {
            sub: Some(s),
            state,
            ..
        }
        | Op::Linear {
            sub: Some(s),
            state,
            ..
        } = o
        {
            if *s == 1 {
                assert!(state.feed_restated, "sub=1 обязан нести restated");
            } else {
                assert!(!state.feed_restated, "sub={s} без restated");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Слова без потребителя: A/B/C/E-диспозиции
// ---------------------------------------------------------------------------

#[test]
fn mround2_abc_word_dispositions_raw() {
    // Ротационные оси A/B/C → Raw{SEMANTICS} (stage 12), не Err.
    for w in ["A90.", "B45.", "C180."] {
        let p = parse(&format!("G0 X1. Y1. Z1.\n{w}\nM30\n")).expect("ok");
        assert!(
            matches!(
                p.ops.iter().find(|o| matches!(o, Op::Raw { .. })),
                Some(Op::Raw {
                    code: ErrorCode::UnsupportedSemantics,
                    ..
                })
            ),
            "{w} → Raw SEMANTICS"
        );
    }
}

#[test]
fn mround2_e_word_disposition_raw() {
    // E (G05.1 precision) → Raw{SYNTAX} (vendor mode).
    let p = parse("G0 X1. Y1. Z1.\nE10\nM30\n").expect("ok");
    assert!(matches!(
        p.ops.iter().find(|o| matches!(o, Op::Raw { .. })),
        Some(Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        })
    ));
}

// ---------------------------------------------------------------------------
// G10-формы: матрица диспозиций
// ---------------------------------------------------------------------------

#[test]
fn mround2_g10_forms_matrix() {
    let codes = |src: &str| {
        let p = parse(src).expect("ok");
        p.ops.iter().find_map(|o| match o {
            Op::Raw { code, .. } => Some(*code),
            _ => None,
        })
    };
    assert_eq!(
        codes("G10 L2 P2.5 X1. Y1. Z1.\nM30\n"),
        Some(ErrorCode::UnsupportedSyntax),
        "P вне домена (дробное) → SYN"
    );
    assert_eq!(
        codes("G10 L20 X1. Y1. Z1.\nM30\n"),
        Some(ErrorCode::RequiresMachineTable)
    );
    assert_eq!(
        codes("G10 L1 X1.\nM30\n"),
        Some(ErrorCode::RequiresMachineTable)
    );
    assert_eq!(
        codes("G10 L2.5 P1 X1. Y1. Z1.\nM30\n"),
        Some(ErrorCode::UnsupportedSyntax),
        "L не-целое → SYN"
    );
    // Частичная L2: X+Z (Y missing) и Y+Z (X missing) — разные ||-плеча.
    assert_eq!(
        codes("G10 L2 P2 X9. Z9.\nM30\n"),
        Some(ErrorCode::RequiresMachineTable)
    );
    assert_eq!(
        codes("G10 L2 P2 Y9. Z9.\nM30\n"),
        Some(ErrorCode::RequiresMachineTable)
    );
    // Без L / L2 без P → Err (сломанные формы).
    assert_eq!(
        err_code(parse("G10 X1. Y1. Z1.\nM30\n")),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse("G10 L2 X1. Y1. Z1.\nM30\n")),
        ErrorCode::InvalidInput
    );
}

// ---------------------------------------------------------------------------
// Дуги: R-формы (sgn), допуск хорды, near-degenerate, Родригес G18/G19
// ---------------------------------------------------------------------------

#[test]
fn mround2_arc_r_sgn_cw_ccw_and_negative() {
    // Знак R и направление: центр = середина хорды ± h·p̂ (хорда 8, R5,
    // h=3, p̂=(0,1)).
    let p = parse("G0 X0. Y0. Z0.\nG3 X8. Y0. Z0. R5. F100.\nM30\n").expect("ok CCW R+");
    let arc = p.ops.iter().find(|o| matches!(o, Op::Arc { .. })).unwrap();
    match arc {
        Op::Arc {
            center,
            sweep_deg,
            dir,
            ..
        } => {
            assert_pt(&center, 4.0, 3.0, 0.0);
            assert!(
                (sweep_deg.value() - 106.26).abs() <= 0.05,
                "sweep {sweep_deg:?}"
            );
            assert!(*dir == Dir::Ccw);
        }
        _ => panic!("не Arc"),
    }
    let p = parse("G0 X0. Y0. Z0.\nG2 X8. Y0. Z0. R5. F100.\nM30\n").expect("ok CW R+");
    let arc = p.ops.iter().find(|o| matches!(o, Op::Arc { .. })).unwrap();
    match arc {
        Op::Arc { center, dir, .. } => {
            assert_pt(&center, 4.0, -3.0, 0.0);
            assert!(*dir == Dir::Cw);
        }
        _ => panic!("не Arc"),
    }
    // R<0: дуга >180°, sgn перевёрнут; sweep = 360 − 106.26.
    let p = parse("G0 X0. Y0. Z0.\nG3 X8. Y0. Z0. R-5. F100.\nM30\n").expect("ok CCW R-");
    let arc = p.ops.iter().find(|o| matches!(o, Op::Arc { .. })).unwrap();
    match arc {
        Op::Arc {
            center, sweep_deg, ..
        } => {
            assert_pt(&center, 4.0, -3.0, 0.0);
            assert!(
                (sweep_deg.value() - 253.74).abs() <= 0.05,
                "sweep {sweep_deg:?}"
            );
        }
        _ => panic!("не Arc"),
    }
    // 106°-дуги — НЕ near-degenerate (notice только у ~180°).
    let (_, ns) = notices_of("G0 X0. Y0. Z0.\nG3 X8. Y0. Z0. R5. F100.\nM30\n");
    assert!(ns.iter().all(|n| n.kind != NoticeKind::NearDegenerateRArc));
}

#[test]
fn mround2_arc_r_near_degenerate_notice() {
    // R-дуга ровно 180° (chord == 2R): notice NearDegenerateRArc при живом
    // опе (амплитификация центра).
    let (p, ns) = notices_of("G0 X0. Y0. Z0.\nG3 X10. Y0. Z0. R5. F100.\nM30\n");
    assert!(
        ns.iter().any(|n| n.kind == NoticeKind::NearDegenerateRArc),
        "{ns:?}"
    );
    assert!(
        p.ops.iter().any(|o| matches!(o, Op::Arc { .. })),
        "оп живой"
    );
}

#[test]
fn mround2_arc_chord_barely_over_2r_tolerance() {
    // Хорда чуть больше 2R в допуске tol_r → легальна; за допуском → Err.
    let p = parse("G0 X0. Y0. Z0.\nG3 X10. Y0. Z0. R4.9998 F100.\nM30\n");
    assert!(p.is_ok(), "хорда 10 vs 2R 9.9996 — в допуске: {p:?}");
    let r = parse("G0 X0. Y0. Z0.\nG3 X10. Y0. Z0. R4.999 F100.\nM30\n");
    assert_eq!(err_code(r), ErrorCode::InvalidInput);
}

#[test]
fn mround2_arc_quarter_g18_rodrigues() {
    // G18 (ZX, a0=Z, a1=X, нормаль +Y): четверть CCW из (0,0,0), центр
    // (2,0,0) → конец (2,0,2). Все компоненты Rodrigues ненулевые.
    let p = parse("G0 X0. Y0. Z0.\nG18\nG1 X0. Y0. Z0. F100.\nG3 X2. Y0. Z2. I2. K0.\nM30\n")
        .expect("G18 четверть CCW ok");
    let arc = p.ops.iter().find(|o| matches!(o, Op::Arc { .. })).unwrap();
    match arc {
        Op::Arc {
            end,
            center,
            sweep_deg,
            ..
        } => {
            assert_pt(&center, 2.0, 0.0, 0.0);
            assert_pt(&end, 2.0, 0.0, 2.0);
            assert!(
                (sweep_deg.value() - 90.0).abs() <= 0.01,
                "sweep {sweep_deg:?}"
            );
        }
        _ => panic!("не Arc"),
    }
}

#[test]
fn mround2_arc_quarter_g19_rodrigues_cw() {
    // G19 (YZ, a0=Y, a1=Z, нормаль +X): четверть CW из (0,0,0), центр
    // (0,2,0) → конец (0,2,2).
    let p = parse("G0 X0. Y0. Z0.\nG19\nG1 X0. Y0. Z0. F100.\nG2 X0. Y2. Z2. J2. K0.\nM30\n")
        .expect("G19 четверть CW ok");
    let arc = p.ops.iter().find(|o| matches!(o, Op::Arc { .. })).unwrap();
    match arc {
        Op::Arc {
            end,
            center,
            sweep_deg,
            ..
        } => {
            assert_pt(&center, 0.0, 2.0, 0.0);
            assert_pt(&end, 0.0, 2.0, 2.0);
            assert!(
                (sweep_deg.value() - 90.0).abs() <= 0.01,
                "sweep {sweep_deg:?}"
            );
        }
        _ => panic!("не Arc"),
    }
}

// ---------------------------------------------------------------------------
// G93: 3D-диагональ (все компоненты len ненулевые), вертикальный feed цикла
// ---------------------------------------------------------------------------

#[test]
fn mround2_g93_linear_feed_len_3d_full() {
    // 3D-диагональ со ВСЕМИ ненулевыми дельтами: len = sqrt(9+16+16) = √41.
    let p = parse("G0 X1. Y2. Z3.\nG93\nG1 X4. Y6. Z7. F1.\nM30\n").expect("ok");
    let lin = p
        .ops
        .iter()
        .find(|o| matches!(o, Op::Linear { .. }))
        .unwrap();
    let expect = 41.0f64.sqrt();
    assert!(
        (feed_of(&lin).unwrap() - expect).abs() <= EPS,
        "feed {} vs {expect}",
        feed_of(&lin).unwrap()
    );
}

#[test]
fn mround2_g93_cycle_vertical_feed() {
    // G93 развёртка: вертикальный feed = |dz|·F = 6·2 = 12.
    let (p, _) = notices_of("G0 X0. Y0. Z5.\nG93\nG81 X10. Z-4. R2. F2.\nM30\n");
    let feeds: Vec<f64> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Linear { state, .. } => state.feed.as_ref().map(|f| f.value()),
            _ => None,
        })
        .collect();
    assert_eq!(feeds.len(), 1, "{feeds:?}");
    assert!((feeds[0] - 12.0).abs() <= EPS, "{feeds:?}");
}

#[test]
fn mround2_g93_unknown_start_err() {
    // G93 linear как первый ход: start неизвестен → E_RUNTIME_STATE.
    assert_eq!(
        err_code(parse("G93\nG1 X10. Y0. Z0. F2.\nM30\n")),
        ErrorCode::RuntimeState
    );
}

// ---------------------------------------------------------------------------
// Циклы: subs-нумерация, пек-последовательности, K-домен
// ---------------------------------------------------------------------------

fn subs_of(p: &Program) -> Vec<u32> {
    p.ops
        .iter()
        .filter_map(|o| match o {
            Op::Rapid { sub: Some(s), .. }
            | Op::Linear { sub: Some(s), .. }
            | Op::Dwell { sub: Some(s), .. } => Some(*s),
            _ => None,
        })
        .collect()
}

#[test]
fn mround2_subs_dense_g82_g85_g86() {
    // G82/G85/G86: dwell/feed-out/stop-ветки тоже инкрементируют sub —
    // последовательность плотная 1..=n.
    for (cycle, expect_len) in [
        ("G82 X5. Z-4. R2. P200 F100.", 5),
        ("G85 X5. Z-4. R2. F100.", 4),
        ("G86 X5. Z-4. R2. P200 F100.", 5),
    ] {
        let p = parse(&format!("G0 X0. Y0. Z5.\n{cycle}\nM30\n")).expect("ok");
        let subs = subs_of(&p);
        assert_eq!(
            subs,
            Vec::<u32>::from_iter(1..=expect_len as u32),
            "{cycle}: {subs:?}"
        );
    }
}

#[test]
fn mround2_subs_k2_default_retract() {
    // G81 K2: sub-инкремент ПОСЛЕ финального отвода обязан сработать —
    // второе отверстие продолжает нумерацию (5..8; traverse/approach
    // повторной дыры zero-drop'ятся, но потребляют sub).
    let p = parse("G0 X0. Y0. Z5.\nG81 X5. Z-4. R2. F100. K2\nM30\n").expect("ok");
    assert_eq!(subs_of(&p), vec![1, 2, 3, 4, 7, 8]);
}

#[test]
fn mround2_subs_k2_borefeedout_and_stop() {
    // G85 K2 (feed-out retract) и G86 K2 (stop retract): инкремент после
    // отвода виден на втором отверстии.
    let p = parse("G0 X0. Y0. Z5.\nG85 X5. Z-4. R2. F100. K2\nM30\n").expect("ok");
    assert_eq!(subs_of(&p), vec![1, 2, 3, 4, 7, 8], "G85 K2");
    // G86 несёт dwell: hole2 = traverse 6 (drop), approach 7 (drop),
    // feed 8, dwell 9, retract 10.
    let p = parse("G0 X0. Y0. Z5.\nG86 X5. Z-4. R2. P200 F100. K2\nM30\n").expect("ok");
    assert_eq!(subs_of(&p), vec![1, 2, 3, 4, 5, 8, 9, 10], "G86 K2");
}

#[test]
fn mround2_g83_peck_depths_g98() {
    // G83 G98: пеки d = max(z, r−kQ); промежуточные отводы на R + подход
    // d+0.254; финальный отвод на initial Z (5).
    let (p, _) = notices_of("G0 X0. Y0. Z5.\nG98 G83 X10. Z-9. R2. Q3. F100.\nM30\n");
    let zs: Vec<f64> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Rapid { end, .. } | Op::Linear { end, .. } => Some(end.z.value()),
            _ => None,
        })
        .collect();
    let expect = [
        5.0, 5.0, 2.0,    // G0 (z=5), traverse (z=5), approach → R (2)
        -1.0,   // feed peck 1
        2.0,    // retract R
        -0.746, // approach d+clr
        -4.0, 2.0, -3.746, -7.0, 2.0, -6.746, -9.0, 5.0, // G98 retract → initial Z
    ];
    assert_eq!(zs.len(), expect.len(), "ops: {zs:?}");
    for (a, b) in zs.iter().zip(expect.iter()) {
        assert!((a - b).abs() <= EPS, "z {a} vs {b}");
    }
}

#[test]
fn mround2_g73_chipbreak_single_rapid() {
    // G73: между пеками — ОДИН rapid на d+0.254 (без отвода на R).
    let (p, _) = notices_of("G0 X0. Y0. Z5.\nG73 X10. Z-6. R2. Q2. F100.\nM30\n");
    let zs: Vec<f64> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Rapid { end, .. } | Op::Linear { end, .. } => Some(end.z.value()),
            _ => None,
        })
        .collect();
    let expect = [
        5.0, 5.0, 2.0, // traverse (z=5), approach → R (2)... traverse и
        // подход: конечные z 5 и 2
        0.0, 0.254, // peck1 (2−2=0), approach
        -2.0, -1.746, // peck2, approach
        -4.0, -3.746, // peck3, approach
        -6.0,   // peck4 (final)
        2.0,    // retract G99
    ];
    assert_eq!(zs.len(), expect.len(), "ops: {zs:?}");
    for (a, b) in zs.iter().zip(expect.iter()) {
        assert!((a - b).abs() <= EPS, "z {a} vs {b}");
    }
}

#[test]
fn mround2_peck_est_limit_exceeded() {
    // Крошечный Q → оценка развёртки > max_unroll_iterations → Limit.
    // G83 (3 опа на пек) и G73 (2 опа на пек) — оба плеча ops_per_peck.
    let r = parse("G0 X0. Y0. Z5.\nG83 X10. Z-9. R2. Q0.001 F100.\nM30\n");
    assert_eq!(err_code(r), ErrorCode::LimitExceeded);
    // G73 на границе: n=5001 → est = 2+2·5001+1 = 10005 > 10000 → Limit.
    let r = parse("G0 X0. Y0. Z5.\nG73 X10. Z-10.002 R0. Q0.002 F100.\nM30\n");
    assert_eq!(err_code(r), ErrorCode::LimitExceeded);
    // G83 на границе: n=3334 → est = 2+3·3334+1 = 10005 > 10000 → Limit.
    let r = parse("G0 X0. Y0. Z5.\nG83 X10. Z-10.002 R0. Q0.003 F100.\nM30\n");
    assert_eq!(err_code(r), ErrorCode::LimitExceeded);
}

#[test]
fn mround2_peck_bottom_above_r_single_pass() {
    // z_bottom выше R (t ≤ 0): n = 0 → 1 — один feed-ход вверх к дну.
    let (p, _) = notices_of("G0 X0. Y0. Z5.\nG83 X10. Z2. R-1. Q3. F100.\nM30\n");
    let lins: Vec<f64> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Linear { end, .. } => Some(end.z.value()),
            _ => None,
        })
        .collect();
    assert_eq!(lins, vec![2.0], "один feed к z_bottom");
}

#[test]
fn mround2_peck_bottom_at_r_single_pass() {
    // z_bottom РОВНО на R (t == 0): n = 1 — один пек, feed до дна zero-length
    // (d = max(r−q, z) = r) → drop+notice; финальный retract G99 на R тоже
    // zero-length → notice. Итого 2 notice. Мутант `t > 0.0` → `>=` дал бы
    // n = ceil(0/q) = 0 → пек-ветка пропущена → только 1 notice (final
    // retract). Спутник mround2_peck_bottom_above_r_single_pass
    // (мутраунд-2: дедупликация нормализации n == 0).
    let (p, notices) = notices_of("G0 X0. Y0. Z5.\nG83 X10. Z2. R2. Q3. F100.\nM30\n");
    assert!(
        p.ops.iter().all(|o| !matches!(o, Op::Linear { .. })),
        "feed к дну == R — zero-length, в опах линейных нет"
    );
    assert_eq!(notices.len(), 2, "zero-пек + zero-retract: {notices:?}");
}

#[test]
fn mround2_peck_est_limit_exact_boundary_ok() {
    // est_total РОВНО 10000 на границе max_unroll_iterations → Ok (не Err):
    // G73 K=16, n = ceil(0.621/0.002) = ceil(310.5) = 311 → est на отверстие
    // 2+2·311+1 = 625 → 16·625 = 10000. Убивает мутантов `>` → `>=` /
    // `==` (est-лимит) и `+` → `*` (формула est: 2·2·311+1 = 1245 →
    // 16·1245 = 19920 > 10000 → Err — тест бы упал). Спутник
    // mround2_peck_est_limit_exceeded («ровно-на-границе», мутраунд-2).
    let p = parse("G0 X0. Y0. Z5.\nG73 X10. Z-0.621 R0. Q0.002 F100. K16\nM30\n")
        .expect("est_total == 10000 ровно — не превышение");
    assert_eq!(p.ops.len() > 1000, true, "развёртка состоялась");
}

#[test]
fn mround2_cycle_param_update_block_only() {
    // Параметр-кадр (без X/Y) не сверлит; обновлённый Z виден на повторе.
    let p = parse("G0 X0. Y0. Z5.\nG81 X10. Z-2. R1. F100.\nG81 Z-3.\nX20.\nM30\n").expect("ok");
    let lins: Vec<f64> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Linear { end, .. } => Some(end.z.value()),
            _ => None,
        })
        .collect();
    assert_eq!(
        lins.len(),
        2,
        "параметр-кадр без X/Y не разворачивается: {lins:?}"
    );
    assert!(lins.contains(&-2.0) && lins.contains(&-3.0), "{lins:?}");
}

#[test]
fn mround2_cycle_k0_no_unroll() {
    // K0 в G90: отверстий нет, линейных feed-ходов нет.
    let p = parse("G0 X0. Y0. Z5.\nG81 X10. Z-2. R1. F100. K0\nM30\n").expect("ok");
    assert!(p.ops.iter().all(|o| !matches!(o, Op::Linear { .. })));
}

#[test]
fn mround2_cycle_k_domain() {
    // K: дробное/отрицательное → Err; 10001 → Limit; 9999 и 10000 — легальны
    // (границы домена 0..9999 и max_unroll_iterations).
    assert_eq!(
        err_code(parse("G0 X0. Y0. Z5.\nG81 X10. Z-2. R1. F100. K0.5\nM30\n")),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse("G0 X0. Y0. Z5.\nG81 X10. Z-2. R1. F100. K-1\nM30\n")),
        ErrorCode::InvalidInput
    );
    // 10000/10001 — вне домена 0..9999 → InvalidInput (домен уже лимита
    // развёртки; LimitExceeded достижим через est_total пеков — см.
    // mround2_peck_est_limit_exceeded).
    assert_eq!(
        err_code(parse(
            "G0 X0. Y0. Z5.\nG81 X10. Z-2. R1. F100. K10000\nM30\n"
        )),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse(
            "G0 X0. Y0. Z5.\nG81 X10. Z-2. R1. F100. K10001\nM30\n"
        )),
        ErrorCode::InvalidInput
    );
    parse("G0 X0. Y0. Z5.\nG81 X10. Z-2. R1. F100. K9999\nM30\n").expect("K9999 ok");
}

#[test]
fn mround2_cycle_g86_spindle_ccw_restart() {
    // G86 рестарт шпинделя в ИСХОДНОМ направлении (M4) — state последующих
    // опов несёт Ccw.
    let p = parse("S500 M4\nG0 X0. Y0. Z5.\nG86 X10. Z-5. R1. P200 F60.\nG0 X20. Y0. Z5.\nM30\n")
        .expect("ok");
    let last = p
        .ops
        .iter()
        .rev()
        .find(|o| matches!(o, Op::Rapid { .. }))
        .unwrap();
    match last {
        Op::Rapid { state, .. } => {
            assert!(
                matches!(state.spindle, Spindle::Ccw(_)),
                "{:?}",
                state.spindle
            );
        }
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Движение: X-only явный ход; zero-length rapid вид опа; зеркальный ход;
// G53 частичный end; G98→G99
// ---------------------------------------------------------------------------

#[test]
fn mround2_single_axis_explicit_move() {
    // Явный G1 с ОДНИМ осевым словом — ход, не no-op (has_axis X-плечо).
    let p = parse("G0 X5. Y0. Z0.\nG1 X10. F100.\nM30\n").expect("ok");
    let lins = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Linear { .. }))
        .count();
    assert_eq!(lins, 1, "X-only G1 обязан дать linear-оп");
}

#[test]
fn mround2_zero_length_rapid_notice_op_kind() {
    // zero-length rapid: notice хранит Rapid-оп (не Linear).
    let (_, ns) = notices_of("G0 X5. Y0. Z0.\nG0 X5. Y0. Z0.\nM30\n");
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].kind, NoticeKind::ZeroLengthMotion);
    assert!(
        matches!(ns[0].op, Op::Rapid { .. }),
        "zero-drop rapid: {:?}",
        ns[0].op
    );
}

#[test]
fn mround2_mirrored_move_not_dropped() {
    // Ход в зеркальную позицию (end == −start): НЕ zero-length
    // (start+end-инверсия len3 не должна дать 0).
    let p = parse("G0 X1. Y0. Z0.\nG0 X-1. Y0. Z0.\nM30\n").expect("ok");
    let rapids = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Rapid { .. }))
        .count();
    assert_eq!(rapids, 2, "оба rapid живые");
}

#[test]
fn mround2_g53_partial_axis_missing_err() {
    // machine-ход с частичным end (Y отсутствует) → E_RUNTIME_STATE даже
    // при известном prev (нуль в machine-координатах не наследуется).
    assert_eq!(
        err_code(parse(
            "G10 L2 P1 X10. Y0. Z0.\nG53 G0 X100. Y100. Z100.\nG53 G0 X90. Y90.\nM30\n"
        )),
        ErrorCode::RuntimeState
    );
}

#[test]
fn mround2_g98_then_g99_retract_restored() {
    // G98 → G99: отвод возвращается к R-плоскости (RetractG99-плечо apply).
    let p = parse("G0 X0. Y0. Z5.\nG98\nG99\nG81 X10. Z-4. R2. F100.\nM30\n").expect("ok");
    let rapids: Vec<f64> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Rapid { end, .. } => Some(end.z.value()),
            _ => None,
        })
        .collect();
    let last = *rapids.last().expect("rapid отвода");
    assert!(
        (last - 2.0).abs() <= EPS,
        "отвод на R, не initial Z: {rapids:?}"
    );
}

// ---------------------------------------------------------------------------
// Лимиты: строки 1e6, ops ровно 200000 / сверх
// ---------------------------------------------------------------------------

#[test]
fn mround2_lines_limit_1m() {
    let mut s = String::with_capacity(2_000_020);
    for _ in 0..1_000_001 {
        s.push_str("%\n");
    }
    let r = parse(&s);
    assert_eq!(err_code(r), ErrorCode::LimitExceeded);
}

/// 4×G81-K10000 (по 40000 опов, разные X/R — без zero-drop) + G81-K9999
/// (39996) + 2×G4 P1 + стартовый G0 + PEND = ровно 200000 опов, 0 notices.
#[test]
fn mround2_ops_limit_exactly_200k_ok() {
    // 5 блоков G81 K9999 (по 39996 опов) + G0 + 18×G4 P1 + PEND =
    // 1 + 199980 + 18 + 1 = ровно 200000 опов.
    let mut s = String::from("G0 X0. Y0. Z5.\n");
    for i in 0..5 {
        s.push_str(&format!("G81 X{}. Z-1. R{}. F100. K9999\n", i + 1, 2 + i));
    }
    for _ in 0..18 {
        s.push_str("G4 P1\n");
    }
    s.push_str("M30\n");
    // Повторы K9999 на той же X/Y дают zero-drop notice'ы (по 2 на дыру) —
    // это легально; важно, что (ops+notices) == 200000 ровно и парс Ok.
    let (p, ns) = notices_of(&s);
    assert_eq!(ns.len(), 99980, "zero-drop повторов: {}", ns.len());
    let _ = p; // Ok — ровно на лимите (не свыше)
}

#[test]
fn mround2_ops_limit_over_200k_err() {
    let mut s = String::from("G0 X0. Y0. Z5.\n");
    for i in 0..6 {
        s.push_str(&format!("G81 X{}. Z-1. R{}. F100. K9999\n", i + 1, 2 + i));
    }
    s.push_str("M30\n");
    let r = parse(&s);
    assert_eq!(err_code(r), ErrorCode::LimitExceeded);
}

// ---------------------------------------------------------------------------
// span: col_end-контракт; семья G10-raw по неинтегральным значениям
// ---------------------------------------------------------------------------

#[test]
fn mround2_span_col_end() {
    let src = "G0 X1. Y2. Z3.\nM30\n";
    let p = parse(src).expect("ok");
    let rapid = p
        .ops
        .iter()
        .find(|o| matches!(o, Op::Rapid { .. }))
        .unwrap();
    match rapid {
        Op::Rapid { src: sp, .. } => {
            assert_eq!(sp.line_start, 1);
            assert_eq!(sp.col_start, 1);
            assert_eq!(sp.line_end, 1);
            // blen = len+EOB → col_end = 16 (len("G0 X1. Y2. Z3.\n") + 1).
            assert_eq!(sp.col_end, 16);
        }
        _ => unreachable!(),
    }
}

#[test]
fn mround2_g43_family_extra_decimals_not_family() {
    // G43.09: round(value*10) != value*10 — НЕ член семьи G49 (интегральный
    // критерий t); G49 после него — IGNORED_SAFE (без Raw).
    let p = parse("G43.09\nG49\nG0 X1. Y1. Z1.\nM30\n").expect("ok");
    let raws = p.ops.iter().filter(|o| matches!(o, Op::Raw { .. })).count();
    assert_eq!(raws, 1, "только G43.09-строка raw; G49 после неё — no-op");
    match p.ops.first().unwrap() {
        Op::Raw { code, .. } => {
            // Неинтегральное значение не матчится в raw-таблицу → generic.
            assert_eq!(*code, ErrorCode::UnsupportedSyntax);
        }
        other => panic!("ожидался Raw: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// state.rs g_raw_disposition: таблица RAW-диспозиций (код + причина).
// Каждая строка таблицы — отдельный контракт COVERAGE §2/§3 («не молча»).
// ---------------------------------------------------------------------------

fn raw_of(src: &str) -> (ErrorCode, String) {
    let p = parse(src).expect("ok");
    match p.ops.iter().find(|o| matches!(o, Op::Raw { .. })) {
        Some(Op::Raw { code, reason, .. }) => (*code, reason.clone()),
        other => panic!("нет Raw-опа для {src}: {other:?}"),
    }
}

#[test]
fn mround2_raw_table_home_and_machine_table() {
    let cases: &[(f64, &str, ErrorCode)] = &[
        (
            27.0,
            "G27 reference return check",
            ErrorCode::RequiresMachineTable,
        ),
        (
            28.0,
            "G28 reference return via intermediate",
            ErrorCode::RequiresMachineTable,
        ),
        (
            29.0,
            "G29 return from reference",
            ErrorCode::RequiresMachineTable,
        ),
        (
            30.0,
            "G30 2nd/3rd/4th reference",
            ErrorCode::RequiresMachineTable,
        ),
        (
            30.1,
            "G30.1 floating reference",
            ErrorCode::RequiresMachineTable,
        ),
        (
            45.0,
            "±H-offset positioning",
            ErrorCode::RequiresMachineTable,
        ),
        (
            46.0,
            "±H-offset positioning",
            ErrorCode::RequiresMachineTable,
        ),
        (
            47.0,
            "±H-offset positioning",
            ErrorCode::RequiresMachineTable,
        ),
        (
            48.0,
            "±H-offset positioning",
            ErrorCode::RequiresMachineTable,
        ),
    ];
    for (v, frag, code) in cases {
        let src = format!("G{v}\nM30\n");
        let (c, reason) = raw_of(&src);
        assert_eq!(&c, code, "G{v}: {reason}");
        assert!(reason.contains(frag), "G{v}: {reason}");
    }
}

#[test]
fn mround2_raw_table_path_css_transforms() {
    let cases: &[(f64, &str)] = &[
        (9.0, "G09 exact stop one-shot"),
        (60.0, "G60 one-shot positioning"),
        (62.0, "G62 automatic corner override"),
        (63.0, "G63 tapping mode"),
        (96.0, "G96 constant surface speed"),
        (97.0, "G97 CSS cancel"),
        (68.0, "G68 coordinate system rotation"),
        (69.0, "G69 rotation cancel"),
        (50.0, "G50 scaling cancel"),
        (51.0, "G51 scaling"),
        (50.1, "G50.1 mirror cancel"),
        (51.1, "G51.1 programmable mirror"),
        (51.2, "G51.2 polygon turning"),
        (50.2, "G50.2 polygon turning cancel"),
        (15.0, "G15 polar coordinates cancel"),
        (16.0, "G16 polar coordinates"),
    ];
    for (v, frag) in cases {
        let src = format!("G{v}\nM30\n");
        let (c, reason) = raw_of(&src);
        assert_eq!(c, ErrorCode::UnsupportedSemantics, "G{v}: {reason}");
        assert!(reason.contains(frag), "G{v}: {reason}");
    }
}

#[test]
fn mround2_raw_table_limits_sensors_threading() {
    let cases: &[(f64, &str, ErrorCode)] = &[
        (
            22.0,
            "G22 stored stroke check",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            23.0,
            "G23 stroke check off",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            25.0,
            "G25 spindle fluctuation",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            26.0,
            "G26 spindle fluctuation",
            ErrorCode::UnsupportedSemantics,
        ),
        (31.0, "G31 skip function", ErrorCode::UnsupportedSemantics),
        (31.8, "G31.8 EGB-skip", ErrorCode::UnsupportedSyntax),
        (
            37.0,
            "G37 auto tool length measurement",
            ErrorCode::UnsupportedSemantics,
        ),
        (33.0, "G33 threading", ErrorCode::UnsupportedSemantics),
        (34.0, "G34 variable lead", ErrorCode::UnsupportedSemantics),
        (
            35.0,
            "G35 circular threading",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            36.0,
            "G36 circular threading",
            ErrorCode::UnsupportedSemantics,
        ),
        (2.2, "G02.2 evolvent", ErrorCode::UnsupportedSemantics),
        (3.2, "G03.2 evolvent", ErrorCode::UnsupportedSemantics),
        (2.3, "G02.3 exponential", ErrorCode::UnsupportedSemantics),
        (3.3, "G03.3 exponential", ErrorCode::UnsupportedSemantics),
        (
            2.4,
            "G02.4 3D coordinate conversion",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            3.4,
            "G03.4 3D coordinate conversion",
            ErrorCode::UnsupportedSemantics,
        ),
        (6.2, "G06.2 NURBS", ErrorCode::UnsupportedSemantics),
    ];
    for (v, frag, code) in cases {
        let src = format!("G{v}\nM30\n");
        let (c, reason) = raw_of(&src);
        assert_eq!(&c, code, "G{v}: {reason}");
        assert!(reason.contains(frag), "G{v}: {reason}");
    }
}

#[test]
fn mround2_raw_table_vendor_modes() {
    let cases: &[(f64, &str, ErrorCode)] = &[
        (5.0, "G05 AI contour", ErrorCode::UnsupportedSyntax),
        (
            5.1,
            "G05.1 AI contour control",
            ErrorCode::UnsupportedSyntax,
        ),
        (8.0, "G08 Nano smoothing", ErrorCode::UnsupportedSyntax),
        (
            7.0,
            "G07 hypothetical axis",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            7.1,
            "G07.1 cylindrical interpolation",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            12.1,
            "G12.1 polar interpolation",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            13.1,
            "G13.1 polar interpolation cancel",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            53.1,
            "G53.1 tool axis direction control",
            ErrorCode::UnsupportedSemantics,
        ),
        (68.2, "G68.2 feature CS", ErrorCode::UnsupportedSemantics),
        (
            10.6,
            "G10.6 retract & recover",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            10.9,
            "G10.9 diameter/radius switch",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            72.1,
            "G72.1 figure rotation copy",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            72.2,
            "G72.2 figure translation copy",
            ErrorCode::UnsupportedSemantics,
        ),
    ];
    for (v, frag, code) in cases {
        let src = format!("G{v}\nM30\n");
        let (c, reason) = raw_of(&src);
        assert_eq!(&c, code, "G{v}: {reason}");
        assert!(reason.contains(frag), "G{v}: {reason}");
    }
}

#[test]
fn mround2_raw_table_cycles_refused() {
    let cases: &[(f64, &str, ErrorCode)] = &[
        (
            74.0,
            "G74 left-hand tapping",
            ErrorCode::UnsupportedSemantics,
        ),
        (76.0, "G76 fine boring", ErrorCode::UnsupportedSemantics),
        (
            84.0,
            "G84 right-hand tapping",
            ErrorCode::UnsupportedSemantics,
        ),
        (84.2, "G84.2 rigid tapping", ErrorCode::UnsupportedSemantics),
        (84.3, "G84.3 rigid tapping", ErrorCode::UnsupportedSemantics),
        (87.0, "G87 back boring", ErrorCode::UnsupportedSemantics),
        (
            88.0,
            "G88 bore manual stop",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            80.5,
            "G80.5 high-speed retract",
            ErrorCode::UnsupportedSyntax,
        ),
        (
            80.8,
            "G80.8 high-speed retract",
            ErrorCode::UnsupportedSyntax,
        ),
        (81.1, "G81.1 chopping", ErrorCode::UnsupportedSyntax),
        (81.5, "G81.5 EGB", ErrorCode::UnsupportedSyntax),
        (81.8, "G81.8 EGB", ErrorCode::UnsupportedSyntax),
    ];
    for (v, frag, code) in cases {
        let src = format!("G{v}\nM30\n");
        let (c, reason) = raw_of(&src);
        assert_eq!(&c, code, "G{v}: {reason}");
        assert!(reason.contains(frag), "G{v}: {reason}");
    }
}

#[test]
fn mround2_raw_table_g92_g54_family() {
    // v0.20: G92.1/.2/.3 — SEMANTICS→TABLE (зонд 4); G54.1 — SEMANTICS.
    let cases: &[(f64, &str, ErrorCode)] = &[
        (
            92.1,
            "G92.1 WCS-shift reset",
            ErrorCode::RequiresMachineTable,
        ),
        (
            92.2,
            "G92.2 WCS-shift suspend",
            ErrorCode::RequiresMachineTable,
        ),
        (
            92.3,
            "G92.3 WCS-shift resume",
            ErrorCode::RequiresMachineTable,
        ),
        (
            54.1,
            "G54.1 Pn extended WCS",
            ErrorCode::UnsupportedSemantics,
        ),
    ];
    for (v, frag, code) in cases {
        let src = format!("G{v}\nM30\n");
        let (c, reason) = raw_of(&src);
        assert_eq!(&c, code, "G{v}: {reason}");
        assert!(reason.contains(frag), "G{v}: {reason}");
    }
}

#[test]
fn mround2_raw_table_macro_b() {
    let cases: &[(f64, &str, ErrorCode)] = &[
        (65.0, "G65 macro call", ErrorCode::UnsupportedSyntax),
        (66.0, "G66 modal macro call", ErrorCode::UnsupportedSyntax),
        (66.1, "G66.1 modal macro call", ErrorCode::UnsupportedSyntax),
        (67.0, "G67 modal macro cancel", ErrorCode::UnsupportedSyntax),
    ];
    for (v, frag, code) in cases {
        let src = format!("G{v}\nM30\n");
        let (c, reason) = raw_of(&src);
        assert_eq!(&c, code, "G{v}: {reason}");
        assert!(reason.contains(frag), "G{v}: {reason}");
    }
}

#[test]
fn mround2_raw_table_compensation_tlo() {
    let cases: &[(f64, &str, ErrorCode)] = &[
        (
            41.0,
            "G41 cutter comp left",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            42.0,
            "G42 cutter comp right",
            ErrorCode::UnsupportedSemantics,
        ),
        (41.1, "G41.1 cutter comp", ErrorCode::UnsupportedSemantics),
        (42.1, "G42.1 cutter comp", ErrorCode::UnsupportedSemantics),
        (41.2, "high-precision comp", ErrorCode::UnsupportedSemantics),
        (41.6, "high-precision comp", ErrorCode::UnsupportedSemantics),
        (42.2, "high-precision comp", ErrorCode::UnsupportedSemantics),
        (42.6, "high-precision comp", ErrorCode::UnsupportedSemantics),
        (38.0, "G38 preserve vector", ErrorCode::UnsupportedSemantics),
        (39.0, "G39 corner circular", ErrorCode::UnsupportedSemantics),
        (38.2, "probing", ErrorCode::UnsupportedSemantics),
        (38.5, "probing", ErrorCode::UnsupportedSemantics),
        (
            43.0,
            "G43 tool length offset +H",
            ErrorCode::RequiresMachineTable,
        ),
        (43.1, "G43.1 dynamic TLO", ErrorCode::RequiresMachineTable),
        (43.4, "G43.4 TCP", ErrorCode::UnsupportedSemantics),
        (
            43.5,
            "G43.5 tool axis direction",
            ErrorCode::UnsupportedSemantics,
        ),
        (
            44.0,
            "G44 tool length offset",
            ErrorCode::RequiresMachineTable,
        ),
    ];
    for (v, frag, code) in cases {
        let src = format!("G{v}\nM30\n");
        let (c, reason) = raw_of(&src);
        assert_eq!(&c, code, "G{v}: {reason}");
        assert!(reason.contains(frag), "G{v}: {reason}");
    }
}

// ---------------------------------------------------------------------------
// МУТ-РАУНД 2, батч 5: добивающие (поштучные выжившие baseline-списка)
// ---------------------------------------------------------------------------

#[test]
fn mround2_modal_y_z_only_continuation() {
    // Модальное продолжение с ОДНОЙ осью: Y-only и Z-only кадры — ходы, не
    // naked-coords Err (плеча Y||Z в has_axis).
    let p = parse("G0 X5. Y2. Z1.\nG1 X4. Y2. Z1. F50.\nY10.\nM30\n").expect("Y-only ok");
    let lins = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Linear { .. }))
        .count();
    assert_eq!(lins, 2, "Y-only продолжение — ход (X-кадр + Y-кадр)");
    let p = parse("G0 X5. Y2. Z1.\nG1 X4. Y2. Z1. F50.\nZ-1.\nM30\n").expect("Z-only ok");
    let lins = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Linear { .. }))
        .count();
    assert_eq!(lins, 2, "Z-only продолжение — ход");
    // G91: Z-only — шаг между отверстиями (легален по §5.1).
    let p = parse("G0 X5. Y2. Z1.\nG91\nG0 Z-2.\nZ-3.\nM30\n").expect("G91 Z-only ok");
}

#[test]
fn mround2_sub3_mirror_3d() {
    // 3D-зеркальный ход (end == −start по всем осям): НЕ zero-length ни по
    // одной компоненте (sub3-инверсия по Y/Z не должна дать 0).
    let p = parse("G0 X1. Y2. Z3.\nG0 X-1. Y-2. Z-3.\nM30\n").expect("ok");
    let rapids = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Rapid { .. }))
        .count();
    assert_eq!(rapids, 2, "оба rapid живые (3D-зеркало)");
}

#[test]
fn mround2_g90_restores_absolute() {
    // G91 → G90: возврат в абсолют (плечо DistAbs в ModalState::apply).
    let p = parse("G0 X5. Y0. Z0.\nG91\nG0 X2. Y0. Z0.\nG90\nG0 X10. Y0. Z0.\nM30\n").expect("ok");
    // end последнего хода = 10 (абсолют), не 7+2=9 (инкрементальное наследие):
    let ends: Vec<[f64; 3]> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Rapid { end, .. } => Some(pt3(&end)),
            _ => None,
        })
        .collect();
    assert_eq!(ends.last(), Some(&[10.0, 0.0, 0.0]), "{ends:?}");
}

#[test]
fn mround2_g91_1_restores_incremental_center() {
    // G90.1 → G91.1: возврат к инкрементальному центру (плечо ArcCenterInc).
    // G90.1 (абс. центр (0,5)) — дуга сходится; G91.1 (инкрем. от старта)
    // с теми же словами — другой центр, но тоже валидный.
    parse(
        "G0 X10. Y0. Z0.\nG90.1\nG2 X-5. Y-5. I0. J5. F100.\nG91.1\nG2 X-10. Y-10. I-5. J0. F100.\nM30\n",
    )
    .expect("G91.1-restore ok");
    // Различитель: после G91.1 центр = start + offset; невыполнение правила
    // даёт radius-mismatch Err (позитивный тест доказывает плечо).
    let p = parse(
        "G0 X10. Y0. Z0.\nG90.1\nG2 X-5. Y-5. I0. J5. F100.\nG91.1\nG2 X-10. Y-10. I-5. J0. F100.\nM30\n",
    )
    .expect("ok");
    let arcs: Vec<[f64; 3]> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Arc { center, .. } => Some(pt3(&center)),
            _ => None,
        })
        .collect();
    // Вторая дуга: start (−5,−5), инкремент I−5 J0 → центр (−10,−5).
    assert_eq!(arcs.last(), Some(&[-10.0, -5.0, 0.0]), "{arcs:?}");
}

#[test]
fn mround2_lines_exactly_1m_ok() {
    // Ровно 1M строк — НА лимите (не свыше) → Ok.
    let mut s = String::with_capacity(2_000_000);
    for _ in 0..1_000_000 {
        s.push_str("%\n");
    }
    parse(&s).expect("ровно 1M строк — в лимите");
}

#[test]
fn mround2_g53_partial_y_missing_xz_err() {
    // machine-ход без Y (X и Z есть) → E_RUNTIME_STATE (первое ||-плечо
    // полного-end правила).
    assert_eq!(
        err_code(parse(
            "G10 L2 P1 X10. Y0. Z0.\nG53 G0 X100. Y100. Z100.\nG53 G0 X90. Z90.\nM30\n"
        )),
        ErrorCode::RuntimeState
    );
}

#[test]
fn mround2_g92_absorb_y_only_raw_block() {
    // G92 Y-only на raw-кадре: пресетка (0,5,0) абсорбируется (плечо Y||Z).
    let p = parse("G91\nG92 Y5. G95\nG0 X1. Y-2. Z1.\nM30\n").expect("ok");
    let mv = p
        .ops
        .iter()
        .find(|o| matches!(o, Op::Rapid { .. }))
        .expect("ход после raw-G92");
    // Пресетка (0,5,0) — якорь; G91 абсорбирован → end = start + слова.
    assert_eq!(known(&mv), [0.0, 5.0, 0.0]);
    match mv {
        Op::Rapid { end, .. } => assert_pt(&end, 1.0, 3.0, 1.0),
        _ => unreachable!(),
    }
}

#[test]
fn mround2_g10_p0_no_g11_family_trigger() {
    // G10 L2 P0 (external offset) — RAW-форма → триггерит семью G11:
    // последующий G11 обязан быть Raw (плечо P-домена в g10_form_is_raw).
    let p = parse("G10 L2 P0 X1. Y1. Z1.\nG11\nG0 X1. Y1. Z1.\nM30\n").expect("ok");
    assert!(
        p.ops.iter().any(|o| matches!(
            o,
            Op::Raw {
                code: ErrorCode::UnsupportedSyntax,
                ..
            }
        )),
        "G11 после RAW-формы G10 (P0) — Raw"
    );
    // Контраст: легальная L2 P2 семью НЕ триггерит (существующий
    // g11_cancel_ignored — тут дублируем с P2).
    let p = parse("G10 L2 P2 X1. Y1. Z1.\nG11\nG0 X1. Y1. Z1.\nM30\n").expect("ok");
    assert!(p.ops.iter().all(|o| !matches!(o, Op::Raw { .. })));
}

#[test]
fn mround2_arc_sweep60_all_terms() {
    // G17 60°-дуга: cos ≠ 0 → все члены уравнения позиции (rv·cos, cr·sin)
    // ненулевые и конечные. end (5, 8.6603): sweep ≈ 60.0°.
    let p =
        parse("G0 X10. Y0. Z0.\nG3 X5. Y8.6603 Z0. I-10. J0. F100.\nM30\n").expect("60° дуга ok");
    let arc = p.ops.iter().find(|o| matches!(o, Op::Arc { .. })).unwrap();
    match arc {
        Op::Arc {
            center, sweep_deg, ..
        } => {
            assert_pt(&center, 0.0, 0.0, 0.0);
            assert!(
                (sweep_deg.value() - 60.0).abs() <= 0.05,
                "sweep {sweep_deg:?}"
            );
        }
        _ => panic!("не Arc"),
    }
    // CW-вариант из той же точки (зеркало по направлению).
    let p =
        parse("G0 X10. Y0. Z0.\nG2 X5. Y-8.6603 Z0. I-10. J0. F100.\nM30\n").expect("CW 60° ok");
    let arc = p.ops.iter().find(|o| matches!(o, Op::Arc { .. })).unwrap();
    match arc {
        Op::Arc { sweep_deg, dir, .. } => {
            assert!((sweep_deg.value() - 60.0).abs() <= 0.05);
            assert!(*dir == Dir::Cw);
        }
        _ => panic!("не Arc"),
    }
}

#[test]
fn mround2_peck_est_boundary_3333_and_4999() {
    // Границы est-формулы: est корректный чуть выше лимита; мутанты арифметики
    // (−, /, +→…) дают est ниже лимита → тест различает.
    // G83: n = 3333 → est = 2+3·3333+1 = 10002 > 10000 → Limit.
    let r = parse("G0 X0. Y0. Z5.\nG83 X5. Z-9.999 R0. Q0.003 F100.\nM30\n");
    assert_eq!(err_code(r), ErrorCode::LimitExceeded);
    // G73: n = 4999 → est = 2+2·4999+1 = 10001 > 10000 → Limit.
    let r = parse("G0 X0. Y0. Z5.\nG73 X5. Z-9.997 R0. Q0.002 F100.\nM30\n");
    assert_eq!(err_code(r), ErrorCode::LimitExceeded);
}
