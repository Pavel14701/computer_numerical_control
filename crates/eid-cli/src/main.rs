//! eid-cli — тонкий бинарь над eid-integration (Агент E).
//!
//! Итерация 2: разбор аргументов без clap (std), валидация конфигурации,
//! --help. Исполнение translate/verify — pending: адаптеры диалектов
//! (Агенты C/D, итерации 2–3) и симулятор (Агент E, итерация 4) ещё не
//! реализованы. Команды проверяют конфигурацию, печатают resolved-параметры
//! и завершаются без паники (стабы Adapter в eid-grbl/eid-linuxcnc не
//! вызываются и не изменяются — freeze v0.1-interface).

use eid_integration::profile::{DialectKind, MachineProfile};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const DIALECT_NAMES: &str = "grbl-1.1 (алиас: grbl), grbl-hal, fluidnc, linuxcnc";

const TRANSLATE_FLAGS: &[&str] = &["--from", "--to", "--profile", "--input", "--output"];
const VERIFY_FLAGS: &[&str] = &["--profile", "--input"];

/// Класс ошибки → код выхода: 2 — usage (аргументы), 1 — конфигурация/файлы.
enum CliError {
    Usage(String),
    Failure(String),
}

type CliResult<T> = Result<T, CliError>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match dispatch(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage(msg)) => {
            eprintln!("error: {msg}");
            eprintln!("справка: eid --help");
            ExitCode::from(2)
        }
        Err(CliError::Failure(msg)) => {
            eprintln!("error: {msg}");
            ExitCode::from(1)
        }
    }
}

fn dispatch(args: &[String]) -> CliResult<()> {
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
// translate
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

    if profile.dialect != from {
        eprintln!(
            "note: диалект профиля ({}) не совпадает с --from ({}); профиль применён как задан",
            profile.dialect.as_name(),
            from.as_name()
        );
    }

    println!("translate: adapters pending (C/D iteration 2-3), конфигурация валидна");
    println!("  from:            {}", from.as_name());
    println!("  to:              {}", to.as_name());
    println!("  input:           {} ({})", input.display(), file_size(&input));
    println!("  output:          {}", output.display());
    print_profile(&profile, &profile_path);
    Ok(())
}

// ---------------------------------------------------------------------------
// verify
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

    println!(
        "verify: adapters/sim pending (C/D iteration 2-3; симулятор — E iteration 4), конфигурация валидна"
    );
    println!("  input:           {} ({})", input.display(), file_size(&input));
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
            return Err(CliError::Usage(format!("флаг `{name}` получил пустое значение")));
        }
        out.push((name, value));
        i += 1;
    }
    Ok(out)
}

fn get_flag<'a>(flags: &'a [(String, String)], name: &str) -> Option<&'a str> {
    flags.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

fn require_flag<'a>(flags: &'a [(String, String)], name: &str) -> CliResult<&'a str> {
    get_flag(flags, name).ok_or_else(|| {
        CliError::Usage(format!("отсутствует обязательный флаг `{name}`"))
    })
}

fn wants_help(args: &[String]) -> bool {
    args.iter().any(|a| a == "--help" || a == "-h")
}

/// Канонические имена диалектов; `grbl` — алиас `grbl-1.1`
/// (короткая форма из IMPLEMENTATION-PLAN §5, критерий #9).
fn parse_dialect(s: &str, flag: &str) -> CliResult<DialectKind> {
    let canonical = if s == "grbl" { "grbl-1.1" } else { s };
    DialectKind::from_name(canonical).ok_or_else(|| {
        CliError::Usage(format!("флаг {flag}: неизвестный диалект `{s}` (доступно: {DIALECT_NAMES})"))
    })
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

fn file_size(path: &Path) -> String {
    std::fs::metadata(path)
        .map(|m| format!("{} bytes", m.len()))
        .unwrap_or_else(|_| "размер неизвестен".to_string())
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
}

// ---------------------------------------------------------------------------
// Справка
// ---------------------------------------------------------------------------

fn print_version() {
    println!(
        "eid {} (interface v0.1, итерация 2 — Агент E)",
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
  translate   Перевод файла между диалектами через EID/L1
  verify      Parse + проверка лимитов профиля (симуляция — этап 8)

ДИАЛЕКТЫ:
  {DIALECT_NAMES}

ОПЦИИ:
  --from <dialect>    Диалект источника (translate)
  --to <dialect>      Диалект цели (translate)
  --profile <toml>    MachineProfile — TOML (примеры: crates/eid-integration/profiles/)
  --input <file>      Входной файл G-кода
  --output <file>     Выходной файл (translate)

СТАТУС (итерация 2, Агент E):
  Разбор аргументов и валидация конфигурации работают. Адаптеры диалектов
  (Агенты C/D, итерации 2–3) и симулятор (Агент E, итерация 4) — pending:
  команды печатают resolved-параметры, трансляция не выполняется.

КОДЫ ВЫХОДА:
  0  успех (в т.ч. `adapters pending` — конфигурация валидна)
  1  ошибка конфигурации или файлов (профиль не загрузился, input недоступен)
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
  eid translate --from grbl --to linuxcnc --profile crates/eid-integration/profiles/grbl_default.toml --input corpus/demo/pocket.gcode --output pocket.ngc

СТАТУС: адаптеры диалектов (Агенты C/D) — pending (итерации 2–3). Команда
валидирует конфигурацию и печатает resolved-параметры, трансляция не выполняется.

КОДЫ ВЫХОДА: 0 — конфигурация валидна; 1 — профиль/файлы; 2 — usage.
"
    );
}

fn print_verify_help() {
    print!(
        "\
eid verify — проверка программы G-кода (parse, лимиты профиля; симуляция — этап 8)

ИСПОЛЬЗОВАНИЕ:
  eid verify --input <file> --profile <toml>

ФЛАГИ (оба обязательны):
  --profile <toml>    MachineProfile (см. crates/eid-integration/profiles/*.toml)
  --input <file>      Входной файл G-кода (существует, читается)

СТАТУС: адаптеры диалектов (Агенты C/D, итерации 2–3) и симулятор (Агент E,
итерация 4) — pending. Команда валидирует конфигурацию и печатает
resolved-параметры, проверка не выполняется.

КОДЫ ВЫХОДА: 0 — конфигурация валидна; 1 — профиль/файлы; 2 — usage.
"
    );
}
