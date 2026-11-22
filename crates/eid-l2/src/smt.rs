//! SMT-кодирование семантики #0 (этап 12.3) — ЧИСТЫЙ TERM-BUILDER без
//! внешнего solver-депа. Источник: `EID-L2-DESIGN.md` v0.1 (коммит d3fd57e),
//! §4 «Кодирование #0 (Null) в SMT» и Приложение §3.1–§3.2.
//!
//! # РЕШЕНИЕ ПО FEATURE-GATE (12.3, агент E)
//!
//! Term-builder (типы [`SmtVal`], [`NumExpr`], [`Formula`], [`ValTerm`],
//! [`PathConditions`], [`Encoder`] + concrete-эвалюатор `eval_num` /
//! `eval_formula`) — БЕЗ `#[cfg(feature = "smt")]`. За фичей — ТОЛЬКО
//! solver-адаптер ([`SatResult`], [`SmtSolver`]). Обоснование:
//! 1. Term-builder — чистые std-структуры данных и функции: ноль внешних
//!    зависимостей, ноль unsafe. Чек-лист T2 дока («cargo build -p eid-l2
//!    без --features smt не тянет ни одной внешней зависимости сверх
//!    eid-core») выполняется БЕЗУСЛОВНО — фича ничего бы не изменила.
//! 2. Главная ценность 12.3 без solver'а — кросс-валидация формул против
//!    семантики [`crate::eval`] (таблицы Приложения 2.1–2.4). Тесты
//!    кросс-валидации компилируются и гоняются на КАЖДОЙ сборке
//!    (`cargo test -p eid-l2` без фичей) — расхождение кодирования с
//!    L2Evaluator ловится сразу в CI, а не когда кто-то вспомнит передать
//!    `--features smt`.
//! 3. Внешние зависимости/FFI может потянуть только сам solver-адаптер
//!    (Z3/CVC5) — именно его держит фича `smt = []` (Cargo.toml), что и
//!    есть исходный замысел флага в §0 дока («Trust Story»: SMT-решатель
//!    подключается строго через опциональный feature-gate).
//!
//! # §3.1 — АКСИОМА НОРМАЛИЗАЦИИ
//!
//! Каждое значение `Val` кодируется кортежем `SMT(v) = (is_null: Bool,
//! num: Real)`; concrete-зеркало — [`SmtVal`]:
//! * `Val::Null`    → `is_null = true,  num = 0.0`;
//! * `Val::Num(c)`  → `is_null = false, num = c`;
//! * неизвестная переменная #i: `is_null_i`/`num_i` свободны, но связаны
//!   ИНВАРИАНТОМ `is_null_i ⟹ (num_i == 0.0)` — благодаря ему арифметика
//!   кодируется БЕЗ ite(is_null, 0.0, val) (§3.2, строки a±b/a*b).
//!   Инвариант enforced assert'ом в [`SmtVal::new`] и запинен тестом
//!   `smtval_invariant_violation_panics`.
//!
//! # §3.2 — ТАБЛИЦА ТРАНСЛЯЦИИ (реализована дословно)
//!
//! ```text
//! a + b  → is_null = false; num_a + num_b        ([NumExpr::Add])
//! a - b  → num_a - num_b                          ([NumExpr::Sub])
//! a * b  → num_a * num_b                          ([NumExpr::Mul])
//! a / b  → num_a / num_b + SIDE-CONDITION
//!          num_b ≠ 0.0 (в PathConditions, НЕ в терме)
//! a EQ b → (is_null_a && is_null_b) ||
//!          (!is_null_a && !is_null_b && num_a == num_b)
//!          — точная формула дока: Null ≠ 0.0! ([macro_eq_formula])
//! a NE b → !(a EQ b)
//! a GT b → num_a > num_b   (инвариант: Null сравнивается как 0.0)
//! a LT b → num_a < num_b; GE/LE — аналогично (Null >= 0.0 → true!)
//! G0 X(expr) → is_null_expr == true → слово X ИГНОРИРУЕТСЯ, координата
//!              наследуется от prev_end.x (таблица 2.1 — resolve_word);
//!              иначе → num_expr. Кодирование — та же пара ValTerm:
//!              is_null-формула = условие наследования, num = значение.
//! ```
//!
//! Отличие от черновика брифа: атомы `EqNum`/`GtNum`/… берут [`NumExpr`]
//! операнды, а не только `VarId` — иначе невыразимо `[#1 + 2 EQ #3]`
//! (таблица §3.2 говорит об операндах-ЗНАЧЕНИЯХ). `EqNum(Var, Var)` —
//! частный случай; новых узлов IR это не добавляет.
//!
//! # Точный IR и допуск Macro B (важный modeling-момент)
//!
//! ВСЕ атомы IR — точные f64-операции (`EqNum` — точное `==`, как в
//! SMT-LIB `(= ...)` на Real). Два ноль-чувствительных места eval.rs —
//! truthiness (`as_num() != 0.0`) и side-condition деления (`d == 0.0`)
//! — ТОЧНЫЕ, поэтому точный атом обязателен (генерируемая кросс-валидация
//! ловила обратное: 1e-9 при tolerance-атоме ложно отсекалась как ноль).
//!
//! Допуск Macro B 1e-9 из эталона `Val::macro_eq` (Приложение §4)
//! моделируется СТРУКТУРНО в [`macro_eq_formula`]: `|x - y| <= 1e-9`
//! раскрывается в `(x - y <= 1e-9) && (y - x <= 1e-9)` — те же
//! f64-операции, что `(a - b).abs() <= 1e-9` (NaN → false в обеих
//! формах), поэтому concrete-кросс-валидация с eval.rs — бит-в-бит, и
//! exact-Real solver'у не нужен никакой особый принтер.
//!
//! # Слои кодирования
//!
//! * [`Encoder`] — [`crate::ast::Expr`] → термы поверх СУЩЕСТВУЮЩИХ
//!   Val/Expr (новых вариантов AST НЕ вводится). Поддержана ровно
//!   таблица §3.2: Null/Const/Var(Direct) + Add/Sub/Mul/Div + шесть
//!   сравнений; всё остальное (Mod, побитовые, ATAN2, унарные функции,
//!   Named/Indirect/Sys-ссылки, сравнение как арифметический операнд)
//!   — [`EncodeError`] (честная граница, не silent-неточность).
//!   VarId — прямой номер переменной (#100 → 100); проверка диапазонов
//!   — дело окружения ([`crate::eval::L2Env`]), кодирование
//!   диапазонно-агностично. Отсутствующая переменная в concrete-вычислении
//!   читается как Null (как в L2Env).
//! * [`PathConditions`] — side-condition'ы пути (div: делитель ≠ 0),
//!   конъюнкция assert'ов — будущий вход `check(..., assumptions)`.
//! * [`encode_cond`] — точка входа для IF/WHILE-условий (D4:
//!   достижимость веток). Не-сравнение → truthiness-атом `num ≠ 0`
//!   (`IF [#0]` → false — как `Val::truthy`).
//!
//! # Подключение solver'а (12.3 полный)
//!
//! См. доку трейта [`SmtSolver`]: две стратегии — std-only subprocess
//! (`z3 -in -smt2` поверх SMT-LIB-принтера термов, рекомендовано: T2
//! сохраняется даже под фичей) или FFI-крейт (`z3`/cvc5, optional
//! dependency за `smt = ["dep:..."]`).

use crate::ast::{BinaryOp, Expr, Val, VarRef};

// ===========================================================================
// §3.1 — SmtVal: concrete-представление пары (is_null, num)
// ===========================================================================

/// Номер переменной в терме: прямой номер #i (Direct). Кодирование —
/// диапазонно-агностично (валидация диапазонов — в [`crate::eval::L2Env`]).
pub type VarId = u32;

/// Concrete-представление кодировки значения (§3.1):
/// `SMT(v) = (is_null_v : Bool, num_v : Real)`.
///
/// ИНВАРИАНТ (аксиома нормализации): `is_null ⇒ num == 0.0`.
/// Валиден также `!is_null ∧ num == 0.0` — это `Val::Num(0.0)`, НЕ Null
/// (главное отличие семантики #0: Null ≠ 0.0 в EQ/NE!).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmtVal {
    is_null: bool,
    num: f64,
}

