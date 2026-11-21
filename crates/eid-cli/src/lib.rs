//! eid-cli — библиотечная логика CLI (Агент E). Бинарь — тонкая обёртка
//! (src/main.rs); тесты exit-матрицы — tests/cli_matrix.rs через [`run`].

use eid_core::adapter::Adapter;
use eid_integration::profile::MachineProfile;
use eid_integration::sim::{check_program, real_sim_available, validate_program};
use eid_integration::translate::{
    format_diagnostic, translate_file, AnyAdapter, TranslateError,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const DIALECT_NAMES: &str = "grbl-1.1 (алиас: grbl), linuxcnc, eid-l1; pending: grbl-hal, fluidnc";

const TRANSLATE_FLAGS: &[&str] = &["--from", "--to", "--profile", "--input", "--output"];
const VERIFY_FLAGS: &[&str] = &["--profile", "--input"];

/// Класс ошибки → код выхода: 2 — usage (аргументы), 1 — конфигурация/файлы/
/// трансляция.
#[derive(Debug)]
pub enum CliError {
    Usage(String),
    Failure(String),
}

pub type CliResult<T> = Result<T, CliError>;

/// Точка входа CLI: аргументы (без argv[0]) → код выхода.
pub fn run(args: Vec<String>) -> ExitCode {
    match dispatch_exit(&args) {
        0 => ExitCode::SUCCESS,
        2 => ExitCode::from(2),
        _ => ExitCode::from(1),
    }
}

/// Класс исхода в числах: 0 ok / 1 failure / 2 usage (exit-матрица, тесты).
pub fn dispatch_exit(args: &[String]) -> u8 {
    match dispatch(args) {
        Ok(()) => 0,
        Err(CliError::Usage(_)) => 2,
        Err(CliError::Failure(_)) => 1,
    }
}

pub fn dispatch(args: &[String]) -> CliResult<()> {
    match args.first().map(String::as_str) {
        None => {
            print_main_help();
            Ok(())
        }
        Some("--help" | "-h" | "help") => {
            print_main_help();
            Ok(())
        }
        Some("--version" | "-V") => {
            print_version();
            Ok(())
        }
        Some("translate") => cmd_translate(&args[1..]),
        Some("verify") => cmd_verify(&args[1..]),
        Some(other) => Err(CliError::Usage(format!(
            "неизвестная команда `{other}` (доступно: translate, verify; справка: eid --help)"
        ))),
    }
}

// ---------------------------------------------------------------------------
// translate (live)
// ---------------------------------------------------------------------------

fn cmd_translate(args: &[String]) -> CliResult<()> {
    if wants_help(args) {
        print_translate_help();
        return Ok(());
    }
    let flags = parse_flags(args, TRANSLATE_FLAGS)?;
    let from = parse_dialect(require_flag(&flags, "--from")?, "--from")?;
    let to = parse_dialect(require_flag(&flags, "--to")?, "--to")?;
    let profile_path = PathBuf::from(require_flag(&flags, "--profile")?);
    let input = PathBuf::from(require_flag(&flags, "--input")?);
    let output = PathBuf::from(require_flag(&flags, "--output")?);

    let profile = load_profile(&profile_path)?;
    check_input_file(&input)?;
    check_output_file(&output, &input)?;

    if profile.dialect.as_name() != from {
        eprintln!(
            "note: диалект профиля ({}) не совпадает с --from ({}); профиль применён как задан",
            profile.dialect.as_name(),
            from
        );
    }

    match translate_file(&from, &to, &input, &output) {
        Ok(outcome) => {
            println!(
                "translate: OK {} -> {} ({} опов)",
                outcome.from,
                outcome.to,
                outcome.program.ops.len()
            );
            for n in &outcome.notices {
                println!("  notice: {n}");
            }
            for w in &outcome.emit_warnings {
                eprintln!("  warning: {}", format_diagnostic(w));
            }
            for w in &outcome.q3_warnings {
                eprintln!("  warning: {w}");
            }
            println!(
                "  output: {} ({} bytes)",
                output.display(),
                std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0)
            );
            print_profile(&profile, &profile_path);
            Ok(())
        }
        Err(err) => Err(report_translate_error(&err)),
    }
}

