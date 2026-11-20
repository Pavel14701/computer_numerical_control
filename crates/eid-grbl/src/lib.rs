//! eid-grbl — канонизатор и эмиттер GRBL 1.1 (Агент C, IMPLEMENTATION-PLAN §2-C).
//! Интерфейс заморожен. quirks — ИСПРАВЛЕННЫЙ список (COVERAGE §5/§6.11):
//! I/J/K literal X/Y/Z, эмиссия in-plane пар (G17: I+J, G18: I+K, G19: J+K);
//! tool_change → E_UNSUPPORTED_SYNTAX; G10 L20 → узел wcs_set_here (в словаре GRBL);
//! G61.1/path_mode → warning (Q3), не отказ; G43 H → E_REQUIRES_MACHINE_TABLE.
#![allow(dead_code)]

use eid_core::adapter::Adapter;
use eid_core::ast::Program;
use eid_core::diag::Diagnostic;

pub struct GrblAdapter;

impl Adapter for GrblAdapter {
    fn dialect_name(&self) -> &'static str {
        "grbl-1.1"
    }

    fn parse(&self, _src: &str, _file: &str) -> Result<Program, Vec<Diagnostic>> {
        todo!("Агент C, итерации 1–2: lexer + canonicalizer")
    }

    fn emit(&self, _program: &Program) -> Result<String, Vec<Diagnostic>> {
        todo!("Агент C, итерация 3: emitter v0")
    }
}
