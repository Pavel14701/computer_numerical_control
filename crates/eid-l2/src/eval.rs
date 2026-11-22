//! EID/L2 — семантика #0 (этап 12.1): эталонное вычисление `Val`,
//! окружение переменных (`L2Env`), ошибки (`L2Error`) и `eval_expr`.
//!
//! СПЕЦ-СТАТУС: сверено с `EID-L2-DESIGN.md` v0.1 (коммит d3fd57e) —
//! Приложение «полная спецификация семантики #0 в Fanuc Custom Macro B /
//! Haas NGC», таблицы 2.1–2.4, и эталонная реализация `Val` (§4
//! Приложения: `as_num/macro_eq/macro_ne/macro_ge`). Каждая строка
//! правил покрыта ед-тестом ниже (чек-лист T3).
//!
//! Сводка семантики #0 (таблицы Приложения):
//!  * 2.1 присваивание/адресные слова: `#100 = #0` копирует Val::Null
//!    (НЕ 0.0); любая арифметика снимает Null (`#0 + 0` → Num(0.0));
//!    слово со значением Null ИГНОРИРУЕТСЯ (`resolve_word → None`);
//!    Null-аргумент G65 пробрасывается как Null.
//!  * 2.2 арифметика: операнды через `as_num` (Null → 0.0); результат
//!    ВСЕГДА `Val::Num` (`-Null` канонизируется в Num(0.0), не -0.0).
//!    Деление на Null (=0.0) и на ноль — ошибка `DivisionByZero` →
//!    `E_INVALID_INPUT`; `LN[#0]` → `LnOfNull` → `E_INVALID_INPUT`.
//!  * 2.3 сравнения: EQ/NE — Null равен ТОЛЬКО Null (Null ≠ 0!;
//!    числа — с допуском 1e-9, эталон Приложения); GT/LT/GE/LE — Null
//!    приводится к 0.0 (`#0 GE 0` → true). Результат — bool для
//!    IF/WHILE или Num(1.0)/Num(0.0) в выражениях (таблица 2.3).
//!  * 2.4 логика: AND/OR/XOR — побитово по i64 (Null → 0, результат Num).
//!  * условия IF/WHILE: cond через `as_num` ≠ 0 → true; `IF [#0]` —
//!    Null в cond как 0.0 → false.
//!
//! Окружение: локальные #1–#33 — стек кадров (CallSub push/pop),
//! глобальные #100–#599 — BTreeMap, именованные — BTreeMap. #0 — всегда
//! Null, запись запрещена. Неинициализированные (в поддерживаемых
//! диапазонах) читаются как Null.

use crate::ast::{BinaryOp, Expr, SysVar, UnaryOp, Val, VarRef};
use eid_core::diag::ErrorCode;
use std::collections::BTreeMap;

// ===========================================================================
// Эталон семантики #0 — методы Val (таблицы 2.1–2.4)
// ===========================================================================

impl Val {
    /// Null?
    pub fn is_null(self) -> bool {
        matches!(self, Self::Null)
    }

    /// Эталон Приложения §4: приведение к числу для арифметики и
    /// неравенств (Null → 0.0).
    #[inline]
    pub fn as_num(self) -> f64 {
        match self {
            Self::Null => 0.0,
            Self::Num(v) => v,
        }
    }

    /// Условие IF/WHILE: `as_num() != 0.0` ⇒ `IF [#0]` — false.
    pub fn truthy(self) -> bool {
        self.as_num() != 0.0
    }

    /// Эталон Приложения §4 / таблица 2.3: семантика Fanuc/Haas EQ —
    /// Null == Null (true), Null == 0.0 (false); числа — с допуском
    /// сравнения Macro B 1e-9.
    #[inline]
    pub fn macro_eq(self, other: Val) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Null, Self::Num(_)) | (Self::Num(_), Self::Null) => false,
            (Self::Num(a), Self::Num(b)) => (a - b).abs() <= 1e-9, // допуск Macro B
        }
    }

    /// Эталон Приложения §4: NE = !EQ.
    #[inline]
    pub fn macro_ne(self, other: Val) -> bool {
        !self.macro_eq(other)
    }

    /// Таблица 2.3: GT — Null → 0.0 с обеих сторон.
    #[inline]
    pub fn macro_gt(self, other: Val) -> bool {
        self.as_num() > other.as_num()
    }

    /// Таблица 2.3: LT.
    #[inline]
    pub fn macro_lt(self, other: Val) -> bool {
        self.as_num() < other.as_num()
    }

    /// Эталон Приложения §4: GE — Null → 0.0 (`#0 GE 0` → true).
    #[inline]
    pub fn macro_ge(self, other: Val) -> bool {
        self.as_num() >= other.as_num()
    }

    /// Таблица 2.3: LE (`#0 LE 0` → true).
    #[inline]
    pub fn macro_le(self, other: Val) -> bool {
        self.as_num() <= other.as_num()
    }

    /// Таблица 2.1 (адресные слова): слово со значением Null → слово
    /// игнорируется (None); числовое → Some(v). Критично для lowering (§3):
    /// `pose_resolve` строит на этом правило «Null-компонента → None».
    pub fn resolve_word(self) -> Option<f64> {
        match self {
            Self::Null => None,
            Self::Num(v) => Some(v),
        }
    }
}

/// Булев результат сравнения как значение выражения (таблица 2.3:
/// «или число 1.0 / 0.0, если используются в выражениях»).
fn bool_val(b: bool) -> Val {
    Val::Num(if b { 1.0 } else { 0.0 })
}

// ===========================================================================
// Ошибки вычисления (маппинг в классы ErrorCode — SPEC §8 / COVERAGE §1.1)
// ===========================================================================

