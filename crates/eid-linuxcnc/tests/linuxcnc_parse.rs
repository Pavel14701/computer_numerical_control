//! Тесты канонизатора LinuxCNC (Агент D, итерация 2).
//!
//! Позитив: пилотные векторы Tier A #01, #02, #03, #11, #27, #36, #46 —
//! сравнение с golden AST через `eid_harness::diff::semantic_diff`
//! (допуски EQUIVALENCE §4; src/sub вне эквивалентности).
//!
//! Негатив (правило двух классов, CORPUS §1.2 v0.5):
//!  * Err-класс (структурные): N1, N2, N5a, N5b, N7, N12, N13, N14, N15,
//!    N16, N17, N19 — точные коды;
//!  * Ok-класс (line-level RAW + диагностика): N3, N8, N9, N10, N18.
//!
//! Отложено (СТОП-вопрос отчёта — нужен парсер/развёртывание циклов,
//! итерация 3): N4 (G83 L-лимит) и N6 (голые координаты после G80 — файл
//! содержит G81). N11 (M98) — жёсткий Err E_UNSUPPORTED_SYNTAX; решение
//! оркестратора CORPUS §1.2 v0.5.2 подтвердило Err.

use eid_core::adapter::{Adapter, EidTextAdapter};
use eid_core::ast::{Op, PathMode, StartPoint};
use eid_core::diag::ErrorCode;
use eid_core::parser::NoticeKind;
use eid_harness::diff::{semantic_diff, Divergence, DiffConfig};
use eid_linuxcnc::LinuxCncAdapter;
use std::path::PathBuf;

fn corpus(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus/tier_a/v0");
    p.push(rel);
    p
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(corpus(rel)).expect("corpus file readable")
}

fn parse_linuxcnc(rel: &str) -> Result<eid_core::ast::Program, Vec<eid_core::diag::Diagnostic>> {
    LinuxCncAdapter::new().parse(&read(rel), rel)
}

fn parse_golden(rel: &str) -> eid_core::ast::Program {
    EidTextAdapter::new()
        .parse(&read(rel), rel)
        .expect("golden AST parses via eid-core")
}

fn assert_diff_empty(a: &eid_core::ast::Program, b: &eid_core::ast::Program, ctx: &str) {
    let d = semantic_diff(a, b, &DiffConfig::default());
    assert!(d.is_empty(), "{ctx}: unexpected divergences: {d:?}");
}

fn assert_err_code(rel: &str, code: ErrorCode, ctx: &str) {
    match parse_linuxcnc(rel) {
        Ok(p) => panic!("{ctx}: expected Err({code:?}), got Ok with {} ops", p.ops.len()),
        Err(diags) => {
            assert!(!diags.is_empty(), "{ctx}: empty diagnostics");
            for d in &diags {
                assert_eq!(
                    d.code, code,
                    "{ctx}: expected {code:?}, got {:?} ({})",
                    d.code, d.message
                );
            }
        }
    }
}

fn assert_ok_diag(rel: &str, code: ErrorCode, ctx: &str) -> eid_core::ast::Program {
    let adapter = LinuxCncAdapter::new();
    let p = match adapter.parse(&read(rel), rel) {
        Ok(p) => p,
        Err(diags) => panic!(
            "{ctx}: expected Ok + diag {code:?}, got Err: {:?}",
            diags.iter().map(|d| (d.code, &d.message)).collect::<Vec<_>>()
        ),
    };
    let codes: Vec<ErrorCode> = adapter.take_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&code),
        "{ctx}: expected diagnostics to contain {code:?}, got {codes:?}"
    );
    p
}

// ===========================================================================
// Позитивные пилотные векторы
// ===========================================================================

#[test]
fn pilot_01_linear_box() {
    let mine = parse_linuxcnc("01_linear_box/linear_box.linuxcnc.ngc").expect("parses");
    let golden = parse_golden("01_linear_box/golden.eid");
    assert_eq!(mine.units, eid_core::ast::Units::Mm);
    assert_eq!(mine.path_mode, PathMode::ExactPath);
    assert!(mine.frame_preset.is_none());
    assert_eq!(mine.ops.len(), 6); // RAPID + 4×LINEAR + PEND
    assert_diff_empty(&mine, &golden, "#01");
}

#[test]
fn pilot_02_rapid_reposition() {
    let mine = parse_linuxcnc("02_rapid_reposition/rapid_reposition.linuxcnc.ngc").expect("parses");
    let golden = parse_golden("02_rapid_reposition/golden.eid");
    assert_eq!(mine.ops.len(), 9); // 8×motion + PEND
    assert_diff_empty(&mine, &golden, "#02");
}

