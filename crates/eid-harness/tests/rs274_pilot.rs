//! Тест критерия 8 (итерация 5): пилот #01 grbl→linuxcnc — каноническое
//! сравнение motion-потоков через rs274 (docker eid-linuxcnc-sim).
//! Оригинал (grbl) и перевод (linuxcnc) прогоняются через rs274, канонические
//! выводы парсятся и сравниваются с допусками EQUIVALENCE §4.
//! Guard: docker недоступен → SKIP (не красный).

use eid_core::adapter::Adapter;
use eid_grbl::GrblAdapter;
use eid_harness::rs274_compare::{compare_files, docker_available};
use std::path::{Path, PathBuf};

fn v1_pair(dir: &str, name: &str, dialect: &str) -> PathBuf {
    let suffix = if dialect == "grbl-1.1" { "grbl" } else { dialect };
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/tier_a/v1")
        .join(format!("{dir}_{name}"))
        .join(format!("{name}.{suffix}.ngc"))
}

fn simwork() -> PathBuf {
    let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/simwork");
    std::fs::create_dir_all(&d).expect("simwork dir");
    d
}

#[test]
fn pilot_01_canonical_comparison_zero_divergences() {
    if !docker_available() {
        eprintln!("SKIP: docker недоступен — критерий 8 переносится в docker-окружение");
        return;
    }
    // перевод grbl → linuxcnc через translate_checked (RAW-правило, Q3-lossy)
    let grbl_src = std::fs::read_to_string(v1_pair("01", "linear_box", "grbl-1.1"))
        .expect("v1 grbl pair readable");
    let grbl_adapter = GrblAdapter::new();
    let translated = grbl_adapter.parse(&grbl_src, "pilot01.grbl").expect("grbl parses");
    // эмитим linuxcnc-текст через LinuxCncAdapter (перевод)
    let lcnc = eid_linuxcnc::LinuxCncAdapter::new();
    let translated_text = lcnc.emit(&translated).expect("translate to linuxcnc");

    // кладём оба в simwork-каталог (монтируемый в docker)
    let work = simwork();
    let orig = work.join("pilot01_original.grbl.ngc");
    let tr = work.join("pilot01_translated.linuxcnc.ngc");
    std::fs::write(&orig, &grbl_src).unwrap();
    std::fs::write(&tr, &translated_text).unwrap();

    // rs274 на обоих → каноническое сравнение
    let divergences = compare_files(&orig, &tr, 1e-4).expect("rs274 runs");
    assert!(
        divergences.is_empty(),
        "pilot #01: канонические потоки разошлись: {divergences:?}"
    );
}
