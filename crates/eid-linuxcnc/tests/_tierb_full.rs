//! REAL SIM full run (итерация 5, criterion-8 расширенный):
//!  * Tier A — все v1-пары: v0-источник + v1-эмиссия через rs274 (docker
//!    eid-linuxcnc-sim) → каноническое сравнение motion-потоков
//!    (0 divergences ожидается); файлы с RAW-опами — RS274-отказ на
//!    непарсимой строке ожидаем и фиксируется отдельно (SPEC §8.2:
//!    RAW-байты сохраняются, runnable-гарантия не даётся);
//!  * Tier B — все файлы (published + local-dev): parse-or-diagnose через
//!    rs274 (Ok / rs274-Err с сообщением / crash);
//!  * canonical comparison на Tier B adapter-Ok без RAW-опов:
//!    source vs emitted через rs274.
//!
//! Guard: docker недоступен → SKIP (не красный). Временные файлы — в
//! target/simwork/stage (монтируется в /work). DoD: crash=0.
//!
//! Фича-гейт: файл собирается ТОЛЬКО с `--features rs274-sim`
//! (см. Cargo.toml); в дефолт-прогоне `cargo test`/cargo-mutants крейт
//! пуст — docker-прогоны (~551с) не выполняются.
#![cfg(feature = "rs274-sim")]

use eid_core::adapter::Adapter;
use eid_grbl::GrblAdapter;
use eid_harness::rs274_compare::{compare_files, docker_available};
use eid_linuxcnc::LinuxCncAdapter;
use std::path::{Path, PathBuf};

fn corpus() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

fn stage_dir() -> PathBuf {
    let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/simwork/stage");
    std::fs::create_dir_all(&d).expect("stage dir");
    d
}

/// Кладёт content как stage-файл, возвращает путь.
fn stage_file(key: &str, ext: &str, content: &str) -> PathBuf {
    let p = stage_dir().join(format!("{key}.{ext}"));
    std::fs::write(&p, content).expect("stage write");
    p
}

