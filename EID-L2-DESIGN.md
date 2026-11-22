# EID-L2-DESIGN — дизайн-документ уровня L2 (параметрика, трансформации, 4/5 осей)

Версия: v0.1 (финальный дизайн-документ Этапа 12); амендмент v0.1.1 (2026-11-23)
Дата: 2026-11-23
Статус: дизайн-спецификация; код crates/eid-l2 не затронут.
Привязан к: SPEC-EID-L1 v0.21, EQUIVALENCE v0.3.1, COVERAGE v0.11, INTERFACE-V0.2-DESIGN v0.2.0.
Ключевой архитектурный принцип: «Оси в AST, переменные в lowering первыми» — единый L2 AST и парсер реализуются одним заходом (поддерживая синтаксис 5-осевых параметрических программ), а опускание в L1 (Progressive Lowering) разделяется на независимые пассы: сперва разрешение переменных и потока управления (+ SMT), а кинематический lowering ротационных осей откладывается на будущее.

## §0. Мотивация и границы ответственности

В SPEC-EID-L1 (§1.1, Q10) параметрика (Macro B, WHILE, G65), трансформации (G68, G51) и 4/5-осевая кинематика сознательно вынесены из L1, так как L1 — это статический плоский поток операций с разрешённым состоянием (resolved-state ops).
Однако реальные промышленные программы (особенно Fanuc и Haas) часто смешивают 5-осевые команды и параметрику в одном кадре (например, G0 A#100 или G1 X#101 Y#102 B#103).
- Если L2 AST не включает оси: парсер не сможет разобрать G0 A#100 и отправит весь файл в Raw до этапа вычисления переменных, делая «максимальную разрешимость» недостижимой.
- Если L2 AST включает оси одним заходом, а lowering разделён: парсер строит полный L2 AST, первый пасс lowering'а разрешает #100 → 5.0 и разворачивает циклы, а кадр G0 A5.0000 опускается в L1 как точечный Op::Raw (с уже подставленным значением). Same-dialect эмиссия воспроизводит G0 A5.0000, а кросс-диалектная трансляция честно отказывает с E_RAW_UNTRANSLATABLE на конкретном опе, а не на всём файле.

Trust Story (нулевые зависимости на основном пути)

- Крейт crates/eid-l2 (AST, парсер, интерпретатор L2, частичный lowering в L1) остаётся строго std-only (единственная зависимость — eid-core), сохраняя гарантии безопасности и воспроизводимости сборки.
- SMT-решатель (для доказательства разрешимости веток при символьных переменных) подключается строго через опциональный Cargo feature-gate (feature = "smt"), не затрагивая базовую сборку.

## §1. Архитектура пайплайна (Progressive Lowering)

## §2. Спецификация L2 AST (crates/eid-l2/src/ast.rs)

L2 AST проектируется одним заходом в полном объёме, чтобы избежать ломающих изменений структур данных в будущем.

### 2.1 Выражения (Expr) и семантика #0 (Null)

В Macro B значение #0 (vacant / null) имеет двойственную природу, критичную для ветвлений:
- В арифметике (#0 + 5, #0 * 10) #0 трактуется как 0.0.
- В сравнениях (EQ, NE) #0 не равно 0.0: #0 EQ #0 → true, но #0 EQ 0 → false.

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,      // #0 (vacant)
    Num(f64),  // f64-зеркало (округление до 4dp — только на границе L1/эмиссии, SPEC §10.5)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Const(Num),
    Null,                                  // #0
    Var(VarRef),                           // #100, #[#1 + 10], #<_x>
    Unary(UnaryOp, Box<Expr>),             // Neg, Abs, Sin, Cos, Tan, Asin, Acos, Atan, Sqrt, Ln, Exp, Round, Fix, Fup
    Binary(BinaryOp, Box<Expr>, Box<Expr>), // Add, Sub, Mul, Div, Mod, And, Or, Xor, Eq, Ne, Gt, Lt, Ge, Le
}

#[derive(Debug, Clone, PartialEq)]
pub enum VarRef {
    Direct(u32),             // #100
    Indirect(Box<Expr>),     // #[#100 + 5]
    Named(String),           // #<my_var> (LinuxCNC)
    Sys(SysVar),             // Каноническая системная переменная
}
```

**v0.1.1 амендмент (корпус-ценз, corpus/tier_b `_l2_targets.md` §6.3):** словари
§2.1 дополняются двумя операциями — `UnaryOp::Exists` (двухаргументная форма не
имеется: `EXISTS[#<var>]` → Num(1.0/0.0), оценивается без вычисления аргумента;
мишень g881) и `BinaryOp::Atan2` (двухаргументный `ATAN[y]/[x]`, результат в
градусах; мишень ngcgui_lib_arc2). Одноаргументный ATAN остаётся UnaryOp::Atan.

### 2.2 Канонические системные переменные (SysVar)

Номера системных переменных различаются между контроллерами (например, позиции: Fanuc/Haas #5021, LinuxCNC #5420; смещения: Fanuc #5221 vs Haas #7001). При парсинге они переводятся в единый диалект-независимый enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X, Y, Z, A, B, C, U, V, W,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysVar {
    /// Текущие координаты (E_RUNTIME_STATE при неизвестном M0 → блокируют lowering)
    AbsMachinePos(Axis),             // Fanuc #5021..#5026
    WorkPos(Axis),                   // Fanuc #5041..#5046
    SkipPos(Axis),                   // Fanuc #5061..#5066 (результат G31)
    /// Модальные группы (разрешимы статически при отсутствии нераскрытых Raw)
    ModalGroup(u8),                  // Fanuc #4001..#4030
    /// Таблицы смещений WCS (зависимость от станка)
    WcsOffset { p: u8, axis: Axis }, // G54–G59 (#5221+), доп. WCS (#7001+)
    /// Вендорская переменная без кросс-диалектного аналога
    VendorSpecific(u32),
}
```

### 2.3 Многоосевая геометрия (ExprPose) и трансформации (TransformKind)

Включаются в AST с первого дня, чтобы парсер корректно разбирал 5-осевые параметрические кадры.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ExprPose {
    pub x: Option<Expr>,
    pub y: Option<Expr>,
    pub z: Option<Expr>,
    pub a: Option<Expr>,
    pub b: Option<Expr>,
    pub c: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransformKind {
    Rotate2D { center: [Expr; 2], angle_deg: Expr, plane: Plane }, // G68 X Y R
    Scale { center: [Expr; 3], factor: Expr },                      // G51 X Y Z P
    Mirror { axis: Axis, pivot: Expr },                             // G51.1 X0 / Haas G101
    TiltedWorkPlane { origin: [Expr; 3], euler: [Expr; 3] },        // G68.2 (3+2)
}
```

### 2.4 Операторы и поток управления (L2Stmt)

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum L2Stmt {
    Move {
        kind: MoveKind, // Rapid, Linear, Arc { ijk: [Option<Expr>; 3], r: Option<Expr>, dir: Dir }
        target: ExprPose,
        feed: Option<Expr>,
        spindle: Option<Expr>,
        src: SrcSpan,
    },
    WithTransform {
        transform: TransformKind,
        body: Vec<L2Stmt>,
        src: SrcSpan,
    },
    Assign { target: VarRef, value: Expr, src: SrcSpan },
    IfGoto { cond: Expr, target_label: u32, src: SrcSpan },
    IfThen { cond: Expr, then_stmt: Box<L2Stmt>, src: SrcSpan },
    While {
        cond: Expr,
        do_id: u32, // DO1..DO3
        body: Vec<L2Stmt>,
        src: SrcSpan,
    },
    CallSub {
        target: SubTarget,     // O-номер (Fanuc/Haas) или N-метка (Haas M97)
        repeat: Option<Expr>,  // L / K
        args: Vec<(u8, Expr)>, // A->#1 .. Z->#26 (G65)
        modal: bool,           // G66 (после каждого motion) vs G65 / M98
        src: SrcSpan,
    },
    /// Узлы 5-осевой кинематики (в Pass 1 опускаются как Op::Raw)
    RtcpMode { enabled: bool, tool_length_h: Option<u32>, src: SrcSpan }, // G43.4 / G49
    L1Op(eid_core::ast::Op),
}
```

## §3. Матрица частичного lowering'а (L2 → L1, Pass 1)

Pass 1 (lower_variables_to_l1) выполняет символьно-конкретную интерпретацию с лимитом шагов (max_eval_steps) и формирует eid_core::ast::Program:

| Конструкция L2 | Поведение в Pass 1 (L2 → L1) | Результат в L1 AST |
|---|---|---|
| #100 = 5, G0 X#100 | Вычисление Expr (single-final-rounding 4dp) | Op::Rapid (X5.0000) |
| WHILE, IF/GOTO, G65 (статические) | Развёртка (unroll/inline) со стеком #1–#33 | Плоская последовательность L1-опов |
| G68 (поворот), G51 (масштаб) | Умножение координат и центров дуг на матрицу | Op::Linear / Op::Arc в базовой СК |
| G51.1 / G101 (зеркало) | Меняет climb ↔ conventional: по умолчанию отказ (E_UNSUPPORTED_SEMANTICS), при profile.allow_mirror_bake = true — инверсия координат и Cw ↔ Ccw | Op::Raw (дефолт) или L1-опы + Notice |
| G0 A#100 (при #100 = 5) | Частичный lowering: #100 вычисляется в 5.0000, но ось A не поддерживается L1 | Op::Raw с payload G0 A5.0000\n (E_UNSUPPORTED_SEMANTICS) |
| G43.4 (RTCP), G68.2 (TWP) | Кинематика в Pass 1 не опускается | Op::Raw (с подставленными переменными, если были) |
| G31 / чтение #5061 (skip) | Зависит от состояния станка в рантайме | Остановка lowering'а в L1 (E_RUNTIME_STATE), программа остаётся в L2 |

**Инвариант безопасности частичного lowering'а**

Если кадр с ротационной осью (G0 A#100) опущен как Op::Raw с подставленным значением G0 A5.0000:
1. Same-dialect round-trip (Fanuc → Fanuc): работает корректно, эмиттер выводит G0 A5.0000 (без зависимости от #100).
2. Cross-dialect (Fanuc → GRBL): конвейер видит Op::Raw и гарантированно отказывает с E_RAW_UNTRANSLATABLE (SPEC §8.2), не допуская молчаливой потери поворота оси A.

## §4. Уровни разрешимости и роль SMT (D0–D5)

Для достижения максимальной разрешимости переменных при наличии неизвестных входных параметров (например, глобальных настроек #500–#599 или входных аргументов макроса) применяется поэтапная модель разрешимости:
- D0–D2 (Константная свёртка и распространение): Вычисление арифметики (#100 = 10 + 5), локальных кадров G65 и детерминированных циклов WHILE.
- D3 (Символьное упрощение и мёртвые ветки): Устранение тривиальных условий (IF [1 EQ 1] GOTO 10) и неиспользуемых присваиваний.
- D4–D5 (SMT-решатель под feature = "smt"):
  - Зачем нужен: Когда ветвление IF [#500 GT 0] GOTO 100 или условие выхода из WHILE зависит от неинициализированной глобальной переменной #500, обычный интерпретатор останавливается (E_RUNTIME_STATE). SMT-решатель позволяет:
    1. Доказать недостижимость ветки (например, IF [#500 * #500 LT 0] всегда ложно → ветка отбрасывается без E_RUNTIME_STATE).
    2. Доказать конечность цикла WHILE и вывести верхнюю границу итераций при ограниченных доменах из MachineProfile.
  - Кодирование #0 (Null) в SMT: Каждая переменная #i кодируется в SMT как пара (is_null_i: Bool, val_i: Real):
    - В арифметических термах используется ite(is_null_i, 0.0, val_i).
    - В предикате EQ(a, b) используется (is_null_a && is_null_b) || (!is_null_a && !is_null_b && val_a == val_b). Это исключает ложные доказательства при проверках IF [#1 EQ #0].

## §5. План реализации Этапа 12 (Поэтапный роадмап)

| Фаза | Содержание | Объём и зависимости | Критерий приёмки (DoD) |
|---|---|---|---|
| 12.1. L2 AST & Front-End (Одним заходом) | Создание crates/eid-l2 (std-only). Полный AST (Expr, Val::Null, SysVar, ExprPose{a,b,c}, WithTransform, RtcpMode, L2Stmt) и парсеры L2 для Fanuc, Haas и LinuxCNC. | eid-core (read-only). | Парсинг всех 26 отложенных файлов Tier B (Macro B, G68/G51, 4/5-axis) в L2 AST без паник; 100% покрытие продукции Expr и #0-семантики. |
| 12.2. Variable & Control-Flow Lowering (D0–D3) | Реализация L2Evaluator: стек вызовов G65/M98/M97 (глубина до 9), WHILE/DO/END, CFG-исполнитель для IF/GOTO, маппинг SysVar и аффинные трансформации G68/G51 (+ G51.1 по флагу профиля). Частичный lowering G0 A#100 → Op::Raw. | Только eid-core. | Trace-эквивалентность (semantic_diff == 0) развёрнутых макросов против эталонного rs274 (LinuxCNC) и golden-векторов. |
| 12.3. SMT-верификация и разрешимость (D4–D5) | Интеграция SMT-бэкенда (feature = "smt"), кодирование пары (is_null, val), доказательство недостижимости веток и ограничений циклов при символьных #500+. | Опциональный crate солвера за feature-gate. | Устранение ложных E_RUNTIME_STATE на параметрических шаблонах с необязательными аргументами (#0) и инвариантами. |
| 12.4. L2-to-L2 Dialect Emit | Прямая трансляция неразворачиваемых L2-программ (с G31 probing или сохранением параметрики) между Fanuc ↔ Haas ↔ LinuxCNC (включая трансляцию GOTO в структурные o-if/o-while для LinuxCNC). | eid-fanuc, eid-haas, eid-linuxcnc. | Валидный round-trip L2-программ с G31 и таблицами маппинга SysVar между Fanuc и Haas. |
| 12.5. Kinematic Axis Lowering (Future) | Добавление MachineKinematics в MachineProfile (оси вращения, векторы смещения стола/головы) и пасс пересчёта Pose6 / G43.4 RTCP / G68.2 TWP. | Отдельный 5-осевой симулятор-оракул. | Валидация 5-осевых траекторий на заданной кинематической схеме станка. |

## §6. Чек-лист приёмки L2 v1 (Фазы 12.1–12.3)

- T1. Zero-overhead для L1: Программы без L2-конструкций не тратят память на построение L2Program и проходят текущий конвейер с прежней скоростью и 1161+ зелёными тестами.
- T2. std-only базовый контур: cargo build -p eid-l2 (без --features smt) не тянет ни одной внешней зависимости сверх eid-core.
- T3. Семантика #0 (Null): Unit-тесты и SMT-тесты подтверждают Null + 5 == 5, Null == Null (true), Null == 0 (false) в точности как на контроллерах Fanuc/Haas.
- T4. Частичный lowering осей: Программа с #100 = 15 и G0 X#100 A#100 опускается в Op::Raw("G0 X15.0000 A15.0000\n") — same-dialect эмиссия проходит, кросс-диалект возвращает E_RAW_UNTRANSLATABLE.
- T5. Защита от зацикливания: Бесконечный WHILE [1 EQ 1] DO 1 детерминированно прерывается по max_eval_steps с кодом E_LIMIT_EXCEEDED.
- T6. Зеркалирование (G51.1 / G101): По умолчанию блокирует lowering (E_UNSUPPORTED_SEMANTICS), при allow_mirror_bake = true корректно инвертирует направление дуг (Cw ↔ Ccw) и выдаёт Notice.

## Приложение: полная спецификация семантики #0 (Null / vacant) в Fanuc Custom Macro B / Haas NGC

Ниже — полная спецификация семантики #0 (Null / vacant) в стандарте Fanuc Custom Macro B / Haas NGC для реализации в интерпретаторе L2Evaluator и трансляции в формулы SMT-решателя.

### 1. Общие правила поведения #0 (Val::Null)

В системах ЧПУ Fanuc и Haas #0 означает «переменная не определена / аргумент не передан», а не число 0.0. Это критично для макросов, где проверяется наличие опционального параметра (например, IF [#18 EQ #0] THEN #18 = 2.0).
Главная ловушка: в арифметике #0 ведёт себя как 0.0, но в операциях сравнения на равенство (EQ/NE) и при передаче в адресные слова G-кода он отличается от 0.0.

### 2. Таблица операций для L2Evaluator

Обозначения: Null = #0, Num(x) = число f64 (x ≠ 0), Zero = Num(0.0).

#### 2.1 Присваивание и адресные слова G-кода

| Операция | Пример | Результат / Поведение | Примечание |
|---|---|---|---|
| Присваивание переменной | #100 = #0 | #100 становится Val::Null | Копируется именно состояние Null, не 0.0 |
| Присваивание арифметики | #100 = #0 + 0 | #100 становится Val::Num(0.0) | Любая арифметическая операция снимает статус Null |
| Подстановка в координату / слово | G0 X#0 Y10.0 (где #0 = Null) | Слово X игнорируется (как будто не написано) | Критично! G0 X#0 — это не G0 X0.0, а сохранение текущей координаты X (аналог пропущенного слова) |
| Вызов макроса с Null-аргументом | G65 P1000 X#0 | Внутри макроса #24 получает Val::Null | Проброс опционального аргумента по цепочке макросов |

#### 2.2 Арифметика и математические функции

Во всех арифметических и тригонометрических операциях Val::Null неявно приводится к 0.0, а результат всегда имеет тип Val::Num(...) (никогда Null):

| Операция | Операнды | Результат L2Evaluator |
|---|---|---|
| Унарный минус -a | Null | Num(-0.0) (канонизируется в Num(0.0)) |
| Сложение a + b | Null + Null / Null + Num(x) | Num(0.0) / Num(x) |
| Вычитание a - b | Null - Num(x) | Num(-x) |
| Умножение a * b | Null * Num(x) | Num(0.0) |
| Деление a / b | Null / Num(x) (x ≠ 0) | Num(0.0) |
| Деление на Null a / Null | Num(x) / Null или Null / Null | Ошибка E_INVALID_INPUT (Division by zero) |
| Функции (SIN, COS, SQRT, ABS, ROUND, FIX, FUP, ATAN) | SIN[#0], COS[#0], SQRT[#0] | Вычисляются от 0.0: Num(0.0), Num(1.0), Num(0.0) |
| Логарифм LN[#0] | Null | Ошибка E_INVALID_INPUT (ln(0) не определён) |

#### 2.3 Операции сравнения (EQ, NE, GT, LT, GE, LE)

В Fanuc/Haas сравнения возвращают булево значение (для условий IF/WHILE) или число 1.0 / 0.0 (если используются в выражениях).

| Оператор | Null vs Null | Null vs Num(0.0) | Null vs Num(x) (x > 0) | Null vs Num(x) (x < 0) |
|---|---|---|---|---|
| EQ (равно) | true | false (главное отличие!) | false | false |
| NE (не равно) | false | true (главное отличие!) | true | true |
| GT (>) | false | false | false (0 > x ложно) | true (0 > x истинно) |
| LT (<) | false | false | true (0 < x истинно) | false (0 < x ложно) |
| GE (>=) | true | true (в Fanuc ≥ приводится к 0.0!) | false | true |
| LE (<=) | true | true (в Fanuc ≤ приводится к 0.0!) | true | false |

Важный нюанс Fanuc/Haas: В операторах неравенства (GT, LT, GE, LE) контроллер приравнивает #0 к 0.0 (поэтому #0 GE 0 и #0 LE 0 дают true), тогда как в операторах тождества (EQ, NE) #0 равен только #0.

#### 2.4 Логические и побитовые операции (AND, OR, XOR)

В стандартном Macro B AND, OR, XOR работают как побитовые операции над целыми числами (с приведением Null к 0), а в условиях IF [...] результат ≠ 0 считается истинным:
- Null AND x → 0 & (x as i64) = Num(0.0)
- Null OR x → 0 | (x as i64) = Num((x as i64) as f64)
- Null XOR x → 0 ^ (x as i64) = Num((x as i64) as f64)

### 3. Кодирование в SMT-солвер (feature = "smt")

Чтобы SMT-решатель (например, Z3 или CVC5) мог точно доказывать достижимость веток IF при неизвестных входных переменных (#500+ или неинициализированных аргументах #1–#33), каждое значение Val кодируется как кортеж из двух SMT-переменных:

SMT(v) = (is_null_v : Bool, num_v : Real)

#### 3.1 Аксиома нормализации констант

- Для Expr::Null: is_null = true, num = 0.0.
- Для Expr::Const(c): is_null = false, num = c.
- Для неизвестной переменной #i: is_null_i и num_i свободны, но связаны инвариантом:
  is_null_i ⟹ (num_i == 0.0)
Это гарантирует, что внутри любых арифметических операций можно использовать num_i напрямую без лишних условных ветвлений ite!

#### 3.2 Трансляция операций в SMT-термы

| Операция L2 | Результирующий is_null | Результирующий num (SMT-формула) |
|---|---|---|
| a + b | false | num_a + num_b (работает благодаря инварианту is_null => num == 0) |
| a - b | false | num_a - num_b |
| a * b | false | num_a * num_b |
| a / b | false | num_a / num_b + проверка пути assert(num_b != 0.0) |
| a EQ b | — (Bool) | (is_null_a && is_null_b) \|\| (!is_null_a && !is_null_b && num_a == num_b) |
| a NE b | — (Bool) | !(a EQ b) |
| a GT b | — (Bool) | num_a > num_b (благодаря инварианту Null автоматически сравнивается как 0.0) |
| a LT b | — (Bool) | num_a < num_b |
| a GE b | — (Bool) | num_a >= num_b (автоматически даёт true для Null >= 0.0!) |
| a LE b | — (Bool) | num_a <= num_b |
| G0 X(expr) | — | Если is_null_expr == true → координата X наследуется от prev_end.x; если false → num_expr |

### 4. Эталонная реализация для crates/eid-l2/src/eval.rs

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Val {
    Null,
    Num(f64),
}

impl Val {
    /// Приведение к числу для арифметики и неравенств (Null -> 0.0).
    #[inline]
    pub fn as_num(self) -> f64 {
        match self {
            Val::Null => 0.0,
            Val::Num(v) => v,
        }
    }

    /// Семантика Fanuc/Haas EQ: Null == Null (true), Null == 0.0 (false).
    #[inline]
    pub fn macro_eq(self, other: Val) -> bool {
        match (self, other) {
            (Val::Null, Val::Null) => true,
            (Val::Null, Val::Num(_)) | (Val::Num(_), Val::Null) => false,
            (Val::Num(a), Val::Num(b)) => (a - b).abs() <= 1e-9, // допуск сравнения Macro B
        }
    }

    #[inline]
    pub fn macro_ne(self, other: Val) -> bool {
        !self.macro_eq(other)
    }

    /// Неравенства (GT, LT, GE, LE): Null приводится к 0.0.
    #[inline]
    pub fn macro_ge(self, other: Val) -> bool {
        self.as_num() >= other.as_num()
    }
}
```
