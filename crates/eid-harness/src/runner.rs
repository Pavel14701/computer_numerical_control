//! runner — прогон корпуса по manifest (CORPUS-CONTRACT §4–§6, EQUIVALENCE §6).
//!
//! Golden-проверки (R1-AST + байт-в-байт эмиссия, R2) выполняются всегда через
//! EidTextAdapter (внутренний, не из registry); диалектные проверки (parse-
//! or-diagnose, R3-golden/R3-consistency, R1-text, R4) требуют адаптер диалекта
//! в [`Registry`] — отсутствующий адаптер даёт статус `Pending`, не crash.
//!
//! Правило двух классов негативов (CORPUS §1.2 v0.5):
//!  * структурный негатив: `expect = E_*` → `parse` обязан отказать
//!    `Err(код == expect)`;
//!  * line-level негатив: `expect = ok` + `expected_diags = [E_*]` →
//!    `parse = Ok`, AST содержит RAW-оп с кодом ∈ expected_diags.
//!
//! R4 — через [`crate::translate::translate_checked`] / `_golden` (CORPUS
//! v0.5.1 + v0.5.3): правило RAW-отказа — одна функция. Позитивы: golden-AST
//! (диалектно-нейтрален) — любой Raw отказывает любой цели; негативы: только
//! кросс-пары (цель ≠ диалект источника файла), same-dialect пара → Skipped
//! (verbatim-RAW легален).
//!
//! R3 — раздельно (CORPUS §4/§9, ruling оркестратора, контрольная точка 2–3):
//!  * `R3-golden`: parse_X(f) ≡ golden по отдельности;
//!  * `R3-consistency`: parse_grbl(f_grbl) ≡ parse_linuxcnc(f_linuxcnc).
//! Атрибуция: красный consistency при зелёном golden = оба парсера согласны,
//! оба неверны; красный consistency = адаптеры разошлись.

use crate::diff::{self, DiffConfig};
use crate::manifest::{Manifest, Outcome, Vector};
use crate::translate::{translate_checked, translate_checked_golden};
use eid_core::adapter::{Adapter, EidTextAdapter};
use eid_core::ast::Op;
use eid_core::diag::ErrorCode;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Реестр диалектных адаптеров (golden-проверки идут мимо него).
#[derive(Default)]
pub struct Registry {
    adapters: BTreeMap<String, Arc<dyn Adapter>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Регистрирует адаптер (ключ — [`Adapter::dialect_name`]).
    pub fn register(&mut self, adapter: Arc<dyn Adapter>) {
        self.adapters
            .insert(adapter.dialect_name().to_string(), adapter);
    }

    pub fn get(&self, dialect: &str) -> Option<&Arc<dyn Adapter>> {
        self.adapters.get(dialect)
    }

    pub fn dialects(&self) -> Vec<&str> {
        self.adapters.keys().map(String::as_str).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Pass,
    /// Красный: корпус/реализация расходятся с ожиданием (детали — текст).
    Fail(String),
    /// Адаптер диалекта ещё не в registry (этапы 3–5) — не crash.
    Pending(String),
    /// Неприменимо по контракту (notice исключает R1-text; same-dialect r4-
    /// пара v0.5.3; структурный негатив без Ok-источника).
    Skipped(String),
}

#[derive(Debug, Clone)]
pub struct Check {
    /// "R1" | "R2" | "R3-golden" | "R3-consistency" | "R1-text" | "R4" | "parse".
    pub relation: &'static str,
    /// Файл (или пара "dialect×dialect" / "golden").
    pub target: String,
    /// Ожидание из контракта/манифеста (для отчёта).
    pub expected: String,
    pub status: Status,
}

#[derive(Debug, Clone)]
pub struct VectorReport {
    pub id: String,
    pub name: String,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub pass: usize,
    pub fail: usize,
    pub pending: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub vectors: Vec<VectorReport>,
    pub summary: Summary,
}

impl Report {
    /// Зелёный = ноль Fail (Pending/Skipped не красят — этапы 3–5 дозаполнят).
    pub fn is_green(&self) -> bool {
        self.summary.fail == 0
    }

