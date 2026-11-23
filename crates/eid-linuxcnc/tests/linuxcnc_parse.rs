//! Тесты канонизатора LinuxCNC (Агент D, итерации 2–5).
//!
//! Позитив: пилотные векторы Tier A #01, #02, #03, #11, #27, #36, #46 —
//! сравнение с golden AST через `eid_harness::diff::semantic_diff`
//! (допуски EQUIVALENCE §4; src/sub вне эквивалентности).
//!
//! Негатив (правило двух классов, CORPUS §1.2 v0.5):
//!
//!  * Err-класс (структурные): N1, N2, N5a, N5b, N12, N13, N14, N15,
//!    N16, N17, N19 — точные коды; N7 снят v0.21 (units-switch легален,
//!    Raw-носитель + chaining — SPEC §2.5 принцип 5 ревизия);
//!  * Ok-класс (line-level RAW + диагностика): N3, N7 (v0.21), N8, N9,
//!    N10, N18 + Macro-B строки (v0.21, зеркало eid-fanuc v0.16).
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
//!    dwell (G82/G89/G86 — P обязателен, v0.18), пеки d_k = R−kQ (G83/G73:
//!    расписание как G83; G73 — одиночный rapid d+клиренс после каждого
//!    промежуточного пека, §7.4a v0.18; клиренс 0.01in unit-scaled),
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
use eid_harness::diff::{semantic_diff, DiffConfig, Divergence};
use eid_linuxcnc::canonicalize::to_f64;
use eid_linuxcnc::parser::NoticeKind;
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
    // v0.2 #22: owning-результат — program в ParseOutput.
    LinuxCncAdapter::new()
        .parse(&read(rel), rel)
        .map(|out| out.program)
}

fn parse_golden(rel: &str) -> eid_core::ast::Program {
    EidTextAdapter::new()
        .parse(&read(rel), rel)
        .expect("golden AST parses via eid-core")
        .program
}

fn assert_diff_empty(a: &eid_core::ast::Program, b: &eid_core::ast::Program, ctx: &str) {
    let d = semantic_diff(a, b, &DiffConfig::default());
    assert!(d.is_empty(), "{ctx}: unexpected divergences: {d:?}");
}

fn assert_err_code(rel: &str, code: ErrorCode, ctx: &str) {
    match parse_linuxcnc(rel) {
        Ok(p) => panic!(
            "{ctx}: expected Err({code:?}), got Ok with {} ops",
            p.ops.len()
        ),
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
    let out = match adapter.parse(&read(rel), rel) {
        Ok(out) => out,
        Err(diags) => panic!(
            "{ctx}: expected Ok + diag {code:?}, got Err: {:?}",
            diags
                .iter()
                .map(|d| (d.code, &d.message))
                .collect::<Vec<_>>()
        ),
    };
    let p = out.program;
    let codes: Vec<ErrorCode> = out.diagnostics.iter().map(|d| d.code).collect();
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
    let mine =
        parse_linuxcnc("03_feed_survives_rapid/feed_survives_rapid.linuxcnc.ngc").expect("parses");
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
        Op::Arc {
            start,
            end,
            center,
            sweep_deg,
            dir,
            pitch,
            ..
        } => {
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
    assert_diff_empty(
        &mine,
        &golden,
        "#11 (golden = derived values, CORPUS v0.5.2)",
    );
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
    let mine = parse_linuxcnc("36_preset_G54_then_switch/preset_G54_then_switch.linuxcnc.ngc")
        .expect("parses");
    let golden = parse_golden("36_preset_G54_then_switch/golden.eid");
    let preset = mine.frame_preset.as_ref().expect("frame_preset present");
    assert_eq!(preset.wcs, eid_core::ast::Wcs::G54);
    assert_eq!(preset.values.x.value, 0.0);
    assert_eq!(preset.values.y.value, 0.0);
    assert_eq!(preset.values.z.value, 5.0);
    // OP1: якорь пресетки → known; OP3/OP5: смена WCS без define → unknown.
    let starts = mine
        .ops
        .iter()
        .map(|o| match o {
            Op::Rapid { start, .. } | Op::Linear { start, .. } => *start == StartPoint::Unknown,
            _ => false,
        })
        .collect::<Vec<_>>();
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
        Op::Arc {
            center,
            sweep_deg,
            pitch,
            dir,
            ..
        } => {
            assert_eq!(
                (center.x.value, center.y.value, center.z.value),
                (0.0, 5.0, 0.0)
            );
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
    assert_err_code(
        "n1_g53_no_origins/g53_no_origins.linuxcnc.ngc",
        ErrorCode::RuntimeState,
        "N1",
    );
}

#[test]
fn n2_mid_g92_ok_frame_preset_op() {
    // SPEC §5.3 v0.20 (зонды 1–3): mid-program G92 = оп frame_preset — Err N2
    // снят. Носитель узла в замороженном интерфейсе v0.2.0 — line-level Raw
    // (байты lossless same-dialect); чейнинг: rebase чтений от текущей
    // позиции. Вектор: G0 (0,0,5) → G1 X10 → G92 X0 Y0 Z0 → M2.
    let p = assert_ok_diag(
        "n2_mid_G92/mid_G92.linuxcnc.ngc",
        ErrorCode::UnsupportedSemantics,
        "N2",
    );
    assert!(p.frame_preset.is_none(), "стартовой пресетки нет — G92 mid");
    assert_eq!(p.ops.len(), 4); // Rapid + Linear + Raw(G92) + PEND
    assert!(
        matches!(&p.ops[0], Op::Rapid { start: StartPoint::Unknown, end, .. }
            if to_f64(end) == [0.0, 0.0, 5.0]),
        "op 0 = Rapid unknown→(0,0,5): {:?}",
        p.ops[0]
    );
    assert!(
        matches!(&p.ops[1], Op::Linear { start: StartPoint::Known(s), end, .. }
            if to_f64(s) == [0.0, 0.0, 5.0] && to_f64(end) == [10.0, 0.0, 5.0]),
        "op 1 = Linear (0,0,5)→(10,0,5) — до G92 чейнинг обычный: {:?}",
        p.ops[1]
    );
    match &p.ops[2] {
        Op::Raw {
            payload_hex,
            code,
            reason,
            ..
        } => {
            // "G92 X0.0000 Y0.0000 Z0.0000" + LF — D9
            assert_eq!(
                payload_hex,
                "4739322058302E303030302059302E30303030205A302E303030300A"
            );
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(
                reason.starts_with("G92 mid-program"),
                "маркер Raw-носителя frame_preset: {reason}"
            );
        }
        other => panic!("op 2 must be Raw (носитель frame_preset), got {other:?}"),
    }
    assert!(matches!(&p.ops[3], Op::ProgramEnd { .. }));
}

#[test]
fn n5a_arc_radius_mismatch_err_invalid_input() {
    assert_err_code(
        "n5a_arc_radius_mismatch/arc_radius_mismatch.linuxcnc.ngc",
        ErrorCode::InvalidInput,
        "N5a",
    );
}

#[test]
fn n5b_arc_chord_gt_2r_err_invalid_input() {
    assert_err_code(
        "n5b_arc_chord_gt_2R/arc_chord_gt_2R.linuxcnc.ngc",
        ErrorCode::InvalidInput,
        "N5b",
    );
}

#[test]
fn n7_unit_switch_mid_ok_carrier_chaining() {
    // SPEC §2.5 принцип 5, ревизия v0.21 (инвентаризация Tier C): LinuxCNC
    // исполняет G20/G21 в любой позиции — прежний Err N7 снят. Смена
    // системы чтения = line-level Raw-носитель (опа units в AST v0.2 нет,
    // байты lossless same-dialect) + chaining-конверсия 25.4 (start
    // следующего хода = прежнее физическое положение в НОВОЙ системе).
    // Вектор: G21-заголовок / G0 (0,0,5) mm / G20 / G0 (10,10,5) inch / M2
    // → [Rapid(unknown), Raw-носитель, Rapid(inch), PEND].
    let p = assert_ok_diag(
        "n7_unit_switch_mid/unit_switch_mid.linuxcnc.ngc",
        ErrorCode::UnsupportedSemantics,
        "N7",
    );
    assert_eq!(
        p.ops.len(),
        4,
        "[Rapid, Raw-носитель G20, Rapid(inch), PEND]"
    );
    assert_eq!(p.units, eid_core::ast::Units::Mm, "первое вхождение = G21");
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    match &p.ops[1] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(
                reason.contains("units switch mid-program"),
                "маркер chaining в reason: {reason}"
            );
        }
        other => panic!("op 1 must be Raw-носитель G20, got {other:?}"),
    }
    // Inch-ход: старт = (0,0,5)mm = (0,0,0.1969)in (конверсия 25.4, 4dp).
    match &p.ops[2] {
        Op::Rapid {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_eq!((s.x.value, s.y.value, s.z.value), (0.0, 0.0, 0.1969));
            assert_eq!((end.x.value, end.y.value, end.z.value), (10.0, 10.0, 5.0));
        }
        other => panic!("op 2 must be inch Rapid, got {other:?}"),
    }
    assert!(matches!(&p.ops[3], Op::ProgramEnd { .. }));
}

#[test]
fn n12_program_end_not_last_raw_tail_ok() {
    // SPEC §5.2 v0.20 (зонд 5): блоки после M2/M30 — Raw-хвост + диагностика
    // диалект-расхождения; Err «program end is not the last block» (N12)
    // снят: rs274 молча игнорирует хвост (exit 0), grbl-стрим исполняет.
    // Вектор: G0 (0,0,5) / M2 / G0 X10 Y10 Z5 / M2 → [Rapid, PEND, Raw, Raw].
    let p = assert_ok_diag(
        "n12_program_end_not_last/program_end_not_last.linuxcnc.ngc",
        ErrorCode::UnsupportedSemantics,
        "N12",
    );
    assert_eq!(
        p.ops.len(),
        4,
        "[Rapid, PEND, Raw(G0-хвост), Raw(M2-хвост)]"
    );
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    assert!(matches!(&p.ops[1], Op::ProgramEnd { .. }));
    match &p.ops[2] {
        Op::Raw {
            payload_hex,
            code,
            reason,
            ..
        } => {
            // "G0 X10.0000 Y10.0000 Z5.0000" + LF — D9
            assert_eq!(
                payload_hex,
                "4730205831302E30303030205931302E30303030205A352E303030300A"
            );
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(
                reason.contains("dialect-divergent"),
                "диалект-расхождение в reason: {reason}"
            );
        }
        other => panic!("op 2 must be Raw-хвост, got {other:?}"),
    }
    assert!(
        matches!(&p.ops[3], Op::Raw { .. }),
        "второй M2 после PEND — тоже хвост (не второй PEND): {:?}",
        p.ops[3]
    );
}

#[test]
fn n13_program_end_duplicate_tail_ok() {
    // SPEC §5.2 v0.20 (зонд 5): второй M2 — БЛОК ПОСЛЕ program_end →
    // Raw-хвост (rs274 его никогда не парсит), duplicate-PEND Err (N13)
    // недостижим из канонизатора. Дубликат PEND в AST (EID-text путь,
    // не из этого парсера) по-прежнему Err (emit-таблица program_end_duplicate).
    let p = assert_ok_diag(
        "n13_program_end_duplicate/program_end_duplicate.linuxcnc.ngc",
        ErrorCode::UnsupportedSemantics,
        "N13",
    );
    let pend_count = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::ProgramEnd { .. }))
        .count();
    assert_eq!(pend_count, 1, "ровно один PEND-оп; второй M2 — Raw-хвост");
    assert!(
        matches!(p.ops.last(), Some(Op::Raw { .. })),
        "хвост последним: {:?}",
        p.ops.last()
    );
}

#[test]
fn n14_program_end_missing_auto_pend() {
    // SPEC §3 v0.16: отсутствие M2/M30 = неявный конец — parse Ok +
    // notice MissingProgramEnd (ProgramEnd добавлен автоматически).
    let a = LinuxCncAdapter::new();
    let out = a
        .parse(
            &read("n14_program_end_missing/program_end_missing.linuxcnc.ngc"),
            "n14",
        )
        .expect("N14: missing PEND теперь Ok (auto-PEND)");
    let p = out.program;
    assert!(
        matches!(p.ops.last(), Some(Op::ProgramEnd { .. })),
        "auto-PEND последним"
    );
    let notices = out.notices;
    assert!(
        notices
            .iter()
            .any(|n| n.kind == NoticeKind::MissingProgramEnd),
        "notice MissingProgramEnd обязателен: {notices:?}"
    );
}

#[test]
fn n15_wcs_set_here_mid_err_semantics() {
    assert_err_code(
        "n15_wcs_set_here_mid/wcs_set_here_mid.linuxcnc.ngc",
        ErrorCode::UnsupportedSemantics,
        "N15",
    );
}

#[test]
fn n16_g53_after_preset_err_runtime_state() {
    assert_err_code(
        "n16_g53_after_preset/g53_after_preset.linuxcnc.ngc",
        ErrorCode::RuntimeState,
        "N16",
    );
}

#[test]
fn n17_multiple_preset_ok_q5_closed() {
    // SPEC §5.3 v0.20 / Q5 ЗАКРЫТ: несколько G92 легальны — стартовая =
    // заголовок Program.frame_preset (back-compat goldens), последующие =
    // опы (Raw-носитель в интерфейсе v0.2.0). Вектор: G92 (0,0,5) — заголовок;
    // G92 (10,0,5) — оп; M2. Motion-опов нет → чейнинг не задействован.
    let p = assert_ok_diag(
        "n17_multiple_preset_at_start/multiple_preset_at_start.linuxcnc.ngc",
        ErrorCode::UnsupportedSemantics,
        "N17",
    );
    let fp = p
        .frame_preset
        .as_ref()
        .expect("стартовая пресетка в заголовке");
    assert_eq!(to_f64(&fp.values), [0.0, 0.0, 5.0]);
    assert_eq!(fp.wcs, eid_core::ast::Wcs::G54);
    assert_eq!(p.ops.len(), 2); // Raw(G92 #2) + PEND
    match &p.ops[0] {
        Op::Raw {
            payload_hex,
            code,
            reason,
            ..
        } => {
            // "G92 X10.0000 Y0.0000 Z5.0000" + LF — D9
            assert_eq!(
                payload_hex,
                "473932205831302E303030302059302E30303030205A352E303030300A"
            );
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(reason.starts_with("G92 mid-program"), "{reason}");
        }
        other => panic!("op 0 must be Raw (носитель frame_preset), got {other:?}"),
    }
    assert!(matches!(&p.ops[1], Op::ProgramEnd { .. }));
}

#[test]
fn n19_arc_r_full_circle_err_invalid_input() {
    assert_err_code(
        "n19_arc_R_full_circle/arc_R_full_circle.linuxcnc.ngc",
        ErrorCode::InvalidInput,
        "N19",
    );
}

// ===========================================================================
// Негативы — Ok-класс (line-level RAW + диагностика)
// ===========================================================================

#[test]
fn n3_raw_vendor_block_ok_with_semantics() {
    let p = assert_ok_diag(
        "n3_raw_vendor_block/raw_vendor_block.linuxcnc.ngc",
        ErrorCode::UnsupportedSemantics,
        "N3",
    );
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    // Payload lossless: "M123" → hex.
    match &p.ops[0] {
        Op::Raw { payload_hex, .. } => assert_eq!(
            payload_hex, "4D3132330A",
            "D9 v0.15: payload с терминатором"
        ),
        other => panic!("op 0 must be Raw, got {other:?}"),
    }
    assert_eq!(p.ops.len(), 4); // Raw + Rapid + Linear + PEND
}

#[test]
fn n8_tapping_g84_ok_with_semantics() {
    // G84 — refuse-цикл: line-level RAW без цикл-парсера (COVERAGE §2.5).
    // Файл: header / G0 / G99 G84 … / M2 → ops: [Rapid, Raw, PEND].
    let p = assert_ok_diag(
        "n8_tapping_G84/tapping_G84.linuxcnc.ngc",
        ErrorCode::UnsupportedSemantics,
        "N8",
    );
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    assert!(matches!(
        &p.ops[1],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    assert!(matches!(&p.ops[2], Op::ProgramEnd { .. }));
}

#[test]
fn n9_bare_t_ok_with_requires_machine_table() {
    // SPEC §5.2 v0.20 (зонд 6): rs274 выполняет tool-table lookup в момент T —
    // отсутствующий номер = жёсткая ошибка с обрывом; машинная зависимость →
    // Raw{E_REQUIRES_MACHINE_TABLE} (бывший SEMANTICS сменён, N9, Ok-класс).
    let p = assert_ok_diag(
        "n9_bare_T_linuxcnc/bare_T_linuxcnc.linuxcnc.ngc",
        ErrorCode::RequiresMachineTable,
        "N9",
    );
    match &p.ops[0] {
        Op::Raw {
            payload_hex, code, ..
        } => {
            assert_eq!(payload_hex, "54350A"); // "T5" + LF — D9 v0.15
            assert_eq!(*code, ErrorCode::RequiresMachineTable);
        }
        other => panic!("op 0 must be Raw, got {other:?}"),
    }
    assert_eq!(p.ops.len(), 4); // Raw + Rapid + Linear + PEND
}

#[test]
fn n10_g43_h_table_ok_with_requires() {
    let p = assert_ok_diag(
        "n10_g43_h_table/g43_h_table.linuxcnc.ngc",
        ErrorCode::RequiresMachineTable,
        "N10",
    );
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
}

#[test]
fn n18_css_g96_ok_with_semantics() {
    let p = assert_ok_diag(
        "n18_css_g96/css_g96.linuxcnc.ngc",
        ErrorCode::UnsupportedSemantics,
        "N18",
    );
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
}

#[test]
fn n4_peck_l_over_limit() {
    // Расконсервирован (итерация 3-4): L99999 > max_unroll_iterations=10000
    // → E_LIMIT_EXCEEDED; guard проверяется до диагностики слов (в файле
    // нет F — feed-проверка не должна опередить лимит).
    assert_err_code(
        "n4_peck_L_over_limit/peck_L_over_limit.linuxcnc.ngc",
        ErrorCode::LimitExceeded,
        "N4",
    );
}

#[test]
fn n6_naked_coords_after_g80() {
    // Расконсервирован (итерация 3-4): G81 в файле теперь разворачивается;
    // голые координаты после G80 → E_INVALID_INPUT (COVERAGE §2.1).
    assert_err_code(
        "n6_naked_coords_after_G80/naked_coords_after_G80.linuxcnc.ngc",
        ErrorCode::InvalidInput,
        "N6",
    );
}

#[test]
fn n11_m98_call_line_level_raw() {
    // Ruling итерации 6 (Phase 1.2): M98/M99 → line-level Raw{SYNTAX}
    // (line-local конструкция, правило двух классов), разбор продолжается.
    let a = LinuxCncAdapter::new();
    let out = a
        .parse(&read("n11_m98_call/m98_call.linuxcnc.ngc"), "n11")
        .expect("N11: Ok (line-level Raw)");
    let p = out.program;
    assert!(
        matches!(
            &p.ops[0],
            Op::Raw { code: ErrorCode::UnsupportedSyntax, payload_hex, .. }
                if payload_hex.starts_with("4D3938")
        ),
        "payload = `M98…`"
    );
    assert!(matches!(&p.ops.last(), Some(Op::ProgramEnd { .. })));
    let codes: Vec<_> = out.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&ErrorCode::UnsupportedSyntax), "{codes:?}");
}

