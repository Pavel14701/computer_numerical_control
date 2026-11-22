//! Смоук корпуса: 17 fanuc-файлов tier_b (COVERAGE §5.1 приёмка этапа 3).
//! Ожидание по manifest.toml: `expect = "parse-or-diagnose"` — файл обязан
//! либо канонизироваться (Ok), либо дать диагностику (Err с кодом) — НЕ
//! паниковать и НЕ молчать (COVERAGE §3 «не молча»). Результаты per-file —
//! в stdout (`--nocapture`), отчёт этапа 3; в sweep НЕ входит (sweep —
//! harness-агентов).
//!
//! Запуск: cargo test -p eid-fanuc --test fanuc_corpus_smoke -- --nocapture

use eid_core::adapter::Adapter;
use eid_fanuc::FanucAdapter;
use std::path::PathBuf;

struct Entry {
    path: String,
    expect: String,
}

/// Минимальный скан `[[tier_b_files]]`-блоков manifest.toml (без toml-депа):
/// таблица-массив → пары key = "value" до следующего заголовка.
fn fanuc_entries() -> Vec<Entry> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/manifest.toml");
    let src = std::fs::read_to_string(&manifest).expect("manifest.toml readable");
    let mut out = Vec::new();
    let mut cur_path: Option<String> = None;
    let mut cur_dialect: Option<String> = None;
    let mut cur_expect: Option<String> = None;
    let mut in_tier_b = false;
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("[[") {
            // Новый блок: зафиксировать предыдущий.
            if in_tier_b {
                if let (Some(p), Some(d), Some(e)) = (&cur_path, &cur_dialect, &cur_expect) {
                    if d == "fanuc" {
                        out.push(Entry {
                            path: p.clone(),
                            expect: e.clone(),
                        });
                    }
                }
            }
            in_tier_b = t.starts_with("[[tier_b_files]]");
            cur_path = None;
            cur_dialect = None;
            cur_expect = None;
            continue;
        }
        if !in_tier_b {
            continue;
        }
        if let Some(v) = t.strip_prefix("path = ") {
            cur_path = Some(v.trim_matches('"').to_string());
        } else if let Some(v) = t.strip_prefix("dialect = ") {
            cur_dialect = Some(v.trim_matches('"').to_string());
        } else if let Some(v) = t.strip_prefix("expect = ") {
            cur_expect = Some(v.trim_matches('"').to_string());
        }
    }
    if in_tier_b {
        if let (Some(p), Some(d), Some(e)) = (&cur_path, &cur_dialect, &cur_expect) {
            if d == "fanuc" {
                out.push(Entry {
                    path: p.clone(),
                    expect: e.clone(),
                });
            }
        }
    }
    out
}

#[test]
fn smoke_17_fanuc_corpus_files() {
    let entries = fanuc_entries();
    assert_eq!(
        entries.len(),
        17,
        "manifest: ожидается 17 fanuc-файлов tier_b"
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut ok = 0usize;
    let mut diagnosed = 0usize;
    println!("=== СМОУК eid-fanuc: 17 fanuc-файлов корпуса (parse-or-diagnose) ===");
    for e in &entries {
        let full = root.join(&e.path);
        let src = std::fs::read_to_string(&full)
            .unwrap_or_else(|err| panic!("{}: файл не читается: {err}", e.path));
        let adapter = FanucAdapter::new();
        let started = std::time::Instant::now();
        let result = std::panic::catch_unwind(move || adapter.parse(&src, &e.path));
        let dt = started.elapsed();
        match result {
            Err(_) => panic!("{}: ПАНИКА парсера (смоук-отказ)", e.path),
            Ok(Ok(out)) => {
                let program = out.program;
                // Same-dialect эмиссия: fixed-point t1 == t2 (байт-в-байт);
                // panic/Err на легальном AST = смоук-отказ эмиссии.
                let ad = FanucAdapter::new();
                let t1 = ad
                    .emit(&program)
                    .unwrap_or_else(|diags| panic!("{}: emit crashed: {diags:?}", e.path))
                    .text;
                let t2 = ad
                    .emit(&program)
                    .unwrap_or_else(|diags| panic!("{}: emit#2 crashed: {diags:?}", e.path))
                    .text;
                assert_eq!(t1, t2, "{}: emit недетерминирован", e.path);
                ok += 1;
                println!(
                    "OK          {:<40} ops={:<5} emit-ok expect={} ({:.1?})",
                    e.path,
                    program.ops.len(),
                    e.expect,
                    dt
                );
            }
            Ok(Err(diags)) => {
                diagnosed += 1;
                let code = diags.first().map(|d| d.code.as_str()).unwrap_or("?");
                let msg = diags
                    .first()
                    .map(|d| d.message.chars().take(110).collect::<String>())
                    .unwrap_or_default();
                let line = diags.first().map(|d| d.src.as_ref().map(|s| s.line_start));
                println!(
                    "DIAGNOSED   {:<40} {} ×{} line={:?} expect={} ({:.1?})\n             └ {msg}",
                    e.path,
                    code,
                    diags.len(),
                    line,
                    e.expect,
                    dt
                );
            }
        }
    }
    println!(
        "=== итог: ok={ok} diagnosed={diagnosed} total={} (crash=0) ===",
        ok + diagnosed
    );
    assert_eq!(ok + diagnosed, 17, "все 17 файлов — parse-or-diagnose");
}
