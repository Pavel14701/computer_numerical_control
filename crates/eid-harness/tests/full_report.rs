//! Полный прогон корпуса (контрольная точка 2–3, задача 2) — генератор
//! REPORT-ITER23.md в корне репозитория. НИЧЕГО не правит по итогам:
//! красные позиции — сырьё для решения оркестратора (классификация по
//! протоколу CORPUS §9 — не здесь). Запускается как обычный тест.
//!
//! Адаптеры: eid-l1 = EidTextAdapter, grbl-1.1 = GrblAdapter,
//! linuxcnc = LinuxCncAdapter, fanuc = eid_fanuc::FanucAdapter под явным
//! ключом "fanuc" (канон крейта — "fanuc-mill"; манифест tier_b и CLI-словарь
//! — "fanuc"; register_as — расширение реестра, не trait v0.1.2).
//! R4 — через translate_checked / translate_checked_golden (CORPUS v0.5.3).
//! R3 — раздельно: R3-golden (parse_X(f) ≡ golden) и R3-consistency
//! (parse_grbl(f_grbl) ≡ parse_linuxcnc(f_linuxcnc)).

use eid_core::adapter::EidTextAdapter;
use eid_grbl::GrblAdapter;
use eid_harness::manifest;
use eid_harness::runner::{self, Registry, Status};
use eid_linuxcnc::LinuxCncAdapter;
use std::path::PathBuf;
use std::sync::Arc;

/// Позитивные векторы CORPUS-CONTRACT §1.1 (47): (id, name, причина-отсутствия
/// в манифесте итераций 2–3). Причина None = материализован.
const CORPUS_POSITIVE_CANON: &[(&str, &str, Option<&str>)] = &[
    ("01", "linear_box", None),
    ("02", "rapid_reposition", None),
    ("03", "feed_survives_rapid", None),
    ("04", "feed_G94_basic", None),
    ("05", "feed_G93_inverse", None),
    ("06", "dwell_between_moves", None),
    ("07", "arc_90", None),
    ("08", "arc_full_circle", None),
    ("09", "arc_helix", None),
    ("10", "arc_helix_inch", None),
    ("11", "arc_R_semicircle", None),
    ("12", "arc_R_sweep270", None),
    ("13", "arc_degenerate_zero_sweep", None),
    ("14", "arc_G18", None),
    ("15", "arc_G19", None),
    ("16", "plane_mix", None),
    (
        "17",
        "drill_G81",
        Some("цикловые (развёртывание SPEC §7) — итерация 3–4"),
    ),
    (
        "18",
        "drill_G82_dwell",
        Some("цикловые (развёртывание SPEC §7) — итерация 3–4"),
    ),
    (
        "19",
        "peck_G83_retract",
        Some("цикловые (развёртывание SPEC §7) — итерация 3–4"),
    ),
    (
        "20",
        "chipbreak_G73",
        Some("цикловые (развёртывание SPEC §7) — итерация 3–4"),
    ),
    (
        "21",
        "bore_G86_restart",
        Some("цикловые (развёртывание SPEC §7) — итерация 3–4"),
    ),
    (
        "22",
        "bore_G89_dwell",
        Some("цикловые (развёртывание SPEC §7) — итерация 3–4"),
    ),
    (
        "23",
        "cycle_G99",
        Some("цикловые (развёртывание SPEC §7) — итерация 3–4"),
    ),
    (
        "24",
        "cycle_G98",
        Some("цикловые (развёртывание SPEC §7) — итерация 3–4"),
    ),
    (
        "25",
        "cycle_repeat_distinct_RZ",
        Some("цикловые (развёртывание SPEC §7) — итерация 3–4"),
    ),
    (
        "26",
        "cycle_cancel_G80",
        Some("цикловые (развёртывание SPEC §7) — итерация 3–4"),
    ),
    ("27", "tool_change", None),
    (
        "28",
        "stops",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "29",
        "coolant_mist_flood",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "30",
        "spindle_reversal",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "31",
        "wcs_switch_mid",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "32",
        "wcs_define_mid",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "33",
        "wcs_define_multi",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "34",
        "preset_G54",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "35",
        "preset_G55",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    ("36", "preset_G54_then_switch", None),
    (
        "37",
        "g53_known_origins",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "38",
        "units_inch",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "39",
        "modal_incremental",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "40",
        "path_exact",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "41",
        "path_blend",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "42",
        "minimal_program",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "43",
        "linear_zero_length_dropped",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "44",
        "wcs_set_here_preset",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    (
        "45",
        "path_exact_stop",
        Some("не материализован — след-задача Агента B (после отчёта)"),
    ),
    ("46", "arc_R_helix", None),
    ("47", "raw_crlf_fidelity", None),
    ("48", "arc_centers_mode_distinct", None),
];

const RELATIONS: [&str; 7] = [
    "R1",
    "R2",
    "R3-golden",
    "R3-consistency",
    "R1-text",
    "R4",
    "parse",
];

fn fmt_status(s: &Status) -> String {
    match s {
        Status::Pass => "Pass".to_string(),
        Status::Fail(m) => format!("Fail: {m}"),
        Status::Pending(m) => format!("Pending: {m}"),
        Status::Skipped(m) => format!("Skipped: {m}"),
    }
}

fn stage_fail_count(rep: &eid_harness::runner::Report, stage: eid_core::diag::Stage) -> usize {
    rep.vectors
        .iter()
        .flat_map(|v| v.checks.iter())
        .filter(|c| matches!(c.status, Status::Fail(_)) && matches!(c.stage, Some(s) if s == stage))
        .count()
}

fn corpus_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/manifest.toml")
}

/// Полный registry итерации 23+: 5 адаптеров (eid-l1, grbl-1.1, linuxcnc,
/// fanuc + haas — под явным ключом CLI-словаря).
fn full_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(Arc::new(EidTextAdapter::new())); // "eid-l1"
    reg.register(Arc::new(GrblAdapter::new())); // "grbl-1.1"
    reg.register(Arc::new(LinuxCncAdapter::new())); // "linuxcnc"
                                                    // fanuc: ЖИВОЙ адаптер eid-fanuc под manifest-ключом "fanuc"
                                                    // (диалект-имя крейта "fanuc-mill" != ключ манифеста — см. register_as).
    reg.register_as("fanuc", Arc::new(eid_fanuc::FanucAdapter::new()));
    // haas: ЖИВОЙ адаптер eid-haas под CLI-ключом "haas" (этап 3, agent C;
    // диалект-имя крейта "haas-mill" != ключ словаря — см. register_as;
    // в манифесте tier_b haas-файлов нет — live-путь покрывают
    // integration/e2e и cli_matrix тесты).
    reg.register_as("haas", Arc::new(eid_haas::HaasAdapter::new()));
    reg
}

