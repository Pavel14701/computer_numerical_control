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
use eid_core::ast::{
    Coolant, Frame, Op, PathMode, Point3, Program, Spindle, StartPoint, Units, Wcs,
};
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
        "notice с текстом Setting 77: {dpi:?}",
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
    assert_eq!(
        feed_of(&p.ops[1]),
        Some(10.0),
        "F bare ×0.0001 (Setting 77)"
    );
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
    let p =
        parse("G21 G90;\nG0 X1. Y1. Z1.;\nG0 X1. Y1. Z1.;\nG0 X2. Y1. Z1.;\nM30;\n").expect("ok");
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
            assert!(
                (sweep_deg.value() - 41.410).abs() <= 0.01,
                "sweep {}",
                sweep_deg.value()
            );
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
    let p =
        parse("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X5. Y5. Z3. R10. F100.;\nM30;\n").expect("ok");
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
    let (_, notices) = notices_of("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG31 X20. F50.;\nM30;\n");
    assert!(
        notices.iter().any(|n| n.kind == NoticeKind::ModalOnRaw),
        "нет W_MODAL_ON_RAW на raw-motion-кадре"
    );
    // Явный G0 ПОСЛЕ raw-motion-G — легален (режим переустановлен).
    let p = parse("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG31 X20. F50.;\nG0 X30. Y30.;\nM30;\n")
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
    // (a) G20 на raw-кадре (G43 → Raw{RT}, этап 2) БЕЗ предыдущего
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
            code: ErrorCode::RequiresMachineTable,
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
        Op::Linear { state, .. } => assert_eq!(
            state.coolant,
            Coolant {
                flood: true,
                mist: false
            }
        ),
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
    // Вне словаря Haas NGC (этапы 1–2; фреймы/циклы — этап 2 словарь) —
    // generic-ведро Raw{SYNTAX}, не молча (COVERAGE §2.7).
    for src in ["G88.8;", "G999;", "G45.5;"] {
        let src = format!("G21 G90;\n{src}\nM30;\n");
        let p = parse(&src).expect("ok");
        assert!(
            matches!(
                &p.ops[0],
                Op::Raw {
                    code: ErrorCode::UnsupportedSyntax,
                    ..
                }
            ),
            "{src}"
        );
    }
    // G107/G103/G187/G167 — в raw-таблице этапа 2 (фиксированные коды
    // COVERAGE §2.6/§2.7), не generic.
    let p = parse("G21 G90;\nG107;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    let p = parse("G21 G90;\nG167 P30 Q5.;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::RuntimeState,
            ..
        }
    ));
}

#[test]
fn m13_brake_collision_pin() {
    // M13 = release 5-й оси ТОРМОЗ (НЕ «шпиндель+СОЖ» — Fanuc-гипотеза
    // опровергнута research'ом §0.1; коллизия номеров диалектов — Raw SEM).
    let (p, notices) = notices_of("G21 G90;\nM13;\nG1 X5. F100.;\nM30;\n");
    match &p.ops[0] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(reason.contains("NOT spindle+coolant"), "{reason}");
        }
        other => panic!("ops[0] не Raw: {other:?}"),
    }
    // Шпиндель M13 НЕ запускает (≠ Fanuc-идиома): на следующем линейном опе
    // шпиндель Off, СОЖ не включена.
    match &p.ops[1] {
        Op::Linear { state, .. } => {
            assert_eq!(state.spindle, Spindle::Off);
            assert!(!state.coolant.flood);
            assert!(!state.coolant.mist);
        }
        other => panic!("ops[1] не Linear: {other:?}"),
    }
    let _ = notices;
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
            assert_eq!(
                state.spindle,
                Spindle::Cw(eid_core::ast::Num::new("5000".into(), 5000.0))
            );
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
    // Адаптер: parse → owning-результат с notices (v0.2 #22 — буфера нет;
    // каждый parse возвращает СВОИ notices); emit — этап 3 (честный отказ).
    let adapter = HaasAdapter::new();
    assert_eq!(adapter.dialect_name(), "haas-mill");
    let out = adapter
        .parse("G21 G90 G94 G17;\nG0 X25 Y0 Z0;\nM30;\n", "a.ngc")
        .expect("ok");
    let p = out.program;
    assert_eq!(p.ops.len(), 2);
    assert!(matches!(&p.ops[1], Op::ProgramEnd { .. }));
    let notices = out.notices;
    assert!(notices
        .iter()
        .any(|n| n.kind == eid_core::verify::NoticeKind::DpiAssumed));
    // M30 присутствует — auto-PEND notice нет; отдельная проверка ниже.
    assert!(!notices
        .iter()
        .any(|n| n.kind == eid_core::verify::NoticeKind::MissingProgramEnd));
    let out2 = adapter.parse("G21 G90;\nG0 X5.;\n", "b.ngc").expect("ok");
    let n2 = out2.notices;
    assert!(n2
        .iter()
        .any(|n| n.kind == eid_core::verify::NoticeKind::MissingProgramEnd));
    // Этап 3: эмиссия жива; warnings в owning-результате emit
    // (дефолт ExactPath → G61 консервативно + warning — COVERAGE §5.2).
    let out3 = adapter.emit(&p).expect("эмиссия этапа 3");
    let text = out3.text;
    assert!(text.starts_with("G21 G90 G94 G17\n"), "{text:?}");
    let w = out3.warnings;
    assert!(w.iter().any(|x| x.contains("exact_path")), "{w:?}");
}

// ---------------------------------------------------------------------------
// ЭТАП 2 — Фреймы: G54–G59 p=1..6; G110–G112 → p=7..9 (нативная симметрия
// слотов #7–#9, notice); G113+ → Raw; G154 Pn; G52-ERR; G10-формы; G92; G53
// ---------------------------------------------------------------------------