#[test]
fn pilot_03_feed_survives_rapid() {
    let mine = parse_linuxcnc("03_feed_survives_rapid/feed_survives_rapid.linuxcnc.ngc").expect("parses");
    let golden = parse_golden("03_feed_survives_rapid/golden.eid");
    // feed=500 на rapid — модальное значение, G0 не сбрасывает (SPEC §4).
    assert_diff_empty(&mine, &golden, "#03");
}

#[test]
fn pilot_11_arc_r_semicircle() {
    // Ill-conditioned case (CORPUS §1.1 #11, EQUIVALENCE §4 v0.3): правило
    // SPEC §6 (центр из start/end/R/dir/normal, середина±перпендикуляр) на
    // 4dp-концах файла даёт center ≈ (−0.0351, 0.0001), sweep ≈ 179.4975°.
    // Итерация 2 подняла СТОП: рукописный golden (design-значения
    // (0,0)/179.9°) был не выводим правилом; оркестратор перегенерировал
    // golden в производные значения (CORPUS v0.5.2) и зафиксировал
    // single-final-rounding (SPEC v0.11 §10.5) + sweep-амплификацию допуска
    // (EQUIVALENCE v0.3). Проверяем, что канонизатор воспроизводит golden.
    let mine = parse_linuxcnc("11_arc_R_semicircle/arc_R_semicircle.linuxcnc.ngc").expect("parses");
    let golden = parse_golden("11_arc_R_semicircle/golden.eid");

    // Тригонометрия правила §6 на данных файла:
    let arc = match &mine.ops[2] {
        Op::Arc { start, end, center, sweep_deg, dir, pitch, .. } => {
            assert!(*dir == eid_core::ast::Dir::Cw);
            assert_eq!(pitch.value, 0.0);
            let (s, e) = match (start, end) {
                (StartPoint::Known(s), e) => (s.clone(), e.clone()),
                _ => panic!("arc start must be known"),
            };
            // Радиусы по правилу == R ровно (5e-4 допуск на снап 4dp):
            let dxs = s.x.value - center.x.value;
            let dys = s.y.value - center.y.value;
            let dxe = e.x.value - center.x.value;
            let dye = e.y.value - center.y.value;
            let rs = (dxs * dxs + dys * dys).sqrt();
            let re = (dxe * dxe + dye * dye).sqrt();
            assert!((rs - 10.0).abs() < 5e-4, "ρ_start = {rs}");
            assert!((re - 10.0).abs() < 5e-4, "ρ_end = {re}");
            let sw = sweep_deg.value;
            assert!((179.49..=179.51).contains(&sw), "sweep by rule = {sw}");
            sw
        }
        other => panic!("op 2 must be Arc, got {other:?}"),
    };
    let _ = arc;

    // CORPUS v0.5.2 + SPEC v0.11 §10.5: golden #11 перегенерирован в
    // производные значения правила §6 (single-final-rounding) — они
    // совпадают с выводом канонизатора побитово
    // (center=-0.0351,0.0001; sweep=179.4975); расхождение итерации 2
    // устранено ревизией golden.
    assert_diff_empty(&mine, &golden, "#11 (golden = derived values, CORPUS v0.5.2)");
}

#[test]
fn pilot_27_tool_change() {
    // T3 M6 → ToolChange узел (R4-отказ для GRBL — вне скоупа парсера D).
    let mine = parse_linuxcnc("27_tool_change/tool_change.linuxcnc.ngc").expect("parses");
    let golden = parse_golden("27_tool_change/golden.eid");
    assert!(matches!(
        &mine.ops[0],
        Op::ToolChange { tool: 3, state, .. } if state.tool == Some(3)
    ));
    assert_diff_empty(&mine, &golden, "#27");
}

#[test]
fn pilot_36_preset_g54_then_switch() {
    // Ключевой инвариант EQUIVALENCE §1.3: Δ константен по трём сегментам;
    // точки разрыва (b) Q11: смена WCS без in-text origin → start=unknown.
    let mine = parse_linuxcnc("36_preset_G54_then_switch/preset_G54_then_switch.linuxcnc.ngc").expect("parses");
    let golden = parse_golden("36_preset_G54_then_switch/golden.eid");
    let preset = mine.frame_preset.as_ref().expect("frame_preset present");
    assert_eq!(preset.wcs, eid_core::ast::Wcs::G54);
    assert_eq!(preset.values.x.value, 0.0);
    assert_eq!(preset.values.y.value, 0.0);
    assert_eq!(preset.values.z.value, 5.0);
    // OP1: якорь пресетки → known; OP3/OP5: смена WCS без define → unknown.
    let starts = mine.ops.iter().map(|o| match o {
        Op::Rapid { start, .. } | Op::Linear { start, .. } => *start == StartPoint::Unknown,
        _ => false,
    }).collect::<Vec<_>>();
    assert_eq!(
        starts,
        vec![false, false, true, false, true, false, false],
        "unknown-паттерн OP3/OP5 (Q11, точка разрыва (b))"
    );
    assert_diff_empty(&mine, &golden, "#36");
}

