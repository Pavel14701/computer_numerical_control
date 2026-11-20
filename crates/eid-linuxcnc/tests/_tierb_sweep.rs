//! ВРЕМЕННЫЙ parse-or-diagnose свип Tier B (CORPUS §7, критерий #7):
//! Ok / Err(класс) / crash. Результаты — в corpus/tier_b/_sweep_results.txt
//! (построчный flush: повисание = виден файл-виновник). Удаляется после отчёта.
use eid_core::adapter::Adapter;
use eid_grbl::GrblAdapter;
use eid_linuxcnc::LinuxCncAdapter;
use std::io::Write;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tier_b")
}

fn dialect_of(fname: &str) -> (&'static str, bool) {
    // (диалект-предположение, grbl-адаптер?)
    if fname.starts_with("grbljs_") {
        ("grbl", true)
    } else if fname.starts_with("mit_svg2g") || fname.starts_with("mit_pycnc") || fname.starts_with("mit_cws")
        || fname.starts_with("mit_shpk") || fname.starts_with("mit_tball") || fname.starts_with("mit_ddp")
    {
        ("grbl-подобный", true)
    } else if fname.starts_with("mit_hilbert") {
        ("linuxcnc-универсальный", false)
    } else {
        ("linuxcnc", false)
    }
}

#[test]
fn tierb_parse_or_diagnose_sweep() {
    let inbox = root().join("inbox");
    let local = root().join("local");
    let mut files: Vec<(PathBuf, bool, &'static str, &'static str)> = Vec::new();
    for e in std::fs::read_dir(&inbox).unwrap() {
        let p = e.unwrap().path();
        let fname = p.file_name().unwrap().to_string_lossy().to_string();
        let (dialect, grbl) = dialect_of(&fname);
        files.push((p, grbl, dialect, "published"));
    }
    for e in std::fs::read_dir(&local).unwrap() {
        let p = e.unwrap().path();
        files.push((p, false, "linuxcnc", "local-dev"));
    }
    files.sort();
    let log_path = root().join("_sweep_results.txt");
    let log_path_str = log_path.display().to_string();
    let mut log = std::io::BufWriter::new(std::fs::File::create(&log_path).unwrap());
    writeln!(log, "# Tier B parse-or-diagnose sweep ({} файлов)", files.len()).unwrap();
    writeln!(log, "# формат: RESULT | tier | dialect | файл | код/ops").unwrap();

    let mut ok = 0usize;
    let mut err_by_class: Vec<(String, usize)> = Vec::new();
    for (p, grbl, dialect, tier) in &files {
        let fname = p.file_name().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(p).unwrap_or_else(|e| {
            writeln!(log, "READ-FAIL | {tier} | {dialect} | {fname} | {e}").unwrap();
            String::new()
        });
        if src.is_empty() {
            continue;
        }
        let started = std::time::Instant::now();
        let adapter: &dyn Adapter = if *grbl { &GrblAdapter::new() } else { &LinuxCncAdapter::new() };
        let res = adapter.parse(&src, &fname);
        let dt = started.elapsed().as_millis();
        match res {
            Ok(program) => {
                writeln!(log, "OK | {tier} | {dialect} | {fname} | {} ops | {}ms", program.ops.len(), dt).unwrap();
                ok += 1;
            }
            Err(diags) => {
                let class = diags
                    .first()
                    .map(|d| d.code.as_str().to_string())
                    .unwrap_or_else(|| "NO-DIAGNOSTICS".to_string());
                let msg = diags
                    .first()
                    .map(|d| d.message.lines().next().unwrap_or("").to_string())
                    .unwrap_or_default();
                writeln!(log, "ERR | {tier} | {dialect} | {fname} | {class} | {msg}").unwrap();
                match err_by_class.iter_mut().find(|(c, _)| *c == class) {
                    Some((_, n)) => *n += 1,
                    None => err_by_class.push((class, 1)),
                }
            }
        }
        log.flush().unwrap();
    }
    writeln!(log, "# SUM: ok={ok} err={}", err_by_class.iter().map(|(_, n)| n).sum::<usize>()).unwrap();
    for (c, n) in &err_by_class {
        writeln!(log, "# ERR-CLASS {c}: {n}").unwrap();
    }
    drop(log);
    println!("SWEEP DONE: ok={ok} err_total={}; log: {log_path_str}", err_by_class.iter().map(|(_, n)| n).sum::<usize>());
    for (c, n) in &err_by_class {
        println!("ERR-CLASS {c}: {n}");
    }
}
