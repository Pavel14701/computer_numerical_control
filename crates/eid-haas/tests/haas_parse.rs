//! Интеграционные тесты канонизатора Haas mill NGC → EID/L1 (этап 1, Агент C).
//! Golden-пары (haas-текст → EID AST) по образцу eid-fanuc tests/fanuc_parse.rs;
//! допуски — EQUIVALENCE §4 (eps 0.001).
//!
//! ЧЕК-ЛИСТ этапа 1 (правило → тест):
//! LEXICAL: EOB-разделение → eob_full_line_program; вложенный/незакрытый
//!   комментарий → lexer unit-тесты (18, src/lexer.rs); % → (unit +)
//!   empty_blocks_skipped (block.rs); N-слова → (unit); трейлинг-точка →
//!   dpi_point_not_flagged; целая часть >9 цифр → integer_overflow_limit;
//!   пробел-в-слове строгий отказ `?` → space_in_word_strict_reject;
//!   payload byte-exact → (block.rs round-trip).
//! MODAL (01–06+15): violation → (block.rs modal_violation_detected); G90/G91
//!   → g91_incremental; плоскости → arc_g18/arc_g19; G90.1/G91.1 `?` →
//!   arc_g90_1_unverified_absorb; G94 → (header); G95 → g95_raw_semantics;
//!   units G20/G21 БЕЗ псевдонимов → dpi_bare_integer_inch +
//!   g70_g71_g72_bolt_raw; смена units → unit_switch_mid_invalid;
//!   G61/G64 value-shift → g61_exact_stop_value_shift / g64_blend_bare /
//!   path_switch_mid_invalid.
//! MOTION: rapid/linear → eob_full_line_program; модальное продолжение →
//!   modal_continuation; голые координаты без motion →
//!   naked_coords_no_motion_invalid; zero-length → zero_length_linear_drop;
//!   linear без F → linear_without_f_invalid.
//! ARC §6 + SPEC §3 v0.18: IJK 3 плоскости × cw/ccw → arc_ijk_cw / arc_ijk_ccw
//!   / arc_g18 / arc_g19; R± → arc_r_positive / arc_r_negative; R+pitch `?`
//!   → arc_r_helix; helical → arc_ijk_cw_helix / helical_full_turn; полный
//!   круг (без end-слов) → full_circle; **360° по геометрии с явными
//!   end-словами** → full_circle_explicit_end_words_360; R-полный-круг →
//!   r_full_circle_invalid; радиус-мисс → radius_mismatch_invalid;
//!   хорда>2R → chord_gt_2r_invalid; перпендикулярная компонента →
//!   plane_perpendicular_invalid; вырожденный центр →
//!   zero_length_arc_degenerate_center_still_dropped.
//! DPI (§1.2 machinists notation + Setting 77): dpi_bare_integer_mm /
//!   _inch / dpi_f_setting77_bare (**F-домен Haas: F12 → 0.0012**) /
//!   dpi_f_dotted_verbatim / dpi_point_not_flagged / dpi_all_bare_arc_haas /
//!   dpi_on_raw_row_flush.
//! §8.6 АБСОРБЦИЯ (v0.17, аудит #33): G91 G28 →
//!   modal_absorption_g91_g28_incremental_next (Raw RT + G91 живёт);
//!   W_MODAL_ON_RAW → modal_on_raw_motion_not_continued; units/F/coolant на
//!   raw-кадре → modal_absorption_on_raw_meta.
//! HAAS-СПЕЦИФИКА: G01 C/R фаска/скругление → g01_chamfer_raw /
//!   g01_corner_radius_raw.
//! STRUCTURE: auto-PEND → auto_pend_missing_program_end; хвост-subs →
//!   subprogram_tail_raw; O-code → o_code_raw; Macro B → macro_b_raw;
//!   block delete → block_delete_raw_runtime; вне словаря этапа 1 →
//!   unknown_g_generic_raw / unknown_m13_generic_provisional; этап
//!   диагностики → stage_parse_vs_verify.

use eid_core::adapter::Adapter;
use eid_core::ast::{Coolant, Op, PathMode, Point3, Program, Spindle, StartPoint, Units, Wcs};
use eid_core::diag::Diagnostic;
use eid_core::diag::{ErrorCode, Stage};
use eid_haas::canonicalize::{canonicalize, canonicalize_with_notices, NoticeKind};
use eid_haas::HaasAdapter;

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