impl SmtVal {
    /// Кодировка `Val::Null`: `(true, 0.0)` (§3.1).
    pub const NULL: Self = Self {
        is_null: true,
        num: 0.0,
    };

    /// Конструктор с enforce'ом инварианта: `is_null ⇒ num == 0.0`
    /// (нарушение — паника; запинено тестом-пином).
    pub fn new(is_null: bool, num: f64) -> Self {
        assert!(
            !is_null || num == 0.0,
            "SmtVal invariant violated: is_null implies num == 0.0 (got is_null={is_null}, num={num})"
        );
        Self { is_null, num }
    }

    /// Аксиома нормализации (§3.1): `Null → (true, 0.0)`,
    /// `Num(v) → (false, v)`.
    pub fn from_val(v: Val) -> Self {
        match v {
            Val::Null => Self::NULL,
            Val::Num(x) => Self::new(false, x),
        }
    }

    /// Числовая кодировка `Val::Num(v)` (Null запрещён инвариантом).
    pub fn num(v: f64) -> Self {
        Self::new(false, v)
    }

    /// `is_null_v` — первая компонента пары.
    pub fn is_null(self) -> bool {
        self.is_null
    }

    /// `num_v` — вторая компонента (при is_null — всегда 0.0).
    pub fn num_value(self) -> f64 {
        self.num
    }

    /// Таблица 2.1 (адресные слова) / строка G0 таблицы §3.2, concrete
    /// сторона: Null → None (слово игнорируется, координата наследуется
    /// от prev_end), Num → Some(v). Зеркало `Val::resolve_word` из eval.rs.
    pub fn resolve_word(self) -> Option<f64> {
        match self.is_null {
            true => None,
            false => Some(self.num),
        }
    }
}

// ===========================================================================
// Term-IR — формулы и числовые термы как данные (без solver)
// ===========================================================================

/// Числовой терм (`num_i`-компоненты кодировки): арифметика таблицы §3.2
/// БЕЗ ite — благодаря инварианту is_null ⇒ num == 0.0.
///
/// [`NumExpr::Div`] несёт side-condition `num_b ≠ 0.0` — он НЕ в терме,
/// а в [`PathConditions`] (assert пути), как в таблице дока.
#[derive(Debug, Clone, PartialEq)]
pub enum NumExpr {
    /// `num_i` переменной #i.
    Var(VarId),
    /// Константа (в т.ч. кодировка `Expr::Const`).
    Const(f64),
    /// §3.2: `a + b` → `num_a + num_b`.
    Add(Box<NumExpr>, Box<NumExpr>),
    /// §3.2: `a - b` → `num_a - num_b`.
    Sub(Box<NumExpr>, Box<NumExpr>),
    /// §3.2: `a * b` → `num_a * num_b`.
    Mul(Box<NumExpr>, Box<NumExpr>),
    /// §3.2: `a / b` → `num_a / num_b` + side-condition `num_b ≠ 0`.
    Div(Box<NumExpr>, Box<NumExpr>),
}

impl NumExpr {
    /// Шорткат: константа.
    pub fn konst(v: f64) -> Self {
        Self::Const(v)
    }
    /// Шорткат: переменная #i.
    pub fn var(i: VarId) -> Self {
        Self::Var(i)
    }
    /// Бокс-шорткат суммы (`mk_*` — чтобы не путать с std-трейтами
    /// Add/Sub/Mul/Div; clippy::should_implement_trait).
    pub fn mk_add(a: Self, b: Self) -> Self {
        Self::Add(Box::new(a), Box::new(b))
    }
    /// Бокс-шорткат разности.
    pub fn mk_sub(a: Self, b: Self) -> Self {
        Self::Sub(Box::new(a), Box::new(b))
    }
    /// Бокс-шорткат произведения.
    pub fn mk_mul(a: Self, b: Self) -> Self {
        Self::Mul(Box::new(a), Box::new(b))
    }
    /// Бокс-шорткат деления (side-condition добавляет кодировщик).
    pub fn mk_div(a: Self, b: Self) -> Self {
        Self::Div(Box::new(a), Box::new(b))
    }
}

/// Булева формула над кодировками переменных — «формулы как данные»
/// (таблица §3.2 в терминах IR). Атомы сравнения берут [`NumExpr`]
/// операнды (см. шапку модуля: обобщение против `VarId`-черновика —
/// выразить операнды-ЗНАЧЕНИЯ §3.2; новых узлов не добавляет).
#[derive(Debug, Clone, PartialEq)]
pub enum Formula {
    /// Константная булева (кодировка is_null у Const/арифметики: false;
    /// у `Expr::Null`: true).
    Const(bool),
    /// `is_null_i` — переменная #i vacant.
    IsNull(VarId),
    /// `num_a == num_b` — ТОЧНОЕ равенство (f64 `==`; SMT-LIB `(= …)`;
    /// см. шапку модуля: допуск Macro B моделируется структурно в
    /// [`macro_eq_formula`], а truthiness/side-condition деления в
    /// eval.rs точны). Zero-check атом — [`Formula::num_nonzero`].
    EqNum(NumExpr, NumExpr),
    /// §3.2: `a GT b` → `num_a > num_b` (Null → 0.0 по инварианту).
    GtNum(NumExpr, NumExpr),
    /// §3.2: `a LT b` → `num_a < num_b`.
    LtNum(NumExpr, NumExpr),
    /// §3.2: `a GE b` → `num_a >= num_b` (Null >= 0.0 → true!).
    GeNum(NumExpr, NumExpr),
    /// §3.2: `a LE b` → `num_a <= num_b`.
    LeNum(NumExpr, NumExpr),
    /// Конъюнкция (n-арная).
    And(Vec<Formula>),
    /// Дизъюнкция (n-арная).
    Or(Vec<Formula>),
    /// Отрицание.
    Not(Box<Formula>),
}

impl Formula {
    /// Атом `num ≠ 0.0` — ТОЧНЫЙ ноль-чек (eval.rs: truthiness
    /// `as_num() != 0.0` и side-condition деления `d == 0.0` точны;
    /// НЕ допуск Macro B — для него см. [`macro_eq_formula`]).
    /// Два смысла одной структуры:
    /// * side-condition деления (assert пути, таблица §3.2 строка a/b);
    /// * truthiness IF/WHILE-условия (`IF [#0]` → false, `IF [#1]` при
    ///   #1=Null → false — инвариант даёт num == 0.0 для Null).
    pub fn num_nonzero(e: NumExpr) -> Self {
        Self::Not(Box::new(Self::EqNum(e, NumExpr::Const(0.0))))
    }
}

/// Символьная кодировка ЗНАЧЕНИЯ (§3.1): пара (is_null : Formula, num :
/// NumExpr). Для арифметических результатов is_null ≡ Const(false)
/// (таблица 2.2: результат арифметики ВСЕГДА Num).
#[derive(Debug, Clone, PartialEq)]
pub struct ValTerm {
    /// Формула «значение — Null».
    pub is_null: Formula,
    /// Числовой терм значения (при is_null — 0.0 по инварианту).
    pub num: NumExpr,
}

impl ValTerm {
    /// Кодировка `Expr::Null`: `(Const(true), Const(0.0))`.
    pub fn null() -> Self {
        Self {
            is_null: Formula::Const(true),
            num: NumExpr::Const(0.0),
        }
    }

    /// Кодировка арифметического результата: `(Const(false), num)`.
    pub fn num(num: NumExpr) -> Self {
        Self {
            is_null: Formula::Const(false),
            num,
        }
    }

    /// Кодировка переменной #i: `(IsNull(i), Var(i))` — свободная пара
    /// со связанным инвариантом (§3.1).
    pub fn var(i: VarId) -> Self {
        Self {
            is_null: Formula::IsNull(i),
            num: NumExpr::Var(i),
        }
    }

    /// Const-свёрнутая кодировка конкретного значения (§3.1).
    pub fn from_val(v: Val) -> Self {
        match v {
            Val::Null => Self::null(),
            Val::Num(x) => Self::num(NumExpr::Const(x)),
        }
    }
}

