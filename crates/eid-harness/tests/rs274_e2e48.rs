//! e2e48 — criterion 8 на всех 48 позитивных векторах (итерация 6):
//! для векторов с парой grbl+linuxcnc: translate(grbl→linuxcnc через
//! адаптеры) → rs274 на ОБОИХ файлах → каноническое сравнение (допуски
//! EQUIVALENCE §4); для одно-диалектных (только linuxcnc: цикловые #17–#26,
//! lossy #41/#45, др.) — canonical self-round-trip rs274(f) vs
//! rs274(emit(parse(f))).
//!
//! Skip по ruling: #27 (r4-отказ tool_change), #47 (r4-отказ RAW),
//! #41/#45 grbl-сторона (lossy path_mode — regen="lossy").
//!
//! Находки НЕ чинятся: красные вербатимом для классификации §9
//! (AST-wrong / File-wrong / Spec-ambiguous / Adapter-wrong).

use eid_core::adapter::Adapter;
use eid_grbl::GrblAdapter;
use eid_harness::manifest;
use eid_linuxcnc::LinuxCncAdapter;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SKIP: [(&str, &str); 4] = [
    ("27", "r4-отказ: tool_change → E_UNSUPPORTED_SYNTAX в GRBL"),
    ("47", "r4-отказ: RAW → E_RAW_UNTRANSLATABLE (обе цели)"),
    ("41", "lossy: grbl-сторона — path_mode теряется (Q3), regen=lossy"),
    ("45", "lossy: grbl-сторона — path_mode теряется (Q3), regen=lossy"),
];

struct RedEntry {
    id: String,
    kind: &'static str, // "rs274-fail" | "divergence" | "translate-fail"
    detail: String,
}

fn rs274_run(root: &Path, rel: &str, findings: &mut Vec<RedEntry>, vid: &str) -> Option<String> {
    // canonicalize: убирает ../.. и смешанные разделители (Docker bind-путь
    // не резолвит точки) — интерфейс v0.1.2 transit-фикс repro
    let root = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
    let rel_vm = format!("/repo/{}", rel.replace('\\', "/"));
    let out_vm = format!("/repo/{}", rel.replace('\\', "/").trim_end_matches(".ngc").to_string() + ".out");
    // трейлинг backslash в источнике ломает bind-mount Docker Desktop
    // (/repo оказывается пуст — rs274 exit 1 «Unable to open file»)
    let mount = format!("{}:/repo", root.display());
    let output = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::process::Command::new("docker")
            .args(["run", "--rm", "-v", &mount, "eid-linuxcnc-sim", "rs274", "-g", &rel_vm, &out_vm])
            .output()
    }));
    let output = match output {
        Ok(Ok(o)) => o,
        _ => {
            findings.push(RedEntry {
                id: vid.into(),
                kind: "rs274-fail",
                detail: format!("docker/rs274 panic или не запустился ({rel})"),
            });
            return None;
        }
    };
    if !output.status.success() {
        let err = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        findings.push(RedEntry {
            id: vid.into(),
            kind: "rs274-fail",
            detail: format!(
                "{rel}: rs274 код={:?} mount={mount}: {err}",
                output.status.code(),
                mount = mount
            ),
        });
        return None;
    }
    let out_path = root.join(rel.replace('\\', "/").trim_end_matches(".ngc").to_string() + ".out");
    match std::fs::read_to_string(&out_path) {
        Ok(t) => Some(t),
        Err(e) => {
            findings.push(RedEntry {
                id: vid.into(),
                kind: "rs274-fail",
                detail: format!("{out_path:?} не прочитан: {e}"),
            });
            None
        }
    }
}