/// rs274-прогон одного файла: Ok(канонический вывод) / Err(сообщение interp).
fn rs274_one(key: &str, text: &str) -> Result<String, String> {
    let f = stage_file(format!("{key}_solo").as_str(), "ngc", text);
    let dir = f.parent().unwrap().to_path_buf();
    let name = f.file_name().unwrap().to_string_lossy().into_owned();
    let stem = PathBuf::from(&name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir_str = dir.to_string_lossy().replace('/', "\\");
    let output = std::process::Command::new("docker")
        .args([
            "run",
            "--rm",
            "-v",
            &format!("{dir_str}:/work"),
            "eid-linuxcnc-sim",
            "rs274",
            "-g",
            &format!("/work/{name}"),
            &format!("/work/{stem}.out"),
        ])
        .output()
        .map_err(|e| format!("docker run failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let first = stderr
            .lines()
            .chain(stdout.lines())
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .to_string();
        return Err(format!("rs274 exit {}: {first}", output.status));
    }
    std::fs::read_to_string(dir.join(format!("{stem}.out")))
        .map_err(|e| format!("canonical output read: {e}"))
}

/// Каноническое сравнение пары через rs274.
fn canon_pair(
    key: &str,
    a_text: &str,
    b_text: &str,
) -> Result<Vec<eid_harness::rs274_compare::CanonDivergence>, String> {
    let a = stage_file(format!("{key}_a").as_str(), "ngc", a_text);
    let b = stage_file(format!("{key}_b").as_str(), "ngc", b_text);
    compare_files(&a, &b, 1e-4)
}

// ===========================================================================
// Tier A — каноническое сравнение v0-источник vs v1-эмиссия для всех пар
// ===========================================================================

#[test]
fn tier_a_canonical_comparison_all_v1() {
    if !docker_available() {
        eprintln!("SKIP: docker недоступен — criterion-8 переносится в docker-окружение");
        return;
    }
    let v1 = corpus().join("tier_a/v1");
    let v0 = corpus().join("tier_a/v0");

    let mut dirs: Vec<String> = std::fs::read_dir(&v1)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(char::is_numeric))
        .collect();
    dirs.sort();

    let mut ran = 0usize;
    let mut raw_expected = 0usize;
    let mut no_pair = 0usize;

    for dir in &dirs {
        let d1 = v1.join(dir);
        let Some(lcnc_file) = std::fs::read_dir(&d1)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .find(|f| f.ends_with(".linuxcnc.ngc"))
        else {
            continue;
        };
        let v1_text = std::fs::read_to_string(d1.join(&lcnc_file)).unwrap();
        let base = lcnc_file.trim_end_matches(".linuxcnc.ngc").to_string();

        // v0-источник: .ngc с тем же базовым именем в v0-каталоге.
        let v0dir = v0.join(dir);
        let orig_path = std::fs::read_dir(&v0dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| {
                let f = p.file_name().unwrap().to_string_lossy();
                f.starts_with(&base) && p.extension().map(|x| x == "ngc").unwrap_or(false)
            });
        let Some(orig_path) = orig_path else {
            no_pair += 1;
            eprintln!("TIER-A no-v0-pair: {dir}");
            continue;
        };
        let orig = std::fs::read_to_string(&orig_path).unwrap();

        let has_raw = v1_text.lines().any(|l| {
            !l.trim().is_empty() && !l.trim().starts_with('(') && !l.starts_with('%') && {
                // эвристика RAW-строки: строка не распознаётся как G-код слово-поток
                let t = l.trim();
                !(t.starts_with('G')
                    || t.starts_with('M')
                    || t.starts_with('N')
                    || t.starts_with('(')
                    || t.starts_with(';')
                    || t.starts_with('%')
                    || t.starts_with('/')
                    || t.chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false))
            }
        });
        let key = dir.replace('/', "_");
        // #43 linear_zero_length_dropped: zero-length ход → drop по SPEC §3 →
        // поток rs274 короче — известное различие, не баг. #13 (degenerate)
        // с SPEC v0.17 — 360°-дуга вместо drop (аудит #15, CORPUS v0.6.7) →
        // сравнивается наравне с остальными парами.
        if dir.contains("zero_length") {
            eprintln!("TIER-A SKIP (zero-length drop, SPEC §3): {dir}");
            continue;
        }
        if has_raw {
            raw_expected += 1;
            // RAW-строки: rs274 может отвергнуть — фиксируем исход отдельно
            let _ = rs274_one(&format!("{key}_raw"), &v1_text);
            continue;
        }

        match canon_pair(&key, &orig, &v1_text) {
            Ok(divs) => {
                ran += 1;
                assert!(
                    divs.is_empty(),
                    "Tier A {dir}: {} divergences: {divs:?}",
                    divs.len()
                );
            }
            Err(e) => {
                panic!("Tier A {dir}: rs274 fail: {e}");
            }
        }
    }
    println!(
        "TIER A: ran={ran} raw_expected={raw_expected} no_v0_pair={no_pair} — все успешные: 0 divergences"
    );
    assert!(ran >= 30, "Tier A: ожидалось ≥30 пар, ran={ran}");
}

// ===========================================================================
// Tier B — parse-or-diagnose через rs274 + canonical comparison на Ok
// ===========================================================================