/// Таблица §3.2, строка `a EQ b` — формула дока
/// `(is_null_a && is_null_b) || (!is_null_a && !is_null_b && num_a == num_b)`
/// с допуском Macro B 1e-9 эталона `Val::macro_eq` (Приложение §4),
/// раскрытым СТРУКТУРНО в точных атомах: `(x - y <= 1e-9) &&
/// (y - x <= 1e-9)` (см. шапку модуля «Точный IR и допуск Macro B»).
///
/// Исключает ложные доказательства при `IF [#1 EQ #0]`: Null равен
/// ТОЛЬКО Null (в отличие от GT/LT/GE/LE, где Null → 0.0).
pub fn macro_eq_formula(a: &ValTerm, b: &ValTerm) -> Formula {
    // |x - y| <= 1e-9 структурно (NaN: каждый LeNum → false, как .abs()).
    let tol = Formula::And(vec![
        Formula::LeNum(
            NumExpr::mk_sub(a.num.clone(), b.num.clone()),
            NumExpr::Const(1e-9),
        ),
        Formula::LeNum(
            NumExpr::mk_sub(b.num.clone(), a.num.clone()),
            NumExpr::Const(1e-9),
        ),
    ]);
    Formula::Or(vec![
        Formula::And(vec![a.is_null.clone(), b.is_null.clone()]),
        Formula::And(vec![
            Formula::Not(Box::new(a.is_null.clone())),
            Formula::Not(Box::new(b.is_null.clone())),
            tol,
        ]),
    ])
}

// ===========================================================================
// PathConditions — side-condition'ы пути (div: assert(num_b != 0.0))
// ===========================================================================

/// Side-condition'ы, накопленные кодировщиком на пути (таблица §3.2,
/// строка `a / b`: «+ проверка пути assert(num_b != 0.0)»). Конъюнкция
/// assert'ов; пустой путь — тавтологично истинен.
///
/// Семантика деления: путь ВАЛИДЕН ⟺ все assert'ы истинны; нарушение
/// (делитель 0.0 или Null) соответствует `L2Error::DivisionByZero`
/// → E_INVALID_INPUT в eval.rs. Future solver'у это подаётся как
/// assumptions ([`SmtSolver::check`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PathConditions {
    /// Assert'ы пути (порядок накопления).
    pub asserts: Vec<Formula>,
}

impl PathConditions {
    /// Новый пустой путь.
    pub fn new() -> Self {
        Self::default()
    }

    /// Добавить side-condition.
    pub fn push(&mut self, f: Formula) {
        self.asserts.push(f);
    }

    /// Число assert'ов (диагностика/тесты).
    pub fn len(&self) -> usize {
        self.asserts.len()
    }

    /// Пустой ли путь.
    pub fn is_empty(&self) -> bool {
        self.asserts.is_empty()
    }

    /// Concrete-проверка: все assert'ы пути истинны на данных значениях.
    pub fn is_satisfied(&self, vars: &dyn Fn(VarId) -> SmtVal) -> bool {
        self.asserts.iter().all(|a| eval_formula(a, vars))
    }

    /// Конъюнкция как одна формула (для solver'а): `And` или `Const(true)`.
    pub fn into_formula(self) -> Formula {
        match self.asserts.len() {
            0 => Formula::Const(true),
            1 => self.asserts.into_iter().next().expect("len == 1"),
            _ => Formula::And(self.asserts),
        }
    }
}

// ===========================================================================
// Encoder — Expr (существующий L2 AST) → термы (таблица §3.2)
// ===========================================================================

/// Ошибка кодирования: узел AST вне таблицы §3.2 (честная граница
/// term-builder'а, не silent-неточность).
#[derive(Debug, Clone, PartialEq)]
pub struct EncodeError {
    /// Человекочитаемая причина (какой узел и почему).
    pub reason: String,
}

impl EncodeError {
    fn unsupported(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SMT encode: {}", self.reason)
    }
}

/// Кодировщик [`Expr`] → термы по таблице §3.2 с накоплением
/// [`PathConditions`]. Кодирование работает ПОВЕРХ существующих
/// Val/Expr — новых вариантов AST не вводится (см. шапку модуля).
///
/// Поддержано ровно: `Null`, `Const`, `Var(Direct)`, `Add/Sub/Mul/Div`,
/// `Eq/Ne/Gt/Lt/Ge/Le`. Всё остальное — [`EncodeError`] (Mod/побитовые/
/// ATAN2/унарные функции — вне таблицы §3.2; Named/Indirect/Sys —
/// вне скоупа term-builder'а 12.3; сравнение как АРИФМЕТИЧЕСКИЙ операнд
/// потребовало бы ite, которого IR сознательно не имеет).
#[derive(Debug, Default)]
pub struct Encoder {
    path: PathConditions,
}

impl Encoder {
    /// Новый кодировщик с пустым путём.
    pub fn new() -> Self {
        Self::default()
    }

    /// Кодировать ЗНАЧЕНИЕ (§3.1 пара): для арифметики is_null ≡ false.
    /// Строка G0 таблицы §3.2 использует ту же пару: `is_null` — условие
    /// наследования prev_end, `num` — значение слова.
    pub fn encode_val(&mut self, expr: &Expr) -> Result<ValTerm, EncodeError> {
        match expr {
            Expr::Null => Ok(ValTerm::null()),
            Expr::Const(c) => Ok(ValTerm::num(NumExpr::Const(c.value()))),
            Expr::Var(VarRef::Direct(i)) => Ok(ValTerm::var(*i)),
            Expr::Var(_) => Err(EncodeError::unsupported(
                "only VarRef::Direct is encodable (Named/Indirect/Sys out of term-builder scope)",
            )),
            Expr::Unary(_, _) => Err(EncodeError::unsupported(
                "unary functions (SIN/COS/SQRT/...) are outside §3.2 table",
            )),
            Expr::Binary(op, lhs, rhs) => match op {
                BinaryOp::Add => {
                    let a = self.encode_val(lhs)?;
                    let b = self.encode_val(rhs)?;
                    Ok(ValTerm::num(NumExpr::mk_add(a.num, b.num)))
                }
                BinaryOp::Sub => {
                    let a = self.encode_val(lhs)?;
                    let b = self.encode_val(rhs)?;
                    Ok(ValTerm::num(NumExpr::mk_sub(a.num, b.num)))
                }
                BinaryOp::Mul => {
                    let a = self.encode_val(lhs)?;
                    let b = self.encode_val(rhs)?;
                    Ok(ValTerm::num(NumExpr::mk_mul(a.num, b.num)))
                }
                BinaryOp::Div => {
                    let a = self.encode_val(lhs)?;
                    let b = self.encode_val(rhs)?;
                    // Таблица §3.2: «num_a / num_b + проверка пути
                    // assert(num_b != 0.0)» — side-condition в путь.
                    self.path.push(Formula::num_nonzero(b.num.clone()));
                    Ok(ValTerm::num(NumExpr::mk_div(a.num, b.num)))
                }
                // Сравнение как АРИФМЕТИЧЕСКИЙ операнд (результат
                // Num(1.0/0.0) в eval.rs) потребовало бы ite-терма —
                // IR сознательно его не имеет (шапка модуля).
                BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Gt
                | BinaryOp::Lt
                | BinaryOp::Ge
                | BinaryOp::Le => Err(EncodeError::unsupported(
                    "comparison as arithmetic operand requires ite (out of IR scope)",
                )),
                BinaryOp::Mod => Err(EncodeError::unsupported("Mod outside §3.2 table")),
                BinaryOp::And | BinaryOp::Or | BinaryOp::Xor => Err(EncodeError::unsupported(
                    "bitwise And/Or/Xor outside §3.2 table",
                )),
                BinaryOp::Atan2 => Err(EncodeError::unsupported("Atan2 outside §3.2 table")),
            },
        }
    }

