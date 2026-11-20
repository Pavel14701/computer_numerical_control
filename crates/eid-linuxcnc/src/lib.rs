//! eid-linuxcnc — канонизатор и эмиттер LinuxCNC (Агент D, IMPLEMENTATION-PLAN §2-D).
//! Интерфейс заморожен. quirks: G90.1/G91.1 ABSORB; O-code/M98/M99 → SYNTAX;
//! `#`-параметры → RUNTIME_STATE, if/while → SYNTAX (SPEC §1.1 v0.3);
//! G10 L2 vs L20 — разные узлы; G92 mid-program → отказ; G93 → разрешение
//! в G94-feed на уровне опа; G5.x — scope decision RAW; M62–M73 → SEMANTICS;
//! G76 — токарная резьба, отказ (§1.2).
//!
//! Итерация 2 (Агент D): parse реализован (lexer.rs → parser.rs →
//! canonicalize.rs). emit — итерация 3 (todo!()).
//!
//! Правило двух классов (CORPUS §1.2 v0.5): line-level RAW-конструкции дают
//! `Ok(Program)`; их диагностики буферизуются и доступны через
//! [`LinuxCncAdapter::take_diagnostics`] (симметрично EidTextAdapter::
//! take_notices — trait v0.1 не имеет канала diagnostics при Ok).

#![allow(dead_code)]

pub mod canonicalize;
pub mod lexer;
pub mod parser;

use eid_core::adapter::Adapter;
use eid_core::ast::Program;
use eid_core::diag::Diagnostic;
use eid_core::parser::Notice;
use std::cell::RefCell;

pub struct LinuxCncAdapter {
    diagnostics: RefCell<Vec<Diagnostic>>,
    notices: RefCell<Vec<Notice>>,
}

impl Default for LinuxCncAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxCncAdapter {
    pub fn new() -> Self {
        Self { diagnostics: RefCell::new(Vec::new()), notices: RefCell::new(Vec::new()) }
    }

    /// Line-level диагностики последнего `parse` (Ok-путь: RAW-узлы, warnings);
    /// опустошает буфер.
    pub fn take_diagnostics(&self) -> Vec<Diagnostic> {
        std::mem::take(&mut *self.diagnostics.borrow_mut())
    }

    /// Notices последнего `parse` (отброшенные zero-length опы, SPEC §3);
    /// опустошает буфер.
    pub fn take_notices(&self) -> Vec<Notice> {
        std::mem::take(&mut *self.notices.borrow_mut())
    }
}

impl Adapter for LinuxCncAdapter {
    fn dialect_name(&self) -> &'static str {
        "linuxcnc"
    }

    fn parse(&self, src: &str, file: &str) -> Result<Program, Vec<Diagnostic>> {
        let out = parser::parse_program(src, file)?;
        *self.diagnostics.borrow_mut() = out.diagnostics;
        *self.notices.borrow_mut() = out.notices;
        Ok(out.program)
    }

    fn emit(&self, _program: &Program) -> Result<String, Vec<Diagnostic>> {
        todo!("Агент D, итерация 3: emitter v0")
    }
}
