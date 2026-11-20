//! eid-grbl — канонизатор и эмиттер GRBL 1.1 (Агент C, IMPLEMENTATION-PLAN §2-C).
//! Интерфейс заморожен. quirks — ИСПРАВЛЕННЫЙ список (COVERAGE §5/§6.11):
//! I/J/K literal X/Y/Z, эмиссия in-plane пар (G17: I+J, G18: I+K, G19: J+K);
//! tool_change → E_UNSUPPORTED_SYNTAX; G10 L20 → узел wcs_set_here (в словаре GRBL);
//! G61.1/path_mode → warning (Q3), не отказ; G43 H → E_REQUIRES_MACHINE_TABLE.
#![allow(dead_code)]

use std::cell::RefCell;
use eid_core::adapter::Adapter;
use eid_core::ast::Program;
use eid_core::diag::Diagnostic;

pub mod block;
pub mod canonicalize;
pub mod gcode;
pub mod lexer;

/// Адаптер GRBL 1.1 (dialect_name = "grbl-1.1").
/// drop-with-notice (zero-length / near-degenerate R-arc) — не ошибка:
/// notices буферизуются, забрать через [`GrblAdapter::take_notices`]
/// (паттерн EidTextAdapter, eid-core adapter.rs).
#[derive(Debug, Default)]
pub struct GrblAdapter {
    notices: RefCell<Vec<canonicalize::Notice>>,
}

impl GrblAdapter {
    pub fn new() -> Self {
        Self { notices: RefCell::new(Vec::new()) }
    }

    /// Notices последнего `parse` (отброшенные zero-length / near-degenerate);
    /// опустошает буфер.
    pub fn take_notices(&self) -> Vec<canonicalize::Notice> {
        std::mem::take(&mut *self.notices.borrow_mut())
    }
}

impl Adapter for GrblAdapter {
    fn dialect_name(&self) -> &'static str {
        "grbl-1.1"
    }

    fn parse(&self, src: &str, file: &str) -> Result<Program, Vec<Diagnostic>> {
        let out = canonicalize::canonicalize_with_notices(src, file)?;
        *self.notices.borrow_mut() = out.notices;
        Ok(out.program)
    }

    fn emit(&self, _program: &Program) -> Result<String, Vec<Diagnostic>> {
        todo!("Агент C, итерация 3: emitter v0")
    }
}