/// Ошибка семантики/исполнения L2.
#[derive(Debug, Clone, PartialEq)]
pub enum L2Error {
    /// Деление/остаток на ноль (включая Null-делитель: Null → 0.0)
    /// → класс `E_INVALID_INPUT` (Приложение, таблица 2.2).
    DivisionByZero,
    /// `LN[#0]` (и LN любого нуля) → класс `E_INVALID_INPUT`
    /// (Приложение, таблица 2.2).
    LnOfNull,
    /// Исчерпан бюджет вычислений (бесконечный WHILE и т.п.)
    /// → класс `E_LIMIT_EXCEEDED` (чек-лист T5).
    MaxEvalSteps { limit: u64 },
    /// Прочее рантайм-отклонение (SQRT из отрицательного, неизвестная
    /// подпрограмма, запись в #0, системная переменная при неизвестном
    /// состоянии станка) → класс `E_RUNTIME_STATE`.
    RuntimeError(String),
}

impl L2Error {
    /// Класс диагностики SPEC §8 для хвостовых диагностик/остановов.
    pub fn error_code(&self) -> ErrorCode {
        match self {
            // Таблица 2.2: деление на ноль/Null и LN[#0] → E_INVALID_INPUT.
            Self::DivisionByZero | Self::LnOfNull => ErrorCode::InvalidInput,
            // T5: бюджет вычислений → E_LIMIT_EXCEEDED.
            Self::MaxEvalSteps { .. } => ErrorCode::LimitExceeded,
            // Прочее рантайм-отклонение, в т.ч. G31/чтение SkipPos/
            // AbsMachinePos при неизвестном — останов E_RUNTIME_STATE
            // (§3, последняя строка матрицы).
            Self::RuntimeError(_) => ErrorCode::RuntimeState,
        }
    }

    /// Человекочитаемое сообщение (для Diagnostic.message).
    pub fn message(&self) -> String {
        match self {
            Self::DivisionByZero => "division by zero (Null divisor counts as 0.0)".to_string(),
            Self::LnOfNull => "LN[0] is undefined".to_string(),
            Self::MaxEvalSteps { limit } => {
                format!("evaluation step budget exhausted (limit {limit})")
            }
            Self::RuntimeError(m) => m.clone(),
        }
    }
}

// ===========================================================================
// Окружение переменных
// ===========================================================================

/// Число локальных слотов кадра: #1–#33.
const N_LOCALS: usize = 33;
/// Диапазон глобальных переменных: #100–#599.
const GLOBAL_MIN: u32 = 100;
const GLOBAL_MAX: u32 = 599;

/// Окружение L2 (§ семантика #0 / задание п.3):
/// локальные #1–#33 — СТЕК кадров (CallSub), глобальные #100–#599 и
/// именованные — BTreeMap. Бюджет шагов (`tick`) — общий для eval_expr и
/// циклов интерпретатора (T5: бесконечный WHILE → `MaxEvalSteps`).
#[derive(Debug, Clone, PartialEq)]
pub struct L2Env {
    /// Стек кадров локальных #1–#33; корневой кадр — главный блок программы.
    locals: Vec<[Val; N_LOCALS]>,
    /// Глобальные #100–#599; отсутствующие читаются как Null.
    globals: BTreeMap<u32, Val>,
    /// Именованные переменные; отсутствующие читаются как Null.
    named: BTreeMap<String, Val>,
    /// Бюджет шагов вычисления (дефолт конфига интерпретатора — 10000).
    budget: u64,
    /// Израсходованные шаги.
    steps: u64,
}

impl Default for L2Env {
    fn default() -> Self {
        Self::new()
    }
}

impl L2Env {
    /// Новое окружение: один корневой кадр, без бюджета (u64::MAX —
    /// ограничение навязывает [`crate::interp::L2Config`]).
    pub fn new() -> Self {
        Self {
            locals: vec![std::array::from_fn(|_| Val::Null)],
            globals: BTreeMap::new(),
            named: BTreeMap::new(),
            budget: u64::MAX,
            steps: 0,
        }
    }

    // --- бюджет шагов ------------------------------------------------------

    /// Установить бюджет (интерпретатор делает это из `L2Config`).
    pub fn set_budget(&mut self, budget: u64) {
        self.budget = budget;
    }

    /// Один шаг вычисления; при исчерпании — `MaxEvalSteps` (E_LIMIT_EXCEEDED).
    pub fn tick(&mut self) -> Result<(), L2Error> {
        self.steps += 1;
        if self.steps > self.budget {
            return Err(L2Error::MaxEvalSteps { limit: self.budget });
        }
        Ok(())
    }

    /// Счётчик израсходованных шагов (диагностика/тесты).
    pub fn steps_used(&self) -> u64 {
        self.steps
    }

    // --- стек локальных кадров ----------------------------------------------

    /// Push кадра локальных #1–#33 (CallSub: все 33 слота нового кадра — Null).
    pub fn push_frame(&mut self) {
        self.locals.push(std::array::from_fn(|_| Val::Null));
    }

    /// Pop кадра (возврат из подпрограммы); pop корневого кадра — ошибка.
    pub fn pop_frame(&mut self) -> Result<(), L2Error> {
        if self.locals.len() <= 1 {
            return Err(L2Error::RuntimeError(
                "local frame stack underflow (RETURN without CALL)".to_string(),
            ));
        }
        self.locals.pop();
        Ok(())
    }

    /// Глубина стека кадров (тесты/диагностика).
    pub fn frame_depth(&self) -> usize {
        self.locals.len()
    }

