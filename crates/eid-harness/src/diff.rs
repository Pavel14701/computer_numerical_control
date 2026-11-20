//! semantic_diff — замороженный интерфейс v0.1.
//! Допуски — EQUIVALENCE §4 (единственный источник коэффициентов; меняются
//! только ревизией контракта, не кодом и не профилем).

use eid_core::ast::Program;

#[derive(Debug, Clone)]
pub struct DiffConfig {
    pub eps_abs: f64,   // 0.001 — floor от 4dp-квантования ×10
    pub k_pos: f64,     // 1e-7 (относительный член, включается при L > 10 м)
    pub q_arc: f64,     // 1e-4 (шаг 4dp источника — член амплификации центра)
    pub a_cap: f64,     // 1000 — cap 1/|cos(sweep/2)| (ревью §4; cos, не sin)
    pub k_r: f64,       // 1e-6
    pub feed_rel: f64,  // 1e-5
    pub rpm_rel: f64,   // 1e-6
    pub dwell_rel: f64, // 1e-5
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            eps_abs: 0.001,
            k_pos: 1e-7,
            q_arc: 1e-4,
            a_cap: 1000.0,
            k_r: 1e-6,
            feed_rel: 1e-5,
            rpm_rel: 1e-6,
            dwell_rel: 1e-5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Divergence {
    OpCount { a: usize, b: usize },
    OpKind { index: usize },
    Geometry { index: usize, field: &'static str },
    StateField { index: usize, field: &'static str },
    MetaField { field: &'static str },
    /// unknown vs known — расхождение (EQUIVALENCE §3 v0.2); R3 красный.
    UnknownVsKnown { index: usize },
    RawMismatch { index: usize },
}

/// L2-дифф на composed-фрейме (EQUIVALENCE §1.3, §3). Пустой вектор = эквивалентны.
/// Вне эквивалентности: src/sub (provenance), аннотации, формат чисел, M1, M2/M30,
/// физические WCS origins (EQUIVALENCE §5).
pub fn semantic_diff(_a: &Program, _b: &Program, _cfg: &DiffConfig) -> Vec<Divergence> {
    todo!("Агент B, итерация 2")
}

pub fn asts_equal(a: &Program, b: &Program, cfg: &DiffConfig) -> bool {
    semantic_diff(a, b, cfg).is_empty()
}