    /// Кодировать БУЛЕВО ЗНАЧЕНИЕ (условие IF/WHILE или сравнение).
    /// Сравнения — по строкам §3.2 (EQ — дословная формула дока);
    /// не-сравнение → truthiness-атом `num ≠ 0` (cond.as_num() != 0).
    pub fn encode_formula(&mut self, expr: &Expr) -> Result<Formula, EncodeError> {
        match expr {
            Expr::Binary(op, lhs, rhs) => match op {
                BinaryOp::Eq => {
                    let a = self.encode_val(lhs)?;
                    let b = self.encode_val(rhs)?;
                    Ok(macro_eq_formula(&a, &b))
                }
                BinaryOp::Ne => {
                    let a = self.encode_val(lhs)?;
                    let b = self.encode_val(rhs)?;
                    Ok(Formula::Not(Box::new(macro_eq_formula(&a, &b))))
                }
                BinaryOp::Gt | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Le => {
                    let a = self.encode_val(lhs)?;
                    let b = self.encode_val(rhs)?;
                    // Инвариант is_null ⇒ num == 0.0 даёт Null → 0.0
                    // автоматически (таблица §3.2, строки GT/LT/GE/LE).
                    let f = match op {
                        BinaryOp::Gt => Formula::GtNum(a.num, b.num),
                        BinaryOp::Lt => Formula::LtNum(a.num, b.num),
                        BinaryOp::Ge => Formula::GeNum(a.num, b.num),
                        _ => Formula::LeNum(a.num, b.num),
                    };
                    Ok(f)
                }
                _ => {
                    // Арифметика в условии: IF [#1 + 1] — truthiness.
                    let v = self.encode_val(expr)?;
                    Ok(Formula::num_nonzero(v.num))
                }
            },
            _ => {
                // IF [#1] / IF [#0]: truthiness (eval.rs: as_num != 0).
                let v = self.encode_val(expr)?;
                Ok(Formula::num_nonzero(v.num))
            }
        }
    }

    /// Забрать накопленные side-condition'ы пути (consumes encoder).
    pub fn into_path(self) -> PathConditions {
        self.path
    }
}

/// Одношаговая точка входа для IF/WHILE-условий (D4: достижимость
/// веток): формула + side-condition'ы пути. Несравнение → truthiness.
pub fn encode_cond(expr: &Expr) -> Result<(Formula, PathConditions), EncodeError> {
    let mut enc = Encoder::new();
    let f = enc.encode_formula(expr)?;
    Ok((f, enc.into_path()))
}

// ===========================================================================
// Concrete-эвалюатор формул (кросс-валидация БЕЗ solver)
// ===========================================================================

/// Concrete-вычисление числового терма. `None` — ТОЛЬКО нарушение
/// side-condition деления (делитель == 0.0/Null) — путь мёртв и должен
/// быть отсечён [`PathConditions`]; поведение формул над мёртвыми
/// подтермами — don't-care (eval_formula даёт false на атоме).
pub fn eval_num(e: &NumExpr, vars: &dyn Fn(VarId) -> SmtVal) -> Option<f64> {
    match e {
        NumExpr::Var(i) => Some(vars(*i).num_value()),
        NumExpr::Const(c) => Some(*c),
        NumExpr::Add(a, b) => Some(eval_num(a, vars)? + eval_num(b, vars)?),
        NumExpr::Sub(a, b) => Some(eval_num(a, vars)? - eval_num(b, vars)?),
        NumExpr::Mul(a, b) => Some(eval_num(a, vars)? * eval_num(b, vars)?),
        NumExpr::Div(a, b) => {
            let d = eval_num(b, vars)?;
            if d == 0.0 {
                None
            } else {
                Some(eval_num(a, vars)? / d)
            }
        }
    }
}

/// Concrete-вычисление формулы на значениях переменных — канал
/// кросс-валидации: `eval_formula(f, vars) == eval_semantics(vars)`
/// (см. тесты). ВСЕ атомы — точные f64-операции, зеркальные eval.rs:
/// * `EqNum` — точное `==` (truthiness и side-condition деления в
///   eval.rs точны);
/// * `GtNum/LtNum/GeNum/LeNum` — `macro_gt/lt/ge/le` (Null → 0.0 по
///   инварианту);
/// * `IsNull` — по первой компоненте значения переменной;
/// * допуск Macro B 1e-9 живёт в СТРУКТУРЕ macro_eq_formula (LeNum/Sub).
pub fn eval_formula(f: &Formula, vars: &dyn Fn(VarId) -> SmtVal) -> bool {
    match f {
        Formula::Const(b) => *b,
        Formula::IsNull(i) => vars(*i).is_null(),
        Formula::EqNum(a, b) => match (eval_num(a, vars), eval_num(b, vars)) {
            (Some(x), Some(y)) => x == y, // ТОЧНО (см. шапку модуля)
            _ => false,
        },
        Formula::GtNum(a, b) => num_cmp(a, b, vars, |x, y| x > y),
        Formula::LtNum(a, b) => num_cmp(a, b, vars, |x, y| x < y),
        Formula::GeNum(a, b) => num_cmp(a, b, vars, |x, y| x >= y),
        Formula::LeNum(a, b) => num_cmp(a, b, vars, |x, y| x <= y),
        Formula::And(fs) => fs.iter().all(|f| eval_formula(f, vars)),
        Formula::Or(fs) => fs.iter().any(|f| eval_formula(f, vars)),
        Formula::Not(f) => !eval_formula(f, vars),
    }
}

/// Числовое сравнение; мёртвый подтерм (None) → false (см. eval_num).
fn num_cmp(
    a: &NumExpr,
    b: &NumExpr,
    vars: &dyn Fn(VarId) -> SmtVal,
    op: impl Fn(f64, f64) -> bool,
) -> bool {
    match (eval_num(a, vars), eval_num(b, vars)) {
        (Some(x), Some(y)) => op(x, y),
        _ => false,
    }
}

// ===========================================================================
// Solver-адаптер — ЗАГОТОВКА за feature = "smt" (12.3 полный)
// ===========================================================================

/// Результат check-sat.
#[cfg(feature = "smt")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SatResult {
    /// Формула выполнима (solver дал модель).
    Sat,
    /// Формула невыполнима (ветка недостижима — D4).
    Unsat,
    /// Решатель не определился (таймаут, неподдержанная теория).
    Unknown,
}

/// Трейт solver-адаптера (заготовка D4–D5). Реализуется поверх
/// Z3/CVC5 в полном 12.3; term-builder и кросс-валидация от него НЕ
/// зависят (см. шапку модуля).
///
/// # Как подключить Z3/CVC5 (12.3 полный)
///
/// 1. SMT-LIB v2 скрипт — термы IR печатаются 1:1:
/// ```text
/// (declare-const is_null_500 Bool)
/// (declare-const num_500 Real)
/// (assert (=> is_null_500 (= num_500 0.0)))   ; §3.1 инвариант
/// ; Formula: And→(and …), Or→(or …), Not→(not …),
/// ; IsNull(i)→is_null_i, EqNum(a,b)→(= num_a num_b),
/// ; GtNum→(> …), LtNum→(< …), GeNum→(>= …), LeNum→(<= …)
/// ; Div: side-condition num_b ≠ 0 — отдельный assert из PathConditions
/// (check-sat)
/// ```
/// 2. Две стратегии интеграции:
///    * subprocess (РЕКОМЕНДОВАНО): `std::process::Command::new("z3")
///      .arg("-in").arg("-smt2")` + pipe stdin/stdout — std-only, T2
///      («ни одной внешней зависимости») сохраняется даже под фичей;
///    * FFI-крейт (`z3` / cvc5 bindings): в Cargo.toml сделать
///      `smt = ["dep:z3"]` с `z3 = { version = "...", optional = true }`
///      — тяжёлая C-сборка уходит за фичу.
/// 3. `assumptions`: конъюнкцией с goal (или push/assert/pop на каждый
///    вызов — для инкрементального решателя).
/// 4. Допуск Macro B уже в СТРУКТУРЕ формул (`macro_eq_formula`
///    раскрывает `|x−y| <= 1e-9` в два `<=`-атома — см. шапку модуля),
///    поэтому принтер — чисто 1:1 в точные Real-операции, без особых
///    форм равенства; concrete-эвалюатор остаётся эталоном L2-семантики.
#[cfg(feature = "smt")]
pub trait SmtSolver {
    /// Проверить `goal ∧ assumptions` (могущество D4: недостижимость
    /// ветки = Unsat; конечность цикла = граница из Sat-модели).
    fn check(&mut self, goal: &Formula, assumptions: &[Formula]) -> SatResult;
}

/// Заглушка-адаптер (компилируемость трейта под фичей без solver-депа):
/// всегда `Unknown`. Заменяется реальным Z3/CVC5-адаптером в 12.3-full.
#[cfg(feature = "smt")]
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSolver;

#[cfg(feature = "smt")]
impl SmtSolver for NullSolver {
    fn check(&mut self, _goal: &Formula, _assumptions: &[Formula]) -> SatResult {
        SatResult::Unknown
    }
}