#[test]
fn pilot_46_arc_r_helix() {
    // R-форма + pitch≠0 валидна (SPEC §6 v0.7); pitch = 2.0.
    let mine = parse_linuxcnc("46_arc_R_helix/arc_R_helix.linuxcnc.ngc").expect("parses");
    let golden = parse_golden("46_arc_R_helix/golden.eid");
    match &mine.ops[1] {
        Op::Arc { center, sweep_deg, pitch, dir, .. } => {
            assert_eq!((center.x.value, center.y.value, center.z.value), (0.0, 5.0, 0.0));
            assert_eq!(sweep_deg.value, 90.0);
            assert_eq!(pitch.value, 2.0, "#46 pitch = 2.0");
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
        }
        other => panic!("op 1 must be Arc, got {other:?}"),
    }
    assert_diff_empty(&mine, &golden, "#46");
}

// ===========================================================================
// Негативы — Err-класс (точные коды)
// ===========================================================================

#[test]
fn n1_g53_no_origins_err_runtime_state() {
    assert_err_code("n1_g53_no_origins/g53_no_origins.linuxcnc.ngc", ErrorCode::RuntimeState, "N1");
}

#[test]
fn n2_mid_g92_err_semantics() {
    assert_err_code("n2_mid_G92/mid_G92.linuxcnc.ngc", ErrorCode::UnsupportedSemantics, "N2");
}

#[test]
fn n5a_arc_radius_mismatch_err_invalid_input() {
    assert_err_code("n5a_arc_radius_mismatch/arc_radius_mismatch.linuxcnc.ngc", ErrorCode::InvalidInput, "N5a");
}

#[test]
fn n5b_arc_chord_gt_2r_err_invalid_input() {
    assert_err_code("n5b_arc_chord_gt_2R/arc_chord_gt_2R.linuxcnc.ngc", ErrorCode::InvalidInput, "N5b");
}

#[test]
fn n7_unit_switch_mid_err_semantics() {
    assert_err_code("n7_unit_switch_mid/unit_switch_mid.linuxcnc.ngc", ErrorCode::UnsupportedSemantics, "N7");
}

#[test]
fn n12_program_end_not_last_err_invalid_input() {
    assert_err_code("n12_program_end_not_last/program_end_not_last.linuxcnc.ngc", ErrorCode::InvalidInput, "N12");
}

#[test]
fn n13_program_end_duplicate_err_invalid_input() {
    assert_err_code("n13_program_end_duplicate/program_end_duplicate.linuxcnc.ngc", ErrorCode::InvalidInput, "N13");
}

#[test]
fn n14_program_end_missing_err_invalid_input() {
    assert_err_code("n14_program_end_missing/program_end_missing.linuxcnc.ngc", ErrorCode::InvalidInput, "N14");
}

#[test]
fn n15_wcs_set_here_mid_err_semantics() {
    assert_err_code("n15_wcs_set_here_mid/wcs_set_here_mid.linuxcnc.ngc", ErrorCode::UnsupportedSemantics, "N15");
}

#[test]
fn n16_g53_after_preset_err_runtime_state() {
    assert_err_code("n16_g53_after_preset/g53_after_preset.linuxcnc.ngc", ErrorCode::RuntimeState, "N16");
}

#[test]
fn n17_multiple_preset_err_semantics() {
    assert_err_code("n17_multiple_preset_at_start/multiple_preset_at_start.linuxcnc.ngc", ErrorCode::UnsupportedSemantics, "N17");
}

#[test]
fn n19_arc_r_full_circle_err_invalid_input() {
    assert_err_code("n19_arc_R_full_circle/arc_R_full_circle.linuxcnc.ngc", ErrorCode::InvalidInput, "N19");
}

// ===========================================================================
// Негативы — Ok-класс (line-level RAW + диагностика)
// ===========================================================================

#[test]
fn n3_raw_vendor_block_ok_with_semantics() {
    let p = assert_ok_diag("n3_raw_vendor_block/raw_vendor_block.linuxcnc.ngc", ErrorCode::UnsupportedSemantics, "N3");
    assert!(matches!(&p.ops[0], Op::Raw { code: ErrorCode::UnsupportedSemantics, .. }));
    // Payload lossless: "M123" → hex.
    match &p.ops[0] {
        Op::Raw { payload_hex, .. } => assert_eq!(payload_hex, "4D313233"),
        other => panic!("op 0 must be Raw, got {other:?}"),
    }
    assert_eq!(p.ops.len(), 4); // Raw + Rapid + Linear + PEND
}

