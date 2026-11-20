//! Corpus manifest — замороженный интерфейс v0.1 (CORPUS-CONTRACT §6).
//!
//! `load()` парсит `manifest.toml` (crates/eid-harness/Cargo.toml). Правила:
//!  * `expect`/`r4_expect`: "ok" → [`Outcome::Ok`], "warning" → [`Outcome::Warning`],
//!    "E_*" → [`Outcome::Code`] (обратный маппинг [`ErrorCode::as_str`]);
//!  * пути файлов/golden хранятся относительно corpus/ (каталога манифеста);
//!    `load` дописывает каталог манифеста и проверяет существование каждого
//!    пути (отсутствующий файл/каталог — `Err`, не молчаливый пропуск);
//!  * неизвестные поля (golden_tag, golden_note, note, bindings) —
//!    информационные, игнорируются (CORPUS §6: поля manifest шире структуры).
//!
//! Формат — TOML (миграция R-1, IMPLEMENTATION-PLAN §6, ruling оркестратора):
//! serde_yaml в maintenance mode с 2024 + unsafe-libyaml транзитивно в CLI;
//! corpus/manifest.yaml удалён (два формата = дрейф). Ключи и содержимое —
//! 1:1 с прежним YAML; диалектные ключи r4_expect квочены ("grbl-1.1" —
//! точка в bare key TOML недопустима).

use eid_core::diag::ErrorCode;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    A,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Positive,
    Negative,
}

/// Исход файла: ok / warning (Q3 — потеря метаданных записывается) / код отказа.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Ok,
    Warning,
    Code(ErrorCode),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub dialect: String, // "grbl-1.1" | "linuxcnc" | ...
    pub expect: Outcome,
    /// Коды диагностики, обязательные при Ok-исходе (line-level RAW: parse Ok,
    /// RAW-узел + диагностика — решение оркестратора, CORPUS §1.2 v0.5).
    pub expected_diags: Vec<ErrorCode>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    pub id: String,
    pub name: String,
    pub tier: Tier,
    pub kind: Kind,
    pub golden: Option<PathBuf>,
    pub files: Vec<FileEntry>,
    /// Явные per-vector ожидания эмиссии — НЕ выводятся из общих правил
    /// (CORPUS v0.3/§6; ревью). Пример: grbl-1.1 → E_UNSUPPORTED_SYNTAX (#27).
    pub r4_expect: BTreeMap<String, Outcome>,
    /// notice=true → только R1-AST, R1-text неприменим (CORPUS §4, v0.2).
    pub notice: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub corpus_version: String,
    pub vectors: Vec<Vector>,
    /// Tier B: реальные файлы (CORPUS §3/§7): parse-or-diagnose прогон.
    pub tier_b_files: Vec<TierBFile>,
}

/// Запись [[tier_b_files]] (CORPUS §6/§7): путь, диалект, ожидание
/// ("parse-or-diagnose"), лицензия, источник, sha256, тир публикации.
#[derive(Debug, Clone, PartialEq)]
pub struct TierBFile {
    pub path: PathBuf,
    pub dialect: String,
    pub expect: String,
    pub license: Option<String>,
    pub source: Option<String>,
    pub sha256: Option<String>,
    pub tier: Option<String>,
}

