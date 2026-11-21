//! eid-fanuc — канонизатор и эмиттер Fanuc mill (0i-D/F, 16i/18i/21i,
//! 30i/31i/32i, статическое CAM-подмножество) ↔ EID/L1 (Агент C, этапы 1–3).
//! Контракт: fanuc_dialect_analysis.md v0.0.1 §8 (reverse gap), SPEC-EID-L1
//! v0.17, COVERAGE v0.9 Fanuc-колонка (§2/§3/§5.1) + rulings оркестратора
//! 1–8 (этапы 2–3).
//! Этап 1: скелет + лексер (EOB `;` — НЕ комментарий, §1.3 ловушка №2) +
//! модальные группы 01–06 + движение G0–G3 (§9.1(1)–(3)).
//! Этап 2: фреймы (G54–G59/G10 L2/G92/G53 machine-frame/G52-ERR; G10 L20
//! Fanuc-семантика — ловушка №3), циклы §5 по канон-шаблону §7.7 SPEC
//! (P dwell = целые мс; δ = 0.5mm + W_CYCLE_PARAM_ASSUMED; K/L-повторы;
//! K0-in-G91 → Raw; G98/G99; модальность группы 09), M-словарь (стопы/
//! шпиндель/СОЖ/инструмент; T-предвыбор → Raw; M19/M29/M98/99/198 → Raw;
//! generic-ведро), Macro B → line-level Raw (`#`/`[ ]`/IF-GOTO/WHILE/G65/
//! M98/O-структуры; byte-exact payload фрагментов, ruling 5), компенсации
//! (G43/G44 → Raw{TABLE}; семейства G40/G49/G11 COVERAGE §1.3), G93 ABSORB
//! (ruling 4), DPI-F фикс (ruling 2: bare F = целое mm/min verbatim,
//! параметр 1422). Этап 3: эмиссия EID → Fanuc (emit.rs, COVERAGE §5.1
//! v0.8 / анализ §8): FanucEmissionLegality (ConversionTarget-паттерн),
//! G0–G3 всегда IJK (R — профиль-флаг этапа 8), полный круг БЕЗ end-слов,
//! decimal point ВСЕГДА (DPI-ловушка), dwell P-целые-мс / X-sub-ms,
//! G54–G59 (G59.x → SYNTAX-отказ; wcs_set_here → SEMANTICS-отказ),
//! path_mode G61/G64 (+warnings), program_end M30 (профиль M02 — этап 8),
//! T## M06; модальная абсорбция на RAW-строках (SPEC §8.6 v0.17) и
//! explicit-end полный круг (SPEC §3 v0.17) — зеркала аудита.
#![allow(dead_code)]

use std::cell::RefCell;
use eid_core::adapter::Adapter;
use eid_core::ast::Program;
use eid_core::diag::Diagnostic;

pub mod block;
pub mod canonicalize;
pub mod emit;
pub mod lexer;
pub mod state;

/// Адаптер Fanuc mill (dialect_name = "fanuc-mill").
/// drop-with-notice (zero-length / DPI-assumed) — не ошибка: notices
/// буферизуются, забрать через [`FanucAdapter::take_notices`]
/// (паттерн GrblAdapter, eid-grbl lib.rs). Warnings эмиссии — через
/// [`FanucAdapter::take_warnings`] (паттерн GrblAdapter::take_warnings).
#[derive(Debug, Default)]
pub struct FanucAdapter {
    notices: RefCell<Vec<canonicalize::Notice>>,
    warnings: RefCell<Vec<String>>,
}

impl FanucAdapter {
    pub fn new() -> Self {
        Self { notices: RefCell::new(Vec::new()), warnings: RefCell::new(Vec::new()) }
    }