#[test]
fn n8_tapping_g84_ok_with_semantics() {
    // G84 — refuse-цикл: line-level RAW без цикл-парсера (COVERAGE §2.5).
    // Файл: header / G0 / G99 G84 … / M2 → ops: [Rapid, Raw, PEND].
    let p = assert_ok_diag("n8_tapping_G84/tapping_G84.linuxcnc.ngc", ErrorCode::UnsupportedSemantics, "N8");
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    assert!(matches!(&p.ops[1], Op::Raw { code: ErrorCode::UnsupportedSemantics, .. }));
    assert!(matches!(&p.ops[2], Op::ProgramEnd { .. }));
}

#[test]
fn n9_bare_t_ok_with_semantics() {
    // Одиночное T — предвыбор, line-level RAW (COVERAGE §3; N9, Ok-класс).
    let p = assert_ok_diag(
        "n9_bare_T_linuxcnc/bare_T_linuxcnc.linuxcnc.ngc",
        ErrorCode::UnsupportedSemantics,
        "N9",
    );
    match &p.ops[0] {
        Op::Raw { payload_hex, code, .. } => {
            assert_eq!(payload_hex, "5435"); // "T5"
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
        }
        other => panic!("op 0 must be Raw, got {other:?}"),
    }
    assert_eq!(p.ops.len(), 4); // Raw + Rapid + Linear + PEND
}

#[test]
fn n10_g43_h_table_ok_with_requires() {
    let p = assert_ok_diag("n10_g43_h_table/g43_h_table.linuxcnc.ngc", ErrorCode::RequiresMachineTable, "N10");
    assert!(matches!(&p.ops[0], Op::Raw { code: ErrorCode::RequiresMachineTable, .. }));
}

#[test]
fn n18_css_g96_ok_with_semantics() {
    let p = assert_ok_diag("n18_css_g96/css_g96.linuxcnc.ngc", ErrorCode::UnsupportedSemantics, "N18");
    assert!(matches!(&p.ops[0], Op::Raw { code: ErrorCode::UnsupportedSemantics, .. }));
}

#[test]
#[ignore = "СТОП-вопрос итерации 2: N4 требует цикл-парсера/лимита развёртывания (итерация 3, SPEC §7)"]
fn n4_peck_l_over_limit() {
    // G83 L99999 > max_unroll_iterations=10000 → E_LIMIT_EXCEEDED.
    // Развёртывание циклов — итерация 3; лимит max_unroll_iterations
    // применяется к сумме L/K при развёртывании.
    let res = parse_linuxcnc("n4_peck_L_over_limit/peck_L_over_limit.linuxcnc.ngc");
    assert_err_code("n4_peck_L_over_limit/peck_L_over_limit.linuxcnc.ngc", ErrorCode::LimitExceeded, "N4");
    let _ = res;
}

#[test]
#[ignore = "СТОП-вопрос итерации 2: файл N6 содержит G81 — ожидаемый код E_INVALID_INPUT достижим только с цикл-парсером (итерация 3)"]
fn n6_naked_coords_after_g80() {
    assert_err_code("n6_naked_coords_after_G80/naked_coords_after_G80.linuxcnc.ngc", ErrorCode::InvalidInput, "N6");
}

#[test]
fn n11_m98_call_err_syntax() {
    // По брифу итерации 2 / SPEC §1.1 (O-code, M98/M99 — структурные):
    // жёсткий Err E_UNSUPPORTED_SYNTAX. CORPUS §1.2 v0.5 перечисляет N11 в
    // Ok-классе — расхождение заведено в отчёт как СТОП-вопрос №1.
    assert_err_code("n11_m98_call/m98_call.linuxcnc.ngc", ErrorCode::UnsupportedSyntax, "N11");
}

// ===========================================================================
// Юнит-тесты: G90.1/G91.1 ABSORB, модальное состояние, Q11
// ===========================================================================

fn parse_str(src: &str) -> Result<eid_core::ast::Program, Vec<eid_core::diag::Diagnostic>> {
    LinuxCncAdapter::new().parse(src, "unit.ngc")
}

#[test]
fn unit_g90_1_g91_1_absorb_same_ast() {
    // Оба режима канонизируются в абсолютный центр (§6): G90.1 I0 J0
    // (абсолютный центр (0,0)) и G91.1 I-5 J0 (инкрементальный от (5,0))
    // дают ИДЕНТИЧНЫЙ AST — ABSORB (COVERAGE §2.2).
    let header = "G21 G90 G94 G17\nM3 S8000\nG0 X5.0000 Y0.0000 Z0.0000\n";
    let abs = format!("G90.1\n{header}G3 X0.0000 Y5.0000 I0.0000 J0.0000 F100.0\nM2\n");
    let inc = format!("G91.1\n{header}G3 X0.0000 Y5.0000 I-5.0000 J0.0000 F100.0\nM2\n");
    let a = parse_str(&abs).expect("G90.1 parses");
    let b = parse_str(&inc).expect("G91.1 parses");
    assert_diff_empty(&a, &b, "G90.1 ≡ G91.1 (ABSORB)");
    match &a.ops[1] {
        Op::Arc { center, sweep_deg, .. } => {
            assert_eq!((center.x.value, center.y.value), (0.0, 0.0));
            assert_eq!(sweep_deg.value, 90.0);
        }
        other => panic!("op 1 must be Arc, got {other:?}"),
    }
}

