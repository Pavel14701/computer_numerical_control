//! E2E smoke, итерация 4 (Агент E): полный цикл через функции конвейера
//! (eid-integration::translate — те же, что вызывает CLI; не через бинарь).
//!
//! Пилот #01: file → translate(grbl→linuxcnc) → файл создан →
//! parse(output) ≡ parse(input) по semantic_diff (EQUIVALENCE §4) → + reverse.
//! Негатив #27: translate(tool_change → grbl-1.1) → Err SYNTAX на emit-стадии
//! (manifest r4_expect, COVERAGE §5) → CLI-маппинг = exit 1.
//! Регистрация fanuc (Агент E): fanuc-цель live (parse-back ≡ на exact_stop),
//! RAW-правило на ok-файле tier_b (кросс-диалект отказ, same-dialect verbatim).

use eid_core::adapter::Adapter;
use eid_core::diag::ErrorCode;
use eid_harness::diff::{semantic_diff, DiffConfig};
use eid_integration::profile::{Limits, MachineProfile};
use eid_integration::sim::{check_program, mock_sim};
use eid_integration::translate::{
    format_diagnostic, stage_tag, translate_file, translate_file_with_profile, translate_text,
    translate_text_with_profile, AnyAdapter, TranslateError, GENERATED_PREAMBLE,
};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
}

fn corpus_pilot(dir: &str, file: &str) -> PathBuf {
    repo_root()
        .join("corpus")
        .join("tier_a")
        .join("v0")
        .join(dir)
        .join(file)
}

fn e2e_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eid_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("создать временный каталог e2e");
    dir
}

/// parse текста заданным диалектом (регистр адаптеров напрямую).
fn parse(dialect: &str, src: &str, file: &str) -> eid_core::ast::Program {
    let a = AnyAdapter::for_name(dialect).expect("адаптер диалекта");
    // v0.2 #22: owning-результат — notices в ParseOutput (буфера нет).
    a.parse(src, file).expect("parse должен пройти").program
}

fn assert_equivalent(a: &eid_core::ast::Program, b: &eid_core::ast::Program, what: &str) {
    let d = semantic_diff(a, b, &DiffConfig::default());
    assert!(d.is_empty(), "{what}: semantic_diff не пуст: {d:?}");
}

// ---------------------------------------------------------------------------
// Пилот #01: grbl → linuxcnc, файл создан, R3-эквивалентность, mock-sim
// ---------------------------------------------------------------------------

#[test]
fn e2e_pilot01_grbl_to_linuxcnc() {
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let output = e2e_dir().join("pilot01.linuxcnc.ngc");
    let _ = std::fs::remove_file(&output);

    // 1. translate через CLI-функцию: файл создан, не пуст
    let outcome =
        translate_file("grbl-1.1", "linuxcnc", &input, &output).expect("translate #01 → linuxcnc");
    let meta = std::fs::metadata(&output).expect("output файл создан");
    assert!(meta.len() > 0, "output пуст");
    assert_eq!(outcome.to, "linuxcnc");

    // 2. parse(v1-output) ≡ parse(input) — R3 на composed-фрейме
    let src = std::fs::read_to_string(&input).unwrap();
    let dst = std::fs::read_to_string(&output).unwrap();
    let a = parse("grbl-1.1", &src, "input.grbl.ngc");
    let b = parse("linuxcnc", &dst, "output.linuxcnc.ngc");
    assert_equivalent(&a, &b, "#01 grbl→linuxcnc");

    // 3. mock-sim принимает выход (критерий #8, mock с пометкой)
    let profile = MachineProfile::linuxcnc_default();
    let report = mock_sim(&profile, &dst, "output.linuxcnc.ngc");
    assert!(
        report.is_ok(),
        "mock-sim должен принять эмиссию: {:?}",
        report.errors
    );
    assert!(report.ops_count > 0);
}

// ---------------------------------------------------------------------------
// Reverse: linuxcnc → grbl (критерий #10) + полный demo-круг
// ---------------------------------------------------------------------------