// ===========================================================================
// Юнит-тесты: G90.1/G91.1 ABSORB, модальное состояние, Q11
// ===========================================================================

fn parse_str(src: &str) -> Result<eid_core::ast::Program, Vec<eid_core::diag::Diagnostic>> {
    LinuxCncAdapter::new()
        .parse(src, "unit.ngc")
        .map(|out| out.program)
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
        Op::Arc {
            center, sweep_deg, ..
        } => {
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
    assert!(
        matches!(&p.ops[0], Op::Linear { state, .. } if state.feed.as_ref().unwrap().value == 150.0)
    );
    assert!(
        matches!(&p.ops[1], Op::Rapid { state, .. } if state.feed.as_ref().unwrap().value == 150.0)
    );
    // Bare X30 продолжает модальную группу 1 (G0 → Rapid), feed 150 жив.
    assert!(
        matches!(&p.ops[2], Op::Rapid { state, .. } if state.feed.as_ref().unwrap().value == 150.0)
    );
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
    assert!(matches!(
        &p.ops[2],
        Op::Rapid {
            start: StartPoint::Unknown,
            ..
        }
    ));
    match &p.ops[3] {
        Op::Rapid {
            start: StartPoint::Known(s),
            ..
        } => {
            assert_eq!((s.x.value, s.y.value, s.z.value), (1.0, 2.0, -2.0));
        }
        other => panic!("op 3 start must be known (обе системы define), got {other:?}"),
    }
}

#[test]
fn unit_q11_first_op_without_anchor_unknown() {
    let src = "G21 G90\nG0 X1.0000 Y2.0000 Z3.0000\nM2\n";
    let p = parse_str(src).expect("parses");
    assert!(matches!(
        &p.ops[0],
        Op::Rapid {
            start: StartPoint::Unknown,
            ..
        }
    ));
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
    let out = adapter.parse(src, "unit.ngc").expect("parses");
    let p = out.program;
    let notices = out.notices;
    assert_eq!(p.ops.len(), 2); // Linear + PEND (нулевые отброшены)
    assert_eq!(notices.len(), 2);
    assert!(notices
        .iter()
        .any(|n| n.kind == eid_linuxcnc::parser::NoticeKind::ZeroLengthMotion));
    assert!(notices
        .iter()
        .any(|n| n.kind == eid_linuxcnc::parser::NoticeKind::ZeroDwell));
}

#[test]
fn unit_block_delete_raw_runtime_state() {
    let src = "G21 G90 G94\n/ G0 X1.0000 Y1.0000 Z1.0000\nG0 X2.0000 Y2.0000 Z2.0000\nM2\n";
    let adapter = LinuxCncAdapter::new();
    let out = adapter.parse(src, "unit.ngc").expect("Ok (line-level)");
    let p = out.program;
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::RuntimeState,
            ..
        }
    ));
    let codes: Vec<_> = out.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&ErrorCode::RuntimeState));
}

#[test]
fn unit_hash_param_line_raw_syntax() {
    // v0.21 (зеркало eid-fanuc политики v0.16): `#`-параметр — line-level
    // Raw{E_UNSUPPORTED_SYNTAX}, разбор продолжается (бывший жёсткий
    // E_RUNTIME_STATE снят; исполнение = этап 12 L2, Q10 закрыт).
    let src = "G21 G90\nG0 X1.0000 Y1.0000 Z1.0000\n#100 = 5\nM2\n";
    let p = parse_str(src).expect("Ok + Raw");
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    match &p.ops[1] {
        Op::Raw {
            code,
            payload_hex,
            reason,
            ..
        } => {
            // "#100 = 5" + LF — D9 (hex lossless)
            assert_eq!(payload_hex, "23313030203D20350A");
            assert_eq!(*code, ErrorCode::UnsupportedSyntax);
            assert!(reason.contains("Macro B"), "{reason}");
        }
        other => panic!("op 1 must be Raw, got {other:?}"),
    }
    assert!(matches!(&p.ops[2], Op::ProgramEnd { .. }));
}

#[test]
fn unit_o_code_err_syntax() {
    // Ruling итерации 6: O-code строки → line-level Raw{SYNTAX}, разбор идёт.
    let a = LinuxCncAdapter::new();
    let out = a.parse("G21 G90\nO100\nM2\n", "o.ngc").expect("Ok + Raw");
    let p = out.program;
    assert!(
        matches!(&p.ops[0], Op::Raw { code: ErrorCode::UnsupportedSyntax, payload_hex, .. }
        if payload_hex == "4F3130300A"),
        "payload = `O100`"
    );
    assert!(matches!(&p.ops[1], Op::ProgramEnd { .. }));
    let codes: Vec<_> = out.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&ErrorCode::UnsupportedSyntax));
}

#[test]
fn q10_o_subprogram_no_inlining_pin() {
    // Q10 (ruling SPEC v0.19, пин «инлайнинг отклонён НАВСЕГДА для L1»):
    // исполнение O-sub/call — этап 12 L2. Количество опов = строк с O/M98:
    // каждая O-строка → ровно один Raw{SYNTAX}; тело sub присутствует в AST
    // РОВНО ОДИН РАЗ (текст программы), вызовы НЕ разгораются в ходы;
    // M98 → 1 Raw{SYNTAX} (P/L поглощены whole-line raw). Emit → reparse:
    // число опов стабильно.
    let src = concat!(
        "G21 G90 G94\n",
        "o100 sub\n",
        "G1 X10.0000 F100.0\n",
        "G1 X0.0000\n",
        "o100 endsub\n",
        "o100 call\n",
        "o100 call\n",
        "M98 P10 L2\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let out = a
        .parse(src, "osub.ngc")
        .expect("O-файл: Ok (line-level Raw)");
    let p = out.program;
    // 5 строк O/M98 (4 O + 1 M98) → 5 Raw; тело — 2 Linear РОВНО ОДИН РАЗ;
    // + PEND = 8. (Инлайнинг дал бы 2 Linear × 2 call = +4 хода.)
    assert_eq!(p.ops.len(), 8, "нет декомпозиции: {:?}", p.ops);
    let raws: Vec<&Op> = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Raw { .. }))
        .collect();
    let linears = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Linear { .. }))
        .count();
    assert_eq!(raws.len(), 5, "каждая O/M98-строка = один Raw");
    assert_eq!(linears, 2, "тело sub НЕ инлайнится (2 × call → +0 ходов)");
    for o in &raws {
        match o {
            Op::Raw {
                code, payload_hex, ..
            } => {
                assert_eq!(code, &ErrorCode::UnsupportedSyntax);
                assert!(
                    payload_hex.starts_with("6F313030") || payload_hex.starts_with("4D3938"),
                    "payload = `o100…`/`M98…`, фактически {payload_hex}"
                );
            }
            other => panic!("ожидались только Raw: {other:?}"),
        }
    }
    // M98: P/L-слова поглощены whole-line Raw (не Err «без потребителя»).
    assert!(
        raws.iter().any(|o| matches!(o, Op::Raw { payload_hex, .. }
            if payload_hex.starts_with("4D393820"))),
        "payload `M98 P10 L2` verbatim"
    );
    let text = a.emit(&p).expect("emit").text;
    let p2 = LinuxCncAdapter::new()
        .parse(&text, "osub2.ngc")
        .expect("reparse")
        .program;
    assert_eq!(
        p2.ops.len(),
        p.ops.len(),
        "emit→reparse: число опов стабильно"
    );
}

#[test]
fn unit_while_line_raw_syntax() {
    // v0.21: control-flow keyword (`while [1] do 1`) — line-level
    // Raw{E_UNSUPPORTED_SYNTAX} (зеркало eid-fanuc; бывший Err снят).
    let src = "G21 G90\nwhile [1] do 1\nM2\n";
    let p = parse_str(src).expect("Ok + Raw");
    assert!(
        matches!(
            &p.ops[0],
            Op::Raw {
                code: ErrorCode::UnsupportedSyntax,
                ..
            }
        ),
        "{:?}",
        p.ops[0]
    );
    assert!(matches!(&p.ops[1], Op::ProgramEnd { .. }));
}

#[test]
fn macro_b_if_goto_then_line_raw() {
    // v0.21 golden-пара: IF/GOTO/THEN строки (fanuc-диалект под linuxcnc-
    // меткой, инвентаризация классы #1/#6/#20) — каждая → ровно один
    // Raw{E_UNSUPPORTED_SYNTAX}, разбор продолжается, PEND на месте.
    let a = LinuxCncAdapter::new();
    let src = concat!(
        "G21 G90 G94\n",
        "#1=3\n",
        "IF [#1 GT 0] GOTO 100\n",
        "GOTO 100\n",
        "IF [#1 EQ 3] THEN M3\n",
        "G1 X10.0000 F100.0\n",
        "M2\n",
    );
    let out = a.parse(src, "macro_b.ngc").expect("Ok + Raw");
    let p = out.program;
    // [#1=3 Raw, IF-GOTO Raw, GOTO Raw, IF-THEN Raw, Linear, PEND]
    assert_eq!(p.ops.len(), 6, "{:?}", p.ops);
    for (i, op) in p.ops.iter().enumerate().take(4) {
        assert!(
            matches!(
                op,
                Op::Raw {
                    code: ErrorCode::UnsupportedSyntax,
                    ..
                }
            ),
            "op {i} must be Raw{{SYNTAX}}: {op:?}"
        );
    }
    assert!(matches!(&p.ops[4], Op::Linear { .. }));
    let syn = out
        .diagnostics
        .iter()
        .filter(|d| d.code == ErrorCode::UnsupportedSyntax)
        .count();
    assert_eq!(syn, 4, "диагностики ⊇ 4×SYNTAX");
}

#[test]
fn macro_b_expr_word_line_raw() {
    // v0.21 golden-пара (инвентаризация класс #2): слово со значением-
    // выражением (`X[...]`, `I[...]`) и значение-параметр (`T#3026`) —
    // line-level Raw{SYNTAX} вместо «слово без числа»/«выражение».
    let a = LinuxCncAdapter::new();
    for (src, label) in [
        ("G21 G90\nG1 X[1+2] F100.0\nM2\n", "X[expr]"),
        ("G21 G90\nT#3026\nM2\n", "T#n"),
        (
            "G21 G90\nG2 X5.0000 Y5.0000 I[1+1] J0.0 F100.0\nM2\n",
            "I[expr]",
        ),
    ] {
        let out = a
            .parse(src, "expr.ngc")
            .unwrap_or_else(|e| panic!("{label}: ожидался Ok + Raw, Err: {e:?}"));
        assert!(
            matches!(
                &out.program.ops[0],
                Op::Raw {
                    code: ErrorCode::UnsupportedSyntax,
                    ..
                }
            ),
            "{label}: первый оп — Raw{{SYNTAX}}: {:?}",
            out.program.ops[0]
        );
    }
}

#[test]
fn v021_units_switch_mid_motion_on_switch_line() {
    // SPEC §2.5 v0.21, ручной расчёт брифа: `G21 G0 X10 / G20 G1 X0.5 /
    // G21 G1 X10` — middle-ход в inch (0.5" = 12.7 мм); физическая
    // последовательность 10 → 12.7 → 10. Ход на switch-строке: rs274
    // исполняет весь блок — цепочка чтений продолжается в НОВОЙ системе
    // (оп не строится — байты строки в носителе, носитель ПЕРВЫМ).
    // AST: [Rapid(→10mm), Raw(G20-носитель), Raw(G21-носитель), PEND].
    let a = LinuxCncAdapter::new();
    let src = "G21 G90 G94 G17\nG0 X10\nG20 G1 X0.5\nG21 G1 X10\nM2\n";
    let out = a.parse(src, "units_mid.ngc").expect("Ok");
    let p = out.program;
    assert_eq!(p.ops.len(), 4, "{:?}", p.ops);
    assert_eq!(
        p.units,
        eid_core::ast::Units::Mm,
        "первое вхождение G21 = заголовок"
    );
    assert!(
        matches!(&p.ops[0], Op::Rapid { start: StartPoint::Unknown, end, .. } if end.x.value == 10.0)
    );
    for (i, expect_prefix) in [
        (1usize, "units switch mid-program"),
        (2, "units switch mid-program"),
    ] {
        match &p.ops[i] {
            Op::Raw { code, reason, .. } => {
                assert_eq!(*code, ErrorCode::UnsupportedSemantics);
                assert!(
                    reason.contains(expect_prefix),
                    "op {i}: маркер в reason: {reason}"
                );
            }
            other => panic!("op {i} must be Raw-носитель, got {other:?}"),
        }
    }
    assert!(matches!(&p.ops[3], Op::ProgramEnd { .. }));
    // Диагностики Ok-пути: 2 носителя (+ 2 W_MODAL_ON_RAW на ходы в
    // switch-кадрах — тот же код SEMANTICS; фильтр по маркеру).
    let carriers = out
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("units switch mid-program"))
        .count();
    assert_eq!(carriers, 2);
}

#[test]
fn v021_units_switch_chaining_converted_starts() {
    // Chaining через конверсию 25.4: ходы ПОСЛЕ switch-строк — start
    // следующего хода = прежнее физическое положение в НОВОЙ системе
    // (вычислимо — НЕ unknown). Физическая цепочка 10 → 12.7 → 10 мм
    // (ручной расчёт: 10mm = 0.3937"; 0.5" = 12.7mm).
    // v0.21-fix (N7, eid verify): anchor-согласование — ХРАНИМЫЙ end
    // последнего motion-опа конвертируется вместе с chaining-состоянием
    // (раньше end оставался literal: op2.end = 0.5 при op3.start = 12.7 —
    // «start != expected» на validate-пути CLI). Теперь пары границ
    // бит-точны: op1.end == op2.start == 0.3937; op2.end == op3.start == 12.7.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X10\n",        // (0,0,0) → (10,0,0) mm
        "G20\n",           // носитель; чтение 10mm = 0.3937"
        "G1 X0.5 F10.0\n", // inch: start (0.3937,0,0), end (0.5,0,0)
        "G21\n",           // носитель; чтение 0.5" = 12.7mm
        "G1 X10 F10.0\n",  // mm: start (12.7,0,0), end (10,0,0)
        "M2\n",
    );
    let p = parse_str(src).expect("Ok");
    // [Rapid, Raw, Linear, Raw, Linear, PEND]
    assert_eq!(p.ops.len(), 6, "{:?}", p.ops);
    match &p.ops[2] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            // 10mm / 25.4 = 0.39370078… → 4dp 0.3937 (дрейф 2e-5 мм < ε_pos)
            assert_eq!((s.x.value, s.y.value, s.z.value), (0.3937, 0.0, 0.0));
            // end в FINAL AST — 12.7: при второй смене (якорь следующего
            // хода) 0.5" конвертирован в mm (v0.21-fix anchor-согласование).
            assert_eq!((end.x.value, end.y.value, end.z.value), (12.7, 0.0, 0.0));
        }
        other => panic!("op 2 must be inch Linear, got {other:?}"),
    }
    match &p.ops[4] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            // 0.5" × 25.4 = 12.7mm — физическая последовательность 10→12.7→10
            assert_eq!((s.x.value, s.y.value, s.z.value), (12.7, 0.0, 0.0));
            assert_eq!((end.x.value, end.y.value, end.z.value), (10.0, 0.0, 0.0));
        }
        other => panic!("op 4 must be mm Linear, got {other:?}"),
    }
    // Пары границ бит-точны (start_{i+1} == end_i) — без «start != expected».
    let ends_starts = |i: usize, j: usize| {
        let e = eid_linuxcnc::parser::motion_end_vals(&p.ops[i]).expect("motion end");
        match &p.ops[j] {
            Op::Linear {
                start: StartPoint::Known(s),
                ..
            }
            | Op::Rapid {
                start: StartPoint::Known(s),
                ..
            } => [s.x.value, s.y.value, s.z.value] == e,
            _ => false,
        }
    };
    assert!(
        ends_starts(0, 2) && ends_starts(2, 4),
        "границы бит-точны после anchor-конверсии"
    );
}