#[test]
fn g54_g59_wcs_select() {
    // Группа 12: G54–G59 → state.wcs p=1..6 (1:1; дефолт G54 — power-on).
    // После переключения WCS ходы с ПОЛНЫМ end (точка разрыва (b): без
    // in-text origins связывание неизвестно — частичный end → RT).
    let p = parse(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG55;\nG1 X5. Y5. Z5. F100.;\nG58;\nG1 X6. Y6. Z5.;\nM30;\n",
    )
    .expect("ok");
    match &p.ops[0] {
        Op::Rapid { state, .. } => assert_eq!(state.wcs, Wcs::G54),
        other => panic!("ops[0] не Rapid: {other:?}"),
    }
    match &p.ops[1] {
        Op::Linear { state, .. } => assert_eq!(state.wcs, Wcs::G55),
        other => panic!("ops[1] не Linear: {other:?}"),
    }
    match &p.ops[2] {
        Op::Linear { state, .. } => assert_eq!(state.wcs, Wcs::G58),
        other => panic!("ops[2] не Linear: {other:?}"),
    }
    // Частичный end сразу после переключения WCS без in-text origins — RT
    // (точка разрыва (b): start неизвестен).
    assert_eq!(
        err_code(parse(
            "G21 G90;\nG0 X0. Y0. Z5.;\nG55;\nG1 X5. Y5. F100.;\nM30;\n"
        )),
        ErrorCode::RuntimeState
    );
    // Два кода группы 12 в блоке — нарушение.
    assert_eq!(
        err_code(parse("G21 G90;\nG54 G55;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn g110_g112_extended_p7_9_notice() {
    // Бриф этапа 2: G110–G112 → EID p=7..9 (нативная симметрия слотов #7–#9
    // = Wcs::G59_1..G59_3) с notice W_WCS_EXTENDED — эмиссия этапа 3 отдаст
    // G110–G112. COVERAGE Haas-колонка v0.9 фиксирует RAW — расхождение
    // колонки и брифа задокументировано notice'ом (не молча).
    let (p, notices) = notices_of(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG110;\nG1 X5. Y5. Z5. F100.;\nG111;\nG112;\nM30;\n",
    );
    match &p.ops[1] {
        Op::Linear { state, .. } => {
            assert_eq!(state.wcs, Wcs::G59_1, "G110 → p7");
        }
        other => panic!("ops[1] не Linear: {other:?}"),
    }
    let ext: Vec<_> = notices
        .iter()
        .filter(|n| n.kind == NoticeKind::WcsExtended)
        .collect();
    assert_eq!(ext.len(), 3, "G110/G111/G112 — три notice: {ext:?}");
    for (n, code) in ext.iter().zip(["G110", "G111", "G112"]) {
        assert!(n.message.contains(code), "{}: {}", code, n.message);
        assert!(n.message.contains("p7") || n.message.contains("p8") || n.message.contains("p9"));
        assert!(n.message.contains("slot"));
    }
    // Порядок: notice G110 прицеплен к Linear (первый op после G110).
    assert_eq!(ext[0].op_index, 1);
}

#[test]
fn g113_g129_outside_p_domain_raw() {
    // G113–G129 = слоты #10–#26 — вне EID p-домена (p ≤ 9) → Raw{SEM}.
    for code in ["G113", "G120", "G129"] {
        let src = format!("G21 G90;\n{code};\nM30;\n");
        let p = parse(&src).expect("ok");
        match &p.ops[0] {
            Op::Raw {
                code: c, reason, ..
            } => {
                assert_eq!(*c, ErrorCode::UnsupportedSemantics, "{code}");
                assert!(reason.contains("p-domain"), "{code}: {reason}");
            }
            other => panic!("{code} не Raw: {other:?}"),
        }
    }
}

#[test]
fn g154_p1_p3_p7_9_and_out_of_domain() {
    // G154 Pn: P1–P3 = алиасы G110–G112 → p=7..9 (P1 = слот #7); P4+ → Raw.
    let (p, notices) =
        notices_of("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG154 P2;\nG1 X5. Y5. Z5. F100.;\nM30;\n");
    match &p.ops[1] {
        Op::Linear { state, .. } => assert_eq!(state.wcs, Wcs::G59_2, "G154 P2 → p8"),
        other => panic!("ops[1] не Linear: {other:?}"),
    }
    assert!(
        notices
            .iter()
            .any(|n| n.kind == NoticeKind::WcsExtended && n.message.contains("G154")),
        "notice W_WCS_EXTENDED для G154"
    );
    // P4 — вне расширенного слот-домена 1..3 → Raw{SEM}.
    let p = parse("G21 G90;\nG154 P4;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    // Без P — сломанный вход (Err).
    assert_eq!(
        err_code(parse("G21 G90;\nG154;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn g52_hard_err_second_reject() {
    // G52 → Err(E_UNSUPPORTED_SEMANTICS) — 2-й вендорский жёсткий отказ
    // (SPEC §8 правило 5): позиционная смена фрейма mid-program. Отмена-
    // форма G52 X0 Y0 Z0 — тоже Err (консервативно, Setting 129/33 `?`).
    assert_eq!(
        err_code(parse("G21 G90;\nG0 X1. Y1. Z1.;\nG52 X0. Y0. Z0.;\nM30;\n")),
        ErrorCode::UnsupportedSemantics
    );
    let Err(d) = parse("G21 G90;\nG52 X1.;\n") else {
        panic!("ожидался Err")
    };
    assert!(d[0].message.contains("hard ERR"));
    assert!(d[0].message.contains("2nd vendor hard-reject"));
}

#[test]
fn g10_l2_wcs_define_full_and_incremental() {
    // G10 L2 P1–P6 → NODE(wcs_define); G90/G91 — абсолют/инкремент записи
    // (§5.3; Haas-пример `G10 L2 P1 G91 X60.` — официальный).
    let p = parse("G21 G90;\nG10 L2 P1 G90 X1. Y2. Z3.;\nG10 L2 P1 G91 X5. Y0 Z0;\nM30;\n")
        .expect("ok");
    match &p.ops[0] {
        Op::WcsDefine { p, origin, .. } => {
            assert_eq!(*p, 1);
            assert_pt(origin, 1.0, 2.0, 3.0);
        }
        other => panic!("ops[0] не WcsDefine: {other:?}"),
    }
    match &p.ops[1] {
        Op::WcsDefine { p, origin, .. } => {
            assert_eq!(*p, 1);
            assert_pt(origin, 6.0, 2.0, 3.0);
        }
        other => panic!("ops[1] не WcsDefine: {other:?}"),
    }
    // P6 — верх домена.
    let p = parse("G21 G90;\nG10 L2 P6 G90 X1. Y1. Z1.;\nM30;\n").expect("ok");
    assert!(matches!(&p.ops[0], Op::WcsDefine { p: 6, .. }));
}

#[test]
fn g10_l2_partial_raw_rt() {
    // Partial G10 L2: невыписанные оси хранят табличные значения (вне
    // текста) → Raw{RT}.
    let p = parse("G21 G90;\nG10 L2 P3 G90 X1.;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::RequiresMachineTable);
            assert!(reason.contains("partial"), "{reason}");
        }
        other => panic!("не Raw: {other:?}"),
    }
}

#[test]
fn g10_l2_p0_g52_slot_collision_pin() {
    // КОЛЛИЗИЯ vs Fanuc: G10 L2 P0 у Haas = запись слота G52 (Haas UK
    // «P0 G52 references work coordinate»), НЕ external/common-offset →
    // Raw{RT} с различающимся диагностическим текстом.
    let p = parse("G21 G90;\nG10 L2 P0 G90 X1. Y1. Z1.;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::RequiresMachineTable);
            assert!(reason.contains("G52 work-coordinate slot"), "{reason}");
            assert!(
                reason.contains("NOT the Fanuc external/common offset"),
                "{reason}"
            );
        }
        other => panic!("не Raw: {other:?}"),
    }
}

#[test]
fn g10_l20_not_wcs_set_here() {
    // Ловушка №2b (та же, что Fanuc): L20 = запись origin доп. СК
    // (G110–G129/G154), НЕ wcs_set_here — маппить запрещено (COVERAGE §2.3).
    let p = parse("G21 G90;\nG10 L20 P2 G90 X100. Y80.;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::RequiresMachineTable);
            assert!(reason.contains("NOT wcs_set_here"), "{reason}");
            assert!(reason.contains("trap #2b"), "{reason}");
        }
        other => panic!("не Raw: {other:?}"),
    }
    // WcsSetHere-опа в программе нет.
    assert!(
        p.ops.iter().all(|op| !matches!(op, Op::WcsSetHere { .. })),
        "L20 → wcs_set_here запрещён"
    );
}

#[test]
fn g10_tool_table_forms_raw_rt() {
    // G10 L1/L10–L13 = записи таблиц инструментов (R = значение; официальный
    // пример `G10 L10 G90 P5 R250.`) → Raw{RT}; R-слово в Frame-плане
    // потреблено (не «слово без потребителя»).
    let p = parse("G21 G90;\nG10 L10 G90 P5 R250.;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
}

#[test]
fn g10_broken_forms() {
    // Без L / L2 без P — Err(INVALID); P вне домена 0..6 — Raw{SYNTAX}.
    assert_eq!(
        err_code(parse("G21 G90;\nG10 G90 X1. Y1. Z1.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse("G21 G90;\nG10 L2 G90 X1. Y1. Z1.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
    let p = parse("G21 G90;\nG10 L2 P7 G90 X1. Y1. Z1.;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSyntax);
            assert!(reason.contains("outside Haas wcs domain"), "{reason}");
        }
        other => panic!("не Raw: {other:?}"),
    }
}

#[test]
fn g92_preset_and_first_motion_anchor() {
    // G92 → frame_preset (META); первый motion-оп после пресетки — start
    // Known(values) (та же WCS).
    let p = parse("G21 G90 G94;\nG92 X1. Y2. Z3.;\nG0 X5. Y5. Z5.;\nM30;\n").expect("ok");
    let pr = p.frame_preset.as_ref().expect("frame_preset");
    assert_pt(&pr.values, 1.0, 2.0, 3.0);
    assert_eq!(pr.wcs, Wcs::G54);
    match &p.ops[0] {
        Op::Rapid { start, .. } => match start {
            StartPoint::Known(s) => assert_pt(s, 1.0, 2.0, 3.0),
            StartPoint::Unknown => panic!("start должен быть Known после пресетки"),
        },
        other => panic!("ops[0] не Rapid: {other:?}"),
    }
    // G92 без осевых слов — сломанный вход.
    assert_eq!(
        err_code(parse("G21 G90;\nG92;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn g92_position_restrictions() {
    // После первого motion-опа — Err (только пресетка, §3); второй G92 — Err.
    assert_eq!(
        err_code(parse("G21 G90;\nG0 X1. Y1. Z1.;\nG92 X0. Y0. Z0.;\nM30;\n")),
        ErrorCode::UnsupportedSemantics
    );
    assert_eq!(
        err_code(parse(
            "G21 G90;\nG92 X1. Y1. Z1.;\nG92 X2. Y2. Z2.;\nM30;\n"
        )),
        ErrorCode::UnsupportedSemantics
    );
}

#[test]
fn g92_s_clamp_raw_sem() {
    // S-кламп-формы в словаре Haas НЕТ (кламп — Setting `?`); S при G92 —
    // не frame_preset → Raw{SEM} (COVERAGE §2.6, паритет Fanuc-строки).
    let p = parse("G21 G90;\nG92 S5000;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(reason.contains("not frame_preset"), "{reason}");
        }
        other => panic!("не Raw: {other:?}"),
    }
    assert!(p.frame_preset.is_none(), "S при G92 — НЕ пресетка");
}

#[test]
fn g53_machine_frame_move() {
    // G53 → state.frame = Machine (правило §5.3: origin активной WCS обязан
    // быть in-text; reading_M = W + O_wcs).
    let p =
        parse("G21 G90;\nG10 L2 P1 G90 X1. Y2. Z3.;\nG0 X5. Y5. Z5.;\nG53 G0 X0 Y0 Z0;\nM30;\n")
            .expect("ok");
    assert_eq!(p.ops.len(), 4);
    match &p.ops[2] {
        Op::Rapid {
            start, end, state, ..
        } => {
            // reading_M = (5,5,5) + (1,2,3) = (6,7,8).
            match start {
                StartPoint::Known(s) => assert_pt(s, 6.0, 7.0, 8.0),
                _ => panic!("start Known"),
            }
            assert_pt(end, 0.0, 0.0, 0.0);
            assert_eq!(state.frame, Frame::Machine);
        }
        other => panic!("ops[2] не Rapid: {other:?}"),
    }
    // Возврат в WCS после machine-хода (точка разрыва (d)):
    // reading_wcs = M − O_wcs = (0,0,0) − (1,2,3) = (−1,−2,−3).
    let p = parse(
        "G21 G90;\nG10 L2 P1 G90 X1. Y2. Z3.;\nG0 X5. Y5. Z5.;\nG53 G0 X0 Y0 Z0;\nG0 X1. Y1. Z1.;\nM30;\n",
    )
    .expect("ok");
    match &p.ops[3] {
        Op::Rapid { start, state, .. } => {
            match start {
                StartPoint::Known(s) => assert_pt(s, -1.0, -2.0, -3.0),
                _ => panic!("start Known"),
            }
            assert_eq!(state.frame, Frame::Wcs);
        }
        other => panic!("ops[3] не Rapid: {other:?}"),
    }
}

#[test]
fn g53_strictness() {
    // Явный G0/G1 обязателен; дуги в machine-frame невыразимы; частичный end
    // → RT (нуль в machine-координатах не наследуется, §2 v0.13); без
    // in-text origin — RT (§5.3b); machine-ход при активной пресетке —
    // невыразим (§5.3a).
    assert_eq!(
        err_code(parse(
            "G21 G90;\nG10 L2 P1 G90 X0. Y0. Z0.;\nG0 X1. Y1. Z1.;\nG53 X0 Y0 Z0;\nM30;\n"
        )),
        ErrorCode::InvalidInput,
        "G53 без явного G0/G1"
    );
    assert_eq!(
        err_code(parse(
            "G21 G90;\nG10 L2 P1 G90 X0. Y0. Z0.;\nG0 X1. Y1. Z1.;\nG53 G2 X0 Y0 Z0 I1. J0.;\nM30;\n"
        )),
        ErrorCode::InvalidInput,
        "G53-дуга"
    );
    assert_eq!(
        err_code(parse(
            "G21 G90;\nG10 L2 P1 G90 X0. Y0. Z0.;\nG0 X1. Y1. Z1.;\nG53 G0 X0;\nM30;\n"
        )),
        ErrorCode::RuntimeState,
        "частичный machine-end"
    );
    assert_eq!(
        err_code(parse("G21 G90;\nG0 X1. Y1. Z1.;\nG53 G0 X0 Y0 Z0;\nM30;\n")),
        ErrorCode::RuntimeState,
        "origin G54 не in-text"
    );
    assert_eq!(
        err_code(parse(
            "G21 G90;\nG92 X1. Y1. Z1.;\nG10 L2 P1 G90 X0. Y0. Z0.;\nG53 G0 X0 Y0 Z0;\nM30;\n"
        )),
        ErrorCode::RuntimeState,
        "machine-ход при активной пресетке (§5.3a)"
    );
}

#[test]
fn wcs_switch_linking_difference_criterion() {
    // Точка разрыва (b): критерий РАЗНОСТИ — reading = pos + O_prev − O_cur;
    // обе системы in-text. Частичный end без второй — RT.
    let p = parse(
        "G21 G90;\nG10 L2 P1 G90 X1. Y2. Z3.;\nG10 L2 P2 G90 X10. Y20. Z30.;\nG0 X5. Y5. Z5.;\nG55;\nG1 X6. Y6. Z6. F100.;\nM30;\n",
    )
    .expect("ok");
    match &p.ops[3] {
        Op::Linear { start, .. } => match start {
            StartPoint::Known(s) => {
                // (5,5,5) + (1,2,3) − (10,20,30) = (−4,−13,−22).
                assert_pt(s, -4.0, -13.0, -22.0);
            }
            _ => panic!("start Known (обе системы in-text)"),
        },
        other => panic!("ops[3] не Linear: {other:?}"),
    }
    // G55 origin не in-text → частичный end при разрыве — RT.
    assert_eq!(
        err_code(parse(
            "G21 G90;\nG10 L2 P1 G90 X1. Y2. Z3.;\nG0 X5. Y5. Z5.;\nG55;\nG1 X6.;\nM30;\n"
        )),
        ErrorCode::RuntimeState
    );
}

// ---------------------------------------------------------------------------
// ЭТАП 2 — Циклы ×Haas-семантика: L-повторы (НЕ K), P-dot dwell, G83 Q ИЛИ
// I/J-K (полный отвод), G73 δ=канон + notice §6.19, G86 рестарт, G98/G99
// ---------------------------------------------------------------------------

#[test]
fn g81_unroll_basic_template() {
    // §7.7 шаблон: traverse (на текущем Z) → approach R → feed дно →
    // retract R (G99-дефолт); feed кадра активен на ВСЕХ производных опах;
    // sub 1..4.
    let p =
        parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G81 X10. Y10. Z-5. R2. F100.;\nG80;\nM30;\n")
            .expect("ok");
    assert_eq!(p.ops.len(), 6); // Rapid + 4 производных + PEND
                                // 1: traverse к отверстию на текущем Z (не слит с подходом).
    match &p.ops[1] {
        Op::Rapid { end, sub, .. } => {
            assert_eq!(known(&p.ops[1]), [0.0, 0.0, 5.0]);
            assert_pt(end, 10.0, 10.0, 5.0);
            assert_eq!(*sub, Some(1));
        }
        other => panic!("ops[1] не Rapid-traverse: {other:?}"),
    }
    // 2: approach к R.
    match &p.ops[2] {
        Op::Rapid { end, sub, .. } => {
            assert_pt(end, 10.0, 10.0, 2.0);
            assert_eq!(*sub, Some(2));
        }
        other => panic!("ops[2] не Rapid-approach: {other:?}"),
    }
    // 3: feed на дно.
    match &p.ops[3] {
        Op::Linear { end, sub, .. } => {
            assert_pt(end, 10.0, 10.0, -5.0);
            assert_eq!(*sub, Some(3));
        }
        other => panic!("ops[3] не Linear: {other:?}"),
    }
    // 4: retract на R (G99).
    match &p.ops[4] {
        Op::Rapid { end, sub, .. } => {
            assert_pt(end, 10.0, 10.0, 2.0);
            assert_eq!(*sub, Some(4));
        }
        other => panic!("ops[4] не Rapid-retract: {other:?}"),
    }
    // feed-снапшот на traverse/approach/retract (§7.7 v0.15.1).
    assert_eq!(feed_of(&p.ops[1]), Some(100.0));
    assert_eq!(feed_of(&p.ops[2]), Some(100.0));
    assert_eq!(feed_of(&p.ops[3]), Some(100.0));
    assert_eq!(feed_of(&p.ops[4]), Some(100.0));
    // DPI F-домен-регресс (Setting 77): bare F100 в кадре цикла → 100 ×
    // 0.0001 = 0.01 units/min на производных опах — НЕ verbatim 100
    // (Haas F-домен ≠ Fanuc integer-mm/min ruling).
    let p2 = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G81 X10. Y10. Z-5. R2. F100;\nM30;\n")
        .expect("ok");
    assert!(
        (feed_of(&p2.ops[3]).unwrap() - 0.01).abs() <= 1e-9,
        "bare F в цикле — Setting 77 ×0.0001, got {}",
        feed_of(&p2.ops[3]).unwrap()
    );
}

#[test]
fn g81_l_repeat_g90_same_position() {
    // L-повторы (НЕ K!): G90 — L = повторы на той же позиции; вырожденные
    // traverse/approach/retract повторов → drop+notice (SPEC §3).
    let (p, notices) = notices_of(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G81 X10. Y10. Z-5. R2. F100. L3;\nG80;\nM30;\n",
    );
    // Позиционирующий Rapid + 1-я итерация (4 опа) + 2 повтора (feed +
    // retract, вырожденные ходы — drop) + PEND = 1 + 4 + 2 + 2 + 1 = 10.
    assert_eq!(p.ops.len(), 10);
    let bottoms = p
        .ops
        .iter()
        .filter(|op| matches!(op, Op::Linear { .. }))
        .count();
    assert_eq!(bottoms, 3, "три сверловки");
    let zero_drops = notices
        .iter()
        .filter(|n| n.kind == NoticeKind::ZeroLengthMotion)
        .count();
    assert_eq!(zero_drops, 4, "вырожденные traverse+approach × 2 повтора");
}

#[test]
fn g81_g91_xy_step_l() {
    // G91: X/Y-слова = шаг между отверстиями; Z = инкремент от R-плоскости;
    // позиции в AST — абсолютные (§7.3). R0: r_cur = initial Z (без дрейфа),
    // z_bottom = 5 − 8 = −3, X-шаг +10.
    let p = parse(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG91;\nG99 G81 X10. Z-8. R0 F100. L3;\nG80;\nM30;\n",
    )
    .expect("ok");
    let mut holes = Vec::new();
    for op in &p.ops {
        if let Op::Linear { end, .. } = op {
            let v = pt3(end);
            if (v[2] + 3.0).abs() <= EPS {
                holes.push([v[0], v[1]]);
            }
        }
    }
    assert_eq!(holes.len(), 3, "три отверстия: {holes:?}");
    assert!((holes[0][0] - 10.0).abs() <= EPS && (holes[0][1] - 0.0).abs() <= EPS);
    assert!((holes[1][0] - 20.0).abs() <= EPS);
    assert!((holes[2][0] - 30.0).abs() <= EPS);
}

#[test]
fn g81_modal_repeat_xy_blocks() {
    // Группа 09 модальна: последующие X/Y-блоки без G8x = новые отверстия.
    let p = parse(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G81 X10. Y10. Z-5. R2. F100.;\nX20. Y20.;\nX30.;\nG80;\nM30;\n",
    )
    .expect("ok");
    assert_eq!(p.ops.len(), 12); // Rapid + 3×4 + PEND
    let holes: Vec<[f64; 2]> = p
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Linear { end, .. } => {
                let v = pt3(end);
                if (v[2] + 5.0).abs() <= EPS {
                    Some([v[0], v[1]])
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();
    assert_eq!(holes.len(), 3);
    assert!((holes[0][0] - 10.0).abs() <= EPS && (holes[0][1] - 10.0).abs() <= EPS);
    assert!((holes[1][0] - 20.0).abs() <= EPS && (holes[1][1] - 20.0).abs() <= EPS);
    assert!((holes[2][0] - 30.0).abs() <= EPS && (holes[2][1] - 20.0).abs() <= EPS);
}

#[test]
fn g80_and_motion_cancel_cycle() {
    // G80 — отмена cycle-моды (IGNORED_SAFE); явный код группы 01 тоже
    // отменяет: следующий ход — движение, не цикл.
    let p = parse(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G81 X10. Y10. Z-5. R2. F100.;\nG80;\nG0 X1. Y1. Z1.;\nM30;\n",
    )
    .expect("ok");
    // Позиционирующий Rapid + G81 (4 производных) + G0 (1 оп, НЕ цикл) + PEND.
    assert_eq!(p.ops.len(), 7);
    match &p.ops[5] {
        Op::Rapid { end, sub, .. } => {
            assert_pt(end, 1.0, 1.0, 1.0);
            assert_eq!(*sub, None, "обычное движение — без sub");
        }
        other => panic!("ops[5] не Rapid: {other:?}"),
    }
    // Явный G1 после цикла (без G80) — тоже отмена.
    let p = parse(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G81 X10. Y10. Z-5. R2. F100.;\nG1 X20. Y20. F100.;\nM30;\n",
    )
    .expect("ok");
    match &p.ops[5] {
        Op::Linear { end, sub, .. } => {
            assert_pt(end, 20.0, 20.0, 2.0);
            assert_eq!(*sub, None);
        }
        other => panic!("ops[5] не Linear: {other:?}"),
    }
}

#[test]
fn g82_dwell_p_dot_rule_both_directions() {
    // P-DOT RULE (уникальное Haas-правило): bare P = МИЛЛИСЕКУНДЫ, dotted P
    // = СЕКУНДЫ; bare — с notice (DPI-домен мс-гранулярности).
    let (p, notices) =
        notices_of("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G82 X10. Z-5. R2. P500 F100.;\nM30;\n");
    let dwells: Vec<_> = p
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Dwell { seconds, .. } => Some(seconds.value()),
            _ => None,
        })
        .collect();
    assert_eq!(dwells, vec![0.5], "bare P500 = 500 мс = 0.5 s");
    assert!(
        notices.iter().any(|n| n.kind == NoticeKind::DpiAssumed
            && n.message.contains("P-dot rule")
            && n.message.contains("500")),
        "notice P-dot для bare P"
    );
    // Dotted P0.5 → 0.5 s verbatim, БЕЗ P-dot-notice.
    let (p2, notices2) =
        notices_of("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G82 X10. Z-5. R2. P0.5 F100.;\nM30;\n");
    let dwells2: Vec<_> = p2
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Dwell { seconds, .. } => Some(seconds.value()),
            _ => None,
        })
        .collect();
    assert_eq!(dwells2, vec![0.5], "dotted P0.5 = 0.5 s verbatim");
    assert!(
        notices2.iter().all(|n| n.kind != NoticeKind::DpiAssumed),
        "dotted P — без DPI-notice"
    );
}

#[test]
fn g82_without_p_invalid() {
    // G82 без P — сломанный вход.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G82 X10. Z-5. R2. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn g86_dwell_required_spindle_restart() {
    // G86 (SPEC §7.4a v0.18): feed → DWELL → STOP_SPINDLE → RAPID-out →
    // рестарт в прежнем направлении (Q9 — в state последующих опов).
    let p = parse(
        "G21 G90 G94;\nS1000;\nM3;\nG0 X0. Y0. Z5.;\nG99 G86 X10. Z-5. R2. P500 F100.;\nG1 X20.;\nM30;\n",
    )
    .expect("ok");
    // Linear + traverse + approach + feed + dwell + rapid(off) + Linear(restart) + PEND.
    assert_eq!(p.ops.len(), 8);
    let dwell = match &p.ops[4] {
        Op::Dwell { seconds, .. } => seconds.value(),
        other => panic!("ops[4] не Dwell: {other:?}"),
    };
    assert!((dwell - 0.5).abs() <= EPS, "P500 bare → 0.5 s");
    match &p.ops[5] {
        Op::Rapid { state, .. } => {
            assert_eq!(state.spindle, Spindle::Off, "шпиндель off на rapid-out");
        }
        other => panic!("ops[5] не Rapid: {other:?}"),
    }
    match &p.ops[6] {
        Op::Linear { state, .. } => {
            assert_eq!(
                state.spindle,
                Spindle::Cw(eid_core::ast::Num::new("1000".into(), 1000.0)),
                "рестарт в прежнем направлении/оборотах (Q9)"
            );
        }
        other => panic!("ops[6] не Linear: {other:?}"),
    }
}

#[test]
fn g86_without_p_rs274_message() {
    // SPEC v0.18: G86 без P — rs274 отказывает; сообщение зеркалируется
    // дословно (зонд e2e48).
    let Err(d) = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G86 X10. Z-5. R2. F100.;\nM30;\n")
    else {
        panic!("ожидался Err")
    };
    assert_eq!(d[0].code, ErrorCode::InvalidInput);
    assert_eq!(d[0].message, "Dwell time p word missing with g86");
}

#[test]
fn g83_q_full_retract_sequence_and_clearance() {
    // G83 Q-форма: полный отвод к R каждый пек; клиренс канон §7.4a v0.18 =
    // 0.01in unit-scaled (0.254 mm в G21). R1, Z−6, Q2 → депеки
    // [−1,−3,−5,−6].
    let p =
        parse("G21 G90 G94 G17;\nG0 X0. Y0. Z3.;\nG99 G83 X10. Z-6. R1. Q2. F50.;\nG80;\nM30;\n")
            .expect("ok");
    // Позиционирующий Rapid + traverse + approach + 4×feed + 3×(rapid R +
    // rapid appr) + retract = 14 (+ PEND = 15).
    assert_eq!(p.ops.len(), 15);
    let ends: Vec<[f64; 3]> = p.ops[1..13].iter().map(end_of3).collect();
    let expected: Vec<[f64; 3]> = vec![
        [10.0, 0.0, 3.0],    // traverse (текущий Z)
        [10.0, 0.0, 1.0],    // approach R
        [10.0, 0.0, -1.0],   // пек 1
        [10.0, 0.0, 1.0],    // полный отвод R
        [10.0, 0.0, -0.746], // approach d+δ (δ = 0.254 mm!)
        [10.0, 0.0, -3.0],   // пек 2
        [10.0, 0.0, 1.0],
        [10.0, 0.0, -2.746],
        [10.0, 0.0, -5.0], // пек 3
        [10.0, 0.0, 1.0],
        [10.0, 0.0, -4.746],
        [10.0, 0.0, -6.0], // дно
    ];
    for (i, (got, want)) in ends.iter().zip(&expected).enumerate() {
        assert!(
            got.iter()
                .zip(want.iter())
                .all(|(a, b)| (a - b).abs() <= EPS),
            "op {}: {got:?} vs {want:?}",
            i + 1
        );
    }
    // Финальный retract G99 → R.
    match &p.ops[13] {
        Op::Rapid { end, .. } => assert_pt(end, 10.0, 0.0, 1.0),
        other => panic!("ops[13] не retract: {other:?}"),
    }
}

#[test]
fn g83_clearance_inch_unit_scaled() {
    // Клиренс 0.01in unit-scaled: в G20 — ровно 0.0100 in (зонд rs274
    // g83_inch; SPEC §7.4a v0.18; профиль константу НЕ переопределяет).
    let p =
        parse("G20 G90 G94 G17;\nG0 X0. Y0. Z0.5;\nG99 G83 X1. Z-0.6 R0.1 Q0.2 F5.;\nG80;\nM30;\n")
            .expect("ok");
    let ends: Vec<[f64; 3]> = p.ops[1..13].iter().map(end_of3).collect();
    let appr = ends[4][2];
    assert!(
        (appr - (-0.1 + 0.01)).abs() <= EPS,
        "клиренс = 0.0100 in в G20, got {appr}"
    );
}

#[test]
fn g83_ijk_variable_pecking() {
    // Переменный пекинг (Haas-расширение §5.3): d1 = I, dk = max(K,
    // d_(k−1) − J); дно клампится на z_bottom; полный отвод между пеками.
    // R1, Z−7, I2, J0.5, K1 → шаги d: 2, 1.5, 1, 1, … → депеки
    // [−1, −2.5, −3.5, −4.5, −5.5, −6.5, −7].
    let p = parse(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z3.;\nG99 G83 X10. Z-7. R1. I2. J0.5 K1. F50.;\nG80;\nM30;\n",
    )
    .expect("ok");
    let bottoms: Vec<f64> = p
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Linear {
                end, sub: Some(_), ..
            } => {
                let v = pt3(end);
                if v[0] > 5.0 && v[2] < 0.9 {
                    Some(v[2])
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();
    let want = [-1.0, -2.5, -3.5, -4.5, -5.5, -6.5, -7.0];
    assert_eq!(bottoms.len(), want.len(), "{bottoms:?}");
    for (g, w) in bottoms.iter().zip(&want) {
        assert!((g - w).abs() <= EPS, "{bottoms:?}");
    }
    // K = min-пек: третий шаг ограничен K=1 (d_3 = max(1, 1.5−0.5) = 1.0 →
    // депек −3.5, не −4.0).
    assert!(
        (bottoms[2] - (-3.5)).abs() <= EPS,
        "K=1 ограничивает пек: {bottoms:?}"
    );
}

#[test]
fn g83_q_and_ijk_mutually_exclusive() {
    // «Do not use a Q value when programming with I, J, and K» — Err.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G83 X10. Z-6. R1. Q2. I2. J0.5 K1. F50.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn g83_without_q_or_ijk() {
    // Ни Q, ни I/J-K — сломанный вход.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G83 X10. Z-6. R1. F50.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn g73_chip_break_single_rapid_delta_canon() {
    // G73 (v0.18): ОДИН промежуточный rapid d+δ на пек (модели min(R,d+Q)
    // НЕТ); δ = канон 0.254mm (Setting 22 вне текста → notice
    // W_CYCLE_PARAM_ASSUMED с текстом диалектного расхождения §6.19).
    let (p, notices) = notices_of(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z3.;\nG99 G73 X10. Z-6. R1. Q2. F50.;\nG80;\nM30;\n",
    );
    // Позиционирующий Rapid + traverse + approach + 4×feed + 3×rapid(d+δ) +
    // retract = 11 (+ PEND = 12).
    assert_eq!(p.ops.len(), 12);
    let ends: Vec<[f64; 3]> = p.ops[1..11].iter().map(end_of3).collect();
    let expected: Vec<[f64; 3]> = vec![
        [10.0, 0.0, 3.0],
        [10.0, 0.0, 1.0],
        [10.0, 0.0, -1.0],
        [10.0, 0.0, -0.746], // одиночный rapid d1+δ
        [10.0, 0.0, -3.0],
        [10.0, 0.0, -2.746],
        [10.0, 0.0, -5.0],
        [10.0, 0.0, -4.746],
        [10.0, 0.0, -6.0],
        [10.0, 0.0, 1.0], // финал G99 → R
    ];
    for (i, (got, want)) in ends.iter().zip(&expected).enumerate() {
        assert!(
            got.iter()
                .zip(want.iter())
                .all(|(a, b)| (a - b).abs() <= EPS),
            "op {}: {got:?} vs {want:?}",
            i + 1
        );
    }
    // Промежуточных полных отводов на R НЕТ (после approach — только финал).
    let r_visits = ends.iter().filter(|e| (e[2] - 1.0).abs() <= EPS).count();
    assert_eq!(
        r_visits, 2,
        "approach + финальный retract, без полных отводов"
    );
    let cpa: Vec<_> = notices
        .iter()
        .filter(|n| n.kind == NoticeKind::CycleParamAssumed)
        .collect();
    assert_eq!(cpa.len(), 1, "notice W_CYCLE_PARAM_ASSUMED: {notices:?}");
    assert!(cpa[0].message.contains("Setting 22"));
    assert!(cpa[0].message.contains("0.254"), "канон-константа в тексте");
    assert!(cpa[0].message.contains("6.19"), "диалектное расхождение");
}

#[test]
fn g73_ijk_variable_peck_raw_sem() {
    // G73 с I/J-K — переменный пекинг не сверен на mill (`?` COVERAGE §6,
    // реестр §10(4)) → Raw{SEM}, не разворачивается.
    let p = parse(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G73 X10. Z-6. R1. I2. J0.5 K1. F50.;\nM30;\n",
    )
    .expect("ok");
    match &p.ops[1] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(reason.contains("unverified on Haas mill"), "{reason}");
        }
        other => panic!("не Raw: {other:?}"),
    }
}

#[test]
fn g85_g89_feed_out_retract() {
    // G85/G89: отвод на ПОДАЧЕ (§7.4 SPEC v0.14); G89 — dwell перед отводом.
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G85 X10. Z-5. R2. F100.;\nM30;\n")
        .expect("ok");
    match &p.ops[4] {
        Op::Linear { end, .. } => assert_pt(end, 10.0, 0.0, 2.0), // feed-out
        other => panic!("ops[4] не Linear-retract: {other:?}"),
    }
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G89 X10. Z-5. R2. P0.25 F100.;\nM30;\n")
        .expect("ok");
    match &p.ops[4] {
        Op::Dwell { seconds, .. } => assert!((seconds.value() - 0.25).abs() <= EPS),
        other => panic!("ops[4] не Dwell: {other:?}"),
    }
    match &p.ops[5] {
        Op::Linear { end, .. } => assert_pt(end, 10.0, 0.0, 2.0), // feed-out после dwell
        other => panic!("ops[5] не Linear-retract: {other:?}"),
    }
}

#[test]
fn g98_initial_z_and_g99_default() {
    // G98 → отвод в initial Z; отсутствие G98/G99 → дефолт G99 (SPEC §7.5a).
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG98 G81 X10. Z-5. R2. F100.;\nG80;\nM30;\n")
        .expect("ok");
    match &p.ops[4] {
        Op::Rapid { end, .. } => assert_pt(end, 10.0, 0.0, 5.0),
        other => panic!("ops[4] не retract: {other:?}"),
    }
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG81 X10. Z-5. R2. F100.;\nG80;\nM30;\n")
        .expect("ok");
    match &p.ops[4] {
        Op::Rapid { end, .. } => assert_pt(end, 10.0, 0.0, 2.0),
        other => panic!("ops[4] не retract: {other:?}"),
    }
}

#[test]
fn g98_first_motion_op_rt() {
    // SPEC §7.6: цикл первым motion-опом — позиция не определена → RT.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG99 G81 X10. Z-5. R2. F100.;\nM30;\n"
        )),
        ErrorCode::RuntimeState
    );
}

#[test]
fn cycle_l0_raw_sem() {
    // L0 `?` — «запись данных без исполнения»: до сверки не разворачивать
    // (COVERAGE §2.5; ≠ Fanuc K0-G90-noop ruling).
    let p = parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G81 X10. Z-5. R2. F100. L0;\nM30;\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(reason.contains("L0"), "{reason}");
        }
        other => panic!("не Raw: {other:?}"),
    }
    assert_eq!(p.ops.len(), 3, "развёртки нет");
}

#[test]
fn cycle_k_is_not_repeat() {
    // K — НЕ повтор цикла на Haas (min-пек G83 / угловой шаг G12): K3 в
    // G81-блоке НЕ порождает повторов (перенос Fanuc-K-семантики запрещён,
    // §1.2); L3 — разворачивается (L/K-различение).
    let p =
        parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G81 X10. Z-5. R2. K3 F100.;\nG80;\nM30;\n")
            .expect("ok");
    let bottoms = p
        .ops
        .iter()
        .filter(|op| matches!(op, Op::Linear { sub: Some(_), .. }))
        .count();
    assert_eq!(bottoms, 1, "K3 не разворачивается — одна сверловка");
    let p =
        parse("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G81 X10. Z-5. R2. L3 F100.;\nG80;\nM30;\n")
            .expect("ok");
    let bottoms = p
        .ops
        .iter()
        .filter(|op| matches!(op, Op::Linear { sub: Some(_), .. }))
        .count();
    assert_eq!(bottoms, 3, "L3 → три сверловки");
}

#[test]
fn cycle_missing_params() {
    // Без R / без Z / без F — Err (паритет «It is an error if…»).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG81 X10. Z-5. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG81 X10. R2. F100.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG81 X10. Z-5. R2.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn cycle_refusals_raw_sem_modal_on_raw() {
    // Циклы-отказы → Raw{SEM} + (с осями) W_MODAL_ON_RAW: motion/цикл не
    // продолжаются; следующие ходы не интерпретируются молча.
    for code in ["G84", "G74", "G76", "G77", "G156", "G174", "G184"] {
        let (p, notices) = notices_of(&format!(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\n{code} X10. Z-5. R2. F50.;\nM30;\n"
        ));
        match &p.ops[1] {
            Op::Raw {
                code: c, reason, ..
            } => {
                assert_eq!(*c, ErrorCode::UnsupportedSemantics, "{code}");
                assert!(!reason.is_empty());
            }
            other => panic!("{code} не Raw: {other:?}"),
        }
        assert!(
            notices.iter().any(|n| n.kind == NoticeKind::ModalOnRaw),
            "{code}: нет W_MODAL_ON_RAW"
        );
    }
    // Тексты-коллизии: G76 — I/J-направление В ТЕКСТЕ (3-й смысл кода,
    // §7.16); G84 — rigid внутренне (Setting 57), без M29.
    let p = parse("G21 G90;\nG0 X0. Y0. Z5.;\nG76 X10. Z-5. R2. Q0.1 I0.05 J0. F50.;\nM30;\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Raw { reason, .. } => assert!(reason.contains("I/J words IN TEXT"), "{reason}"),
        other => panic!("G76 не Raw: {other:?}"),
    }
    let p = parse("G21 G90;\nG0 X0. Y0. Z5.;\nG84 X10. Z-5. R2. F50.;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Raw { reason, .. } => assert!(reason.contains("no M29"), "{reason}"),
        other => panic!("G84 не Raw: {other:?}"),
    }
    // После raw-цикла с осями голый X — отказ (режим не продолжается).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG84 X10. Z-5. R2. F50.;\nX20.;\nM30;\n"
        )),
        ErrorCode::InvalidInput
    );
}