    // --- прямые чтение/запись -----------------------------------------------

    /// Чтение по прямому номеру #idx:
    /// 0 → Null (всегда); 1–33 → верх кадра; 100–599 → глобальные
    /// (отсутствующие → Null); прочие диапазоны — ошибка (ловит
    /// опечатки/баги косвенной адресации).
    pub fn read_direct(&self, idx: u32) -> Result<Val, L2Error> {
        match idx {
            0 => Ok(Val::Null),
            1..=33 => {
                let frame = self.locals.last().expect("root frame always present");
                Ok(frame[(idx - 1) as usize])
            }
            GLOBAL_MIN..=GLOBAL_MAX => Ok(self.globals.get(&idx).cloned().unwrap_or(Val::Null)),
            _ => Err(L2Error::RuntimeError(format!(
                "variable #{idx} outside supported ranges (#0, #1-#33, #100-#599)"
            ))),
        }
    }

    /// Запись по прямому номеру: 1–33 → верх кадра; 100–599 → глобальные;
    /// запись в #0 запрещена (Null не хранится в переменных — #0 всегда
    /// читается как Null); прочие диапазоны — ошибка.
    pub fn write_direct(&mut self, idx: u32, v: Val) -> Result<(), L2Error> {
        match idx {
            0 => Err(L2Error::RuntimeError(
                "#0 is read-only (always Null); assignment to #0 is forbidden".to_string(),
            )),
            1..=33 => {
                let frame = self.locals.last_mut().expect("root frame always present");
                frame[(idx - 1) as usize] = v;
                Ok(())
            }
            GLOBAL_MIN..=GLOBAL_MAX => {
                // Присваивание копирует Null: переменная становится пустой.
                self.globals.insert(idx, v);
                Ok(())
            }
            _ => Err(L2Error::RuntimeError(format!(
                "variable #{idx} outside supported ranges (#1-#33, #100-#599 writable)"
            ))),
        }
    }

    /// Чтение именованной (отсутствующая → Null).
    pub fn read_named(&self, name: &str) -> Val {
        self.named.get(name).cloned().unwrap_or(Val::Null)
    }

    /// Запись именованной (присваивание копирует Null).
    pub fn write_named(&mut self, name: impl Into<String>, v: Val) {
        self.named.insert(name.into(), v);
    }
}

// ===========================================================================
// Разрешение ссылок (общее для eval_expr и интерпретатора)
// ===========================================================================

/// Вычислить числовой индекс косвенной адресации `#[expr]`: значение
/// выражения через `as_num`, дробная часть отбрасывается (trunc — FIX).
pub fn resolve_var_index(expr: &Expr, env: &mut L2Env) -> Result<u32, L2Error> {
    let v = eval_expr(expr, env)?;
    let n = v.as_num().trunc();
    if !n.is_finite() || n < 0.0 || n > u32::MAX as f64 {
        return Err(L2Error::RuntimeError(format!(
            "indirect variable index out of range: {n}"
        )));
    }
    Ok(n as u32)
}

/// Прочитать переменную по [`VarRef`] (Direct/Named/Indirect/Sys).
/// Системные переменные при неизвестном состоянии станка → `RuntimeError`
/// (класс `E_RUNTIME_STATE`, останов — §3 последняя строка; в D0–D2
/// снапшота станка нет, поэтому чтение ВСЕХ Sys-переменных — останов).
pub fn read_var(r: &VarRef, env: &mut L2Env) -> Result<Val, L2Error> {
    match r {
        VarRef::Direct(idx) => env.read_direct(*idx),
        VarRef::Indirect(idx_expr) => {
            let idx = resolve_var_index(idx_expr, env)?;
            env.read_direct(idx)
        }
        VarRef::Named(name) => Ok(env.read_named(name)),
        VarRef::Sys(sv) => Err(L2Error::RuntimeError(format!(
            "system variable {} requires machine state (unknown in D0-D2) — E_RUNTIME_STATE stop",
            sys_var_name(sv)
        ))),
    }
}

/// Записать значение в [`VarRef`] (Direct/Named/Indirect; Sys — запись
/// запрещена). Присваивание копирует Null (движок хранит Val::Null как есть).
pub fn write_var(r: &VarRef, v: Val, env: &mut L2Env) -> Result<(), L2Error> {
    match r {
        VarRef::Direct(idx) => env.write_direct(*idx, v),
        VarRef::Indirect(idx_expr) => {
            let idx = resolve_var_index(idx_expr, env)?;
            env.write_direct(idx, v)
        }
        VarRef::Named(name) => {
            env.write_named(name.clone(), v);
            Ok(())
        }
        VarRef::Sys(sv) => Err(L2Error::RuntimeError(format!(
            "assignment to system variable {} is forbidden",
            sys_var_name(sv)
        ))),
    }
}

/// Имя системной переменной для сообщений.
fn sys_var_name(sv: &SysVar) -> String {
    match sv {
        SysVar::AbsMachinePos(a) => format!("#ABS[{}]", a.letter()),
        SysVar::WorkPos(a) => format!("#WORK[{}]", a.letter()),
        SysVar::SkipPos(a) => format!("#SKIP[{}]", a.letter()),
        SysVar::ModalGroup(g) => format!("#MODAL[{g}]"),
        SysVar::WcsOffset { p, axis } => format!("#WCS[{p}.{}]", axis.letter()),
        SysVar::VendorSpecific(n) => format!("#VENDOR[{n}]"),
    }
}

// ===========================================================================
// eval_expr — вычисление выражения (Result<Val, L2Error>)
// ===========================================================================