#[test]
fn v021_units_switch_roundtrip_stable() {
    // Round-trip: emit → reparse — число опов и физическая цепочка
    // (дрейф в пределах ε_pos: verbatim-концы не переквантуются).
    let a = LinuxCncAdapter::new();
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X10\n",
        "G20\n",
        "G1 X0.5 F10.0\n",
        "G21\n",
        "G1 X10 F10.0\n",
        "M2\n",
    );
    let p1 = a.parse(src, "rt1.ngc").expect("parse 1").program;
    let text = a.emit(&p1).expect("emit").text;
    // Носитель G20 в потоке — иначе семантика потеряна.
    assert!(
        text.lines().any(|l| l.trim().starts_with("G20")),
        "G20 в эмиттенном потоке:\n{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.trim().starts_with("G21") && !l.contains("G90 G94")),
        "G21-носитель после заголовка:\n{text}"
    );
    let p2 = a.parse(&text, "rt2.ngc").expect("reparse").program;
    let d = semantic_diff(&p1, &p2, &DiffConfig::default());
    assert!(d.is_empty(), "round-trip: unexpected divergences: {d:?}");
}

#[test]
fn v021_units_anchor_n7_pattern_exact_chain() {
    // N7-паттерн (ходы → switch на ОТДЕЛЬНОЙ строке → ход): anchor-
    // согласование — op1.end конвертируется вместе с chaining-состоянием,
    // границы бит-точны, и КОРЕКТОР (eid_core::verify::validate — тот же
    // путь, что eid verify CLI) проходит без «start != expected».
    // Физика: Z5mm = 0.1969"; ход 10" (X) в inch-режиме.
    let src = concat!(
        "G21 G90 G94 G17 G91.1\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G20\n",
        "G0 X10.0000 Y10.0000 Z5.0000\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let p = a.parse(src, "n7pat.ngc").expect("parse Ok").program;
    assert_eq!(p.ops.len(), 4, "[Rapid, Raw(G20), Rapid, PEND]");
    // Anchor-конверсия: op1.end = 5mm → 0.1969" (бит-равен op2.start).
    match (&p.ops[0], &p.ops[2]) {
        (
            Op::Rapid { end: e1, .. },
            Op::Rapid {
                start: StartPoint::Known(s2),
                end: e2,
                ..
            },
        ) => {
            let e1v = [e1.x.value, e1.y.value, e1.z.value];
            assert_eq!(e1v, [0.0, 0.0, 0.1969], "op1.end конвертирован (якорь)");
            assert_eq!(
                [s2.x.value, s2.y.value, s2.z.value],
                e1v,
                "граница бит-точна: op2.start == op1.end"
            );
            assert_eq!(
                [e2.x.value, e2.y.value, e2.z.value],
                [10.0, 10.0, 5.0],
                "inch-слова хранятся literal (система чтения)"
            );
        }
        other => panic!("неожиданный AST: {other:?}"),
    }
    // Тот же путь, что eid verify CLI: parse → core validate → Ok.
    let vdiags = eid_core::verify::validate(&p);
    assert!(
        vdiags.is_empty(),
        "core validate (путь eid verify) чист: {vdiags:?}"
    );
    // R1: emit → reparse — тот же AST (эмиттер восстанавливает
    // до-конверсные литералы, репарс повторяет конверсию).
    let text = a.emit(&p).expect("emit").text;
    let p2 = a.parse(&text, "n7pat2.ngc").expect("reparse").program;
    let d = semantic_diff(&p, &p2, &DiffConfig::default());
    assert!(
        d.is_empty(),
        "R1 n7-паттерна: {d:?}\n--- emitted ---\n{text}"
    );
}

#[test]
fn v021_units_anchor_no_switch_unchanged() {
    // Негатив-регресс (N7-фикс): БЕЗ switch-строки anchor-сравнение
    // работает как прежде — цепочка literal-точна, коректор чист.
    let src =
        "G21 G90 G94 G17 G91.1\nG0 X0.0000 Y0.0000 Z5.0000\nG0 X10.0000 Y10.0000 Z5.0000\nM2\n";
    let a = LinuxCncAdapter::new();
    let p = a.parse(src, "nosw.ngc").expect("parse Ok").program;
    match (&p.ops[0], &p.ops[1]) {
        (
            Op::Rapid { end: e1, .. },
            Op::Rapid {
                start: StartPoint::Known(s2),
                ..
            },
        ) => {
            assert_eq!(
                [e1.x.value, e1.y.value, e1.z.value],
                [s2.x.value, s2.y.value, s2.z.value],
                "без switch цепочка не тронута"
            );
        }
        other => panic!("неожиданный AST: {other:?}"),
    }
    let vdiags = eid_core::verify::validate(&p);
    assert!(vdiags.is_empty(), "core validate чист: {vdiags:?}");
}

#[test]
fn v021_negative_regressions_intact() {
    // Негатив-регрессы волны v0.21: настоящий мусор остаётся Err.
    let a = LinuxCncAdapter::new();
    // P-слово без потребителя вне Macro-B (инвентаризация #10, TRUE-ERR —
    // rs274: «P word with no G4/G10/...»).
    let err = a
        .parse(
            "G21 G90 G94\nG0 X0 Y0 Z0\nG2 X5.0000 Y5.0000 I5.0000 J0.0000 P4 F100.0\nM2\n",
            "p.ngc",
        )
        .expect_err("P без потребителя");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
    // Letter-run мусор (не keyword) — Err (настоящий мусор).
    let err = a
        .parse("G21 G90\nGT 5\nM2\n", "gt.ngc")
        .expect_err("letter-run мусор");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
    // Дуга-продолжение без offset-слов — текст выровнен с контроллерами
    // (GRBL error 2 ARC_OFFSETS_MISSING; инвентаризация #9), вердикт Err.
    let err = a
        .parse(
            "G21 G90 G94 G17\nG0 X0 Y0 Z0\nG91.1\nG2 X5.0000 Y5.0000 F100.0\nM2\n",
            "arc.ngc",
        )
        .expect_err("дуга без offsets");
    assert!(
        err.iter()
            .any(|d| d.message.contains("ARC_OFFSETS_MISSING")),
        "{err:?}"
    );
    // Частичные offsets (I без нужной J → ρ_s ≠ ρ_e) — прежний
    // radius-mismatch текст цел (N5a).
    let err = a
        .parse(
            "G21 G90 G94 G17\nG0 X0 Y0 Z0\nG91.1\nG2 X5.0000 Y0.0000 I2.0000 F100.0\nM2\n",
            "arc2.ngc",
        )
        .expect_err("radius mismatch");
    assert!(
        err.iter()
            .any(|d| d.message.contains("несоответствие радиусов")),
        "{err:?}"
    );
    // bare-T → Raw{TABLE} (N9) цел — см. unit_bare_t_then_m6_uses_selection.
}

#[test]
fn unit_bare_t_then_m6_uses_selection() {
    // SPEC §5.2 v0.20 (зонд 6b): T с существующим номером → SELECT_TOOL-absorb;
    // AST-уровень наличия в tool.tbl не знает → Raw{TABLE} для обоих случаев,
    // предвыбор сохраняется; M6 без T → ToolChange 5.
    let src = "G21 G90 G94\nT5\nM6\nG0 X1.0000 Y1.0000 Z1.0000\nM2\n";
    let adapter = LinuxCncAdapter::new();
    let out = adapter.parse(src, "unit.ngc").expect("Ok");
    let p = out.program;
    assert!(matches!(&p.ops[0], Op::Raw { .. }));
    assert!(matches!(&p.ops[1], Op::ToolChange { tool: 5, .. }));
    let codes: Vec<_> = out.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&ErrorCode::RequiresMachineTable));
}

#[test]
fn unit_m0_m1_stops() {
    let src = "G21 G90 G94\nG0 X0 Y0 Z0\nM0\nG0 X1.0000 Y0 Z0\nM1\nG0 X2.0000 Y0 Z0\nM2\n";
    let p = parse_str(src).expect("parses");
    assert!(matches!(
        &p.ops[1],
        Op::ProgramStop {
            optional: false,
            ..
        }
    ));
    assert!(matches!(&p.ops[3], Op::ProgramStop { optional: true, .. }));
}

#[test]
fn unit_path_blend_and_mid_switch_err() {
    let ok = parse_str("G21 G90\nG64 P0.0500\nG0 X0 Y0 Z0\nM2\n").expect("G64 P- parses");
    assert!(matches!(&ok.path_mode, PathMode::Blend { t } if t.as_ref().unwrap().value == 0.05));
    let err = parse_str("G21 G90\nG61\nG0 X0 Y0 Z0\nG64\nM2\n").expect_err("mid-switch");
    assert!(
        err.iter()
            .all(|d| d.code == ErrorCode::UnsupportedSemantics),
        "{err:?}"
    );
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
    let out = adapter.parse(src, "unit.ngc").expect("Ok (line-level)");
    let p = out.program;
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
}

#[test]
fn unit_naked_coords_after_g80_err_invalid_input() {
    // Правило N6 на уровне, достижимом без циклов (COVERAGE §2.1): после G80
    // модальная группа очищена → голые координаты = E_INVALID_INPUT.
    let src = "G21 G90 G94\nG0 X0 Y0 Z0\nG80\nX20.0000 Y20.0000\nM2\n";
    let err = parse_str(src).expect_err("E_INVALID_INPUT");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
}

#[test]
fn unit_dwell_negative_err() {
    let src = "G21 G90 G94\nG0 X0 Y0 Z0\nG4 P-1.0\nM2\n";
    let err = parse_str(src).expect_err("E_INVALID_INPUT");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
}

#[test]
fn unit_modal_group_conflict_err() {
    let src = "G21 G90 G94\nG0 G1 X5.0000\nM2\n";
    let err = parse_str(src).expect_err("E_INVALID_INPUT");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
}

#[test]
fn unit_word_len_limit_exceeded() {
    let long = "1".repeat(33);
    let src = format!("G21 G90\nG0 X{long} Y0 Z0\nM2\n");
    let err = parse_str(&src).expect_err("E_LIMIT_EXCEEDED");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::LimitExceeded),
        "{err:?}"
    );
}

#[test]
fn unit_arc_without_center_err() {
    let src = "G21 G90 G94 G17\nG0 X0 Y0 Z0\nG2 X5.0000 Y5.0000 F100.0\nM2\n";
    let err = parse_str(src).expect_err("E_INVALID_INPUT");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
}

