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

use eid_core::ast::{Op, Point3, Program, StartPoint, Units};
use eid_core::diag::{ErrorCode, Stage};
use eid_core::diag::Diagnostic;
use eid_fanuc::canonicalize::{canonicalize, canonicalize_with_notices, NoticeKind};

const EPS: f64 = 0.001;

fn parse(src: &str) -> Result<Program, Vec<Diagnostic>> {
    canonicalize(src, "test.ngc")
}

fn err_code(r: Result<Program, Vec<Diagnostic>>) -> ErrorCode {
    let Err(d) = r else { panic!("ожидался Err") };
    d[0].code
}

fn err_stage(r: Result<Program, Vec<Diagnostic>>) -> Stage {
    let Err(d) = r else { panic!("ожидался Err") };
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
    assert!(matches!(&p.ops[0], Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }));
    assert!(matches!(&p.ops[1], Op::Rapid { start: StartPoint::Unknown, .. }));
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
    // M5 — вне словаря этапа 1 → provisional Raw (M-матрица — этап 2),
    // программа продолжает парситься (правило двух классов).
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG1 X-10. Y10. F250.;\nM5;\nM30;\n").expect("ok");
    assert_eq!(p.ops.len(), 4); // Rapid, Linear, Raw(M5), ProgramEnd
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    assert!(matches!(&p.ops[1], Op::Linear { .. }));
    assert!(matches!(&p.ops[2], Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }));
    assert!(matches!(&p.ops[3], Op::ProgramEnd { .. }));
    match &p.ops[1] {
        Op::Linear { end, .. } => assert_pt(end, -10.0, 10.0, 5.0),
        _ => unreachable!(),
    }
}

#[test]
fn auto_pend_missing_program_end() {
    // SPEC §3 v0.16 / анализ §4.4: DNC-файл без M30 — неявный конец.
    let (p, notices) = notices_of("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG1 X10. F150.;\n");
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    assert!(matches!(&p.ops[1], Op::Linear { .. }));
    assert!(matches!(p.ops.last().expect("non-empty"), Op::ProgramEnd { .. }));
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
    let dpi: Vec<_> = notices.iter().filter(|n| n.kind == NoticeKind::DpiAssumed).collect();
    assert!(dpi.len() >= 3, "X25/Y0/Z0 — три bare-слова");
    assert!(dpi.iter().any(|n| n.message.contains("W_DPI_ASSUMED")
        && n.message.contains("X25") && n.message.contains("0.025")));
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
    // Дуга со всеми bare-словами: равномерное сжатие ×0.001 сохраняет геометрию
    // (X-5000→-5, Y5000→5, I0→0, J5000→5, F100000→100).
    let (p, notices) = notices_of("G21 G90 G94 G17;\nG0 X0 Y0 Z0;\nG2 X-5000 Y5000 I0 J5000 F100000;\nM30;\n");
    match &p.ops[1] {
        Op::Arc { center, sweep_deg, dir, .. } => {
            assert_pt(center, 0.0, 5.0, 0.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS, "sweep {}", sweep_deg.value());
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
        }
        _ => panic!("op1 не Arc"),
    }
    assert_eq!(feed_of(&p.ops[1]), Some(100.0));
    let dpi = notices.iter().filter(|n| n.kind == NoticeKind::DpiAssumed).count();
    assert!(dpi >= 5, "все слова дуги bare: {dpi}");
}

#[test]
fn dpi_pending_f_flushes_to_next_op() {
    // F-only блок (нет движения): DPI-notice висит до следующего опа.
    let (p, notices) = notices_of("G21 G90 G94;\nF100;\nG1 X5.;\nM2;\n");
    assert_eq!(p.ops.len(), 2); // Linear + ProgramEnd
    assert_eq!(feed_of(&p.ops[0]), Some(0.1)); // F100 bare → 0.1
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, NoticeKind::DpiAssumed);
    assert_eq!(notices[0].op_index, 0, "notice прицеплен к Linear");
    assert!(notices[0].message.contains("F100"));
}

// ---------------------------------------------------------------------------
// Дуги §6 — все формы
// ---------------------------------------------------------------------------