/// Печать отказа трансляции (диагностики: stage + code + message + src) → exit 1.
fn report_translate_error(err: &TranslateError) -> CliError {
    match err {
        TranslateError::Parse(diags) | TranslateError::Emit(diags) => {
            eprintln!(
                "translate: отказ на стадии {} ({} диагностики)",
                err.stage(),
                diags.len()
            );
            for d in diags {
                eprintln!("  {}", format_diagnostic(d));
            }
        }
        TranslateError::AdapterPending { message, .. } => {
            eprintln!("translate: {message}");
        }
        TranslateError::UnknownDialect(d) => {
            eprintln!("translate: неизвестный диалект `{d}`");
        }
        TranslateError::Io(m) => {
            eprintln!("translate: io: {m}");
        }
    }
    CliError::Failure(format!("translate: отказ на стадии {}", err.stage()))
}

// ---------------------------------------------------------------------------
// verify (parse + validate + mock-sim)
// ---------------------------------------------------------------------------

fn cmd_verify(args: &[String]) -> CliResult<()> {
    if wants_help(args) {
        print_verify_help();
        return Ok(());
    }
    let flags = parse_flags(args, VERIFY_FLAGS)?;
    let profile_path = PathBuf::from(require_flag(&flags, "--profile")?);
    let input = PathBuf::from(require_flag(&flags, "--input")?);

    let profile = load_profile(&profile_path)?;
    check_input_file(&input)?;
    let src = std::fs::read_to_string(&input)
        .map_err(|e| CliError::Failure(format!("input {}: {e}", input.display())))?;
    let file_name = input
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| input.display().to_string());

    // 1. parse адаптером диалекта профиля
    let mut adapter = match AnyAdapter::for_name(profile.dialect.as_name()) {
        Ok(a) => a,
        Err(err) => return Err(report_translate_error(&err)),
    };
    let program = match adapter.parse(&src, &file_name) {
        Ok(p) => p,
        Err(diags) => {
            eprintln!("verify: parse отверг вход ({} диагностики)", diags.len());
            for d in &diags {
                eprintln!("  {}", format_diagnostic(d));
            }
            return Err(CliError::Failure("verify: parse".to_string()));
        }
    };
    let notices = adapter.take_notices();

    println!(
        "verify: parse OK, {} опов ({})",
        program.ops.len(),
        profile.dialect.as_name()
    );
    for n in &notices {
        println!("  notice: {n}");
    }

    // 2. валидация AST (eid_core::parser::validate)
    if let Err(diags) = validate_program(&program) {
        eprintln!(
            "verify: validate отверг программу ({} диагностики)",
            diags.len()
        );
        for d in &diags {
            eprintln!("  {}", format_diagnostic(d));
        }
        return Err(CliError::Failure("verify: validate".to_string()));
    }

    // 3. structural mock-sim (критерий #8 — mock с пометкой)
    let report = check_program(&profile, &program);
    match &report.bbox {
        Some(b) => println!(
            "  mock-sim: bbox x[{:.4}, {:.4}] y[{:.4}, {:.4}] z[{:.4}, {:.4}]",
            b.min[0], b.max[0], b.min[1], b.max[1], b.min[2], b.max[2]
        ),
        None => println!("  mock-sim: motion-опов нет, bbox пуст"),
    }
    for w in &report.warnings {
        println!("  warning: {w}");
    }
    if !report.is_ok() {
        eprintln!(
            "verify: mock-sim отверг программу ({} ошибок)",
            report.errors.len()
        );
        for e in &report.errors {
            eprintln!("  error: {e}");
        }
        return Err(CliError::Failure("verify: mock-sim".to_string()));
    }

    // 4. real sim — наличие в системе (запуск — решение после MVP)
    if real_sim_available() {
        println!("  real sim: linuxcnc/rs274 обнаружен в системе (интеграция запуска — после MVP)");
    } else {
        println!("  real sim: linuxcnc/rs274 не обнаружен — mock-only MVP (REPORT, PLAN §2-E)");
    }

    println!("verify: OK ({} опов; mock-sim 0 ошибок)", report.ops_count);
    print_profile(&profile, &profile_path);
    Ok(())
}

// ---------------------------------------------------------------------------
// Разбор флагов (std, без clap)
// ---------------------------------------------------------------------------

