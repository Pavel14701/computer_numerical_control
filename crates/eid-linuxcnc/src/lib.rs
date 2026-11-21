//! eid-linuxcnc — канонизатор и эмиттер LinuxCNC (Агент D, IMPLEMENTATION-PLAN §2-D).
//! Интерфейс заморожен. quirks: G90.1/G91.1 ABSORB; O-code/M98/M99 → SYNTAX;
//! `#`-параметры → RUNTIME_STATE, if/while → SYNTAX (SPEC §1.1 v0.3);
//! G10 L2 vs L20 — разные узлы; G92 mid-program → отказ; G93 → разрешение
//! в G94-feed на уровне опа; G5.x — scope decision RAW; M62–M73 → SEMANTICS;
//! G76 — токарная резьба, отказ (§1.2).
//!
//! Итерация 2 (Агент D): parse реализован (lexer.rs → parser.rs →
//! canonicalize.rs). Итерация 3: emit v0 (emit.rs; COVERAGE §5, EQUIVALENCE
//! §1.4; warnings → take_diagnostics).
//!
//! Правило двух классов (CORPUS §1.2 v0.5): line-level RAW-конструкции дают
//! `Ok(Program)`; их диагностики буферизуются и доступны через
//! [`LinuxCncAdapter::take_diagnostics`] (симметрично EidTextAdapter::
//! take_notices — trait v0.1 не имеет канала diagnostics при Ok).

pub mod canonicalize;
pub mod emit;
pub mod lexer;
pub mod parser;

use eid_core::adapter::Adapter;
use eid_core::ast::Program;
use eid_core::diag::Diagnostic;
use crate::parser::Notice;
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

    /// Таблица легальности эмиссии LinuxCNC (COVERAGE §5, LinuxCNC-колонка;
    /// ConversionTarget-паттерн, зеркально eid-grbl). Словарь полный:
    /// все 10 видов опов легальны; path_mode/WCS вне словаря невозможны
    /// (G54–G59.3, G61/G61.1/G64 — весь словарь целевого).
    pub fn emission_legality(&self) -> emit::EmissionLegality {
        use eid_core::diag::ErrorCode::{
            InvalidInput as INVAL, RuntimeState as RT, UnsupportedSemantics as SEM,
        };
        emit::EmissionLegality {
            legal: vec![
                "rapid",
                "linear",
                "arc",
                "dwell",
                "tool_change",
                "wcs_define",
                "wcs_set_here",
                "frame_preset",
                "program_stop",
                "program_end",
                "raw",
            ],
            illegal: vec![
                ("arc_unknown_start", RT),          // SPEC §2 Q11
                ("arc_normal_non_canonical", INVAL), // нормаль не +ось (§6)
                ("machine_frame_arc", SEM),          // G53 требует G0/G1
                ("motion_feed_none", INVAL),         // канон-форма §3/§4
                ("raw_payload_not_hex", INVAL),      // grammar D9 v0.3
                ("program_end_missing", INVAL),      // структура (CORPUS n14)
                ("program_end_duplicate", INVAL),    // n13
                ("program_end_not_last", INVAL),     // n12
            ],
        }
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

    fn emit(&self, program: &Program) -> Result<String, Vec<Diagnostic>> {
        let out = emit::emit_program(program)?;
        *self.diagnostics.borrow_mut() = out.warnings;
        Ok(out.text)
    }
}