#[test]
fn arc_ijk_cw_helix() {
    // IJK инкрементальные от start (§2.1), helical Z→pitch, G17 → normal +Z.
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X-5. Y5. Z2. I0. J5. F100.;\nM30;\n").expect("ok");
    assert!(matches!(&p.ops[0], Op::Rapid { start: StartPoint::Unknown, .. }));
    match &p.ops[1] {
        Op::Arc { start, end, center, normal, dir, sweep_deg, pitch, state, .. } => {
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
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG3 X5. Y-5. Z-2. I5. J0. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, normal, dir, sweep_deg, pitch, .. } => {
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
    let p = parse("G21 G90 G94 G18;\nG0 X0. Y0. Z0.;\nG2 X5. Y2. Z-5. I5. K0. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, normal, sweep_deg, pitch, .. } => {
            assert_pt(center, 5.0, 0.0, 0.0);
            assert_pt(normal, 0.0, 1.0, 0.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS, "sweep {}", sweep_deg.value());
            assert!((pitch.value() - 2.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_g19() {
    // G19 = YZ: пара J(Y)+K(Z), нормаль +X, linear X.
    let p = parse("G21 G90 G94 G19;\nG0 X0. Y0. Z0.;\nG3 X2. Y5. Z-5. J5. K0. F100.;\nM30;\n").expect("ok");
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
fn arc_r_positive() {
    // R>0 → дуга ≤180°: из (0,10) в (10,0) по R10 — четверть вокруг (0,0).
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y10. Z0.;\nG2 X10. Y0. R10. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, sweep_deg, pitch, .. } => {
            assert_pt(center, 0.0, 0.0, 0.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS, "sweep {}", sweep_deg.value());
            assert!(pitch.value().abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_r_negative() {
    // R<0 → дуга >180° (знак R выбирает сторону; SPEC §6).
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y10. Z0.;\nG2 X10. Y0. R-10. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, sweep_deg, .. } => {
            assert_pt(center, 10.0, 10.0, 0.0);
            assert!((sweep_deg.value() - 270.0).abs() <= EPS, "sweep {}", sweep_deg.value());
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_r_helix() {
    // R + винтовая компонента валидна (SPEC §6.1): Z вне плоскости → pitch.
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X8. Y0. Z2. R5. F100.;\nM30;\n").expect("ok");
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
fn full_circle() {
    // Полная окружность: start==end (end-слова отсутствуют) + center≠start →
    // sweep 360° (§6 отчёта; SPEC §6 ловушка (а)).
    let (p, notices) = notices_of("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 I-5. F100.;\nM30;\n");
    assert!(notices.iter().all(|n| n.kind != NoticeKind::ZeroLengthArc),
        "полное кольцо — не zero-length drop");
    match &p.ops[1] {
        Op::Arc { end, center, sweep_deg, pitch, .. } => {
            assert_pt(end, 0.0, 0.0, 0.0);
            assert_pt(center, -5.0, 0.0, 0.0);
            assert!((sweep_deg.value() - 360.0).abs() <= EPS, "sweep {}", sweep_deg.value());
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
        Op::Arc { sweep_deg, pitch, .. } => {
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
    let p = parse("G21 G90 G94 G17 G90.1;\nG0 X10. Y0. Z0.;\nG2 X-5. Y-5. I0. J5. F100.;\nG91.1;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, sweep_deg, .. } => {
            assert_pt(center, 0.0, 5.0, 0.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
    // G91.1 возвращает инкрементальный режим — следующая дуга ок.
    let p = parse("G21 G90 G94 G17;\nG90.1;\nG91.1;\nG0 X0. Y0. Z0.;\nG2 X-5. Y5. I0. J5. F100.;\nM30;\n").expect("ok");
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
    assert_eq!(err_code(parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 R5. F100.;\nM30;\n")), ErrorCode::InvalidInput);
    assert_eq!(err_code(parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X0. Y0. R5. F100.;\nM30;\n")), ErrorCode::InvalidInput);
}

#[test]
fn radius_mismatch_invalid() {
    // ρ_start=5, ρ_end=|(3,4)−(0,5)|=3.16 → невозможная дуга (SPEC §6).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X3. Y4. I0. J5. F100.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn chord_gt_2r_invalid() {
    // Хорда 20 > 2R=10 — невозможная дуга (SPEC §6).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X20. Y0. R5. F100.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn plane_perpendicular_invalid() {
    // Валидация нормали (SPEC §4/§6): перпендикулярная компонента (center −
    // start) обязана быть ≈0 — K≠0 в G17 / J≠0 в G18 → E_INVALID_INPUT.
    assert_eq!(
        err_code(parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X-5. Y5. K2. I0. J5. F100.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse("G21 G90 G94 G18;\nG0 X0. Y0. Z0.;\nG2 X5. Y2. Z-5. I5. J1. K0. F100.;\nM30;\n")),
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
    assert!(matches!(&p.ops[0], Op::Linear { start: StartPoint::Unknown, .. }));
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
    let (_, notices) = notices_of("G21 G90 G94 G17;\nG0 X5. Y0. Z0.;\nG1 X5. Y0. Z0. F100.;\nM30;\n");
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, NoticeKind::ZeroLengthMotion);
}

#[test]
fn g91_incremental() {
    // G91 на старте: база = дефолт контроллера 0 (SPEC §2 v0.11); start
    // Unknown (точка разрыва (a)), end Known.
    let p = parse("G91 G17;\nG1 X1. Y2. F100.;\nX1. Y1.;\nM30;\n").expect("ok");
    assert!(matches!(&p.ops[0], Op::Linear { start: StartPoint::Unknown, .. }));
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
        err_code(parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nG20;\nG0 X2. Y1. Z1.;\nM30;\n")),
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
    let out = canonicalize_with_notices("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n", "t.ngc").expect("ok");
    assert!(out.program.ops.iter().all(|o| !matches!(o, Op::Raw { .. })));
    let out = canonicalize_with_notices("G21 G90 G95 G17;\nG0 X1. Y1. Z1.;\nM30;\n", "t.ngc").expect("Ok + Raw");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw { code: ErrorCode::UnsupportedSemantics, .. }
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
    let p = parse("N10 G21 G90 G94 G17;\nN20 G0 X1. Y1. Z1.;\nN30 G1 X5. F100.;\nM30;\n").expect("ok");
    assert_eq!(p.ops.len(), 3);
}

#[test]
fn empty_blocks_and_comments_skipped() {
    // Пустые блоки — пропуск без диагностики (§1.3); комментарии — аннотации.
    let (p, notices) = notices_of(";;\n(cStartup)G21 G90 G94 G17;;\nG0 X1. Y1. Z1.;\n(face) G1 X5. F100. (pass);\nM30;\n");
    assert_eq!(p.ops.len(), 3); // Rapid + Linear + ProgramEnd
    assert!(notices.is_empty());
}

#[test]
fn o_code_raw() {
    // O-код → Raw{E_UNSUPPORTED_SYNTAX} (§1.3, SPEC §1.1 v0.16) — финальная
    // диспозиция; программа продолжается.
    let out = canonicalize_with_notices("O0030;\nG21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n", "o.ngc").expect("ok");
    assert!(matches!(&out.program.ops[0], Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }));
    assert!(matches!(&out.program.ops[1], Op::Rapid { .. }));
}

#[test]
fn unknown_g_raw_provisional() {
    // G43/H — вне словаря этапа 1 → provisional Raw{SYNTAX} (молчаливый
    // пропуск запрещён; матрица §2.4 — этап 2: E_REQUIRES_MACHINE_TABLE).
    let out = canonicalize_with_notices("G21 G90 G94 G17;\nG43 H2;\nG0 X1. Y1. Z1.;\nM30;\n", "g.ngc").expect("ok");
    assert!(matches!(&out.program.ops[0], Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }));
    // Голое S — легальное сохранение оборотов (§3 отчёта), не Raw.
    let out = canonicalize_with_notices("S5000;\nG21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n", "s.ngc").expect("ok");
    assert!(out.program.ops.iter().take(1).all(|o| !matches!(o, Op::Raw { .. })));
    assert_eq!(out.program.ops.len(), 2);
}

#[test]
fn block_delete_raw_runtime() {
    // `/` block delete → Raw{E_RUNTIME_STATE} (§1.3, как LinuxCNC COVERAGE §4).
    let out = canonicalize_with_notices("G21 G90 G94 G17;\n/ G1 X5. F100.;\nG0 X1. Y1. Z1.;\nM30;\n", "b.ngc").expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw { code: ErrorCode::RuntimeState, .. }
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
    assert_eq!(d[0].limit.as_ref().map(|l| l.threshold), Some("max_integer_digits"));
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
    let p = parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\nO0100;\nG1 X-10. F200.;\nM99;\n").expect("Ok (хвост → Raw)");
    // ops: Rapid, ProgramEnd, Raw(O), Raw(G1-тело subs), Raw(M99)
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    assert!(matches!(&p.ops[1], Op::ProgramEnd { .. }));
    assert!(matches!(&p.ops[2], Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }));
    assert!(matches!(&p.ops[3], Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }));
    assert!(matches!(&p.ops[4], Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }));
    assert_eq!(p.ops.len(), 5);
}

#[test]
fn lowercase_and_whitespace_tolerance() {
    // Регистр и whitespace между словами — легальны (паттерн grbl gcode.c).
    let p = parse("g21 g90 g94 g17;\ng0 x0. y0. z0.;\ng2 x-5. y5. i0. j5. f100.;\nm30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { center, .. } => assert_pt(center, 0.0, 5.0, 0.0),
        _ => panic!("op1 не Arc"),
    }
}
