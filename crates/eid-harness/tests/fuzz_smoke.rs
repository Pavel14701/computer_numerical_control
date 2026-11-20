//! Фаззер итерации 5 (CORPUS §2.1 + QA-таксономия addendum) — integration
//! (dev-deps: eid-grbl/eid-linuxcnc):
//!  * M-петля M1–M17: generate → mutate → emit → parse-back → сверка
//!    диспозиции (Expect::check_eid/check_dialect);
//!  * byte-level panic-freedom: произвольные/обрезанные/unicode/EOL-суп
//!    входы в диалектные парсеры — инвариант «никогда не паникует»
//!    (интерфейс Adapter — &str, байты подаются lossy-представлением);
//!  * shrinker: деление пополам до минимального паник-repro;
//!  * метаморфические отношения: comment / line-ending / whitespace /
//!    lowercase / N-слово инвариантности AST (RAW-файлы исключены из
//!    line-ending-метаморфики — payload семантичен, D9).
//!
//! Ничего не чинится: находки = кандидаты edge_cases (классификация §9 —
//! оркестратор). Детерминизм: один seed — идентичные находки.

use eid_core::adapter::{Adapter, EidTextAdapter};
use eid_core::parser;
use eid_grbl::GrblAdapter;
use eid_harness::generator::{self, GenConfig, Lcg};
use eid_harness::mutate::{self, Expect};
use eid_linuxcnc::LinuxCncAdapter;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
struct Finding {
    family: &'static str, // "m-op" | "panic" | "metamorphic"
    operator: &'static str,
    seed: u64,
    dialect: &'static str,
    detail: String,
}

// ===========================================================================
// Shrinker: деление пополам входной строки до минимального паник-repro
// ===========================================================================
fn shrink(input: &str, reproduces: &mut dyn FnMut(&str) -> bool) -> String {
    let mut cur = input.to_string();
    if !reproduces(&cur) {
        return cur;
    }
    loop {
        let len = cur.chars().count();
        if len <= 1 {
            return cur;
        }
        let head: String = cur.chars().take(len / 2).collect();
        if reproduces(&head) {
            cur = head;
            continue;
        }
        let tail: String = cur.chars().skip(len / 2).collect();
        if reproduces(&tail) {
            cur = tail;
            continue;
        }
        return cur;
    }
}

// ===========================================================================
// M-петля
// ===========================================================================
fn m_loop(findings: &mut Vec<Finding>, seed_base: u64, n: usize, lcnc: &LinuxCncAdapter) {
    let eid = EidTextAdapter::new();
    for op_id in mutate::OPERATORS {
        for i in 0..n {
            let seed = seed_base + i as u64;
            let cfg = GenConfig { seed, max_ops: 5 + i, ..GenConfig::default() };
            let program = generator::generate(&cfg);

            let base_text = {
                let t = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    eid.emit(&program).expect("base emit")
                })) {
                    Ok(t) => t,
                    Err(_) => {
                        findings.push(Finding {
                            family: "panic",
                            operator: op_id,
                            seed,
                            dialect: "eid-l1",
                            detail: "base emit panic (generator bug)".into(),
                        });
                        continue;
                    }
                };
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    eid.parse(&t, "fuzz-base").expect("base parse")
                })) {
                    Ok(_) => t,
                    Err(_) => {
                        findings.push(Finding {
                            family: "panic",
                            operator: op_id,
                            seed,
                            dialect: "eid-l1",
                            detail: "base parse panic".into(),
                        });
                        continue;
                    }
                }
            };

            // AST-оператор → eid-парсер
            if let Some((mutated, expect)) = mutate::apply(op_id, &program) {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let t = match eid.emit(&mutated) {
                        Ok(t) => t,
                        Err(e) => return Err(format!("emit: {e:?}")),
                    };
                    let res = parser::parse_with_notices(&t, "fuzz-m");
                    Ok((t, res))
                }));
                let (t, res) = match outcome {
                    Ok(Ok(v)) => v,
                    Ok(Err(msg)) => {
                        findings.push(Finding {
                            family: "panic",
                            operator: op_id,
                            seed,
                            dialect: "eid-l1",
                            detail: msg,
                        });
                        continue;
                    }
                    Err(_) => {
                        findings.push(Finding {
                            family: "panic",
                            operator: op_id,
                            seed,
                            dialect: "eid-l1",
                            detail: "panic".into(),
                        });
                        continue;
                    }
                };
                if !expect.check_eid(&res) {
                    findings.push(Finding {
                        family: "m-op",
                        operator: op_id,
                        seed,
                        dialect: "eid-l1",
                        detail: format!(
                            "expected {}, actual {}; текст {} байт",
                            expect.describe(),
                            match &res {
                                Err(d) => format!(
                                    "Err({})",
                                    d.iter().map(|x| x.code.as_str()).collect::<Vec<_>>().join(",")
                                ),
                                Ok(o) => format!("Ok + {} notices", o.notices.len()),
                            },
                            t.len()
                        ),
                    });
                }
            }

            // диалект-оператор → linuxcnc-парсер
            if let Some((mutated_text, expect)) = mutate::apply_dialect(op_id, &base_text) {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    lcnc.parse(&mutated_text, "fuzz-d")
                }));
                let res = match res {
                    Ok(r) => r,
                    Err(_) => {
                        findings.push(Finding {
                            family: "panic",
                            operator: op_id,
                            seed,
                            dialect: "linuxcnc",
                            detail: "dialect parse panic".into(),
                        });
                        continue;
                    }
                };
                if !expect.check_dialect(&res) {
                    findings.push(Finding {
                        family: "m-op",
                        operator: op_id,
                        seed,
                        dialect: "linuxcnc",
                        detail: format!(
                            "expected {}, actual {}",
                            expect.describe(),
                            match &res {
                                Err(d) => format!(
                                    "Err({})",
                                    d.iter().map(|x| x.code.as_str()).collect::<Vec<_>>().join(",")
                                ),
                                Ok(_) => "Ok".into(),
                            }
                        ),
                    });
                }
            }

            // текстовый оператор (eid-текст)
            if let Some((mutated_text, expect)) = mutate::apply_text(op_id, &base_text) {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    parser::parse_with_notices(&mutated_text, "fuzz-t")
                }));
                let res = match res {
                    Ok(r) => r,
                    Err(_) => {
                        findings.push(Finding {
                            family: "panic",
                            operator: op_id,
                            seed,
                            dialect: "eid-l1",
                            detail: "text parse panic".into(),
                        });
                        continue;
                    }
                };
                if !expect.check_eid(&res) {
                    findings.push(Finding {
                        family: "m-op",
                        operator: op_id,
                        seed,
                        dialect: "eid-l1",
                        detail: format!("expected {}, actual Ok", expect.describe()),
                    });
                }
            }
        }
    }
}