#[test]
fn unit_arc_r_negative_major_sweep() {
    // Отрицательный R → дуга >180° (знак R выбирает ≤/> 180°, SPEC §6).
    let src = "G21 G90 G94 G17\nG0 X0 Y10.0000 Z0\nG2 X0.0175 Y-9.9998 R-10.0 F100.0\nM2\n";
    let p = parse_str(src).expect("parses");
    match &p.ops[1] {
        Op::Arc {
            center, sweep_deg, ..
        } => {
            // Зеркальная сторона перпендикуляра; sweep = 360 − 179.4975 ≈ 180.5025.
            assert!(
                sweep_deg.value > 180.0,
                "negative R → major arc, got {}",
                sweep_deg.value
            );
            assert!(
                (center.x.value - 0.0526).abs() < 1e-3,
                "center.x = {}",
                center.x.value
            );
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
    let src =
        "G21 G90 G94 G17 G91.1\nG0 X10.0000 Y0.0000 Z0.0000\nG2 I-5.0000 J0.0000 F120.0\nM2\n";
    let p = parse_str(src).expect("parses");
    assert_eq!(p.ops.len(), 3, "RAPID + ARC(360°) + PEND");
    match &p.ops[1] {
        Op::Arc {
            start: StartPoint::Known(s),
            end,
            center,
            sweep_deg,
            dir,
            ..
        } => {
            assert_eq!((s.x.value, s.y.value, s.z.value), (10.0, 0.0, 0.0));
            assert_eq!((end.x.value, end.y.value, end.z.value), (10.0, 0.0, 0.0));
            assert_eq!(
                (center.x.value, center.y.value, center.z.value),
                (5.0, 0.0, 0.0)
            );
            assert_eq!(sweep_deg.value, 360.0);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
        }
        other => panic!("op 1 must be Arc, got {other:?}"),
    }
}

#[test]
fn audit15_explicit_end_quantized_to_start_is_full_circle() {
    // SPEC §6 v0.17 (аудит #15, зонд rs274 2026-11-21): `G2 X10. Y0. I-5. J0`
    // из (10,0) → rs274 ARC_FEED(..., rotation=-1, ...) = ПОЛНЫЙ оборот CW;
    // CAM-посты (Fusion/Mastercam) систематически пишут end-слова на полных
    // кругах. Явный end, квантовавшийся к старту, при center ≠ start →
    // 360°-дуга (правило «формы команды» v0.13 заменено геометрическим
    // критерием; CORPUS #13: drop → 360°-дуга, golden regen v0.6.7).
    let src = concat!(
        "G21 G90 G94 G17 G91.1\n",
        "G0 X0.0000 Y0.0000 Z0.0000\n",
        "G2 X0.00004 Y0.0000 I5.0000 J0.0000 F100.0\n",
        "G1 X10.0000\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let p = a.parse(src, "unit13.ngc").expect("parses").program;
    assert_eq!(p.ops.len(), 4, "RAPID + ARC(360°) + LINEAR + PEND");
    match &p.ops[1] {
        Op::Arc {
            start: StartPoint::Known(s),
            end,
            center,
            sweep_deg,
            dir,
            pitch,
            ..
        } => {
            assert_eq!((s.x.value, s.y.value, s.z.value), (0.0, 0.0, 0.0));
            assert_eq!((end.x.value, end.y.value, end.z.value), (0.0, 0.0, 0.0));
            assert_eq!(
                (center.x.value, center.y.value, center.z.value),
                (5.0, 0.0, 0.0)
            );
            assert_eq!(
                sweep_deg.value, 360.0,
                "полный круг — независимо от явного end"
            );
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
            assert_eq!(pitch.value, 0.0);
        }
        other => panic!("op 1 must be Arc, got {other:?}"),
    }
    // v0.2 #22: notices в owning-результате (повторный parse эквивалентен).
    assert!(
        a.parse(src, "unit13.ngc")
            .expect("parses")
            .notices
            .is_empty(),
        "дуга сохранена — notice нет"
    );
}

#[test]
fn audit15_degenerate_center_start_dropped() {
    // Единственный drop-класс center-формы (v0.17): center == start (радиус
    // 0) при end == start и pitch == 0. rs274: «Zero-radius arc» (отказ) —
    // у нас drop+notice по правилу zero-length (SPEC §3).
    let src = concat!(
        "G21 G90 G94 G17 G91.1\n",
        "G0 X10.0000 Y0.0000 Z0.0000\n",
        "G2 X10.0000 Y0.0000 I0.0000 J0.0000 F100.0\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let out = a.parse(src, "deg.ngc").expect("parses");
    let p = out.program;
    assert_eq!(p.ops.len(), 2, "RAPID + PEND (дуга отброшена)");
    let notices = out.notices;
    assert!(
        notices.iter().any(|n| n.kind == NoticeKind::ZeroLengthArc),
        "zero-length arc notice отсутствует: {notices:?}"
    );
}

#[test]
fn audit15_degenerate_center_start_inherited_end_dropped() {
    // Degenerate без слов осей (`G2 I0 J0`, end наследует старт): rs274
    // отказывает («Zero-radius arc») — drop+notice (v0.17: критерий
    // геометрический, end == start && center == start).
    let src = concat!(
        "G21 G90 G94 G17 G91.1\n",
        "G0 X10.0000 Y0.0000 Z0.0000\n",
        "G2 I0.0000 J0.0000 F100.0\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let out = a.parse(src, "deg2.ngc").expect("parses");
    let p = out.program;
    assert_eq!(p.ops.len(), 2, "RAPID + PEND (дуга отброшена)");
    let notices = out.notices;
    assert!(
        notices.iter().any(|n| n.kind == NoticeKind::ZeroLengthArc),
        "{notices:?}"
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
        Op::Arc {
            sweep_deg, pitch, ..
        } => {
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
        Op::Arc {
            start: StartPoint::Known(s),
            end,
            center,
            sweep_deg,
            dir,
            state,
            ..
        } => {
            // G91: end = base + (5,5) = (15,5); G91.1: center = start + (5,0).
            assert_eq!((s.x.value, s.y.value, s.z.value), (10.0, 0.0, 0.0));
            assert_eq!((end.x.value, end.y.value, end.z.value), (15.0, 5.0, 0.0));
            assert_eq!(
                (center.x.value, center.y.value, center.z.value),
                (15.0, 0.0, 0.0)
            );
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
        Op::Rapid {
            start: StartPoint::Known(s),
            end,
            state,
            ..
        } => {
            // start = prev_end + O_G54 − O_G55 = (10,0,5) + (1,2,3) − (4,5,6).
            assert_eq!((s.x.value, s.y.value, s.z.value), (7.0, -3.0, 2.0));
            assert_eq!((end.x.value, end.y.value, end.z.value), (12.0, -3.0, 2.0));
            assert_eq!(state.wcs, eid_core::ast::Wcs::G55);
            assert_eq!(
                state.feed.as_ref().unwrap().value,
                150.0,
                "SPEC §4: feed живёт через G0/WCS"
            );
        }
        other => panic!("op 4 must be Rapid, got {other:?}"),
    }
    // Golden OP005 feed=none — File-wrong (отчёт итерации 3b; противоречит
    // golden #03 feed_survives_rapid). Допускаем ровно это расхождение;
    // после ревизии golden diff станет пустым.
    let golden = parse_golden("33_wcs_define_multi/golden.eid");
    let d = semantic_diff(&mine, &golden, &DiffConfig::default());
    assert!(
        d.iter().all(|x| matches!(
            x,
            Divergence::StateField {
                index: 4,
                field: "feed"
            }
        )),
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
            Op::Rapid {
                start: StartPoint::Known(s),
                end,
                sub,
                ..
            } => (to_f64(s), to_f64(end), *sub),
            Op::Linear {
                start: StartPoint::Known(s),
                end,
                sub,
                ..
            } => (to_f64(s), to_f64(end), *sub),
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
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            state,
            sub,
            ..
        } => {
            assert_eq!(
                (to_f64(s), to_f64(end)),
                ([10.0, 10.0, 2.0], [10.0, 10.0, -5.0])
            );
            assert_eq!(state.feed.as_ref().unwrap().value, 100.0);
            assert_eq!(*sub, Some(3));
        }
        other => panic!("op 3 must be Linear: {other:?}"),
    }
    // 4. retract (G99) на R=2.
    let (s, e, sub) = snap(4);
    assert_eq!(
        (s, e, sub),
        ([10.0, 10.0, -5.0], [10.0, 10.0, 2.0], Some(4))
    );
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
    let out = a.parse(src, "l2.ngc").expect("parses");
    let p = out.program;
    // iter1: traverse, rapid R, feed, retract (4); iter2: traverse/R нулевые
    // (drop), feed + retract живы (2) → initial + 6 + PEND = 8.
    assert_eq!(p.ops.len(), 8);
    assert!(matches!(&p.ops[5], Op::Linear { sub: Some(7), .. }));
    assert!(matches!(&p.ops[6], Op::Rapid { sub: Some(8), .. }));
    let notices = out.notices;
    assert_eq!(
        notices
            .iter()
            .filter(|n| n.kind == NoticeKind::ZeroLengthMotion)
            .count(),
        2
    );
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
    assert!(
        err.iter().all(|d| d.code == ErrorCode::RuntimeState),
        "{err:?}"
    );
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
    assert!(
        err.iter().all(|d| d.code == ErrorCode::RuntimeState),
        "{err:?}"
    );
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
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
}

#[test]
fn unit_cycle_g86_without_p_err() {
    // SPEC §7.4a v0.18: P в G86 обязателен — rs274 exit 1 «Dwell time p
    // word missing with g86» на файле без P (probe, вектор #21 v0);
    // E_INVALID_INPUT, сообщение зеркалирует rs274 дословно.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "M3 S8000\n",
        "G0 X1.0000 Y1.0000 Z1.0000\n",
        "G99 G86 X2.0000 Y2.0000 Z-1.0000 R1.0000 F100.0\n",
        "M2\n",
    );
    let err = parse_str(src).expect_err("G86 требует P (v0.18)");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
    assert!(
        err.iter()
            .any(|d| d.message == "Dwell time p word missing with g86"),
        "сообщение зеркалирует rs274: {err:?}"
    );
}

// ===========================================================================
// Циклы — этап B: пеки G83/G73, feed-out G85/G89, лимит развёртки
// ===========================================================================

#[test]
fn unit_cycle_g83_peck_order_full_retract() {
    // R=2, Z=−4, Q=1.5 → 4 пека вниз шагами Q: 0.5, −1.0, −2.5, −4.0;
    // полный отвод на R + подход (клиренс пека 0.01in unit-scaled = 0.254mm
    // в G21 над последней глубиной, ≤ R; SPEC §7.4a v0.18) после каждого
    // промежуточного пека; порядок пиков точный (SPEC §7.4).
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
    let mut approaches: Vec<f64> = Vec::new();
    let mut retract_after_last_feed = f64::NAN;
    let mut cur = 5.0f64;
    for o in &p.ops[1..14] {
        match o {
            Op::Linear { end, .. } => {
                depths.push(to_f64(end)[2]);
                cur = to_f64(end)[2];
            }
            Op::Rapid { end, .. } => {
                let z = to_f64(end)[2];
                if !depths.is_empty() {
                    // после первого пека: отвод на R (z ≥ cur) или подход
                    // вниз (z < cur)
                    if z < cur {
                        approaches.push(z); // подход вниз после отвода на R
                    } else {
                        retract_after_last_feed = z;
                    }
                }
                cur = z;
            }
            _ => {}
        }
    }
    assert_eq!(
        depths,
        vec![0.5, -1.0, -2.5, -4.0],
        "пеки вниз шагами Q (порядок §7.4)"
    );
    // Подходы = последняя глубина + 0.254 (клиренс 0.01in в G21-числах).
    assert_eq!(
        approaches,
        vec![0.754, -0.746, -2.246],
        "подход d+0.254 (§7.4a v0.18: клиренс 0.01in unit-scaled)"
    );
    assert_eq!(retract_after_last_feed, 2.0, "G99: финальный отвод на R");
}

#[test]
fn unit_cycle_g73_chipbreak_single_de_retract() {
    // G73 (SPEC §7.4a v0.18): депеки R−kQ (расписание как G83), после
    // КАЖДОГО промежуточного пека — ОДИН rapid на d+клиренс (0.254 в G21),
    // финальный отвод (G99) на R. R=2, Z=−4, Q=2 → 3 пека: 0.0, −2.0, −4.0;
    // промежуточные отводы: 0.254, −1.746; финал: R=2.0.
    // Модели «de-retract min(R, d+Q) + отдельный подход» больше нет.
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
    // Один rapid d+0.254 после каждого промежуточного пека: 0.254 (над 0.0),
    // −1.746 (над −2.0); финальный отвод (G99) на R=2.0.
    assert_eq!(
        ups,
        vec![0.254, -1.746, 2.0],
        "G73 одиночный de-retract d+клиренс + финальный отвод на R"
    );
}

#[test]
fn unit_cycle_g73_probe_g73_r2_structure() {
    // Структура опов 1:1 с зонд-потоком rs274 `g73_r2` (SPEC §7.4a v0.18):
    // R2 Q2 Z−5 → депеки R−Q = 0/−2/−4/−5; отводы d+0.254 после каждого
    // промежуточного пека; финал на R. Полная цепочка (subs 1..10):
    // traverse(5) → approach R(2) → feed 0 → rapid 0.254 → feed −2 →
    // rapid −1.746 → feed −4 → rapid −3.746 → feed −5 → rapid 2.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G73 X10.0000 Y0.0000 Z-5.0000 R2.0000 Q2.0000 F100.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    // initial rapid + 10 производных + PEND = 12.
    assert_eq!(p.ops.len(), 12);
    let seq: Vec<(char, f64)> = p.ops[1..11]
        .iter()
        .map(|o| match o {
            Op::Rapid { end, sub, .. } => (*sub, 'R', to_f64(end)[2]),
            Op::Linear { end, sub, .. } => (*sub, 'F', to_f64(end)[2]),
            other => panic!("не ход: {other:?}"),
        })
        .map(|(s, k, z)| {
            assert!(s.is_some(), "производный оп без sub");
            (k, z)
        })
        .collect();
    assert_eq!(
        seq,
        vec![
            ('R', 5.0),    // traverse на текущем Z
            ('R', 2.0),    // подход к R
            ('F', 0.0),    // депек 1: R−Q
            ('R', 0.254),  // отвод d+0.254
            ('F', -2.0),   // депек 2: R−2Q
            ('R', -1.746), // отвод d+0.254
            ('F', -4.0),   // депек 3: R−3Q
            ('R', -3.746), // отвод d+0.254
            ('F', -5.0),   // депек 4: z_bottom
            ('R', 2.0),    // финал на R (G99)
        ],
        "1:1 с зонд-потоком g73_r2"
    );
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
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
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
    assert!(
        err.iter().all(|d| d.code == ErrorCode::LimitExceeded),
        "{err:?}"
    );
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
        Op::Rapid {
            end, state, sub, ..
        } => {
            assert_eq!(to_f64(end)[2], 2.0, "rapid out на R");
            assert_eq!(
                state.spindle,
                eid_core::ast::Spindle::Off,
                "шпиндель off на отводе"
            );
            assert_eq!(*sub, Some(5));
        }
        other => panic!("op 5 must be Rapid: {other:?}"),
    }
    // Рестарт: следующий ход несёт прежнее направление и обороты.
    match &p.ops[6] {
        Op::Linear { state, .. } => {
            assert_eq!(
                state.spindle,
                eid_core::ast::Spindle::Cw(eid_core::ast::Num::new("8000".into(), 8000.0))
            );
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
    assert_eq!(
        to_f64(match &p.ops[4] {
            Op::Rapid { end, .. } => end,
            other => panic!("op 4: {other:?}"),
        })[2],
        2.0
    );
    // G0 Z6.
    assert_eq!(
        to_f64(match &p.ops[5] {
            Op::Rapid { end, .. } => end,
            other => panic!("op 5: {other:?}"),
        })[2],
        6.0
    );
    // Цикл 2 (G98): retract → initial Z = 6 (не R=4).
    assert_eq!(
        to_f64(match &p.ops[9] {
            Op::Rapid { end, .. } => end,
            other => panic!("op 9: {other:?}"),
        })[2],
        6.0,
        "G98: отвод на initial Z (6), не на R (4)"
    );
}

#[test]
fn unit_unknown_dollar_line_raw_syntax() {
    // CORPUS #47-класс: неизвестная строка (`$` вне словаря LinuxCNC) —
    // line-local RAW{E_UNSUPPORTED_SYNTAX}, разбор продолжается; payload —
    // байты строки С терминатором (D9 v0.15).
    let src = "G21 G90 G94\n$$\nG0 X1.0000 Y1.0000 Z1.0000\nM2\n";
    let a = LinuxCncAdapter::new();
    let out = a.parse(src, "dollar.ngc").expect("Ok (line-level)");
    let p = out.program;
    assert!(
        matches!(
            &p.ops[0],
            Op::Raw { code: ErrorCode::UnsupportedSyntax, payload_hex, .. }
                if payload_hex.as_str() == "24240A"
        ),
        "payload = `$$\n` = 24240A"
    );
    assert!(
        matches!(&p.ops[1], Op::Rapid { .. }),
        "разбор продолжается после raw"
    );
    let codes: Vec<_> = out.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&ErrorCode::UnsupportedSyntax));
}

#[test]
fn unit_raw_crlf_payload_terminator() {
    // D9 v0.15: CRLF-файл → payload включает \r\n; emit воспроизводит
    // терминатор дословно (не нормализует); reparse — payload равен.
    let src = "G21 G90 G94\r\n$$\r\nG0 X1.0000 Y1.0000 Z1.0000\r\nM2\r\n";
    let a = LinuxCncAdapter::new();
    let p = a.parse(src, "crlf.ngc").expect("parses").program;
    match &p.ops[0] {
        Op::Raw { payload_hex, .. } => {
            assert_eq!(payload_hex, "24240D0A", "`$$\r\n` — зеркально golden #47");
        }
        other => panic!("op 0 must be Raw: {other:?}"),
    }
    let text = a.emit(&p).expect("emit").text;
    assert!(
        text.contains("$$\r\n"),
        "CRLF-терминатор сохранён дословно:\n{text:?}"
    );
    let p2 = LinuxCncAdapter::new()
        .parse(&text, "rt.ngc")
        .expect("reparse")
        .program;
    match (&p.ops[0], &p2.ops[0]) {
        (
            Op::Raw {
                payload_hex: h1, ..
            },
            Op::Raw {
                payload_hex: h2, ..
            },
        ) => {
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
    let p = a.parse(src, "nurbs.ngc").expect("Ok (line-level)").program;
    assert!(
        matches!(
            &p.ops[1],
            Op::Raw { code: ErrorCode::UnsupportedSemantics, payload_hex, .. }
                if payload_hex.starts_with("4735 2E32".replace(' ', "").as_str())
        ),
        "payload = `G5.2\n`-префикс (G5.2)"
    );
    assert!(
        matches!(&p.ops[2], Op::Linear { .. }),
        "разбор продолжается"
    );
}

// ===========================================================================
// spec_cov — ruling SPEC v0.19: NURBS/сплайны (G5/G5.1/G5.2/G5.3) →
// line-level Raw{E_UNSUPPORTED_SEMANTICS} ВСЕЙ строкой, включая строки-
// продолжения (контрольные точки P/X/Y без G-кода). Полярность — зеркально
// eid-fanuc G06.2 NURBS (Raw{SEM}). Раньше строки-продолжения падали в
// жёсткий Err «P-слово без потребителя» — nurbs-файлы Tier B.
// ===========================================================================

#[test]
fn spec_cov_lcnc_nurbs_continuation_points_raw_semantics() {
    // Ruling golden: NURBS-блок G5.2 P1 L3 / контрольные точки / G5.3 —
    // ВСЕ 6 строк → Ok + Raw{SEM}; payload — байты строки с терминатором
    // (same-dialect verbatim, D9); после G5.3 модальный G0 продолжается
    // обычными ходами (bare-координаты — снова Rapid, НЕ Raw).
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "F100\n",
        "G5.2 P1 L3\n",
        "X0 Y1 P1\n",
        "X2 Y2 P1\n",
        "X2 Y0 P1\n",
        "X0 Y0 P2\n",
        "G5.3\n",
        "G0 X0 Y1\n",
        "X2 Y2\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let out = a.parse(src, "nurbs.ngc").expect("ruling: Ok + Raw, НЕ Err");
    let p = out.program;
    // Rapid + 6 Raw (G5.2, 4 контрольные точки, G5.3) + 2 Rapid + PEND.
    assert_eq!(p.ops.len(), 10, "{:?}", p.ops);
    for (k, (label, expect_prefix)) in [
        ("G5.2 P1 L3", "47352E32205031204C33"),
        ("X0 Y1 P1", "5830205931205031"),
        ("X2 Y2 P1", "5832205932205031"),
        ("X2 Y0 P1", "5832205930205031"),
        ("X0 Y0 P2", "5830205930205032"),
        ("G5.3", "47352E33"),
    ]
    .into_iter()
    .enumerate()
    {
        let op = k + 1; // ops[0] — якорный Rapid
        match &p.ops[op] {
            Op::Raw {
                code, payload_hex, ..
            } => {
                assert_eq!(*code, ErrorCode::UnsupportedSemantics, "строка `{label}`");
                assert!(
                    payload_hex.starts_with(expect_prefix),
                    "`{label}`: payload = `{expect_prefix}…`, фактически {payload_hex}"
                );
            }
            other => panic!("op {op} должен быть Raw (`{label}`): {other:?}"),
        }
    }
    assert!(matches!(&p.ops[7], Op::Rapid { .. }), "после G5.3 — ходы");
    assert!(matches!(&p.ops[8], Op::Rapid { .. }), "модальный G0 жив");
    assert!(matches!(&p.ops.last(), Some(Op::ProgramEnd { .. })));
    // Диагностики Ok-пути: 6 × Raw{SEM}.
    let sem = out
        .diagnostics
        .iter()
        .filter(|d| d.code == ErrorCode::UnsupportedSemantics)
        .count();
    assert_eq!(sem, 6, "6 диагностик UnsupportedSemantics");
    // Reason — ruling-формулировка, зеркально fanuc G06.2 NURBS.
    assert!(
        out.diagnostics.iter().any(|d| {
            d.message.contains("NURBS")
                && d.message.contains("этап 12")
                && d.message.contains("Raw-строки")
        }),
        "reason: NURBS/этап 12/Raw-строки: {:?}",
        out.diagnostics.first().map(|d| &d.message)
    );
}

#[test]
fn spec_cov_lcnc_nurbs_g5_g51_open_lines_raw_and_mode_clear() {
    // G5 (кубический сплайн) и G5.1 (квадратичный) — та же диспозиция
    // Raw{SEM}; строки-продолжения (bare X/Y в сплайн-режиме) — Raw; явный
    // motion-G (G1) РАЗРЫВАЕТ модальный сплайн → следующие bare-координаты
    // снова обычные ходы (регресс «режим не прилип навсегда»).
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "F100\n",
        "G5 X0 Y1\n",
        "X2 Y2\n",
        "G1 X5.0000 Y5.0000\n",
        "X7.0000 Y7.0000\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let p = a.parse(src, "g5.ngc").expect("Ok + Raw").program;
    // Rapid(якорь) + Raw(G5) + Raw(X2 Y2) + Linear + Linear + PEND.
    assert_eq!(p.ops.len(), 6, "{:?}", p.ops);
    let g5_hex = match &p.ops[1] {
        Op::Raw {
            code, payload_hex, ..
        } => {
            assert_eq!(
                code,
                &ErrorCode::UnsupportedSemantics,
                "G5-строка → Raw SEM"
            );
            payload_hex.as_str()
        }
        other => panic!("op 1 должен быть Raw (G5): {other:?}"),
    };
    assert!(g5_hex.starts_with("473520"), "payload `G5 …`: {g5_hex}");
    let cont_hex = match &p.ops[2] {
        Op::Raw {
            code, payload_hex, ..
        } => {
            assert_eq!(
                code,
                &ErrorCode::UnsupportedSemantics,
                "продолжение X2 Y2 → Raw SEM"
            );
            payload_hex.as_str()
        }
        other => panic!("op 2 должен быть Raw (продолжение): {other:?}"),
    };
    assert!(
        cont_hex.starts_with("58322059"),
        "payload `X2 Y2`: {cont_hex}"
    );
    assert!(
        matches!(&p.ops[3], Op::Linear { .. }),
        "G1 разрывает сплайн"
    );
    assert!(
        matches!(&p.ops[4], Op::Linear { .. }),
        "bare X/Y — модальный G1"
    );

    // G5.1 — квадратичная форма.
    let src51 = "G21 G90 G94\nG0 X0.0000 Y0.0000\nF100\nG5.1 X0 Y1\nX2 Y2\nM2\n";
    let p51 = LinuxCncAdapter::new()
        .parse(src51, "g51.ngc")
        .expect("G5.1: Ok + Raw")
        .program;
    assert!(matches!(
        &p51.ops[1],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    assert!(matches!(
        &p51.ops[2],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
}

#[test]
fn spec_cov_lcnc_nurbs_q_word_absorbed_in_raw_line() {
    // Q-слово G5.2 (счётчик точек NURBS) — часть Raw-строки: ни на строке
    // открытия, ни на строке-продолжении оно НЕ даёт
    // «Q-слово без пекерующего цикла» (жёсткий Err).
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "F100\n",
        "G5.2 P1 Q6 X10.0000 Y10.0000\n",
        "X20.0000 Y20.0000 Q1\n",
        "G5.3\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let p = a
        .parse(src, "nurbs_q.ngc")
        .expect("Q в G5.x-строках — Raw")
        .program;
    // Rapid + 3 Raw (G5.2/Q / продолжение с Q / G5.3) + PEND.
    assert_eq!(p.ops.len(), 5, "{:?}", p.ops);
    assert!(matches!(
        &p.ops[1],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    assert!(matches!(
        &p.ops[2],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
    assert!(matches!(
        &p.ops[3],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
}

#[test]
fn spec_cov_lcnc_p_word_without_consumer_still_err() {
    // Регресс ruling: фикс НЕ маскирует настоящие опечатки. P-слово БЕЗ
    // G5.x-контекста — по-прежнему жёсткий Err E_INVALID_INPUT:
    //  (a) P в случайном кадре без потребителя;
    //  (b) P после ЗАКРЫТИЯ NURBS-блока (G5.3) — режим снят, проверка жива;
    //  (c) Q-слово без цикла — по-прежнему Err;
    //  (d) L-слово без G10/цикла — по-прежнему Err.
    // Легальные P (G4/G10/G64/цикл-dwell) покрыты векторами выше/other tests.
    let a = LinuxCncAdapter::new();
    for (src, ctx) in [
        (
            "G21 G90\nG0 X1.0000 Y1.0000\nP5\nM2\n",
            "P в случайном кадре",
        ),
        (
            "G21 G90\nG0 X1.0000 Y1.0000\nG5.2 P1 L2\nX0 Y1 P1\nG5.3\nP9\nM2\n",
            "P после G5.3",
        ),
        ("G21 G90\nG0 X1.0000 Y1.0000\nQ3\nM2\n", "Q без цикла"),
        ("G21 G90\nG0 X1.0000 Y1.0000\nL4\nM2\n", "L без G10/цикла"),
    ] {
        let err = a.parse(src, "regress.ngc").expect_err(ctx);
        assert!(
            err.iter().all(|d| d.code == ErrorCode::InvalidInput),
            "{ctx}: {err:?}"
        );
    }
    // Сообщение «P-слово без потребителя» сохранено дословно.
    let err = a
        .parse("G21 G90\nG0 X1.0000 Y1.0000\nP5\nM2\n", "regress.ngc")
        .expect_err("P без потребителя");
    assert!(
        err[0].message.contains("P-слово без потребителя"),
        "{}",
        err[0].message
    );
    // И контрольная точка P-слово на НЕ-сплайн строке при активном... нет
    // режима: bare X/Y после обычного G0 — это ход, не Raw (фикс не расширяет
    // диспозицию на файлы без G5.x).
    let p = a
        .parse(
            "G21 G90\nG0 X1.0000 Y1.0000\nX2.0000 Y2.0000\nM2\n",
            "plain.ngc",
        )
        .expect("без G5.x bare-координаты — ходы, как раньше")
        .program;
    assert_eq!(p.ops.len(), 3); // Rapid + Rapid + PEND, Raw нет
    assert!(!p.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
}

#[test]
fn spec_cov_lcnc_nurbs_tierb_corpus_files_ok_raw() {
    // Приёмочный критерий ruling (SPEC v0.19): nurbs-файлы Tier B local —
    // Ok + Raw{SEM} (parse проходит, same-dialect verbatim), НЕ Err.
    // 4 файла Curva_* содержат `#1=…` — жёсткий лексический отказ SPEC §1.1
    // (вне скоупа ruling: `#`-параметры — E_RUNTIME_STATE во ВСЕХ диалектах).
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tier_b/local");
    const FIXED: &[&str] = &[
        "lcnc_nc_files_nurbs_G5_butterfly-xy.ngc",
        "lcnc_nc_files_nurbs_G5_butterfly-yz.ngc",
        "lcnc_nc_files_nurbs_G5_butterfly-zx.ngc",
        "lcnc_nc_files_nurbs_G5_leto-xy.ngc",
        "lcnc_nc_files_nurbs_G5_nurbs-xy.ngc",
        "lcnc_nc_files_nurbs_G5_nurbs-yz.ngc",
        "lcnc_nc_files_nurbs_G5_nurbs-zx.ngc",
        "lcnc_nc_files_nurbs_G5_trident-xy.ngc",
        "lcnc_nc_files_nurbs_G5_trident-yz.ngc",
        "lcnc_nc_files_nurbs_G5_trident-zx.ngc",
        "lcnc_nc_files_nurbs_G5_trident-xy-yz-zx.ngc",
    ];
    let a = LinuxCncAdapter::new();
    for name in FIXED {
        let src = std::fs::read_to_string(dir.join(name)).expect("corpus file readable");
        let out = a
            .parse(&src, name)
            .unwrap_or_else(|e| panic!("{name}: ожидался Ok + Raw, фактически Err: {e:?}"));
        let raw_sem = out
            .program
            .ops
            .iter()
            .filter(|o| {
                matches!(
                    o,
                    Op::Raw {
                        code: ErrorCode::UnsupportedSemantics,
                        ..
                    }
                )
            })
            .count();
        assert!(raw_sem > 0, "{name}: ожидались Raw{{SEM}}-опы");
        let sem_diags = out
            .diagnostics
            .iter()
            .filter(|d| d.code == ErrorCode::UnsupportedSemantics)
            .count();
        assert_eq!(
            raw_sem, sem_diags,
            "{name}: диагностики Ok-пути ⊇ {{код}} (CORPUS §1.2)"
        );
    }
    // Curva_* (v0.21, инвентаризация §2): dead-assignment `#1=3` — та же
    // line-level Raw{SYNTAX} диспозиция (drop+notice отвергнут: Raw хранит
    // байты честно) — Ok+Raw вместо Err, NURBS-Raw{SEM} строки целы.
    for name in [
        "lcnc_nc_files_nurbs_G5_Curva_Chitarra__1_3_G5.ngc",
        "lcnc_nc_files_nurbs_G5_Curva_Delfino__1_1_G5.ngc",
        "lcnc_nc_files_nurbs_G5_Curva_Mazatrol__1_1_G5.ngc",
        "lcnc_nc_files_nurbs_G5_Curva_Stella__1_1_G5.ngc",
    ] {
        let src = std::fs::read_to_string(dir.join(name)).expect("corpus file readable");
        let out = a
            .parse(&src, name)
            .unwrap_or_else(|e| panic!("{name}: ожидался Ok + Raw, Err: {e:?}"));
        let raw_sem = out
            .program
            .ops
            .iter()
            .filter(|o| {
                matches!(
                    o,
                    Op::Raw {
                        code: ErrorCode::UnsupportedSemantics,
                        ..
                    }
                )
            })
            .count();
        assert!(raw_sem > 0, "{name}: NURBS Raw{{SEM}}-опы целы");
        let raw_syn = out
            .program
            .ops
            .iter()
            .filter(|o| {
                matches!(
                    o,
                    Op::Raw {
                        code: ErrorCode::UnsupportedSyntax,
                        ..
                    }
                )
            })
            .count();
        assert!(
            raw_syn >= 1,
            "{name}: `#1=3` (dead assignment) → Raw{{SYNTAX}} (зеркало fanuc)"
        );
    }
}

#[test]
fn spec_cov_lcnc_nurbs_same_dialect_verbatim_roundtrip() {
    // R1-класс на NURBS: same-dialect emit → payload байт-в-байт (включая
    // терминаторы), reparse ≡ (semantic_diff 0).
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G5.2 P1 L3\n",
        "X0 Y1 P1\n",
        "G5.3\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let p = a.parse(src, "rt.ngc").expect("Ok").program;
    let text = a.emit(&p).expect("emit").text;
    for needle in ["G5.2 P1 L3\n", "X0 Y1 P1\n", "G5.3\n"] {
        assert!(
            text.contains(needle),
            "verbatim потерян: {needle:?} в {text:?}"
        );
    }
    let p2 = LinuxCncAdapter::new()
        .parse(&text, "rt2.ngc")
        .expect("reparse")
        .program;
    let raws1: Vec<_> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Raw { payload_hex, .. } => Some(payload_hex.clone()),
            _ => None,
        })
        .collect();
    let raws2: Vec<_> = p2
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Raw { payload_hex, .. } => Some(payload_hex.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(raws1, raws2, "payload байт-в-байт (R1-text-класс)");
    let d = semantic_diff(&p, &p2, &DiffConfig::default());
    assert!(d.is_empty(), "{d:?}");
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
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
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
        Op::Arc {
            center,
            sweep_deg,
            dir,
            ..
        } => {
            assert_eq!(
                (center.x.value, center.y.value),
                (0.0, 0.0),
                "G90.1 абсолютный центр"
            );
            assert_eq!(sweep_deg.value, 90.0);
            assert_eq!(*dir, eid_core::ast::Dir::Ccw);
        }
        other => panic!("op 1: {other:?}"),
    }
    match &p.ops[2] {
        Op::Arc {
            start: StartPoint::Known(s),
            end,
            center,
            sweep_deg,
            dir,
            ..
        } => {
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
        Op::Arc {
            center,
            sweep_deg,
            dir,
            ..
        } => {
            assert_eq!(
                (center.x.value, center.y.value, center.z.value),
                (0.0, 0.0, 5.0)
            );
            assert_eq!(sweep_deg.value, 90.0);
            assert_eq!(*dir, eid_core::ast::Dir::Ccw);
        }
        other => panic!("op 1: {other:?}"),
    }
    let b = parse_str(g18_inc).expect("G18 G91.1");
    match &b.ops[1] {
        Op::Arc { center, .. } => {
            assert_eq!(
                (center.x.value, center.y.value, center.z.value),
                (0.0, 0.0, 5.0)
            );
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
        Op::Arc {
            center,
            normal,
            sweep_deg,
            dir,
            ..
        } => {
            // G19: center.x = start.x (normal), center.y = J0, center.z = K5
            // → center (0,0,5); ρ = 5; G2 cw из (0,0) в (5,5) вокруг (0,5):
            // 270° (длинный путь — легален, sweep ∈ (0°, 360°]).
            assert_eq!(
                (center.x.value, center.y.value, center.z.value),
                (0.0, 0.0, 5.0)
            );
            assert_eq!(
                (normal.x.value, normal.y.value, normal.z.value),
                (1.0, 0.0, 0.0)
            );
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
        "G10 L2 P1 X1.0000 Y2.0000 Z3.0000\n",
        "G0 X1.0000 Y1.0000 Z1.0000\n",
        "G53 G0 X2.0000 Y3.0000 Z4.0000\n",
        "G53 G0 X6.0000 Y3.0000 Z4.0000\n",
        "G0 X8.0000 Y4.0000 Z4.0000\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    // Первый G53-ход (2,3,4)→(2,3,4) — нулевой в machine-фрейме →
    // отбрасывается с notice (kill-поверхность знаковых мутантов подхода).
    // ops: [WcsDefine, G53-rapid(zero→drop), G53-rapid(цепочка), wcs-возврат, PEND].
    assert!(
        matches!(&p.ops[2], Op::Rapid { start: StartPoint::Known(s), end, .. }
        if to_f64(s) == [2.0, 3.0, 4.0] && to_f64(end) == [6.0, 3.0, 4.0]),
        "machine→machine: цепочка exact (§2 v0.10)"
    );
    // (d): возврат в WCS — reading = M − O_P1 = (6,3,4) − (1,2,3) = (5,1,1).
    // Ненулевой origin убивает знаковые/осевые мутанты в reading = M − O.
    assert!(
        matches!(&p.ops[3], Op::Rapid { start: StartPoint::Known(s), end, .. }
        if to_f64(s) == [5.0, 1.0, 1.0] && to_f64(end) == [8.0, 4.0, 4.0]),
        "возврат в WCS: reading = M − O (§2 (d), ненулевой origin)"
    );
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
        err.iter()
            .all(|d| matches!(d.code, ErrorCode::RuntimeState | ErrorCode::InvalidInput)),
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
    assert!(
        err.iter().all(|d| d.code == ErrorCode::RuntimeState),
        "{err:?}"
    );
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

    // Parse: line-level Raw (O-code) — диагностика stage=Parse, разбор идёт.
    let a = LinuxCncAdapter::new();
    let out = a
        .parse(
            "G21 G90 G94
O100
G0 X1.0000 Y1.0000 Z1.0000
M2
",
            "o.ngc",
        )
        .expect("O-code: Ok + Raw");
    let p = out.program;
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    let d1 = out.diagnostics;
    assert!(d1
        .iter()
        .all(|d| d.stage == Stage::Parse && d.code == ErrorCode::UnsupportedSyntax));

    // Parse: структурный отказ кадра (G53 без G0/G1) — stage=Parse.
    let err = parse_str("G21 G90 G94\nG53 G2 X5.0000 Y5.0000 I5.0000 J0.0000 F100.0\nM2\n")
        .expect_err("G53+дуга");
    assert!(err.iter().all(|d| d.stage == Stage::Parse));

    // Verify: linear без feed (инвариант §3) — stage=Verify.
    let err =
        parse_str("G21 G90 G94\nG0 X1.0000 Y1.0000 Z1.0000\nG1 X2.0000 Y1.0000 Z1.0000\nM2\n")
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
    let err = LinuxCncAdapter::new()
        .emit(&p)
        .expect_err("не-осевой normal");
    // eid-core validate (Canonicalize/Verify у ядра — нас интересует факт
    // отказа до прохода эмиссии) или таблица легальности (Emit).
    for d in &err {
        assert_eq!(d.code, ErrorCode::InvalidInput, "{d:?}");
    }
}

// ===========================================================================
// Метаморфические инварианты (несемантичность): comment / whitespace /
// n-word — parse(X) ≡ parse(без X). Сравнение — semantic_diff (EQUIVALENCE
// §3: src/sub — provenance, вне эквивалентности; корень «10 нарушений»
// фаззера B — PartialEq на всём Program, включающий SrcSpan — артефакт
// фаззера, верну оркестратору; парсер инварианты СОБЛЮДАЕТ).
// ===========================================================================

fn assert_meta_invariant(base_src: &str, variant_src: &str, tag: &str) {
    let a = LinuxCncAdapter::new()
        .parse(base_src, tag)
        .expect("base parses")
        .program;
    let b = LinuxCncAdapter::new()
        .parse(variant_src, tag)
        .expect("variant parses")
        .program;
    let d = eid_harness::diff::semantic_diff(&a, &b, &DiffConfig::default());
    assert!(d.is_empty(), "{tag}: semantic divergences: {d:?}");
}

#[test]
fn metamorphic_comment_invariance() {
    // (а) содержимое комментария не протекает; (б) конец строки; (в) `;` ≡ `()`.
    let base = concat!(
        "G21 G90 G94 G17 G91.1\n",
        "M3 S10000\n",
        "G0 X0.0000 Y0.0000 Z5.0000 (подвод)\n",
        "G1 X10.0000 F150.0 ; проход 1\n",
        "M2\n",
    );
    let no_comments = concat!(
        "G21 G90 G94 G17 G91.1\n",
        "M3 S10000\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G1 X10.0000 F150.0\n",
        "M2\n",
    );
    assert_meta_invariant(base, no_comments, "comment: stripped ≡ без комментариев");
    // (а) «жадное» содержимое, похожее на слова: не протекает в токены.
    let greedy = concat!(
        "(G1 X99 Y99 Z99 начало)\n",
        "G21 G90 G94 G17 G91.1\n",
        "M3 S10000\n",
        "(конец; G1 X98)\n",
        "G0 X0.0000 Y0.0000 Z5.0000 (подвод)\n",
        "G1 X10.0000 F150.0 ; проход 1 X77 Y77\n",
        "M2\n",
    );
    assert_meta_invariant(
        greedy,
        no_comments,
        "comment: жадное содержимое не протекает",
    );
}

#[test]
fn metamorphic_whitespace_invariance() {
    // Кратность пробелов/табов между словами не меняет токенизацию.
    let base = "G21 G90 G94 G17 G91.1\nG0 X1.0000 Y2.0000 Z3.0000\nG1 X-4.0000 F100.0\nM2\n";
    let doubled =
        "G21  G90  G94  G17  G91.1\nG0   X1.0000   Y2.0000   Z3.0000\nG1\tX-4.0000  F100.0\nM2\n";
    assert_meta_invariant(base, doubled, "whitespace: кратность/табы");
    let tight = "G21G90G94G17G91.1\nG0X1.0000Y2.0000Z3.0000\nG1X-4.0000F100.0\nM2\n";
    assert_meta_invariant(base, tight, "whitespace: без пробелов вовсе (RS274-стиль)");
}

#[test]
fn metamorphic_n_word_invariance() {
    // N-слова — IGNORED_SAFE: вставка/удаление N-слов не меняет AST.
    let base = "G21 G90 G94 G17 G91.1\nM3 S8000\nN10 G0 X1.0000 Y1.0000 Z1.0000\nN20 G1 X5.0000 F100.0\nM2\n";
    let no_n =
        "G21 G90 G94 G17 G91.1\nM3 S8000\nG0 X1.0000 Y1.0000 Z1.0000\nG1 X5.0000 F100.0\nM2\n";
    assert_meta_invariant(base, no_n, "n-word: без N ≡ с N");
    let other_n = "N1 G21 G90 G94 G17 G91.1\nM3 S8000\nN2 G0 X1.0000 Y1.0000 Z1.0000\nG1 N99 X5.0000 F100.0\nM2\n";
    assert_meta_invariant(other_n, no_n, "n-word: другие номера ≡ без N");
}

// ===========================================================================
// Mutation-kill усиление (кластеры 1107-1122/1339/1370/1426/1586, emit 45)
// ===========================================================================

#[test]
fn mutation_g93_helical_arc_feed_formula() {
    // Кластер 1339: G93-дуга feed = len·F, len = sqrt((sweep·ρ)² + pitch²).
    // R=5 (I-центр (0,0) от (5,0)), sweep 90°, pitch 2, F=2:
    // len = sqrt((5·π/2)² + 2²) = 8.104754…; feed = 2·len = 16.209508…
    // Мутанты «без pitch»/«без sweep» дают 15.708/16.0 — убиваются ассертом.
    let src = concat!(
        "G21 G90 G93 G17\n",
        "G0 X5.0000 Y0.0000 Z0.0000\n",
        "G3 X0.0000 Y5.0000 Z2.0000 I-5.0000 J0.0000 F2.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    match &p.ops[1] {
        Op::Arc {
            state,
            sweep_deg,
            pitch,
            ..
        } => {
            assert_eq!(sweep_deg.value, 90.0);
            assert_eq!(pitch.value, 2.0);
            let f = state.feed.as_ref().unwrap().value;
            // π/2·5 = 7.853982; +pitch² → sqrt(65.685019) = 8.104754; ×F2 = 16.209508
            assert!(
                (f - 16.2095).abs() < 0.001,
                "G93-дуга: feed = len·F = 16.2095, got {f}"
            );
            // Мутант «pitch выброшен» дал бы 15.708 — убит строгой границей:
            assert!(f > 16.0, "pitch вносит вклад (мутант «без pitch» убит)");
        }
        other => panic!("op 1 must be Arc: {other:?}"),
    }
}

#[test]
fn mutation_approach_clearance_inch() {
    // Кластер 1426 + SPEC §7.4a v0.18: approach_clearance(In) = 0.01
    // (ровно 0.0100 в inch-числах; прежде 0.05 — зондом rs274 g83_inch
    // опровергнуто). G20 G83: R=0, Z=-1.0, Q=0.4 → пеки -0.4/-0.8/-1.0;
    // после 1-го пека подход rapid на -0.4 + 0.01 = -0.39.
    let src = concat!(
        "G20 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z1.0000\n",
        "G99 G83 X1.0000 Y1.0000 Z-1.0000 R0.0000 Q0.4000 F50.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    // Подход после 1-го пека: rapid из текущего z=0.0 вниз на -0.39
    // (= последняя глубина -0.4 + clearance 0.0100 inch).
    let mut approach_z = None;
    for o in &p.ops[2..] {
        if let Op::Rapid {
            start: StartPoint::Known(s),
            end,
            ..
        } = o
        {
            let (sz, ez) = (to_f64(s)[2], to_f64(end)[2]);
            if sz.abs() < 1e-9 && (ez + 0.39).abs() < 1e-9 {
                approach_z = Some(ez); // подход вниз после первого пека
            }
        }
    }
    assert_eq!(
        approach_z,
        Some(-0.39),
        "approach(In) = ровно 0.0100 над последней глубиной (§7.4a v0.18)"
    );
}

#[test]
fn mutation_unroll_est_boundary_pass_and_fail() {
    // Кластер 1586: est = 2 + 3n + 1; граница est = MAX(10000):
    // Z=-3332 → n=3332 → est = 9999 ≤ 10000 → Ok;
    // Z=-3333 → n=3333 → est = 10002 > 10000 → Err LIMIT_EXCEEDED.
    let ok_src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z0.0000\n",
        "G99 G83 X1.0000 Y1.0000 Z-3332.0000 R0.0000 Q1.0000 F100.0\n",
        "M2\n",
    );
    assert!(
        parse_str(ok_src).is_ok(),
        "est = 9999 ≤ 10000 — на границе изнутри"
    );
    let bad_src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z0.0000\n",
        "G99 G83 X1.0000 Y1.0000 Z-3333.0000 R0.0000 Q1.0000 F100.0\n",
        "M2\n",
    );
    let err = parse_str(bad_src).expect_err("est = 10002 > 10000");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::LimitExceeded),
        "{err:?}"
    );
}

#[test]
fn mutation_l_word_boundary_pass_and_fail() {
    // L=10000 → ровно лимит, L=10001 → E_LIMIT_EXCEEDED (COVERAGE §1.2).
    let ok = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G81 X1.0000 Y1.0000 Z-1.0000 R0.0000 F100.0 L10000\n",
        "M2\n",
    );
    let p = LinuxCncAdapter::new()
        .parse(ok, "l_ok.ngc")
        .expect("L=10000 — ровно лимит")
        .program;
    assert!(p.ops.len() >= 3);
    let over = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G81 X1.0000 Y1.0000 Z-1.0000 R0.0000 F100.0 L10001\n",
        "M2\n",
    );
    let err = parse_str(over).expect_err("L=10001 > 10000");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::LimitExceeded),
        "{err:?}"
    );
}

#[test]
fn spec_cov_lcnc_partial_end_first_move_inherits() {
    // SPEC §2/§10.6 v0.11: частичный end на старте (break (a)) —
    // отсутствующие оси наследуют дефолт-0. G0 X10 → end (10,0,0).
    let p = parse_str("G21 G90 G94 G17\nG0 X10.0000\nM2\n").expect("parses");
    match &p.ops[0] {
        Op::Rapid {
            start: StartPoint::Unknown,
            end,
            ..
        } => {
            assert_eq!(
                to_f64(end),
                [10.0, 0.0, 0.0],
                "частичный end наследует дефолт-0"
            );
        }
        other => panic!("op 0: {other:?}"),
    }
}

// ===========================================================================
// Аудит #33 (SPEC §8.5 правило 6, v0.17): модальная абсорбция на RAW-строках
// ===========================================================================

#[test]
fn audit33_g91_absorbed_on_raw_line_incremental_continues() {
    // Пробой: `G91 G28 Z0` — G28 ушёл в Raw, G91 терялся → следующий
    // `G0 X10 Y10` молча парсился АБСОЛЮТНЫМ. После фикса G91 абсорбирован:
    // end = стартовая точка + (10,10) инкрементально.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X5.0000 Y5.0000 Z2.0000\n",
        "G91 G28 Z0\n",
        "G0 X10.0000 Y10.0000\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses Ok (G28 — line-level raw)");
    assert!(
        p.ops.iter().any(|o| matches!(o, Op::Raw { .. })),
        "строка G28 — RAW-оп"
    );
    let mut rapids = p.ops.iter().filter_map(|o| match o {
        Op::Rapid { start, end, .. } => Some((*start == StartPoint::Unknown, to_f64(end))),
        _ => None,
    });
    let (unknown0, end0) = rapids.next().expect("rapid #0");
    assert!(
        unknown0 && end0 == [5.0, 5.0, 2.0],
        "первый rapid: {end0:?}"
    );
    let (_, end1) = rapids.next().expect("rapid после RAW-строки");
    assert_eq!(
        end1,
        [15.0, 15.0, 2.0],
        "G91 абсорбирован: end = (5,5,2) + (10,10) инкрементально (было (10,10,0) — тихое искажение)"
    );
    assert!(rapids.next().is_none());
}

#[test]
fn audit33_units_absorbed_on_raw_line_g20_g43() {
    // `G20 G43 H2 X5.` — G43 → RAW, но G20 абсорбирован: units = inch,
    // следующая строка читается в inch (до фикса units терялись → mm).
    let adapter = LinuxCncAdapter::new();
    let src = concat!("G20 G43 H2 X5.\n", "G1 X1.0 F10.0\n", "M2\n",);
    let p = adapter
        .parse(src, "a33_units.ngc")
        .expect("parses Ok")
        .program;
    assert_eq!(
        p.units,
        eid_core::ast::Units::In,
        "G20 абсорбирован с RAW-строки"
    );
    assert!(p.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
}

#[test]
fn audit33_feed_spindle_absorbed_on_raw_line() {
    // F/S/M3 на RAW-строке (`G43 H2 F240.0 M3 S6000`) абсорбируются:
    // следующий linear несёт feed=240 и шпиндель Cw(6000) без своих слов.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G43 H2 F240.0 M3 S6000\n",
        "G1 X10.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses Ok");
    let lin = p
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Linear { state, .. } => Some(state.clone()),
            _ => None,
        })
        .expect("linear op");
    assert_eq!(
        lin.feed.as_ref().map(|f| f.value),
        Some(240.0),
        "F240 абсорбирован"
    );
    assert!(
        matches!(&lin.spindle, eid_core::ast::Spindle::Cw(n) if n.value == 6000.0),
        "M3 S6000 абсорбирован: {lin:?}"
    );
}

#[test]
fn audit33_w_modal_on_raw_notice_on_motion_g() {
    // Motion-G на RAW-строке: st.motion НЕ обновляется + notice W_MODAL_ON_RAW.
    // `G0 X20.0 G43 H2` — G43 → RAW; модальный motion остаётся G1 (строка ниже),
    // G0 этой строки НЕ становится модальным.
    let adapter = LinuxCncAdapter::new();
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G1 X5.0 F100.0\n",
        "G0 X20.0 G43 H2\n",
        "G1 X8.0\n",
        "M2\n",
    );
    let out = adapter.parse(src, "a33_notice.ngc").expect("parses Ok");
    let p = out.program;
    assert!(p.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
    let diags = out.diagnostics;
    assert!(
        diags.iter().any(|d| d.message.contains("W_MODAL_ON_RAW")
            && d.message.contains("модальное состояние абсорбировано")),
        "ожидалась W_MODAL_ON_RAW-диагностика, получено: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn audit33_g92_preset_absorbed_on_raw_line() {
    // G92 на RAW-строке абсорбируется: frame_preset задан, следующий ход
    // связывается с якоря пресетки (start known).
    let src = concat!(
        "G21 G90 G17\n",
        "G92 X0.0 Y0.0 Z5.0 G28\n",
        "G0 X1.0 Y1.0 Z6.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses Ok");
    let pr = p
        .frame_preset
        .as_ref()
        .expect("G92 абсорбирован: пресетка задана");
    assert_eq!(to_f64(&pr.values), [0.0, 0.0, 5.0]);
    assert!(p.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
    match &p
        .ops
        .iter()
        .find(|o| matches!(o, Op::Rapid { .. }))
        .expect("rapid")
    {
        Op::Rapid {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_eq!(to_f64(s), [0.0, 0.0, 5.0], "start = якорь пресетки");
            assert_eq!(to_f64(end), [1.0, 1.0, 6.0]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn audit33_g10_l2_absorbed_on_raw_line_emits_wcs_define() {
    // G10 L2 на RAW-строке: состояние origin абсорбировано + WcsDefine-оп
    // эмитится (ветка raw попадает только на motion/ops-построение), строка
    // дополнительно сохранена RAW (fidelity байт).
    let src = concat!("G21 G90 G17\n", "G10 L2 P2 X1.0 Y2.0 Z3.0 G28\n", "M2\n",);
    let p = parse_str(src).expect("parses Ok");
    assert!(
        p.ops
            .iter()
            .any(|o| matches!(o, Op::WcsDefine { p: 2, .. })),
        "WcsDefine(p=2) эмитится с RAW-строки"
    );
    assert!(p.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
}

// ===========================================================================
// Аудит #29/#30: try_from для T/L, домен P до приведения
// ===========================================================================

#[test]
fn audit29_t_word_out_of_range_is_err_not_silent_t0() {
    // T4294967296 ранее молча давал T0 (as-cast с отбрасыванием битов) —
    // теперь E_INVALID_INPUT «value out of range».
    let err = parse_str("G21 G90\nT4294967296 M6\nM2\n").expect_err("T > u32::MAX");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
    assert!(
        err.iter().any(|d| d.message.contains("out of range")),
        "{err:?}"
    );
    // Граница домена: T4294967295 — легален (уходит в предвыбор/Raw, но не T0).
    let a = LinuxCncAdapter::new();
    let p = a
        .parse("G21 G90\nT4294967295\nM2\n", "t_max.ngc")
        .expect("T = u32::MAX легален")
        .program;
    assert!(
        p.ops.iter().any(|o| matches!(o, Op::Raw { .. })),
        "одиночное T — Raw"
    );
}

#[test]
fn audit29_l_word_out_of_range_is_err_not_silent_l0() {
    // L4294967296 ранее молча давал L0 — теперь E_INVALID_INPUT.
    let err = parse_str(concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G81 X1.0000 Y1.0000 Z-1.0000 R0.0000 F100.0 L4294967296\n",
        "M2\n",
    ))
    .expect_err("L > u32::MAX");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
    assert!(
        err.iter().any(|d| d.message.contains("out of range")),
        "{err:?}"
    );
}

#[test]
fn audit30_g10_p_fractional_is_err_not_silent_p1() {
    // P1.9 ранее молча давал P1 (as-cast) — номер системы искажён; теперь
    // E_INVALID_INPUT (домен проверяется ДО приведения).
    for p_word in ["P1.9", "P2.5", "P0.5"] {
        let src = format!("G21 G90 G17\nG10 L2 {p_word} X1.0 Y2.0 Z3.0\nM2\n");
        let err = parse_str(&src)
            .expect_err(&format!("{p_word} — дробный P обязан дать E_INVALID_INPUT"));
        assert!(
            err.iter().all(|d| d.code == ErrorCode::InvalidInput),
            "{p_word}: {err:?}"
        );
    }
    let err = parse_str("G21 G90 G17\nG10 L20 P1.9 X1.0 Y2.0 Z3.0\nM2\n").expect_err("L20 P1.9");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
}

#[test]
fn audit30_g10_p_zero_and_ten_out_of_domain() {
    // P0 и P10 — вне домена 1..9 (D7): E_INVALID_INPUT (раньше P0 давал
    // p=0 → доменная ошибка и так; теперь единая pre-cast проверка).
    let err = parse_str("G21 G90 G17\nG10 L2 P0 X1.0 Y2.0 Z3.0\nM2\n").expect_err("P0");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
    let err = parse_str("G21 G90 G17\nG10 L2 P10 X1.0 Y2.0 Z3.0\nM2\n").expect_err("P10");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
    // Легальная граница: P1 и P9, в т.ч. verbatim-форма P1.0000 (fract == 0).
    assert!(parse_str("G21 G90 G17\nG10 L2 P1 X1.0 Y2.0 Z3.0\nM2\n").is_ok());
    assert!(parse_str("G21 G90 G17\nG10 L2 P9 X1.0 Y2.0 Z3.0\nM2\n").is_ok());
    assert!(parse_str("G21 G90 G17\nG10 L2 P1.0000 X1.0 Y2.0 Z3.0\nM2\n").is_ok());
}

// ===========================================================================
// Аудит #18/#17: пробел внутри слова; вложенные комментарии (текст диагностики)
// ===========================================================================

#[test]
fn audit18_space_between_letter_and_number_accepted() {
    // Зонд rs274: `G0 X 10.5 Y20` + `G1 X5. Y 8. F100` — контроллер
    // ПРИНИМАЕТ (ошибки лексики нет). Ранее наш лексер давал
    // «word X без числа» — расхождение с контроллером.
    let p = parse_str(concat!(
        "G21 G90 G94 G17\n",
        "G0 X 10.5 Y20\n",
        "G1 X5. Y 8. F100\n",
        "M2\n",
    ))
    .expect("пробел между буквой и числом — легален (rs274-симметрия)");
    // rapid end = (10.5, 20, 0); linear end = (5, 8, 0)
    let ends: Vec<[f64; 3]> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Rapid { end, .. } | Op::Linear { end, .. } => Some(to_f64(end)),
            _ => None,
        })
        .collect();
    assert_eq!(ends, vec![[10.5, 20.0, 0.0], [5.0, 8.0, 0.0]], "{ends:?}");
    // множественные пробелы и таб — тоже
    let p =
        parse_str("G21 G90\nG0 X  10.0\nG1 Y\t3.0 F10.0\nM2\n").expect("tabs/multiple spaces ok");
    let ends: Vec<[f64; 3]> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Rapid { end, .. } | Op::Linear { end, .. } => Some(to_f64(end)),
            _ => None,
        })
        .collect();
    assert_eq!(ends, vec![[10.0, 0.0, 0.0], [10.0, 3.0, 0.0]]);
    // буква без числа ПОСЛЕ пропуска пробела — по-прежнему отказ
    let err = parse_str("G21 G90\nG0 X Y20\nM2\n").expect_err("X без числа (даже с пробелом)");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
}

#[test]
fn audit17_nested_comment_diagnostic_mentions_rs274() {
    // Зонд rs274: вложенные скобки → «Nested comment found» = отказ.
    // Наша строгость честна (отказ сохранён), текст диагностики называет
    // причину (аудит #17 — поведение не меняется).
    let err = parse_str("G21 G90\nG1 (outer (inner) more) X10.0\nM2\n")
        .expect_err("вложенный комментарий — лексический отказ");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
    assert!(
        err.iter().any(|d| d.message.contains("вложенные скобки")
            && d.message.contains("Nested comment found")),
        "текст диагностики должен упоминать вложенность и rs274: {err:?}"
    );
    // Незакрытая одиночная '(' без вложенности — прежний протокол «остаток
    // строки комментируется»: строка парсится (hint не добавляется).
    assert!(
        parse_str("G21 G90\nG0 X5.0 (незакрытый комментарий\nM2\n").is_ok(),
        "незакрытая ( — хвост-комментарий, код до неё валиден"
    );
}

// ===========================================================================
// Аудит #14: word_num — один финальный раунд из ТЕКСТА (half away from zero)
// ===========================================================================

#[test]
fn audit14_word_num_single_final_rounding_from_text() {
    use eid_linuxcnc::lexer::word_num;
    // .5-границы: 5-й знак ≥ 5 → вверх по ТЕКСТУ, без f64-шума парса.
    assert_eq!(
        word_num("1.23455"),
        Some(1.2346),
        "старая формула давала 1.2345 (двойное округление)"
    );
    assert_eq!(word_num("0.00005"), Some(0.0001));
    assert_eq!(word_num("1.99995"), Some(2.0), "перенос в целую часть");
    assert_eq!(word_num("-1.23455"), Some(-1.2346), "half AWAY from zero");
    // 5-й знак < 5 → усечение (без округления вверх).
    assert_eq!(word_num("1.23454"), Some(1.2345));
    assert_eq!(
        word_num("0.00004"),
        Some(0.0),
        "CORPUS #13 — квантование к старту"
    );
    // ≤ 4 дробных знаков — значение текста без изменений.
    assert_eq!(word_num("10."), Some(10.0));
    assert_eq!(word_num("+5"), Some(5.0));
    assert_eq!(word_num("-.5"), Some(-0.5));
    assert_eq!(word_num("10.0000"), Some(10.0));
    assert_eq!(word_num("0"), Some(0.0));
    // Мусор — None.
    assert_eq!(word_num(""), None);
    assert_eq!(word_num("-"), None);
    assert_eq!(word_num("1.2.3"), None);
    assert_eq!(word_num("abc"), None);
}

#[test]
fn audit14_word_num_fmt_num_roundtrip_invariant() {
    // Инвариант (аудит #14): word_num(s) — неподвижная точка fmt_num:
    // word_num(s) == parse(fmt_num(word_num(s))) на .5-границах.
    use eid_linuxcnc::lexer::word_num;
    for s in [
        "1.23455",
        "0.00005",
        "2.00005",
        "-1.23455",
        "9.99995",
        "1.23454",
        "123.45675",
        "0.00015",
    ] {
        let v = word_num(s).expect("parses");
        let rt: f64 = eid_core::emitter::fmt_num(v)
            .parse()
            .expect("fmt_num parses");
        assert_eq!(
            v,
            rt,
            "инвариант сломан на {s}: {v} vs fmt_num {}",
            eid_core::emitter::fmt_num(v)
        );
    }
}

// ===========================================================================
// Аудит #19 (пин G55 G53 G0 X0), #20 (M3 без S), #34 (round-trip G18/G19)
// ===========================================================================

#[test]
fn audit19_g55_g53_machine_frame_keeps_wcs_state() {
    // Пин (аудит #19): `G55 G53 G0 X0` — WCS сменён на G55 и ход
    // исполняется в machine-frame (SPEC §5.3; wcs_to_p(G55) = p2 — origin
    // G55 определён in-text, §5.3(b)); следующая строка — в G55:
    // чтение = machine − origin(G55). Соответствует LinuxCNC.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G10 L2 P2 X1.0000 Y2.0000 Z3.0000\n",
        "G55\n",
        "G53 G0 X0.0000 Y0.0000 Z0.0000\n",
        "G0 X1.0000 Y2.0000 Z3.0000\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    let motions: Vec<(&eid_core::ast::Frame, eid_core::ast::Wcs, bool, [f64; 3])> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Rapid {
                start, end, state, ..
            } => Some((
                &state.frame,
                state.wcs,
                *start == StartPoint::Unknown,
                to_f64(end),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(motions.len(), 2, "{motions:?}");
    // Ход 1: machine-frame, wcs = G55 (p=5), end (0,0,0).
    assert_eq!(
        *motions[0].0,
        eid_core::ast::Frame::Machine,
        "G53 → machine-frame"
    );
    assert_eq!(
        motions[0].1,
        eid_core::ast::Wcs::G55,
        "WCS сменён на G55 ДО machine-хода"
    );
    assert!(
        motions[0].2,
        "первый machine-ход: start unknown (M₀ внешний)"
    );
    assert_eq!(motions[0].3, [0.0, 0.0, 0.0]);
    // Ход 2: возврат в WCS G55: start = machine(0,0,0) − origin(G55).
    assert_eq!(*motions[1].0, eid_core::ast::Frame::Wcs);
    assert_eq!(
        motions[1].1,
        eid_core::ast::Wcs::G55,
        "следующая строка — в G55"
    );
    assert!(!motions[1].2, "start известен: (0,0,0) − (1,2,3)");
    let _ = motions[1].3;
    // round-trip: эмиссия воспроизводит G55 + G53 G0.
    let emitted = LinuxCncAdapter::new().emit(&p).expect("emits").text;
    assert!(emitted.contains("G55"), "{emitted}");
    assert!(emitted.contains("G53 G0"), "{emitted}");
}

#[test]
fn audit20_m3_without_s_is_warning_not_err() {
    // Аудит #20: M3/M4 без S — notice-warning (не Err): эмитится S0.0000.
    let a = LinuxCncAdapter::new();
    let out = a
        .parse(
            "G21 G90 G94 G17\nG0 X0.0000 Y0.0000 Z0.0000\nM3\nG1 X10.0000 F100.0\nM2\n",
            "a20.ngc",
        )
        .expect("M3 без S — Ok, не отказ");
    let p = out.program;
    let diags = out.diagnostics;
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("W_SPINDLE_S_MISSING")
                && d.message.contains("задать обороты вручную")),
        "ожидалась W_SPINDLE_S_MISSING: {diags:?}"
    );
    // AST: шпиндель Cw(0) — S0.0000 при эмиссии.
    let emitted = a.emit(&p).expect("emits").text;
    assert!(emitted.contains("M3 S0.0000"), "{emitted}");
    // M4 — тот же класс.
    let a2 = LinuxCncAdapter::new();
    let out2 = a2
        .parse(
            "G21 G90 G94 G17\nG0 X0.0000 Y0.0000 Z0.0000\nM4\nG1 X10.0000 F100.0\nM2\n",
            "a20b.ngc",
        )
        .expect("M4 без S — Ok");
    assert!(
        out2.diagnostics
            .iter()
            .any(|d| d.message.contains("W_SPINDLE_S_MISSING")),
        "M4: {:?}",
        out2.diagnostics
    );
    // Наследование предыдущего S-слова — БЕЗ warning (S модален).
    let a3 = LinuxCncAdapter::new();
    let out3 = a3
        .parse(
            "G21 G90 G94 G17\nS5000\nG0 X0.0000 Y0.0000 Z0.0000\nM3\nG1 X10.0000 F100.0\nM2\n",
            "a20c.ngc",
        )
        .expect("M3 с наследованным S — Ok");
    assert!(
        !out3
            .diagnostics
            .iter()
            .any(|d| d.message.contains("W_SPINDLE_S_MISSING")),
        "наследованный S5000 — warning не нужен"
    );
}

#[test]
fn audit34_roundtrip_g18_arcs_negative_ik() {
    // Аудит #34: round-trip emit→parse для G18-дуг с отрицательными I/K —
    // 180°, винтовая (pitch через Y) и полный круг (end==start, center≠start).
    let src = concat!(
        "G21 G90 G94 G18 G91.1\n",
        "M3 S8000\n",
        "G0 X0.0000 Y0.0000 Z10.0000\n",
        "G2 X-10.0000 Z4.0000 I-5.0000 K-3.0000 F100.0\n",
        "G3 X0.0000 Z10.0000 I5.0000 K3.0000\n",
        "G2 X-10.0000 Z4.0000 Y6.0000 I-5.0000 K-3.0000\n",
        "G2 X-10.0000 Z4.0000 I-5.0000 K-3.0000\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let p1 = a.parse(src, "a34_g18.ngc").expect("parses").program;
    // Контроль: полный круг присутствует (sweep=360, F2-правило для явного end).
    let arcs: Vec<(f64, f64, eid_core::ast::Dir)> = p1
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Arc {
                sweep_deg,
                pitch,
                dir,
                ..
            } => Some((sweep_deg.value, pitch.value, *dir)),
            _ => None,
        })
        .collect();
    assert_eq!(
        arcs,
        vec![
            (180.0, 0.0, eid_core::ast::Dir::Cw),
            (180.0, 0.0, eid_core::ast::Dir::Ccw),
            (180.0, 6.0, eid_core::ast::Dir::Cw),
            (360.0, 0.0, eid_core::ast::Dir::Cw)
        ],
        "{arcs:?}"
    );
    let emitted = a.emit(&p1).expect("emits").text;
    // Эмиссия G18: in-plane пара I+K (не J), смена плоскости отдельной строкой.
    assert!(emitted.contains("G18"), "{emitted}");
    assert!(emitted.contains("I-5.0000 K-3.0000"), "{emitted}");
    assert!(!emitted.contains(" J"), "{emitted}");
    // Полный круг — без слов осей (CORPUS #08 форма; F модален — на 4-й дуге
    // feed уже 100, слово не воспроизводится).
    assert!(emitted.contains("\nG2 I-5.0000 K-3.0000\n"), "{emitted}");
    let p2 = a
        .parse(&emitted, "a34_g18_rt.ngc")
        .expect("re-parses")
        .program;
    let d = semantic_diff(&p1, &p2, &DiffConfig::default());
    assert!(d.is_empty(), "G18 round-trip divergences: {d:?}");
}

#[test]
fn audit34_roundtrip_g19_arcs_negative_jk() {
    // Аудит #34: G19-дуги с отрицательными J/K — 180°, helical (X), полный круг.
    let src = concat!(
        "G21 G90 G94 G19 G91.1\n",
        "M3 S8000\n",
        "G0 X0.0000 Y0.0000 Z10.0000\n",
        "G3 Y-10.0000 Z4.0000 J-5.0000 K-3.0000 F100.0\n",
        "G2 Y0.0000 Z10.0000 J5.0000 K3.0000\n",
        "G3 Y-10.0000 Z4.0000 X6.0000 J-5.0000 K-3.0000\n",
        "G3 Y-10.0000 Z4.0000 J-5.0000 K-3.0000\n",
        "M2\n",
    );
    let a = LinuxCncAdapter::new();
    let p1 = a.parse(src, "a34_g19.ngc").expect("parses").program;
    let arcs: Vec<(f64, f64)> = p1
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Arc {
                sweep_deg, pitch, ..
            } => Some((sweep_deg.value, pitch.value)),
            _ => None,
        })
        .collect();
    assert_eq!(
        arcs,
        vec![(180.0, 0.0), (180.0, 0.0), (180.0, 6.0), (360.0, 0.0)],
        "{arcs:?}"
    );
    let emitted = a.emit(&p1).expect("emits").text;
    assert!(emitted.contains("G19"), "{emitted}");
    assert!(emitted.contains("J-5.0000 K-3.0000"), "{emitted}");
    assert!(
        !emitted.contains(" I-"),
        "{emitted} — I не in-plane для G19"
    );
    let p2 = a
        .parse(&emitted, "a34_g19_rt.ngc")
        .expect("re-parses")
        .program;
    let d = semantic_diff(&p1, &p2, &DiffConfig::default());
    assert!(d.is_empty(), "G19 round-trip divergences: {d:?}");
}

#[test]
fn audit19_g53_partial_end_is_runtime_state() {
    // Дополнение пина #19: буквально `G53 G0 X0` (частичный end) —
    // machine-ходы не наследуют нуль (SPEC §2 v0.13) → E_RUNTIME_STATE;
    // wcs-абсорбция G55 при этом не при чём.
    let err = parse_str(concat!(
        "G21 G90 G94 G17\n",
        "G10 L2 P2 X1.0000 Y2.0000 Z3.0000\n",
        "G55\n",
        "G53 G0 X0.0000\n",
        "M2\n",
    ))
    .expect_err("machine-ход с частичным end — E_RUNTIME_STATE");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::RuntimeState),
        "{err:?}"
    );
}

// ===========================================================================
// Мутационный раунд v0.17/v0.18 (агент D): preset-cross-WCS связывание,
// G93 3D-длина, sub-нумерация G83-развёртки (v0.18 пеки).
// ===========================================================================

#[test]
fn mround_preset_cross_wcs_with_origins() {
    // Точка разрыва (b) для пресетки: first move в ЧУЖОЙ WCS с обеими
    // origin in-text: start = preset + O_preset − O_current
    // = (5,5,5) + (1,1,1) − (2,2,2) = (4,4,4).
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G92 X5.0000 Y5.0000 Z5.0000\n",
        "G10 L2 P1 X1.0000 Y1.0000 Z1.0000\n",
        "G10 L2 P2 X2.0000 Y2.0000 Z2.0000\n",
        "G55\n",
        "G1 X10.0000 F100.0\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    assert!(
        matches!(&p.ops[2], Op::Linear { start: StartPoint::Known(s), .. }
        if to_f64(s) == [4.0, 4.0, 4.0]),
        "start = preset + O54 − O55: {:?}",
        p.ops[2]
    );
}

#[test]
fn mround_g93_linear_feed_len_3d() {
    // G93: 3D-диагональ от ненулевого старта — len = |(3,4,0)| = 5, F=1 → 5
    // (прежние G93-тесты были осе-выровнены: мутанты len-формулы выживали).
    let src = "G21 G90 G93 G17\nG0 X1.0000 Y2.0000 Z3.0000\nG1 X4.0000 Y6.0000 Z3.0000 F1.0\nM2\n";
    let p = parse_str(src).expect("parses");
    match &p.ops[1] {
        Op::Linear { state, .. } => {
            let f = state.feed.as_ref().unwrap().value;
            assert!((f - 5.0).abs() < 1e-6, "G93 feed = len·F = 5, got {f}");
        }
        other => panic!("op 1 must be Linear, got {other:?}"),
    }
}

#[test]
fn mround_g83_unroll_subs_and_depths() {
    // G83-развёртка v0.18: sub производных опов — ПЛОТНАЯ последовательность
    // 1..13; депеки 0/−2/−4/−6, промежуточные подходы d+0.254 (клиренс),
    // полные отводы на R=2 (+=-мутанты счётчиков и клиренс-арифметика).
    let src = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/tier_a/v0/19_peck_G83_retract/peck_G83_retract.linuxcnc.ngc"
    ));
    let p = parse_str(src).expect("parses");
    let mut subs = Vec::new();
    for op in &p.ops {
        if let Op::Rapid { sub, .. } | Op::Linear { sub, .. } = op {
            if let Some(s) = sub {
                subs.push(*s);
            }
        }
    }
    assert_eq!(
        subs,
        Vec::<u32>::from_iter(1..=13),
        "sub-нумерация G83: 1 traverse, 2 подход, (3 feed, 4 отвод, 5 подход)×3, 12 feed, 13 финальный отвод: {subs:?}"
    );
    // Депеки: концы feed-ходов 0, −2, −4, −6.
    let feed_ends: Vec<f64> = p
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Linear { end, .. } => Some(to_f64(end)[2]),
            _ => None,
        })
        .collect();
    assert_eq!(
        feed_ends,
        [0.0, -2.0, -4.0, -6.0],
        "глубины депеков по Q=2 от R=2: {feed_ends:?}"
    );
}

