//! Corpus manifest — замороженный интерфейс v0.1 (CORPUS-CONTRACT §6).

use eid_core::diag::ErrorCode;
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
}

pub fn load(_path: &Path) -> Result<Manifest, String> {
    todo!("Агент B, итерация 1")
}
