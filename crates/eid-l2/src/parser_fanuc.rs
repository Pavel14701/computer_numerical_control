//! ЗАГОТОВКА (фаза 12.1, агент A) — парсер L2 для диалекта FANUC macro B.
//! Заполняет АГЕНТ B в фазе 12.1 (параллельная волна).
//!
//! ОЖИДАЕМЫЙ КОНТРАКТ (зафиксирован оркестратором, не менять сигнатуру):
//!
//! ```text
//! pub fn parse_program(src: &str) -> Result<L2Program, Vec<L2Error>>
//! ```
//!
//! * вход — исходный текст программы FANUC (кадры, `#`-параметры,
//!   `IF/WHILE/DO/END`, `G65/G66/G67`, O-программы);
//! * выход — [`L2Program`] (см. `crate::ast`): stmts + subs (O-номера),
//!   IF-GOTO — `target_label: u32` (N-метка, §2.4), вызовы —
//!   `SubTarget::ONumber`; DO/END-парность (do_id 1..3) проверяется
//!   здесь (E_INVALID_INPUT);
//! * каждый оператор несёт `src: SrcSpan` (§2.4 — реальный спан кадра);
//! * ошибки — вектор `L2Error` (см. `crate::eval`); `error_code()` задаёт
//!   класс SPEC §8;
//! * литеральный `#0` в адресном слове → `Expr::Null` (слово игнорируется,
//!   семантика #0 / таблица 2.4);
//! * числа — f64; triгонометрия в градусах (словарь FANUC).
//!
//! До заполнения агентом B функция возвращает Err (RuntimeError,
//! E_RUNTIME_STATE) — интерфейс компилируется, поведение — заглушка.

use crate::ast::L2Program;
use crate::eval::L2Error;

/// Разобрать программу FANUC в L2 AST (контракт — см. модульную шапку).
pub fn parse_program(_src: &str) -> Result<L2Program, Vec<L2Error>> {
    Err(vec![L2Error::RuntimeError(
        "parser_fanuc: stub of phase 12.1 — implementation expected from agent B".to_string(),
    )])
}