#[test]
fn mround_g10_l20_set_here_anchor_links() {
    // G10 L20 P1 (wcs_set_here) — ПОЗИТИВНОЕ связывание: первый ход получает
    // start = delta (якорь (a)); l20_of → None-мутант ронял Known-старт.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G10 L20 P1 X3.0000 Y4.0000 Z0.0000\n",
        "G0 X5.0000 Y4.0000 Z0.0000\n",
        "M2\n",
    );
    let p = parse_str(src).expect("parses");
    assert!(matches!(&p.ops[0], Op::WcsSetHere { p: 1, .. }));
    assert!(
        matches!(&p.ops[1], Op::Rapid { start: StartPoint::Known(s), .. }
        if to_f64(s) == [3.0, 4.0, 0.0]),
        "первый ход после L20: start = delta: {:?}",
        p.ops[1]
    );
}

// ===========================================================================
// SPEC v0.20 (зонд-сверка rs274, коммит c76ed9a): G92-mid = оп frame_preset
// (зонды 1–3, 7, 8), G92.1/.2/.3 → Raw{TABLE} (зонд 4), tail после PEND →
// Raw + диалект-расхождение (зонд 5), bare-T → Raw{TABLE} (зонд 6).
// ===========================================================================

/// Owning-результат parse (диагностики + notices) для unit-исходников.
fn parse_out(src: &str) -> eid_core::adapter::ParseOutput {
    LinuxCncAdapter::new()
        .parse(src, "v020.ngc")
        .expect("parses Ok")
}

