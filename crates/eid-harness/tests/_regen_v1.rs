//! ОДНОРАЗОВЫЙ инструмент регенерации v1-пар (закрытие долга CORPUS §1,
//! итерация 4): emit(golden AST) через GrblAdapter/LinuxCncAdapter ->
//! corpus/tier_a/v1/<vector>/<name>.<dialect>.ngc + самопроверка
//! (parse(v1) ≡ golden diff=0; emit(parse(v1)) == v1 байт-в-байт).
//! Цикловые #17–#26 исключены (закрыты в v5). После использования удалить.

use eid_core::adapter::{Adapter, EidTextAdapter};
use eid_grbl::GrblAdapter;
use eid_linuxcnc::LinuxCncAdapter;
use eid_harness::diff::DiffConfig;
use eid_harness::manifest;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CYCLES: [&str; 10] = [
    "17", "18", "19", "20", "21", "22", "23", "24", "25", "26",
];

#[test]
fn regenerate_v1_pairs_once() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let m = manifest::load(&root.join("corpus/manifest.toml")).expect("manifest loads");
    let eid = EidTextAdapter::new();
    let grbl = Arc::new(GrblAdapter::new());
    let lcnc = Arc::new(LinuxCncAdapter::new());
    let cfg = DiffConfig::default();
    let mut report = String::new();
    let mut regenerated = 0usize;
    let mut diverged = 0usize;
    let mut skipped_emit = 0usize;

    for v in &m.vectors {
        if CYCLES.contains(&v.id.as_str()) {
            continue;
        }
        let Some(golden_path) = &v.golden else { continue }; // негативы — вне v1
        let golden_text = std::fs::read_to_string(golden_path).expect("golden readable");
        let golden = eid.parse(&golden_text, &v.id).expect("golden parses");
        assert!(eid.take_notices().is_empty(), "{}: golden notices", v.id);

        let short = v.name.as_str();
        let v1_dir = root.join("corpus/tier_a/v1").join(format!("{}_{}", v.id, v.name));
        let pairs: [(&str, &dyn Adapter); 2] =
            [("grbl-1.1", grbl.as_ref()), ("linuxcnc", lcnc.as_ref())];
        for (dialect, adapter) in pairs {
            let text = match adapter.emit(&golden) {
                Ok(t) => t,
                Err(diags) => {
                    skipped_emit += 1;
                    report.push_str(&format!(
                        "SKIP-EMIT {} {} -> {}: {}\n",
                        v.id,
                        dialect,
                        diags.iter().map(|d| d.code.as_str()).collect::<Vec<_>>().join(","),
                        diags.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("; ")
                    ));
                    continue;
                }
            };
            let dir = PathBuf::from(&v1_dir);
            std::fs::create_dir_all(&dir).expect("v1 dir");
            let file = dir.join(format!("{short}.{dialect}.ngc"));
            std::fs::write(&file, &text).expect("v1 writable");
            // Самопроверка: parse(v1) ≡ golden; двойная эмиссия байт-в-байт.
            // Расхождение — не прерывание: вербатим в отчёт (классификация §9).
            let mut ok = true;
            match adapter.parse(&text, &file.display().to_string()) {
                Err(diags) => {
                    ok = false;
                    report.push_str(&format!(
                        "FAIL-CHECK {} {} parse-back: {}\n",
                        v.id,
                        dialect,
                        diags
                            .iter()
                            .map(|d| format!("{} {}", d.code.as_str(), d.message))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ));
                }
                Ok(back) => {
                    let ds = eid_harness::diff::semantic_diff(&back, &golden, &cfg);
                    if !ds.is_empty() {
                        ok = false;
                        report.push_str(&format!(
                            "FAIL-CHECK {} {} v1!=golden: {:?}\n",
                            v.id, dialect, ds
                        ));
                    }
                    match adapter.emit(&back) {
                        Err(diags) => {
                            ok = false;
                            report.push_str(&format!(
                                "FAIL-CHECK {} {} re-emit refused: {}\n",
                                v.id,
                                dialect,
                                diags
                                    .iter()
                                    .map(|d| d.code.as_str())
                                    .collect::<Vec<_>>()
                                    .join(",")
                            ));
                        }
                        Ok(re_emitted) => {
                            if re_emitted != text {
                                ok = false;
                                report.push_str(&format!(
                                    "FAIL-CHECK {} {} emit(parse(emit)) != emit\n",
                                    v.id, dialect
                                ));
                            }
                        }
                    }
                }
            }
            if ok {
                regenerated += 1;
                report.push_str(&format!("OK {} {} -> {}\n", v.id, dialect, file.display()));
            } else {
                diverged += 1;
            }
        }
        let _ = golden_path;
    }
    report.push_str(&format!(
        "TOTAL regenerated: {regenerated}, diverged: {diverged}, skip-emit: {skipped_emit}\n"
    ));
    std::fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/v1_regen_report.txt"),
        &report,
    )
    .expect("report writable");
    println!("{report}");
}
