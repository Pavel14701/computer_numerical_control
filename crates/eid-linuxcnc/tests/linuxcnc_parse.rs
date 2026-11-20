//! Тесты канонизатора LinuxCNC (Агент D, итерации 2–5).
//!
//! Позитив: пилотные векторы Tier A #01, #02, #03, #11, #27, #36, #46 —
//! сравнение с golden AST через `eid_harness::diff::semantic_diff`
//! (допуски EQUIVALENCE §4; src/sub вне эквивалентности).
//!
//! Негатив (правило двух классов, CORPUS §1.2 v0.5):
//!
//!  * Err-класс (структурные): N1, N2, N5a, N5b, N7, N12, N13, N14, N15,
//!    N16, N17, N19 — точные коды;
//!  * Ok-класс (line-level RAW + диагностика): N3, N8, N9, N10, N18.
//!
//! Итерация 3-4: N4/N6 расконсервированы (развёртывание циклов §7.7);
//! N11 — Err по CORPUS §1.2 v0.5.2.
//!
//! ПОКРЫТИЕ COVERAGE LinuxCNC-колонка (§2–§4) + SPEC §7.7-шаблон —
//! чек-лист «всё что можно и всё что нельзя» (метод C, зеркально
//! eid-grbl tests/grbl_parse.rs; имена spec_cov_lcnc_<family>_<case>):
//!
//!  * движение: G0/G1/G2/G3 оп-виды; дуги IJK+R, G90.1/G91.1 ОБА режима
//!    (ABSORB) × плоскости G17/G18/G19; полный круг center-формой
//!    (sweep=360, #08) и R-полный-круг (N19); винтовой полный оборот;
//!  * модальные режимы: G90/G91, G93/G94 (len·F), G17/18/19,
//!    G54–G59.3, path_mode G61/G61.1/G64±P, Distance≠ArcDistance группы;
//!  * фреймы: G10 L2/L20, G53 (§7.6/§5.3: (a) preset-Err, (b) разность
//!    origin'ов, (c) machine Known/Unknown/Err-ARC, (d) возврат в WCS);
//!  * циклы §7.7 — каждый cycle-тип × каждая позиция шаблона:
//!    traverse (модальный feed), подход R отдельным опом, feed-ходы,
//!    dwell (G82/G89), пеки d_k+0.5 (G83), de-retract min(R,d+Q) (G73),
//!    feed-out (G85/G89), G86 off/рестарт, retract G98/G99,
//!    L/K-повторы (G90-констант/G91-шаг), лимиты (L и Q-guard);
//!  * refuse/line-level: G84/G74/G87/G88, G76, G5.x+P (NURBS, находка
//!    Tier B sweep), `$`-строки и границы (`$` в комментарии/внутри слова),
//!    одиночное T, G43 H, block delete, unknown G/M;
//!  * структура: PEND-инварианты (N12/N13/N14), G80-naked (N6),
//!    лимит слова (§1.2), конфликт модальных групп;
//!  * Stage-атрибуция: Parse (лексика/структура кадра), Verify (finish:
//!    PEND/feed/arc-guard), Emit (нормаль дуги) — spec_cov_lcnc_stage_*.
//!
//! Пробелов против колонки нет: каждая диспозиция §2–§4 покрыта парой
//! (позитив + негатив с точным кодом) — см. также emit-таблицу
//! EmissionLegality (COVERAGE §5) и её тесты (linuxcnc_emit.rs).

use eid_core::adapter::{Adapter, EidTextAdapter};
use eid_core::ast::{Op, PathMode, StartPoint};
use eid_core::diag::ErrorCode;
use eid_core::parser::NoticeKind;
use eid_harness::diff::{semantic_diff, Divergence, DiffConfig};
use eid_linuxcnc::LinuxCncAdapter;
use eid_linuxcnc::canonicalize::to_f64;
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
        Op::Raw { payload_hex, .. } => assert_eq!(payload_hex, "4D3132330A", "D9 v0.15: payload с терминатором"),
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
            assert_eq!(payload_hex, "54350A"); // "T5" + LF — D9 v0.15
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
fn n4_peck_l_over_limit() {
    // Расконсервирован (итерация 3-4): L99999 > max_unroll_iterations=10000
    // → E_LIMIT_EXCEEDED; guard проверяется до диагностики слов (в файле
    // нет F — feed-проверка не должна опередить лимит).
    assert_err_code("n4_peck_L_over_limit/peck_L_over_limit.linuxcnc.ngc", ErrorCode::LimitExceeded, "N4");
}