    pub fn checks(&self, relation: &str) -> Vec<&Check> {
        self.vectors
            .iter()
            .flat_map(|v| v.checks.iter())
            .filter(|c| c.relation == relation)
            .collect()
    }
}

/// Прогон всех векторов манифеста. Читает файлы по путям, разрешённым
/// `manifest::load` (относительно corpus/).
pub fn run(manifest: &Manifest, adapters: &Registry) -> Report {
    let cfg = DiffConfig::default();
    let eid = EidTextAdapter::new();
    let mut vectors = Vec::with_capacity(manifest.vectors.len());
    for v in &manifest.vectors {
        vectors.push(run_vector(v, &eid, adapters, &cfg));
    }
    let mut summary = Summary::default();
    for v in &vectors {
        for c in &v.checks {
            match c.status {
                Status::Pass => summary.pass += 1,
                Status::Fail(_) => summary.fail += 1,
                Status::Pending(_) => summary.pending += 1,
                Status::Skipped(_) => summary.skipped += 1,
            }
        }
    }
    Report { vectors, summary }
}

fn expect_desc(expect: &Outcome) -> String {
    match expect {
        Outcome::Ok => "parse Ok".to_string(),
        Outcome::Warning => "warning (Ok + запись потери)".to_string(),
        Outcome::Code(c) => format!("отказ {}", c.as_str()),
    }
}

fn run_vector(
    v: &Vector,
    eid: &EidTextAdapter,
    adapters: &Registry,
    cfg: &DiffConfig,
) -> VectorReport {
    let mut checks = Vec::new();
    // Ok-распарсенные файлы (для R3-consistency и R4-негативов, CORPUS v0.5.1):
    // (метка-цель, диалект источника, текст файла).
    let mut parsed_ok: Vec<(String, String, String)> = Vec::new();

    // --- Golden: R1-AST (parse→emit==файл байт-в-байт→parse→diff=0) + R2 -----
    let golden_ast = match &v.golden {
        Some(g) => match run_golden(g, eid, cfg) {
            Ok(ast) => {
                checks.push(Check {
                    relation: "R1",
                    target: "golden".to_string(),
                    expected: "emit(parse(golden)) == golden-файл байт-в-байт; re-parse ≡ golden"
                        .into(),
                    status: Status::Pass,
                });
                checks.push(Check {
                    relation: "R2",
                    target: "golden".to_string(),
                    expected: "parse→emit→parse→emit байт-в-байт (стабильность канона)".into(),
                    status: Status::Pass,
                });
                Some(ast)
            }
            Err(fails) => {
                for (relation, expected, msg) in fails {
                    checks.push(Check {
                        relation,
                        target: "golden".to_string(),
                        expected,
                        status: Status::Fail(msg),
                    });
                }
                None
            }
        },
        None => None,
    };

    // --- Диалектные файлы: parse-or-diagnose + правило двух классов ----------
    for f in &v.files {
        let target = f.path.display().to_string();
        let parse_expected = expect_desc(&f.expect);
        let adapter = match adapters.get(&f.dialect) {
            Some(a) => a,
            None => {
                checks.push(Check {
                    relation: "parse",
                    target: target.clone(),
                    expected: parse_expected.clone(),
                    status: Status::Pending(format!(
                        "no adapter for dialect `{}` (registry: {:?})",
                        f.dialect,
                        adapters.dialects()
                    )),
                });
                if v.kind_positive_with_golden()
                    && matches!(f.expect, Outcome::Ok | Outcome::Warning)
                {
                    checks.push(Check {
                        relation: "R3-golden",
                        target: target.clone(),
                        expected: "parse(file) ≡ golden AST".into(),
                        status: Status::Pending(format!(
                            "no adapter for dialect `{}`",
                            f.dialect
                        )),
                    });
                }
                continue;
            }
        };
        let text = match std::fs::read_to_string(&f.path) {
            Ok(t) => t,
            Err(e) => {
                checks.push(Check {
                    relation: "parse",
                    target: target.clone(),
                    expected: parse_expected.clone(),
                    status: Status::Fail(format!("cannot read file: {e}")),
                });
                continue;
            }
        };
        match adapter.parse(&text, &target) {
            Err(diags) => {
                let codes: Vec<&str> = diags.iter().map(|dg| dg.code.as_str()).collect();
                let status = match &f.expect {
                    Outcome::Code(expect_code) => {
                        // Структурный негатив: Err(code == expect) (CORPUS §1.2).
                        if diags.iter().any(|dg| dg.code == *expect_code) {
                            Status::Pass
                        } else {
                            Status::Fail(format!(
                                "expected refusal {}, got {}",
                                expect_code.as_str(),
                                codes.join(", ")
                            ))
                        }
                    }
                    Outcome::Ok => Status::Fail(format!(
                        "expected ok, refused: {}",
                        codes.join(", ")
                    )),
                    Outcome::Warning => Status::Fail(format!(
                        "expected warning(ok+loss), refused: {}",
                        codes.join(", ")
                    )),
                };
                checks.push(Check {
                    relation: "parse",
                    target: target.clone(),
                    expected: parse_expected.clone(),
                    status,
                });
            }
            Ok(program) => {
                let status = match &f.expect {
                    Outcome::Code(expect_code) => Status::Fail(format!(
                        "expected refusal {}, parsed ok (правило двух классов: структурный негатив не может быть Ok)",
                        expect_code.as_str()
                    )),
                    Outcome::Ok | Outcome::Warning => {
                        // Line-level: parse Ok + RAW{code} ∈ expected_diags.
                        let raw_codes: Vec<ErrorCode> = program
                            .ops
                            .iter()
                            .filter_map(|o| match o {
                                Op::Raw { code, .. } => Some(*code),
                                _ => None,
                            })
                            .collect();
                        let missing: Vec<&str> = f
                            .expected_diags
                            .iter()
                            .filter(|e| !raw_codes.contains(e))
                            .map(|e| e.as_str())
                            .collect();
                        if missing.is_empty() {
                            Status::Pass
                        } else {
                            Status::Fail(format!(
                                "expected diagnostics missing in AST: {} (raw codes: {:?})",
                                missing.join(", "),
                                raw_codes.iter().map(|c| c.as_str()).collect::<Vec<_>>()
                            ))
                        }
                    }
                };
                checks.push(Check {
                    relation: "parse",
                    target: target.clone(),
                    expected: parse_expected.clone(),
                    status,
                });

                // R3-golden: parse(f_dialect) ≡ golden (Tier A позитивы, Ok-исход).
                if v.kind_positive_with_golden()
                    && matches!(f.expect, Outcome::Ok | Outcome::Warning)
                {
                    if let Some(g) = &golden_ast {
                        let ds = diff::semantic_diff(&program, g, cfg);
                        checks.push(Check {
                            relation: "R3-golden",
                            target: target.clone(),
                            expected: "parse(file) ≡ golden AST (semantic_diff = 0)".into(),
                            status: if ds.is_empty() {
                                Status::Pass
                            } else {
                                Status::Fail(format!(
                                    "{} divergences vs golden: {:?}",
                                    ds.len(),
                                    ds
                                ))
                            },
                        });
                    }
                }
                // Ok-исход: текст пригоден как источник R3-consistency/R4.
                if matches!(f.expect, Outcome::Ok | Outcome::Warning) {
                    parsed_ok.push((target.clone(), f.dialect.clone(), text.clone()));
                }

                // R1-text: same-dialect RAW round-trip, payload байт-в-байт
                // (CORPUS §4; notice:true исключает R1-text).
                let has_raw = program.ops.iter().any(|o| matches!(o, Op::Raw { .. }));
                if matches!(f.expect, Outcome::Ok | Outcome::Warning) && has_raw {
                    if v.notice {
                        checks.push(Check {
                            relation: "R1-text",
                            target: target.clone(),
                            expected: "неприменимо (notice)".into(),
                            status: Status::Skipped(
                                "notice: true — R1-text исключён (CORPUS §4)".into(),
                            ),
                        });
                    } else {
                        let mut c = run_r1_text(&program, adapter.as_ref(), &target);
                        c.expected =
                            "same-dialect round-trip: RAW payload байт-в-байт (D9)".into();
                        checks.push(c);
                    }
                }
            }
        }
    }

    // --- R3-consistency: parse_grbl(f_grbl) ≡ parse_linuxcnc(f_linuxcnc) -----
    // Только позитивы с golden (оба файла одного AST); цепочка от первого
    // Ok-источника к каждому следующему с другим диалектом.
    if v.kind_positive_with_golden() && golden_ast.is_some() && parsed_ok.len() >= 2 {
        let (l0, d0, _) = (&parsed_ok[0].0, &parsed_ok[0].1, &parsed_ok[0].2);
        let mut chain: Vec<(String, String)> = vec![(d0.clone(), l0.clone())];
        for (label, dialect, text) in parsed_ok.iter().skip(1) {
            if chain.iter().any(|(d, _)| d == dialect) {
                continue; // тот же диалект уже в цепочке
            }
            let (prev_d, prev_l) = chain.last().expect("non-empty");
            let (a, b) = match (adapters.get(prev_d), adapters.get(dialect)) {
                (Some(pa), Some(pb)) => (pa, pb),
                _ => {
                    checks.push(Check {
                        relation: "R3-consistency",
                        target: format!("{prev_l}×{label}"),
                        expected: "parse(f_prev) ≡ parse(f_next) (different dialects)".into(),
                        status: Status::Pending("no adapter for one of the dialects".into()),
                    });
                    continue;
                }
            };
            let pa = match a.parse(&parsed_ok[0].2, prev_l) {
                Ok(p) => p,
                Err(diags) => {
                    checks.push(Check {
                        relation: "R3-consistency",
                        target: format!("{prev_l}×{label}"),
                        expected: "parse(f_prev) ≡ parse(f_next)".into(),
                        status: Status::Fail(format!(
                            "re-parse of {} failed: {}",
                            prev_l,
                            diags.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("; ")
                        )),
                    });
                    continue;
                }
            };
            let pb = match b.parse(text, label) {
                Ok(p) => p,
                Err(diags) => {
                    checks.push(Check {
                        relation: "R3-consistency",
                        target: format!("{prev_l}×{label}"),
                        expected: "parse(f_prev) ≡ parse(f_next)".into(),
                        status: Status::Fail(format!(
                            "re-parse of {} failed: {}",
                            label,
                            diags.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("; ")
                        )),
                    });
                    continue;
                }
            };
            let ds = diff::semantic_diff(&pa, &pb, cfg);
            checks.push(Check {
                relation: "R3-consistency",
                target: format!("{prev_l}×{label}"),
                expected: "parse(f_prev) ≡ parse(f_next) (semantic_diff = 0)".into(),
                status: if ds.is_empty() {
                    Status::Pass
                } else {
                    Status::Fail(format!("{} divergences between dialect parses: {:?}", ds.len(), ds))
                },
            });
            chain.push((dialect.clone(), label.clone()));
        }
    }

    // --- R4: ожидания эмиссии (r4_expect) через translate_checked ------------
    // Источник AST: golden (позитивы — любой Raw отказывает любой цели) либо
    // Ok-распарсенные файлы (негативы; v0.5.3: только кросс-пары, same-dialect
    // → Skipped). Нет адаптера цели → Pending; нет Ok-источника (структурный
    // негатив) → Skipped.
    for (dialect, expect) in &v.r4_expect {
        let r4_expected = match expect {
            Outcome::Code(c) => format!("emit отказ {}", c.as_str()),
            Outcome::Ok => "emit успех".to_string(),
            Outcome::Warning => "emit успех (warning-канал)".to_string(),
        };
        let adapter = match adapters.get(dialect) {
            Some(a) => a,
            None => {
                checks.push(Check {
                    relation: "R4",
                    target: dialect.clone(),
                    expected: r4_expected,
                    status: Status::Pending(format!("no adapter for dialect `{dialect}`")),
                });
                continue;
            }
        };
        let sources: Vec<(String, Option<String>, String)> = match &golden_ast {
            Some(g) => {
                // golden: эмитим AST напрямую (translate_checked_golden).
                match translate_checked_golden(g, adapter.as_ref()) {
                    Err(diags) => {
                        let status = match expect {
                            Outcome::Code(c) if diags.iter().any(|dg| dg.code == *c) => {
                                Status::Pass
                            }
                            Outcome::Code(c) => Status::Fail(format!(
                                "expected emit refusal {}, got {}",
                                c.as_str(),
                                diags.iter().map(|dg| dg.code.as_str()).collect::<Vec<_>>().join(", ")
                            )),
                            Outcome::Ok | Outcome::Warning => Status::Fail(format!(
                                "expected successful emit, refused: {}",
                                diags.iter().map(|dg| dg.code.as_str()).collect::<Vec<_>>().join(", ")
                            )),
                        };
                        checks.push(Check {
                            relation: "R4",
                            target: format!("golden→{dialect}"),
                            expected: r4_expected,
                            status,
                        });
                    }
                    Ok(_) => {
                        let status = match expect {
                            Outcome::Ok | Outcome::Warning => Status::Pass, // warning-канал не моделирован в freeze v0.1 trait
                            Outcome::Code(c) => Status::Fail(format!(
                                "expected emit refusal {0}, emitted ok",
                                c.as_str()
                            )),
                        };
                        checks.push(Check {
                            relation: "R4",
                            target: format!("golden→{dialect}"),
                            expected: r4_expected,
                            status,
                        });
                    }
                }
                continue;
            }
            None => parsed_ok
                .iter()
                .map(|(t, d, txt)| (t.clone(), Some(d.clone()), txt.clone()))
                .collect(),
        };
        if sources.is_empty() {
            checks.push(Check {
                relation: "R4",
                target: dialect.clone(),
                expected: r4_expected,
                status: Status::Skipped(
                    "нет источника AST: негатив без Ok-парса (структурный)".into(),
                ),
            });
            continue;
        }
        for (label, src_dialect, text) in &sources {
            let target = format!("{label}→{dialect}");
            let sd = src_dialect.as_deref().unwrap_or("");
            // v0.5.3: same-dialect пара у негатива — not-applicable.
            if sd == dialect {
                checks.push(Check {
                    relation: "R4",
                    target,
                    expected: r4_expected.clone(),
                    status: Status::Skipped(
                        "same-dialect пара: RAW-эмиссия легальна (verbatim, CORPUS v0.5.3)".into(),
                    ),
                });
                continue;
            }
            let from = match adapters.get(sd) {
                Some(a) => a,
                None => {
                    checks.push(Check {
                        relation: "R4",
                        target,
                        expected: r4_expected.clone(),
                        status: Status::Pending(format!(
                            "no adapter for source dialect `{sd}`"
                        )),
                    });
                    continue;
                }
            };
            let status = match translate_checked(from.as_ref(), adapter.as_ref(), text, label) {
                Err(diags) => match expect {
                    Outcome::Code(c) if diags.iter().any(|dg| dg.code == *c) => Status::Pass,
                    Outcome::Code(c) => Status::Fail(format!(
                        "expected emit refusal {}, got {}",
                        c.as_str(),
                        diags.iter().map(|dg| dg.code.as_str()).collect::<Vec<_>>().join(", ")
                    )),
                    Outcome::Ok | Outcome::Warning => Status::Fail(format!(
                        "expected successful emit, refused: {}",
                        diags.iter().map(|dg| dg.code.as_str()).collect::<Vec<_>>().join(", ")
                    )),
                },
                Ok(_) => match expect {
                    Outcome::Ok | Outcome::Warning => Status::Pass,
                    Outcome::Code(c) => Status::Fail(format!(
                        "expected emit refusal {0}, emitted ok",
                        c.as_str()
                    )),
                },
            };
            checks.push(Check { relation: "R4", target, expected: r4_expected.clone(), status });
        }
    }

    VectorReport { id: v.id.clone(), name: v.name.clone(), checks }
}

/// Golden-проверки: R1-AST (parse → emit==файл байт-в-байт → parse → diff=0)
/// + R2 (parse→emit→parse→emit байт-в-байт — стабильность канона).
/// Успех → Ok(golden AST) для последующих R3/R4.
fn run_golden(
    path: &std::path::Path,
    eid: &EidTextAdapter,
    cfg: &DiffConfig,
) -> Result<eid_core::ast::Program, Vec<(&'static str, String, String)>> {
    let mut fails: Vec<(&'static str, String, String)> = Vec::new();
    let r1_exp = "emit(parse(golden)) == golden-файл байт-в-байт; re-parse ≡ golden".to_string();
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            return Err(vec![("R1", r1_exp, format!("cannot read golden: {e}"))]);
        }
    };
    let p1 = match eid.parse(&text, "golden") {
        Ok(p) => p,
        Err(diags) => {
            return Err(vec![(
                "R1",
                r1_exp,
                format!(
                    "golden does not parse: {}",
                    diags.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("; ")
                ),
            )]);
        }
    };
    if !eid.take_notices().is_empty() {
        fails.push((
            "R1",
            r1_exp.clone(),
            "golden parse produced drop-with-notice — golden AST должен быть каноничным (CORPUS §4)"
                .into(),
        ));
    }
    let emitted = match eid.emit(&p1) {
        Ok(t) => t,
        Err(diags) => {
            return Err(vec![(
                "R1",
                r1_exp,
                format!(
                    "golden emit failed: {}",
                    diags.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("; ")
                ),
            )]);
        }
    };
    if emitted != text {
        fails.push(("R1", r1_exp.clone(), "emit(golden AST) != golden file byte-exact".into()));
    }
    let p2 = match eid.parse(&emitted, "golden") {
        Ok(p) => p,
        Err(diags) => {
            return Err(vec![(
                "R1",
                r1_exp,
                format!(
                    "re-parse of emitted golden failed: {}",
                    diags.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("; ")
                ),
            )]);
        }
    };
    let ds = diff::semantic_diff(&p1, &p2, cfg);
    if !ds.is_empty() {
        fails.push(("R1", r1_exp, format!("parse(emit(parse(golden))) diverges: {ds:?}")));
    }
    if !fails.is_empty() {
        return Err(fails);
    }
    // R2: canon(canon(x)) ≡ canon(x) — второй emit байт-в-байт.
    let stable = match eid.emit(&p2) {
        Ok(t2) => t2 == emitted,
        Err(_) => false,
    };
    if !stable {
        return Err(vec![(
            "R2",
            "parse→emit→parse→emit байт-в-байт".to_string(),
            "not byte-identical".into(),
        )]);
    }
    Ok(p1)
}