#[test]
fn unit_modal_motion_and_feed_survive() {
    // Модальный motion (X30 без G-кода продолжает G0) + feed переживает G0.
    let src = "G21 G90 G94 G17\nG1 X10.0000 F150.0\nG0 X20.0000\nX30.0000\nM2\n";
    let p = parse_str(src).expect("parses");
    assert!(matches!(&p.ops[0], Op::Linear { state, .. } if state.feed.as_ref().unwrap().value == 150.0));
    assert!(matches!(&p.ops[1], Op::Rapid { state, .. } if state.feed.as_ref().unwrap().value == 150.0));
    // Bare X30 продолжает модальную группу 1 (G0 → Rapid), feed 150 жив.
    assert!(matches!(&p.ops[2], Op::Rapid { state, .. } if state.feed.as_ref().unwrap().value == 150.0));
    assert!(matches!(&p.ops[3], Op::ProgramEnd { .. }));
}

#[test]
fn unit_q11_wcs_switch_with_in_text_origins_known() {
    // Точка разрыва (b), v0.10: start вычислим ⇔ обе системы имеют in-text
    // origin: reading_new = prev_end + O_prev − O_new.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G10 L2 P1 X0.0000 Y0.0000 Z0.0000\n",
        "G10 L2 P2 X0.0000 Y0.0000 Z5.0000\n",
        "G0 X1.0000 Y2.0000 Z3.0000\n",
        "G55\n",
        "G0 X1.0000 Y2.0000 Z8.0000\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    // ops: [WcsDefine P1, WcsDefine P2, Rapid (unknown), Rapid (known), PEND].
    // Второй ход: start = (1,2,3) + O_G54 − O_G55 = (1,2,3) + (0,0,−5) = (1,2,−2).
    assert!(matches!(&p.ops[2], Op::Rapid { start: StartPoint::Unknown, .. }));
    match &p.ops[3] {
        Op::Rapid { start: StartPoint::Known(s), .. } => {
            assert_eq!((s.x.value, s.y.value, s.z.value), (1.0, 2.0, -2.0));
        }
        other => panic!("op 3 start must be known (обе системы define), got {other:?}"),
    }
}

#[test]
fn unit_q11_first_op_without_anchor_unknown() {
    let src = "G21 G90\nG0 X1.0000 Y2.0000 Z3.0000\nM2\n";
    let p = parse_str(src).expect("parses");
    assert!(matches!(&p.ops[0], Op::Rapid { start: StartPoint::Unknown, .. }));
}

#[test]
fn unit_g93_inverse_time_feed_resolution() {
    // G93: feed = len × F (SPEC §4): len 30 × F 1 = 30 (движение 1/1 мин).
    let src = "G21 G90 G93 G17\nG0 X0.0000 Y0.0000 Z0.0000\nG1 X30.0000 F1.0\nM2\n";
    let p = parse_str(src).expect("parses");
    match &p.ops[1] {
        Op::Linear { state, .. } => {
            let f = state.feed.as_ref().unwrap().value;
            assert!((f - 30.0).abs() < 1e-9, "G93 feed = len·F, got {f}");
        }
        other => panic!("op 1 must be Linear, got {other:?}"),
    }
}

#[test]
fn unit_zero_length_and_zero_dwell_notice() {
    let src = "G21 G90 G94\nG1 X10.0000 F100.0\nG1 X10.0000\nG4 P0\nM2\n";
    let adapter = LinuxCncAdapter::new();
    let p = adapter.parse(src, "unit.ngc").expect("parses");
    let notices = adapter.take_notices();
    assert_eq!(p.ops.len(), 2); // Linear + PEND (нулевые отброшены)
    assert_eq!(notices.len(), 2);
    assert!(notices.iter().any(|n| n.kind == eid_core::parser::NoticeKind::ZeroLengthMotion));
    assert!(notices.iter().any(|n| n.kind == eid_core::parser::NoticeKind::ZeroDwell));
}