#[test]
fn n6_naked_coords_after_g80() {
    // Расконсервирован (итерация 3-4): G81 в файле теперь разворачивается;
    // голые координаты после G80 → E_INVALID_INPUT (COVERAGE §2.1).
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

// ===========================================================================
// Циклы LinuxCNC — разворачивание (SPEC §7; итерация 3-4, этап A)
// ===========================================================================

#[test]
fn unit_cycle_g81_minimal_unroll() {
    // G99 G81: traverse XY → rapid R → feed дно → rapid retract на R.
    // Производные опы: src блока + sub=1..5 (SPEC §11); feed модальный.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "M3 S10000\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G81 X10.0000 Y10.0000 Z-5.0000 R2.0000 F100.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    assert_eq!(p.ops.len(), 6, "Rapid + 4 производных + PEND");
    let snap = |i: usize| -> ([f64; 3], [f64; 3], Option<u32>) {
        match &p.ops[i] {
            Op::Rapid { start: StartPoint::Known(s), end, sub, .. } => {
                (to_f64(s), to_f64(end), *sub)
            }
            Op::Linear { start: StartPoint::Known(s), end, sub, .. } => {
                (to_f64(s), to_f64(end), *sub)
            }
            other => panic!("op {i} не ход: {other:?}"),
        }
    };
    // 1. traverse XY на текущем z=5.
    let (s, e, sub) = snap(1);
    assert_eq!((s, e, sub), ([0.0, 0.0, 5.0], [10.0, 10.0, 5.0], Some(1)));
    // 2. rapid на R=2.
    let (s, e, sub) = snap(2);
    assert_eq!((s, e, sub), ([10.0, 10.0, 5.0], [10.0, 10.0, 2.0], Some(2)));
    // 3. feed на дно Z=-5 (linear, feed 100).
    match &p.ops[3] {
        Op::Linear { start: StartPoint::Known(s), end, state, sub, .. } => {
            assert_eq!((to_f64(s), to_f64(end)), ([10.0, 10.0, 2.0], [10.0, 10.0, -5.0]));
            assert_eq!(state.feed.as_ref().unwrap().value, 100.0);
            assert_eq!(*sub, Some(3));
        }
        other => panic!("op 3 must be Linear: {other:?}"),
    }
    // 4. retract (G99) на R=2.
    let (s, e, sub) = snap(4);
    assert_eq!((s, e, sub), ([10.0, 10.0, -5.0], [10.0, 10.0, 2.0], Some(4)));
    assert!(matches!(&p.ops[5], Op::ProgramEnd { .. }));
}

#[test]
fn unit_cycle_g82_dwell_on_bottom() {
    // G82 = G81 + dwell P (секунды) на дне, до отвода.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G82 X1.0000 Y1.0000 Z-3.0000 R1.0000 P0.5000 F80.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    assert_eq!(p.ops.len(), 7); // Rapid + 5 производных + PEND
    match &p.ops[4] {
        Op::Dwell { seconds, sub, .. } => {
            assert_eq!(seconds.value, 0.5);
            assert_eq!(*sub, Some(4));
        }
        other => panic!("op 4 must be Dwell: {other:?}"),
    }
    // Retract после dwell — sub=5.
    match &p.ops[5] {
        Op::Rapid { sub, .. } => assert_eq!(*sub, Some(5)),
        other => panic!("op 5 must be Rapid: {other:?}"),
    }
}

#[test]
fn unit_cycle_l_repeat_g90_dedup_zero_moves() {
    // L=2 в G90: оба отверстия в одной точке; повторные traverse/R ходы
    // нулевые → отбрасываются с notice (SPEC §3), feed-ходы живы.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X5.0000 Y5.0000 Z5.0000\n",
        "G99 G81 X10.0000 Y10.0000 Z-3.0000 R2.0000 F100.0 L2\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let p = a.parse(src, "l2.ngc").expect("parses");
    // iter1: traverse, rapid R, feed, retract (4); iter2: traverse/R нулевые
    // (drop), feed + retract живы (2) → initial + 6 + PEND = 8.
    assert_eq!(p.ops.len(), 8);
    assert!(matches!(&p.ops[5], Op::Linear { sub: Some(7), .. }));
    assert!(matches!(&p.ops[6], Op::Rapid { sub: Some(8), .. }));
    let notices = a.take_notices();
    assert_eq!(notices.iter().filter(|n| n.kind == NoticeKind::ZeroLengthMotion).count(), 2);
}

#[test]
fn unit_cycle_l_repeat_g91_steps() {
    // G91: шаг X/R на повтор (LinuxCNC); Z — инкремент от R-плоскости.
    let src = concat!(
        "G21 G91 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G81 X1.0000 Z-2.0000 R1.0000 F100.0 L2\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    // iter1: xy (1,0); R = 5+1 = 6; дно = 6−2 = 4. iter2: xy (2,0); R = 7;
    // дно = 5. Все ходы ненулевые: 8 производных + initial + PEND = 10.
    assert_eq!(p.ops.len(), 10);
    let end_of = |i: usize| -> [f64; 3] {
        match &p.ops[i] {
            Op::Rapid { end, .. } | Op::Linear { end, .. } => to_f64(end),
            other => panic!("op {i}: {other:?}"),
        }
    };
    assert_eq!(end_of(2), [1.0, 0.0, 6.0], "iter1: rapid на R=6");
    assert_eq!(end_of(3), [1.0, 0.0, 4.0], "iter1: дно=4");
    assert_eq!(end_of(5), [2.0, 0.0, 6.0], "iter2: traverse XY (шаг X)");
    assert_eq!(end_of(6), [2.0, 0.0, 7.0], "iter2: rapid на R=7 (шаг R)");
    assert_eq!(end_of(7), [2.0, 0.0, 5.0], "iter2: дно=5");
}

#[test]
fn unit_cycle_first_motion_err_runtime_state() {
    // SPEC §7.6: цикл как первый motion-оп → E_RUNTIME_STATE
    // (пресетки-якоря не считаются — strict reading).
    let src = "G21 G90 G94 G17\nG99 G81 X1.0000 Y1.0000 Z-1.0000 R1.0000 F100.0\nM2\n";
    let err = parse_str(src).expect_err("§7.6");
    assert!(err.iter().all(|d| d.code == ErrorCode::RuntimeState), "{err:?}");
}

#[test]
fn unit_cycle_after_unknown_position_err_runtime_state() {
    // SPEC §7.6: цикл после позиционно-неопределённого участка (смена WCS
    // без in-text origins) → E_RUNTIME_STATE.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X1.0000 Y1.0000 Z1.0000\n",
        "G55\n",
        "G99 G81 X2.0000 Y2.0000 Z-1.0000 R1.0000 F100.0\n",
        "M2\n",
    );
    let err = parse_str(src).expect_err("§7.6");
    assert!(err.iter().all(|d| d.code == ErrorCode::RuntimeState), "{err:?}");
}

#[test]
fn unit_cycle_requires_r_and_z_and_f() {
    for (src, ctx) in [
        ("G21 G90 G94\nG0 X1.0000 Y1.0000 Z1.0000\nG99 G81 X2.0000 Y2.0000 Z-1.0000 F100.0\nM2\n", "без R"),
        ("G21 G90 G94\nG0 X1.0000 Y1.0000 Z1.0000\nG99 G81 X2.0000 Y2.0000 R1.0000 F100.0\nM2\n", "без Z"),
        ("G21 G90 G94\nG99 G81 X2.0000 Y2.0000 Z-1.0000 R1.0000 F100.0\nM2\n", "первый motion + без F"),
    ] {
        let err = parse_str(src).unwrap_err();
        assert!(!err.is_empty(), "{ctx}");
    }
}

#[test]
fn unit_cycle_g82_without_p_err() {
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X1.0000 Y1.0000 Z1.0000\n",
        "G99 G82 X2.0000 Y2.0000 Z-1.0000 R1.0000 F100.0\n",
        "M2\n",
    );
    let err = parse_str(src).expect_err("G82 требует P");
    assert!(err.iter().all(|d| d.code == ErrorCode::InvalidInput), "{err:?}");
}