#[test]
fn v020_probe1_g92_mid_chaining() {
    // Зонд 1: `G0 X10 Y10 / G92 X0 Y0 / G1 X5` — start следующего хода =
    // (0,0) (присвоенные оси), end (5,0); оп-носитель — Raw (маркер
    // "G92 mid-program"); Ok-файл (Err N2 снят).
    let out = parse_out("%\nG0 X10. Y10. F100\nG92 X0. Y0.\nG1 X5.\n%\n");
    let p = out.program;
    assert_eq!(p.ops.len(), 4); // Rapid + Raw + Linear + PEND
    assert!(
        matches!(&p.ops[0], Op::Rapid { start: StartPoint::Unknown, end, .. }
            if to_f64(end) == [10.0, 10.0, 0.0]),
        "{:?}",
        p.ops[0]
    );
    assert!(
        matches!(&p.ops[1], Op::Raw { reason, code: ErrorCode::UnsupportedSemantics, .. }
            if reason.starts_with("G92 mid-program")),
        "{:?}",
        p.ops[1]
    );
    assert!(
        matches!(&p.ops[2], Op::Linear { start: StartPoint::Known(s), end, .. }
            if to_f64(s) == [0.0, 0.0, 0.0] && to_f64(end) == [5.0, 0.0, 0.0]),
        "чейнинг: start следующего хода = новое чтение (зонд 1): {:?}",
        p.ops[2]
    );
    assert!(matches!(&p.ops[3], Op::ProgramEnd { .. }));
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("G92 mid-program")),
        "диагностика Ok-пути с маркером: {:?}",
        out.diagnostics
    );
}