#[test]
fn unit_block_delete_raw_runtime_state() {
    let src = "G21 G90 G94\n/ G0 X1.0000 Y1.0000 Z1.0000\nG0 X2.0000 Y2.0000 Z2.0000\nM2\n";
    let adapter = LinuxCncAdapter::new();
    let p = adapter.parse(src, "unit.ngc").expect("Ok (line-level)");
    assert!(matches!(&p.ops[0], Op::Raw { code: ErrorCode::RuntimeState, .. }));
    let codes: Vec<_> = adapter.take_diagnostics().iter().map(|d| d.code).collect();
    assert!(codes.contains(&ErrorCode::RuntimeState));
}

#[test]
fn unit_hash_param_err_runtime_state() {
    let src = "G21 G90\nG0 X1.0000 Y1.0000 Z1.0000\n#100 = 5\nM2\n";
    let err = parse_str(src).expect_err("E_RUNTIME_STATE");
    assert!(err.iter().all(|d| d.code == ErrorCode::RuntimeState), "{err:?}");
}

#[test]
fn unit_o_code_err_syntax() {
    let src = "G21 G90\nO100\nM2\n";
    let err = parse_str(src).expect_err("E_UNSUPPORTED_SYNTAX");
    assert!(err.iter().all(|d| d.code == ErrorCode::UnsupportedSyntax), "{err:?}");
}

#[test]
fn unit_while_err_syntax() {
    let src = "G21 G90\nwhile [1] do 1\nM2\n";
    let err = parse_str(src).expect_err("E_UNSUPPORTED_SYNTAX");
    assert!(err.iter().all(|d| d.code == ErrorCode::UnsupportedSyntax), "{err:?}");
}

#[test]
fn unit_bare_t_then_m6_uses_selection() {
    // T5 без M6 → Raw (diag) + предвыбор сохраняется; M6 без T → ToolChange 5.
    let src = "G21 G90 G94\nT5\nM6\nG0 X1.0000 Y1.0000 Z1.0000\nM2\n";
    let adapter = LinuxCncAdapter::new();
    let p = adapter.parse(src, "unit.ngc").expect("Ok");
    assert!(matches!(&p.ops[0], Op::Raw { .. }));
    assert!(matches!(&p.ops[1], Op::ToolChange { tool: 5, .. }));
    let codes: Vec<_> = adapter.take_diagnostics().iter().map(|d| d.code).collect();
    assert!(codes.contains(&ErrorCode::UnsupportedSemantics));
}

#[test]
fn unit_m0_m1_stops() {
    let src = "G21 G90 G94\nG0 X0 Y0 Z0\nM0\nG0 X1.0000 Y0 Z0\nM1\nG0 X2.0000 Y0 Z0\nM2\n";
    let p = parse_str(src).expect("parses");
    assert!(matches!(&p.ops[1], Op::ProgramStop { optional: false, .. }));
    assert!(matches!(&p.ops[3], Op::ProgramStop { optional: true, .. }));
}

#[test]
fn unit_path_blend_and_mid_switch_err() {
    let ok = parse_str("G21 G90\nG64 P0.0500\nG0 X0 Y0 Z0\nM2\n").expect("G64 P- parses");
    assert!(matches!(&ok.path_mode, PathMode::Blend { t } if t.as_ref().unwrap().value == 0.05));
    let err = parse_str("G21 G90\nG61\nG0 X0 Y0 Z0\nG64\nM2\n").expect_err("mid-switch");
    assert!(err.iter().all(|d| d.code == ErrorCode::UnsupportedSemantics), "{err:?}");
}

#[test]
fn unit_wcs_g59_3_selection() {
    let src = "G21 G90\nG59.3\nG0 X1.0000 Y2.0000 Z3.0000\nM2\n";
    let p = parse_str(src).expect("parses");
    match &p.ops[0] {
        Op::Rapid { state, .. } => assert_eq!(state.wcs, eid_core::ast::Wcs::G59_3),
        other => panic!("op 0 must be Rapid, got {other:?}"),
    }
}

#[test]
fn unit_unknown_g_raw_syntax() {
    let src = "G21 G90 G94\nG11\nG0 X0 Y0 Z0\nM2\n";
    let adapter = LinuxCncAdapter::new();
    let p = adapter.parse(src, "unit.ngc").expect("Ok (line-level)");
    assert!(matches!(&p.ops[0], Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }));
}

#[test]
fn unit_naked_coords_after_g80_err_invalid_input() {
    // Правило N6 на уровне, достижимом без циклов (COVERAGE §2.1): после G80
    // модальная группа очищена → голые координаты = E_INVALID_INPUT.
    let src = "G21 G90 G94\nG0 X0 Y0 Z0\nG80\nX20.0000 Y20.0000\nM2\n";
    let err = parse_str(src).expect_err("E_INVALID_INPUT");
    assert!(err.iter().all(|d| d.code == ErrorCode::InvalidInput), "{err:?}");
}

