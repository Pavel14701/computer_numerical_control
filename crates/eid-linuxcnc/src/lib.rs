//! eid-linuxcnc — канонизатор и эмиттер LinuxCNC (Агент D, IMPLEMENTATION-PLAN §2-D).
//! Интерфейс заморожен. quirks: G90.1/G91.1 ABSORB; O-code/M98/M99 → SYNTAX;
//! `#`-параметры → RUNTIME_STATE, if/while → SYNTAX (SPEC §1.1 v0.3);
//! G10 L2 vs L20 — разные узлы; G92 mid-program → отказ; G93 → разрешение
//! в G94-feed на уровне опа; G5.x — scope decision RAW; M62–M73 → SEMANTICS;
//! G76 — токарная резьба, отказ (§1.2).
#![allow(dead_code)]

use eid_core::adapter::Adapter;
use eid_core::ast::Program;
use eid_core::diag::Diagnostic;

pub struct LinuxCncAdapter;

impl Adapter for LinuxCncAdapter {
    fn dialect_name(&self) -> &'static str {
        "linuxcnc"
    }

    fn parse(&self, _src: &str, _file: &str) -> Result<Program, Vec<Diagnostic>> {
        todo!("Агент D, итерации 1–2: lexer + canonicalizer")
    }

    fn emit(&self, _program: &Program) -> Result<String, Vec<Diagnostic>> {
        todo!("Агент D, итерация 3: emitter v0")
    }
}
