//! Profile-матрица (Агент E, итерация 5): MachineProfile::load на семействе
//! валидных/невалидных TOML. Именование prof_<case>.
//! (Позитивы дефолтов подробно — tests/profile.rs; здесь — матрица веток.)

use eid_integration::profile::{DialectKind, MachineProfile};
use std::path::{Path, PathBuf};

fn tmp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eid_prof_matrix_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

fn write_tmp(name: &str, content: &str) -> PathBuf {
    let p = tmp().join(name);
    std::fs::write(&p, content).expect("write tmp");
    p
}

fn load(name: &str, content: &str) -> Result<MachineProfile, String> {
    MachineProfile::load(&write_tmp(name, content))
}

fn err_of(name: &str, content: &str) -> String {
    load(name, content).expect_err("профиль должен быть отвергнут")
}

// ---------------------------------------------------------------------------
// Позитивы: оба дефолта (краткая матрица; детали — tests/profile.rs)
// ---------------------------------------------------------------------------

#[test]
fn prof_defaults_all_three() {
    let grbl_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("profiles/grbl_default.toml")
        .canonicalize()
        .unwrap();
    let g = MachineProfile::load(&grbl_path).expect("grbl_default.toml");
    assert_eq!(g.dialect, DialectKind::Grbl11);
    assert_eq!(g.name, "GRBL 1.1 default");

    let lnx_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("profiles/linuxcnc_default.toml")
        .canonicalize()
        .unwrap();
    let l = MachineProfile::load(&lnx_path).expect("linuxcnc_default.toml");
    assert_eq!(l.dialect, DialectKind::LinuxCnc);
    assert_eq!(l.name, "LinuxCNC default");

    // #25: fanuc-профиль — третий дефолт (адаптер pending, профиль валиден).
    let fanuc_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("profiles/fanuc_default.toml")
        .canonicalize()
        .unwrap();
    let f = MachineProfile::load(&fanuc_path).expect("fanuc_default.toml");
    assert_eq!(f.dialect, DialectKind::Fanuc);
    assert_eq!(f.name, "Fanuc default");
    for p in [&g, &l, &f] {
        assert!(!p.emit_r_arcs);
        assert!(!p.emit_preamble, "#11: дефолт преамбулы — off");
        assert_eq!(p.axis_limits, None);
    }
}

// ---------------------------------------------------------------------------
// Негативы
// ---------------------------------------------------------------------------

#[test]
fn prof_unknown_dialect_err() {
    let err = err_of(
        "unknown_dialect.toml",
        "name = \"X\"\ndialect = \"haas\"\n\n[limits]\nmax-ops-per-program = 1\nmax-unroll-iterations = 2\nmax-source-lines = 3\nmax-word-chars = 4\n",
    );
    assert!(
        err.contains("невалидный TOML") || err.contains("unknown variant"),
        "{err}"
    );
}

// --- пост-валидация семантики (#26) ---------------------------------------

#[test]
fn prof_zero_limits_err() {
    // нулевой max-ops-per-program: форма TOML валидна, семантика — нет
    let err = err_of(
        "zero_ops.toml",
        "name = \"Z\"\ndialect = \"linuxcnc\"\nemit-r-arcs = false\n\n[limits]\nmax-ops-per-program = 0\nmax-unroll-iterations = 2\nmax-source-lines = 3\nmax-word-chars = 4\n",
    );
    assert!(
        err.contains("max-ops-per-program") && err.contains("> 0"),
        "{err}"
    );
}

#[test]
fn prof_zero_unroll_and_lines_err() {
    let err = err_of(
        "zero_unroll.toml",
        "name = \"Z\"\ndialect = \"linuxcnc\"\nemit-r-arcs = false\n\n[limits]\nmax-ops-per-program = 1\nmax-unroll-iterations = 0\nmax-source-lines = 3\nmax-word-chars = 4\n",
    );
    assert!(err.contains("max-unroll-iterations"), "{err}");

    let err = err_of(
        "zero_lines.toml",
        "name = \"Z\"\ndialect = \"linuxcnc\"\nemit-r-arcs = false\n\n[limits]\nmax-ops-per-program = 1\nmax-unroll-iterations = 2\nmax-source-lines = 0\nmax-word-chars = 4\n",
    );
    assert!(err.contains("max-source-lines"), "{err}");
}