#[test]
fn e2e_pilot01_reverse_linuxcnc_to_grbl() {
    // прямое направление из corpus-файла LinuxCNC
    let input = corpus_pilot("01_linear_box", "linear_box.linuxcnc.ngc");
    let mid = e2e_dir().join("pilot01.rev.grbl.ngc");
    let outcome = translate_file("linuxcnc", "grbl-1.1", &input, &mid).expect("reverse #01");
    assert_eq!(outcome.to, "grbl-1.1");

    let a = parse(
        "linuxcnc",
        &std::fs::read_to_string(&input).unwrap(),
        "input.linuxcnc.ngc",
    );
    let b = parse(
        "grbl-1.1",
        &std::fs::read_to_string(&mid).unwrap(),
        "mid.grbl.ngc",
    );
    assert_equivalent(&a, &b, "#01 linuxcnc→grbl");

    // demo-круг (PLAN §5): grbl → linuxcnc → grbl, все три AST равны
    let regenerated = e2e_dir().join("pilot01.regenerated.grbl.ngc");
    translate_file(
        "grbl-1.1",
        "linuxcnc",
        &corpus_pilot("01_linear_box", "linear_box.grbl.ngc"),
        &e2e_dir().join("pilot01.fwd.linuxcnc.ngc"),
    )
    .expect("прямой ход demo-круга");
    translate_file(
        "linuxcnc",
        "grbl-1.1",
        &e2e_dir().join("pilot01.fwd.linuxcnc.ngc"),
        &regenerated,
    )
    .expect("обратный ход demo-круга");
    let original = parse(
        "grbl-1.1",
        &std::fs::read_to_string(corpus_pilot("01_linear_box", "linear_box.grbl.ngc")).unwrap(),
        "original.grbl.ngc",
    );
    let regen = parse(
        "grbl-1.1",
        &std::fs::read_to_string(&regenerated).unwrap(),
        "regenerated.grbl.ngc",
    );
    assert_equivalent(&original, &regen, "demo-круг grbl→linuxcnc→grbl");
}

// ---------------------------------------------------------------------------
// Негатив #27 (tool_change): Err E_UNSUPPORTED_SYNTAX на emit-стадии
// ---------------------------------------------------------------------------

#[test]
fn e2e_negative_27_tool_change_emit_syntax() {
    // GRBL-источника у #27 нет (manifest: r4_expect grbl-1.1 = SYNTAX) —
    // направление linuxcnc → grbl-1.1.
    let input = corpus_pilot("27_tool_change", "tool_change.linuxcnc.ngc");
    let output = e2e_dir().join("t27.grbl.ngc");
    let _ = std::fs::remove_file(&output);

    let err = translate_file("linuxcnc", "grbl-1.1", &input, &output)
        .expect_err("#27: tool_change вне словаря GRBL — emit обязан отказать");

    // отказ на emit-стадии (parse linuxcnc прошёл: T3 M6 → ToolChange-оп)
    match &err {
        TranslateError::Emit(diags) => {
            assert!(
                diags
                    .iter()
                    .any(|d| d.code.as_str() == "E_UNSUPPORTED_SYNTAX"),
                "#27: ожидалась E_UNSUPPORTED_SYNTAX, фактические: {:?}",
                diags.iter().map(|d| d.code.as_str()).collect::<Vec<_>>()
            );
            // stage-атрибуция (MLIR-урок, interface v0.1.2): печать диагностики
            // несёт этап — CLI stderr для #27 обязан содержать `[emit]`.
            assert!(
                diags
                    .iter()
                    .any(|d| format_diagnostic(d).starts_with("[emit] ")),
                "#27: format_diagnostic без [emit]-тега: {:?}",
                diags.iter().map(format_diagnostic).collect::<Vec<_>>()
            );
        }
        other => panic!("#27: ожидался Err(Emit), фактический: {other:?}"),
    }
    // CLI-маппинг: TranslateError::Emit → Failure → exit 1 (документировано в
    // справке; сам exit-код проверяется смоук-запуском бинаря оркестратором).
    assert_eq!(err.stage(), "emit");
    assert!(!output.exists(), "#27: output файл не должен создаваться");

    // parse-стадия для полноты: сам файл линуксцнсовский — parse проходит
    // (ToolChange-оп), отказ именно в эмиссии цели.
    let src = std::fs::read_to_string(&input).unwrap();
    let program = parse("linuxcnc", &src, "tool_change.linuxcnc.ngc");
    assert!(!program.ops.is_empty());

    // и через text-конвейер (без файлов) — та же стадия
    let err2 = translate_text("linuxcnc", "grbl-1.1", &src, "tool_change.linuxcnc.ngc")
        .expect_err("#27 text-конвейер");
    assert!(matches!(err2, TranslateError::Emit(_)));
}

// ---------------------------------------------------------------------------
// eid-l1: registry-путь EidTextAdapter (golden #01 через текстовый формат)
// ---------------------------------------------------------------------------

#[test]
fn e2e_eidl1_route_grbl_roundtrip() {
    // grbl → eid-l1 → grbl через text-конвейер: R1-машина в registry
    let src =
        std::fs::read_to_string(corpus_pilot("01_linear_box", "linear_box.grbl.ngc")).unwrap();
    let out =
        translate_text("grbl-1.1", "eid-l1", &src, "linear_box.grbl.ngc").expect("grbl → eid-l1");
    let back = translate_text("eid-l1", "grbl-1.1", &out.emitted, "linear_box.eid")
        .expect("eid-l1 → grbl");
    let a = parse("grbl-1.1", &src, "input.grbl.ngc");
    let b = parse("grbl-1.1", &back.emitted, "roundtrip.grbl.ngc");
    assert_equivalent(&a, &b, "grbl→eid-l1→grbl");
}