#[test]
fn v020_probe3_g92_per_axis() {
    // Зонд 3: неуказанные оси — чтение не меняется: G0 X10 Y10 / G92 X0 /
    // G0 X5 Y5 → start (0,10,0) (Y переносится), end (5,5,0).
    let out = parse_out("%\nG0 X10. Y10.\nG92 X0.\nG0 X5. Y5.\n%\n");
    let p = out.program;
    assert_eq!(p.ops.len(), 4); // Rapid + Raw + Rapid + PEND
    assert!(
        matches!(&p.ops[2], Op::Rapid { start: StartPoint::Known(s), end, .. }
            if to_f64(s) == [0.0, 10.0, 0.0] && to_f64(end) == [5.0, 5.0, 0.0]),
        "per-axis: X присвоен 0, Y — прежнее чтение 10 (зонд 3): {:?}",
        p.ops[2]
    );
}

#[test]
fn v020_probe8_g92_wcs_switch_unknown_start() {
    // Зонд 8: регистр G92 глобален поверх WCS; после G92 смена G54→G55 →
    // start следующего хода = unknown (rs274 пересчитывает от машины,
    // машины в AST нет — честный unknown). Ход G0 X5 — end (5,0,0) в G55.
    let out = parse_out("%\nG54 G0 X10.\nG92 X0.\nG55\nG0 X5. Y0. Z0.\n%\n");
    let p = out.program;
    assert_eq!(p.ops.len(), 4); // Rapid + Raw + Rapid(G55) + PEND
    assert!(
        matches!(&p.ops[2], Op::Rapid { start: StartPoint::Unknown, end, state, .. }
            if to_f64(end) == [5.0, 0.0, 0.0] && state.wcs == eid_core::ast::Wcs::G55),
        "WCS-переключение после G92 → unknown-start (зонд 8); частичный end в
         неизвестной позиции — E_RUNTIME_STATE (SPEC §2 v0.11), ход — полный:
         {:?}",
        p.ops[2]
    );
}