// ---------------------------------------------------------------------------
// ЭТАП 2 — M-словарь: M0/M1, M6/M16+T, вендорское ведро (коллизии-пины)
// ---------------------------------------------------------------------------

#[test]
fn m0_m1_program_stop() {
    let p = parse("G21 G90;\nM0;\nM1;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::ProgramStop { optional, .. } => assert!(!optional, "M0 — не optional"),
        other => panic!("ops[0] не ProgramStop: {other:?}"),
    }
    match &p.ops[1] {
        Op::ProgramStop { optional, .. } => assert!(*optional, "M1 — optional (Q8)"),
        other => panic!("ops[1] не ProgramStop: {other:?}"),
    }
}

#[test]
fn m6_m16_tool_change_idiom() {
    // Идиома `T01 M06` → ToolChange; tool в state последующих опов; M16 —
    // альтернативный код смены (то же).
    for (src, want_tool) in [
        ("G21 G90;\nT01 M06;\nG1 X5. F100.;\nM30;\n", 1u32),
        ("G21 G90;\nT5 M16;\nG1 X5. F100.;\nM30;\n", 5u32),
    ] {
        let p = parse(src).expect("ok");
        match &p.ops[0] {
            Op::ToolChange { tool, .. } => assert_eq!(*tool, want_tool),
            other => panic!("ops[0] не ToolChange: {other:?}"),
        }
        match &p.ops[1] {
            Op::Linear { state, .. } => assert_eq!(state.tool, Some(want_tool)),
            other => panic!("ops[1] не Linear: {other:?}"),
        }
    }
    // M6/M16 без T — сломанный вход.
    assert_eq!(
        err_code(parse("G21 G90;\nM6;\nM30;\n")),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse("G21 G90;\nM16;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn t_alone_preload_raw_sem() {
    // Одиночное T = предвыбор next-tool (SMTC) → Raw{SEM}, candidate
    // `selected_tool`; state.tool НЕ меняется.
    let p = parse("G21 G90;\nT5;\nG1 X5. F100.;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(reason.contains("preload"), "{reason}");
        }
        other => panic!("не Raw: {other:?}"),
    }
    match &p.ops[1] {
        Op::Linear { state, .. } => assert_eq!(state.tool, None, "предвыбор — не установка"),
        other => panic!("ops[1] не Linear: {other:?}"),
    }
}

#[test]
fn m07_shower_collision_pin() {
    // КОЛЛИЗИЯ vs Fanuc: M07 = Shower — ВТОРОЙ канал СОЖ, НЕ mist (Fanuc
    // M07 = mist-ABSORB) → Raw{SEM}; coolant.mist НЕ устанавливается.
    let p = parse("G21 G90;\nM07;\nG1 X5. F100.;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(reason.contains("NOT mist"), "{reason}");
            assert!(reason.contains("second coolant channel"), "{reason}");
        }
        other => panic!("не Raw: {other:?}"),
    }
    match &p.ops[1] {
        Op::Linear { state, .. } => {
            assert!(!state.coolant.mist, "M07 ≠ mist (≠ Fanuc)");
            assert!(!state.coolant.flood);
        }
        other => panic!("ops[1] не Linear: {other:?}"),
    }
}

