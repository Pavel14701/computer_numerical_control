//! Trait адаптеров диалектов — интерфейс v0.2 (#22 AdapterOutput,
//! INTERFACE-V0.2-DESIGN §2.2: owning-результат вместо RefCell-буферов;
//! `Adapter: Send + Sync` — `Arc<dyn Adapter>` шарится между потоками,
//! compile-фейл при будущем interior mutability).

use crate::ast::Program;
use crate::diag::Diagnostic;
use crate::verify::Notice;

/// Owning-результат `Adapter::parse` (v0.2 #22).
///
/// Заменяет паттерн «parse потом take_notices» (v0.1): буферы адаптера
/// затирались повторным parse — теперь каждый parse возвращает СВОИ notices
/// и Ok-диагностики (linuxcnc RAW-узлы, fanuc/haas DPI-assumed и т.п.).
#[derive(Debug, Clone)]
pub struct ParseOutput {
    /// Канонический AST.
    pub program: Program,
    /// Drop-with-notice (SPEC §3/§5.2/§6; W_DPI_ASSUMED-класс — тоже сюда).
    /// Единый тип: [`crate::verify::Notice`] (NoticeKind-супермножество v0.2).
    pub notices: Vec<Notice>,
    /// Ok-путь diagnostics (правило двух классов, CORPUS §1.2 v0.5:
    /// line-level RAW-конструкции дают Ok + код в AST + запись здесь).
    pub diagnostics: Vec<Diagnostic>,
}

/// Owning-результат `Adapter::emit` (v0.2 #22).
#[derive(Debug, Clone)]
pub struct EmitOutput {
    /// Текст диалекта.
    pub text: String,
    /// Q3-потери (path_mode и т.п., COVERAGE §6.1) — строковый канал
    /// grbl/fanuc/haas.
    pub warnings: Vec<String>,
    /// Emit-stage диагностики Ok-пути (Diagnostic-канал linuxcnc).
    pub diagnostics: Vec<Diagnostic>,
}

/// Адаптер диалекта. v0.2: `Send + Sync` — supertrait-гарантия (данные
/// только, без interior mutability); `Arc<dyn Adapter>` — `Send + Sync`
/// контрактом, не надеждой.
pub trait Adapter: Send + Sync {
    /// Идентификатор диалекта: "grbl-1.1", "linuxcnc", "eid-l1" (R1-машина).
    fn dialect_name(&self) -> &'static str;

    /// Текст диалекта → EID Program + notices + Ok-диагностики.
    /// Отказ = Vec<Diagnostic> (SPEC §8).
    fn parse(&self, src: &str, file: &str) -> Result<ParseOutput, Vec<Diagnostic>>;

    /// EID Program → текст диалекта (+ warnings/diagnostics Ok-пути).
    /// Отказы эмиссии — по COVERAGE §5: вне словаря цели → UnsupportedSyntax;
    /// непредставимо ходами → UnsupportedSemantics; потеря метаданных
    /// (path_mode, Q3) → warning, НЕ отказ.
    fn emit(&self, program: &Program) -> Result<EmitOutput, Vec<Diagnostic>>;
}

// ===========================================================================
// EidTextAdapter — R1-машина для текстового формата EID/L1 (этап 2,
// IMPLEMENTATION-PLAN §2-A). Поверх parser::parse_with_notices + emitter::emit.
// ===========================================================================

/// Адаптер канонического текста EID/L1 (dialect_name = "eid-l1").
///
/// drop-with-notice (SPEC §3: zero-length motion / zero dwell) — не ошибка,
/// поэтому `Adapter::parse` возвращает `Ok(ParseOutput)` с notices в
/// owning-результате (или напрямую через `parser::parse_with_notices`).
#[derive(Debug, Default)]
pub struct EidTextAdapter;

impl EidTextAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Adapter for EidTextAdapter {
    fn dialect_name(&self) -> &'static str {
        "eid-l1"
    }

    fn parse(&self, src: &str, file: &str) -> Result<ParseOutput, Vec<Diagnostic>> {
        let out = crate::parser::parse_with_notices(src, file)?;
        Ok(ParseOutput {
            program: out.program,
            notices: out.notices,
            diagnostics: Vec::new(),
        })
    }

    fn emit(&self, program: &Program) -> Result<EmitOutput, Vec<Diagnostic>> {
        crate::emitter::emit(program).map(|text| EmitOutput {
            text,
            warnings: Vec::new(),
            diagnostics: Vec::new(),
        })
    }
}

// ===========================================================================
// Compile-time canary (T5, INTERFACE-V0.2-DESIGN §5): все адаптеры Sync,
// trait object — Send + Sync.
// ===========================================================================

#[cfg(test)]
mod sync_canary {
    use super::*;

    #[test]
    fn adapters_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EidTextAdapter>();
        assert_send_sync::<std::sync::Arc<dyn Adapter>>();
    }
}
