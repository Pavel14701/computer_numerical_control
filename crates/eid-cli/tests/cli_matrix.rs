//! CLI exit-матрица + translate/verify live-матрица (Агент E, итерация 5).
//! Именование cli_<case>. Все кейсы через `eid_cli::dispatch_exit`/`run` —
//! реальный CLI-путь. Чек-лист классов — шапка src/main.rs.
//!
//! Замечание о stdout/stderr: `run` печатает в реальный stdout/stderr
//! (перехвата в-process нет) — содержимое сообщений проверяется на уровне
//! eid-integration (tests/e2e.rs: format_diagnostic), здесь — коды выхода,
//! существование файлов и семантика (semantic_diff).

use eid_harness::diff::{semantic_diff, DiffConfig};
use eid_integration::translate::parse_with;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Фикстуры
// ---------------------------------------------------------------------------

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

fn grbl_profile() -> PathBuf {
    repo_root().join("crates/eid-integration/profiles/grbl_default.toml")
}

fn lnx_profile() -> PathBuf {
    repo_root().join("crates/eid-integration/profiles/linuxcnc_default.toml")
}

fn tmp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eid_cli_matrix_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

fn write_tmp(name: &str, content: &str) -> PathBuf {
    let p = tmp().join(name);
    std::fs::write(&p, content).expect("write tmp");
    p
}

/// &str из связанной локальной переменной-пути (литералы массива аргументов
/// не должны заимствовать временные PathBuf — E0716).
fn p(path: &Path) -> &str {
    path.to_str().expect("utf8 path")
}

fn code_of(args: &[&str]) -> u8 {
    let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    eid_cli::dispatch_exit(&v)
}

// ---------------------------------------------------------------------------
// Exit-матрица: usage → 2
// ---------------------------------------------------------------------------

#[test]
fn cli_unknown_command_usage() {
    assert_eq!(code_of(&["frobnicate"]), 2);
}

