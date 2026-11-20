//! Диагностика — интерфейс v0.1.2 (freeze v0.1 + аддитивное добавление
//! `Stage`, итерация 4). Коды — SPEC §8 / COVERAGE §1.1.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    InvalidInput,         // E_INVALID_INPUT — вход сломан
    UnsupportedSyntax,    // E_UNSUPPORTED_SYNTAX — вне грамматики/словаря L1 или цели
    UnsupportedSemantics, // E_UNSUPPORTED_SEMANTICS — не представимо ходами
    RequiresMachineTable, // E_REQUIRES_MACHINE_TABLE — нужна таблица станка
    RuntimeState,         // E_RUNTIME_STATE — зависит от состояния исполнения
    LimitExceeded,        // E_LIMIT_EXCEEDED — guard транслятора (COVERAGE §1.2)
    RawUntranslatable,    // E_RAW_UNTRANSLATABLE — кросс-диалект при RAW
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidInput => "E_INVALID_INPUT",
            Self::UnsupportedSyntax => "E_UNSUPPORTED_SYNTAX",
            Self::UnsupportedSemantics => "E_UNSUPPORTED_SEMANTICS",
            Self::RequiresMachineTable => "E_REQUIRES_MACHINE_TABLE",
            Self::RuntimeState => "E_RUNTIME_STATE",
            Self::LimitExceeded => "E_LIMIT_EXCEEDED",
            Self::RawUntranslatable => "E_RAW_UNTRANSLATABLE",
        }
    }
}

/// Этап пайплайна, на котором возникла диагностика (interface v0.1.2,
/// итерация 4). MLIR-паттерн: каждый pass помечает свои диагностики,
/// потребители (harness, CLI) фильтруют по этапу.
/// Порядок потока: parse → canonicalize → verify → emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Лексика/структура входного текста (любой диалект, включая EID).
    Parse,
    /// Канонизация диалект → AST (адаптеры источников, агенты C/D).
    Canonicalize,
    /// Верификация канонического AST — pass `eid_core::verify`.
    Verify,
    /// Эмиссия: ограничения целевого диалекта/формата.
    Emit,
}

/// «что» + «где (source map)» + «варианты» (SPEC §8); пороги — для LimitExceeded;
/// «этап пайплайна» (v0.1.2).
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: ErrorCode,
    pub stage: Stage,
    pub message: String,
    pub src: Option<crate::ast::SrcSpan>,
    pub options: Vec<String>,
    pub limit: Option<LimitInfo>,
}

#[derive(Debug, Clone)]
pub struct LimitInfo {
    pub threshold: &'static str,
    pub threshold_value: f64,
    pub actual: f64,
}