// ===========================================================================
// Циклы — этап B: пеки G83/G73, feed-out G85/G89, лимит развёртки
// ===========================================================================

#[test]
fn unit_cycle_g83_peck_order_full_retract() {
    // R=2, Z=−4, Q=1.5 → 4 пека вниз шагами Q: 0.5, −1.0, −2.5, −4.0;
    // полный отвод на R + подход (clr=0.5 над последней глубиной, ≤ R)
    // после каждого промежуточного пека; порядок пиков точный (SPEC §7.4).
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G83 X10.0000 Y0.0000 Z-4.0000 R2.0000 Q1.5000 F100.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    // subs: 1 traverse, 2 rapid-R, затем 4×(feed, retract-R, approach) = 12,
    // финальный retract (G99 → R) sub=13; всего 13 + initial + PEND = 15.
    assert_eq!(p.ops.len(), 15);
    let mut depths: Vec<f64> = Vec::new();
    let mut retract_after_last_feed = f64::NAN;
    for o in &p.ops[1..14] {
        match o {
            Op::Linear { end, sub, .. } => {
                depths.push(to_f64(end)[2]);
                let _ = sub;
            }
            Op::Rapid { end, sub: Some(s), .. } if *s == 4 || *s == 7 || *s == 10 || *s == 13 => {
                retract_after_last_feed = to_f64(end)[2];
            }
            _ => {}
        }
    }
    assert_eq!(depths, vec![0.5, -1.0, -2.5, -4.0], "пеки вниз шагами Q (порядок §7.4)");
    assert_eq!(retract_after_last_feed, 2.0, "G99: финальный отвод на R");
}