fn end_of3(op: &Op) -> [f64; 3] {
    match op {
        Op::Rapid { end, .. } | Op::Linear { end, .. } | Op::Arc { end, .. } => pt3(end),
        other => panic!("end у {other:?}"),
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

fn notices_of(src: &str) -> (Program, Vec<eid_haas::canonicalize::Notice>) {
    let out = canonicalize_with_notices(src, "test.ngc").expect("ok");
    (out.program, out.notices)
}

// ---------------------------------------------------------------------------
// Golden-пары: программа → AST
// ---------------------------------------------------------------------------

#[test]
fn eob_full_line_program() {
    // Вся программа на ОДНОЙ физической строке: `;` делит блоки (§1.3
    // ловушка №2). GRBL-лексер с `;`-комментарием съел бы хвосты — 4 блока.
    // O-код → Raw{SYNTAX} (§1.3: O##### — структура подпрограмм).
    let p = parse("O0030; G21 G90 G94 G17; G0 X0. Y0. Z5.; G1 X10. F150.; M30;").expect("ok");
    assert_eq!(p.ops.len(), 4); // Raw(O) + Rapid + Linear + ProgramEnd
    assert!(matches!(&p.ops[0], Op::Raw { .. }));
    match &p.ops[1] {
        Op::Rapid { end, state, .. } => {
            assert_pt(end, 0.0, 0.0, 5.0);
            assert_eq!(state.wcs, Wcs::G54, "дефолт NGC power-on");
        }
        other => panic!("ops[1] не Rapid: {other:?}"),
    }
    match &p.ops[2] {
        Op::Linear { end, state, .. } => {
            assert_pt(end, 10.0, 0.0, 5.0);
            assert_eq!(feed_of(&p.ops[2]).unwrap(), 150.0, "dotted F — verbatim");
            assert_eq!(state.spindle, Spindle::Off);
        }
        other => panic!("ops[2] не Linear: {other:?}"),
    }
    assert!(matches!(&p.ops[3], Op::ProgramEnd { .. }));
    assert_eq!(p.units, Units::Mm);
    assert_eq!(p.path_mode, PathMode::ExactPath, "дефолт SPEC §3 v0.12");
}

// ---------------------------------------------------------------------------
// DPI §1.2 — machinists notation (Setting 162 OFF) + Setting 77 (F-домен!)
// ---------------------------------------------------------------------------

#[test]
fn dpi_bare_integer_mm() {
    // X25 в G21 = 0.025 mm (НЕ 25!) — machinists notation, дефолт NGC
    // (§1.2 ловушка №1); молчаливое чтение как целое запрещено (×1000).
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
    assert!(dpi.iter().any(|n| n.message.contains("Setting 162")));
}

#[test]
fn dpi_bare_integer_inch() {
    // Least increment inch: 25 × 0.0001" = 0.0025 (§1.2 по units).
    let (p, notices) = notices_of("G20 G90 G94 G17;\nG0 X25 Y0 Z0;\nM30;\n");
    assert_eq!(p.units, Units::In);
    match &p.ops[0] {
        Op::Rapid { end, .. } => assert_pt(end, 0.0025, 0.0, 0.0),
        _ => panic!("op0 не Rapid"),
    }
    assert!(notices.iter().any(|n| n.message.contains("0.0001")));
}

#[test]
fn dpi_f_setting77_bare() {
    // HAAS F-ДОМЕН (≠ Fanuc ruling 2!): bare F12 = 0.0012 units/min
    // (Setting 77 «Scale Integer F» дефолт OFF); notice — с текстом
    // Setting 77. Fanuc «целое mm/min verbatim» НЕ переносится.
    let (p, notices) = notices_of("G21 G90 G94 G17;\nG1 X10. F12;\nM30;\n");
    match &p.ops[0] {
        Op::Linear { .. } => assert!(
            (feed_of(&p.ops[0]).unwrap() - 0.0012).abs() <= 1e-9,
            "F12 → 0.0012 units/min, got {}",
            feed_of(&p.ops[0]).unwrap()
        ),
        _ => panic!("op0 не Linear"),
    }
    let dpi: Vec<_> = notices
        .iter()
        .filter(|n| n.kind == NoticeKind::DpiAssumed)
        .collect();
    assert!(
        dpi.iter()
            .any(|n| n.message.contains("Setting 77") && n.message.contains("0.0012")),
        "notice с текстом Setting 77: {:?}",
        dpi
    );
}

#[test]
fn dpi_f_dotted_verbatim() {
    // Dotted-F → verbatim (Setting 77 масштабирует ТОЛЬКО bare-integer);
    // F150. = 150 units/min, без notice.
    let (p, notices) = notices_of("G21 G90 G94 G17;\nG1 X10. F150.;\nM30;\n");
    assert_eq!(feed_of(&p.ops[0]), Some(150.0));
    assert!(
        notices.iter().all(|n| n.kind != NoticeKind::DpiAssumed),
        "dotted-F — verbatim, DPI не применяется"
    );
}

#[test]
fn dpi_point_not_flagged() {
    // С точкой — verbatim (SPEC §10.1), без DPI-notice; трейлинг-точка
    // легальна (§1.2: обязательна в эмиссии).
    let (_, notices) = notices_of("G21 G90 G94 G17;\nG0 X25. Y50. Z0.0001;\nM30;\n");
    assert!(notices.iter().all(|n| n.kind != NoticeKind::DpiAssumed));
    let p = parse("G21 G90 G94 G17;\nG0 X25. Y50. Z0.0001;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Rapid { end, .. } => assert_pt(end, 25.0, 50.0, 0.0001),
        _ => panic!("op0 не Rapid"),
    }
}

#[test]
fn dpi_all_bare_arc_haas() {
    // Дуга со всеми bare-словами: геометрия — равномерное сжатие ×0.001
    // (X-5000→-5, Y5000→5, I0→0, J5000→5); F — HAAS-ПРАВИЛО: bare F ×
    // least increment → F100000 → 100.0 (≠ Fanuc verbatim 100000!).
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
    assert_eq!(feed_of(&p.ops[1]), Some(10.0), "F bare ×0.0001 (Setting 77)");
    let dpi = notices
        .iter()
        .filter(|n| n.kind == NoticeKind::DpiAssumed)
        .count();
    assert!(dpi >= 5, "все слова дуги bare: {dpi}");
}

// ---------------------------------------------------------------------------
// Модальности 01–06 + 15
// ---------------------------------------------------------------------------

#[test]
fn g91_incremental() {
    let p = parse("G21 G91;\nG0 X10. Y10. Z10.;\nG0 X1. Y1. Z1.;\nM30;\n").expect("ok");
    // Первый ход: точка разрыва (a) — start Unknown, оси наследуют 0.
    match &p.ops[0] {
        Op::Rapid {
            start: StartPoint::Unknown,
            end,
            ..
        } => assert_pt(end, 10.0, 10.0, 10.0),
        other => panic!("ops[0] не Rapid(Unknown): {other:?}"),
    }
    // Второй ход: инкремент от (10,10,10) → (11,11,11).
    match &p.ops[1] {
        Op::Rapid { .. } => {
            assert_eq!(known(&p.ops[1]), [10.0, 10.0, 10.0]);
            assert_eq!(end_of3(&p.ops[1]), [11.0, 11.0, 11.0]);
        }
        other => panic!("ops[1] не Rapid: {other:?}"),
    }
}

#[test]
fn g95_raw_semantics() {
    // G95 feed/rev — rpm от override, не представимо (§2.2; кандидат
    // ABSORB при статическом S — ревизия по корпусу). Raw, не Err.
    let p = parse("G21 G90 G94 G17;\nG0 X5.;\nG95 F10;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[1],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    // G94 — легален (ABSORB).
    let p = parse("G21 G90 G94 G17;\nG1 X5. F100.;\nM30;\n").expect("ok");
    assert!(matches!(&p.ops[0], Op::Linear { .. }));
}

#[test]
fn unit_switch_mid_invalid() {
    // Ровно одна система единиц на программу (SPEC §2.5) — жёсткий отказ.
    assert_eq!(
        err_code(parse("G21 G90;\nG0 X5.;\nG20;\nM30;\n")),
        ErrorCode::UnsupportedSemantics
    );
}

#[test]
fn g61_exact_stop_value_shift() {
    // Группа 15 — G61 = exact stop mode (совпадает по значению с Fanuc G61;
    // LinuxCNC G61 = exact_path — маппинг по значению, не по имени).
    let p = parse("G21 G90 G61;\nG0 X5.;\nM30;\n").expect("ok");
    assert_eq!(p.path_mode, PathMode::ExactStop);
}

#[test]
fn g64_blend_bare() {
    // G64 = cancel G61 → bare blend (t-квантификатора в словаре Haas НЕТ —
    // сдвиг значения vs LinuxCNC G64 blend(t)).
    let p = parse("G21 G90 G64;\nG0 X5.;\nM30;\n").expect("ok");
    assert_eq!(p.path_mode, PathMode::Blend { t: None });
}

#[test]
fn path_switch_mid_invalid() {
    // path_mode глобален (SPEC §3) — переключение G61→G64 mid-program — отказ.
    assert_eq!(
        err_code(parse("G21 G61;\nG0 X5.;\nG64;\nM30;\n")),
        ErrorCode::UnsupportedSemantics
    );
}

// ---------------------------------------------------------------------------
// Движение
// ---------------------------------------------------------------------------

#[test]
fn modal_continuation() {
    // Голые координаты при активной группе 01 — продолжение (§1.3 `?`).
    let p = parse("G21 G90;\nG0 X1. Y1. Z1.;\nX5.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Rapid { .. } => {
            assert_eq!(known(&p.ops[1]), [1.0, 1.0, 1.0]);
            assert_eq!(end_of3(&p.ops[1]), [5.0, 1.0, 1.0]);
        }
        other => panic!("ops[1] не Rapid: {other:?}"),
    }
}

#[test]
fn naked_coords_no_motion_invalid() {
    // Без активного motion-режима голые координаты — сломанный вход.
    assert_eq!(
        err_code(parse("G21 G90;\nX5. Y5.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn linear_without_f_invalid() {
    // linear/arc требуют модального F (SPEC §3/§4).
    assert_eq!(
        err_code(parse("G21 G90;\nG1 X5.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn zero_length_linear_drop() {
    // start == end → drop + notice (SPEC §3), позиция обновляется.
    let (p, notices) = notices_of("G21 G90;\nG0 X1. Y1. Z1.;\nG0 X1. Y1. Z1.;\nM30;\n");
    assert_eq!(p.ops.len(), 2); // Rapid + PEND (второй rapid отброшен)
    assert!(
        notices
            .iter()
            .any(|n| n.kind == NoticeKind::ZeroLengthMotion),
        "zero-length drop notice"
    );
    // Третий ход связывается с (1,1,1) — drop обновил позицию.
    let p = parse("G21 G90;\nG0 X1. Y1. Z1.;\nG0 X1. Y1. Z1.;\nG0 X2. Y1. Z1.;\nM30;\n").expect("ok");
    assert_eq!(known(&p.ops[1]), [1.0, 1.0, 1.0]);
}

// ---------------------------------------------------------------------------
// Дуги §6 + полный круг 360° по геометрии (SPEC §3 v0.18)
// ---------------------------------------------------------------------------

#[test]
fn arc_ijk_cw_helix() {
    // G17 CW с вне-плоскостным Z-словом → pitch (винтовая; thread milling).
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG2 X0. Y-10. Z5. I-10. J0. F100.;\nM30;\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            sweep_deg,
            pitch,
            dir,
            ..
        } => {
            assert_pt(center, 0.0, 0.0, 0.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
            assert!((pitch.value() - 5.0).abs() <= EPS);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_ijk_ccw() {
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG3 X0. Y10. I-10. J0. F100.;\nM30;\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center, sweep_deg, ..
        } => {
            assert_pt(center, 0.0, 0.0, 0.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_g18() {
    // G18 = ZX-плоскость (пары как LinuxCNC; нормаль +Y): центр K↔Z.
    let p = parse("G21 G90 G94 G18;\nG0 X0. Y0. Z0.;\nG3 X-5. Z5. K5. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            sweep_deg,
            normal,
            ..
        } => {
            assert_pt(center, 0.0, 0.0, 5.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
            assert_pt(normal, 0.0, 1.0, 0.0);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_g19() {
    // G19 = YZ-плоскость (нормаль +X): центр J↔Y.
    let p = parse("G21 G90 G94 G19;\nG0 X0. Y0. Z0.;\nG2 Y5. Z5. J5. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            sweep_deg,
            normal,
            ..
        } => {
            assert_pt(center, 0.0, 5.0, 0.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
            assert_pt(normal, 1.0, 0.0, 0.0);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_r_positive() {
    // R>0 → дуга ≤180°: chord=(5,5), R=10 → sweep ≈ 41.4°; знак направления
    // выбирает сторону (формула LinuxCNC/grbl).
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X5. Y5. R10. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center, sweep_deg, ..
        } => {
            assert_pt(center, 9.114, -4.114, 0.0);
            assert!((sweep_deg.value() - 41.410).abs() <= 0.01, "sweep {}", sweep_deg.value());
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_r_negative() {
    // R<0 → дуга >180°: та же хорда, R=-5 → sweep = 270°.
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X5. Y5. R-5. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center, sweep_deg, ..
        } => {
            assert_pt(center, 0.0, 5.0, 0.0);
            assert!((sweep_deg.value() - 270.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_r_helix() {
    // R + вне-плоскостное слово — helical R-форма: допустимо (SPEC §6 v0.7
    // правило 1 — нормаль из плоскости); `?` §10(24) — Haas-допустимость
    // R+Z сверить (реестр), семантика EID не зависит.
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X5. Y5. Z3. R10. F100.;\nM30;\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            sweep_deg, pitch, ..
        } => {
            assert!((sweep_deg.value() - 41.410).abs() <= 0.01);
            assert!((pitch.value() - 3.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn arc_g90_1_unverified_absorb() {
    // G90.1/G91.1 — В СЛОВАРЕ HAAS НЕТ (группа 04 отсутствует, I/J/K всегда
    // инкрементальные); ruling этапа 1 — ABSORB как LinuxCNC, помечено `?`
    // (несверено). Позитивный тест доказывает G90.1-путь: инкрементальная
    // интерпретация дала бы radius-mismatch.
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

#[test]
fn full_circle() {
    // Полное кольцо по Haas-ману: «Defining only the I J K values… will
    // cut a circle» — start==end (end-слова отсутствуют) + center≠start →
    // sweep 360° (§2.1; SPEC §6 ловушка (а)).
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
fn full_circle_explicit_end_words_360() {
    // ЗЕРКАЛО SPEC §3 v0.18 (бриф этапа 1d): center-форма с center ≠ start
    // при start == end = полный круг 360° ПО ГЕОМЕТРИИ НЕЗАВИСИМО от явных
    // end-слов (CAM-посты систематически пишут end-слова на полных кругах).
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

// Дуги §6 — негативы

#[test]
fn r_full_circle_invalid() {
    // R при полной окружности → E_INVALID_INPUT (SPEC §6(2); контракт §2.1:
    // «R при полной окружности невалидна»; центр неоднозначен).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 R5. F100.;\nM30;\n")),
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
    // start) обязана быть ≈0 — K≠0 в G17 → E_INVALID_INPUT.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X5. Y5. K2. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

// ---------------------------------------------------------------------------
// Haas-специфика: G01 C/R (фаска/скругление), G70/G71/G72 болтовые
// ---------------------------------------------------------------------------

#[test]
fn g01_chamfer_raw() {
    // C в G01 = фаска угла между этим и следующим ходом (Haas-специфика
    // §2.1) — lookahead-декомпозиция, этап 12 → Raw{SEMANTICS}.
    let p = parse("G21 G90 G94;\nG0 X0. Y0.;\nG1 X10. C2. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        } => {}
        other => panic!("ops[1] не Raw(SEMANTICS): {other:?}"),
    }
}

#[test]
fn g01_corner_radius_raw() {
    // R в G01 = радиус скругления угла (контекстное слово §4.1) → Raw.
    let p = parse("G21 G90 G94;\nG0 X0. Y0.;\nG1 X10. R5. F100.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        } => {}
        other => panic!("ops[1] не Raw(SEMANTICS): {other:?}"),
    }
}

#[test]
fn g70_g71_g72_bolt_raw() {
    // КОЛЛИЗИЯ АДРЕСОВ: G70/G71/G72 Haas = болтовые паттерны (Bolt Hole
    // Circle/Arc/Holes Along an Angle), НЕ units-псевдонимы (≠ Fanuc mill).
    // Raw{SEMANTICS}; units НЕ переключаются.
    for code in ["G70", "G71", "G72"] {
        let src = format!("G21 G90 G94;\n{code};\nM30;\n");
        let (p, _) = notices_of(&src);
        assert_eq!(p.units, Units::Mm, "{code} — не units!");
        let p = parse(&src).expect("ok");
        assert!(
            matches!(
                &p.ops[0],
                Op::Raw {
                    code: ErrorCode::UnsupportedSemantics,
                    ..
                }
            ),
            "{code} → Raw SEMANTICS (болтовой паттерн)"
        );
    }
}

// ---------------------------------------------------------------------------
// §8.6 модальная абсорбция на RAW-строках (v0.17 — аудит #33)
// ---------------------------------------------------------------------------

#[test]
fn modal_absorption_g91_g28_incremental_next() {
    // ЗЕРКАЛО пробоя аудита #33: `G91 G28 Z0` терял G91 (raw-возврат ДО
    // абсорбции) → следующий `G0 X10 Y10` молча АБСОЛЮТНЫЙ. G28 в словаре
    // Haas ЕСТЬ (§2.3: Return To Machine Zero → Raw{RT}, Q1) → Raw +
    // абсорбция; G91 живёт: следующий ход инкрементальный (20,20,10)+(10,10,0).
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
fn modal_on_raw_motion_not_continued() {
    // SPEC §8.6: raw-motion-G (G31 probing…) с осями — модальное состояние
    // абсорбируется, motion-режим НЕ продолжается + notice W_MODAL_ON_RAW.
    let (_, notices) =
        notices_of("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG31 X20. F50.;\nM30;\n");
    assert!(
        notices.iter().any(|n| n.kind == NoticeKind::ModalOnRaw),
        "нет W_MODAL_ON_RAW на raw-motion-кадре"
    );
    // Явный G0 ПОСЛЕ raw-motion-G — легален (режим переустановлен).
    let p = parse(
        "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG31 X20. F50.;\nG0 X30. Y30.;\nM30;\n",
    )
    .expect("ok");
    assert_eq!(known(&p.ops[2]), [10.0, 10.0, 5.0]);
    // Голые координаты ПОСЛЕ raw-motion-G без переустановки режима —
    // честный отказ (режим не продолжается молча).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG31 X20. F50.;\nX30. Y30.;\nM30;\n",
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn modal_absorption_on_raw_meta() {
    // SPEC §8.6: units/F/coolant абсорбируются и на raw-кадрах.
    // (a) G20 на raw-кадре (G43 → generic provisional) БЕЗ предыдущего
    // units-слова — устанавливает inch (первое вхождение, SPEC §2.5).
    let out = canonicalize_with_notices("G43 H2 G20;\nM30;\n", "t.ngc").expect("Ok + Raw");
    assert_eq!(
        out.program.units,
        Units::In,
        "G20 на raw-кадре абсорбирован"
    );
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    // (b) F на raw-кадре — модальная подача установлена.
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG43 H2 F500.;\nG1 X20. Y20.;\nM30;\n")
        .expect("ok");
    match &p.ops[2] {
        Op::Linear { state, .. } => {
            assert!((state.feed.as_ref().expect("feed").value() - 500.0).abs() <= EPS);
        }
        other => panic!("ops[2] не Linear: {other:?}"),
    }
    // (c) M8 на raw-кадре — СОЖ абсорбирована.
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG43 H2 M8;\nG1 X20. Y20. F100.;\nM30;\n")
        .expect("ok");
    match &p.ops[2] {
        Op::Linear { state, .. } => assert_eq!(state.coolant, Coolant { flood: true, mist: false }),
        other => panic!("ops[2] не Linear: {other:?}"),
    }
}

#[test]
fn dpi_on_raw_row_flush() {
    // Bare-F на raw-кадре: DPI-notice (Setting 77) прицепляется к raw-опу
    // через pending-буфер (§8.6 — абсорбция до push_raw).
    let (_, notices) = notices_of("G21 G90 G94 G17;\nG43 H2 F500;\nM30;\n");
    let dpi: Vec<_> = notices
        .iter()
        .filter(|n| n.kind == NoticeKind::DpiAssumed)
        .collect();
    assert_eq!(dpi.len(), 1, "F500 bare → один notice");
    assert!(dpi[0].message.contains("Setting 77"));
    assert!(dpi[0].message.contains("0.05"), "500 × 0.0001 = 0.05");
}

// ---------------------------------------------------------------------------
// Структура: auto-PEND, хвост-subs, ведра
// ---------------------------------------------------------------------------

#[test]
fn auto_pend_missing_program_end() {
    // DNC-файл без M30 → auto-PEND + notice (SPEC §3 v0.16; §4.4).
    let (p, notices) = notices_of("G21 G90 G94 G17;\nG0 X5.;\n");
    assert_eq!(p.ops.len(), 2);
    assert!(matches!(&p.ops[1], Op::ProgramEnd { .. }));
    assert!(
        notices
            .iter()
            .any(|n| n.kind == NoticeKind::MissingProgramEnd),
        "W_MISSING_PROGRAM_END"
    );
}

#[test]
fn subprogram_tail_raw() {
    // Хвост после PEND → Raw (main + subs в одном файле — норма Haas §4.4;
    // осознанное расхождение с grbl n12, зеркало Fanuc ruling 1).
    let p = parse("G21 G90;\nM30;\nG1 X5. F100.;\n").expect("ok");
    assert_eq!(p.ops.len(), 2);
    assert!(matches!(&p.ops[0], Op::ProgramEnd { .. }));
    assert!(matches!(&p.ops[1], Op::Raw { .. }));
}

#[test]
fn o_code_raw() {
    // O##### — номер программы (NGC 5 цифр `?` §10) → Raw{SYNTAX} (§1.3).
    let p = parse("G21 G90;\nO0030;\nG0 X5.;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        } => {}
        other => panic!("ops[0] не Raw(SYNTAX): {other:?}"),
    }
}

#[test]
fn macro_b_raw() {
    // Macro B (`#`-переменные, Fanuc-совместимый §4.3) → line-level Raw;
    // соседние статические ходы парсятся.
    let p = parse("G21 G90;\n#100 = 5;\nG1 X5. F100.;\nM30;\n").expect("ok");
    assert_eq!(p.ops.len(), 3);
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    assert!(matches!(&p.ops[1], Op::Linear { .. }));
}

#[test]
fn block_delete_raw_runtime() {
    // `/` — исполнение зависит от переключателя станка → Raw{RUNTIME_STATE}.
    let p = parse("G21 G90;\n/ G1 X5. F100.;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Raw {
            code: ErrorCode::RuntimeState,
            ..
        } => {}
        other => panic!("ops[0] не Raw(RUNTIME): {other:?}"),
    }
}

#[test]
fn unknown_g_generic_raw() {
    // Вне словаря этапа 1 (фреймы/циклы — этап 2; G107 — этап 12+ ведро) —
    // generic-ведро Raw{SYNTAX}, не молча (COVERAGE §2.7).
    for (src, code) in [
        ("G107;", ErrorCode::UnsupportedSyntax),
        ("G54;", ErrorCode::UnsupportedSyntax),  // фреймы — этап 2
        ("G81 X5. Z-2. R1. F50.;", ErrorCode::UnsupportedSyntax), // циклы — этап 2
        ("G52 X1.;", ErrorCode::UnsupportedSyntax), // G52-ERR — этап 2 (provisional)
    ] {
        let src = format!("G21 G90;\n{src}\nM30;\n");
        let p = parse(&src).expect("ok");
        assert!(
            matches!(&p.ops[0], Op::Raw { code: c, .. } if *c == code),
            "{src}"
        );
    }
}

#[test]
fn unknown_m13_generic_provisional() {
    // M13 (5-я ось тормоз! НЕ «шпиндель+СОЖ» — гипотеза опровергнута §0.1)
    // — этап 2 вендорское ведро; на этапе 1 generic provisional SYNTAX.
    let p = parse("G21 G90;\nM13;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
}

#[test]
fn raw_table_g28_g31_g12() {
    // Raw-таблица фиксированных диспозиций (COVERAGE §2 Haas-колонка).
    let p = parse("G21 G90;\nG28;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
    let p = parse("G21 G90;\nG31 X5. F50.;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    let p = parse("G21 G90;\nG12 I10. J0. K2.;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
}

// ---------------------------------------------------------------------------
// Состояние: шпиндель/СОЖ
// ---------------------------------------------------------------------------

#[test]
fn spindle_coolant_state() {
    // Голое S — сохранение оборотов (§3); M3 подхватывает last_s;
    // M5/M9 гасят.
    let p = parse("G21 G90 G94;\nS5000;\nM3;\nG1 X5. F100.;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Linear { state, .. } => {
            assert_eq!(state.spindle, Spindle::Cw(eid_core::ast::Num::new("5000".into(), 5000.0)));
            assert_eq!(state.coolant, Coolant::default());
        }
        other => panic!("ops[0] не Linear: {other:?}"),
    }
    let p = parse("G21 G90 G94;\nM8;\nG1 X5. F100.;\nM9;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Linear { state, .. } => assert!(state.coolant.flood),
        _ => panic!("ops[0] не Linear"),
    }
}

// ---------------------------------------------------------------------------
// Лимиты и этапы диагностики
// ---------------------------------------------------------------------------

#[test]
fn integer_overflow_limit() {
    // §1.2: >9 цифр целой части → E_LIMIT_EXCEEDED (диапазон Haas `?` §10(31)).
    let Err(d) = parse("G21 G90;\nG0 X1234567890.;\nM30;\n") else {
        panic!("ожидался Err")
    };
    assert_eq!(d[0].code, ErrorCode::LimitExceeded);
    let lim = d[0].limit.as_ref().expect("LimitInfo");
    assert_eq!(lim.threshold, "max_integer_digits");
    assert_eq!(lim.actual, 10.0);
}

#[test]
fn stage_parse_vs_verify() {
    // Лексические ошибки — Parse; канонизационные инварианты — Verify.
    assert_eq!(err_stage(parse("(nested (bad)) G1 X1")), Stage::Parse);
    assert_eq!(err_stage(parse("G21 G90;\nX5. Y5.;\n")), Stage::Verify);
}

#[test]
fn space_in_word_strict_reject() {
    // `?`-реестр (контракт не специфицирует): пробел-в-слове — строгий
    // отказ; отличие от eid-fanuc (там легально) — вендорное, помечено `?`.
    assert_eq!(
        err_code(parse("G21 G90;\nX 25;\n")),
        ErrorCode::InvalidInput
    );
    assert_eq!(err_stage(parse("G 1;")), Stage::Parse);
}

#[test]
fn adapter_parse_and_notices() {
    // Адаптер: parse + notice-буфер (паттерн GrblAdapter/FanucAdapter);
    // emit — этап 3 (честный отказ).
    let adapter = HaasAdapter::new();
    assert_eq!(adapter.dialect_name(), "haas-mill");
    let p = adapter.parse("G21 G90 G94 G17;\nG0 X25 Y0 Z0;\nM30;\n", "a.ngc").expect("ok");
    assert_eq!(p.ops.len(), 2);
    assert!(matches!(&p.ops[1], Op::ProgramEnd { .. }));
    let notices = adapter.take_notices();
    assert!(notices.iter().any(|n| n.kind == NoticeKind::DpiAssumed));
    // M30 присутствует — auto-PEND notice нет; отдельная проверка ниже.
    assert!(!notices.iter().any(|n| n.kind == NoticeKind::MissingProgramEnd));
    let p2 = adapter.parse("G21 G90;\nG0 X5.;\n", "b.ngc").expect("ok");
    let n2 = adapter.take_notices();
    assert!(n2.iter().any(|n| n.kind == NoticeKind::MissingProgramEnd));
    let r = adapter.emit(&p);
    assert!(r.is_err(), "эмиссия — этап 3");
}