// ---------------------------------------------------------------------------
// Pending-диалекты: grbl-hal/fluidnc → AdapterPending (не паника)
// ---------------------------------------------------------------------------

#[test]
fn e2e_pending_dialect_is_reported() {
    let src =
        std::fs::read_to_string(corpus_pilot("01_linear_box", "linear_box.grbl.ngc")).unwrap();
    let err =
        translate_text("grbl-1.1", "grbl-hal", &src, "x.grbl.ngc").expect_err("grbl-hal — pending");
    assert!(matches!(err, TranslateError::AdapterPending { .. }));
    assert_eq!(err.stage(), "config");
}

// ---------------------------------------------------------------------------
// RAW-правило (SPEC §8.2, фикс критический, итерация 5)
// ---------------------------------------------------------------------------

#[test]
fn e2e_47_raw_cross_refuses_both_directions() {
    // grbl-файл с '$$' → linuxcnc: Err E_RAW_UNTRANSLATABLE (emit-стадия),
    // все Raw-спаны в options (формат B raw_refusal, зеркально)
    let grbl = std::fs::read_to_string(corpus_pilot(
        "47_raw_crlf_fidelity",
        "raw_crlf_fidelity.grbl.ngc",
    ))
    .unwrap();
    let err = translate_text("grbl-1.1", "linuxcnc", &grbl, "raw_crlf_fidelity.grbl.ngc")
        .expect_err("#47: кросс-диалект с RAW обязан отказать");
    match &err {
        TranslateError::Emit(diags) => {
            assert_eq!(diags.len(), 1, "ровно одна диагностика: {diags:?}");
            assert_eq!(diags[0].code, ErrorCode::RawUntranslatable);
            assert_eq!(stage_tag(diags[0].stage), "emit");
            assert!(
                diags[0].message.contains("1 RAW op(s)"),
                "количество RAW в message: {diags:?}"
            );
            assert!(diags[0].src.is_some(), "первый Raw-спан в src");
            assert_eq!(diags[0].options.len(), 1, "все Raw-спаны в options");
        }
        other => panic!("#47: ожидался Err(Emit), фактический: {other:?}"),
    }

    // linuxcnc-файл с '$$' → grbl: то же правило (v1-генерация — v0-файла
    // linuxcnc у #47 нет в regression_files)
    let lnx_path = repo_root()
        .join("corpus/tier_a/v1/47_raw_crlf_fidelity/raw_crlf_fidelity.linuxcnc.ngc")
        .canonicalize()
        .expect("v1 linuxcnc-файл #47");
    let lnx = std::fs::read_to_string(&lnx_path).unwrap();
    let err2 = translate_text(
        "linuxcnc",
        "grbl-1.1",
        &lnx,
        "raw_crlf_fidelity.linuxcnc.ngc",
    )
    .expect_err("#47 reverse: кросс-диалект с RAW обязан отказать");
    assert!(matches!(err2, TranslateError::Emit(_)));
    assert_eq!(err2.stage(), "emit");
}

#[test]
fn e2e_47_raw_golden_refuses_any_target() {
    // golden/eid-l1 источник: любой Raw → отказ ЛЮБОЙ цели (включая eid-l1)
    let golden =
        std::fs::read_to_string(corpus_pilot("47_raw_crlf_fidelity", "golden.eid")).unwrap();
    for to in ["eid-l1", "grbl-1.1", "linuxcnc"] {
        let err = translate_text("eid-l1", to, &golden, "golden.eid")
            .expect_err("golden с Raw отказывает любой цели");
        match &err {
            TranslateError::Emit(diags) => {
                assert_eq!(diags[0].code, ErrorCode::RawUntranslatable, "target={to}");
            }
            other => panic!("target={to}: ожидался Err(Emit), фактический: {other:?}"),
        }
    }
}

#[test]
fn e2e_n3_same_dialect_raw_verbatim() {
    // N3: same-dialect RAW вербатимен (SPEC §8.1) — без отказа
    let grbl = std::fs::read_to_string(corpus_pilot(
        "n3_raw_vendor_block",
        "raw_vendor_block.grbl.ngc",
    ))
    .unwrap();
    let out = translate_text("grbl-1.1", "grbl-1.1", &grbl, "n3.grbl.ngc")
        .expect("same-dialect RAW легален");
    assert!(
        out.emitted.contains("$110=250.000"),
        "RAW '$110=250.000' потерян при same-dialect: {:?}",
        out.emitted
    );

    let lnx = std::fs::read_to_string(corpus_pilot(
        "n3_raw_vendor_block",
        "raw_vendor_block.linuxcnc.ngc",
    ))
    .unwrap();
    let out2 = translate_text("linuxcnc", "linuxcnc", &lnx, "n3.linuxcnc.ngc")
        .expect("same-dialect RAW легален");
    assert!(
        out2.emitted.contains("M123"),
        "RAW 'M123' потерян при same-dialect: {:?}",
        out2.emitted
    );
}