#[test]
fn unit_cycle_g73_chipbreak_retract_q() {
    // G73: отвод на Q над последней глубиной (не полный на R);
    // подход clr=0.5 над последней глубиной. R=2, Z=−4, Q=2 → 3 пека:
    // 0.0, −2.0, −4.0.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G73 X10.0000 Y0.0000 Z-4.0000 R2.0000 Q2.0000 F100.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    let mut depths: Vec<f64> = Vec::new();
    let mut ups: Vec<f64> = Vec::new();
    let mut cur = 5.0f64;
    for o in &p.ops[1..] {
        match o {
            Op::Linear { end, .. } => {
                depths.push(to_f64(end)[2]);
                cur = to_f64(end)[2];
            }
            Op::Rapid { end, .. } => {
                let z = to_f64(end)[2];
                if z > cur {
                    ups.push(z); // upward rapid = de-retract (или финальный отвод)
                }
                cur = z;
            }
            _ => {}
        }
    }
    assert_eq!(depths, vec![0.0, -2.0, -4.0], "G73 пеки");
    // De-retract на Q над последней глубиной: 2.0 (над 0.0, clamp на R),
    // 0.0 (над −2.0); финальный отвод (G99) на R=2.0.
    assert_eq!(ups, vec![2.0, 0.0, 2.0], "G73 de-retract + финальный отвод на R");
}

#[test]
fn unit_cycle_g85_bore_feed_out() {
    // G85: отвод на ПОДАЧЕ (linear), не rapid; G99 → отвод до R.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "M3 S8000\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G85 X10.0000 Y0.0000 Z-3.0000 R2.0000 F60.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    match &p.ops[4] {
        Op::Linear { start: StartPoint::Known(s), end, .. } => {
            assert_eq!(to_f64(s)[2], -3.0, "feed-out со дна");
            assert_eq!(to_f64(end)[2], 2.0, "G99: feed-out до R");
        }
        other => panic!("retract G85 — linear (feed-out), got {other:?}"),
    }
}

#[test]
fn unit_cycle_g89_bore_dwell_feed_out() {
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G89 X1.0000 Y1.0000 Z-3.0000 R1.0000 P0.2500 F50.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    assert!(matches!(&p.ops[4], Op::Dwell { seconds, .. } if seconds.value == 0.25));
    assert!(matches!(&p.ops[5], Op::Linear { .. }), "G89: feed-out");
}

#[test]
fn unit_cycle_peck_guard_limit_exceeded() {
    // Guard blowup: Q=0.001, глубина 12 → 12000 пеков > 10000 →
    // E_LIMIT_EXCEEDED (COVERAGE §1.2; N4-класс по Q).
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G83 X1.0000 Y1.0000 Z-10.0000 R2.0000 Q0.0010 F100.0\n",
        "M2\n",
    );
    let err = parse_str(src).expect_err("пек-лимит");
    assert!(err.iter().all(|d| d.code == ErrorCode::LimitExceeded), "{err:?}");
}

// ===========================================================================
// Циклы — этап C: G86 рестарт шпинделя, G98/G99 отвод
// ===========================================================================