pub fn load(path: &Path) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read manifest {}: {e}", path.display()))?;
    let raw: ManifestToml =
        toml::from_str(&text).map_err(|e| format!("invalid manifest TOML: {e}"))?;
    // Пути внутри манифеста — относительно corpus/ (каталога manifest.toml).
    let root = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut vectors = Vec::with_capacity(raw.vectors.len());
    for v in raw.vectors {
        let tier = match v.tier.as_str() {
            "A" => Tier::A,
            "B" => Tier::B,
            other => return Err(format!("vector {}: invalid tier `{other}` (A|B)", v.id)),
        };
        let kind = match v.kind.as_str() {
            "positive" => Kind::Positive,
            "negative" => Kind::Negative,
            other => return Err(format!("vector {}: invalid kind `{other}` (positive|negative)", v.id)),
        };
        let golden = match v.golden.as_deref() {
            None | Some("null") => None,
            Some(rel) => Some(resolve_exists(&root, rel, &format!("golden of {}", v.id))?),
        };
        let mut files = Vec::with_capacity(v.files.len());
        for f in v.files {
            let expect = parse_outcome(&f.expect, &format!("file {}", f.path))?;
            let mut expected_diags = Vec::with_capacity(f.expected_diags.len());
            for c in &f.expected_diags {
                expected_diags.push(parse_code(c)?);
            }
            let p = resolve_exists(&root, &f.path, &format!("file of {}", v.id))?;
            files.push(FileEntry {
                path: p,
                dialect: f.dialect,
                expect,
                expected_diags,
                sha256: f.sha256,
            });
        }
        let mut r4_expect = BTreeMap::new();
        for (dialect, outcome) in v.r4_expect {
            let o = parse_outcome(&outcome, &format!("r4_expect {dialect}"))?;
            r4_expect.insert(dialect, o);
        }
        vectors.push(Vector {
            id: v.id,
            name: v.name,
            tier,
            kind,
            golden,
            files,
            r4_expect,
            notice: v.notice,
        });
    }
    // Инвариант: дубликаты id векторов — Err (id = ключ классификации/ссылок)
    let mut seen = std::collections::BTreeSet::new();
    for v in &vectors {
        if !seen.insert(v.id.clone()) {
            return Err(format!("duplicate vector id `{}`", v.id));
        }
    }

    let mut tier_b = Vec::new();
    for f in &raw.tier_b_files {
        let p = root.join(&f.path);
        if !p.is_file() {
            return Err(format!(
                "tier_b file `{}` does not exist under {}",
                f.path,
                root.display()
            ));
        }
        tier_b.push(TierBFile {
            path: p,
            dialect: f.dialect.clone(),
            expect: f.expect.clone().unwrap_or_else(|| "parse-or-diagnose".into()),
            license: f.license.clone(),
            source: f.source.clone(),
            sha256: f.sha256.clone(),
            tier: f.tier.clone(),
        });
    }
    Ok(Manifest {
        corpus_version: raw.corpus_version,
        vectors,
        tier_b_files: tier_b,
    })
}

/// Относительный путь → путь относительно corpus/ + проверка существования.
fn resolve_exists(root: &Path, rel: &str, what: &str) -> Result<PathBuf, String> {
    let p = root.join(rel);
    if !p.is_file() {
        return Err(format!(
            "{what}: path `{rel}` does not exist under {}",
            root.display()
        ));
    }
    Ok(p)
}

fn parse_outcome(s: &str, what: &str) -> Result<Outcome, String> {
    match s {
        "ok" => Ok(Outcome::Ok),
        "warning" => Ok(Outcome::Warning),
        _ => {
            if s.starts_with("E_") {
                Ok(Outcome::Code(parse_code(s)?))
            } else {
                Err(format!(
                    "{what}: invalid expect `{s}` (ok | warning | E_* — SPEC §8)"
                ))
            }
        }
    }
}

/// Обратный маппинг [`ErrorCode::as_str`] (строки кодов → ErrorCode).
fn parse_code(s: &str) -> Result<ErrorCode, String> {
    const ALL: [ErrorCode; 7] = [
        ErrorCode::InvalidInput,
        ErrorCode::UnsupportedSyntax,
        ErrorCode::UnsupportedSemantics,
        ErrorCode::RequiresMachineTable,
        ErrorCode::RuntimeState,
        ErrorCode::LimitExceeded,
        ErrorCode::RawUntranslatable,
    ];
    ALL.into_iter()
        .find(|c| c.as_str() == s)
        .ok_or_else(|| format!("invalid diag code `{s}` (7 codes, SPEC §8)"))
}

// ===========================================================================
// TOML-схема (serde). Публичные структуры выше — freeze v0.1; ниже —
// промежуточные, точно повторяют форму manifest.toml (CORPUS-CONTRACT §6).
// Неизвестные поля (golden_tag, golden_note, note, bindings) serde
// игнорирует по умолчанию; отсутствие ключа = null/пусто (TOML без null).
// ===========================================================================

#[derive(Deserialize)]
struct ManifestToml {
    corpus_version: String,
    #[serde(default)]
    #[allow(dead_code)]
    bindings: Option<toml::Value>,
    vectors: Vec<VectorToml>,
    #[serde(default)]
    tier_b_files: Vec<TierBFileToml>,
}

#[derive(Deserialize)]
struct TierBFileToml {
    path: String,
    dialect: String,
    #[serde(default)]
    expect: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    tier: Option<String>,
}

#[derive(Deserialize)]
struct VectorToml {
    id: String,
    name: String,
    tier: String,
    kind: String,
    #[serde(default)]
    golden: Option<String>,
    #[serde(default)]
    files: Vec<FileToml>,
    #[serde(default)]
    r4_expect: BTreeMap<String, String>,
    #[serde(default)]
    notice: bool,
}

#[derive(Deserialize)]
struct FileToml {
    path: String,
    dialect: String,
    expect: String,
    #[serde(default)]
    expected_diags: Vec<String>,
    #[serde(default)]
    sha256: Option<String>,
}
