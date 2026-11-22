//! ЗАГОТОВКА (фаза 12.1, агент A) — парсер L2 для диалекта Haas.
//! Заполняет АГЕНТ C в фазе 12.1 (параллельная волна).
//!
//! ОЖИДАЕМЫЙ КОНТРАКТ (зафиксирован оркестратором, не менять сигнатуру):
//!
//! ```text
//! pub fn parse_program(src: &str) -> Result<L2Program, Vec<L2Error>>
//! ```
//!
//! * вход — исходный текст программы Haas (Q-параметры, G65-подобные вызовы,
//!   M-коды Haas, `IF/WHILE/DO/END` в стиле macro B);
//! * выход — [`L2Program`] (см. `crate::ast`): stmts + subs, IF-GOTO —
//!   `target_label: u32` (N-метка, §2.4); каждый оператор несёт
//!   `src: SrcSpan` (§2.4);
//! * ошибки — вектор `L2Error` (см. `crate::eval`); классы — SPEC §8;
//! * литеральный `#0` в адресном слове → `Expr::Null` (слово игнорируется);
//! * отличия от FANUC (см. haas_dialect_analysis.md в корне workspace):
//!   свои коды подProgram (M97/M98/M99), номера O-программ, Q-слово.
//!
//! До заполнения агентом C функция возвращает Err (RuntimeError,
//! E_RUNTIME_STATE) — интерфейс компилируется, поведение — заглушка.

use crate::ast::L2Program;
use crate::eval::L2Error;

/// Разобрать программу Haas в L2 AST (контракт — см. модульную шапку).
pub fn parse_program(_src: &str) -> Result<L2Program, Vec<L2Error>> {
    Err(vec![L2Error::RuntimeError(
        "parser_haas: stub of phase 12.1 — implementation expected from agent C".to_string(),
    )])
}