#[test]
fn v020_probe8b_g92_wcs_switch_with_origins_formula_holds() {
    // Δ (регистр G92) сокращается в разности чтений — формула (b)
    // reading_new = end + O_prev − O_new точна и после mid-G92:
    // rebase (0,0,0), O54=(1,1,1), O55=(2,2,2) → start (−1,−1,−1)
    // (совпадает с core-verify — подавление НЕ требуется).
    let out = parse_out(
        "%\nG54 G0 X10.\nG92 X0.\nG10 L2 P1 X1. Y1. Z1.\nG10 L2 P2 X2. Y2. Z2.\nG55\nG0 X5.\n%\n",
    );
    let p = out.program;
    assert!(
        matches!(&p.ops[4], Op::Rapid { start: StartPoint::Known(s), .. }
            if to_f64(s) == [-1.0, -1.0, -1.0]),
        "формула (b) с in-text origins точна после mid-G92: {:?}",
        p.ops[4]
    );
}

#[test]
fn v020_probe2_g92_twice_replace_from_current_reading() {
    // Зонд 2: повторная G92 — replace по пересчёту от текущего чтения (не
    // аккумуляция): после G0 X5 чтение (5,10,0), G92 X0 → (0,10,0) — НЕ (−10).
    let out = parse_out("%\nG0 X10. Y10.\nG92 X0.\nG0 X5.\nG92 X0.\nG0 X5.\n%\n");
    let p = out.program;
    assert_eq!(p.ops.len(), 6); // Rapid + Raw + Rapid + Raw + Rapid + PEND
    assert!(
        matches!(&p.ops[2], Op::Rapid { start: StartPoint::Known(s), end, .. }
            if to_f64(s) == [0.0, 10.0, 0.0] && to_f64(end) == [5.0, 10.0, 0.0]),
        "{:?}",
        p.ops[2]
    );
    assert!(
        matches!(&p.ops[4], Op::Rapid { start: StartPoint::Known(s), end, .. }
            if to_f64(s) == [0.0, 10.0, 0.0] && to_f64(end) == [5.0, 10.0, 0.0]),
        "вторая G92: replace от текущего чтения (зонд 2: 15 = 10+5, не 10+10): {:?}",
        p.ops[4]
    );
}

// ===========================================================================
// v0.21 per-axis (SPEC §5.3 ревизия 0566db5/c76ed9a, fresa-класс
// mit_cws_fresa.gcode:5 `G92 z50 x-10`): частичная стартовая G92 легальна —
// указанные оси = присвоенные значения, неуказанные = текущее чтение
// (на старте = 0) → header vec3 partial. Стартовый путь = header-поле для
// ПЕРВОЙ G92 до ходов (full ИЛИ partial — axes_from_words base 0.0),
// семантика per-axis идентична mid-пути (зонды 1–3 выше); chaining обычный.
// ===========================================================================

#[test]
fn v021_g92_startup_partial_fresa() {
    // fresa-паттерн дословно: G92 z50 x-10 до ходов → vec3 (X-10, Y0, Z50)
    // — ПОРЯДОК X,Y,Z; rs274-док: «axis word not used for a given axis —
    // offset zero» == стартовое чтение 0. Chaining: G0 z10 стартует от
    // пресетки (−10,0,50), G1 z40 — ход в новой системе (10 → 40).
    let out = parse_out("G17\nG21\nG90\nG40\nG92 z50 x-10\nG0 z10.000\nG1 z40. F100\nM2\n");
    let p = out.program;
    let fp = p
        .frame_preset
        .as_ref()
        .expect("частичная стартовая G92 → header Program.frame_preset");
    assert_eq!(to_f64(&fp.values), [-10.0, 0.0, 50.0]);
    assert_eq!(fp.wcs, eid_core::ast::Wcs::G54);
    assert_eq!(p.ops.len(), 3); // Rapid + Linear + PEND
    assert!(
        matches!(&p.ops[0], Op::Rapid { start: StartPoint::Known(s), end, .. }
            if to_f64(s) == [-10.0, 0.0, 50.0] && to_f64(end) == [-10.0, 0.0, 10.0]),
        "старт первого хода = частичная пресетка (Y чтение 0): {:?}",
        p.ops[0]
    );
    assert!(
        matches!(&p.ops[1], Op::Linear { start: StartPoint::Known(s), end, .. }
            if to_f64(s) == [-10.0, 0.0, 10.0] && to_f64(end) == [-10.0, 0.0, 40.0]),
        "G1 z40 — ход в новой системе: {:?}",
        p.ops[1]
    );
    assert!(matches!(&p.ops[2], Op::ProgramEnd { .. }));
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("G92 mid-program")),
        "стартовая G92 — НЕ mid-носитель (header-путь): {:?}",
        out.diagnostics
    );
}

#[test]
fn v021_g92_startup_partial_single_axis() {
    // Одиночная ось: G92 X2.0 — Y/Z чтение 0; ход G0 Y3.0 от (2,0,0).
    let out = parse_out("G21\nG90\nG92 X2.0\nG0 Y3.0\nM2\n");
    let p = out.program;
    let fp = p.frame_preset.as_ref().expect("header preset");
    assert_eq!(to_f64(&fp.values), [2.0, 0.0, 0.0]);
    assert!(
        matches!(&p.ops[0], Op::Rapid { start: StartPoint::Known(s), end, .. }
            if to_f64(s) == [2.0, 0.0, 0.0] && to_f64(end) == [2.0, 3.0, 0.0]),
        "{:?}",
        p.ops[0]
    );
}

#[test]
fn v020_probe7_g92_after_g53_reads_machine() {
    // Зонд 7/7b: G92 после machine-хода — rebase от машинного чтения:
    // G53 G0 X0 (машина 0, origin (0,0,0) in-text) / G92 X5 → чтение 5;
    // G0 X7 → start (5,0,0), end (7,0,0). Команда X5 дала бы машину 0 (7b).
    let out =
        parse_out("%\nG10 L2 P1 X0. Y0. Z0.\nG53 G0 X0. Y0. Z0.\nG92 X5.\nG0 X7. Y0. Z0.\n%\n");
    let p = out.program;
    assert!(
        matches!(&p.ops[1], Op::Rapid { start: StartPoint::Unknown, end, state, .. }
            if to_f64(end) == [0.0, 0.0, 0.0] && state.frame == eid_core::ast::Frame::Machine),
        "{:?}",
        p.ops[1]
    );
    assert!(
        matches!(&p.ops[3], Op::Rapid { start: StartPoint::Known(s), end, .. }
            if to_f64(s) == [5.0, 0.0, 0.0] && to_f64(end) == [7.0, 0.0, 0.0]),
        "rebase от машинного чтения (зонд 7: value = машина − programmed): {:?}",
        p.ops[3]
    );
}

#[test]
fn v020_probe7_corner_unknown_unspecified_axes() {
    // Corner зонда 7: неуказанные оси после machine-истории БЕЗ in-text
    // origin (G55 не define'd) — чтение не вычислимо → Raw{E_RUNTIME_STATE}
    // (честный unknown), следующий ход: start Unknown, полный end.
    let out = parse_out(
        "%\nG10 L2 P1 X1. Y2. Z3.\nG53 G0 X0. Y0. Z0.\nG55\nG92 X5.\nG0 X1. Y1. Z1.\n%\n",
    );
    let p = out.program;
    assert_eq!(p.ops.len(), 5); // WcsDefine + Rapid(machine) + Raw + Rapid + PEND
    match &p.ops[2] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::RuntimeState);
            assert!(reason.starts_with("G92 mid-program"), "{reason}");
        }
        other => panic!("op 2 must be Raw (corner), got {other:?}"),
    }
    assert!(
        matches!(&p.ops[3], Op::Rapid { start: StartPoint::Unknown, end, .. }
            if to_f64(end) == [1.0, 1.0, 1.0]),
        "полный end при corner-unknown старте (SPEC §2 v0.11): {:?}",
        p.ops[3]
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == ErrorCode::RuntimeState),
        "{:?}",
        out.diagnostics
    );
}

#[test]
fn v020_g92_mid_machine_frame_still_err_n16() {
    // Machine-frame правило §5.3(a) — без изменений: mid-program G92 тоже
    // ломает известность машинных координат → N16.
    let err = LinuxCncAdapter::new()
        .parse("%\nG0 X10.\nG92 X0.\nG53 G0 X0.\n%\n", "v020.ngc")
        .expect_err("G53 после mid-G92 — E_RUNTIME_STATE");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::RuntimeState),
        "{err:?}"
    );
}

#[test]
fn v020_g92_1_cancel_requires_machine_table() {
    // Зонд 4 (Q4 частично): G92.1 = reset всего регистра — возврат чтений
    // требует абсолютной машинной позиции → Raw{E_REQUIRES_MACHINE_TABLE}
    // (код сменён с SEMANTICS). Отменяющая семантика к чейнингу НЕ
    // применяется (регистр в AST не хранится) — следующий ход цепляется
    // от текущего чтения.
    let out = parse_out("%\nG0 X10. Y10. F100\nG92 X0.\nG92.1\nG1 X5.\n%\n");
    let p = out.program;
    match &p.ops[2] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::RequiresMachineTable);
            assert!(reason.contains("зонд 4"), "{reason}");
        }
        other => panic!("op 2 must be Raw, got {other:?}"),
    }
    assert!(
        matches!(&p.ops[3], Op::Linear { start: StartPoint::Known(s), end, .. }
            if to_f64(s) == [0.0, 10.0, 0.0] && to_f64(end) == [5.0, 10.0, 0.0]),
        "чейнинг от текущего чтения (G92.1 регистр AST не меняет): {:?}",
        p.ops[3]
    );
}

#[test]
fn v020_probe5_tail_raw_and_state_not_absorbed() {
    // Зонд 5: хвост после M2 — Raw-опы; состояние хвост-строками НЕ
    // абсорбируется (rs274 их никогда не парсит): M3 S500 в хвосте не даёт
    // spindle-оп; % после M2 — без опа.
    let out = parse_out("%\nG0 X10.\nM2\nM3 S500\nG0 X20.\n%\n");
    let p = out.program;
    assert_eq!(p.ops.len(), 4, "[Rapid, PEND, Raw(M3), Raw(G0 X20)]");
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    assert!(
        matches!(&p.ops[1], Op::ProgramEnd { .. }),
        "PEND, не forced-last"
    );
    // Хвост — только Raw-опы (состояние не абсорбируется, ходов нет).
    assert!(
        p.ops[2..].iter().all(|o| matches!(o, Op::Raw { .. })),
        "хвост не даёт motion/процесс-опов: {:?}",
        &p.ops[2..]
    );
    match &p.ops[2] {
        Op::Raw {
            reason,
            code,
            payload_hex,
            ..
        } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(reason.contains("dialect-divergent"), "{reason}");
            assert!(
                reason.contains("rs274-file") && reason.contains("grbl-stream"),
                "{reason}"
            );
            assert_eq!(*payload_hex, hex_of("M3 S500\n"), "payload lossless (D9)");
        }
        other => panic!("op 2 must be Raw-хвост, got {other:?}"),
    }
    match &p.ops[3] {
        Op::Raw { payload_hex, .. } => {
            assert_eq!(*payload_hex, hex_of("G0 X20.\n"), "payload lossless (D9)");
        }
        other => panic!("op 3 must be Raw-хвост, got {other:?}"),
    }
}

/// hex-payload строки с LF (D9) — для lossless-проверок.
fn hex_of(s: &str) -> String {
    s.as_bytes().iter().map(|b| format!("{b:02X}")).collect()
}

#[test]
fn v020_g92_mid_zero_length_after_rebase_dropped() {
    // Ребейз может дать zero-length ход (G92 X0 Y0 / G1 X0) — drop+notice
    // (SPEC §3), файл Ok; «вооружение» подавления снимается на следующем
    // выжившем опе (маркер-механизм по AST).
    let out = parse_out("%\nG0 X10. Y10.\nG92 X0. Y0.\nG1 X0. F100\n%\n");
    let p = out.program;
    assert!(
        p.ops.iter().all(|o| !matches!(o, Op::Linear { .. })),
        "zero-length Linear дропнут: {:?}",
        p.ops
    );
    assert!(
        out.notices
            .iter()
            .any(|n| n.kind == NoticeKind::ZeroLengthMotion),
        "{:?}",
        out.notices
    );
}

#[test]
fn v020_g92_mid_cycle_chain() {
    // Цикл после mid-G92: traverse развёртки цепляется от ребейзнутого
    // чтения (0,10,0).
    let out = parse_out("%\nG0 X10. Y10.\nG92 X0.\nG81 X1. Y1. Z-2. R1. F50\n%\n");
    let p = out.program;
    match &p.ops[2] {
        Op::Rapid {
            start: StartPoint::Known(s),
            end,
            sub: Some(1),
            ..
        } => {
            assert_eq!(to_f64(s), [0.0, 10.0, 0.0], "rebase в развёртку");
            assert_eq!(to_f64(end), [1.0, 1.0, 0.0]);
        }
        other => panic!("op 2 = derived traverse (sub=1), got {other:?}"),
    }
}

#[test]
fn v020_g92_mid_on_raw_line_with_other_reason() {
    // `G28 G92 X0` — строка уходит в Raw{TABLE} (G28), но G92 в блоке
    // исполняется (rs274) → rebase применён, маркер дописан в reason
    // (суффикс), подавление start-проверки сработало → файл Ok.
    let out = parse_out("%\nG0 X10. Y10.\nG28 G92 X0.\nG0 X5. Y5. Z5.\n%\n");
    let p = out.program;
    assert_eq!(p.ops.len(), 4); // Rapid + Raw + Rapid + PEND
    match &p.ops[1] {
        Op::Raw { reason, code, .. } => {
            assert_eq!(*code, ErrorCode::RequiresMachineTable, "G28 — основание");
            assert!(
                reason.contains("G92 mid-program"),
                "маркер-суффикс в reason: {reason}"
            );
        }
        other => panic!("op 1 must be Raw, got {other:?}"),
    }
    assert!(
        matches!(&p.ops[2], Op::Rapid { start: StartPoint::Known(s), end, .. }
            if to_f64(s) == [0.0, 10.0, 0.0] && to_f64(end) == [5.0, 5.0, 5.0]),
        "rebase применён и на строке с другим raw-основанием: {:?}",
        p.ops[2]
    );
}

#[test]
fn v020_negative_regress_p_word_and_l2_p19() {
    // Пин негативов v0.20: NURBS-фикс (spline_active) не маскирует
    // «P-слово без потребителя»; G10 L2 P1.9 — Err (as-cast-гейт).
    let a = LinuxCncAdapter::new();
    let err = a
        .parse("G21 G90\nG0 X1. Y1.\nP5\nM2\n", "r.ngc")
        .expect_err("P без G5.x-контекста — Err");
    assert!(
        err[0].message.contains("P-слово без потребителя"),
        "{}",
        err[0].message
    );
    let err = a
        .parse("G21 G90\nG10 L2 P1.9 X1. Y1. Z1.\nM2\n", "r.ngc")
        .expect_err("G10 L2 P1.9 — Err");
    assert!(
        err.iter().all(|d| d.code == ErrorCode::InvalidInput),
        "{err:?}"
    );
}