    /// ConversionTarget-таблица легальности эмиссии в Fanuc mill
    /// (COVERAGE §5.1). legal — виды опов, представимые ходами; illegal —
    /// (условие, код класса). `emit` валидирует программу ПРОТИВ этой
    /// таблицы — коды отказа берутся только отсюда (единый путь отказа).
    /// E_RAW_UNTRANSLATABLE намеренно НЕ в таблице: это translate-путь
    /// (SPEC §8.2), не same-dialect эмиссия.
    pub fn emission_legality(&self) -> emit::FanucEmissionLegality {
        use eid_core::diag::ErrorCode::{
            InvalidInput as INVAL, UnsupportedSemantics as USEM, UnsupportedSyntax as SYN,
        };
        emit::FanucEmissionLegality {
            // 9 из 10 видов опов (wcs_set_here запрещён ловушкой №3);
            // frame_preset — мета программы (G92, EQUIVALENCE §1.4).
            legal: vec![
                "rapid",
                "linear",
                "arc",
                "dwell",
                "tool_change",
                "wcs_define",
                "program_stop",
                "program_end",
                "raw",
                "frame_preset",
            ],
            illegal: vec![
                // COVERAGE §5.1: wcs_set_here эмиссия запрещена как L20
                // (Fanuc L20 = запись origin G54.1 Pn) — честный отказ.
                ("wcs_set_here", USEM),
                // G59.1/.2/.3 — кодов нет (правило §5: вне словаря → SYNTAX);
                // профиль «map → G54.1 Pn» с notice — этап 8, НЕ дефолт.
                ("wcs_outside_g54_g59", SYN),
                ("wcs_define_p_outside_g54_g59", SYN),
                // G53 ходы прямые (SPEC §5.3) — дуга не представима.
                ("machine_frame_arc", USEM),
                ("motion_feed_none", INVAL),         // канон-форма §3/§4
                ("arc_unknown_start", INVAL),        // IJK требует числового старта
                ("arc_normal_non_canonical", INVAL), // нормаль не +ось (§6)
                ("raw_payload_not_hex", INVAL),      // grammar D9 v0.3
                ("program_end_missing", INVAL),      // структура (SPEC §3)
                ("program_end_duplicate", INVAL),
                ("program_end_not_last", INVAL),
            ],
            warnings: vec![
                // COVERAGE §5.1: exact_path → ближайший G61 консервативно
                // (дефолт); профиль bare G64 — этап 8.
                (
                    "path_mode_exact_path",
                    "Fanuc exact_path эквивалента нет — эмитится G61 (консервативный дефолт: G61 = exact stop, value shift COVERAGE §2.2; профиль bare G64 — этап 8)",
                ),
                // COVERAGE §5.1: blend(t) → bare G64, t теряется.
                (
                    "path_mode_blend_t",
                    "path_mode blend t теряется — bare G64 (P-квантификатора в словаре Fanuc нет; COVERAGE §5.1)",
                ),
            ],
        }
    }

    /// Notices последнего `parse` (DPI-assumed, отброшенные zero-length,
    /// auto-PEND); опустошает буфер.
    pub fn take_notices(&self) -> Vec<canonicalize::Notice> {
        std::mem::take(&mut *self.notices.borrow_mut())
    }

    /// Warnings последнего `emit` (потери path_mode/снапшотов); опустошает
    /// буфер.
    pub fn take_warnings(&self) -> Vec<String> {
        std::mem::take(&mut *self.warnings.borrow_mut())
    }
}

impl Adapter for FanucAdapter {
    fn dialect_name(&self) -> &'static str {
        "fanuc-mill"
    }

    fn parse(&self, src: &str, file: &str) -> Result<Program, Vec<Diagnostic>> {
        let out = canonicalize::canonicalize_with_notices(src, file)?;
        *self.notices.borrow_mut() = out.notices;
        Ok(out.program)
    }

    fn emit(&self, program: &Program) -> Result<String, Vec<Diagnostic>> {
        let mut warnings = Vec::new();
        let result = emit::emit_with_warnings(program, &mut warnings);
        // Warnings — в буфер адаптера (забрать take_warnings); emit_with_
        // warnings опустошает переданный Vec — берём из EmitOut.
        if let Ok(out) = &result {
            *self.warnings.borrow_mut() = out.warnings.clone();
        }
        result.map(|out| out.text)
    }
}