// ===========================================================================
// Byte-level panic-freedom + shrinker
// ===========================================================================
fn corpus_bytes() -> Vec<Vec<u8>> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tier_a/v0");
    let mut out = Vec::new();
    for rel in [
        "01_linear_box/linear_box.grbl.ngc",
        "01_linear_box/linear_box.linuxcnc.ngc",
        "08_arc_full_circle/arc_full_circle.linuxcnc.ngc",
        "20_chipbreak_G73/chipbreak_G73.linuxcnc.ngc",
    ] {
        if let Ok(b) = std::fs::read(base.join(rel)) {
            out.push(b);
        }
    }
    out
}

fn soup_inputs(seed: u64, corpus: &[Vec<u8>]) -> Vec<(String, Vec<u8>)> {
    let mut rng = Lcg::new(seed);
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    out.push(("empty".into(), Vec::new()));
    out.push(("lf-only".into(), b"\n".to_vec()));
    out.push(("crlf-only".into(), b"\r\n".to_vec()));
    out.push(("headers-repeated".into(), "G21 G90\n".repeat(100).into_bytes()));
    out.push((
        "unicode-soup".into(),
        "\u{1F600}\u{00A0}\u{2028}G21\u{0000}G90\n\u{FFFD}".as_bytes().to_vec(),
    ));
    for (i, b) in corpus.iter().enumerate() {
        if !b.is_empty() {
            out.push((format!("corpus{i}-truncated-quarter"), b[..b.len() / 4].to_vec()));
        }
        let mut noisy = b.clone();
        for _ in 0..32 {
            let pos = rng.below(noisy.len());
            noisy[pos] = (rng.next_u64() % 256) as u8;
        }
        out.push((format!("corpus{i}-byte-noise"), noisy));
        let mut mixed = Vec::new();
        for ch in b.iter() {
            if *ch == b'\n' {
                match rng.below(3) {
                    0 => mixed.extend_from_slice(b"\r\n"),
                    1 => mixed.push(b'\n'),
                    _ => mixed.push(b'\r'),
                }
            } else {
                mixed.push(*ch);
            }
        }
        out.push((format!("corpus{i}-eol-mix"), mixed));
    }
    for k in 0..4 {
        let n = 32 + rng.below(200);
        let v: Vec<u8> = (0..n).map(|_| (rng.next_u64() % 256) as u8).collect();
        out.push((format!("random{k}"), v));
    }
    out
}

