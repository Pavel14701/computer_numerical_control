//! Тесты MachineProfile (Агент E, итерация 2; TOML с итерации 3 — R-1):
//! загрузка дефолтных TOML-профилей, дефолты лимитов COVERAGE §1.2,
//! обработка невалидных профилей.
//!
//! Аудит-волна (итерация 6): #25 fanuc-профиль (диалект валиден, адаптер
//! pending), #26 пост-валидация семантики (лимиты > 0, word_chars 1..=4096,
//! axis_limits min<=max + finite), #11/#27 — serde-дефолты новых полей
//! (emit_preamble=false, axis_limits_units=mm) на старых TOML.

use eid_core::ast::Units;
use eid_integration::profile::{
    AxisRange, DialectKind, Limits, MachineProfile, ProfileEmissionOpts, ProgramEndStyle,
    FANUC_DEFAULT_PROFILE, GRBL_DEFAULT_PROFILE, HAAS_DEFAULT_PROFILE, LINUXCNC_DEFAULT_PROFILE,
    MAX_WORD_CHARS_CAP,
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
    let fanuc: MachineProfile = toml::from_str(FANUC_DEFAULT_PROFILE).unwrap();
    assert_eq!(fanuc, MachineProfile::fanuc_default());
}

#[test]
fn load_fanuc_default_profile() {
    // #25: fanuc-диалект валиден в профилях (адаптер pending — см. e2e);
    // профиль грузится и несёт дефолты COVERAGE §1.2.
    let path = write_temp("fanuc_default.toml", FANUC_DEFAULT_PROFILE);
    let p = MachineProfile::load(&path).expect("fanuc_default.toml должен грузиться");
    assert_eq!(p.name, "Fanuc default");
    assert_eq!(p.dialect, DialectKind::Fanuc);
    assert_eq!(p.dialect.as_name(), "fanuc");
    assert_eq!(DialectKind::from_name("fanuc"), Some(DialectKind::Fanuc));
    assert_eq!(p.limits, coverage_defaults());
    assert!(!p.emit_r_arcs);
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
    for p in [
        MachineProfile::grbl_default(),
        MachineProfile::linuxcnc_default(),
    ] {
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
    let path = write_temp(
        "malformed.toml",
        "name = \"незакрытая скобка\nlimits = [1, 2\n",
    );
    let err = MachineProfile::load(&path).expect_err("TOML сломан");
    assert!(err.contains("невалидный TOML"), "неожиданная ошибка: {err}");
}

#[test]
fn unknown_dialect_is_err() {
    // "okuma" — вне словаря DialectKind (регистрация "haas" — этап 3
    // agent C: haas теперь ВАЛИДНОЕ имя, см. load_haas_default_profile).
    let path = write_temp("unknown_dialect.toml", UNKNOWN_DIALECT_TOML);
    let err = MachineProfile::load(&path).expect_err("диалект okuma вне словаря");
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
// 4. Переопределение профилем: нестандартные лимиты читаются (R-эмиссия
//    true вне fanuc/haas гейтится — см. stage8_r_arcs_true_... ниже).
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
    assert!(!p.emit_r_arcs);
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
    assert_eq!(
        a.x,
        AxisRange {
            min: -100.0,
            max: 400.0
        }
    );
    assert_eq!(
        a.y,
        AxisRange {
            min: 0.0,
            max: 300.0
        }
    );
    assert_eq!(
        a.z,
        AxisRange {
            min: -10.0,
            max: 150.0
        }
    );
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

// ---------------------------------------------------------------------------
// 6. Новые поля (аудит #11/#27): serde-дефолты на старых TOML + явные значения.
// ---------------------------------------------------------------------------

#[test]
fn new_fields_serde_defaults_on_legacy_toml() {
    // Старый TOML без emit-preamble/axis-limits-units: дефолты false/mm,
    // загрузка не ломается (аддитивная эволюция схемы).
    let p: MachineProfile = toml::from_str(GRBL_DEFAULT_PROFILE).unwrap();
    assert!(!p.emit_preamble);
    assert_eq!(p.axis_limits_units, Units::Mm);
    let legacy = concat!(
        "name = \"Legacy\"\n",
        "dialect = \"grbl-1.1\"\n",
        "emit-r-arcs = false\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 200000\n",
        "max-unroll-iterations = 10000\n",
        "max-source-lines = 1000000\n",
        "max-word-chars = 32\n",
    );
    let p2: MachineProfile = toml::from_str(legacy).expect("легаси-TOML валиден");
    assert!(!p2.emit_preamble);
    assert_eq!(p2.axis_limits_units, Units::Mm);
}

#[test]
fn explicit_preamble_and_units_parse() {
    // Явные значения: emit-preamble = true, axis-limits-units = "in".
    let toml_text = concat!(
        "name = \"Inch bench\"\n",
        "dialect = \"linuxcnc\"\n",
        "emit-r-arcs = false\n",
        "emit-preamble = true\n",
        "axis-limits-units = \"in\"\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 200000\n",
        "max-unroll-iterations = 10000\n",
        "max-source-lines = 1000000\n",
        "max-word-chars = 32\n",
    );
    let p: MachineProfile = toml::from_str(toml_text).expect("профиль с новыми полями");
    assert!(p.emit_preamble);
    assert_eq!(p.axis_limits_units, Units::In);
}

#[test]
fn unknown_units_value_is_err() {
    let toml_text = concat!(
        "name = \"Bad units\"\n",
        "dialect = \"linuxcnc\"\n",
        "emit-r-arcs = false\n",
        "axis-limits-units = \"ft\"\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 200000\n",
        "max-unroll-iterations = 10000\n",
        "max-source-lines = 1000000\n",
        "max-word-chars = 32\n",
    );
    let err =
        toml::from_str::<MachineProfile>(toml_text).expect_err("units \"ft\" вне словаря (mm|in)");
    let msg = err.to_string();
    assert!(
        msg.contains("mm") || msg.contains("unknown units"),
        "неожиданная ошибка: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 7. Пост-валидация семантики (#26): лимиты > 0, word_chars 1..=4096,
//    axis_limits min <= max + is_finite. load() = parse + validate.
// ---------------------------------------------------------------------------

#[test]
fn validate_zero_limits_rejected() {
    for (field, toml_text) in [
        ("max-ops-per-program", limits_toml_with(0, 10, 10, 32)),
        ("max-unroll-iterations", limits_toml_with(10, 0, 10, 32)),
        ("max-source-lines", limits_toml_with(10, 10, 0, 32)),
    ] {
        let path = write_temp("zero_limit.toml", &toml_text);
        let err = MachineProfile::load(&path)
            .expect_err(&format!("нулевой лимит {field} должен отвергаться"));
        assert!(
            err.contains(field),
            "{field}: сообщение без имени поля: {err}"
        );
        assert!(
            err.contains("> 0"),
            "{field}: ожидалось пояснение «> 0»: {err}"
        );
    }
}

#[test]
fn validate_word_chars_bounds() {
    // 0 → отказ; 1 и 4096 (границы) → ок; 4097 → отказ.
    let zero = write_temp("wc_zero.toml", &limits_toml_with(10, 10, 10, 0));
    let err = MachineProfile::load(&zero).expect_err("max-word-chars = 0 отвергается");
    assert!(err.contains("max-word-chars"), "{err}");

    for ok in [1usize, MAX_WORD_CHARS_CAP] {
        let path = write_temp("wc_ok.toml", &limits_toml_with(10, 10, 10, ok));
        MachineProfile::load(&path)
            .unwrap_or_else(|e| panic!("max-word-chars = {ok} должен грузиться: {e}"));
    }
    let over = write_temp(
        "wc_over.toml",
        &limits_toml_with(10, 10, 10, MAX_WORD_CHARS_CAP + 1),
    );
    let err = MachineProfile::load(&over).expect_err("max-word-chars > 4096 отвергается");
    assert!(
        err.contains("max-word-chars") && err.contains("4096"),
        "неожиданная ошибка: {err}"
    );
}

#[test]
fn validate_axis_limits_min_le_max_and_finite() {
    // min > max → отказ с именем оси.
    let bad = concat!(
        "name = \"Inverted\"\n",
        "dialect = \"linuxcnc\"\n",
        "emit-r-arcs = false\n",
        "\n",
        "[axis-limits]\n",
        "x = { min = 10.0, max = -10.0 }\n",
        "y = { min = 0.0, max = 100.0 }\n",
        "z = { min = 0.0, max = 100.0 }\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 10\n",
        "max-unroll-iterations = 10\n",
        "max-source-lines = 10\n",
        "max-word-chars = 32\n",
    );
    let path = write_temp("axis_inverted.toml", bad);
    let err = MachineProfile::load(&path).expect_err("min > max отвергается");
    assert!(
        err.contains("axis_limits.x") && err.contains("min") && err.contains("max"),
        "неожиданная ошибка: {err}"
    );

    // NaN у границы → отказ (TOML-literal `nan`).
    let nan = concat!(
        "name = \"NaN bound\"\n",
        "dialect = \"linuxcnc\"\n",
        "emit-r-arcs = false\n",
        "\n",
        "[axis-limits]\n",
        "x = { min = nan, max = 100.0 }\n",
        "y = { min = 0.0, max = 100.0 }\n",
        "z = { min = 0.0, max = 100.0 }\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 10\n",
        "max-unroll-iterations = 10\n",
        "max-source-lines = 10\n",
        "max-word-chars = 32\n",
    );
    let path = write_temp("axis_nan.toml", nan);
    let err = MachineProfile::load(&path).expect_err("nan-граница отвергается");
    assert!(
        err.contains("axis_limits.x") && err.contains("конечное"),
        "неожиданная ошибка: {err}"
    );

    // inf у границы → отказ (TOML-literal `inf`).
    let inf = concat!(
        "name = \"Inf bound\"\n",
        "dialect = \"linuxcnc\"\n",
        "emit-r-arcs = false\n",
        "\n",
        "[axis-limits]\n",
        "x = { min = -100.0, max = inf }\n",
        "y = { min = 0.0, max = 100.0 }\n",
        "z = { min = 0.0, max = 100.0 }\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 10\n",
        "max-unroll-iterations = 10\n",
        "max-source-lines = 10\n",
        "max-word-chars = 32\n",
    );
    let path = write_temp("axis_inf.toml", inf);
    let err = MachineProfile::load(&path).expect_err("inf-граница отвергается");
    assert!(err.contains("axis_limits.x"), "неожиданная ошибка: {err}");

    // Валидный профиль с конечными min<=max проходит.
    let ok = concat!(
        "name = \"OK\"\n",
        "dialect = \"linuxcnc\"\n",
        "emit-r-arcs = false\n",
        "\n",
        "[axis-limits]\n",
        "x = { min = -100.0, max = 100.0 }\n",
        "y = { min = 0.0, max = 100.0 }\n",
        "z = { min = 0.0, max = 100.0 }\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 10\n",
        "max-unroll-iterations = 10\n",
        "max-source-lines = 10\n",
        "max-word-chars = 32\n",
    );
    let path = write_temp("axis_ok.toml", ok);
    MachineProfile::load(&path).expect("конечные min<=max валидны");
}

/// Профиль с переопределёнными лимитами (для валидационных негативов).
fn limits_toml_with(ops: usize, unroll: usize, lines: usize, word_chars: usize) -> String {
    format!(
        concat!(
            "name = \"Limits probe\"\n",
            "dialect = \"linuxcnc\"\n",
            "emit-r-arcs = false\n",
            "\n",
            "[limits]\n",
            "max-ops-per-program = {ops}\n",
            "max-unroll-iterations = {unroll}\n",
            "max-source-lines = {lines}\n",
            "max-word-chars = {word_chars}\n",
        ),
        ops = ops,
        unroll = unroll,
        lines = lines,
        word_chars = word_chars,
    )
}

#[test]
fn default_constructors_pass_validation() {
    for p in [
        MachineProfile::grbl_default(),
        MachineProfile::linuxcnc_default(),
        MachineProfile::fanuc_default(),
        MachineProfile::haas_default(),
    ] {
        p.validate()
            .unwrap_or_else(|e| panic!("дефолт {} невалиден: {e}", p.name));
    }
}

#[test]
fn load_haas_default_profile() {
    // Регистрация "haas" (этап 3, agent C): haas-диалект валиден в профилях;
    // профиль грузится и несёт дефолты COVERAGE §1.2 (паттерн fanuc).
    let path = write_temp("haas_default.toml", HAAS_DEFAULT_PROFILE);
    let p = MachineProfile::load(&path).expect("haas_default.toml должен грузиться");
    assert_eq!(p.name, "Haas default");
    assert_eq!(p.dialect, DialectKind::Haas);
    assert_eq!(p.dialect.as_name(), "haas");
    assert_eq!(DialectKind::from_name("haas"), Some(DialectKind::Haas));
    assert_eq!(p.limits, coverage_defaults());
    assert!(!p.emit_r_arcs);
    assert_eq!(p, MachineProfile::haas_default());
}

// TOML-фрагменты тестов.
const UNKNOWN_DIALECT_TOML: &str = concat!(
    "name = \"X\"\n",
    "dialect = \"haas\"\n",
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
    // emit_r_arcs = true вне fanuc/haas гейтится validate() (реестр §7.1 —
    // опции в grbl-адаптеры не пробрасываются); валидное значение — false.
    "emit-r-arcs = false\n",
    "\n",
    "[limits]\n",
    "max-ops-per-program = 5000\n",
    "max-unroll-iterations = 100\n",
    "max-source-lines = 20000\n",
    "max-word-chars = 8\n",
);

// ---------------------------------------------------------------------------
// 8. Этап 8 (emission-профили fanuc/haas): serde-default на старых TOML,
//    явные значения (program_end_style, wcs_extended_map, allow-r-arcs
//    алиас), валидация диалекта/формата.
// ---------------------------------------------------------------------------

#[test]
fn stage8_flags_serde_defaults_on_legacy_toml() {
    // Старый TOML без новых ключей: дефолты M30 / мапы нет — загрузка не
    // ломается (аддитивная эволюция схемы, serde-default).
    let p: MachineProfile = toml::from_str(GRBL_DEFAULT_PROFILE).unwrap();
    assert_eq!(p.program_end_style, ProgramEndStyle::M30);
    assert_eq!(p.wcs_extended_map, None);
    let legacy = concat!(
        "name = \"Legacy8\"\n",
        "dialect = \"fanuc\"\n",
        "emit-r-arcs = false\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 200000\n",
        "max-unroll-iterations = 10000\n",
        "max-source-lines = 1000000\n",
        "max-word-chars = 32\n",
    );
    let p2: MachineProfile = toml::from_str(legacy).expect("легаси-TOML валиден");
    assert_eq!(p2.program_end_style, ProgramEndStyle::M30);
    assert_eq!(p2.wcs_extended_map, None);
    // Дефолтные TOML fanuc/haas несут явный M30 и равны конструкторам.
    let fanuc: MachineProfile = toml::from_str(FANUC_DEFAULT_PROFILE).unwrap();
    assert_eq!(fanuc.program_end_style, ProgramEndStyle::M30);
    assert_eq!(fanuc.wcs_extended_map, None);
    let haas: MachineProfile = toml::from_str(HAAS_DEFAULT_PROFILE).unwrap();
    assert_eq!(haas, MachineProfile::haas_default());
}

#[test]
fn stage8_explicit_flags_parse() {
    // Явные значения всех трёх флагов (M02 + мапа + allow-r-arcs-алиас).
    let toml_text = concat!(
        "name = \"Stage8 bench\"\n",
        "dialect = \"fanuc\"\n",
        "allow-r-arcs = true\n",
        "program-end-style = \"M02\"\n",
        "\n",
        "[wcs-extended-map]\n",
        "\"G59.1\" = \"G54.1 P1\"\n",
        "\"G59.2\" = \"G54.1 P2\"\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 200000\n",
        "max-unroll-iterations = 10000\n",
        "max-source-lines = 1000000\n",
        "max-word-chars = 32\n",
    );
    let p: MachineProfile = toml::from_str(toml_text).expect("профиль с флагами этапа 8");
    assert_eq!(p.program_end_style, ProgramEndStyle::M02);
    let map = p.wcs_extended_map.as_ref().expect("мапа прочитана");
    assert_eq!(map.get("G59.1").map(String::as_str), Some("G54.1 P1"));
    assert_eq!(map.get("G59.2").map(String::as_str), Some("G54.1 P2"));
    // allow-r-arcs — serde-алиас emit-r-arcs (имя флага из брифа этапа 8):
    // TOML-ключ может называться любым из двух (оба в одном файле = duplicate
    // field — serde-семантика алиасов).
    assert!(p.emit_r_arcs);
    // Проекция в ProfileEmissionOpts.
    let o = ProfileEmissionOpts::from_profile(&p);
    assert!(!o.is_default());
    assert_eq!(o.program_end_style, ProgramEndStyle::M02);
    assert!(o.allow_r_arcs);
    assert_eq!(o.wcs_extended_map.len(), 2);
}

#[test]
fn stage8_m02_on_non_fanuc_haas_dialect_is_err() {
    // program_end_style = "M02" вне fanuc/haas → отказ загрузки (тихое
    // игнорирование запрещено).
    let toml_text = concat!(
        "name = \"Grbl M02\"\n",
        "dialect = \"grbl-1.1\"\n",
        "emit-r-arcs = false\n",
        "program-end-style = \"M02\"\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 200000\n",
        "max-unroll-iterations = 10000\n",
        "max-source-lines = 1000000\n",
        "max-word-chars = 32\n",
    );
    let path = write_temp("stage8_m02_grbl.toml", toml_text);
    let err = MachineProfile::load(&path).expect_err("M02 на grbl отвергается");
    // Честная формулировка (реестр §7.1: старый текст «в словаре … флага
    // program_end нет» был фактической ошибкой — M02/M30 ЕСТЬ в словарях
    // обеих целей, COVERAGE §3 «различие вне L1»; отказ — из-за механики:
    // стиль program-end не выбирается профилем, дефолт эмиссии = M2/M30).
    assert!(
        err.contains("program_end_style")
            && err.contains("неприменим на цели")
            && err.contains("L1-эквивалентны")
            && !err.contains("в словаре"),
        "{err}"
    );
    // Явный дефолт M30 на любом диалекте валиден (no-op).
    let ok = toml_text.replace("program-end-style = \"M02\"", "program-end-style = \"M30\"");
    let path = write_temp("stage8_m30_grbl.toml", &ok);
    MachineProfile::load(&path).expect("M30 на grbl валиден (дефолт)");
}

#[test]
fn stage8_r_arcs_true_on_non_fanuc_haas_dialect_is_err() {
    // Гейт emit_r_arcs (реестр §7.1/§7.3): единственный из трёх
    // emission-флагов этапа 8 без диалект-гейта — true на grbl/linuxcnc
    // проходил validate и ТИХО игнорировался (опции эмиссии пробрасываются
    // только в fanuc/haas-обёртки registry). Теперь — отказ загрузки
    // (тихое игнорирование запрещено); false валиден на любом диалекте,
    // true — на fanuc/haas.
    let mk = |dialect: &str, flag: &str| {
        format!(
            concat!(
                "name = \"R gate\"\n",
                "dialect = \"{d}\"\n",
                "emit-r-arcs = {f}\n",
                "\n",
                "[limits]\n",
                "max-ops-per-program = 200000\n",
                "max-unroll-iterations = 10000\n",
                "max-source-lines = 1000000\n",
                "max-word-chars = 32\n",
            ),
            d = dialect,
            f = flag
        )
    };
    for dialect in ["grbl-1.1", "grbl-hal", "fluidnc", "linuxcnc"] {
        let name = format!("stage8_r_true_{}.toml", dialect.replace('-', "_"));
        let path = write_temp(&name, &mk(dialect, "true"));
        let err =
            MachineProfile::load(&path).expect_err("emit_r_arcs=true вне fanuc/haas отвергается");
        assert!(
            err.contains("emit_r_arcs") && err.contains("неприменим"),
            "{err}"
        );
    }
    // false валиден на любом диалекте (дефолт no-op).
    for dialect in ["grbl-1.1", "linuxcnc"] {
        let name = format!("stage8_r_false_{}.toml", dialect.replace('-', "_"));
        let path = write_temp(&name, &mk(dialect, "false"));
        let p = MachineProfile::load(&path).expect("false валиден везде");
        assert!(!p.emit_r_arcs);
    }
    // true на fanuc/haas валиден (опции пробрасываются в адаптер).
    for dialect in ["fanuc", "haas"] {
        let name = format!("stage8_r_true_{dialect}.toml");
        let path = write_temp(&name, &mk(dialect, "true"));
        let p = MachineProfile::load(&path).expect("true на fanuc/haas валиден");
        assert!(p.emit_r_arcs);
    }
}

#[test]
fn stage8_map_on_non_fanuc_haas_dialect_is_err() {
    let toml_text = concat!(
        "name = \"LinuxCNC map\"\n",
        "dialect = \"linuxcnc\"\n",
        "emit-r-arcs = false\n",
        "\n",
        "[wcs-extended-map]\n",
        "\"G59.1\" = \"G154 P1\"\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 200000\n",
        "max-unroll-iterations = 10000\n",
        "max-source-lines = 1000000\n",
        "max-word-chars = 32\n",
    );
    let path = write_temp("stage8_map_lnx.toml", toml_text);
    let err = MachineProfile::load(&path).expect_err("мапа на linuxcnc отвергается");
    assert!(err.contains("wcs_extended_map"), "{err}");
}

#[test]
fn stage8_map_value_format_validation() {
    let mk = |dialect: &str, key: &str, value: &str| {
        format!(
            concat!(
                "name = \"Probe\"\n",
                "dialect = \"{d}\"\n",
                "emit-r-arcs = false\n",
                "\n",
                "[wcs-extended-map]\n",
                "\"{k}\" = \"{v}\"\n",
                "\n",
                "[limits]\n",
                "max-ops-per-program = 200000\n",
                "max-unroll-iterations = 10000\n",
                "max-source-lines = 1000000\n",
                "max-word-chars = 32\n",
            ),
            d = dialect,
            k = key,
            v = value
        )
    };
    // Чужой-диалектный формат значения: G154 на fanuc → отказ.
    let path = write_temp("s8_map_wrong_fmt.toml", &mk("fanuc", "G59.1", "G154 P1"));
    let err = MachineProfile::load(&path).expect_err("G154 на fanuc отвергается");
    assert!(err.contains("G54.1 P<n>"), "{err}");
    // Ключ вне словаря → отказ.
    let path = write_temp("s8_map_bad_key.toml", &mk("fanuc", "G54.1", "G54.1 P1"));
    let err = MachineProfile::load(&path).expect_err("ключ G54.1 вне словаря");
    assert!(
        err.contains("валидные ключи") && err.contains("G59.1"),
        "{err}"
    );
    // Домен Pn: fanuc P49 > 48 → отказ; P1..P48 границы ок.
    let path = write_temp("s8_map_p49.toml", &mk("fanuc", "G59.1", "G54.1 P49"));
    assert!(MachineProfile::load(&path).is_err(), "P49 > домена 48");
    let path = write_temp("s8_map_p48.toml", &mk("fanuc", "G59.1", "G54.1 P48"));
    MachineProfile::load(&path).expect("P48 = граница домена fanuc");
    // haas: P99 ок, P100 отказ.
    let path = write_temp("s8_map_p99.toml", &mk("haas", "G59.1", "G154 P99"));
    MachineProfile::load(&path).expect("P99 = граница домена haas");
    let path = write_temp("s8_map_p100.toml", &mk("haas", "G59.1", "G154 P100"));
    assert!(MachineProfile::load(&path).is_err(), "P100 > домена 99");
    // P0 → отказ (домен 1..N).
    let path = write_temp("s8_map_p0.toml", &mk("haas", "G59.1", "G154 P0"));
    assert!(MachineProfile::load(&path).is_err(), "P0 вне домена");
    // Пустая мапа → отказ (конфигурационная ошибка).
    let empty = concat!(
        "name = \"Empty map\"\n",
        "dialect = \"haas\"\n",
        "emit-r-arcs = false\n",
        "\n",
        "[wcs-extended-map]\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 200000\n",
        "max-unroll-iterations = 10000\n",
        "max-source-lines = 1000000\n",
        "max-word-chars = 32\n",
    );
    let path = write_temp("s8_map_empty.toml", empty);
    let err = MachineProfile::load(&path).expect_err("пустая мапа отвергается");
    assert!(err.contains("пуста"), "{err}");
}

#[test]
fn stage8_allow_r_arcs_alias_reads() {
    // allow_r_arcs (имя флага из брифа этапа 8) — serde-алиас канонического
    // поля emit_r_arcs: старые TOML (`emit-r-arcs`) остаются валидны.
    let toml_text = concat!(
        "name = \"R bench\"\n",
        "dialect = \"haas\"\n",
        "allow-r-arcs = true\n",
        "\n",
        "[limits]\n",
        "max-ops-per-program = 200000\n",
        "max-unroll-iterations = 10000\n",
        "max-source-lines = 1000000\n",
        "max-word-chars = 32\n",
    );
    let p: MachineProfile = toml::from_str(toml_text).expect("алиас читается");
    assert!(p.emit_r_arcs);
    let o = ProfileEmissionOpts::from_profile(&p);
    assert!(o.allow_r_arcs);
}