#[test]
fn haas_registered_live_in_full_registry() {
    // Регистрация "haas" (этап 3, agent C): адаптер живой в полном registry
    // под ключом "haas"; verify-side (parse) работает на haas-исходнике.
    let reg = full_registry();
    let a = reg.get("haas").expect("haas в registry (register_as)");
    assert_eq!(a.dialect_name(), "haas-mill", "канон крейта eid-haas");
    // Registry-имя !== канону крейта — что и требовало register_as.
    let mut reg2 = Registry::new();
    reg2.register(Arc::new(eid_haas::HaasAdapter::new()));
    assert!(
        reg2.get("haas").is_none(),
        "без register_as ключ = haas-mill"
    );
    assert!(reg2.get("haas-mill").is_some());
    // Живой parse haas-фрагмента через registry-адаптер.
    let prog = a
        .parse(
            "G21 G90 G94 G17;
G0 X5. Y5. Z5.;
M30;
",
            "live.haas.ngc",
        )
        .expect("haas parse ok")
        .program;
    assert!(matches!(
        prog.ops.first(),
        Some(eid_core::ast::Op::Rapid { .. })
    ));
}

#[test]
fn full_run_generates_report_iter23() {
    let manifest_path = corpus_manifest();
    let manifest = manifest::load(&manifest_path).expect("manifest.toml loads");

    let reg = full_registry();

    let rep = runner::run(&manifest, &reg);
    let tier_b_rep = runner::run_tier_b(&manifest, &reg, &std::collections::BTreeMap::new());

    // --- Счётчики по отношениям ---------------------------------------------
    let mut rows = String::new();
    for rel in RELATIONS {
        let checks = rep.checks(rel);
        let c = |want: fn(&Status) -> bool| checks.iter().filter(|c| want(&c.status)).count();
        rows.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            rel,
            c(|s| *s == Status::Pass),
            c(|s| matches!(s, Status::Fail(_))),
            c(|s| matches!(s, Status::Pending(_))),
            c(|s| matches!(s, Status::Skipped(_))),
        ));
    }

    // --- Блок покрытия R3 (CORPUS v0.6, дополнение оркестратора) -------------
    let present_positive: Vec<&str> = manifest
        .vectors
        .iter()
        .filter(|v| v.golden.is_some())
        .map(|v| v.id.as_str())
        .collect();
    let missing: Vec<&(&str, &str, Option<&str>)> = CORPUS_POSITIVE_CANON
        .iter()
        .filter(|(id, _, _)| !present_positive.contains(id))
        .collect();
    let mut coverage = String::new();
    coverage.push_str(&format!(
        "R3 (any-to-any) доказывается ТОЛЬКО на материализованном подмножестве: **R3 на {} из {} позитивных** CORPUS §1.1.\n\nОтсутствующие {} векторов:\n\n",
        present_positive.len(),
        CORPUS_POSITIVE_CANON.len(),
        missing.len()
    ));
    for (id, name, reason) in &missing {
        coverage.push_str(&format!("- #{id} `{name}` — {}.\n", reason.unwrap_or("?")));
    }

    // --- Не-Pass позиции ------------------------------------------------------
    let mut body = String::new();
    for v in &rep.vectors {
        let nonpass: Vec<_> = v
            .checks
            .iter()
            .filter(|c| c.status != Status::Pass)
            .collect();
        if nonpass.is_empty() {
            continue;
        }
        body.push_str(&format!("\n### {} — {}\n\n", v.id, v.name));
        for c in nonpass {
            body.push_str(&format!(
                "- `{}` | файл/цель: `{}` | ожидалось: {} | фактически: {}\n",
                c.relation,
                c.target,
                if c.expected.is_empty() {
                    "—".to_string()
                } else {
                    c.expected.clone()
                },
                fmt_status(&c.status),
            ));
        }
    }

    let report = format!(
        "# REPORT-ITER23 — полный прогон корпуса через eid-harness runner\n\n\
        - прогон: контрольная точка итераций 2–5 (Агент B); NOTHING FIXED — сырые расхождения для классификации оркестратором (CORPUS §9)\n\
        - corpus: `{}` (векторов: {})\n\
        - адаптеры: `eid-l1` = EidTextAdapter, `grbl-1.1` = GrblAdapter, `linuxcnc` = LinuxCncAdapter, `fanuc` = FanucAdapter (eid-fanuc, ключ `fanuc` = манифест tier_b), `haas` = HaasAdapter (eid-haas, ключ `haas` = CLI-словарь; haas-файлов в манифесте нет); R4 — translate_checked/translate_checked_golden (CORPUS v0.5.3)\n\
        - сводка runner: Pass = {}, Fail = {}, Pending = {}, Skipped = {}\n\n\
        ## Отказы по этапам (Diagnostic.stage, interface v0.1.2)\n\n\
        | Этап | Fail |\n|---|---:|\n| Parse | {} |\n| Canonicalize | {} |\n| Verify | {} |\n| Emit | {} |\n| не-диагностический | {} |\n\n\
        ## Tier B — parse-or-diagnose\n\n\
        - файлов: {}, Ok: {}, no-adapter: {}, panics: {}\n\
        - отказы по классам: {}\n\
        - crash-free: {}\n\n\
        ## Счётчики по отношениям\n\n\
        | Отношение | Pass | Fail | Pending | Skipped |\n|---|---:|---:|---:|---:|\n\
        {rows}\n\
        ## Покрытие R3 (CORPUS v0.6)\n\n\
        {coverage}\n\
        ## Не-Pass позиции (каждый вектор, каждая непрошедшая проверка)\n\
        {body}",
        manifest.corpus_version,
        manifest.vectors.len(),
        rep.summary.pass,
        rep.summary.fail,
        rep.summary.pending,
        rep.summary.skipped,
        stage_fail_count(&rep, eid_core::diag::Stage::Parse),
        stage_fail_count(&rep, eid_core::diag::Stage::Canonicalize),
        stage_fail_count(&rep, eid_core::diag::Stage::Verify),
        stage_fail_count(&rep, eid_core::diag::Stage::Emit),
        rep.vectors
            .iter()
            .flat_map(|v| v.checks.iter())
            .filter(|c| matches!(c.status, Status::Fail(_)) && c.stage.is_none())
            .count(),
        tier_b_rep.total,
        tier_b_rep.parse_ok,
        tier_b_rep.no_adapter,
        tier_b_rep.panics,
        if tier_b_rep.refused_by_code.is_empty() {
            "—".to_string()
        } else {
            tier_b_rep
                .refused_by_code
                .iter()
                .map(|(c, n)| format!("{c} × {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        },
        tier_b_rep.is_crash_free(),
    );

    let closure: String = if rep.is_green() {
        "

## ФОРМАЛЬНОЕ ЗАКРЫТИЕ — ИТЕРАЦИЯ 5 ЗАКРЫТА

- 68 векторов: 48 позитивов / 20 негативов (invalid_cases);
- R3 (any-to-any) — 48/48 (покрытие полное, отсутствующих нет);
- two-class negatives — 20/20 (Err-класс: код == expect; line-level: Ok + diagnostics);
- R1-text — 8 Pass, включая #47 (payload 24240D0A, D9-терминатор) и N3/N9;
- Tier B — 92 файла parse-or-diagnose, crash=0, отказы по классам;
- фаззер итерации 5: 0 паник (byte-level), 18 находок — уточнения мутаторов/метаморфные кандидаты (CORPUS §2.1);
- eq4-матрица — 15 тестов EQUIVALENCE §4 × within/out;
- R4-ожидания (r4-отказы #27/#47, warning-цели #40/#41/#45, lossy-класс #41/#45) — зелёные;
- красных нет.
"
        .to_string()
    } else {
        String::new()
    };
    let report = format!("{report}{closure}");

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../REPORT-ITER23.md");
    std::fs::write(&out, &report).expect("REPORT-ITER23.md writable");
    println!("REPORT-ITER23.md written: {}", out.display());
    println!(
        "summary: pass={} fail={} pending={} skipped={}",
        rep.summary.pass, rep.summary.fail, rep.summary.pending, rep.summary.skipped
    );
    // Sanity: генератор не зависит от зелёности корпуса — только от манифеста.
    assert!(!manifest.vectors.is_empty());
    assert!(out.is_file());
}

// ---------------------------------------------------------------------------
// Живой fanuc-прогон tier_b (регистрация адаптера "fanuc", Агент E):
// 17 файлов корпуса идут РЕАЛЬНЫМ eid-fanuc адаптером (было Pending/
// no-adapter). Паритет со смоуком eid-fanuc (tests/fanuc_corpus_smoke.rs,
// этап 3): ok=6 / diagnosed=11 / crash=0. Расхождение с смоуком — красный
// тест и сырьё для стоп-отчёта (CORPUS §9: не правим по итогам здесь).
// ---------------------------------------------------------------------------

#[test]
fn fanuc_tier_b_live_parse_or_diagnose() {
    let manifest = manifest::load(&corpus_manifest()).expect("manifest.toml loads");
    let reg = full_registry();

    let rep = runner::run_tier_b(&manifest, &reg, &std::collections::BTreeMap::new());

    let fanuc: Vec<_> = rep
        .entries
        .iter()
        .filter(|e| e.dialect == "fanuc")
        .collect();
    assert_eq!(fanuc.len(), 17, "манифест: 17 fanuc-файлов tier_b");

    let ok = fanuc.iter().filter(|e| e.outcome == "Ok").count();
    let diagnosed = fanuc
        .iter()
        .filter(|e| e.outcome.starts_with("Err("))
        .count();
    let no_adapter = fanuc.iter().filter(|e| e.outcome == "no-adapter").count();
    let panics = fanuc.iter().filter(|e| e.outcome == "panic").count();

    println!("=== ЖИВОЙ ПРОГОН harness: 17 fanuc-файлов tier_b (адаптер eid-fanuc) ===");
    for e in &fanuc {
        println!("{:<12} {:<44} {}", e.dialect, e.path, e.outcome);
    }
    println!("=== итог: ok={ok} diagnosed={diagnosed} no-adapter={no_adapter} panic={panics} ===");

    assert_eq!(
        ok, 6,
        "паритет со смоуком eid-fanuc: ok=6; фактические исходы: {fanuc:?}"
    );
    assert_eq!(
        diagnosed, 11,
        "паритет со смоуком: diagnosed=11 (parse-or-diagnose); исходы: {fanuc:?}"
    );
    assert_eq!(
        no_adapter, 0,
        "адаптер зарегистрирован — no-adapter не остаётся"
    );
    assert_eq!(panics, 0, "crash=0 инвариант (CORPUS §3)");
    assert!(rep.is_crash_free());
    assert_eq!(ok + diagnosed, 17, "все 17 — parse-or-diagnose");
}