// ---------------------------------------------------------------------------
// Идемпотентность (QA, итерация 5)
// ---------------------------------------------------------------------------

#[test]
fn e2e_translate_idempotent_double_run() {
    // (a) двойной прогон одного файла → байт-в-байт тот же результат
    let src =
        std::fs::read_to_string(corpus_pilot("01_linear_box", "linear_box.grbl.ngc")).unwrap();
    let run1 = translate_text("grbl-1.1", "linuxcnc", &src, "in.grbl.ngc").unwrap();
    let run2 = translate_text("grbl-1.1", "linuxcnc", &src, "in.grbl.ngc").unwrap();
    assert_eq!(run1.emitted, run2.emitted, "translate недетерминирован");

    // (б) translate(translate-выхода) → идентичный выход: same-dialect
    // re-translate (linuxcnc→linuxcnc) стабилен (R1-свойство эмиссии)
    let again = translate_text("linuxcnc", "linuxcnc", &run1.emitted, "mid.linuxcnc.ngc").unwrap();
    assert_eq!(again.emitted, run1.emitted, "re-translate не идемпотентен");

    // (в) и через файловую функцию: повторная запись — тот же байтовый файл
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let out1 = e2e_dir().join("idem1.ngc");
    let out2 = e2e_dir().join("idem2.ngc");
    translate_file("grbl-1.1", "linuxcnc", &input, &out1).unwrap();
    translate_file("grbl-1.1", "linuxcnc", &input, &out2).unwrap();
    assert_eq!(
        std::fs::read(&out1).unwrap(),
        std::fs::read(&out2).unwrap(),
        "файловый translate недетерминирован"
    );
}

#[test]
fn e2e_verify_idempotent_same_report() {
    // verify(verify) → тот же отчёт: parse + structural-проверки дважды
    let output = e2e_dir().join("idem_verify.ngc");
    translate_file(
        "grbl-1.1",
        "linuxcnc",
        &corpus_pilot("01_linear_box", "linear_box.grbl.ngc"),
        &output,
    )
    .unwrap();
    let text = std::fs::read_to_string(&output).unwrap();
    let profile = MachineProfile::linuxcnc_default();
    let r1 = mock_sim(&profile, &text, "idem_verify.ngc");
    let r2 = mock_sim(&profile, &text, "idem_verify.ngc");
    assert_eq!(r1, r2, "verify-отчёт недетерминирован");

    // и на AST-уровне: check_program на двух независимых parse — тот же отчёт
    let a = parse("linuxcnc", &text, "f.ngc");
    let b = parse("linuxcnc", &text, "f.ngc");
    assert_eq!(check_program(&profile, &a), check_program(&profile, &b));
}

// ---------------------------------------------------------------------------
// Смешанные line endings (метаморфное отношение)
// ---------------------------------------------------------------------------

#[test]
fn e2e_mixed_line_endings_equivalent() {
    let lf = std::fs::read_to_string(corpus_pilot("01_linear_box", "linear_box.grbl.ngc")).unwrap();

    // вперемешку: CRLF через строку, LF — остальные (голый \r без \n — вне
    // словаря лексера, отдельная диспозиция владельца диалекта)
    let mut mixed = String::new();
    for (i, line) in lf.lines().enumerate() {
        if i % 2 == 0 {
            mixed.push_str(&format!("{line}\r\n"));
        } else {
            mixed.push_str(&format!("{line}\n"));
        }
    }

    // parse Ok для смешанного входа и AST-эквивалентность LF-версии
    // (одинаковое имя файла → src-spans сравнимы; semantic_diff их и так
    // исключает — provenance, EQUIVALENCE §5)
    let a = parse("grbl-1.1", &lf, "f.ngc");
    let b = parse("grbl-1.1", &mixed, "f.ngc");
    assert_eq!(a.ops.len(), b.ops.len(), "число опов разошлось на CRLF");
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    assert!(d.is_empty(), "CRLF-версия не эквивалентна LF: {d:?}");

    // метаморфное отношение на уровне CLI: эмиссии байт-в-байт равны
    let out_lf = translate_text("grbl-1.1", "linuxcnc", &lf, "f.ngc").unwrap();
    let out_mixed = translate_text("grbl-1.1", "linuxcnc", &mixed, "f.ngc").unwrap();
    assert_eq!(
        out_lf.emitted, out_mixed.emitted,
        "translate(LF) != translate(LF+CRLF)"
    );
}

// ---------------------------------------------------------------------------
// Профиль в конвейере (аудит #2): лимиты профиля реально применяются
// ---------------------------------------------------------------------------