/// Вычислить выражение (семантика #0). Один шаг бюджета на узел AST.
pub fn eval_expr(expr: &Expr, env: &mut L2Env) -> Result<Val, L2Error> {
    env.tick()?;
    match expr {
        Expr::Const(v) => Ok(Val::Num(v.value())),
        Expr::Null => Ok(Val::Null),
        Expr::Var(r) => read_var(r, env),
        Expr::Unary(op, inner) => {
            let v = eval_expr(inner, env)?;
            eval_unary(*op, &v)
        }
        Expr::Binary(op, lhs, rhs) => {
            let a = eval_expr(lhs, env)?;
            let b = eval_expr(rhs, env)?;
            eval_binary(*op, &a, &b)
        }
    }
}

/// Унарные операции (Приложение, таблица 2.2): тригонометрия в ГРАДУСАХ
/// (стандарт Fanuc Custom Macro B), ROUND — к ближайшему (0.5 — от нуля),
/// FIX — усечение к нулю, FUP — округление вверх к +∞ (стандарт Fanuc
/// Custom Macro B, утверждено оркестратором). LN(0)/LN[#0] — `LnOfNull`;
/// SQRT/LN из отрицательного, ASIN/ACOS вне [-1,1] — `RuntimeError`.
pub fn eval_unary(op: UnaryOp, v: &Val) -> Result<Val, L2Error> {
    // Результат арифметики ВСЕГДА Num (Null-аргумент → 0.0).
    let x = v.as_num();
    let r = match op {
        // Таблица 2.2: -Null → Num(-0.0) «канонизируется в Num(0.0)».
        UnaryOp::Neg => {
            let r = -x;
            if r == 0.0 {
                0.0 // канонизация -0.0 → +0.0 (бит-паттерн)
            } else {
                r
            }
        }
        UnaryOp::Abs => x.abs(),
        UnaryOp::Sin => x.to_radians().sin(),
        UnaryOp::Cos => x.to_radians().cos(),
        UnaryOp::Tan => x.to_radians().tan(),
        UnaryOp::Asin => {
            if !(-1.0..=1.0).contains(&x) {
                return Err(L2Error::RuntimeError(format!("ASIN domain violation: {x}")));
            }
            x.asin().to_degrees()
        }
        UnaryOp::Acos => {
            if !(-1.0..=1.0).contains(&x) {
                return Err(L2Error::RuntimeError(format!("ACOS domain violation: {x}")));
            }
            x.acos().to_degrees()
        }
        UnaryOp::Atan => x.atan().to_degrees(),
        UnaryOp::Sqrt => {
            if x < 0.0 {
                return Err(L2Error::RuntimeError(format!("SQRT of negative: {x}")));
            }
            x.sqrt()
        }
        UnaryOp::Ln => {
            if x == 0.0 {
                // LN[#0] → ошибка (таблица 2.2): Null-аргумент тоже даёт 0.0.
                return Err(L2Error::LnOfNull);
            }
            if x < 0.0 {
                return Err(L2Error::RuntimeError(format!("LN of negative: {x}")));
            }
            x.ln()
        }
        UnaryOp::Exp => x.exp(),
        UnaryOp::Round => x.round(),
        UnaryOp::Fix => x.trunc(),
        UnaryOp::Fup => x.ceil(),
    };
    Ok(Val::Num(r))
}

/// Бинарные операции: арифметика (2.1), сравнения (2.2), побитовая логика
/// (2.3). Операнды — через `as_num`; результат арифметики/логики — всегда
/// Num; Div/Mod с нулевым (в т.ч. Null) делителем — `DivisionByZero`.
pub fn eval_binary(op: BinaryOp, a: &Val, b: &Val) -> Result<Val, L2Error> {
    let r = match op {
        BinaryOp::Add => a.as_num() + b.as_num(),
        BinaryOp::Sub => a.as_num() - b.as_num(),
        BinaryOp::Mul => a.as_num() * b.as_num(),
        BinaryOp::Div => {
            let d = b.as_num();
            if d == 0.0 {
                return Err(L2Error::DivisionByZero);
            }
            a.as_num() / d
        }
        BinaryOp::Mod => {
            let d = b.as_num();
            if d == 0.0 {
                return Err(L2Error::DivisionByZero);
            }
            // Остаток — truncated remainder (знак следит за делимым):
            // стандарт Fanuc Custom Macro B, утверждено оркестратором.
            a.as_num() % d
        }
        // Таблица 2.4: побитово по i64 (f64 → i64, насыщающий as-cast;
        // Null → 0).
        BinaryOp::And => ((a.as_num() as i64) & (b.as_num() as i64)) as f64,
        BinaryOp::Or => ((a.as_num() as i64) | (b.as_num() as i64)) as f64,
        BinaryOp::Xor => ((a.as_num() as i64) ^ (b.as_num() as i64)) as f64,
        // Таблица 2.3: EQ/NE — Null-строгие (допуск 1e-9); GT/LT/GE/LE —
        // Null → 0.0. Результат — Num(1.0)/Num(0.0) в выражениях.
        BinaryOp::Eq => return Ok(bool_val(a.macro_eq(*b))),
        BinaryOp::Ne => return Ok(bool_val(a.macro_ne(*b))),
        BinaryOp::Gt => return Ok(bool_val(a.macro_gt(*b))),
        BinaryOp::Lt => return Ok(bool_val(a.macro_lt(*b))),
        BinaryOp::Ge => return Ok(bool_val(a.macro_ge(*b))),
        BinaryOp::Le => return Ok(bool_val(a.macro_le(*b))),
    };
    Ok(Val::Num(r))
}

