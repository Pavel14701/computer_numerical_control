//! eid-fanuc — канонизатор Fanuc mill (0i-D/F, 16i/18i/21i, 30i/31i/32i,
//! статическое CAM-подмножество) → EID/L1 (Агент C, этап 1).
//! Контракт: fanuc_dialect_analysis.md v0.0 (commit 7cd6d74), SPEC-EID-L1 v0.16.
//! Этап 1: скелет + лексер (EOB `;` — НЕ комментарий, §1.3 ловушка №2) +
//! модальные группы 01–06 + движение G0–G3 (§9.1(1)–(3)).
//! Этап 2 (по брифу): фреймы G54–G59/G10 L2/G92/G53/G52-ERR, циклы §5,
//! M-коды, Macro B → line-level Raw. Этап 3: эмиссия.
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