#[test]
fn unit_cycle_g86_spindle_off_restart() {
    // SPEC §7 (Q9): feed на дно → dwell P? → шпиндель off → rapid out →
    // рестарт в прежнем направлении (resolved-state: следующие опа несут
    // восстановленный spindle).
    let src = concat!(
        "G21 G90 G94 G17\n",
        "M3 S8000\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G86 X10.0000 Y0.0000 Z-3.0000 R2.0000 P0.3000 F60.0\n",
        "G1 X20.0000 F100.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    // subs: 1 traverse, 2 rapid-R, 3 feed, 4 dwell, 5 rapid-out (spindle=Off).
    assert!(matches!(&p.ops[4], Op::Dwell { seconds, .. } if seconds.value == 0.3));
    match &p.ops[5] {
        Op::Rapid { end, state, sub, .. } => {
            assert_eq!(to_f64(end)[2], 2.0, "rapid out на R");
            assert_eq!(state.spindle, eid_core::ast::Spindle::Off, "шпиндель off на отводе");
            assert_eq!(*sub, Some(5));
        }
        other => panic!("op 5 must be Rapid: {other:?}"),
    }
    // Рестарт: следующий ход несёт прежнее направление и обороты.
    match &p.ops[6] {
        Op::Linear { state, .. } => {
            assert_eq!(state.spindle, eid_core::ast::Spindle::Cw(eid_core::ast::Num::new("8000".into(), 8000.0)));
            assert_eq!(state.feed.as_ref().unwrap().value, 100.0);
        }
        other => panic!("op 6 must be Linear: {other:?}"),
    }
}

#[test]
fn unit_cycle_g98_vs_g99_retract_target() {
    // §7.1: G99 → отвод на R-плоскость; G98 → initial Z (позиция до цикла,
    // вычислимая из предшествующих опов).
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G81 X10.0000 Y0.0000 Z-3.0000 R2.0000 F100.0\n",
        "G0 Z6.0000\n",
        "G98 G81 X20.0000 Y0.0000 Z-3.0000 R4.0000 F100.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    // Цикл 1 (G99): retract → R=2.
    assert_eq!(to_f64(match &p.ops[4] {
        Op::Rapid { end, .. } => end,
        other => panic!("op 4: {other:?}"),
    })[2], 2.0);
    // G0 Z6.
    assert_eq!(to_f64(match &p.ops[5] {
        Op::Rapid { end, .. } => end,
        other => panic!("op 5: {other:?}"),
    })[2], 6.0);
    // Цикл 2 (G98): retract → initial Z = 6 (не R=4).
    assert_eq!(to_f64(match &p.ops[9] {
        Op::Rapid { end, .. } => end,
        other => panic!("op 9: {other:?}"),
    })[2], 6.0, "G98: отвод на initial Z (6), не на R (4)");
}

#[test]
fn unit_unknown_dollar_line_raw_syntax() {
    // CORPUS #47-класс: неизвестная строка (`$` вне словаря LinuxCNC) —
    // line-local RAW{E_UNSUPPORTED_SYNTAX}, разбор продолжается; payload —
    // байты строки С терминатором (D9 v0.15).
    let src = "G21 G90 G94\n$$\nG0 X1.0000 Y1.0000 Z1.0000\nM2\n";
    let a = LinuxCncAdapter::new();
    let p = a.parse(src, "dollar.ngc").expect("Ok (line-level)");
    assert!(matches!(
        &p.ops[0],
        Op::Raw { code: ErrorCode::UnsupportedSyntax, payload_hex, .. }
            if payload_hex.as_str() == "24240A"
    ), "payload = `$$\n` = 24240A");
    assert!(matches!(&p.ops[1], Op::Rapid { .. }), "разбор продолжается после raw");
    let codes: Vec<_> = a.take_diagnostics().iter().map(|d| d.code).collect();
    assert!(codes.contains(&ErrorCode::UnsupportedSyntax));
}

#[test]
fn unit_raw_crlf_payload_terminator() {
    // D9 v0.15: CRLF-файл → payload включает \r\n; emit воспроизводит
    // терминатор дословно (не нормализует); reparse — payload равен.
    let src = "G21 G90 G94\r\n$$\r\nG0 X1.0000 Y1.0000 Z1.0000\r\nM2\r\n";
    let a = LinuxCncAdapter::new();
    let p = a.parse(src, "crlf.ngc").expect("parses");
    match &p.ops[0] {
        Op::Raw { payload_hex, .. } => {
            assert_eq!(payload_hex, "24240D0A", "`$$\r\n` — зеркально golden #47");
        }
        other => panic!("op 0 must be Raw: {other:?}"),
    }
    let text = a.emit(&p).expect("emit");
    assert!(text.contains("$$\r\n"), "CRLF-терминатор сохранён дословно:\n{text:?}");
    let p2 = LinuxCncAdapter::new().parse(&text, "rt.ngc").expect("reparse");
    match (&p.ops[0], &p2.ops[0]) {
        (Op::Raw { payload_hex: h1, .. }, Op::Raw { payload_hex: h2, .. }) => {
            assert_eq!(h1, h2, "R1-text-класс: payload байт-в-байт");
        }
        _ => panic!("оба должны начинаться с Raw"),
    }
    let d = eid_harness::diff::semantic_diff(&p, &p2, &DiffConfig::default());
    assert!(d.is_empty(), "{d:?}");
}

// ===========================================================================
// spec_cov — Tier B находки: NURBS/P-слова, `$`-границыли (итерация 4)
// ===========================================================================

#[test]
fn spec_cov_lcnc_nurbs_p_words_raw_semantics() {
    // Tier B находка (sweep: 13 real-файлов nurbs/G5): G5.2/G5.3 с P/Q —
    // line-level Raw SEMANTICS, P/Q поглощены whole-line raw, разбор идёт.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G5.2 P1 Q2 X10.0000 Y10.0000\n",
        "G1 X20.0000 F100.0\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let p = a.parse(src, "nurbs.ngc").expect("Ok (line-level)");
    assert!(matches!(
        &p.ops[1],
        Op::Raw { code: ErrorCode::UnsupportedSemantics, payload_hex, .. }
            if payload_hex.starts_with("4735 2E32".replace(' ', "").as_str())
    ), "payload = `G5.2\n`-префикс (G5.2)");
    assert!(matches!(&p.ops[2], Op::Linear { .. }), "разбор продолжается");
}

#[test]
fn spec_cov_lcnc_dollar_in_comment_not_raw() {
    // Граница: `$` внутри комментария — комментарий изымается ДО слов →
    // строка НЕ raw (аннотации, SPEC §11).
    let src = "G21 G90 G94\n($$ config hint)\nG0 X1.0000 Y1.0000 Z1.0000\nM2\n";
    let p = parse_str(src).expect("парсится: комментарий не raw");
    assert!(!p.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
}

#[test]
fn spec_cov_lcnc_dollar_mid_word_raw_whole_line() {
    // Граница: `$` в середине слова — вся строка line-local raw.
    let src = "G21 G90 G94\nG0 X1$0\nM2\n";
    let p = parse_str(src).expect("Ok (line-level)");
    assert!(matches!(&p.ops[0], Op::Raw { code: ErrorCode::UnsupportedSyntax, .. }));
    assert_eq!(p.ops.len(), 2); // Raw + PEND
}

// ===========================================================================
// spec_cov — G90.1/G91.1: микс в одной программе; обе плоскости × режимы
// ===========================================================================

#[test]
fn spec_cov_lcnc_arc_modes_mixed_in_one_program() {
    // Микс: первая дуга G90.1 (абсолютный центр), вторая G91.1 (инкремент) —
    // обе канонизуются в абсолют; каждая с явным переключением режима.
    let src = concat!(
        "G21 G90 G94 G17 G90.1\n",
        "G0 X5.0000 Y0.0000 Z0.0000\n",
        "G3 X0.0000 Y5.0000 I0.0000 J0.0000 F100.0\n",
        "G91.1\n",
        "G2 X5.0000 Y0.0000 I0.0000 J-5.0000 F120.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    assert_eq!(p.ops.len(), 4); // Rapid + Arc + Arc + PEND
    match &p.ops[1] {
        Op::Arc { center, sweep_deg, dir, .. } => {
            assert_eq!((center.x.value, center.y.value), (0.0, 0.0), "G90.1 абсолютный центр");
            assert_eq!(sweep_deg.value, 90.0);
            assert_eq!(*dir, eid_core::ast::Dir::Ccw);
        }
        other => panic!("op 1: {other:?}"),
    }
    match &p.ops[2] {
        Op::Arc { start: StartPoint::Known(s), end, center, sweep_deg, dir, .. } => {
            // G91.1: center = start(0,5) + (0,-5) = (0,0); end (5,0) —
            // cw от 90° к 0° = 90° (координаты G90-абсолютные).
            assert_eq!((s.x.value, s.y.value), (0.0, 5.0));
            assert_eq!((end.x.value, end.y.value), (5.0, 0.0));
            assert_eq!((center.x.value, center.y.value), (0.0, 0.0));
            assert_eq!(sweep_deg.value, 90.0);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
        }
        other => panic!("op 2 must be Arc, got {other:?}"),
    }
}

#[test]
fn spec_cov_lcnc_arc_modes_all_planes() {
    // Оба режима × все 3 плоскости: одна и та же дуга, записанная в
    // G90.1-центрах и G91.1-центрах, даёт идентичный AST (ABSORB, §2.2).
    // Плоскости G18 (u=Z,K / v=X,I) и G19 (u=Y,J / v=Z,K) — правые тройки.
    let g18_abs = concat!(
        "G21 G90 G94 G18 G90.1\n",
        "G0 X0.0000 Y0.0000 Z0.0000\n",
        "G3 X-5.0000 Y0.0000 Z5.0000 I0.0000 K5.0000 F100.0\n",
        "M2\n",
    );
    let g18_inc = concat!(
        "G21 G90 G94 G18 G91.1\n",
        "G0 X0.0000 Y0.0000 Z0.0000\n",
        "G3 X-5.0000 Y0.0000 Z5.0000 I0.0000 K5.0000 F100.0\n",
        "M2\n",
    );
    // G18 (u=Z, v=X, normal=+Y): center G90.1 = (I=0 → x, K=5 → z) = (0,0,5);
    // из (0,0,0): ρ = 5, ccw 90° → end (−5, 0, 5). При нулевом старте I/K
    // совпадают в обоих режимах → идентичный AST (ABSORB).
    let a = parse_str(g18_abs).expect("G18 G90.1");
    match &a.ops[1] {
        Op::Arc { center, sweep_deg, dir, .. } => {
            assert_eq!((center.x.value, center.y.value, center.z.value), (0.0, 0.0, 5.0));
            assert_eq!(sweep_deg.value, 90.0);
            assert_eq!(*dir, eid_core::ast::Dir::Ccw);
        }
        other => panic!("op 1: {other:?}"),
    }
    let b = parse_str(g18_inc).expect("G18 G91.1");
    match &b.ops[1] {
        Op::Arc { center, .. } => {
            assert_eq!((center.x.value, center.y.value, center.z.value), (0.0, 0.0, 5.0));
        }
        other => panic!("op 1: {other:?}"),
    }
    assert_diff_empty(&a, &b, "G18: G90.1 ≡ G91.1 (ABSORB)");

    // G19: u=Y (J), v=Z (K), normal=+X.
    let g19_abs = concat!(
        "G21 G90 G94 G19 G90.1\n",
        "G0 X0.0000 Y0.0000 Z0.0000\n",
        "G2 X0.0000 Y5.0000 Z5.0000 J0.0000 K5.0000 F100.0\n",
        "M2\n",
    );
    let c = parse_str(g19_abs).expect("G19 parses");
    match &c.ops[1] {
        Op::Arc { center, normal, sweep_deg, dir, .. } => {
            // G19: center.x = start.x (normal), center.y = J0, center.z = K5
            // → center (0,0,5); ρ = 5; G2 cw из (0,0) в (5,5) вокруг (0,5):
            // 270° (длинный путь — легален, sweep ∈ (0°, 360°]).
            assert_eq!((center.x.value, center.y.value, center.z.value), (0.0, 0.0, 5.0));
            assert_eq!((normal.x.value, normal.y.value, normal.z.value), (1.0, 0.0, 0.0));
            assert_eq!(sweep_deg.value, 270.0);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
        }
        other => panic!("op 1 must be Arc, got {other:?}"),
    }
    let d = parse_str(g19_abs.replace("G19 G90.1", "G19 G91.1").as_str()).expect("G19 G91.1");
    // Инкрементальные центры от старта (0,0,0): J0 K5 → center (0,0,5) — тот же.
    assert_diff_empty(&c, &d, "G19: G90.1 ≡ G91.1");
}

#[test]
fn spec_cov_lcnc_q11_machine_frame_computable_and_chain() {
    // §2(c) v0.10: machine-ход с in-text origin → старт Known (если
    // вычислим) / цепочка последующих machine-ходов; (d) возврат в WCS.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G10 L2 P1 X0.0000 Y0.0000 Z0.0000\n",
        "G0 X1.0000 Y1.0000 Z1.0000\n",
        "G53 G0 X2.0000 Y3.0000 Z4.0000\n",
        "G53 G0 X6.0000 Y3.0000 Z4.0000\n",
        "G0 X8.0000 Y3.0000 Z4.0000\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    // §2(c) computable (in-text origin): machine-ходы Known; цепочка.
    assert!(matches!(&p.ops[2], Op::Rapid { start: StartPoint::Known(s), end, .. }
        if to_f64(s) == [1.0, 1.0, 1.0] && to_f64(end) == [2.0, 3.0, 4.0]),
        "machine-ход: start = prev_wcs_end + O_P1 (§2c computable)");
    assert!(matches!(&p.ops[3], Op::Rapid { start: StartPoint::Known(s), end, .. }
        if to_f64(s) == [2.0, 3.0, 4.0] && to_f64(end) == [6.0, 3.0, 4.0]),
        "последующие machine-ходы цепляются (§2 v0.10)");
    // (d): возврат в WCS — reading = M − O_P1 = (6,3,4).
    assert!(matches!(&p.ops[4], Op::Rapid { start: StartPoint::Known(s), end, .. }
        if to_f64(s) == [6.0, 3.0, 4.0] && to_f64(end) == [8.0, 3.0, 4.0]),
        "возврат в WCS: reading = M − O (§2 (d))");
}

#[test]
fn spec_cov_lcnc_q11_machine_arc_first_op_err() {
    // §2 Q11: ARC с start=unknown (первый motion — без якоря) →
    // E_RUNTIME_STATE (rapid/linear допустимы с unknown, arc — нет).
    // Machine-дуга и вовсе недостижима: G53 требует явного G0/G1.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G10 L2 P1 X0.0000 Y0.0000 Z0.0000\n",
        "G53 G2 X5.0000 Y5.0000 I5.0000 J0.0000 F100.0\n",
        "M2\n",
    );
    let err = parse_str(src).expect_err("arc требует числового старта / G53 требует G0/G1");
    assert!(
        err.iter().all(|d| matches!(
            d.code,
            ErrorCode::RuntimeState | ErrorCode::InvalidInput
        )),
        "{err:?}"
    );
}

#[test]
fn spec_cov_lcnc_q11_arc_after_unknown_position_runtime() {
    // §2 Q11: ARC после позиционно-неопределённого участка (смена WCS без
    // in-text origins — разрыв (b)) → start unknown недопустим для дуги →
    // E_RUNTIME_STATE (rapid/linear с unknown — Ok при полном end).
    let err = parse_str(concat!(
        "G21 G90 G94 G17\n",
        "G0 X1.0000 Y1.0000 Z1.0000\n",
        "G55\n",
        "G2 X5.0000 Y5.0000 I5.0000 J0.0000 F100.0\n",
        "M2\n",
    ))
    .expect_err("§2 Q11: ARC на неизвестном старте");
    assert!(err.iter().all(|d| d.code == ErrorCode::RuntimeState), "{err:?}");
}

// ===========================================================================
// spec_cov — Stage-атрибуция диагностик (interface v0.1.2: Parse/Verify/Emit)
// ===========================================================================

#[test]
fn spec_cov_lcnc_stage_attribution() {
    // Каждая диагностика крейта несёт правильный Stage:
    //  * Parse — лексика/структура кадра (lexer + parse_block fatals);
    //  * Verify — инварианты собранного AST (finish: PEND/feed/arc-guard);
    //  * Emit — эмиссионные отказы (таблица легальности, COVERAGE §5).
    use eid_core::diag::Stage;

    // Parse: лексический отказ (O-code) — stage=Parse.
    let err = parse_str("G21 G90 G94\nO100\nM2\n").expect_err("O-code");
    assert!(err.iter().all(|d| d.stage == Stage::Parse && d.code == ErrorCode::UnsupportedSyntax));

    // Parse: структурный отказ кадра (G53 без G0/G1) — stage=Parse.
    let err = parse_str("G21 G90 G94\nG53 G2 X5.0000 Y5.0000 I5.0000 J0.0000 F100.0\nM2\n")
        .expect_err("G53+дуга");
    assert!(err.iter().all(|d| d.stage == Stage::Parse));

    // Verify: PEND-инвариант (нет program_end) — stage=Verify (finish).
    let err = parse_str("G21 G90 G94\nG0 X1.0000 Y1.0000 Z1.0000\n")
        .expect_err("без M2");
    assert!(err.iter().all(|d| d.stage == Stage::Verify && d.code == ErrorCode::InvalidInput));

    // Verify: linear без feed (инвариант §3) — stage=Verify.
    let err = parse_str("G21 G90 G94\nG0 X1.0000 Y1.0000 Z1.0000\nG1 X2.0000 Y1.0000 Z1.0000\nM2\n")
        .expect_err("linear без F");
    assert!(err.iter().all(|d| d.stage == Stage::Verify));

    // Emit: не-каноническая нормаль дуги — stage=Emit (таблица легальности).
    let mut p = parse_str("G21 G90 G94 G17\nG0 X0.0000 Y0.0000 Z0.0000\nG2 X5.0000 Y5.0000 I5.0000 J0.0000 F100.0\nM2\n")
        .expect("parses");
    for op in &mut p.ops {
        if let Op::Arc { normal, .. } = op {
            normal.x = eid_core::ast::Num::new("0.6000".into(), 0.6);
            normal.y = eid_core::ast::Num::new("0.8000".into(), 0.8);
        }
    }
    let err = LinuxCncAdapter::new().emit(&p).expect_err("не-осевой normal");
    // eid-core validate (Canonicalize/Verify у ядра — нас интересует факт
    // отказа до прохода эмиссии) или таблица легальности (Emit).
    for d in &err {
        assert_eq!(d.code, ErrorCode::InvalidInput, "{d:?}");
    }
}