#[test]
fn e2e48_canonical_comparison_all_positives() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let m = manifest::load(&root.join("corpus/manifest.toml")).expect("manifest loads");
    let grbl = GrblAdapter::new();
    let lcnc = LinuxCncAdapter::new();

    let work = root.join("target/simwork");
    std::fs::create_dir_all(&work).unwrap();

    let mut reds: Vec<RedEntry> = Vec::new();
    let reds = Mutex::new(reds);
    let mut pairs_ok = 0usize;
    let mut self_ok = 0usize;
    let mut skipped = 0usize;
    let mut total = 0usize;

    for v in &m.vectors {
        if v.golden.is_none() {
            continue; // негативы — вне criterion 8
        }
        total += 1;
        if let Some((_, reason)) = SKIP.iter().find(|(id, _)| *id == v.id) {
            skipped += 1;
            println!("[skip] {} {}: {reason}", v.id, v.name);
            continue;
        }
        let grbl_file = v.files.iter().find(|f| f.dialect == "grbl-1.1");
        let lcnc_file = v.files.iter().find(|f| f.dialect == "linuxcnc");

        match (grbl_file, lcnc_file) {
            (Some(g), Some(l)) => {
                // ПАРА: translate(grbl→linuxcnc) → rs274 на обоих
                let grbl_text = std::fs::read_to_string(&g.path).expect("grbl readable");
                let ast = match grbl.parse(&grbl_text, &g.path.display().to_string()) {
                    Ok(a) => a,
                    Err(d) => {
                        reds.lock().unwrap().push(RedEntry {
                            id: v.id.clone(),
                            kind: "translate-fail",
                            detail: format!("grbl parse: {d:?}"),
                        });
                        continue;
                    }
                };
                let translated = match lcnc.emit(&ast) {
                    Ok(t) => t,
                    Err(d) => {
                        reds.lock().unwrap().push(RedEntry {
                            id: v.id.clone(),
                            kind: "translate-fail",
                            detail: format!(
                                "linuxcnc emit: {}",
                                d.iter().map(|x| x.code.as_str()).collect::<Vec<_>>().join(",")
                            ),
                        });
                        continue;
                    }
                };
                let tr_file = root.join("target/simwork").join(format!("{}_translated.linuxcnc.ngc", v.id));
                std::fs::write(&tr_file, &translated).unwrap();

                let mut red_here = false;
                {
                    let mut r = reds.lock().unwrap();
                    let rel = g.path.strip_prefix(&root).expect("manifest path under root").to_string_lossy().replace('\\', "/");
                    let ca = rs274_run(&root, &rel, &mut r, &v.id);
                    let rel_t = tr_file.strip_prefix(&root).expect("under root").to_string_lossy().replace('\\', "/");
                    let cb = rs274_run(&root, &rel_t, &mut r, &v.id);
                    drop(r);
                    if let (Some(ta), Some(tb)) = (ca, cb) {
                        let div = eid_harness::rs274_compare::compare_canonical(
                            &eid_harness::rs274_compare::parse_canonical(&ta),
                            &eid_harness::rs274_compare::parse_canonical(&tb),
                            1e-4,
                        );
                        if div.is_empty() {
                            pairs_ok += 1;
                            println!("[ok] {} {}: canonical 0 divergences", v.id, v.name);
                        } else {
                            red_here = true;
                            let mut r = reds.lock().unwrap();
                            for d in &div {
                                r.push(RedEntry {
                                    id: v.id.clone(),
                                    kind: "divergence",
                                    detail: format!(
                                        "canonical: {} — {}",
                                        d.index, d.detail
                                    ),
                                });
                            }
                        }
                    } else {
                        red_here = true;
                    }
                }
                if red_here {
                    println!("[red] {} {} — см. отчёт", v.id, v.name);
                }
            }
            (None, Some(l)) => {
                // ОДИН-диалектный (linuxcnc only): canonical self-round-trip
                let text = std::fs::read_to_string(&l.path).expect("readable");
                let ast = match lcnc.parse(&text, &l.path.display().to_string()) {
                    Ok(a) => a,
                    Err(d) => {
                        reds.lock().unwrap().push(RedEntry {
                            id: v.id.clone(),
                            kind: "translate-fail",
                            detail: format!("linuxcnc parse: {d:?}"),
                        });
                        continue;
                    }
                };
                let re_emitted = match lcnc.emit(&ast) {
                    Ok(t) => t,
                    Err(d) => {
                        reds.lock().unwrap().push(RedEntry {
                            id: v.id.clone(),
                            kind: "translate-fail",
                            detail: format!(
                                "linuxcnc re-emit: {}",
                                d.iter().map(|x| x.code.as_str()).collect::<Vec<_>>().join(",")
                            ),
                        });
                        continue;
                    }
                };
                let tr_file = root.join("target/simwork").join(format!("{}_self.linuxcnc.ngc", v.id));
                std::fs::write(&tr_file, &re_emitted).unwrap();

                let mut r = reds.lock().unwrap();
                let rel = l.path.strip_prefix(&root).expect("manifest path under root").to_string_lossy().replace('\\', "/");
                let ca = rs274_run(&root, &rel, &mut r, &v.id);
                let rel_t = tr_file.strip_prefix(&root).expect("under root").to_string_lossy().replace('\\', "/");
                    let cb = rs274_run(&root, &rel_t, &mut r, &v.id);
                drop(r);
                if let (Some(ta), Some(tb)) = (ca, cb) {
                    let div = eid_harness::rs274_compare::compare_canonical(
                        &eid_harness::rs274_compare::parse_canonical(&ta),
                        &eid_harness::rs274_compare::parse_canonical(&tb),
                        1e-4,
                    );
                    if div.is_empty() {
                        self_ok += 1;
                        println!("[ok] {} {}: canonical self-round-trip 0 divergences", v.id, v.name);
                    } else {
                        let mut r = reds.lock().unwrap();
                        for d in &div {
                            r.push(RedEntry {
                                id: v.id.clone(),
                                kind: "divergence",
                                detail: format!("canonical self: {} — {}", d.index, d.detail),
                            });
                        }
                    }
                }
            }
            _ => {
                skipped += 1;
                println!("[skip] {} {}: нет диалектных файлов", v.id, v.name);
            }
        }
    }

    // --- Сводка ---------------------------------------------------------------
    let reds = reds.into_inner().unwrap();
    println!("=== E2E48 SUMMARY ===");
    println!("total positives: {total}, pairs-ok: {pairs_ok}, self-ok: {self_ok}, skipped: {skipped}, red entries: {}", reds.len());
    for r in &reds {
        println!(
            "[red] {} {} {} — {}",
            r.id,
            r.kind,
            r.id,
            r.detail
        );
    }
    let _ = reds;
}
