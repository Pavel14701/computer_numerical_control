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
//! Этап 2: фреймы (G54–G59 → p=1..6; **G110–G112 → p=7..9** — нативная
//! симметрия EID-слотов #7–#9 = Wcs::G59_1..G59_3 с notice W_WCS_EXTENDED;
//! G154 P1–P3 → 7..9; G113–G129 → Raw SEM; **G52 → Err** — 2-й жёсткий
//! отказ; G53 machine-frame §5.3; G10 L2 P1–P6 → wcs_define, **L2 P0 = слот
//! G52 → Raw RT** (≠ Fanuc external offset), **L20 → Raw RT** (НЕ
//! wcs_set_here), L1/L10–L13 → Raw RT; G92 пресетка, S-кламп → Raw SEM;
//! absorb_frame_structs на raw-строках); циклы ×Haas-семантика (G81/G82/
//! G83/G73/G85/G86/G89 разворачивание §7.7 + клиренс 0.01in unit-scaled
//! v0.18; **L-повторы — НЕ K** (K = min-пек G83), G91-шаги, **G83: Q ИЛИ
//! I/J-K** (d1=I, dk=max(K, d_(k−1)−J), mutually exclusive) — полный отвод,
//! **G73 δ = канон 0.01in/0.254mm** + notice W_CYCLE_PARAM_ASSUMED
//! (документированный дефолт Setting 22 0.02"/0.5mm ≠ канон — COVERAGE
//! §6.19), G86 рестарт шпинделя + **P-dot-rule dotted=сек/bare=мс**
//! (G4/G82/G86/G89), дефолт G99, G98-known-position, G74/G76 (I/J-направ-
//! ление в тексте)/G77/G84/G156/G174/G184 → Raw SEM, L0 `?` → Raw,
//! G80 + группа 09-модальность); M-словарь (M0/M1 → program_stop; M6/M16+T
//! идиома `T01 M06`; одиночное T → Raw SEM; M07 Shower → Raw SEM — НЕ mist;
//! M10–M13 тормоза 4/5-й осей → Raw SEM; M29 output-relay → Raw SEM — ≠
//! Fanuc rigid-tap; M19 → Raw; M97/M98/M99/M46/M96 → Raw SYNTAX; M109 → Raw
//! RUNTIME); компенсации G43/G44/G143 → Raw RT + семейства Haas-состава
//! G40={G41,G42,G141}, G49={G43,G44,G143}; Macro B → line-level Raw
//! (система-переменные #500–#699/#109xxx ≠ Fanuc — в диагностике).
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
