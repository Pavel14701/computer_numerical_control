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