#[test]
fn m10_m12_brakes_raw_sem() {
    // M10/M11 — тормоз 4-й оси, M12/M13 — 5-й оси (гипотеза «M13 =
    // шпиндель+СОЖ» опровергнута research'ом) → Raw{SEM};
    // M13-пин — m13_brake_collision_pin.
    for code in ["M10", "M11", "M12"] {
        let p = parse(&format!("G21 G90;\n{code};\nM30;\n")).expect("ok");
        assert!(
            matches!(
                &p.ops[0],
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
fn m29_relay_collision_pin() {
    // КОЛЛИЗИЯ vs Fanuc: M29 = output relay с M-Fin, НЕ rigid-tap mode
    // (rigid tap Haas = Setting 57 + G84 внутренне, без M29).
    let p = parse("G21 G90;\nM29;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(reason.contains("NOT rigid-tap"), "{reason}");
            assert!(reason.contains("output relay"), "{reason}");
        }
        other => panic!("не Raw: {other:?}"),
    }
}

#[test]
fn m19_subs_m46_m96_m109_raw() {
    let p = parse("G21 G90;\nM19;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    // M97 (Haas-специфика — P = N-номер)/M98/M99/M46/M96 → Raw SYNTAX.
    for code in ["M97 P10 L2", "M98 P0010 L3", "M99", "M46", "M96 P20 Q3"] {
        let p = parse(&format!("G21 G90;\n{code};\nM30;\n")).expect("ok");
        match &p.ops[0] {
            Op::Raw {
                code: c, reason, ..
            } => {
                assert_eq!(*c, ErrorCode::UnsupportedSyntax, "{code}");
                assert!(!reason.is_empty());
            }
            other => panic!("{code} не Raw: {other:?}"),
        }
    }
    // M97 — Haas-специфика в тексте (≠ Fanuc, где M97 нет).
    let p = parse("G21 G90;\nM97 P10;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Raw { reason, .. } => assert!(reason.contains("Haas-specific"), "{reason}"),
        other => panic!("не Raw: {other:?}"),
    }
    // M109 — ожидание ввода оператора → Raw{RUNTIME_STATE}.
    let p = parse("G21 G90;\nM109;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::RuntimeState,
            ..
        }
    ));
    // Неизвестный M64 — generic SYNTAX.
    let p = parse("G21 G90;\nM64;\nM30;\n").expect("ok");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
}

// ---------------------------------------------------------------------------
// ЭТАП 2 — Компенсации: G43/G44/G143 → RT; семейства Haas-состава
// ---------------------------------------------------------------------------

#[test]
fn comp_active_codes_raw() {
    // G41/G42 (+D) / G141 → Raw{SEM} (comp-геометрия; G141 — Haas 3D+);
    // G43/G44/G143 → Raw{RT} (TLO из таблицы); G234 → Raw{SEM} (TCPC).
    for (code, want) in [
        ("G41 D5", ErrorCode::UnsupportedSemantics),
        ("G42 D5", ErrorCode::UnsupportedSemantics),
        ("G141", ErrorCode::UnsupportedSemantics),
        ("G43 H2", ErrorCode::RequiresMachineTable),
        ("G44 H2", ErrorCode::RequiresMachineTable),
        ("G143 H2", ErrorCode::RequiresMachineTable),
        ("G234", ErrorCode::UnsupportedSemantics),
    ] {
        let p = parse(&format!("G21 G90;\n{code};\nM30;\n")).expect("ok");
        match &p.ops[0] {
            Op::Raw { code: c, .. } => assert_eq!(*c, want, "{code}"),
            other => panic!("{code} не Raw: {other:?}"),
        }
    }
    // G141 — член Haas-семейства G40 в тексте диагностики.
    let p = parse("G21 G90;\nG141;\nM30;\n").expect("ok");
    match &p.ops[0] {
        Op::Raw { reason, .. } => assert!(reason.contains("G40-family"), "{reason}"),
        other => panic!("не Raw: {other:?}"),
    }
}

#[test]
fn comp_cancel_ignored_safe_without_family() {
    // G40/G49 без активных кодов семейств в файле — IGNORED_SAFE (опа нет).
    let p = parse("G21 G90;\nG40;\nG49;\nG0 X1. Y1. Z1.;\nM30;\n").expect("ok");
    assert_eq!(p.ops.len(), 2); // Rapid + PEND; cancel-кодов в ops нет
}

#[test]
fn comp_cancel_with_active_family_raw() {
    // Правило семейств §1.3 (HAAS-СОСТАВЫ ≠ Fanuc): G40 при живой семье
    // {G41,G42,G141} → Raw{SEM}; G49 при {G43,G44,G143} → Raw{RT}.
    let p = parse("G21 G90;\nG41 D5;\nG40;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(reason.contains("family rule"), "{reason}");
            assert!(reason.contains("G141"), "Haas-состав в тексте");
        }
        other => panic!("G40 не Raw: {other:?}"),
    }
    let p = parse("G21 G90;\nG143 H3;\nG49;\nM30;\n").expect("ok");
    match &p.ops[1] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::RequiresMachineTable);
            assert!(reason.contains("G143"), "Haas-состав в тексте");
        }
        other => panic!("G49 не Raw: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// ЭТАП 2 — Macro B: bucket + система-переменные ≠ Fanuc в диагностике
// ---------------------------------------------------------------------------

#[test]
fn macro_b_system_var_ranges_in_diag() {
    // Система-переменные Haas ≠ Fanuc (#500–#699 non-volatile vs #500–#999;
    // #109xxx NGC-расширения) — в диагностике (бриф этапа 2, не молча).
    let p = parse("G21 G90;\n#500 = 5;\nIF [#10930 GT 0] GOTO 9;\nM30;\n").expect("ok");
    for op in &p.ops[0..2] {
        match op {
            Op::Raw { code, reason, .. } => {
                assert_eq!(*code, ErrorCode::UnsupportedSyntax);
                assert!(reason.contains("#500–#699"), "{reason}");
                assert!(reason.contains("#109xxx"), "{reason}");
                assert!(reason.contains("mapping table"), "{reason}");
            }
            other => panic!("не Raw: {other:?}"),
        }
    }
    // Соседние статические ходы парсятся (правило двух классов).
    let p = parse("G21 G90;\n#100 = 1;\nG1 X5. F100.;\nM30;\n").expect("ok");
    assert!(matches!(&p.ops[1], Op::Linear { .. }));
}

// ---------------------------------------------------------------------------
// ЭТАП 2 — G4 dwell: P-dot rule; X/U-форм НЕТ; P0 → drop; цикл не отменяет
// ---------------------------------------------------------------------------

#[test]
fn g4_p_dot_rule_both_directions() {
    // Dotted P = секунды verbatim; bare P = миллисекунды + notice.
    let (p, notices) = notices_of("G21 G90 G94;\nG4 P2.5;\nM30;\n");
    match &p.ops[0] {
        Op::Dwell { seconds, .. } => assert!((seconds.value() - 2.5).abs() <= EPS),
        other => panic!("ops[0] не Dwell: {other:?}"),
    }
    assert!(notices.iter().all(|n| n.kind != NoticeKind::DpiAssumed));
    let (p, notices) = notices_of("G21 G90 G94;\nG4 P500;\nM30;\n");
    match &p.ops[0] {
        Op::Dwell { seconds, .. } => {
            assert!((seconds.value() - 0.5).abs() <= EPS, "P500 bare = 500 мс")
        }
        other => panic!("ops[0] не Dwell: {other:?}"),
    }
    assert!(
        notices
            .iter()
            .any(|n| n.kind == NoticeKind::DpiAssumed && n.message.contains("P-dot rule")),
        "notice P-dot rule для bare P"
    );
}

#[test]
fn g4_x_u_forms_absent() {
    // X/U-форм dwell у Haas НЕТ (§1.2): G4 X1. / U1. — слово без
    // потребителя → Err(INVALID); G4 без dwell-слова → Err.
    assert_eq!(
        err_code(parse("G21 G90;\nG4 X1.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse("G21 G90;\nG4 U1.;\nM30;\n")),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse("G21 G90;\nG4;\nM30;\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn g4_p0_drop_notice() {
    // `G04 P0` — no-op → drop + notice (SPEC §5.2).
    let (p, notices) = notices_of("G21 G90 G94;\nG4 P0;\nG1 X5. F100.;\nM30;\n");
    assert!(p.ops.iter().all(|op| !matches!(op, Op::Dwell { .. })));
    assert!(notices
        .iter()
        .any(|n| n.kind == NoticeKind::ZeroLengthDwell));
}

#[test]
fn g4_does_not_cancel_cycle() {
    // G4 — one-shot группа 00: модальный цикл группы 09 НЕ отменяется —
    // следующий X/Y-блок = повтор сверления.
    let p = parse(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G81 X10. Z-5. R2. F100.;\nG4 P0.5;\nX20.;\nG80;\nM30;\n",
    )
    .expect("ok");
    let bottoms = p
        .ops
        .iter()
        .filter(|op| matches!(op, Op::Linear { sub: Some(_), .. }))
        .count();
    assert_eq!(bottoms, 2, "цикл пережил G4");
    let dwells = p
        .ops
        .iter()
        .filter(|op| matches!(op, Op::Dwell { .. }))
        .count();
    assert_eq!(dwells, 1, "G4 между отверстиями — один Dwell-оп");
}
