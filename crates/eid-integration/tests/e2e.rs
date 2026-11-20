//! E2E smoke, итерация 4 (Агент E): полный цикл через функции конвейера
//! (eid-integration::translate — те же, что вызывает CLI; не через бинарь).
//!
//! Пилот #01: file → translate(grbl→linuxcnc) → файл создан →
//! parse(output) ≡ parse(input) по semantic_diff (EQUIVALENCE §4) → + reverse.
//! Негатив #27: translate(tool_change → grbl-1.1) → Err SYNTAX на emit-стадии
//! (manifest r4_expect, COVERAGE §5) → CLI-маппинг = exit 1.

use eid_core::adapter::Adapter;
use eid_harness::diff::{semantic_diff, DiffConfig};
use eid_integration::profile::MachineProfile;
use eid_integration::sim::mock_sim;
use eid_integration::translate::{
    format_diagnostic, translate_file, translate_text, AnyAdapter, TranslateError,
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
    let mut a = AnyAdapter::for_name(dialect).expect("адаптер диалекта");
    let p = a.parse(src, file).expect("parse должен пройти");
    let _ = a.take_notices();
    p
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
    translate_file("grbl-1.1", "linuxcnc", &corpus_pilot("01_linear_box", "linear_box.grbl.ngc"), &e2e_dir().join("pilot01.fwd.linuxcnc.ngc"))
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
    assert!(program.ops.len() > 0);

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
    let src = std::fs::read_to_string(corpus_pilot("01_linear_box", "linear_box.grbl.ngc")).unwrap();
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
    let src = std::fs::read_to_string(corpus_pilot("01_linear_box", "linear_box.grbl.ngc")).unwrap();
    let err = translate_text("grbl-1.1", "grbl-hal", &src, "x.grbl.ngc")
        .expect_err("grbl-hal — pending");
    assert!(matches!(err, TranslateError::AdapterPending { .. }));
    assert_eq!(err.stage(), "config");
}