/// R1-text: RAW-payload same-dialect round-trip байт-в-байт (CORPUS §3/§4, D9).
fn run_r1_text(program: &eid_core::ast::Program, adapter: &dyn Adapter, target: &str) -> Check {
    let payload_of = |p: &eid_core::ast::Program| -> Vec<String> {
        p.ops
            .iter()
            .filter_map(|o| match o {
                Op::Raw { payload_hex, .. } => Some(payload_hex.clone()),
                _ => None,
            })
            .collect()
    };
    let original = payload_of(program);
    let re = match adapter.emit(program).and_then(|text| adapter.parse(&text, target)) {
        Ok(p) => p,
        Err(diags) => {
            return Check {
                relation: "R1-text",
                target: target.to_string(),
                expected: "same-dialect round-trip: RAW payload байт-в-байт (D9)".into(),
                status: Status::Fail(format!(
                    "same-dialect round-trip failed: {}",
                    diags.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("; ")
                )),
            }
        }
    };
    let after = payload_of(&re);
    let status = if after == original {
        Status::Pass
    } else {
        Status::Fail(format!(
            "RAW payload changed by same-dialect round-trip: {original:?} -> {after:?}"
        ))
    };
    Check {
        relation: "R1-text",
        target: target.to_string(),
        expected: "same-dialect round-trip: RAW payload байт-в-байт (D9)".into(),
        status,
    }
}

impl Vector {
    /// Позитив с golden AST (R3-диспозиция; Tier B-файлы без golden R3 не имеют).
    fn kind_positive_with_golden(&self) -> bool {
        matches!(
            (self.kind, self.golden.as_ref(), self.tier),
            (crate::manifest::Kind::Positive, Some(_), crate::manifest::Tier::A)
        )
    }
}
