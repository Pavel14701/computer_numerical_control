//! Trait адаптеров диалектов — замороженный интерфейс v0.1.

use crate::ast::Program;
use crate::diag::Diagnostic;

pub trait Adapter {
    /// Идентификатор диалекта: "grbl-1.1", "linuxcnc", "eid-l1" (R1-машина).
    fn dialect_name(&self) -> &'static str;

    /// Текст диалекта → EID Program. Отказ = Vec<Diagnostic> (SPEC §8).
    fn parse(&self, src: &str, file: &str) -> Result<Program, Vec<Diagnostic>>;

    /// EID Program → текст диалекта. Отказы эмиссии — по COVERAGE §5:
    /// вне словаря цели → UnsupportedSyntax; непредставимо ходами →
    /// UnsupportedSemantics; потеря метаданных (path_mode, Q3) → warning
    /// в source map, НЕ отказ.
    fn emit(&self, program: &Program) -> Result<String, Vec<Diagnostic>>;
}

// ===========================================================================
// EidTextAdapter — R1-машина для текстового формата EID/L1 (этап 2,
// IMPLEMENTATION-PLAN §2-A). Поверх parser::parse_with_notices + emitter::emit.
// Сигнатуры trait не меняются (freeze v0.1-interface).
// ===========================================================================

use std::cell::RefCell;
use crate::parser::Notice;

/// Адаптер канонического текста EID/L1 (dialect_name = "eid-l1").
///
/// drop-with-notice (SPEC §3: zero-length motion / zero dwell) — не ошибка,
/// поэтому `Adapter::parse` возвращает `Ok(Program)`, а notices собираются во
/// внутренний буфер; забрать их можно через [`EidTextAdapter::take_notices`]
/// (или напрямую через `parser::parse_with_notices`).
#[derive(Debug, Default)]
pub struct EidTextAdapter {
    notices: RefCell<Vec<Notice>>,
}

impl EidTextAdapter {
    pub fn new() -> Self {
        Self { notices: RefCell::new(Vec::new()) }
    }

    /// Notices последнего `parse` (отброшенные опы); опустошает буфер.
    pub fn take_notices(&self) -> Vec<Notice> {
        std::mem::take(&mut *self.notices.borrow_mut())
    }
}

impl Adapter for EidTextAdapter {
    fn dialect_name(&self) -> &'static str {
        "eid-l1"
    }

    fn parse(&self, src: &str, file: &str) -> Result<Program, Vec<Diagnostic>> {
        let out = crate::parser::parse_with_notices(src, file)?;
        *self.notices.borrow_mut() = out.notices;
        Ok(out.program)
    }

    fn emit(&self, program: &Program) -> Result<String, Vec<Diagnostic>> {
        crate::emitter::emit(program)
    }
}