// ===========================================================================
// Тесты (БЕЗ фичи): пины инварианта/структуры + кросс-валидация
// формул против eval.rs-семантики на ТЕХ ЖЕ значениях
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{self, L2Error};
    use eid_core::ast::Num;
    use std::collections::BTreeMap;

    fn n(v: f64) -> Val {
        Val::Num(v)
    }

    /// Карта значений переменных для concrete-эвалюатора.
    type Vars = BTreeMap<VarId, SmtVal>;

    fn vars1(a: Val, b: Val) -> Vars {
        Vars::from([(1, SmtVal::from_val(a)), (2, SmtVal::from_val(b))])
    }

    fn lookup<'m>(vars: &'m Vars) -> impl Fn(VarId) -> SmtVal + 'm {
        move |i: VarId| vars.get(&i).copied().unwrap_or(SmtVal::NULL)
    }

    // ---------- §3.1: пины аксиомы нормализации и инварианта ---------------

    #[test]
    fn smtval_from_val_axiom_pin() {
        // §3.1 дословно: Null → (true, 0.0); Const(c) → (false, c).
        assert_eq!(SmtVal::from_val(Val::Null), SmtVal::NULL);
        assert!(SmtVal::from_val(Val::Null).is_null());
        assert_eq!(SmtVal::from_val(Val::Null).num_value(), 0.0);
        assert_eq!(SmtVal::from_val(n(5.0)), SmtVal::new(false, 5.0));
        // ПИН главное отличие #0: Num(0.0) — НЕ Null: (false, 0.0) валидно.
        let z = SmtVal::from_val(n(0.0));
        assert!(!z.is_null());
        assert_eq!(z.num_value(), 0.0);
    }

    #[test]
    #[should_panic(expected = "SmtVal invariant violated")]
    fn smtval_invariant_violation_panics() {
        // Инвариант is_null ⇒ num == 0.0 enforced конструктором.
        let _ = SmtVal::new(true, 5.0);
    }

    #[test]
    #[should_panic(expected = "SmtVal invariant violated")]
    fn smtval_invariant_violation_tiny_panics() {
        // Даже 1e-9 при is_null — нарушение (только 0.0 валиден).
        let _ = SmtVal::new(true, 1e-9);
    }

    #[test]
    fn smtval_invariant_valid_combinations() {
        // (false, 0.0) валидно (Num(0.0)); (true, 0.0) валидно (Null);
        // -0.0 == 0.0 — инвариант не нарушен (канонизация не требуется:
        // обе компоненты concrete-зеркала, бит-паттерн фиксирует eval.rs).
        SmtVal::new(false, 0.0);
        SmtVal::new(true, 0.0);
        SmtVal::new(true, -0.0);
        SmtVal::new(false, f64::MIN);
    }

    #[test]
    fn smtval_resolve_word_pin() {
        // Таблица 2.1 / строка G0 §3.2, concrete-сторона.
        assert_eq!(SmtVal::from_val(Val::Null).resolve_word(), None);
        assert_eq!(
            SmtVal::from_val(n(0.0)).resolve_word(),
            Some(0.0),
            "0 — НЕ Null: слово X0 остаётся в кадре"
        );
        assert_eq!(SmtVal::from_val(n(12.5)).resolve_word(), Some(12.5));
        // Зеркальность с eval.rs на тех же значениях.
        for v in [Val::Null, n(0.0), n(5.0), n(-3.5)] {
            assert_eq!(
                SmtVal::from_val(v).resolve_word(),
                v.resolve_word(),
                "resolve_word divergence for {v:?}"
            );
        }
    }

    // ---------- Term-IR: пины структуры формул §3.2 ------------------------

    #[test]
    fn eq_formula_verbatim_structure_pin() {
        // Строка EQ таблицы §3.2 на переменных: каркас дока
        // (is_null-дизъюнкция) + допуск Macro B 1e-9 эталона §4,
        // раскрытый структурно (см. шапку модуля).
        let a = ValTerm::var(1);
        let b = ValTerm::var(2);
        let expect = Formula::Or(vec![
            Formula::And(vec![Formula::IsNull(1), Formula::IsNull(2)]),
            Formula::And(vec![
                Formula::Not(Box::new(Formula::IsNull(1))),
                Formula::Not(Box::new(Formula::IsNull(2))),
                Formula::And(vec![
                    Formula::LeNum(
                        NumExpr::mk_sub(NumExpr::var(1), NumExpr::var(2)),
                        NumExpr::Const(1e-9),
                    ),
                    Formula::LeNum(
                        NumExpr::mk_sub(NumExpr::var(2), NumExpr::var(1)),
                        NumExpr::Const(1e-9),
                    ),
                ]),
            ]),
        ]);
        assert_eq!(macro_eq_formula(&a, &b), expect);
    }

    #[test]
    fn num_nonzero_structure_pin() {
        // Side-condition деления / truthiness: Not(EqNum(e, 0)).
        assert_eq!(
            Formula::num_nonzero(NumExpr::var(2)),
            Formula::Not(Box::new(Formula::EqNum(
                NumExpr::Var(2),
                NumExpr::Const(0.0)
            )))
        );
    }

    #[test]
    fn valterm_leaves_pin() {
        // Кодировки-листья: Null, Const, Var (§3.1).
        assert_eq!(ValTerm::null().is_null, Formula::Const(true));
        assert_eq!(ValTerm::null().num, NumExpr::Const(0.0));
        assert_eq!(
            ValTerm::num(NumExpr::Const(5.0)).is_null,
            Formula::Const(false)
        );
        assert_eq!(
            ValTerm::var(500),
            ValTerm {
                is_null: Formula::IsNull(500),
                num: NumExpr::Var(500),
            }
        );
        // Const-свёртка.
        assert_eq!(ValTerm::from_val(Val::Null), ValTerm::null());
        assert_eq!(ValTerm::from_val(n(7.25)).num, NumExpr::Const(7.25));
    }

    // ---------- Encoder: структура пути и границы --------------------------

    #[test]
    fn encoder_arith_terms_and_div_side_condition() {
        // #1 + #2 * #3: Add(Var1, Mul(Var2, Var3)); Div пушит assert.
        let e = Expr::Binary(
            BinaryOp::Add,
            Box::new(Expr::var(1)),
            Box::new(Expr::Binary(
                BinaryOp::Mul,
                Box::new(Expr::var(2)),
                Box::new(Expr::var(3)),
            )),
        );
        let mut enc = Encoder::new();
        let t = enc.encode_val(&e).expect("arith encodes");
        assert_eq!(t.is_null, Formula::Const(false), "арифметика всегда Num");
        assert_eq!(
            t.num,
            NumExpr::mk_add(
                NumExpr::var(1),
                NumExpr::mk_mul(NumExpr::var(2), NumExpr::var(3))
            )
        );
        assert!(enc.into_path().is_empty(), "без Div путь пуст");

        // Div: side-condition num_2 ≠ 0 в PathConditions (НЕ в терме).
        let d = Expr::Binary(
            BinaryOp::Div,
            Box::new(Expr::var(1)),
            Box::new(Expr::var(2)),
        );
        let mut enc = Encoder::new();
        let t = enc.encode_val(&d).expect("div encodes");
        assert_eq!(t.num, NumExpr::mk_div(NumExpr::var(1), NumExpr::var(2)));
        let path = enc.into_path();
        assert_eq!(path.len(), 1);
        assert_eq!(path.asserts[0], Formula::num_nonzero(NumExpr::var(2)));

        // Вложенный Div: (#1 / #2) / #3 → два assert'а.
        let d2 = Expr::Binary(BinaryOp::Div, Box::new(d), Box::new(Expr::var(3)));
        let mut enc = Encoder::new();
        let _ = enc.encode_val(&d2).expect("nested div encodes");
        let path = enc.into_path();
        assert_eq!(path.len(), 2);
        assert_eq!(path.asserts[0], Formula::num_nonzero(NumExpr::var(2)));
        assert_eq!(path.asserts[1], Formula::num_nonzero(NumExpr::var(3)));
    }

    #[test]
    fn encoder_honest_boundary_unsupported() {
        // Честная граница: всё вне таблицы §3.2 — EncodeError (не silent).
        let cases: Vec<(Expr, &str)> = vec![
            (
                Expr::Binary(
                    BinaryOp::Mod,
                    Box::new(Expr::var(1)),
                    Box::new(Expr::var(2)),
                ),
                "Mod",
            ),
            (
                Expr::Binary(
                    BinaryOp::And,
                    Box::new(Expr::var(1)),
                    Box::new(Expr::var(2)),
                ),
                "bitwise",
            ),
            (
                Expr::Binary(
                    BinaryOp::Atan2,
                    Box::new(Expr::var(1)),
                    Box::new(Expr::var(2)),
                ),
                "Atan2",
            ),
            (
                Expr::Unary(crate::ast::UnaryOp::Sin, Box::new(Expr::var(1))),
                "unary",
            ),
            (Expr::Var(VarRef::Named("depth".into())), "Named"),
            (
                Expr::Var(VarRef::Indirect(Box::new(Expr::var(100)))),
                "Indirect",
            ),
            (
                Expr::Var(VarRef::Sys(crate::ast::SysVar::WorkPos(
                    crate::ast::Axis::X,
                ))),
                "Sys",
            ),
            (
                // Сравнение как АРИФМЕТИЧЕСКИЙ операнд → нужен ite.
                Expr::Binary(
                    BinaryOp::Add,
                    Box::new(Expr::Binary(
                        BinaryOp::Eq,
                        Box::new(Expr::var(1)),
                        Box::new(Expr::var(2)),
                    )),
                    Box::new(Expr::var(3)),
                ),
                "ite",
            ),
        ];
        for (e, tag) in &cases {
            let mut enc = Encoder::new();
            let err = enc.encode_val(e).expect_err(tag);
            assert!(!err.reason.is_empty());
        }
    }

    #[test]
    fn encode_cond_truthiness_pin() {
        // IF [#0] → false; IF [#1] при #1 = Null → false; #1 = 5 → true;
        // #1 = 0 → false; #1 = -2 → true (eval.rs: as_num != 0).
        let cases: Vec<(Expr, Val, bool)> = vec![
            (Expr::Null, Val::Null, false),
            (Expr::var(1), Val::Null, false),
            (Expr::var(1), n(0.0), false),
            (Expr::var(1), n(5.0), true),
            (Expr::var(1), n(-2.0), true),
        ];
        for (e, v, expect) in cases {
            let (f, path) = encode_cond(&e).expect("cond encodes");
            let mut vars = Vars::new();
            vars.insert(1, SmtVal::from_val(v));
            let lk = lookup(&vars);
            assert!(path.is_satisfied(&lk));
            assert_eq!(eval_formula(&f, &lk), expect, "cond {e:?} with {v:?}");
            // Эталон eval.rs — truthy (env: #1 := v).
            let mut env = crate::eval::L2Env::new();
            env.write_direct(1, v).unwrap();
            let l2 = eval::eval_expr(&e, &mut env).unwrap();
            assert_eq!(l2, v);
            assert_eq!(l2.truthy(), expect);
        }
    }

    #[test]
    fn missing_var_reads_as_null_pin() {
        // Отсутствующая переменная в concrete-вычислении — Null
        // (как L2Env: «отсутствующие читаются как Null»).
        let empty = Vars::new();
        let lk = lookup(&empty);
        assert!(eval_formula(&Formula::IsNull(42), &lk));
        assert_eq!(eval_num(&NumExpr::var(42), &lk), Some(0.0));
        // IF [#42] → false.
        let e = Expr::var(42);
        let (f, _) = encode_cond(&e).unwrap();
        assert!(!eval_formula(&f, &lk));
    }

    #[test]
    fn path_conditions_pin() {
        // Пустой путь — истинен; конъюнкция — по всем assert'ам.
        let mut path = PathConditions::new();
        assert!(path.is_empty());
        assert_eq!(path.clone().into_formula(), Formula::Const(true));
        let vars = vars1(n(5.0), n(2.5));
        let lk = lookup(&vars);
        assert!(path.is_satisfied(&lk));
        path.push(Formula::num_nonzero(NumExpr::var(2)));
        assert_eq!(path.len(), 1);
        assert!(path.is_satisfied(&lk), "num_2 = 2.5 ≠ 0");
        path.push(Formula::num_nonzero(NumExpr::var(1)));
        assert!(path.is_satisfied(&lk));
        path.push(Formula::num_nonzero(NumExpr::var(9)));
        assert!(
            !path.is_satisfied(&lk),
            "num_9 отсутствует → Null → 0.0 → assert нарушен"
        );
        // into_formula: 1 assert → сам assert; 3 → And.
        let three = path.clone().into_formula();
        assert_eq!(
            three,
            Formula::And(vec![
                Formula::num_nonzero(NumExpr::var(2)),
                Formula::num_nonzero(NumExpr::var(1)),
                Formula::num_nonzero(NumExpr::var(9)),
            ])
        );
        let mut one = PathConditions::new();
        one.push(Formula::num_nonzero(NumExpr::var(2)));
        assert_eq!(one.into_formula(), Formula::num_nonzero(NumExpr::var(2)));
    }

    // ---------- КРОСС-ВАЛИДАЦИЯ (главная ценность 12.3 без solver) --------
    //
    // Протокол: на одних и тех же значениях (#1 := a, #2 := b)
    //   1) L2-эталон: eval::eval_binary(op, a, b);
    //   2) SMT-сторона: Encoder кодирует Expr::Binary(op, #1, #2);
    //      formula-канал — для сравнений, num-канал — для арифметики;
    //      side-condition деления должен быть нарушен ТОЧНО тогда, когда
    //      L2 вернул DivisionByZero.
    // Расхождение = разное bool/число/ошибка.

    /// Кросс-проверка одного кейса: Ok(()) — расхождений нет.
    fn cross_check(op: BinaryOp, a: Val, b: Val) -> Result<(), String> {
        let l2 = eval::eval_binary(op, &a, &b);
        let expr = Expr::Binary(op, Box::new(Expr::var(1)), Box::new(Expr::var(2)));

        let vars = vars1(a, b);
        let lk = lookup(&vars);

        // 1) formula-канал (условия IF/WHILE — сравнения и truthiness).
        let mut enc = Encoder::new();
        let f = enc
            .encode_formula(&expr)
            .map_err(|e| format!("encode: {e}"))?;
        let path = enc.into_path();

        match l2 {
            Ok(Val::Num(res)) => {
                if !path.is_satisfied(&lk) {
                    return Err(format!(
                        "{op:?} {a:?} {b:?}: side-condition нарушен, но L2 дал Ok({res:?})"
                    ));
                }
                let smt = eval_formula(&f, &lk);
                if smt != Val::Num(res).truthy() {
                    return Err(format!(
                        "{op:?} {a:?} {b:?}: formula {smt} != L2 truthy(Num({res}))"
                    ));
                }
                // 2) num-канал для арифметики: точное совпадение f64
                // (обе стороны — одна и та же f64-операция над теми же
                // значениями; бит-в-бит).
                if matches!(
                    op,
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div
                ) {
                    let mut enc2 = Encoder::new();
                    let t = enc2.encode_val(&expr).map_err(|e| format!("encode: {e}"))?;
                    let _path2 = enc2.into_path();
                    match eval_num(&t.num, &lk) {
                        Some(v) if v == res => {}
                        Some(v) => {
                            return Err(format!("{op:?} {a:?} {b:?}: num {v} != L2 Num({res})"))
                        }
                        None => {
                            return Err(format!(
                                "{op:?} {a:?} {b:?}: num None при выполненном side-condition"
                            ))
                        }
                    }
                }
                Ok(())
            }
            Err(L2Error::DivisionByZero) => {
                if path.is_satisfied(&lk) {
                    Err(format!(
                        "{op:?} {a:?} {b:?}: L2 DivisionByZero, но side-condition выполнен"
                    ))
                } else {
                    Ok(())
                }
            }
            Err(e) => Err(format!("{op:?} {a:?} {b:?}: неожиданная ошибка L2 {e:?}")),
            // eval_binary всегда возвращает Num — arm для исчерпательности.
            Ok(Val::Null) => Err(format!("{op:?} {a:?} {b:?}: L2 вернул Null (impossible)")),
        }
    }

    /// Детерминированный пул значений: Null-всеварианты, 0, границы,
    /// отрицательные, допуск-окрестность 1e-9 (Eq Macro B).
    fn val_pool() -> Vec<Val> {
        vec![
            Val::Null,    // vacant
            n(0.0),       // Num-ноль (главное отличие от Null!)
            n(1.0),       // единица
            n(-1.0),      // отрицательная единица
            n(5.0),       // положительное
            n(-3.5),      // отрицательное дробное
            n(0.5),       // дробь
            n(-0.25),     // отрицательная дробь
            n(100.0),     // крупное
            n(0.1 + 0.2), // 0.30000000000000004 — кейс допуска 1e-9
            n(0.3),       // напарник предыдущего
            n(1e-9),      // граница допуска macro_eq (<= 1e-9 → true)
            n(2e-9),      // за границей допуска от 0
            n(-1e-9),     // отрицательная граница
            n(1234.5),    // «координата»
        ]
    }

    fn all_ops() -> Vec<BinaryOp> {
        vec![
            BinaryOp::Add,
            BinaryOp::Sub,
            BinaryOp::Mul,
            BinaryOp::Div,
            BinaryOp::Eq,
            BinaryOp::Ne,
            BinaryOp::Gt,
            BinaryOp::Lt,
            BinaryOp::Ge,
            BinaryOp::Le,
        ]
    }

    #[test]
    fn cross_validation_hand_table() {
        // Таблица §3.2 × таблицы Приложения 2.2–2.3: Null-всеварианты на
        // каждом операторе + Num-случаи + границы 0/отрицательные/допуск.
        let n5 = n(5.0);
        let cases: Vec<(BinaryOp, Val, Val)> = vec![
            // Арифметика с Null (таблица 2.2: Null → 0.0, результат Num).
            (BinaryOp::Add, Val::Null, Val::Null),
            (BinaryOp::Add, Val::Null, n5),
            (BinaryOp::Add, n5, Val::Null),
            (BinaryOp::Add, Val::Null, n(-3.5)),
            (BinaryOp::Sub, Val::Null, Val::Null),
            (BinaryOp::Sub, Val::Null, n5),
            (BinaryOp::Sub, n5, Val::Null),
            (BinaryOp::Mul, Val::Null, Val::Null),
            (BinaryOp::Mul, Val::Null, n5),
            (BinaryOp::Mul, n(-7.0), Val::Null),
            (BinaryOp::Div, Val::Null, n5),
            (BinaryOp::Div, n5, Val::Null),        // → DivisionByZero
            (BinaryOp::Div, Val::Null, Val::Null), // → DivisionByZero
            (BinaryOp::Div, n(7.0), n(2.0)),
            (BinaryOp::Div, n(-7.0), n(-2.0)),
            (BinaryOp::Div, n5, n(0.0)), // → DivisionByZero
            (BinaryOp::Div, n(0.0), n5),
            // EQ/NE — Null равен ТОЛЬКО Null (главное отличие!).
            (BinaryOp::Eq, Val::Null, Val::Null),
            (BinaryOp::Eq, Val::Null, n(0.0)), // false — Null ≠ 0!
            (BinaryOp::Eq, n(0.0), Val::Null), // false
            (BinaryOp::Eq, Val::Null, n5),     // false
            (BinaryOp::Eq, n5, Val::Null),     // false
            (BinaryOp::Eq, n(2.0), n(2.0)),
            (BinaryOp::Eq, n(2.0), n(3.0)),
            (BinaryOp::Eq, n(0.1 + 0.2), n(0.3)), // допуск 1e-9 → true
            (BinaryOp::Eq, n(1.0), n(1.001)),     // вне допуска → false
            (BinaryOp::Ne, Val::Null, Val::Null), // false
            (BinaryOp::Ne, Val::Null, n(0.0)),    // true — Null ≠ 0!
            (BinaryOp::Ne, n(2.0), n(3.0)),
            (BinaryOp::Ne, n(3.0), n(3.0)),
            // GT/LT/GE/LE — Null → 0.0 (таблица 2.3).
            (BinaryOp::Gt, Val::Null, Val::Null), // 0 > 0 → false
            (BinaryOp::Gt, Val::Null, n(1.0)),    // 0 > 1 → false
            (BinaryOp::Gt, n(1.0), Val::Null),    // 1 > 0 → true
            (BinaryOp::Gt, Val::Null, n(-1.0)),   // 0 > -1 → true
            (BinaryOp::Lt, Val::Null, n(1.0)),    // 0 < 1 → true
            (BinaryOp::Lt, Val::Null, n(-1.0)),   // 0 < -1 → false
            (BinaryOp::Lt, n(-1.0), Val::Null),   // -1 < 0 → true
            (BinaryOp::Ge, Val::Null, Val::Null), // 0 >= 0 → true
            (BinaryOp::Ge, Val::Null, n(0.0)),    // 0 >= 0 → true (Fanuc!)
            (BinaryOp::Ge, Val::Null, n(0.0001)), // false
            (BinaryOp::Ge, n(5.0), n(5.0)),       // true
            (BinaryOp::Le, Val::Null, Val::Null), // 0 <= 0 → true
            (BinaryOp::Le, Val::Null, n(0.0)),    // true (Fanuc!)
            (BinaryOp::Le, Val::Null, n(-1.0)),   // 0 <= -1 → false
            (BinaryOp::Le, n(4.0), n(5.0)),       // true
        ];
        assert!(
            cases.len() >= 20,
            "рукописная таблица должна покрывать >= 20 кейсов (сейчас {})",
            cases.len()
        );
        let mut divergences = Vec::new();
        for (op, a, b) in &cases {
            if let Err(d) = cross_check(*op, *a, *b) {
                divergences.push(d);
            }
        }
        assert!(
            divergences.is_empty(),
            "кросс-валидация: {} расхождений формул с eval.rs: {divergences:#?}",
            divergences.len()
        );
    }

    #[test]
    fn cross_validation_generated_sweep() {
        // Генерируемый свип: детерминированный LCG (без внешнего rand —
        // std-only), все 10 операторов × пул из 15 значений. Цель —
        // 0 расхождений с eval.rs на N кейсов.
        let pool = val_pool();
        let ops = all_ops();
        let mut rng = Lcg(0x5EED_12C3_AFE1_C0DE);
        const N_CASES: usize = 400;
        let mut divergences = Vec::new();
        for _ in 0..N_CASES {
            let op = ops[(rng.next() % ops.len() as u64) as usize];
            let a = pool[(rng.next() % pool.len() as u64) as usize];
            let b = pool[(rng.next() % pool.len() as u64) as usize];
            if let Err(d) = cross_check(op, a, b) {
                divergences.push(d);
            }
        }
        assert!(
            divergences.is_empty(),
            "генерируемая кросс-валидация ({N_CASES} кейсов): {} расхождений: {divergences:#?}",
            divergences.len()
        );
    }

    /// Детерминированный LCG (xorshift-free, splittingMix64-подобный
    /// шаг): достаточно для свипа, без внешних крейтов.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 11
        }
    }

    #[test]
    fn div_side_condition_matches_l2_error_pin() {
        // Таблица §3.2 строка a/b + таблица 2.2: деление на Null/0 —
        // E_INVALID_INPUT; side-condition нарушен ТОЧНО тогда же.
        // 5 / Null → ошибка; side-cond num_2 ≠ 0 нарушен (Null → 0.0).
        let err = eval::eval_binary(BinaryOp::Div, &n(5.0), &Val::Null);
        assert_eq!(err, Err(L2Error::DivisionByZero));
        let (t, path) = div_term();
        let vars = vars1(n(5.0), Val::Null);
        let lk = lookup(&vars);
        assert_eq!(t.num, NumExpr::mk_div(NumExpr::var(1), NumExpr::var(2)));
        assert!(!path.is_satisfied(&lk), "делитель Null → num = 0.0");
        assert_eq!(eval_num(&t.num, &lk), None, "мёртвый путь → None");

        // 5 / 2.5 → Ok(2.0); side-cond выполнен.
        assert_eq!(
            eval::eval_binary(BinaryOp::Div, &n(5.0), &n(2.5)),
            Ok(n(2.0))
        );
        let vars = vars1(n(5.0), n(2.5));
        let lk = lookup(&vars);
        let (t, path) = div_term();
        let _ = t;
        assert!(path.is_satisfied(&lk));
        assert_eq!(
            eval_num(&NumExpr::mk_div(NumExpr::var(1), NumExpr::var(2)), &lk),
            Some(2.0)
        );

        // Null / 5 → Ok(0.0): Null-делимое валидно (0.0 / 5).
        assert_eq!(
            eval::eval_binary(BinaryOp::Div, &Val::Null, &n5_helper()),
            Ok(n(0.0))
        );
        let vars = vars1(Val::Null, n5_helper());
        let lk = lookup(&vars);
        let (_, path) = div_term();
        assert!(path.is_satisfied(&lk));
        assert_eq!(
            eval_num(&NumExpr::mk_div(NumExpr::var(1), NumExpr::var(2)), &lk),
            Some(0.0)
        );
    }

    fn n5_helper() -> Val {
        n(5.0)
    }

    /// #1 / #2 — терм + путь (общий хелпер div-тестов).
    fn div_term() -> (ValTerm, PathConditions) {
        let mut enc = Encoder::new();
        let t = enc
            .encode_val(&Expr::Binary(
                BinaryOp::Div,
                Box::new(Expr::var(1)),
                Box::new(Expr::var(2)),
            ))
            .expect("div encodes");
        (t, enc.into_path())
    }

    #[test]
    fn g0_word_inheritance_pin() {
        // Строка G0 таблицы §3.2: G0 X#1 — is_null(#1) → слово
        // игнорируется (координата наследуется от prev_end.x); иначе
        // → num_1. Эталон — Val::resolve_word (eval.rs), которым
        // пользуется interp::pose_resolve; 0 — НЕ Null (слово X0
        // остаётся!). Формульная сторона: is_null-формула терма.
        let e = Expr::var(1);
        let mut enc = Encoder::new();
        let term = enc.encode_val(&e).expect("word expr encodes");
        let path = enc.into_path();
        assert!(path.is_empty(), "G0-кодирование без Div — путь пуст");

        let cases = vec![
            (Val::Null, None),   // наследовать prev_end.x
            (n(0.0), Some(0.0)), // критичное отличие: X0 остаётся
            (n(5.0), Some(5.0)),
            (n(-7.25), Some(-7.25)),
            (n(1e-9), Some(1e-9)),
        ];
        for (v, expect) in cases {
            let vars = vars1(v, Val::Null);
            let lk = lookup(&vars);
            // Формульная сторона: условие наследования = is_null-формула.
            let inherit = eval_formula(&term.is_null, &lk);
            // Concrete-сторона: resolve_word.
            let word = eval_num(&term.num, &lk).and_then(|_| SmtVal::from_val(v).resolve_word());
            assert_eq!(inherit, v.is_null(), "is_null-формула для {v:?}");
            assert_eq!(word, expect, "G0 X#1 при #1 = {v:?}");
            // Эталон eval.rs — resolve_word на том же значении.
            assert_eq!(word, v.resolve_word());
        }
    }

    #[test]
    fn g0_word_with_expr_operand_pin() {
        // G0 X[#1 + #2]: кодирование работает поверх произвольного
        // выражения; Null-операнд (#1 = Null, #2 = 5) → X-слово
        // игнорируется (арифметика бы дала 5.0 — но слово решается
        // по is_null ВЫРАЖЕНИЯ... стоп: таблица 2.1/§3.2 — слово
        // игнорируется, когда ЗНАЧЕНИЕ выражения — Null; арифметика
        // снимает Null → Num(5.0). Проверяем согласованность с eval.rs.)
        let e = Expr::Binary(
            BinaryOp::Add,
            Box::new(Expr::var(1)),
            Box::new(Expr::var(2)),
        );
        let mut enc = Encoder::new();
        let term = enc.encode_val(&e).expect("add encodes");
        let _ = enc.into_path();
        // Арифметика всегда Num: is_null ≡ false — слово НЕ наследуется.
        assert_eq!(term.is_null, Formula::Const(false));
        // Эталон: eval_expr даёт Num(5.0) → слово Some(5.0).
        let mut env = crate::eval::L2Env::new();
        env.write_direct(1, Val::Null).unwrap();
        env.write_direct(2, n(5.0)).unwrap();
        let v = eval::eval_expr(&e, &mut env).unwrap();
        assert_eq!(v, n(5.0));
        assert_eq!(v.resolve_word(), Some(5.0));
        // И формула is_null на тех же значениях → false.
        let vars = vars1(Val::Null, n(5.0));
        let lk = lookup(&vars);
        assert!(!eval_formula(&term.is_null, &lk));
        assert_eq!(eval_num(&term.num, &lk), Some(5.0));
    }

    // ---------- §3.2 формула EQ против eval.rs — точечные пины -------------

    #[test]
    fn eq_formula_null_strictness_pin() {
        // Формула EQ дока воспроизводит тройку T3 дока:
        // Null EQ Null → true; Null EQ 0 → false; Num EQ Num — допуск.
        let cases = [
            (Val::Null, Val::Null, true),
            (Val::Null, n(0.0), false),
            (n(0.0), Val::Null, false),
            (Val::Null, n(5.0), false),
            (n(2.0), n(2.0), true),
            (n(0.1 + 0.2), n(0.3), true),
        ];
        for (a, b, expect) in cases {
            let mut enc = Encoder::new();
            let f = enc
                .encode_formula(&Expr::Binary(
                    BinaryOp::Eq,
                    Box::new(Expr::var(1)),
                    Box::new(Expr::var(2)),
                ))
                .expect("eq encodes");
            let vars = vars1(a, b);
            let lk = lookup(&vars);
            assert_eq!(eval_formula(&f, &lk), expect, "EQ {a:?} {b:?}");
            // Эталон.
            assert_eq!(
                a.macro_eq(b),
                expect,
                "eval.rs macro_eq расходится с формулой дока"
            );
        }
    }

    #[test]
    fn ne_formula_is_not_eq_pin() {
        // NE → Not(EQ) — и на Null-паре, и на числах.
        for (a, b) in [
            (Val::Null, Val::Null),
            (Val::Null, n(0.0)),
            (n(1.0), n(2.0)),
        ] {
            let mut enc = Encoder::new();
            let f = enc
                .encode_formula(&Expr::Binary(
                    BinaryOp::Ne,
                    Box::new(Expr::var(1)),
                    Box::new(Expr::var(2)),
                ))
                .expect("ne encodes");
            let vars = vars1(a, b);
            let lk = lookup(&vars);
            assert_eq!(eval_formula(&f, &lk), a.macro_ne(b), "NE {a:?} {b:?}");
            // Структура: Not поверх macro_eq_formula.
            let expect_not = matches!(f, Formula::Not(_));
            assert!(expect_not);
        }
    }

    #[test]
    fn const_folding_through_encoder() {
        // Чисто константное выражение (без переменных): кодирование и
        // concrete-вычисление совпадает с eval.rs напрямую (D3-свёртка).
        let e = Expr::Binary(
            BinaryOp::Add,
            Box::new(Expr::Const(Num::new("2.5".into(), 2.5))),
            Box::new(Expr::Null),
        );
        // Эталон.
        let mut env = crate::eval::L2Env::new();
        let l2 = eval::eval_expr(&e, &mut env).unwrap();
        assert_eq!(l2, n(2.5));
        // SMT-сторона.
        let mut enc = Encoder::new();
        let t = enc.encode_val(&e).expect("const add encodes");
        assert!(enc.into_path().is_empty());
        let empty = Vars::new();
        let lk = lookup(&empty);
        assert_eq!(eval_num(&t.num, &lk), Some(2.5));
        assert_eq!(eval_formula(&t.is_null, &lk), false);
    }
}

// ===========================================================================
// Тесты ЗА фичей smt: solver-trait заготовка
// ===========================================================================

#[cfg(all(test, feature = "smt"))]
mod solver_tests {
    use super::*;

    #[test]
    fn null_solver_returns_unknown() {
        let mut s = NullSolver;
        let goal = Formula::num_nonzero(NumExpr::var(500));
        let assumptions = vec![Formula::IsNull(500)];
        assert_eq!(s.check(&goal, &assumptions), SatResult::Unknown);
        assert_eq!(s.check(&goal, &[]), SatResult::Unknown);
    }

    #[test]
    fn sat_result_copy_semantics() {
        let r = SatResult::Unsat;
        let r2 = r;
        assert_eq!(r, r2);
        assert_ne!(SatResult::Sat, SatResult::Unsat);
        assert_ne!(SatResult::Unknown, SatResult::Sat);
    }
}