#[test]
fn prof_word_chars_out_of_range_err() {
    let err = err_of(
        "wc_zero.toml",
        "name = \"W\"\ndialect = \"linuxcnc\"\nemit-r-arcs = false\n\n[limits]\nmax-ops-per-program = 1\nmax-unroll-iterations = 2\nmax-source-lines = 3\nmax-word-chars = 0\n",
    );
    assert!(err.contains("max-word-chars"), "{err}");

    let err = err_of(
        "wc_huge.toml",
        "name = \"W\"\ndialect = \"linuxcnc\"\nemit-r-arcs = false\n\n[limits]\nmax-ops-per-program = 1\nmax-unroll-iterations = 2\nmax-source-lines = 3\nmax-word-chars = 100000\n",
    );
    assert!(err.contains("max-word-chars"), "{err}");
}

#[test]
fn prof_axis_min_gt_max_err() {
    let err = err_of(
        "axis_inverted.toml",
        "name = \"P\"\ndialect = \"linuxcnc\"\nemit-r-arcs = false\n\n\
         [axis-limits]\nx = { min = 5.0, max = -5.0 }\ny = { min = -1.0, max = 1.0 }\nz = { min = -1.0, max = 1.0 }\n\n\
         [limits]\nmax-ops-per-program = 200000\nmax-unroll-iterations = 10000\nmax-source-lines = 1000000\nmax-word-chars = 32\n",
    );
    assert!(err.contains("axis_limits.x"), "{err}");
}

#[test]
fn prof_axis_non_finite_err() {
    let err = err_of(
        "axis_inf.toml",
        "name = \"P\"\ndialect = \"linuxcnc\"\nemit-r-arcs = false\n\n\
         [axis-limits]\nx = { min = -inf, max = inf }\ny = { min = -1.0, max = 1.0 }\nz = { min = -1.0, max = 1.0 }\n\n\
         [limits]\nmax-ops-per-program = 200000\nmax-unroll-iterations = 10000\nmax-source-lines = 1000000\nmax-word-chars = 32\n",
    );
    assert!(
        err.contains("axis_limits.x") && err.contains("конечное"),
        "{err}"
    );
}

#[test]
fn prof_axis_valid_bounds_ok() {
    // валидные конечные min<=max — грузится (позитив к негативам выше)
    let p = load(
        "axis_ok.toml",
        "name = \"P\"\ndialect = \"linuxcnc\"\nemit-r-arcs = false\n\n\
         [axis-limits]\nx = { min = -5.0, max = 5.0 }\ny = { min = -1.0, max = 1.0 }\nz = { min = -1.0, max = 1.0 }\n\n\
         [limits]\nmax-ops-per-program = 200000\nmax-unroll-iterations = 10000\nmax-source-lines = 1000000\nmax-word-chars = 32\n",
    )
    .expect("валидные axis_limits");
    let a = p.axis_limits.expect("axis_limits");
    assert_eq!(a.x.min, -5.0);
    assert_eq!(a.x.max, 5.0);
}

#[test]
fn prof_missing_file_err() {
    let err = MachineProfile::load(&tmp().join("no_such_profile.toml")).expect_err("файла нет");
    assert!(err.contains("не удалось прочитать"), "{err}");
}

#[test]
fn prof_malformed_toml_err() {
    let err = err_of("malformed.toml", "name = \"незакрытая\nlimits = [1, 2\n");
    assert!(err.contains("невалидный TOML"), "{err}");
}

#[test]
fn prof_axis_partial_only_x_err() {
    // частично установлены (только x): y/z обязательны в AxisLimits → Err
    let err = err_of(
        "axis_partial.toml",
        "name = \"P\"\ndialect = \"linuxcnc\"\nemit-r-arcs = false\n\n\
         [axis-limits]\nx = { min = -1.0, max = 1.0 }\n\n\
         [limits]\nmax-ops-per-program = 1\nmax-unroll-iterations = 2\nmax-source-lines = 3\nmax-word-chars = 4\n",
    );
    assert!(err.contains("невалидный TOML"), "{err}");
}

#[test]
fn prof_axis_empty_table_err() {
    // пустая таблица [axis-limits]: ни одного поля → Err
    let err = err_of(
        "axis_empty.toml",
        "name = \"E\"\ndialect = \"linuxcnc\"\nemit-r-arcs = false\n\n\
         [axis-limits]\n\n\
         [limits]\nmax-ops-per-program = 1\nmax-unroll-iterations = 2\nmax-source-lines = 3\nmax-word-chars = 4\n",
    );
    assert!(err.contains("невалидный TOML"), "{err}");
}
