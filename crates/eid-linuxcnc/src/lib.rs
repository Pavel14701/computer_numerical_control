//! eid-linuxcnc — канонизатор и эмиттер LinuxCNC (Агент D, IMPLEMENTATION-PLAN §2-D).
//! Интерфейс заморожен. quirks: G90.1/G91.1 ABSORB; O-code/M98/M99 → SYNTAX;
//! `#`-параметры → RUNTIME_STATE, if/while → SYNTAX (SPEC §1.1 v0.3);
//! G10 L2 vs L20 — разные узлы; mid-program G92 → Raw-носитель узла
//! frame_preset + чейнинг-ребейз чтений (SPEC §5.3 v0.20, зонды 1–3/7/8);
//! G92.1–.3 → Raw{TABLE} (зонд 4); одиночное T → Raw{TABLE} (зонд 6);
//! хвост после M2/M30 → Raw + диалект-расхождение (SPEC §5.2 v0.20, зонд 5);
//! G93 → разрешение в G94-feed на уровне опа; G5.x — scope decision RAW;
//! M62–M73 → SEMANTICS; G76 — токарная резьба, отказ (§1.2).
//!
//! Итерация 2 (Агент D): parse реализован (lexer.rs → parser.rs →
//! canonicalize.rs). Итерация 3: emit v0 (emit.rs; COVERAGE §5, EQUIVALENCE
//! §1.4; warnings → take_diagnostics).
//!
//! Правило двух классов (CORPUS §1.2 v0.5): line-level RAW-конструкции дают
//! `Ok(ParseOutput)`; их диагностики возвращаются в owning-результате
//! (`ParseOutput.diagnostics`, v0.2 #22 — trait имеет Ok-канал diagnostics).

pub mod canonicalize;
pub mod emit;
pub mod lexer;
pub mod parser;

use crate::parser::Notice;
use eid_core::adapter::{Adapter, EmitOutput, ParseOutput};
use eid_core::ast::Program;
use eid_core::diag::Diagnostic;
use eid_core::verify::Notice as CoreNotice;

/// v0.2 #22: адаптер только-данные (без interior mutability) — Send + Sync;
/// notices/diagnostics возвращаются в owning-результате [`ParseOutput`] /
/// [`EmitOutput`].
#[derive(Debug, Default)]
pub struct LinuxCncAdapter;

impl LinuxCncAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Таблица легальности эмиссии LinuxCNC (COVERAGE §5, LinuxCNC-колонка;
    /// ConversionTarget-паттерн, зеркально eid-grbl). Словарь полный:
    /// все 10 видов опов легальны; path_mode/WCS вне словаря невозможны
    /// (G54–G59.3, G61/G61.1/G64 — весь словарь целевого).
    pub fn emission_legality(&self) -> emit::EmissionLegality {
        use eid_core::diag::ErrorCode::{
            InvalidInput as INVAL, RuntimeState as RT, UnsupportedSemantics as SEM,
        };
        use emit::LinuxCncEmissionCondition as C;
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
            // v0.2 волна 4 (§2.5 В1): ключи — типизированный enum; код-зеркало
            // обязано совпадать с `cond.code()` (тест legality_table_complete).
            illegal: vec![
                (C::ArcUnknownStart, RT),          // SPEC §2 Q11
                (C::ArcNormalNonCanonical, INVAL), // нормаль не +ось (§6)
                (C::MachineFrameArc, SEM),         // G53 требует G0/G1
                (C::MotionFeedNone, INVAL),        // канон-форма §3/§4
                (C::RawPayloadNotHex, INVAL),      // grammar D9 v0.3
                (C::ProgramEndMissing, INVAL),     // структура (CORPUS n14)
                (C::ProgramEndDuplicate, INVAL),   // n13
                (C::ProgramEndNotLast, INVAL),     // n12
            ],
        }
    }
}

/// Диалектный Notice → core-Notice: kind уже единый core-тип (v0.2 #22) —
/// копирование скелета {kind, message, op_index, op}.
impl From<Notice> for CoreNotice {
    fn from(n: Notice) -> Self {
        CoreNotice {
            kind: n.kind,
            message: n.message,
            op_index: n.op_index,
            op: n.op,
        }
    }
}

// T5 (INTERFACE-V0.2-DESIGN §5): compile-time assert Send + Sync —
// interior mutability у адаптера запрещён контрактом trait v0.2.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<LinuxCncAdapter>();
};

impl Adapter for LinuxCncAdapter {
    fn dialect_name(&self) -> &'static str {
        "linuxcnc"
    }

    fn parse(&self, src: &str, file: &str) -> Result<ParseOutput, Vec<Diagnostic>> {
        let out = parser::parse_program(src, file)?;
        Ok(ParseOutput {
            program: out.program,
            notices: out.notices.into_iter().map(Into::into).collect(),
            diagnostics: out.diagnostics,
        })
    }

    fn emit(&self, program: &Program) -> Result<EmitOutput, Vec<Diagnostic>> {
        let out = emit::emit_program(program)?;
        Ok(EmitOutput {
            text: out.text,
            warnings: Vec::new(),
            diagnostics: out.warnings,
        })
    }
}