/// Программа на 20 linear-опов (grbl; после auto-PEND — 21 оп).
fn grbl_program_20_ops() -> String {
    let mut s = String::from("G21 G90\n");
    for i in 0..20 {
        s.push_str(&format!("G1 X{i} Y{i} F100.0\n"));
    }
    s.push_str("M2\n");
    s
}

/// Профиль с лимитом max_ops_per_program = limit.
fn profile_with_ops_limit(limit: usize) -> MachineProfile {
    MachineProfile {
        name: "Tiny ops".to_string(),
        dialect: eid_integration::profile::DialectKind::LinuxCnc,
        limits: Limits {
            max_ops_per_program: limit,
            ..Limits::default()
        },
        emit_r_arcs: false,
        emit_preamble: false,
        axis_limits: None,
        axis_limits_units: eid_core::ast::Units::Mm,
    }
}

#[test]
fn e2e_profile_ops_limit_refuses() {
    // #2-критерий: профиль max_ops_per_program=10, программа на 20 опытах →
    // отказ E_LIMIT_EXCEEDED через конвейер с профилем.
    let tiny = profile_with_ops_limit(10);
    let src = grbl_program_20_ops();
    let err =
        translate_text_with_profile("grbl-1.1", "linuxcnc", &src, "tiny.grbl.ngc", Some(&tiny))
            .expect_err("20 опов против лимита 10 обязан отказать");
    let diags = err.diagnostics();
    assert!(
        diags.iter().any(|d| d.code.as_str() == "E_LIMIT_EXCEEDED"),
        "ожидался E_LIMIT_EXCEEDED, фактические: {:?}",
        diags.iter().map(|d| d.code.as_str()).collect::<Vec<_>>()
    );
    let d = diags
        .iter()
        .find(|d| d.code == ErrorCode::LimitExceeded)
        .expect("limit-диагностика");
    let info = d.limit.as_ref().expect("LimitInfo с порогом");
    assert_eq!(info.threshold, "max_ops_per_program");
    assert_eq!(info.threshold_value, 10.0);
    assert_eq!(info.actual, 21.0, "20 ходов + auto-PEND");

    // контроль: тот же вход БЕЗ профиля (дефолт 200000) — проходит
    let ok = translate_text("grbl-1.1", "linuxcnc", &src, "tiny.grbl.ngc")
        .expect("дефолт-лимит 200000 пропускает 21 оп");
    assert!(ok.program.ops.len() >= 20);
}

#[test]
fn e2e_profile_source_lines_limit_refuses() {
    // max_source_lines конвейером ДО parse (guard зависания лексера).
    let mut tiny = profile_with_ops_limit(200_000);
    tiny.limits.max_source_lines = 5;
    let src = grbl_program_20_ops(); // 22 строки > 5
    let err = translate_text_with_profile("grbl-1.1", "linuxcnc", &src, "big.ngc", Some(&tiny))
        .expect_err("22 строки против лимита 5 обязан отказать");
    assert!(matches!(err, TranslateError::Parse(_)));
    let d = err
        .diagnostics()
        .iter()
        .find(|d| d.code == ErrorCode::LimitExceeded)
        .expect("limit-диагностика");
    assert_eq!(d.limit.as_ref().unwrap().threshold, "max_source_lines");
    assert_eq!(stage_tag(d.stage), "parse");
}

#[test]
fn e2e_profile_word_chars_limit_refuses() {
    // max_word_chars: слово X<31 девяток> длиннее порога — до лексера.
    let mut tiny = profile_with_ops_limit(200_000);
    tiny.limits.max_word_chars = 8;
    let src = "G21 G90\nG1 X99999999999.0000 F100.0\nM2\n"; // слово X… = 15 символов
    let err = translate_text_with_profile("grbl-1.1", "linuxcnc", src, "long.ngc", Some(&tiny))
        .expect_err("слово 15 символов против лимита 8 обязан отказать");
    let d = err
        .diagnostics()
        .iter()
        .find(|d| d.code == ErrorCode::LimitExceeded)
        .expect("limit-диагностика");
    assert_eq!(d.limit.as_ref().unwrap().threshold, "max_word_chars");

    // комментарии не считаются: то же слово в скобках — проходит
    let commented = "G21 G90\n(X99999999999.0000 in comment)\nG1 X1.0000 F100.0\nM2\n";
    translate_text_with_profile("grbl-1.1", "linuxcnc", commented, "ok.ngc", Some(&tiny))
        .expect("комментарии не слова — лимит не срабатывает");
}