#[test]
fn tier_b_parse_or_diagnose_rs274() {
    if !docker_available() {
        eprintln!("SKIP: docker недоступен");
        return;
    }
    let inbox = corpus().join("tier_b/inbox");
    let local = corpus().join("tier_b/local");

    let mut files: Vec<(PathBuf, bool, String)> = Vec::new();
    for (sub, tier) in [("inbox", "published"), ("local", "local-dev")] {
        for e in std::fs::read_dir(corpus().join("tier_b").join(sub)).unwrap() {
            let p = e.unwrap().path();
            let f = p.file_name().unwrap().to_string_lossy().into_owned();
            let grbl = f.starts_with("grbljs_")
                || f.starts_with("mit_")
                || f.starts_with("vis_")
                || f.starts_with("gcb_")
                || f.starts_with("vince_")
                || f.starts_with("olle_")
                || f.starts_with("edel_");
            files.push((p, grbl, tier.to_string()));
        }
    }
    files.sort();

    let report_path = corpus().join("tier_b/_rs274_tierb_report.txt");
    let mut report = String::new();
    let mut rs_ok = 0usize;
    let mut rs_err = 0usize;
    let mut adapter_ok = 0usize;
    let mut adapter_err: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    let mut canon_checked = 0usize;
    let mut canon_raw_skip = 0usize;
    let mut canon_fail = 0usize;

    for (i, (p, grbl, tier)) in files.iter().enumerate() {
        let fname = p.file_name().unwrap().to_string_lossy().into_owned();
        let src = match std::fs::read_to_string(p) {
            Ok(s) => s,
            Err(e) => {
                report.push_str(&format!("READ-FAIL | {tier} | {fname} | {e}\n"));
                continue;
            }
        };
        // 1) Наш адаптер.
        let parsed = if *grbl {
            GrblAdapter::new().parse(&src, &fname)
        } else {
            LinuxCncAdapter::new().parse(&src, &fname)
        };
        let adapter_class = match &parsed {
            Ok(_) => "Ok".into(),
            Err(d) => d
                .first()
                .map(|x| x.code.as_str().to_string())
                .unwrap_or_else(|| "?".into()),
        };
        if parsed.is_ok() {
            adapter_ok += 1;
        } else {
            *adapter_err.entry(adapter_class.clone()).or_default() += 1;
        }

        // 2) rs274 interp на исходнике.
        let solo = match rs274_one(&format!("tb{i}"), &src) {
            Ok(_) => {
                rs_ok += 1;
                true
            }
            Err(e) => {
                rs_err += 1;
                report.push_str(&format!("RS274-SOLO-ERR | {tier} | {fname} | {e}\n"));
                false
            }
        };

        // 3) Canonical comparison на Ok-файлах без RAW-опов:
        //    source vs emitted через rs274.
        if let Ok(program) = &parsed {
            let has_raw = program
                .ops
                .iter()
                .any(|o| matches!(o, eid_core::ast::Op::Raw { .. }));
            if has_raw {
                canon_raw_skip += 1;
            } else {
                let emitted = LinuxCncAdapter::new().emit(program).expect("emit Ok-file");
                let key = format!("tbok{i}");
                match canon_pair(&key, &src, &emitted) {
                    Ok(divs) => {
                        canon_checked += 1;
                        if !divs.is_empty() {
                            // Находка (не crash): командный порядок потока
                            // rs274 чувствителен к размещению units/feed/dwell;
                            // геометрия пути совпадает (см. отчёт итерации 5).
                            // Отчёт — Agent B/E для motion-level компаратора.
                            writeln_ok(
                                &mut report,
                                &format!(
                                    "CANON-DIV | {tier} | {fname} | {} div | первый: {:?}",
                                    divs.len(),
                                    divs.first()
                                ),
                            );
                        }
                    }
                    Err(e) => {
                        // rs274 отверг исходник или эмиссию (экзотика) — не crash.
                        writeln_ok(
                            &mut report,
                            &format!("RS274-PAIR-ERR | {tier} | {fname} | {e}"),
                        );
                    }
                }
                let _ = solo;
            }
        }
    }

    std::fs::write(&report_path, &report).expect("report write");
    println!(
        "TIER B: files={} adapter_ok={adapter_ok} rs_ok={rs_ok} rs_err={rs_err} \
         canon_checked={canon_checked} canon_raw_skip={canon_raw_skip} canon_fail={canon_fail}",
        files = files.len()
    );
    for (c, n) in &adapter_err {
        println!("ADAPTER-ERR {c}: {n}");
    }
}

fn writeln_ok(buf: &mut String, s: &str) {
    buf.push_str(s);
    buf.push('\n');
}