// ===========================================================================
// T3: ед-тесты на КАЖДУЮ строку правил семантики #0 (таблицы 2.1–2.4)
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Axis;

    fn n(v: f64) -> Val {
        Val::Num(v)
    }
    fn bin(op: BinaryOp, a: Val, b: Val) -> Result<Val, L2Error> {
        eval_binary(op, &a, &b)
    }
    fn un(op: UnaryOp, a: Val) -> Result<Val, L2Error> {
        eval_unary(op, &a)
    }

    // ---------- Таблица 2.2: арифметика (Null → 0.0, результат Num) ----------

    #[test]
    fn t21_arith_null_operand_is_zero() {
        // Null + 5 = 5; Null - 5 = -5; 5 * Null = 0; Null - Null = 0.
        assert_eq!(bin(BinaryOp::Add, Val::Null, n(5.0)), Ok(n(5.0)));
        assert_eq!(bin(BinaryOp::Sub, Val::Null, n(5.0)), Ok(n(-5.0)));
        assert_eq!(bin(BinaryOp::Mul, n(5.0), Val::Null), Ok(n(0.0)));
        assert_eq!(bin(BinaryOp::Sub, Val::Null, Val::Null), Ok(n(0.0)));
    }

    #[test]
    fn t21_pin_assignment_arithmetic_strips_null() {
        // ПИН (таблица 2.1, строка 2): «#100 = #0 + 0» — любая арифметика
        // снимает статус Null: результат Num(0.0), НЕ Null.
        assert_eq!(bin(BinaryOp::Add, Val::Null, n(0.0)), Ok(n(0.0)));
        // Контраст с чистым Null (строка 1): Null без арифметики — Null.
        assert_eq!(Val::Null, Val::Null);
        assert!(Val::Null.is_null());
    }

    #[test]
    fn t21_pin_neg_null_canonical_zero() {
        // ПИН (таблица 2.2): -Null → Num(-0.0) «канонизируется в Num(0.0)»
        // — бит-паттерн именно +0.0 (знаковый бит не установлен).
        let r = un(UnaryOp::Neg, Val::Null).unwrap();
        assert_eq!(r, n(0.0));
        let bits = match r {
            Val::Num(v) => v.to_bits(),
            Val::Null => panic!("Neg must return Num"),
        };
        assert_eq!(bits, 0.0f64.to_bits(), "-0.0 must be canonicalized to +0.0");
    }

    #[test]
    fn pin_macro_eq_epsilon_and_null_strictness() {
        // ПИН (эталон Приложения §4): EQ чисел с допуском Macro B 1e-9;
        // EQ/NE — Null равен только Null (по значению bool).
        assert!(n(0.1 + 0.2).macro_eq(n(0.3)), "0.1+0.2 == 0.3 within 1e-9");
        assert!(!n(1.0).macro_eq(n(1.001)));
        assert!(Val::Null.macro_eq(Val::Null));
        assert!(!Val::Null.macro_eq(n(0.0)), "Null != 0.0 (главное отличие)");
        assert!(!n(0.0).macro_eq(Val::Null));
        assert!(Val::Null.macro_ne(n(0.0)));
        assert!(!Val::Null.macro_ne(Val::Null));
        // GE/LE с Null → 0.0-сравнение (таблица 2.3: «#0 GE 0 и #0 LE 0 — true»).
        assert!(Val::Null.macro_ge(n(0.0)));
        assert!(Val::Null.macro_le(n(0.0)));
        assert!(Val::Null.macro_ge(Val::Null));
    }

    #[test]
    fn t21_arith_basic() {
        assert_eq!(bin(BinaryOp::Add, n(2.5), n(3.5)), Ok(n(6.0)));
        assert_eq!(bin(BinaryOp::Sub, n(10.0), n(4.0)), Ok(n(6.0)));
        assert_eq!(bin(BinaryOp::Mul, n(-3.0), n(2.0)), Ok(n(-6.0)));
        assert_eq!(bin(BinaryOp::Div, n(7.0), n(2.0)), Ok(n(3.5)));
        assert_eq!(bin(BinaryOp::Mod, n(7.0), n(3.0)), Ok(n(1.0)));
        assert_eq!(bin(BinaryOp::Mod, n(-7.0), n(3.0)), Ok(n(-1.0)));
    }

    #[test]
    fn t21_div_by_null_is_error() {
        // 5 / Null → ошибка (Null-делитель = 0.0).
        assert_eq!(
            bin(BinaryOp::Div, n(5.0), Val::Null),
            Err(L2Error::DivisionByZero)
        );
        // и класс — E_INVALID_INPUT.
        assert_eq!(
            bin(BinaryOp::Div, n(5.0), Val::Null)
                .unwrap_err()
                .error_code(),
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn t21_div_by_zero_is_error() {
        assert_eq!(
            bin(BinaryOp::Div, n(1.0), n(0.0)),
            Err(L2Error::DivisionByZero)
        );
        assert_eq!(
            bin(BinaryOp::Mod, n(1.0), n(0.0)),
            Err(L2Error::DivisionByZero)
        );
        assert_eq!(
            bin(BinaryOp::Mod, n(1.0), n(0.0)).unwrap_err().error_code(),
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn t21_neg_null_is_zero() {
        assert_eq!(un(UnaryOp::Neg, Val::Null), Ok(n(0.0)));
        assert_eq!(un(UnaryOp::Neg, n(-4.25)), Ok(n(4.25)));
    }

    #[test]
    fn t21_ln_of_null_is_error() {
        assert_eq!(un(UnaryOp::Ln, Val::Null), Err(L2Error::LnOfNull));
        assert_eq!(un(UnaryOp::Ln, n(0.0)), Err(L2Error::LnOfNull));
        assert_eq!(
            un(UnaryOp::Ln, Val::Null).unwrap_err().error_code(),
            ErrorCode::InvalidInput
        );
        assert_eq!(
            un(UnaryOp::Ln, n(-1.0)).unwrap_err(),
            L2Error::RuntimeError("LN of negative: -1".into())
        );
        assert_eq!(un(UnaryOp::Ln, n(1.0)), Ok(n(0.0)));
        assert_eq!(un(UnaryOp::Ln, n(std::f64::consts::E)), Ok(n(1.0)));
    }

    #[test]
    fn t21_unary_math() {
        // Тригонометрия в градусах.
        assert!((un(UnaryOp::Sin, n(30.0)).unwrap().as_num() - 0.5).abs() < 1e-12);
        assert!((un(UnaryOp::Cos, n(60.0)).unwrap().as_num() - 0.5).abs() < 1e-12);
        assert!((un(UnaryOp::Tan, n(45.0)).unwrap().as_num() - 1.0).abs() < 1e-12);
        assert!((un(UnaryOp::Asin, n(0.5)).unwrap().as_num() - 30.0).abs() < 1e-12);
        assert!((un(UnaryOp::Acos, n(0.5)).unwrap().as_num() - 60.0).abs() < 1e-12);
        assert!((un(UnaryOp::Atan, n(1.0)).unwrap().as_num() - 45.0).abs() < 1e-12);
        assert!((un(UnaryOp::Asin, Val::Null).unwrap().as_num()).abs() < 1e-12); // asin(0)=0
                                                                                 // Корни/степени/модуль.
        assert_eq!(un(UnaryOp::Sqrt, n(9.0)), Ok(n(3.0)));
        assert_eq!(un(UnaryOp::Abs, n(-3.0)), Ok(n(3.0)));
        assert_eq!(un(UnaryOp::Abs, Val::Null), Ok(n(0.0)));
        assert_eq!(un(UnaryOp::Exp, n(0.0)), Ok(n(1.0)));
        assert_eq!(un(UnaryOp::Exp, Val::Null), Ok(n(1.0)));
        assert!(un(UnaryOp::Exp, n(1.0)).unwrap().as_num() - std::f64::consts::E < 1e-12);
        // Домены.
        assert!(matches!(
            un(UnaryOp::Sqrt, n(-1.0)),
            Err(L2Error::RuntimeError(_))
        ));
        assert!(matches!(
            un(UnaryOp::Asin, n(2.0)),
            Err(L2Error::RuntimeError(_))
        ));
        assert!(matches!(
            un(UnaryOp::Acos, n(-2.0)),
            Err(L2Error::RuntimeError(_))
        ));
    }

    #[test]
    fn t21_round_fix_fup() {
        // ROUND — к ближайшему, 0.5 от нуля (f64::round).
        assert_eq!(un(UnaryOp::Round, n(1.5)), Ok(n(2.0)));
        assert_eq!(un(UnaryOp::Round, n(-1.5)), Ok(n(-2.0)));
        assert_eq!(un(UnaryOp::Round, n(1.4)), Ok(n(1.0)));
        // FIX — усечение к нулю.
        assert_eq!(un(UnaryOp::Fix, n(1.9)), Ok(n(1.0)));
        assert_eq!(un(UnaryOp::Fix, n(-1.9)), Ok(n(-1.0)));
        // FUP — вверх (к +∞; предположение, см. модуль).
        assert_eq!(un(UnaryOp::Fup, n(1.1)), Ok(n(2.0)));
        assert_eq!(un(UnaryOp::Fup, n(-1.1)), Ok(n(-1.0)));
        // Null-аргумент → 0-функции.
        assert_eq!(un(UnaryOp::Round, Val::Null), Ok(n(0.0)));
        assert_eq!(un(UnaryOp::Fix, Val::Null), Ok(n(0.0)));
        assert_eq!(un(UnaryOp::Fup, Val::Null), Ok(n(0.0)));
    }

    // ---------- Таблица 2.3: сравнения ---------------------------------------

    #[test]
    fn t22_eq_null_only_equals_null() {
        assert_eq!(bin(BinaryOp::Eq, Val::Null, Val::Null), Ok(n(1.0)));
        // КЛЮЧЕВАЯ строка: Null НЕ равен 0.
        assert_eq!(bin(BinaryOp::Eq, Val::Null, n(0.0)), Ok(n(0.0)));
        assert_eq!(bin(BinaryOp::Eq, n(0.0), Val::Null), Ok(n(0.0)));
        assert_eq!(bin(BinaryOp::Eq, n(2.0), n(2.0)), Ok(n(1.0)));
        assert_eq!(bin(BinaryOp::Eq, n(2.0), n(3.0)), Ok(n(0.0)));
    }

    #[test]
    fn t22_ne_mirror_of_eq() {
        assert_eq!(bin(BinaryOp::Ne, Val::Null, Val::Null), Ok(n(0.0)));
        assert_eq!(bin(BinaryOp::Ne, Val::Null, n(0.0)), Ok(n(1.0)));
        assert_eq!(bin(BinaryOp::Ne, n(2.0), n(3.0)), Ok(n(1.0)));
        assert_eq!(bin(BinaryOp::Ne, n(3.0), n(3.0)), Ok(n(0.0)));
    }

    #[test]
    fn t22_ordering_null_coerces_to_zero() {
        // GT/LT/GE/LE: Null → 0.0 с ОБЕИХ сторон.
        assert_eq!(bin(BinaryOp::Gt, Val::Null, Val::Null), Ok(n(0.0))); // 0 > 0
        assert_eq!(bin(BinaryOp::Ge, Val::Null, Val::Null), Ok(n(1.0))); // 0 >= 0
        assert_eq!(bin(BinaryOp::Lt, Val::Null, Val::Null), Ok(n(0.0)));
        assert_eq!(bin(BinaryOp::Le, Val::Null, Val::Null), Ok(n(1.0)));
        assert_eq!(bin(BinaryOp::Gt, Val::Null, n(1.0)), Ok(n(0.0))); // 0 > 1
        assert_eq!(bin(BinaryOp::Lt, Val::Null, n(1.0)), Ok(n(1.0))); // 0 < 1
        assert_eq!(bin(BinaryOp::Gt, n(1.0), Val::Null), Ok(n(1.0))); // 1 > 0
        assert_eq!(bin(BinaryOp::Lt, n(-1.0), Val::Null), Ok(n(1.0))); // -1 < 0
        assert_eq!(bin(BinaryOp::Ge, Val::Null, n(0.0)), Ok(n(1.0))); // 0 >= 0
        assert_eq!(bin(BinaryOp::Le, Val::Null, n(-1.0)), Ok(n(0.0))); // 0 <= -1
        assert_eq!(bin(BinaryOp::Ge, n(5.0), n(5.0)), Ok(n(1.0)));
        assert_eq!(bin(BinaryOp::Le, n(4.0), n(5.0)), Ok(n(1.0)));
        assert_eq!(bin(BinaryOp::Gt, n(5.0), n(4.0)), Ok(n(1.0)));
        assert_eq!(bin(BinaryOp::Lt, n(5.0), n(4.0)), Ok(n(0.0)));
    }

    // ---------- Таблица 2.4: побитовая логика (i64) --------------------------

    #[test]
    fn t23_bitwise_and_or_xor() {
        assert_eq!(bin(BinaryOp::And, n(5.0), n(3.0)), Ok(n(1.0))); // 101 & 011
        assert_eq!(bin(BinaryOp::Or, n(5.0), n(3.0)), Ok(n(7.0))); // 101 | 011
        assert_eq!(bin(BinaryOp::Xor, n(5.0), n(3.0)), Ok(n(6.0))); // 101 ^ 011
        assert_eq!(bin(BinaryOp::And, n(12.0), n(10.0)), Ok(n(8.0)));
        assert_eq!(bin(BinaryOp::Or, n(12.0), n(10.0)), Ok(n(14.0)));
        assert_eq!(bin(BinaryOp::Xor, n(12.0), n(10.0)), Ok(n(6.0)));
    }

    #[test]
    fn t23_bitwise_negative_and_null() {
        // Отрицательные — через i64 (доп. код).
        assert_eq!(bin(BinaryOp::And, n(-1.0), n(255.0)), Ok(n(255.0)));
        assert_eq!(bin(BinaryOp::Or, n(-2.0), n(1.0)), Ok(n(-1.0)));
        assert_eq!(bin(BinaryOp::Xor, n(-1.0), n(-1.0)), Ok(n(0.0)));
        // Null → 0.0 → 0; результат всегда Num.
        assert_eq!(bin(BinaryOp::And, Val::Null, n(255.0)), Ok(n(0.0)));
        assert_eq!(bin(BinaryOp::Or, Val::Null, Val::Null), Ok(n(0.0)));
        assert_eq!(bin(BinaryOp::Xor, Val::Null, n(7.0)), Ok(n(7.0)));
    }

    // ---------- Таблица 2.1: адресные слова ----------------------------------

    #[test]
    fn t24_word_with_null_is_ignored() {
        // Слово со значением Null игнорируется (None), числовое — Some.
        assert_eq!(Val::Null.resolve_word(), None);
        assert_eq!(n(12.5).resolve_word(), Some(12.5));
        assert_eq!(n(0.0).resolve_word(), Some(0.0)); // 0 — НЕ Null: слово остаётся
    }

    // ---------- Присваивание копирует Null ------------------------------------

    #[test]
    fn assign_copies_null() {
        let mut env = L2Env::new();
        env.write_direct(100, Val::Null).unwrap();
        assert_eq!(env.read_direct(100).unwrap(), Val::Null);
        // именованная — то же.
        env.write_named("depth", Val::Null);
        assert_eq!(env.read_named("depth"), Val::Null);
        // перезапись числа Null'ом.
        env.write_direct(100, n(5.0)).unwrap();
        assert_eq!(env.read_direct(100).unwrap(), n(5.0));
        env.write_direct(100, Val::Null).unwrap();
        assert_eq!(env.read_direct(100).unwrap(), Val::Null);
    }

    // ---------- Окружение: диапазоны, стек кадров, #0 -------------------------

    #[test]
    fn env_locals_stack_and_ranges() {
        let mut env = L2Env::new();
        // Локальные #1–#33 (корневой кадр).
        env.write_direct(1, n(11.0)).unwrap();
        env.write_direct(33, n(33.0)).unwrap();
        assert_eq!(env.read_direct(1).unwrap(), n(11.0));
        assert_eq!(env.read_direct(33).unwrap(), n(33.0));
        // #0 — чтение всегда Null, запись запрещена.
        assert_eq!(env.read_direct(0).unwrap(), Val::Null);
        assert!(matches!(
            env.write_direct(0, n(1.0)),
            Err(L2Error::RuntimeError(_))
        ));
        // Вне диапазонов — ошибки.
        assert!(matches!(env.read_direct(34), Err(L2Error::RuntimeError(_))));
        assert!(matches!(
            env.write_direct(99, n(1.0)),
            Err(L2Error::RuntimeError(_))
        ));
        assert!(matches!(
            env.write_direct(600, n(1.0)),
            Err(L2Error::RuntimeError(_))
        ));
        // Глобальные #100–#599: отсутствующие — Null.
        assert_eq!(env.read_direct(100).unwrap(), Val::Null);
        assert_eq!(env.read_direct(599).unwrap(), Val::Null);
        env.write_direct(599, n(1.0)).unwrap();
        assert_eq!(env.read_direct(599).unwrap(), n(1.0));
    }

    #[test]
    fn env_frame_push_pop() {
        let mut env = L2Env::new();
        env.write_direct(1, n(7.0)).unwrap();
        env.push_frame();
        // Новый кадр: локальные сброшены в Null (теневая семантика).
        assert_eq!(env.read_direct(1).unwrap(), Val::Null);
        env.write_direct(1, n(42.0)).unwrap();
        assert_eq!(env.read_direct(1).unwrap(), n(42.0));
        env.pop_frame().unwrap();
        assert_eq!(env.read_direct(1).unwrap(), n(7.0));
        assert_eq!(env.frame_depth(), 1);
        // Pop корневого — ошибка.
        assert!(matches!(env.pop_frame(), Err(L2Error::RuntimeError(_))));
    }

    // ---------- eval_expr: переменные и косвенность ---------------------------

    #[test]
    fn eval_expr_vars_and_indirect() {
        let mut env = L2Env::new();
        env.write_direct(100, n(5.0)).unwrap();
        env.write_direct(5, n(55.0)).unwrap();
        // #[#100] → #5 (локальная, записана 55).
        let e = Expr::Var(VarRef::Indirect(Box::new(Expr::var(100))));
        assert_eq!(eval_expr(&e, &mut env).unwrap(), n(55.0));
        // Дробный индекс → trunc.
        env.write_direct(101, n(2.9)).unwrap();
        let e = Expr::Var(VarRef::Indirect(Box::new(Expr::var(101))));
        assert_eq!(eval_expr(&e, &mut env).unwrap(), Val::Null); // #2 локальная → Null
        env.write_direct(2, n(9.0)).unwrap();
        assert_eq!(eval_expr(&e, &mut env).unwrap(), n(9.0));
        // Индекс-Null → #0 → чтение Null (запись была бы ошибкой).
        let e = Expr::Var(VarRef::Indirect(Box::new(Expr::Null)));
        assert_eq!(eval_expr(&e, &mut env).unwrap(), Val::Null);
        // Литеральный Null-вар.
        assert_eq!(eval_expr(&Expr::Null, &mut env).unwrap(), Val::Null);
    }

    #[test]
    fn eval_expr_named_and_sys() {
        let mut env = L2Env::new();
        env.write_named("feed_scale", n(0.5));
        let e = Expr::Var(VarRef::Named("feed_scale".into()));
        assert_eq!(eval_expr(&e, &mut env).unwrap(), n(0.5));
        // Отсутствующая именованная → Null.
        let e = Expr::Var(VarRef::Named("nope".into()));
        assert_eq!(eval_expr(&e, &mut env).unwrap(), Val::Null);
        // Системная при неизвестном состоянии станка → останов E_RUNTIME_STATE.
        let e = Expr::Var(VarRef::Sys(SysVar::SkipPos(Axis::X)));
        let err = eval_expr(&e, &mut env).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::RuntimeState);
        let e = Expr::Var(VarRef::Sys(SysVar::AbsMachinePos(Axis::Z)));
        assert_eq!(
            eval_expr(&e, &mut env).unwrap_err().error_code(),
            ErrorCode::RuntimeState
        );
        let e = Expr::Var(VarRef::Sys(SysVar::WcsOffset {
            p: 1,
            axis: Axis::X,
        }));
        assert!(matches!(
            eval_expr(&e, &mut env),
            Err(L2Error::RuntimeError(_))
        ));
    }

    #[test]
    fn eval_expr_nested_tree() {
        let mut env = L2Env::new();
        env.write_direct(1, n(10.0)).unwrap();
        // #1 * (2 + 3) - 4 → 46; сравнение > 40 → 1.
        let e = Expr::Binary(
            BinaryOp::Gt,
            Box::new(Expr::Binary(
                BinaryOp::Sub,
                Box::new(Expr::Binary(
                    BinaryOp::Mul,
                    Box::new(Expr::var(1)),
                    Box::new(Expr::Binary(
                        BinaryOp::Add,
                        Box::new(Expr::num(2.0)),
                        Box::new(Expr::num(3.0)),
                    )),
                )),
                Box::new(Expr::num(4.0)),
            )),
            Box::new(Expr::num(40.0)),
        );
        assert_eq!(eval_expr(&e, &mut env).unwrap(), n(1.0));
    }

    #[test]
    fn truthy_and_cond_null_false() {
        // IF [#0] — Null в cond как 0.0 → false.
        assert!(!Val::Null.truthy());
        assert!(!n(0.0).truthy());
        assert!(n(0.5).truthy());
        assert!(n(-1.0).truthy());
    }

    // ---------- Бюджет шагов (T5-низ) -----------------------------------------

    #[test]
    fn step_budget_exhaustion() {
        let mut env = L2Env::new();
        env.set_budget(10);
        let deep = (0..50).fold(Expr::num(1.0), |acc, _| {
            Expr::Binary(BinaryOp::Add, Box::new(acc), Box::new(Expr::num(1.0)))
        });
        let err = eval_expr(&deep, &mut env).unwrap_err();
        assert_eq!(err, L2Error::MaxEvalSteps { limit: 10 });
        assert_eq!(err.error_code(), ErrorCode::LimitExceeded);
    }
}