#[test]
fn e2e_profile_unroll_limit_refuses() {
    // max_unroll_iterations: развёртка цикла L=50 против лимита 10 —
    // sub-индексы 1..N (N = развёрнутые cycle-опы, SPEC §11; та же семантика
    // счёта, что у адаптерной est_total-защиты) → отказ по максимуму sub.
    let mut tiny = profile_with_ops_limit(200_000);
    tiny.limits.max_unroll_iterations = 10;
    let src = "G21 G90\nG0 X0 Y0 Z5\nG81 X10 Y10 Z-2 R3 L50 F50.0\nM2\n";
    let err = translate_text_with_profile("linuxcnc", "grbl-1.1", src, "cycle.ngc", Some(&tiny))
        .expect_err("L=50 против лимита развёртки 10 обязан отказать");
    let d = err
        .diagnostics()
        .iter()
        .find(|d| d.code == ErrorCode::LimitExceeded)
        .expect("limit-диагностика");
    assert_eq!(d.limit.as_ref().unwrap().threshold, "max_unroll_iterations");
    assert!(
        d.limit.as_ref().unwrap().actual > 10.0,
        "факт > лимита: {}",
        d.limit.as_ref().unwrap().actual
    );

    // L=2 (8 развёрнутых опов) при лимите 10 — проходит.
    let small = "G21 G90\nG0 X0 Y0 Z5\nG81 X10 Y10 Z-2 R3 L2 F50.0\nM2\n";
    translate_text_with_profile("linuxcnc", "grbl-1.1", small, "cycle2.ngc", Some(&tiny))
        .expect("L=2 против лимита 10 проходит");
}

#[test]
fn e2e_no_profile_uses_dialect_defaults() {
    // Библиотечный вызов без профиля = дефолт-профиль диалекта (лимиты
    // COVERAGE §1.2): 21 оп проходит, а профиль с max_ops=1 — нет.
    let src = grbl_program_20_ops();
    let ok =
        translate_text("grbl-1.1", "linuxcnc", &src, "d.ngc").expect("дефолт-профиль пропускает");
    assert!(!ok.emitted.is_empty());

    let one_op = profile_with_ops_limit(1);
    let err = translate_text_with_profile("grbl-1.1", "linuxcnc", &src, "d.ngc", Some(&one_op))
        .expect_err("лимит 1 опа против 21 обязан отказать");
    assert!(err
        .diagnostics()
        .iter()
        .any(|d| d.code == ErrorCode::LimitExceeded));
}

// ---------------------------------------------------------------------------
// Префамбул GENERATED BY EID (аудит #11) — только integration-слой
// ---------------------------------------------------------------------------

#[test]
fn e2e_preamble_flag_on_off() {
    let src =
        std::fs::read_to_string(corpus_pilot("01_linear_box", "linear_box.grbl.ngc")).unwrap();

    // флаг off (дефолт) → эмиссия без преамбулы, байт-в-байт как библиотечный путь
    let mut off = MachineProfile::linuxcnc_default();
    assert!(!off.emit_preamble, "дефолт профиля — преамбула off");
    let out_off =
        translate_text_with_profile("grbl-1.1", "linuxcnc", &src, "f.grbl.ngc", Some(&off))
            .unwrap();
    let out_plain = translate_text("grbl-1.1", "linuxcnc", &src, "f.grbl.ngc").unwrap();
    assert_eq!(
        out_off.emitted, out_plain.emitted,
        "emit_preamble=false не меняет эмиссию"
    );
    assert!(
        !out_off.emitted.contains("GENERATED BY EID"),
        "преамбулы не должно быть при флаге off"
    );

    // флаг on → первая строка ровно GENERATED_PREAMBLE
    off.emit_preamble = true;
    let out_on =
        translate_text_with_profile("grbl-1.1", "linuxcnc", &src, "f.grbl.ngc", Some(&off))
            .unwrap();
    let first = out_on.emitted.lines().next().expect("первая строка");
    assert_eq!(first, GENERATED_PREAMBLE, "первая строка — преамбулa #11");
    assert_eq!(
        first,
        "(GENERATED BY EID — VERIFY IN SIMULATOR BEFORE MACHINING)"
    );
    // семантика не тронута: без преамбулы AST совпадает (комментарии игнорируются)
    let a = parse("linuxcnc", &out_plain.emitted, "a.ngc");
    let b = parse("linuxcnc", &out_on.emitted, "b.ngc");
    assert_equivalent(&a, &b, "#11: преамбулa — комментарий, семантика та же");

    // файловый путь: выходной .ngc начинается с преамбулы
    let out_path = e2e_dir().join("preamble.ngc");
    translate_file_with_profile(
        "grbl-1.1",
        "linuxcnc",
        &corpus_pilot("01_linear_box", "linear_box.grbl.ngc"),
        &out_path,
        Some(&off),
    )
    .expect("file-конвейер с преамбулой");
    let written = std::fs::read_to_string(&out_path).unwrap();
    assert!(
        written.starts_with(&format!("{GENERATED_PREAMBLE}\n")),
        "выходной .ngc обязан начинаться с преамбулы: {written:?}"
    );
}

