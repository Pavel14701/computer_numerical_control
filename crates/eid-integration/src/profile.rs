//! MachineProfile — замороженный интерфейс v0.1 (минимум для MVP; полная схема —
//! этап 8 исходной карты: кинематика, макросы, safety, escape hatch).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialectKind {
    Grbl11,
    GrblHal,
    FluidNc,
    LinuxCnc,
}

/// Пороги E_LIMIT_EXCEEDED — дефолты COVERAGE §1.2; профиль переопределяет.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_ops_per_program: usize,
    pub max_unroll_iterations: usize,
    pub max_source_lines: usize,
    pub max_word_chars: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_ops_per_program: 200_000,
            max_unroll_iterations: 10_000,
            max_source_lines: 1_000_000,
            max_word_chars: 32,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MachineProfile {
    pub name: String,
    pub dialect: DialectKind,
    pub limits: Limits,
    /// R-эмиссия дуг — по флагу профиля с notice (EQUIVALENCE §4: по умолчанию
    /// всегда IJK; R1 всегда через IJK).
    pub emit_r_arcs: bool,
}