#[test]
fn unit_dwell_negative_err() {
    let src = "G21 G90 G94\nG0 X0 Y0 Z0\nG4 P-1.0\nM2\n";
    let err = parse_str(src).expect_err("E_INVALID_INPUT");
    assert!(err.iter().all(|d| d.code == ErrorCode::InvalidInput), "{err:?}");
}

#[test]
fn unit_modal_group_conflict_err() {
    let src = "G21 G90 G94\nG0 G1 X5.0000\nM2\n";
    let err = parse_str(src).expect_err("E_INVALID_INPUT");
    assert!(err.iter().all(|d| d.code == ErrorCode::InvalidInput), "{err:?}");
}

#[test]
fn unit_word_len_limit_exceeded() {
    let long = "1".repeat(33);
    let src = format!("G21 G90\nG0 X{long} Y0 Z0\nM2\n");
    let err = parse_str(&src).expect_err("E_LIMIT_EXCEEDED");
    assert!(err.iter().all(|d| d.code == ErrorCode::LimitExceeded), "{err:?}");
}

#[test]
fn unit_arc_without_center_err() {
    let src = "G21 G90 G94 G17\nG0 X0 Y0 Z0\nG2 X5.0000 Y5.0000 F100.0\nM2\n";
    let err = parse_str(src).expect_err("E_INVALID_INPUT");
    assert!(err.iter().all(|d| d.code == ErrorCode::InvalidInput), "{err:?}");
}

#[test]
fn unit_arc_r_negative_major_sweep() {
    // Отрицательный R → дуга >180° (знак R выбирает ≤/> 180°, SPEC §6).
    let src = "G21 G90 G94 G17\nG0 X0 Y10.0000 Z0\nG2 X0.0175 Y-9.9998 R-10.0 F100.0\nM2\n";
    let p = parse_str(src).expect("parses");
    match &p.ops[1] {
        Op::Arc { center, sweep_deg, .. } => {
            // Зеркальная сторона перпендикуляра; sweep = 360 − 179.4975 ≈ 180.5025.
            assert!(sweep_deg.value > 180.0, "negative R → major arc, got {}", sweep_deg.value);
            assert!((center.x.value - 0.0526).abs() < 1e-3, "center.x = {}", center.x.value);
        }
        other => panic!("op 1 must be Arc, got {other:?}"),
    }
}

// ===========================================================================
// SPEC v0.12 (красный-протокол): sweep=360 center-form, квантование входа,
// независимость Distance/ArcDistance (CORPUS #08/#13/#39), #33
// ===========================================================================

#[test]
fn unit_full_circle_center_form_no_axis_words() {
    // SPEC §6 v0.12 (CORPUS #08): явный G2 с I/J/K БЕЗ слов осей — end
    // наследует старт → полный круг, sweep = 360° (не zero-sweep drop).
    let src = "G21 G90 G94 G17 G91.1\nG0 X10.0000 Y0.0000 Z0.0000\nG2 I-5.0000 J0.0000 F120.0\nM2\n";
    let p = parse_str(src).expect("parses");
    assert_eq!(p.ops.len(), 3, "RAPID + ARC(360°) + PEND");
    match &p.ops[1] {
        Op::Arc { start: StartPoint::Known(s), end, center, sweep_deg, dir, .. } => {
            assert_eq!((s.x.value, s.y.value, s.z.value), (10.0, 0.0, 0.0));
            assert_eq!((end.x.value, end.y.value, end.z.value), (10.0, 0.0, 0.0));
            assert_eq!((center.x.value, center.y.value, center.z.value), (5.0, 0.0, 0.0));
            assert_eq!(sweep_deg.value, 360.0);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
        }
        other => panic!("op 1 must be Arc, got {other:?}"),
    }
}