#[test]
fn e2e_preamble_off_emits_verbatim_and_rolls_back_clean() {
    // отрицательный контроль к #11: emit_preamble=true не должен ломать
    // обычный путь повторного parse (преамбулa = комментарий адаптера).
    let src =
        std::fs::read_to_string(corpus_pilot("01_linear_box", "linear_box.grbl.ngc")).unwrap();
    let mut on = MachineProfile::linuxcnc_default();
    on.emit_preamble = true;
    let out =
        translate_text_with_profile("grbl-1.1", "linuxcnc", &src, "f.grbl.ngc", Some(&on)).unwrap();
    // re-translate выхода (same-dialect) — стабилен: преамбулa отсекается
    // parse'ом (комментарий), эмиссия после неё байт-в-байт совпадает.
    let prefix = format!("{GENERATED_PREAMBLE}\n");
    let body = out
        .emitted
        .strip_prefix(&prefix)
        .expect("преамбулa в начале выхода");
    let again = translate_text("linuxcnc", "linuxcnc", body, "mid.linuxcnc.ngc").unwrap();
    assert_eq!(again.emitted, body, "re-translate тела не идемпотентен");
}

// ---------------------------------------------------------------------------
// Haas-диалект (этап 3, agent C: адаптер eid-haas LIVE в registry) — 2 кейса
// ---------------------------------------------------------------------------

#[test]
fn e2e_haas_live_target_parse_back_equiv() {
    // Цель haas live: parse(выход haas) ≡ parse(источник) — semantic_diff 0.
    // Источник linuxcnc с ЯВНЫМ G61.1 (exact_stop): Haas G61 = exact_stop по
    // ЗНАЧЕНИЮ (совпадает с Fanuc, COVERAGE §2.2/§5.2) — value-shift не
    // возникает; источник exact_path дал бы задокументированный сдвиг
    // ExactPath→G61→ExactStop (warning-канал, COVERAGE §5.2). Haas-специфика
    // заголовка: G91.1 НЕ эмитится (группы 04 в словаре нет) — полные end-
    // слова ходов покрывают это (I/J/K инкрементальные — дефолт контроллера).
    let src = "G21 G90 G94 G17
G61.1
M3 S8000
M8
G0 X0 Y0 Z5
G1 X10 F150
M5
M9
M2
";
    let out = translate_text("linuxcnc", "haas", src, "haas_target.linuxcnc.ngc")
        .expect("haas-цель live: translate linuxcnc → haas");
    assert_eq!(out.from, "linuxcnc");
    assert_eq!(out.to, "haas", "registry-имя обёртки HaasNamedAdapter");
    assert!(
        out.emitted.contains("G61"),
        "exact_stop эмиссия — G61: {:?}",
        out.emitted
    );
    assert!(out.emitted.contains("M30"), "program_end haas — M30");
    assert!(
        !out.emitted.contains("G91.1"),
        "G91.1 в словаре Haas отсутствует: {:?}",
        out.emitted
    );
    assert!(
        out.q3_warnings.is_empty(),
        "exact_stop → G61 1:1 без потерь: {:?}",
        out.q3_warnings
    );

    // parse-back: выход читается haas-адаптером и AST эквивалентен источнику.
    let a = parse("linuxcnc", src, "in.linuxcnc.ngc");
    let b = parse("haas", &out.emitted, "out.haas.ngc");
    assert_equivalent(&a, &b, "linuxcnc(G61.1)→haas parse-back");

    // haas-профиль жив: mock-sim принимает эмиссию (verify-путь #10/#27)
    let profile = MachineProfile::haas_default();
    let report = mock_sim(&profile, &out.emitted, "out.haas.ngc");
    assert!(report.is_ok(), "mock-sim haas-профиль: {:?}", report.errors);
}

#[test]
fn e2e_haas_raw_rule_cross_refusal_and_same_dialect_verbatim() {
    // Macro B — живой диалект Haas тоже (Haas Macro B Fanuc-совместим,
    // анализ §4.3): ok-файл tier_b парсится haas-адаптером в Raw-опы →
    // кросс-диалект с Raw → честный E_RAW_UNTRANSLATABLE (SPEC §8.2);
    // same-dialect haas→haas → verbatim (SPEC §8.1).
    let path = repo_root().join("corpus/tier_b/inbox/mpe_while_clear_variables.nc");
    let src = std::fs::read_to_string(&path).expect("mpe_while_clear_variables.nc");

    let err = translate_text("haas", "grbl-1.1", &src, "wcv.haas.nc")
        .expect_err("кросс-диалект haas с Raw обязан отказать");
    match &err {
        TranslateError::Emit(diags) => {
            assert_eq!(diags.len(), 1, "ровно одна диагностика: {diags:?}");
            assert_eq!(diags[0].code, ErrorCode::RawUntranslatable);
            assert!(diags[0].message.contains("RAW op(s)"), "{diags:?}");
        }
        other => panic!("ожидался Err(Emit/RawUntranslatable), фактический: {other:?}"),
    }

    let out = translate_text("haas", "haas", &src, "wcv.haas.nc")
        .expect("same-dialect haas → verbatim RAW легален");
    assert_eq!(out.from, "haas");
    assert_eq!(out.to, "haas");
    for needle in ["WHILE[", "END1", "#1 = #1 + 1"] {
        assert!(
            out.emitted.contains(needle),
            "Macro B payload потерян при same-dialect: {needle:?} в {:?}",
            out.emitted
        );
    }
}

