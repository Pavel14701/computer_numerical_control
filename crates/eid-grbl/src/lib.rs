//! eid-grbl — канонизатор и эмиттер GRBL 1.1 (Агент C, IMPLEMENTATION-PLAN §2-C).
//! Интерфейс заморожен. quirks — ИСПРАВЛЕННЫЙ список (COVERAGE §5/§6.11):
//! I/J/K literal X/Y/Z, эмиссия in-plane пар (G17: I+J, G18: I+K, G19: J+K);
//! tool_change → E_UNSUPPORTED_SYNTAX (при эмиссии; на parse bare-T → ABSORB
//! selected_tool, SPEC §5.2 v0.20); G10 L20 → узел wcs_set_here (в словаре GRBL);
//! G61.1/path_mode → warning (Q3), не отказ; G43 H → E_REQUIRES_MACHINE_TABLE.
//! SPEC v0.20 (зонд-сверка target/probe/g92): bare-T → ABSORB; G92-mid →
//! chaining + Raw-носитель (грамматика v0.3 не имеет mid-preset опа);
//! G92.1 → Raw{E_REQUIRES_MACHINE_TABLE}; блоки после M2/M30 → Raw-хвост
//! + notice диалект-расхождения (grbl исполняет — protocol.c:74-105).
#![allow(dead_code)]

use eid_core::adapter::{Adapter, EmitOutput, ParseOutput};
use eid_core::ast::Program;
use eid_core::diag::Diagnostic;
use eid_core::verify::{Notice, NoticeKind};

pub mod block;
pub mod canonicalize;
pub mod emit;
pub mod gcode;
pub mod lexer;

/// Адаптер GRBL 1.1 (dialect_name = "grbl-1.1").
/// drop-with-notice (zero-length / near-degenerate R-arc) — не ошибка:
/// notices возвращаются в owning-результате [`ParseOutput`] (v0.2 #22 —
/// без interior mutability; адаптер только-данные, Send + Sync).
#[derive(Debug, Default)]
pub struct GrblAdapter;

impl GrblAdapter {
    pub fn new() -> Self {
        Self
    }

    /// ConversionTarget-таблица легальности эмиссии в GRBL 1.1 (COVERAGE §5).
    /// legal — виды опов, представимые ходами; illegal — (условие, код класса).
    /// `emit` валидирует программу ПРОТИВ этой таблицы — коды отказа
    /// берутся только отсюда (единый путь отказа).
    /// E_RAW_UNTRANSLATABLE намеренно НЕ в таблице: это translate-путь
    /// (Агент E, SPEC §8.2), не same-dialect эмиссия.
    pub fn emission_legality(&self) -> emit::EmissionLegality {
        use eid_core::diag::ErrorCode::{InvalidInput as INVAL, UnsupportedSyntax as SYN};
        use emit::GrblEmissionCondition as C;
        emit::EmissionLegality {
            legal: vec![
                "rapid",
                "linear",
                "arc",
                "dwell",
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
                (C::ToolChange, SYN),              // M6 вне словаря GRBL (#27)
                (C::WcsOutsideG54G59, SYN),        // G59.x вне словаря GRBL
                (C::MotionFeedNone, INVAL),        // канон-форма §3/§4
                (C::ArcUnknownStart, INVAL),       // IJK требует числового старта
                (C::ArcNormalNonCanonical, INVAL), // нормаль не +ось (§6)
                (C::RawPayloadNotHex, INVAL),      // grammar D9 v0.3
                (C::ProgramEndMissing, INVAL),     // структура (CORPUS n14)
                (C::ProgramEndDuplicate, INVAL),   // n13
                (C::ProgramEndNotLast, INVAL), // n12: НЕ-Raw оп после PEND (SPEC §5.2 v0.20 — Raw-хвост легален)
            ],
            warnings: vec![
                // Q3 (COVERAGE §6.1/§5): PATH_MODE не выражается в GRBL-шапке.
                (
                    "path_mode_exact_path",
                    "junction blending behaviour lost on GRBL (COVERAGE §6.1)",
                ),
                (
                    "path_mode_exact_stop",
                    "exact stop not representable on GRBL (no G61.1; COVERAGE §5)",
                ),
                (
                    "path_mode_blend",
                    "target G64 P- tolerance not representable",
                ),
            ],
        }
    }
}

/// Диалектный Notice → единый core-Notice (NoticeKind-супермножество v0.2;
/// INTERFACE-V0.2-DESIGN §2.2 В2 — конверсия на границе адаптера).
impl From<canonicalize::Notice> for Notice {
    fn from(n: canonicalize::Notice) -> Self {
        Notice {
            kind: match n.kind {
                canonicalize::NoticeKind::ZeroLengthMotion => NoticeKind::ZeroLengthMotion,
                canonicalize::NoticeKind::ZeroLengthArc => NoticeKind::ZeroLengthArc,
                canonicalize::NoticeKind::ZeroDwell => NoticeKind::ZeroDwell,
                canonicalize::NoticeKind::NearDegenerateRArc => NoticeKind::NearDegenerateRArc,
                canonicalize::NoticeKind::MissingProgramEnd => NoticeKind::MissingProgramEnd,
                canonicalize::NoticeKind::ModalOnRaw => NoticeKind::ModalOnRaw,
                // SPEC §5.2 v0.20: core NoticeKind-супермножество v0.2 заморожен
                // (9 вариантов, eid-core вне зоны агента) — диалект-расхождение
                // хвоста после PEND маппится в ближайшую семью «поведенческая
                // заметка при Raw-опах»; РЕАЛЬНАЯ семантика — в message
                // («executed in grbl-stream semantics, ignored in rs274-file»).
                canonicalize::NoticeKind::DialectDivergence => NoticeKind::ModalOnRaw,
            },
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
    assert_send_sync::<GrblAdapter>();
};

impl Adapter for GrblAdapter {
    fn dialect_name(&self) -> &'static str {
        "grbl-1.1"
    }

    fn parse(&self, src: &str, file: &str) -> Result<ParseOutput, Vec<Diagnostic>> {
        let out = canonicalize::canonicalize_with_notices(src, file)?;
        Ok(ParseOutput {
            program: out.program,
            notices: out.notices.into_iter().map(Into::into).collect(),
            diagnostics: Vec::new(),
        })
    }

    fn emit(&self, program: &Program) -> Result<EmitOutput, Vec<Diagnostic>> {
        let mut warnings = Vec::new();
        let text = emit::emit_with_warnings(program, &mut warnings)?;
        Ok(EmitOutput {
            text,
            warnings,
            diagnostics: Vec::new(),
        })
    }
}
