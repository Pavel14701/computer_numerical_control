//! eid-fanuc — канонизатор Fanuc mill (0i-D/F, 16i/18i/21i, 30i/31i/32i,
//! статическое CAM-подмножество) → EID/L1 (Агент C, этапы 1–2).
//! Контракт: fanuc_dialect_analysis.md v0.0 (commit 7cd6d74), SPEC-EID-L1
//! v0.16, COVERAGE v0.8 Fanuc-колонка (§2/§3/§5.1) + rulings оркестратора
//! 1–8 (этап 2).
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
//! параметр 1422). Этап 3: эмиссия (анализ §8; отдельный бриф).
#![allow(dead_code)]

use std::cell::RefCell;
use eid_core::adapter::Adapter;
use eid_core::ast::Program;
use eid_core::diag::{Diagnostic, ErrorCode};

pub mod block;
pub mod canonicalize;
pub mod lexer;
pub mod state;

/// Адаптер Fanuc mill (dialect_name = "fanuc-mill").
/// drop-with-notice (zero-length / DPI-assumed) — не ошибка: notices
/// буферизуются, забрать через [`FanucAdapter::take_notices`]
/// (паттерн GrblAdapter, eid-grbl lib.rs).
#[derive(Debug, Default)]
pub struct FanucAdapter {
    notices: RefCell<Vec<canonicalize::Notice>>,
}

impl FanucAdapter {
    pub fn new() -> Self {
        Self { notices: RefCell::new(Vec::new()) }
    }

    /// Notices последнего `parse` (DPI-assumed, отброшенные zero-length,
    /// auto-PEND); опустошает буфер.
    pub fn take_notices(&self) -> Vec<canonicalize::Notice> {
        std::mem::take(&mut *self.notices.borrow_mut())
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

    fn emit(&self, _program: &Program) -> Result<String, Vec<Diagnostic>> {
        // Эмиссия EID → Fanuc — этап 3 (анализ §8). В интерфейсе Adapter
        // обязана существовать; честный отказ до реализации.
        Err(vec![Diagnostic {
            code: ErrorCode::UnsupportedSyntax,
            message: "Fanuc emission not implemented yet (stage 3; analysis §8)".into(),
            src: None,
            options: Vec::new(),
            limit: None,
            stage: eid_core::diag::Stage::Emit,
        }])
    }
}