/// Разбор `--flag value`-пар. Значение — отдельный токен или `--flag=value`.
/// Неизвестный флаг, дубликат, пропущенное/пустое значение, лишний
/// позиционный токен — Usage-ошибка.
fn parse_flags(args: &[String], allowed: &[&str]) -> CliResult<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let tok = args[i].as_str();
        if !tok.starts_with("--") {
            return Err(CliError::Usage(format!(
                "лишний позиционный аргумент `{tok}` (ожидался флаг из: {})",
                allowed.join(", ")
            )));
        }
        let (name, inline_value) = match tok.split_once('=') {
            Some((n, v)) => (n.to_string(), Some(v.to_string())),
            None => (tok.to_string(), None),
        };
        if !allowed.contains(&name.as_str()) {
            return Err(CliError::Usage(format!(
                "неизвестный флаг `{name}` (допустимо: {})",
                allowed.join(", ")
            )));
        }
        if out.iter().any(|(n, _)| *n == name) {
            return Err(CliError::Usage(format!(
                "флаг `{name}` указан более одного раза"
            )));
        }
        let value = match inline_value {
            Some(v) => v,
            None => match args.get(i + 1) {
                Some(v) if !v.starts_with("--") => {
                    i += 1;
                    v.clone()
                }
                _ => {
                    return Err(CliError::Usage(format!(
                        "флагу `{name}` не передано значение"
                    )))
                }
            },
        };
        if value.is_empty() {
            return Err(CliError::Usage(format!(
                "флаг `{name}` получил пустое значение"
            )));
        }
        out.push((name, value));
        i += 1;
    }
    Ok(out)
}

fn get_flag<'a>(flags: &'a [(String, String)], name: &str) -> Option<&'a str> {
    flags
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

fn require_flag<'a>(flags: &'a [(String, String)], name: &str) -> CliResult<&'a str> {
    get_flag(flags, name)
        .ok_or_else(|| CliError::Usage(format!("отсутствует обязательный флаг `{name}`")))
}

fn wants_help(args: &[String]) -> bool {
    args.iter().any(|a| a == "--help" || a == "-h")
}

/// Канонические имена диалектов registry; `grbl` — алиас `grbl-1.1`
/// (короткая форма из IMPLEMENTATION-PLAN §5, критерий #9).
fn parse_dialect(s: &str, flag: &str) -> CliResult<String> {
    const KNOWN: &[&str] = &["grbl-1.1", "grbl-hal", "fluidnc", "linuxcnc", "eid-l1"];
    let canonical = if s == "grbl" { "grbl-1.1" } else { s };
    if KNOWN.contains(&canonical) {
        Ok(canonical.to_string())
    } else {
        Err(CliError::Usage(format!(
            "флаг {flag}: неизвестный диалект `{s}` (доступно: {DIALECT_NAMES})"
        )))
    }
}

// ---------------------------------------------------------------------------
// Валидация конфигурации
// ---------------------------------------------------------------------------

fn load_profile(path: &Path) -> CliResult<MachineProfile> {
    MachineProfile::load(path).map_err(|e| CliError::Failure(format!("профиль: {e}")))
}

fn check_input_file(path: &Path) -> CliResult<()> {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => Ok(()),
        Ok(_) => Err(CliError::Failure(format!(
            "input {} не является обычным файлом",
            path.display()
        ))),
        Err(e) => Err(CliError::Failure(format!(
            "input {} недоступен: {e}",
            path.display()
        ))),
    }
}

fn check_output_file(output: &Path, input: &Path) -> CliResult<()> {
    if paths_equal(output, input) {
        return Err(CliError::Failure(format!(
            "output совпадает с input ({})",
            output.display()
        )));
    }
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        if !parent.is_dir() {
            return Err(CliError::Failure(format!(
                "каталог для output не существует: {}",
                parent.display()
            )));
        }
    }
    if let Ok(m) = std::fs::metadata(output) {
        if !m.is_file() {
            return Err(CliError::Failure(format!(
                "output {} существует и не является файлом",
                output.display()
            )));
        }
    }
    Ok(())
}

