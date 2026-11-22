//! ЗАГОТОВКА (фаза 12.1, агент A) — парсер L2 для диалекта LinuxCNC.
//! Заполняет АГЕНТ D в фазе 12.1 (параллельная волна).
//!
//! ОЖИДАЕМЫЙ КОНТРАКТ (зафиксирован оркестратором, не менять сигнатуру):
//!
//! ```text
//! pub fn parse_program(src: &str) -> Result<L2Program, Vec<L2Error>>
//! ```
//!
//! * вход — исходный текст программы LinuxCNC (`#<_named>` именованные
//!   параметры, O-коды sub/endsub/while/while/if/if-else/do, G5.x-сплайны);
//! * выход — [`L2Program`] (см. `crate::ast`): stmts + subs (O-номера),
//!   именованные параметры → `VarRef::Named`, системные `#<_...>` →
//!   `VarRef::Sys` (§2.2); каждый оператор несёт `src: SrcSpan` (§2.4);
//! * ошибки — вектор `L2Error` (см. `crate::eval`); классы — SPEC §8;
//! * литеральный `#0` в адресном слове → `Expr::Null` (слово игнорируется).
//!
//! До заполнения агентом D функция возвращает Err (RuntimeError,
//! E_RUNTIME_STATE) — интерфейс компилируется, поведение — заглушка.

use crate::ast::L2Program;
use crate::eval::L2Error;

/// Разобрать программу LinuxCNC в L2 AST (контракт — см. модульную шапку).
pub fn parse_program(_src: &str) -> Result<L2Program, Vec<L2Error>> {
    Err(vec![L2Error::RuntimeError(
        "parser_linuxcnc: stub of phase 12.1 — implementation expected from agent D".to_string(),
    )])
}