// ---------------------------------------------------------------------------
// Fanuc-диалект (#25 закрыт: адаптер eid-fanuc LIVE в registry) — 2 кейса
// ---------------------------------------------------------------------------

#[test]
fn e2e_fanuc_live_target_parse_back_equiv() {
    // Цель fanuc live: parse(выход fanuc) ≡ parse(источник) — semantic_diff 0.
    // Источник linuxcnc с ЯВНЫМ G61.1 (exact_stop): value-shift не возникает
    // (fanuc ExactStop → G61 1:1, без warning — COVERAGE §2.2/§5.1); источник
    // exact_path дал бы задокументированный сдвиг ExactPath→G61→ExactStop
    // (warning-канал), поэтому честная эквивалентность берётся на exact_stop.
    let src = "G21 G90 G94 G17\nG61.1\nM3 S8000\nM8\nG0 X0 Y0 Z5\nG1 X10 F150\nM5\nM9\nM2\n";
    let out = translate_text("linuxcnc", "fanuc", src, "fanuc_target.linuxcnc.ngc")
        .expect("fanuc-цель live: translate linuxcnc → fanuc");
    assert_eq!(out.from, "linuxcnc");
    assert_eq!(out.to, "fanuc", "registry-имя обёртки FanucNamedAdapter");
    assert!(
        out.emitted.contains("G61"),
        "exact_stop эмиссия — G61: {:?}",
        out.emitted
    );
    assert!(out.emitted.contains("M30"), "program_end fanuc — M30");
    assert!(
        out.q3_warnings.is_empty(),
        "exact_stop → G61 1:1 без потерь: {:?}",
        out.q3_warnings
    );

    // parse-back: выход читается fanuc-адаптером и AST эквивалентен источнику.
    let a = parse("linuxcnc", src, "in.linuxcnc.ngc");
    let b = parse("fanuc", &out.emitted, "out.fanuc.nc");
    assert_equivalent(&a, &b, "linuxcnc(G61.1)→fanuc parse-back");

    // и фанук-профиль жив: mock-sim принимает эмиссию (verify-путь #10/#25)
    let profile = MachineProfile::fanuc_default();
    let report = mock_sim(&profile, &out.emitted, "out.fanuc.nc");
    assert!(
        report.is_ok(),
        "mock-sim fanuc-профиль: {:?}",
        report.errors
    );
}

#[test]
fn e2e_fanuc_raw_rule_cross_refusal_and_same_dialect_verbatim() {
    // Ok-файл tier_b с Macro B (WHILE) — parse-or-diagnose ok=6 смоука:
    // кросс-диалект с Raw → честный E_RAW_UNTRANSLATABLE (SPEC §8.2),
    // same-dialect fanuc→fanuc → verbatim (SPEC §8.1).
    let path = repo_root().join("corpus/tier_b/inbox/mpe_while_clear_variables.nc");
    let src = std::fs::read_to_string(&path).expect("mpe_while_clear_variables.nc");

    let err = translate_text("fanuc", "grbl-1.1", &src, "wcv.fanuc.nc")
        .expect_err("кросс-диалект fanuc с Raw обязан отказать");
    match &err {
        TranslateError::Emit(diags) => {
            assert_eq!(diags.len(), 1, "ровно одна диагностика: {diags:?}");
            assert_eq!(diags[0].code, ErrorCode::RawUntranslatable);
            assert!(diags[0].message.contains("RAW op(s)"), "{diags:?}");
        }
        other => panic!("ожидался Err(Emit/RawUntranslatable), фактический: {other:?}"),
    }

    let out = translate_text("fanuc", "fanuc", &src, "wcv.fanuc.nc")
        .expect("same-dialect fanuc → verbatim RAW легален");
    assert_eq!(out.from, "fanuc");
    assert_eq!(out.to, "fanuc");
    for needle in ["WHILE[", "END1", "#1 = #1 + 1"] {
        assert!(
            out.emitted.contains(needle),
            "Macro B payload потерян при same-dialect: {needle:?} в {:?}",
            out.emitted
        );
    }
}
