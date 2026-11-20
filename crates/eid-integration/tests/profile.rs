//! Тесты MachineProfile (Агент E, итерация 2; TOML с итерации 3 — R-1):
//! загрузка дефолтных TOML-профилей, дефолты лимитов COVERAGE §1.2,
//! обработка невалидных профилей.

use eid_integration::profile::{
    AxisLimits, AxisRange, DialectKind, Limits, MachineProfile, GRBL_DEFAULT_PROFILE,
    LINUXCNC_DEFAULT_PROFILE,
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
// 1. Загрузка обоих дефолтных TOML → ожидаемые поля.
// ---------------------------------------------------------------------------

#[test]
fn load_grbl_default_profile() {
    let path = write_temp("grbl_default.toml", GRBL_DEFAULT_PROFILE);
    let p = MachineProfile::load(&path).expect("grbl_default.toml должен грузиться");
    assert_eq!(p.name, "GRBL 1.1 default");
    assert_eq!(p.dialect, DialectKind::Grbl11);
    assert_eq!(p.limits, coverage_defaults());
    // EQUIVALENCE §4: эмиссия всегда IJK, R — только по флагу с notice.
    assert!(!p.emit_r_arcs);
}

#[test]
fn load_linuxcnc_default_profile() {
    let path = write_temp("linuxcnc_default.toml", LINUXCNC_DEFAULT_PROFILE);
    let p = MachineProfile::load(&path).expect("linuxcnc_default.toml должен грузиться");
    assert_eq!(p.name, "LinuxCNC default");
    assert_eq!(p.dialect, DialectKind::LinuxCnc);
    assert_eq!(p.limits, coverage_defaults());
    assert!(!p.emit_r_arcs);
}

#[test]
fn embedded_profiles_match_constructed_defaults() {
    // single source of truth: TOML и конструкторы не расходятся.
    let grbl: MachineProfile = toml::from_str(GRBL_DEFAULT_PROFILE).unwrap();
    assert_eq!(grbl, MachineProfile::grbl_default());
    let lnx: MachineProfile = toml::from_str(LINUXCNC_DEFAULT_PROFILE).unwrap();
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
    let path = temp_file("no_such_profile_here.toml");
    let err = MachineProfile::load(&path).expect_err("файл не существует");
    assert!(
        err.contains("не удалось прочитать"),
        "неожиданная ошибка: {err}"
    );
}

#[test]
fn malformed_toml_is_err() {
    let path = write_temp("malformed.toml", "name = \"незакрытая скобка\nlimits = [1, 2\n");
    let err = MachineProfile::load(&path).expect_err("TOML сломан");
    assert!(err.contains("невалидный TOML"), "неожиданная ошибка: {err}");
}

#[test]
fn unknown_dialect_is_err() {
    let path = write_temp("unknown_dialect.toml", UNKNOWN_DIALECT_TOML);
    let err = MachineProfile::load(&path).expect_err("диалект fanuc вне словаря");
    assert!(
        err.contains("unknown variant") || err.contains("невалидный TOML"),
        "неожиданная ошибка: {err}"
    );
}

#[test]
fn empty_file_is_err() {
    let path = write_temp("empty.toml", "");
    assert!(MachineProfile::load(&path).is_err());
}

// ---------------------------------------------------------------------------
// 4. Переопределение профилем: нестандартные лимиты и R-эмиссия читаются.
// ---------------------------------------------------------------------------

#[test]
fn profile_overrides_parse() {
    let path = write_temp("override.toml", OVERRIDE_TOML);
    let p = MachineProfile::load(&path).expect("валидный профиль");
    assert_eq!(p.dialect, DialectKind::GrblHal);
    assert_eq!(p.limits.max_ops_per_program, 5000);
    assert_eq!(p.limits.max_unroll_iterations, 100);
    assert_eq!(p.limits.max_source_lines, 20_000);
    assert_eq!(p.limits.max_word_chars, 8);
    assert!(p.emit_r_arcs);
}

// ---------------------------------------------------------------------------
// 5. axis_limits (итерация 4): дефолт None, опциональное чтение, contains.
// ---------------------------------------------------------------------------

#[test]
fn axis_limits_default_is_none() {
    // дефолтные TOML-профили не задают axis-limits; конструкторы — тоже
    assert_eq!(MachineProfile::grbl_default().axis_limits, None);
    assert_eq!(MachineProfile::linuxcnc_default().axis_limits, None);
    let p: MachineProfile = toml::from_str(GRBL_DEFAULT_PROFILE).unwrap();
    assert_eq!(p.axis_limits, None);
}

#[test]
fn axis_limits_optional_field_parses() {
    let path = write_temp("axis.toml", AXIS_LIMITS_TOML);
    let p = MachineProfile::load(&path).expect("профиль с axis_limits");
    let a = p.axis_limits.expect("axis_limits прочитаны");
    assert_eq!(a.x, AxisRange { min: -100.0, max: 400.0 });
    assert_eq!(a.y, AxisRange { min: 0.0, max: 300.0 });
    assert_eq!(a.z, AxisRange { min: -10.0, max: 150.0 });
    assert!(a.contains(0.0, 0.0, 0.0));
    assert!(a.contains(400.0, 300.0, 150.0));
    assert!(!a.contains(400.1, 0.0, 0.0));
    assert!(!a.contains(0.0, -0.001, 0.0));
}

const AXIS_LIMITS_TOML: &str = concat!(
    "name = \"Bench XYZ\"\n",
    "dialect = \"linuxcnc\"\n",
    "emit-r-arcs = false\n",
    "\n",
    "[axis-limits]\n",
    "x = { min = -100.0, max = 400.0 }\n",
    "y = { min = 0.0, max = 300.0 }\n",
    "z = { min = -10.0, max = 150.0 }\n",
    "\n",
    "[limits]\n",
    "max-ops-per-program = 200000\n",
    "max-unroll-iterations = 10000\n",
    "max-source-lines = 1000000\n",
    "max-word-chars = 32\n",
);

// TOML-фрагменты тестов.
const UNKNOWN_DIALECT_TOML: &str = concat!(
    "name = \"X\"\n",
    "dialect = \"fanuc\"\n",
    "\n",
    "[limits]\n",
    "max-ops-per-program = 1\n",
    "max-unroll-iterations = 2\n",
    "max-source-lines = 3\n",
    "max-word-chars = 4\n",
);

const OVERRIDE_TOML: &str = concat!(
    "name = \"grblHAL bench\"\n",
    "dialect = \"grbl-hal\"\n",
    "emit-r-arcs = true\n",
    "\n",
    "[limits]\n",
    "max-ops-per-program = 5000\n",
    "max-unroll-iterations = 100\n",
    "max-source-lines = 20000\n",
    "max-word-chars = 8\n",
);