fn byte_loop(findings: &mut Vec<Finding>, seed: u64, lcnc: &LinuxCncAdapter, grbl: &GrblAdapter) {
    let corpus = corpus_bytes();
    let adapters: [(&'static str, &dyn Adapter); 2] = [("grbl-1.1", grbl), ("linuxcnc", lcnc)];
    for (tag, input) in soup_inputs(seed, &corpus) {
        for (dname, adapter) in adapters {
            let lossy = String::from_utf8_lossy(&input).into_owned();
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                adapter.parse(&lossy, &tag)
            }));
            if run.is_err() {
                let minimal = shrink(&lossy, &mut |s: &str| {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let _ = adapter.parse(s, &tag);
                    }))
                    .is_err()
                });
                findings.push(Finding {
                    family: "panic",
                    operator: "byte-fuzz",
                    seed,
                    dialect: dname,
                    detail: format!(
                        "tag={tag}: parser PANIC; минимальный repro ({} chars): {:?}",
                        minimal.chars().count(),
                        minimal
                    ),
                });
            }
        }
    }
}

// ===========================================================================
// Метаморфические отношения (трансформация → тот же AST)
// ===========================================================================
fn metamorphic(findings: &mut Vec<Finding>) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tier_a/v0");
    let lcnc = LinuxCncAdapter::new();
    let grbl = GrblAdapter::new();
    let cases: Vec<(&'static str, &'static str, &'static str)> = vec![
        ("01_linear_box", "linear_box", "linuxcnc"),
        ("01_linear_box", "linear_box", "grbl-1.1"),
        ("17_drill_G81", "drill_G81", "linuxcnc"),
        ("28_stops", "stops", "linuxcnc"),
        ("22_bore_G89_dwell", "bore_G89_dwell", "linuxcnc"),
    ];
    for (dir, name, dialect) in cases {
        let p = base.join(format!("{dir}/{name}.{dialect}.ngc"));
        let Ok(orig) = std::fs::read_to_string(&p) else { continue };
        let adapter: &dyn Adapter = if dialect == "grbl-1.1" { &grbl } else { &lcnc };
        let Ok(ast_a) = adapter.parse(&orig, &name) else { continue };
        let check = |tag: &'static str, txt: &str, findings: &mut Vec<Finding>| {
            if let Ok(ast_b) = adapter.parse(txt, &name) {
                if ast_b != ast_a {
                    findings.push(Finding {
                        family: "metamorphic",
                        operator: tag,
                        seed: 0,
                        dialect,
                        detail: format!("{name}: AST изменился (инвариант нарушен)"),
                    });
                }
            }
        };
        // comment-invariance
        check(
            "comment-invariance",
            &format!("(fuzz comment)\n{orig}\n(fuzz end)"),
            findings,
        );
        // line-ending-invariance (LF / CRLF / MIXED)
        let lf = orig.replace("\r\n", "\n");
        let crlf = lf.replace('\n', "\r\n");
        let mixed: String = lf
            .lines()
            .enumerate()
            .map(|(i, l)| if i % 2 == 0 { format!("{l}\n") } else { format!("{l}\r\n") })
            .collect::<Vec<_>>()
            .join("");
        check("line-ending-invariance LF", &lf, findings);
        check("line-ending-invariance CRLF", &crlf, findings);
        check("line-ending-invariance MIXED", &mixed, findings);
        // whitespace-invariance
        let spaced: String = orig
            .lines()
            .map(|l| if l.starts_with('G') || l.starts_with('M') { l.replace(' ', "  ") } else { l.to_string() })
            .collect::<Vec<_>>()
            .join("\n");
        check("whitespace-invariance", &spaced, findings);
        // case-invariance
        check("case-invariance", &orig.to_lowercase(), findings);
        // n-word-invariance
        let nwords: String = orig
            .lines()
            .map(|l| {
                if l.starts_with('G') {
                    if l.starts_with("G0") || l.starts_with("G1") {
                        format!("N11 {l}")
                    } else {
                        format!("N10 {l}")
                    }
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        check("n-word-invariance", &nwords, findings);
    }
}

// ===========================================================================
// Движок
// ===========================================================================
fn run_full_suite(m_iterations: usize, byte_seed: u64) -> Vec<Finding> {
    let lcnc = LinuxCncAdapter::new();
    let grbl = GrblAdapter::new();
    let mut findings = Vec::new();
    m_loop(&mut findings, 100, m_iterations, &lcnc);
    byte_loop(&mut findings, byte_seed, &lcnc, &grbl);
    metamorphic(&mut findings);
    findings
}

#[test]
fn fuzz_smoke_deterministic_and_findings_collected() {
    let a = run_full_suite(3, 700);
    let b = run_full_suite(3, 700);
    assert_eq!(a, b, "один seed — идентичные находки (детерминизм)");
    for f in &a {
        println!(
            "[finding] family={} op={} seed={} dialect={} detail={}",
            f.family, f.operator, f.seed, f.dialect, f.detail
        );
    }
    println!("TOTAL findings: {}", a.len());
}

#[test]
fn fuzz_smoke_other_seed_runs_clean() {
    let c = run_full_suite(2, 900);
    println!("TOTAL findings (seed 900): {}", c.len());
}