#[test]
fn cli_unknown_dialect_usage() {
    // "okuma" вне словаря CLI → usage (2). fanuc/haas — имена валидны и LIVE
    // (см. cli_dir_grbl_to_fanuc_live / cli_dir_grbl_to_haas_live); pending
    // остались только grbl-hal/fluidnc → exit 1 (cli_adapter_pending_grbl_hal).
    let prof = grbl_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let output = tmp().join("ud.out");
    let code = code_of(&[
        "translate",
        "--from",
        "okuma",
        "--to",
        "linuxcnc",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 2);
}

#[test]
fn cli_dir_grbl_to_fanuc_live() {
    // #25 закрыт: fanuc-цель LIVE (адаптер eid-fanuc в registry) — Raw-free
    // grbl-источник переводится, выход — легальная fanuc-программа.
    // Полная semantic-эквивалентность — на уровне eid-integration (tests/
    // e2e.rs: exact_stop без value-shift); здесь — живость пути CLI.
    let prof = grbl_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let output = tmp().join("fanuc_live.out");
    let _ = std::fs::remove_file(&output);
    let code = code_of(&[
        "translate",
        "--from",
        "grbl",
        "--to",
        "fanuc",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 0, "fanuc-цель live: translate должен завершиться 0");
    let meta = std::fs::metadata(&output).expect("output файл создан");
    assert!(meta.len() > 0, "output пуст");
    let emitted = std::fs::read_to_string(&output).unwrap();
    assert!(emitted.contains("M30"), "program_end fanuc — M30: {emitted:?}");
    // выход разбирается fanuc-адаптером (registry-имя "fanuc")
    parse_with("fanuc", &emitted, "out.fanuc.nc").expect("выход парсится fanuc");
}

#[test]
fn cli_fanuc_raw_cross_refuses() {
    // RAW-правило (SPEC §8.2): ok-файл tier_b с Macro B (Raw-опы) не
    // переводится кросс-диалектно — честный E_RAW_UNTRANSLATABLE, exit 1,
    // output не создаётся. same-dialect fanuc→fanuc — verbatim (см.
    // eid-integration tests/e2e.rs).
    let prof = repo_root().join("crates/eid-integration/profiles/fanuc_default.toml");
    let input = repo_root().join("corpus/tier_b/inbox/mpe_while_clear_variables.nc");
    let output = tmp().join("fanuc_raw.out");
    let _ = std::fs::remove_file(&output);
    let code = code_of(&[
        "translate",
        "--from",
        "fanuc",
        "--to",
        "grbl",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 1, "кросс-диалект с Raw — отказ трансляции");
    assert!(!output.exists(), "отказ не пишет output");
}

#[test]
fn cli_dir_fanuc_same_dialect_live() {
    // same-dialect fanuc→fanuc: RAW вербатимен (SPEC §8.1) — live путь exit 0.
    let prof = repo_root().join("crates/eid-integration/profiles/fanuc_default.toml");
    let input = repo_root().join("corpus/tier_b/inbox/mpe_while_clear_variables.nc");
    let output = tmp().join("fanuc_same.out");
    let _ = std::fs::remove_file(&output);
    let code = code_of(&[
        "translate",
        "--from",
        "fanuc",
        "--to",
        "fanuc",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 0, "same-dialect RAW легален");
    let emitted = std::fs::read_to_string(&output).unwrap();
    assert!(emitted.contains("WHILE["), "Macro B payload verbatim: {emitted:?}");
}

#[test]
fn cli_dir_grbl_to_haas_live() {
    // Регистрация "haas" (этап 3, agent C): haas-цель LIVE (адаптер eid-haas
    // в registry) — Raw-free grbl-источник переводится, выход — легальная
    // haas-программа (по образцу cli_dir_grbl_to_fanuc_live #25). Полная
    // semantic-эквивалентность — eid-integration tests/e2e.rs; здесь —
    // живость пути CLI + Haas-специфика заголовка (G91.1 НЕТ в словаре).
    let prof = grbl_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let output = tmp().join("haas_live.out");
    let _ = std::fs::remove_file(&output);
    let code = code_of(&[
        "translate",
        "--from",
        "grbl",
        "--to",
        "haas",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 0, "haas-цель live: translate должен завершиться 0");
    let meta = std::fs::metadata(&output).expect("output файл создан");
    assert!(meta.len() > 0, "output пуст");
    let emitted = std::fs::read_to_string(&output).unwrap();
    assert!(emitted.contains("M30"), "program_end haas — M30: {emitted:?}");
    assert!(
        !emitted.contains("G91.1"),
        "G91.1 в словаре Haas отсутствует — заголовок без него: {emitted:?}"
    );
    // выход разбирается haas-адаптером (registry-имя "haas")
    parse_with("haas", &emitted, "out.haas.ngc").expect("выход парсится haas");
}

#[test]
fn cli_haas_raw_cross_refuses() {
    // RAW-правило (SPEC §8.2): Macro B — живой диалект Haas тоже (строки с
    // `#`-переменными парсятся как line-level Raw) — кросс-диалект честно
    // E_RAW_UNTRANSLATABLE, exit 1; same-dialect verbatim — отдельный тест.
    // Источник — fanuc tier_b файл с Macro B: Haas Macro B Fanuc-совместим
    // (анализ §4.3), лексер Haas даёт те же Raw-опы.
    let prof = repo_root().join("crates/eid-integration/profiles/haas_default.toml");
    let input = repo_root().join("corpus/tier_b/inbox/mpe_while_clear_variables.nc");
    let output = tmp().join("haas_raw.out");
    let _ = std::fs::remove_file(&output);
    let code = code_of(&[
        "translate",
        "--from",
        "haas",
        "--to",
        "grbl",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 1, "кросс-диалект с Raw — отказ трансляции");
    assert!(!output.exists(), "отказ не пишет output");
}

#[test]
fn cli_dir_haas_same_dialect_live() {
    // same-dialect haas→haas: RAW вербатимен (SPEC §8.1) — live путь exit 0.
    let prof = repo_root().join("crates/eid-integration/profiles/haas_default.toml");
    let input = repo_root().join("corpus/tier_b/inbox/mpe_while_clear_variables.nc");
    let output = tmp().join("haas_same.out");
    let _ = std::fs::remove_file(&output);
    let code = code_of(&[
        "translate",
        "--from",
        "haas",
        "--to",
        "haas",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 0, "same-dialect RAW легален");
    let emitted = std::fs::read_to_string(&output).unwrap();
    assert!(
        emitted.contains("WHILE"),
        "Macro B payload verbatim: {emitted:?}"
    );
}

#[test]
fn cli_missing_flag_usage() {
    let prof = grbl_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    // --output пропущен
    let code = code_of(&[
        "translate",
        "--from",
        "grbl",
        "--to",
        "linuxcnc",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
    ]);
    assert_eq!(code, 2);
}

#[test]
fn cli_duplicate_flag_usage() {
    let prof = grbl_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let output = tmp().join("dup.out");
    let code = code_of(&[
        "translate",
        "--from",
        "grbl",
        "--from",
        "linuxcnc",
        "--to",
        "linuxcnc",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 2);
}

#[test]
fn cli_unknown_flag_usage() {
    let prof = lnx_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let code = code_of(&[
        "verify",
        "--input",
        p(&input),
        "--profile",
        p(&prof),
        "--verbose",
    ]);
    assert_eq!(code, 2);
}

#[test]
fn cli_positional_arg_usage() {
    let prof = lnx_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let code = code_of(&[
        "verify",
        "stray",
        "--input",
        p(&input),
        "--profile",
        p(&prof),
    ]);
    assert_eq!(code, 2);
}

// ---------------------------------------------------------------------------
// Exit-матрица: ok → 0
// ---------------------------------------------------------------------------

#[test]
fn cli_no_args_help_ok() {
    let v: Vec<String> = Vec::new();
    assert_eq!(eid_cli::dispatch_exit(&v), 0);
}

#[test]
fn cli_help_flag_ok() {
    assert_eq!(code_of(&["--help"]), 0);
    assert_eq!(code_of(&["help"]), 0);
    assert_eq!(code_of(&["translate", "-h"]), 0);
    assert_eq!(code_of(&["verify", "--help"]), 0);
}

#[test]
fn cli_version_flag_ok() {
    assert_eq!(code_of(&["--version"]), 0);
}

// ---------------------------------------------------------------------------
// Exit-матрица: fail → 1 (конфигурация/файлы)
// ---------------------------------------------------------------------------

#[test]
fn cli_missing_input_file() {
    let prof = grbl_profile();
    let missing = tmp().join("no_such_input.ngc");
    let output = tmp().join("whatever.out");
    let code = code_of(&[
        "translate",
        "--from",
        "grbl",
        "--to",
        "linuxcnc",
        "--profile",
        p(&prof),
        "--input",
        p(&missing),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 1);
}

#[test]
fn cli_bad_profile_toml() {
    let bad = write_tmp("bad_profile.toml", "name = \"незакрытая\n");
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let output = tmp().join("bp.out");
    let code = code_of(&[
        "translate",
        "--from",
        "grbl",
        "--to",
        "linuxcnc",
        "--profile",
        p(&bad),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 1);
}

#[test]
fn cli_output_equals_input() {
    let prof = grbl_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let code = code_of(&[
        "translate",
        "--from",
        "grbl",
        "--to",
        "grbl",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&input),
    ]);
    assert_eq!(code, 1);
}

#[test]
fn cli_output_dir_missing() {
    let prof = grbl_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let output = tmp().join("no_dir_here").join("out.ngc");
    let code = code_of(&[
        "translate",
        "--from",
        "grbl",
        "--to",
        "linuxcnc",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 1);
}

#[test]
fn cli_adapter_pending_grbl_hal() {
    let prof = grbl_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let output = tmp().join("hal.out");
    let code = code_of(&[
        "translate",
        "--from",
        "grbl",
        "--to",
        "grbl-hal",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 1);
}

#[test]
fn cli_profile_dialect_mismatch_note_ok() {
    // профиль GRBL, --from linuxcnc → note в stderr, перевод выполняется (0)
    let prof = grbl_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.linuxcnc.ngc");
    let output = tmp().join("mismatch.out");
    let _ = std::fs::remove_file(&output);
    let code = code_of(&[
        "translate",
        "--from",
        "linuxcnc",
        "--to",
        "grbl",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 0);
    assert!(output.exists());
}

// ---------------------------------------------------------------------------
// Translate live-матрица: 6 направлений на пилоте #01 (файл + parse-back ≡)
// ---------------------------------------------------------------------------

const GRBL_SRC: (&str, &str) = ("01_linear_box", "linear_box.grbl.ngc");
const LNX_SRC: (&str, &str) = ("01_linear_box", "linear_box.linuxcnc.ngc");
const EID_SRC: (&str, &str) = ("01_linear_box", "golden.eid");

/// Одно направление translate: exit 0, файл создан непустой,
/// parse(output, to) ≡ parse(input, from) по semantic_diff.
fn assert_direction(from: &str, to: &str, input: &Path, profile: &Path, tag: &str) {
    let output = tmp().join(format!("{tag}.out"));
    let _ = std::fs::remove_file(&output);
    let code = code_of(&[
        "translate",
        "--from",
        from,
        "--to",
        to,
        "--profile",
        p(profile),
        "--input",
        p(input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 0, "{tag}: translate должен завершиться 0");
    let meta = std::fs::metadata(&output).unwrap_or_else(|_| panic!("{tag}: output файл создан"));
    assert!(meta.len() > 0, "{tag}: output пуст");

    let a = parse_with(from, &std::fs::read_to_string(input).unwrap(), "input")
        .unwrap_or_else(|e| panic!("{tag}: parse источника: {e:?}"));
    let b = parse_with(to, &std::fs::read_to_string(&output).unwrap(), "output")
        .unwrap_or_else(|e| panic!("{tag}: parse-back результата: {e:?}"));
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    assert!(d.is_empty(), "{tag}: semantic_diff не пуст: {d:?}");
}

#[test]
fn cli_dir_grbl_to_linuxcnc() {
    let (d, f) = GRBL_SRC;
    assert_direction(
        "grbl-1.1",
        "linuxcnc",
        &corpus_pilot(d, f),
        &grbl_profile(),
        "g2l",
    );
}

#[test]
fn cli_dir_linuxcnc_to_grbl() {
    let (d, f) = LNX_SRC;
    assert_direction(
        "linuxcnc",
        "grbl-1.1",
        &corpus_pilot(d, f),
        &lnx_profile(),
        "l2g",
    );
}

#[test]
fn cli_dir_grbl_to_eidl1() {
    let (d, f) = GRBL_SRC;
    assert_direction(
        "grbl-1.1",
        "eid-l1",
        &corpus_pilot(d, f),
        &grbl_profile(),
        "g2e",
    );
}

#[test]
fn cli_dir_linuxcnc_to_eidl1() {
    let (d, f) = LNX_SRC;
    assert_direction(
        "linuxcnc",
        "eid-l1",
        &corpus_pilot(d, f),
        &lnx_profile(),
        "l2e",
    );
}

#[test]
fn cli_dir_eidl1_to_grbl() {
    let (d, f) = EID_SRC;
    assert_direction(
        "eid-l1",
        "grbl-1.1",
        &corpus_pilot(d, f),
        &grbl_profile(),
        "e2g",
    );
}

#[test]
fn cli_dir_eidl1_to_linuxcnc() {
    let (d, f) = EID_SRC;
    assert_direction(
        "eid-l1",
        "linuxcnc",
        &corpus_pilot(d, f),
        &lnx_profile(),
        "e2l",
    );
}

// ---------------------------------------------------------------------------
// Негатив #27: Err SYNTAX на emit-стадии → exit 1, output НЕ создан
// ---------------------------------------------------------------------------

#[test]
fn cli_27_negative_emit_syntax() {
    let prof = lnx_profile();
    let input = corpus_pilot("27_tool_change", "tool_change.linuxcnc.ngc");
    let output = tmp().join("t27_cli.grbl.ngc");
    let _ = std::fs::remove_file(&output);
    let code = code_of(&[
        "translate",
        "--from",
        "linuxcnc",
        "--to",
        "grbl",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 1, "#27: emit-отказ → exit 1");
    assert!(!output.exists(), "#27: output не должен создаваться");
}

// ---------------------------------------------------------------------------
// Verify-матрица: axis_limits в пределах / за min / за max / частично / None
// ---------------------------------------------------------------------------

fn axis_profile(x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> PathBuf {
    // уникальное имя: тесты идут параллельно в одном процессе
    let name = format!(
        "axis_x{}-{}_y{}-{}_z{}-{}.toml",
        x.0, x.1, y.0, y.1, z.0, z.1
    )
    .replace(['.', '-'], "_")
    .replace("_toml", ".toml");
    // собрано: axis_x.._y.._z....toml (минусы/точки заменены)
    let name = name.replace("_toml_", "_");
    write_tmp(
        &name,
        &format!(
            "name = \"Tight bench\"\ndialect = \"linuxcnc\"\nemit-r-arcs = false\n\n\
             [axis-limits]\n\
             x = {{ min = {}, max = {} }}\n\
             y = {{ min = {}, max = {} }}\n\
             z = {{ min = {}, max = {} }}\n\n\
             [limits]\n\
             max-ops-per-program = 200000\n\
             max-unroll-iterations = 10000\n\
             max-source-lines = 1000000\n\
             max-word-chars = 32\n",
            x.0, x.1, y.0, y.1, z.0, z.1
        ),
    )
}

#[test]
fn cli_verify_axis_within_ok() {
    // bbox пилота: x[0,10] y[0,10] z[5,5] — внутри
    let prof = axis_profile((-100.0, 100.0), (0.0, 300.0), (0.0, 150.0));
    let input = corpus_pilot("01_linear_box", "linear_box.linuxcnc.ngc");
    assert_eq!(
        code_of(&["verify", "--profile", p(&prof), "--input", p(&input)]),
        0
    );
}

#[test]
fn cli_verify_axis_out_max() {
    // z[5,5] против z max 4 → [verify] axis_limits → exit 1
    let prof = axis_profile((-100.0, 100.0), (0.0, 300.0), (0.0, 4.0));
    let input = corpus_pilot("01_linear_box", "linear_box.linuxcnc.ngc");
    assert_eq!(
        code_of(&["verify", "--profile", p(&prof), "--input", p(&input)]),
        1
    );
}

#[test]
fn cli_verify_axis_out_min() {
    // z[5,5] против z min 6 → за нижнюю границу → exit 1
    let prof = axis_profile((-100.0, 100.0), (0.0, 300.0), (6.0, 150.0));
    let input = corpus_pilot("01_linear_box", "linear_box.linuxcnc.ngc");
    assert_eq!(
        code_of(&["verify", "--profile", p(&prof), "--input", p(&input)]),
        1
    );
}

#[test]
fn cli_verify_axis_partial_profile() {
    // частично установлены (только x) → профиль не грузится (missing y) → 1
    let prof = write_tmp(
        "axis_partial.toml",
        "name = \"Partial\"\ndialect = \"linuxcnc\"\nemit-r-arcs = false\n\n\
         [axis-limits]\n\
         x = { min = -100.0, max = 100.0 }\n\n\
         [limits]\n\
         max-ops-per-program = 200000\n\
         max-unroll-iterations = 10000\n\
         max-source-lines = 1000000\n\
         max-word-chars = 32\n",
    );
    let input = corpus_pilot("01_linear_box", "linear_box.linuxcnc.ngc");
    assert_eq!(
        code_of(&["verify", "--profile", p(&prof), "--input", p(&input)]),
        1
    );
}

#[test]
fn cli_verify_axis_none_off() {
    // None = проверка отключена: координаты далеко за разумными пределами,
    // но axis_limits не заданы → verify проходит (0)
    let wild = write_tmp(
        "wild.ngc",
        "G21 G90 G94 G17\nG0 X99999.0000 Y-99999.0000 Z5000.0000\nG1 X-99999.0000 F100.0\nM2\n",
    );
    let prof = lnx_profile();
    assert_eq!(
        code_of(&["verify", "--profile", p(&prof), "--input", p(&wild)]),
        0,
        "axis_limits None = проверка отключена"
    );
}

// ---------------------------------------------------------------------------
// Verify-матрица: zero-length flood (notice, не отказ), program_end, parse
// ---------------------------------------------------------------------------

#[test]
fn cli_verify_zero_flood_ok() {
    // дублирующий ход → drop-with-notice (SPEC §3) → verify проходит (0)
    let flood = write_tmp(
        "flood.ngc",
        "G21 G90 G94 G17\nG0 X0.0000 Y0.0000 Z5.0000\nG1 X5.0000 F100.0\nG1 X5.0000 F100.0\nM2\n",
    );
    let prof = grbl_profile();
    assert_eq!(
        code_of(&["verify", "--profile", p(&prof), "--input", p(&flood)]),
        0
    );
}

#[test]
fn cli_verify_program_end_autopend_ok() {
    // SPEC v0.16: программа без M2/M30 валидна — canonicalizer auto-PEND +
    // W_MISSING_PROGRAM_END notice; verify проходит (0)
    let no_end = write_tmp(
        "no_end.ngc",
        "G21 G90 G94 G17\nG0 X1.0000 Y1.0000 Z1.0000\nG1 X2.0000 F100.0\n",
    );
    let prof = grbl_profile();
    assert_eq!(
        code_of(&["verify", "--profile", p(&prof), "--input", p(&no_end)]),
        0,
        "auto-PEND (SPEC v0.16): отсутствие program_end — notice, не отказ"
    );
}

#[test]
fn cli_verify_parse_reject_garbage() {
    let bad = write_tmp("garbage.ngc", "G1 X0.0000 Y0.0000\n@@garbage@@\n");
    let prof = grbl_profile();
    assert_eq!(
        code_of(&["verify", "--profile", p(&prof), "--input", p(&bad)]),
        1
    );
}

// ---------------------------------------------------------------------------
// Exit-код повторяемость: каждый кейс матрицы × 2 → тот же код (детерминизм)
// ---------------------------------------------------------------------------

#[test]
fn cli_exit_codes_repeatable_matrix() {
    let prof_g = grbl_profile();
    let prof_l = lnx_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let t27 = corpus_pilot("27_tool_change", "tool_change.linuxcnc.ngc");
    let bad_profile = write_tmp("rep_bad.toml", "name = \"незакрытая\n");
    let missing = tmp().join("rep_missing.ngc");
    let bad_dir = tmp().join("rep_nodir").join("out.ngc");
    let garbage = write_tmp("rep_garbage.ngc", "G1 X0.0000 Y0.0000\n@@garbage@@\n");

    // (ожидаемый код, аргументы) — каждый прогон выполняется дважды
    let case = |expected: u8, args: &[&str]| -> (u8, Vec<String>) {
        (expected, args.iter().map(|s| s.to_string()).collect())
    };
    let out = |name: &str| -> String { p(&tmp().join(name)).to_string() };
    let cases: Vec<(u8, Vec<String>)> = vec![
        case(0, &["--help"]),
        case(0, &[]),
        case(2, &["frobnicate"]),
        case(
            2,
            &[
                "translate",
                "--from",
                "okuma",
                "--to",
                "linuxcnc",
                "--profile",
                p(&prof_g),
                "--input",
                p(&input),
                "--output",
                &out("rep_a.out"),
            ],
        ),
        case(
            0,
            &[
                "translate",
                "--from",
                "grbl",
                "--to",
                "fanuc", // live (регистрация fanuc: адаптер eid-fanuc в registry)
                "--profile",
                p(&prof_g),
                "--input",
                p(&input),
                "--output",
                &out("rep_fanuc.out"),
            ],
        ),
        case(
            0,
            &[
                // #10: --profile не задан → дефолт-профиль по --to
                "translate",
                "--from",
                "grbl",
                "--to",
                "linuxcnc",
                "--input",
                p(&input),
                "--output",
                &out("rep_noprof.out"),
            ],
        ),
        case(
            2,
            &[
                "translate",
                "--from",
                "grbl",
                "--to",
                "linuxcnc",
                "--profile",
                p(&prof_g),
                "--input",
                p(&input), // --output отсутствует
            ],
        ),
        case(
            1,
            &[
                "translate",
                "--from",
                "grbl",
                "--to",
                "linuxcnc",
                "--profile",
                p(&bad_profile),
                "--input",
                p(&input),
                "--output",
                &out("rep_b.out"),
            ],
        ),
        case(
            1,
            &[
                "translate",
                "--from",
                "grbl",
                "--to",
                "linuxcnc",
                "--profile",
                p(&prof_g),
                "--input",
                p(&missing),
                "--output",
                &out("rep_c.out"),
            ],
        ),
        case(
            1,
            &[
                "translate",
                "--from",
                "grbl",
                "--to",
                "grbl",
                "--profile",
                p(&prof_g),
                "--input",
                p(&input),
                "--output",
                p(&input),
            ],
        ),
        case(
            1,
            &[
                "translate",
                "--from",
                "grbl",
                "--to",
                "linuxcnc",
                "--profile",
                p(&prof_g),
                "--input",
                p(&input),
                "--output",
                p(&bad_dir),
            ],
        ),
        case(
            1,
            &[
                "translate",
                "--from",
                "grbl",
                "--to",
                "grbl-hal",
                "--profile",
                p(&prof_g),
                "--input",
                p(&input),
                "--output",
                &out("rep_d.out"),
            ],
        ),
        case(
            1,
            &[
                "translate",
                "--from",
                "linuxcnc",
                "--to",
                "grbl",
                "--profile",
                p(&prof_l),
                "--input",
                p(&t27),
                "--output",
                &out("rep_t27.ngc"),
            ],
        ),
        case(
            1,
            &["verify", "--profile", p(&prof_g), "--input", p(&garbage)],
        ),
        case(
            0,
            &[
                "translate",
                "--from",
                "grbl",
                "--to",
                "linuxcnc",
                "--profile",
                p(&prof_g),
                "--input",
                p(&input),
                "--output",
                &out("rep_ok.out"),
            ],
        ),
    ];

    for (expected, args) in &cases {
        let first = eid_cli::dispatch_exit(args);
        let second = eid_cli::dispatch_exit(args);
        assert_eq!(first, second, "exit-код недетерминирован: {args:?}");
        assert_eq!(first, *expected, "exit-код матрицы изменился: {args:?}");
    }
}

// ---------------------------------------------------------------------------
// #10: --profile опционален (translate → дефолт по --to; verify → --dialect
// или дефолт linuxcnc)
// ---------------------------------------------------------------------------

#[test]
fn cli_translate_no_profile_ok() {
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let output = tmp().join("noprof_t.out");
    let _ = std::fs::remove_file(&output);
    let code = code_of(&[
        "translate",
        "--from",
        "grbl",
        "--to",
        "linuxcnc",
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(
        code, 0,
        "translate без --profile обязан работать (дефолт по --to)"
    );
    assert!(output.exists());
}

#[test]
fn cli_translate_no_profile_grbl_target_ok() {
    // дефолт по целевому диалекту = grbl → grbl_default: linuxcnc-файл → grbl
    let input = corpus_pilot("01_linear_box", "linear_box.linuxcnc.ngc");
    let output = tmp().join("noprof_g.out");
    let _ = std::fs::remove_file(&output);
    let code = code_of(&[
        "translate",
        "--from",
        "linuxcnc",
        "--to",
        "grbl",
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 0);
    assert!(output.exists());
}

#[test]
fn cli_verify_no_profile_ok() {
    // verify без --profile и без --dialect → дефолт linuxcnc
    let input = corpus_pilot("01_linear_box", "linear_box.linuxcnc.ngc");
    assert_eq!(
        code_of(&["verify", "--input", p(&input)]),
        0,
        "verify без --profile обязан работать (дефолт linuxcnc)"
    );
}

#[test]
fn cli_verify_dialect_flag_ok() {
    // --dialect вместо профиля: grbl-файл против grbl-диалекта
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    assert_eq!(
        code_of(&["verify", "--input", p(&input), "--dialect", "grbl"]),
        0,
        "--dialect grbl → дефолт grbl-профиля"
    );
    assert_eq!(
        code_of(&["verify", "--input", p(&input), "--dialect", "linuxcnc"]),
        0,
        "--dialect linuxcnc против grbl-файла: parse может пройти или нет, но не usage"
    );
}

#[test]
fn cli_verify_profile_and_dialect_conflict_usage() {
    let prof = lnx_profile();
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    assert_eq!(
        code_of(&[
            "verify",
            "--input",
            p(&input),
            "--profile",
            p(&prof),
            "--dialect",
            "grbl"
        ]),
        2,
        "--profile и --dialect взаимоисключающие → usage"
    );
}

#[test]
fn cli_verify_unknown_dialect_flag_usage() {
    // "okuma" — вне словаря CLI → usage. "haas" — ВАЛИДНО и LIVE с этапа 3
    // (agent C): verify --dialect haas реально работает (см. тест ниже).
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    assert_eq!(
        code_of(&["verify", "--input", p(&input), "--dialect", "okuma"]),
        2,
        "неизвестный диалект в --dialect → usage"
    );
}

#[test]
fn cli_verify_haas_dialect_live() {
    // Регистрация "haas" (этап 3): verify --dialect haas — живой parse-path
    // (дефолт-профиль haas_default подхватывается по --dialect).
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    assert_eq!(
        code_of(&["verify", "--input", p(&input), "--dialect", "haas"]),
        0,
        "verify --dialect haas — live (exit 0)"
    );
}

// ---------------------------------------------------------------------------
// #2: лимиты профиля реально применяются через CLI (E_LIMIT_EXCEEDED)
// ---------------------------------------------------------------------------

const TINY_OPS_PROFILE_TOML: &str = concat!(
    "name = \"Tiny ops\"\n",
    "dialect = \"linuxcnc\"\n",
    "emit-r-arcs = false\n",
    "\n",
    "[limits]\n",
    "max-ops-per-program = 10\n",
    "max-unroll-iterations = 10000\n",
    "max-source-lines = 1000000\n",
    "max-word-chars = 32\n",
);

/// Программа на 20 linear-опов (+ auto-PEND).
fn program_20_ops() -> PathBuf {
    let mut s = String::from("G21 G90\n");
    for i in 0..20 {
        s.push_str(&format!("G1 X{i} Y{i} F100.0\n"));
    }
    s.push_str("M2\n");
    write_tmp("ops20.ngc", &s)
}

#[test]
fn cli_profile_max_ops_refusal() {
    // #2-критерий: профиль max_ops_per_program=10, программа на 20 опытах →
    // отказ E_LIMIT_EXCEEDED → exit 1.
    let prof = write_tmp("tiny_ops.toml", TINY_OPS_PROFILE_TOML);
    let input = program_20_ops();
    let output = tmp().join("tiny.out");
    let _ = std::fs::remove_file(&output);
    let code = code_of(&[
        "translate",
        "--from",
        "grbl",
        "--to",
        "linuxcnc",
        "--profile",
        p(&prof),
        "--input",
        p(&input),
        "--output",
        p(&output),
    ]);
    assert_eq!(code, 1, "20 опов против лимита 10 → отказ");
    assert!(!output.exists(), "отказ лимита не пишет output");

    // библиотечный уровень: код отказа именно E_LIMIT_EXCEEDED
    let profile: eid_integration::profile::MachineProfile =
        toml::from_str(TINY_OPS_PROFILE_TOML).expect("профиль валиден");
    let err = eid_integration::translate::translate_text_with_profile(
        "grbl-1.1",
        "linuxcnc",
        &std::fs::read_to_string(&input).unwrap(),
        "ops20.ngc",
        Some(&profile),
    )
    .expect_err("тот же отказ на библиотечном уровне");
    let lim = err
        .diagnostics()
        .iter()
        .find(|d| d.code.as_str() == "E_LIMIT_EXCEEDED")
        .expect("E_LIMIT_EXCEEDED в диагностиках");
    assert_eq!(
        lim.limit.as_ref().expect("LimitInfo").threshold,
        "max_ops_per_program"
    );

    // тот же вход с ДЕФОЛТ-профилем (--profile не задан) — проходит (0)
    let output2 = tmp().join("tiny_default.out");
    let _ = std::fs::remove_file(&output2);
    assert_eq!(
        code_of(&[
            "translate",
            "--from",
            "grbl",
            "--to",
            "linuxcnc",
            "--input",
            p(&input),
            "--output",
            p(&output2),
        ]),
        0,
        "дефолт-лимит 200000 пропускает 21 оп"
    );
}

// ---------------------------------------------------------------------------
// #11: emit_preamble профиля → первая строка выходного .ngc
// ---------------------------------------------------------------------------

const PREAMBLE_PROFILE_TOML: &str = concat!(
    "name = \"Preamble bench\"\n",
    "dialect = \"linuxcnc\"\n",
    "emit-r-arcs = false\n",
    "emit-preamble = true\n",
    "\n",
    "[limits]\n",
    "max-ops-per-program = 200000\n",
    "max-unroll-iterations = 10000\n",
    "max-source-lines = 1000000\n",
    "max-word-chars = 32\n",
);

#[test]
fn cli_preamble_on_first_line() {
    let prof = write_tmp("preamble_on.toml", PREAMBLE_PROFILE_TOML);
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let output = tmp().join("preamble_on.out");
    let _ = std::fs::remove_file(&output);
    assert_eq!(
        code_of(&[
            "translate",
            "--from",
            "grbl",
            "--to",
            "linuxcnc",
            "--profile",
            p(&prof),
            "--input",
            p(&input),
            "--output",
            p(&output),
        ]),
        0
    );
    let written = std::fs::read_to_string(&output).unwrap();
    assert!(
        written.starts_with("(GENERATED BY EID — VERIFY IN SIMULATOR BEFORE MACHINING)\n"),
        "первая строка выхода — заголовок #11: {written:?}"
    );
    // семантика не тронута: parse(выход) ≡ parse(вход)
    let a = parse_with("linuxcnc", &written, "out.ngc").expect("parse выхода с преамбулой");
    let b = parse_with(
        "grbl-1.1",
        &std::fs::read_to_string(&input).unwrap(),
        "in.ngc",
    )
    .expect("parse входа");
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    assert!(d.is_empty(), "преамбулa ломает семантику: {d:?}");
}

#[test]
fn cli_preamble_off_no_header() {
    let prof = lnx_profile(); // дефолт: emit-preamble = false
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let output = tmp().join("preamble_off.out");
    let _ = std::fs::remove_file(&output);
    assert_eq!(
        code_of(&[
            "translate",
            "--from",
            "grbl",
            "--to",
            "linuxcnc",
            "--profile",
            p(&prof),
            "--input",
            p(&input),
            "--output",
            p(&output),
        ]),
        0
    );
    let written = std::fs::read_to_string(&output).unwrap();
    assert!(
        !written.contains("GENERATED BY EID"),
        "при emit-preamble = false заголовка нет: {written:?}"
    );
}

#[test]
fn cli_preamble_no_profile_never_writes_header() {
    // дефолт-профиль (#10) всегда с emit_preamble=false
    let input = corpus_pilot("01_linear_box", "linear_box.grbl.ngc");
    let output = tmp().join("preamble_def.out");
    let _ = std::fs::remove_file(&output);
    assert_eq!(
        code_of(&[
            "translate",
            "--from",
            "grbl",
            "--to",
            "linuxcnc",
            "--input",
            p(&input),
            "--output",
            p(&output),
        ]),
        0
    );
    let written = std::fs::read_to_string(&output).unwrap();
    assert!(!written.contains("GENERATED BY EID"));
}
