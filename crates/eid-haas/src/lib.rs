//! eid-haas — канонизатор и эмиттер Haas mill NGC (VF/ST-вертикальные,
//! mini-mill, DT/DM; статическое CAM-подмножество) ↔ EID/L1 (Агент C, этапы
//! 1–3). Контракт: haas_dialect_analysis.md v0.0 (§1 профиль, §2 G-матрица,
//! §9.1 v0.1-скоуп, §10 реестр 34 `?`), SPEC-EID-L1 v0.18, COVERAGE v0.9.2
//! Haas-колонка. Зеркало архитектуры eid-fanuc.
//! Этап 1: скелет + лексер (EOB `;` — НЕ комментарий, §1.3 ловушка №2;
//! пробел-в-слове — строгий отказ `?`) + модальные группы 01–06 + 15
//! (G61/G64 value-shift) + движение G0–G3 (§9.1(1)–(3)): IJK-инкремент
//! (группы 04 в словаре НЕТ — G90.1/G91.1 ABSORB `?`), R-форма, helical,
//! полный круг 360° по геометрии (SPEC §3 v0.18), §8.6 модальная абсорбция
//! на RAW-строках, auto-PEND, DPI: X/Y/Z/A/B/C/I/J/K/R/Q bare → × least
//! increment; **F bare → × least increment + notice Setting 77** (домен F у
//! Haas ≠ Fanuc-ruling: F12 = 0.0012 units/min при Setting 77 OFF),
//! G70/G71/G72 → Raw-болтовые (НЕ units-псевдонимы), G28/G29/G31/G12/G13 →
//! raw-таблица.
#![allow(dead_code)]

pub mod block;
pub mod canonicalize;
pub mod lexer;
pub mod state;

use eid_core::adapter::Adapter;
use eid_core::ast::Program;
use eid_core::diag::{Diagnostic, ErrorCode};
use std::cell::RefCell;

/// Адаптер Haas mill NGC (dialect_name = "haas-mill").
/// drop-with-notice (zero-length / DPI-assumed) — не ошибка: notices
/// буферизуются, забрать через [`HaasAdapter::take_notices`]
/// (паттерн GrblAdapter/FanucAdapter). Этап 1 — parse-сторона; эмиссия
/// EID → Haas NGC (dwell P-dot, полный круг без end-слов, decimal point
/// обязателен, p7..9 → G110–G112, M30) — этап 3.
#[derive(Debug, Default)]
pub struct HaasAdapter {
    notices: RefCell<Vec<canonicalize::Notice>>,
}

impl HaasAdapter {
    pub fn new() -> Self {
        Self {
            notices: RefCell::new(Vec::new()),
        }
    }

    /// Notices последнего `parse` (DPI-assumed, отброшенные zero-length,
    /// auto-PEND); опустошает буфер.
    pub fn take_notices(&self) -> Vec<canonicalize::Notice> {
        std::mem::take(&mut *self.notices.borrow_mut())
    }
}

impl Adapter for HaasAdapter {
    fn dialect_name(&self) -> &'static str {
        "haas-mill"
    }

    fn parse(&self, src: &str, file: &str) -> Result<Program, Vec<Diagnostic>> {
        let out = canonicalize::canonicalize_with_notices(src, file)?;
        *self.notices.borrow_mut() = out.notices;
        Ok(out.program)
    }

    fn emit(&self, _program: &Program) -> Result<String, Vec<Diagnostic>> {
        // Этап 3 (бриф: этап 1 из 3 — скелет/лексер/модальности/движение).
        Err(vec![Diagnostic {
            code: ErrorCode::UnsupportedSyntax,
            message: "Haas emission (EID → Haas NGC) — stage-3 scope, not implemented in stage 1"
                .into(),
            src: None,
            options: Vec::new(),
            limit: None,
            stage: eid_core::diag::Stage::Verify,
        }])
    }
}