/// Лексическое сравнение путей; при существовании обоих — canonicalize
/// (ловит регистронезависимость Windows и относительные пути).
fn paths_equal(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

fn print_profile(profile: &MachineProfile, path: &Path) {
    println!("  profile:         {}", path.display());
    println!("    name:          \"{}\"", profile.name);
    println!("    dialect:       {}", profile.dialect.as_name());
    let l = &profile.limits;
    println!(
        "    limits:        max_ops_per_program={}, max_unroll_iterations={}, max_source_lines={}, max_word_chars={}",
        l.max_ops_per_program, l.max_unroll_iterations, l.max_source_lines, l.max_word_chars
    );
    let r_note = if profile.emit_r_arcs {
        "R-эмиссия включена профилем (эмиссия с notice)"
    } else {
        "всегда IJK — EQUIVALENCE §4"
    };
    println!("    emit_r_arcs:   {} ({})", profile.emit_r_arcs, r_note);
    match &profile.axis_limits {
        Some(a) => println!(
            "    axis_limits:   x[{}, {}] y[{}, {}] z[{}, {}]",
            a.x.min, a.x.max, a.y.min, a.y.max, a.z.min, a.z.max
        ),
        None => println!("    axis_limits:   не заданы (проверка отключена)"),
    }
}

// ---------------------------------------------------------------------------
// Справка
// ---------------------------------------------------------------------------

fn print_version() {
    println!(
        "eid {} (interface v0.1, итерация 5 — Агент E)",
        env!("CARGO_PKG_VERSION")
    );
}

fn print_main_help() {
    print!(
        "\
eid — транслятор G-кода через канонический формат EID/L1 (GRBL 1.1 <-> LinuxCNC)

ИСПОЛЬЗОВАНИЕ:
  eid translate --from <dialect> --to <dialect> --profile <toml> --input <file> --output <file>
  eid verify --input <file> --profile <toml>
  eid --help | --version

КОМАНДЫ:
  translate   Перевод файла между диалектами через EID/L1 (live: реальные адаптеры)
  verify      Parse + validate + mock-sim structural-проверки профиля

ДИАЛЕКТЫ:
  {DIALECT_NAMES}

ОПЦИИ:
  --from <dialect>    Диалект источника (translate)
  --to <dialect>      Диалект цели (translate)
  --profile <toml>    MachineProfile — TOML (примеры: crates/eid-integration/profiles/)
  --input <file>      Входной файл G-кода
  --output <file>     Выходной файл (translate)

СТАТУС (итерация 5, Агент E):
  translate: live для grbl-1.1, linuxcnc, eid-l1 (реальные адаптеры C/D/A).
  grbl-hal/fluidnc — pending (Агент C). verify: parse + validate + mock-sim
  (structural-проверки: program_end, zero-length, axis_limits профиля).
  real sim (rs274ngc): если не обнаружен в системе — mock-only MVP (REPORT).

КОДЫ ВЫХОДА:
  0  успех
  1  ошибка конфигурации/файлов или отказ трансляции (диагностики в stderr)
  2  ошибка аргументов (usage)
"
    );
}

fn print_translate_help() {
    print!(
        "\
eid translate — перевод G-кода между диалектами через EID/L1

ИСПОЛЬЗОВАНИЕ:
  eid translate --from <dialect> --to <dialect> --profile <toml> --input <file> --output <file>

ФЛАГИ (все обязательны):
  --from <dialect>    Диалект источника: {DIALECT_NAMES}
  --to <dialect>      Диалект цели (тот же список)
  --profile <toml>    MachineProfile (см. crates/eid-integration/profiles/*.toml)
  --input <file>      Входной файл G-кода (существует, читается)
  --output <file>     Выходной файл (не совпадает с input; каталог должен существовать)

ПРИМЕР:
  eid translate --from grbl --to linuxcnc --profile crates/eid-integration/profiles/grbl_default.toml --input corpus/tier_a/v0/01_linear_box/linear_box.grbl.ngc --output pocket.ngc

СТАТУС (итерация 4): live — реальные адаптеры grbl-1.1/linuxcnc/eid-l1 через
registry (eid-integration::translate). Отказы parse/emit — диагностики в
stderr ([stage] code + message + src), exit 1. drop-notices (SPEC §3) и
emit-warnings (Q3) печатаются отдельно (notice/warning, не отказ).

КОДЫ ВЫХОДА: 0 — перевод выполнен; 1 — профиль/файлы/отказ трансляции; 2 — usage.
"
    );
}

fn print_verify_help() {
    print!(
        "\
eid verify — проверка программы G-кода: parse + validate + mock-sim

ИСПОЛЬЗОВАНИЕ:
  eid verify --input <file> --profile <toml>

ФЛАГИ (оба обязательны):
  --profile <toml>    MachineProfile (см. crates/eid-integration/profiles/*.toml);
                      диалект профиля определяет адаптер обратного parse
  --input <file>      Входной файл G-кода (существует, читается)

ПРОВЕРКИ: parse адаптером диалекта профиля; eid_core::parser::validate;
mock-sim structural-проверки (program_end presence, zero-length flood,
axis_limits профиля). Наличие real sim (linuxcnc/rs274) — детекция и отчёт;
запуск — решение после MVP (mock-only MVP, PLAN §2-E).

КОДЫ ВЫХОДА: 0 — проверка пройдена; 1 — профиль/файлы/отказ проверок; 2 — usage.
"
    );
}