#[test]
fn unit_zero_sweep_quantized_end_dropped() {
    // SPEC §10.6 v0.12 (CORPUS #13): слово X0.00004 квантуется к 0.0000 при
    // построении значения → end == start при явных словах осей и pitch == 0
    // → zero-sweep дуга → drop+notice (не полный круг).
    let src = concat!(
        "G21 G90 G94 G17 G91.1\n",
        "G0 X0.0000 Y0.0000 Z0.0000\n",
        "G2 X0.00004 Y0.0000 I5.0000 J0.0000 F100.0\n",
        "G1 X10.0000\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let p = a.parse(src, "unit13.ngc").expect("parses");
    assert_eq!(p.ops.len(), 3, "RAPID + LINEAR + PEND (дуга отброшена)");
    assert!(matches!(&p.ops[1], Op::Linear { .. }));
    let notices = a.take_notices();
    assert!(
        notices.iter().any(|n| n.kind == NoticeKind::ZeroLengthArc),
        "zero-length arc notice отсутствует: {notices:?}"
    );
}

#[test]
fn unit_zero_sweep_helical_explicit_end_kept() {
    // Явный end, квантованный к старту В ПЛОСКОСТИ, но pitch ≠ 0 — полный
    // винтовой оборот: НЕ zero-sweep, sweep = 360° (SPEC §6 v0.12).
    let src = concat!(
        "G21 G90 G94 G17 G91.1\n",
        "G0 X10.0000 Y0.0000 Z0.0000\n",
        "G2 X10.0000 Y0.0000 Z5.0000 I-5.0000 J0.0000 F100.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    match &p.ops[1] {
        Op::Arc { sweep_deg, pitch, .. } => {
            assert_eq!(sweep_deg.value, 360.0);
            assert_eq!(pitch.value, 5.0);
        }
        other => panic!("op 1 must be Arc, got {other:?}"),
    }
}

#[test]
fn unit_modal_distance_and_arcdist_independent() {
    // CORPUS #39 — проверка гипотезы о конфликте: G91 (Distance) и G91.1
    // (Arc-Distance) — РАЗНЫЕ модальные группы (COVERAGE §2.2), заголовок
    // парсится без конфликта; валидная G91.1-дуга канонизуется абсолютной.
    // (Файл #39 как записан даёт E_INVALID_INPUT по радиусам — I15 при
    // инкрементальных центрах от старта (10,0) даёт центр (25,0), ρ 15 ≠
    // ρ_end 11.18; корректные файлы: GRBL-пара I5.0 либо заголовок G90.1 —
    // File-wrong, отчёт итерации 3b.)
    let src = concat!(
        "G21 G91 G94 G17 G91.1\n",
        "M3 S12000\n",
        "G0 X5.0000\n",
        "G1 X5.0000 F100.0\n",
        "G2 X5.0000 Y5.0000 I5.0000 J0.0000\n",
        "M2\n",
    );
    let p = parse_str(src).expect("G91 + G91.1 сосуществуют без конфликта");
    assert_eq!(p.ops.len(), 4);
    match &p.ops[2] {
        Op::Arc { start: StartPoint::Known(s), end, center, sweep_deg, dir, state, .. } => {
            // G91: end = base + (5,5) = (15,5); G91.1: center = start + (5,0).
            assert_eq!((s.x.value, s.y.value, s.z.value), (10.0, 0.0, 0.0));
            assert_eq!((end.x.value, end.y.value, end.z.value), (15.0, 5.0, 0.0));
            assert_eq!((center.x.value, center.y.value, center.z.value), (15.0, 0.0, 0.0));
            assert_eq!(sweep_deg.value, 90.0);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
            assert_eq!(state.feed.as_ref().unwrap().value, 100.0);
        }
        other => panic!("op 2 must be Arc, got {other:?}"),
    }
}

#[test]
fn vector_33_wcs_define_multi_linking_and_feed() {
    // Ключевая семантика #33: кросс-WCS связывание при обеих in-text origins
    // (точка разрыва (b), критерий вычислимости §2 v0.10) + feed переживает
    // G0 и смену WCS (SPEC §4).
    let mine = parse_linuxcnc("33_wcs_define_multi/wcs_define_multi.linuxcnc.ngc").expect("parses");
    assert!(matches!(&mine.ops[0], Op::WcsDefine { p: 1, .. }));
    assert!(matches!(&mine.ops[1], Op::WcsDefine { p: 2, .. }));
    match &mine.ops[4] {
        Op::Rapid { start: StartPoint::Known(s), end, state, .. } => {
            // start = prev_end + O_G54 − O_G55 = (10,0,5) + (1,2,3) − (4,5,6).
            assert_eq!((s.x.value, s.y.value, s.z.value), (7.0, -3.0, 2.0));
            assert_eq!((end.x.value, end.y.value, end.z.value), (12.0, -3.0, 2.0));
            assert_eq!(state.wcs, eid_core::ast::Wcs::G55);
            assert_eq!(state.feed.as_ref().unwrap().value, 150.0, "SPEC §4: feed живёт через G0/WCS");
        }
        other => panic!("op 4 must be Rapid, got {other:?}"),
    }
    // Golden OP005 feed=none — File-wrong (отчёт итерации 3b; противоречит
    // golden #03 feed_survives_rapid). Допускаем ровно это расхождение;
    // после ревизии golden diff станет пустым.
    let golden = parse_golden("33_wcs_define_multi/golden.eid");
    let d = semantic_diff(&mine, &golden, &DiffConfig::default());
    assert!(
        d.iter().all(|x| matches!(x, Divergence::StateField { index: 4, field: "feed" })),
        "#33: расхождения вне feed на op 4 — разбор неверен: {d:?}"
    );
}
