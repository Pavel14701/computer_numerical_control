//! Тесты MachineProfile (Агент E, итерация 2): загрузка дефолтных YAML-профилей,
//! дефолты лимитов COVERAGE §1.2, обработка невалидных профилей.

use eid_integration::profile::{
    DialectKind, Limits, MachineProfile, GRBL_DEFAULT_YAML, LINUXCNC_DEFAULT_YAML,
};
use std::path::PathBuf;

/// Дефолты COVERAGE §1.2: 200000 / 10000 / 1000000 / 32.
fn coverage_defaults() -> Limits {
    Limits {
        max_ops_per_program: 200_000,
        max_unroll_iterations: 10_000,
        max_source_lines: 1_000_000,
        max_word_chars: 32,
    }
}

/// Уникальный временный файл (без внешних зависимостей).
fn temp_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eid_profile_tests_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("создать временный каталог");
    dir.join(name)
}

fn write_temp(name: &str, content: &str) -> PathBuf {
    let path = temp_file(name);
    std::fs::write(&path, content).expect("записать временный файл");
    path
}

// ---------------------------------------------------------------------------
// 1. Загрузка обоих дефолтных YAML → ожидаемые поля.
// ---------------------------------------------------------------------------

#[test]
fn load_grbl_default_yaml() {
    let path = write_temp("grbl_default.yaml", GRBL_DEFAULT_YAML);
    let p = MachineProfile::load_yaml(&path).expect("grbl_default.yaml должен грузиться");
    assert_eq!(p.name, "GRBL 1.1 default");
    assert_eq!(p.dialect, DialectKind::Grbl11);
    assert_eq!(p.limits, coverage_defaults());
    // EQUIVALENCE §4: эмиссия всегда IJK, R — только по флагу с notice.
    assert!(!p.emit_r_arcs);
}

#[test]
fn load_linuxcnc_default_yaml() {
    let path = write_temp("linuxcnc_default.yaml", LINUXCNC_DEFAULT_YAML);
    let p = MachineProfile::load_yaml(&path).expect("linuxcnc_default.yaml должен грузиться");
    assert_eq!(p.name, "LinuxCNC default");
    assert_eq!(p.dialect, DialectKind::LinuxCnc);
    assert_eq!(p.limits, coverage_defaults());
    assert!(!p.emit_r_arcs);
}

#[test]
fn embedded_yaml_matches_constructed_defaults() {
    // single source of truth: YAML и конструкторы не расходятся.
    let grbl: MachineProfile = serde_yaml::from_str(GRBL_DEFAULT_YAML).unwrap();
    assert_eq!(grbl, MachineProfile::grbl_default());
    let lnx: MachineProfile = serde_yaml::from_str(LINUXCNC_DEFAULT_YAML).unwrap();
    assert_eq!(lnx, MachineProfile::linuxcnc_default());
}

// ---------------------------------------------------------------------------
// 2. Дефолтные конструкторы: лимиты COVERAGE §1.2, emit_r_arcs: false.
// ---------------------------------------------------------------------------

#[test]
fn limits_defaults_match_coverage() {
    assert_eq!(Limits::default(), coverage_defaults());
}

#[test]
fn dialect_defaults_have_coverage_limits_and_ijk_only() {
    for p in [MachineProfile::grbl_default(), MachineProfile::linuxcnc_default()] {
        assert_eq!(p.limits, coverage_defaults());
        assert!(!p.emit_r_arcs);
    }
    assert_eq!(MachineProfile::grbl_default().dialect, DialectKind::Grbl11);
    assert_eq!(
        MachineProfile::linuxcnc_default().dialect,
        DialectKind::LinuxCnc
    );
    assert_eq!(DialectKind::Grbl11.as_name(), "grbl-1.1");
    assert_eq!(DialectKind::LinuxCnc.as_name(), "linuxcnc");
}

// ---------------------------------------------------------------------------
// 3. Невалидные профили → Err.
// ---------------------------------------------------------------------------

#[test]
fn missing_file_is_err() {
    let path = temp_file("no_such_profile_here.yaml");
    let err = MachineProfile::load_yaml(&path).expect_err("файл не существует");
    assert!(err.contains("не удалось прочитать"), "неожиданная ошибка: {err}");
}

#[test]
fn malformed_yaml_is_err() {
    let path = write_temp(
        "malformed.yaml",
        "name: \"незакрытая скобка\nlimits: [1, 2\n",
    );
    let err = MachineProfile::load_yaml(&path).expect_err("YAML сломан");
    assert!(err.contains("невалидный YAML"), "неожиданная ошибка: {err}");
}

#[test]
fn unknown_dialect_is_err() {
    let path = write_temp("unknown_dialect.yaml", UNKNOWN_DIALECT_YAML);
    let err = MachineProfile::load_yaml(&path).expect_err("диалект fanuc вне словаря");
    assert!(err.contains("unknown variant") || err.contains("невалидный YAML"),
        "неожиданная ошибка: {err}");
}

#[test]
fn empty_file_is_err() {
    let path = write_temp("empty.yaml", "");
    assert!(MachineProfile::load_yaml(&path).is_err());
}

// ---------------------------------------------------------------------------
// 4. Переопределение профилем: нестандартные лимиты и R-эмиссия читаются.
// ---------------------------------------------------------------------------

#[test]
fn profile_overrides_parse() {
    let path = write_temp("override.yaml", OVERRIDE_YAML);
    let p = MachineProfile::load_yaml(&path).expect("валидный профиль");
    assert_eq!(p.dialect, DialectKind::GrblHal);
    assert_eq!(p.limits.max_ops_per_program, 5000);
    assert_eq!(p.limits.max_unroll_iterations, 100);
    assert_eq!(p.limits.max_source_lines, 20_000);
    assert_eq!(p.limits.max_word_chars, 8);
    assert!(p.emit_r_arcs);
}

// YAML-фрагменты тестов (concat! — чтобы не бороться с \-continuation,
// съедающей ведущие пробелы: отступы YAML значимы).
const UNKNOWN_DIALECT_YAML: &str = concat!(
    "name: \"X\"\n",
    "dialect: fanuc\n",
    "limits:\n",
    "  max-ops-per-program: 1\n",
    "  max-unroll-iterations: 2\n",
    "  max-source-lines: 3\n",
    "  max-word-chars: 4\n",
    "emit-r-arcs: false\n",
);

const OVERRIDE_YAML: &str = concat!(
    "name: \"grblHAL bench\"\n",
    "dialect: grbl-hal\n",
    "limits:\n",
    "  max-ops-per-program: 5000\n",
    "  max-unroll-iterations: 100\n",
    "  max-source-lines: 20000\n",
    "  max-word-chars: 8\n",
    "emit-r-arcs: true\n",
);
