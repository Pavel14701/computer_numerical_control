//! EID/L2 AST — этап 12.1, фаза 12.1 (агент A). Источник: DESIGN-док
//! `EID-L2-DESIGN.md` v0.1 (коммит d3fd57e) — §2.1–§2.4 ДОСЛОВНО.
//! Смежные спеки: SPEC-EID-L1 v0.20, interface v0.2.0, grammar v0.4.
//!
//! Синхронизация с доком (финальный заход):
//! * §2.1: `Expr::Const(Num)` (verbatim-константа, Num из eid_core);
//!   `Val` — + Copy (эталон Приложения дока);
//! * §2.2: SysVar — derive Debug/Clone/**Copy**/PartialEq/**Eq**;
//! * §2.3: TransformKind — Rotate2D{center:[Expr;2], angle_deg, plane},
//!   Scale{center:[Expr;3], factor}, Mirror{axis, pivot},
//!   TiltedWorkPlane{origin:[Expr;3], euler:[Expr;3]};
//! * §2.4: все управляющие варианты несут `src: SrcSpan`; Move{target,
//!   feed, spindle}; IfThen{then_stmt} — одиночный (без else, Macro B);
//!   IfGoto{target_label: u32}; While{do_id: u32}; CallSub.args —
//!   Vec<(u8, Expr)>; RtcpMode{enabled, tool_length_h}.
//!
//! ОТКЛОНЕНИЕ (задокументировано, стоп-отчёт агента A): `Plane` в доке
//! §2.3 заявлен «из eid_core», но в eid-core::ast его НЕТ (проверено
//! grep по crates/eid-core/src). eid-core read-only для 12.1 — enum
//! определён ЗДЕСЬ (значения G17/G18/G19 → XY/ZX/YZ) с переездом в
//! eid-core по PR к оркестратору (замена импорта — 1 строка).
//! L2Program/L2Sub/placeholder_span — каркас 12.1 (доком §2 не покрыты),
//! расширяются аддитивно.
//!
//! v0.20.1 (12.2, агент A) — аддитивно: `L2Stmt::Label` (N-метка-узел),
//! `L2Stmt::Return` (M99/endsub-выход), `L2Program::labels` (label-таблица,
//! заполняет парсер — контракт для B2/D2).
//!
//! Амендмент дока v0.1.2 (BUG-D1/D2; trace-зеркало agent D, коммит
//! 841ab2c → фикс B2): привязка букв-аргументов G65 — НЕлинейная
//! Fanuc-таблица ([`call_arg_local_index`]; линейная «A→#1..Z→#26» дока
//! §2.4 отвергнута), LinuxCNC `o call` — позиционная
//! ([`positional_arg_local_index`]); dwell G4 — диалект-семантика
//! (`parser_macro_common::DwellSemantics`: Fanuc bare-P = мс, Haas
//! P-dot-rule, X/U = сек). AST-узлы НЕ менялись (аддитивно).

use eid_core::ast::{Num, Op, SrcSpan};

/// Плоскость (G17/G18/G19). ЛОКАЛЬНО до переезда в eid-core (см. шапку
/// модуля): док §2.3 ожидает `eid_core`-тип.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plane {
    /// G17 — XY.
    XY,
    /// G18 — ZX.
    ZX,
    /// G19 — YZ.
    YZ,
}

/// Значение макро-мирa (семантика #0, Приложение дока — эталон):
/// Null («пустая» переменная #0) или число. В арифметике Null → 0.0,
/// результат всегда `Num`; в EQ/NE Null равен только Null.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Val {
    /// #0 (vacant).
    Null,
    /// f64-зеркало (округление до 4dp — только на границе L1/эмиссии,
    /// SPEC §10.5).
    Num(f64),
}

/// Выражение макро-языка (§2.1 дословно).
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Константа с verbatim-текстом (Num из eid_core, SPEC §10.1).
    Const(Num),
    /// Литеральный #0.
    Null,
    /// Переменная: #100, #[#100 + 5], #<my_var>, системная (§2.2).
    Var(VarRef),
    /// Neg, Abs, Sin, Cos, Tan, Asin, Acos, Atan, Sqrt, Ln, Exp, Round,
    /// Fix, Fup (градусы — у тригонометрии аргумент/результат).
    Unary(UnaryOp, Box<Expr>),
    /// Add, Sub, Mul, Div, Mod, And, Or, Xor, Eq, Ne, Gt, Lt, Ge, Le.
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
}

impl Expr {
    /// Константа из f64 (шорткат тестов/парсеров; verbatim — кратчайший
    /// Display, детерминированный).
    pub fn num(v: f64) -> Self {
        Self::Const(Num::new(v.to_string(), v))
    }
    /// Переменная #<idx> (шорткат).
    pub fn var(idx: u32) -> Self {
        Self::Var(VarRef::Direct(idx))
    }
}

/// Унарные операции (§2.1, словарь Fanuc Custom Macro B).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Abs,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sqrt,
    Ln,
    Exp,
    Round,
    Fix,
    Fup,
    /// v0.1.1 амендмент (корпус-ценз, агент B2): `EXISTS[#<var>]` — 1.0,
    /// если переменная определена, иначе 0.0 (LinuxCNC-конструкция,
    /// corpus/tier_b g881). Оценивается БЕЗ вычисления аргумента
    /// (см. `crate::eval::eval_expr`).
    Exists,
}

/// Бинарные операции (§2.1): арифметика, побитовая логика, сравнения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Xor,
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    /// v0.1.1 амендмент (корпус-ценз, агент B2): двухаргументный
    /// арктангенс `ATAN[y]/[x]` → Atan2(y, x), результат в градусах
    /// (corpus/tier_b ngcgui_lib_arc2). Одноаргументный ATAN — UnaryOp::Atan.
    Atan2,
}

/// Ссылка на переменную (§2.1 дословно).
#[derive(Debug, Clone, PartialEq)]
pub enum VarRef {
    /// #100.
    Direct(u32),
    /// #[#100 + 5] — индекс вычисляется выражением.
    Indirect(Box<Expr>),
    /// #<my_var> (LinuxCNC).
    Named(String),
    /// Каноническая системная переменная (§2.2).
    Sys(SysVar),
}

/// Ось (§2.2): линейные X/Y/Z + ротационные A/B/C + дополнительные U/V/W.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
    A,
    B,
    C,
    U,
    V,
    W,
}

impl Axis {
    /// Ротационная ось (A/B/C) — не lowered в L1 без RTCP-контекста (§3).
    pub fn is_rotary(self) -> bool {
        matches!(self, Self::A | Self::B | Self::C)
    }
    /// Буква оси (эмиссия/диагностика).
    pub fn letter(self) -> char {
        match self {
            Self::X => 'X',
            Self::Y => 'Y',
            Self::Z => 'Z',
            Self::A => 'A',
            Self::B => 'B',
            Self::C => 'C',
            Self::U => 'U',
            Self::V => 'V',
            Self::W => 'W',
        }
    }
}

/// Канонические системные переменные (§2.2 дословно; derives §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysVar {
    /// Текущие координаты (Fanuc #5021..) — E_RUNTIME_STATE при неизвестном
    /// M0 → блокируют lowering.
    AbsMachinePos(Axis),
    /// Fanuc #5041..
    WorkPos(Axis),
    /// Fanuc #5061.. (результат G31).
    SkipPos(Axis),
    /// Модальные группы (Fanuc #4001..#4030) — разрешимы статически при
    /// отсутствии нераскрытых Raw (разрешение — 12.2).
    ModalGroup(u8),
    /// Таблицы смещений WCS: G54–G59 (#5221+), доп. WCS (#7001+).
    WcsOffset { p: u8, axis: Axis },
    /// Вендорская переменная без кросс-диалектного аналога.
    VendorSpecific(u32),
}

/// Поза с выражениями в словах-адресах (§2.3 дословно): отсутствующее
/// слово = None; присутствующее слово со значением Null = слово
/// игнорируется (Приложение, таблица 2.1) — [`crate::interp::pose_resolve`].
#[derive(Debug, Clone, PartialEq)]
pub struct ExprPose {
    pub x: Option<Expr>,
    pub y: Option<Expr>,
    pub z: Option<Expr>,
    pub a: Option<Expr>,
    pub b: Option<Expr>,
    pub c: Option<Expr>,
}

impl ExprPose {
    /// Пустая поза (нет ни одного слова).
    pub fn empty() -> Self {
        Self {
            x: None,
            y: None,
            z: None,
            a: None,
            b: None,
            c: None,
        }
    }
    /// Есть хоть одна ротационная компонента A/B/C.
    pub fn has_rotary(&self) -> bool {
        self.a.is_some() || self.b.is_some() || self.c.is_some()
    }
}

/// Трансформации (§2.3 ДОСЛОВНО).
#[derive(Debug, Clone, PartialEq)]
pub enum TransformKind {
    /// G68 X Y R (поворот в плоскости `plane` вокруг `center` на угол).
    Rotate2D {
        center: [Expr; 2],
        angle_deg: Expr,
        plane: Plane,
    },
    /// G51 X Y Z P (масштаб относительно центра).
    Scale { center: [Expr; 3], factor: Expr },
    /// G51.1 X0 / Haas G101 (зеркало по оси относительно пивота).
    Mirror { axis: Axis, pivot: Expr },
    /// G68.2 (3+2): наклонная рабочая плоскость (origin + эйлеровы углы).
    TiltedWorkPlane { origin: [Expr; 3], euler: [Expr; 3] },
}

/// Вид хода (§2.4 комментарий дословно): Rapid, Linear,
/// Arc { ijk: [Option<Expr>; 3], r: Option<Expr>, dir: Dir }.
#[derive(Debug, Clone, PartialEq)]
pub enum MoveKind {
    Rapid,
    Linear,
    Arc {
        /// [I, J, K] — смещение центра дуги.
        ijk: [Option<Expr>; 3],
        /// R-форма (радиус), альтернатива IJK.
        r: Option<Expr>,
        /// G2/G3 (eid_core::ast::Dir).
        dir: eid_core::ast::Dir,
    },
}

/// Цель перехода/вызова: O-номер (Fanuc/Haas), N-метка (Haas M97) или
/// именованная O-подпрограмма (LinuxCNC).
/// (IfGoto в доке §2.4 несёт `target_label: u32` — SubTarget используется
/// только CallSub.)
// АМЕНДМЕНТ v0.1.2 (парсер D2, рулинг оркестратора из задания D2:
// «o-call → CallSub{target: SubTarget::Named(name), args}»): добавлен
// Named(String) для именованных O-sub LinuxCNC `o<name> sub/call` (имя —
// произвольный текст: "factorial", "probe_y_minus"; в корпусе ×34 это
// ~25 файлов). Copy СНЯТ (String не Copy) — единственные потребители
// (crate::interp) компилируются без Copy; матч форматирования ошибки в
// interp.rs дополнен arm'ом (механическое следствие — не-exhaustive match).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubTarget {
    /// `O123` — числовая подпрограмма (Fanuc/Haas/LinuxCNC `o123 sub`).
    ONumber(u32),
    /// Haas M97 P<N> — вызов по N-метке блока.
    NLabel(u32),
    /// LinuxCNC `o<name>` — именованная подпрограмма (имя до `>`,
    /// verbatim; напр. "factorial", "probe_y_minus", "g881").
    Named(String),
}

/// Оператор L2 (§2.4 ДОСЛОВНО). Все управляющие варианты несут
/// `src: SrcSpan` (единственное окно до старта парсеров B/C/D).
// Точечный allow с обоснованием (паттерн eid-core::ast::Op): Move инлайнит
// ExprPose (6 слов) — крупнейший вариант против RtcpMode{bool}. Box<ExprPose>
// исправил бы линт, но это отступление от формы §2.4 (target: ExprPose) и
// лишняя аллокация на каждый ход — доминирующий тип L2. Вопрос к
// разморозке API — оркестратору; до тех пор гейт держится на allow.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum L2Stmt {
    /// Ход с макро-выражениями.
    Move {
        kind: MoveKind,
        /// Целевая поза (§2.4: поле называется `target`).
        target: ExprPose,
        feed: Option<Expr>,
        spindle: Option<Expr>,
        src: SrcSpan,
    },
    /// Блок трансформации: применяется к вложенному телу (§2.3).
    WithTransform {
        transform: TransformKind,
        body: Vec<L2Stmt>,
        src: SrcSpan,
    },
    /// Присваивание: #var = expr (семантика #0: копирует Null).
    Assign {
        target: VarRef,
        value: Expr,
        src: SrcSpan,
    },
    /// IF [cond] GOTO <target_label> — метка N-строки (u32; таблица меток — 12.2).
    IfGoto {
        cond: Expr,
        target_label: u32,
        src: SrcSpan,
    },
    /// IF [cond] THEN <stmt> — одиночный оператор (Macro B; else нет).
    IfThen {
        cond: Expr,
        then_stmt: Box<L2Stmt>,
        src: SrcSpan,
    },
    /// WHILE [cond] DO<do_id> ... END<do_id> (do_id 1..3, DO/END-парность —
    /// обязанность парсера).
    While {
        cond: Expr,
        do_id: u32,
        body: Vec<L2Stmt>,
        src: SrcSpan,
    },
    /// Вызов подпрограммы: target (O-номер | N-метка), repeat — L/K,
    /// args — буквы-аргументы (ASCII u8): Macro B G65 — именованная
    /// НЕлинейная Fanuc-таблица [`call_arg_local_index`] (амендмент
    /// v0.1.2, BUG-D1: линейная «A→#1..Z→#26» дока §2.4 отвергнута);
    /// LinuxCNC `o call [a][b]` — позиционная ([`positional_arg_local_index`],
    /// выбор на binding-сайтах interp). modal — G66.
    CallSub {
        target: SubTarget,
        repeat: Option<Expr>,
        args: Vec<(u8, Expr)>,
        modal: bool,
        src: SrcSpan,
    },
    /// Узлы 5-осевой кинематики (в Pass 1 опускаются как Op::Raw):
    /// G43.4 вкл / G49 выкл; tool_length_h — H-номер компенсации длины.
    RtcpMode {
        enabled: bool,
        tool_length_h: Option<u32>,
        src: SrcSpan,
    },
    /// v0.20.1 (аддитивно, агент A — 12.2): N-метка как отдельный узел
    /// (`N10`). КОНТРАКТ ПАРСЕРОВ (B2/D2): N-метки, достижимые GOTO/M97,
    /// НЕ отбрасываются — носитель либо этот узел (собирается
    /// `lowering::collect_labels` в per-body таблицу), либо таблица
    /// [`L2Program::labels`] (заполняет парсер). Исполнение — no-op
    /// (позиция перехода в CFG-исполнителе).
    Label { n: u32, src: SrcSpan },
    /// v0.20.1 (аддитивно, агент A — 12.2): возврат из подпрограммы —
    /// M99 (Fanuc/Haas) / endsub с досрочным выходом. Парсер: M99 → этот
    /// узел; О-конец тела БЕЗ M99 — неявный return (pop кадра делает
    /// интерпретатор на выходе из тела, семантика одинаковая). Возврат-
    /// ЗНАЧЕНИЕ (LinuxCNC `return [expr]`, endsub-retval) — вне D0–D3
    /// (ценз-пробел ценза B: corpus ×3).
    Return { src: SrcSpan },
    /// Уже канонический L1-оп (passthrough конвейера; grammar v0.4, L1 v0.20).
    L1Op(Op),
}

impl L2Stmt {
    /// Спан оператора (для носителей RAW/диагностик); L1Op несёт спан
    /// внутри eid_core::Op — возвращает None.
    pub fn src(&self) -> Option<&SrcSpan> {
        match self {
            L2Stmt::Move { src, .. }
            | L2Stmt::WithTransform { src, .. }
            | L2Stmt::Assign { src, .. }
            | L2Stmt::IfGoto { src, .. }
            | L2Stmt::IfThen { src, .. }
            | L2Stmt::While { src, .. }
            | L2Stmt::CallSub { src, .. }
            | L2Stmt::RtcpMode { src, .. }
            | L2Stmt::Label { src, .. }
            | L2Stmt::Return { src } => Some(src),
            L2Stmt::L1Op(_) => None,
        }
    }
}

/// Индекс локальной переменной для буквы аргумента G65/M98 (Macro B),
/// регистронезависимо. Небуквенный/внедиапазонный байт → None.
///
/// АМЕНДМЕНТ v0.1.2 (BUG-D1, критично; trace-зеркало deep-drill — agent D,
/// коммит 841ab2c → фикс B2): таблица НЕлинейная — стандарт Fanuc Custom
/// Macro B (Haas Macro B идентична):
/// `A→#1 B→#2 C→#3 D→#7 E→#8 F→#9 H→#11 I→#4 J→#5 K→#6 M→#13 Q→#17
/// R→#18 S→#19 T→#20 U→#21 V→#22 W→#23 X→#24 Y→#25 Z→#26`.
/// ЛИНЕЙНАЯ таблица «A→#1 .. Z→#26» из дока v0.1 §2.4 ОТВЕРГНУТА: на
/// corpus/tier_b/inbox/mpe_g65_macro_deep_drill.nc она связывала D-.4→#4
/// и F60.→#6 (Fanuc: #7/#9) → guard `IF[#7 EQ #0] GOTO943` срабатывал →
/// alarm-путь `#3000=3` вместо рабочей программы (trace semantic_diff
/// ≠ 0). Слоты #10/#12/#14/#15/#16 буквами G65 не адресуются. Буквы
/// G/L/N/O/P аргументами НЕ являются (G — коды, L — повтор, P — номер
/// программы/время, N/O — номера строк/программ) → None.
///
/// Для ПОЗИЦИОННОЙ привязки (LinuxCNC `o<…> call [a][b]…` — парсер
/// LinuxCNC синтезирует буквы 'A'+i в порядке следования) — см.
/// [`positional_arg_local_index`]; выбор таблицы на binding-сайтах
/// `interp` (у узла [`L2Stmt::CallSub`] нет поля диалекта).
pub fn call_arg_local_index(letter: u8) -> Option<u32> {
    Some(match letter.to_ascii_uppercase() {
        b'A' => 1,
        b'B' => 2,
        b'C' => 3,
        b'I' => 4,
        b'J' => 5,
        b'K' => 6,
        b'D' => 7,
        b'E' => 8,
        b'F' => 9,
        b'H' => 11,
        b'M' => 13,
        b'Q' => 17,
        b'R' => 18,
        b'S' => 19,
        b'T' => 20,
        b'U' => 21,
        b'V' => 22,
        b'W' => 23,
        b'X' => 24,
        b'Y' => 25,
        b'Z' => 26,
        // G/L/N/O/P — мета-слова, аргументами не передаются (см. шапку).
        _ => return None,
    })
}

/// ПОЗИЦИОННАЯ привязка: k-я буква A..Z → локаль #(k+1), линейно,
/// регистронезависимо. Это семантика LinuxCNC `o<…> call [arg1] [arg2] …`
/// (первый `[arg]` → #1 и т.д.); парсер LinuxCNC кодирует позицию i
/// буквой `'A'+i`. Macro B G65 НЕ использует (именованная таблица —
/// [`call_arg_local_index`]).
pub fn positional_arg_local_index(letter: u8) -> Option<u32> {
    let up = letter.to_ascii_uppercase();
    if up.is_ascii_uppercase() {
        Some((up - b'A') as u32 + 1)
    } else {
        None
    }
}

/// Подпрограмма (O-номер/N-метка → тело). Наполняется парсерами (B/C/D).
/// Каркас 12.1 (доком §2 не определён, необходим интерпретатору).
#[derive(Debug, Clone, PartialEq)]
pub struct L2Sub {
    pub target: SubTarget,
    pub body: Vec<L2Stmt>,
}

/// L2-программа: верхнеуровневый блок + подпрограммы.
/// Каркас 12.1 (доком §2 не определён); поля расширяются аддитивно (12.2).
#[derive(Debug, Clone, PartialEq)]
pub struct L2Program {
    pub stmts: Vec<L2Stmt>,
    /// Обнаруженные парсером подпрограммы (поиск CallSub по SubTarget).
    pub subs: Vec<L2Sub>,
    /// v0.20.1: label-таблица, ЗАПОЛНЯЕТ ПАРСЕР (контракт B2/D2):
    /// `N<номер> → индекс stmt` в `stmts` (индекс помеченного оператора;
    /// переход GOTO продолжает С него). N-метки, достижимые GOTO/M97
    /// в ГЛАВНОМ блоке, не отбрасываются. Метки в телах subs — через узлы
    /// [`L2Stmt::Label`] (per-body сбор в `lowering::collect_labels`).
    /// При конфликте с собранными из узлов — поле парсера имеет приоритет.
    pub labels: std::collections::BTreeMap<u32, usize>,
}

impl L2Program {
    /// Пустая программа.
    pub fn empty() -> Self {
        Self {
            stmts: Vec::new(),
            subs: Vec::new(),
            labels: std::collections::BTreeMap::new(),
        }
    }
    /// Конструктор (шорткат тестов/парсеров): stmts + subs, labels пусты
    /// (парсер, строящий таблицу меток, заполняет поле вручную после).
    pub fn new(stmts: Vec<L2Stmt>, subs: Vec<L2Sub>) -> Self {
        Self {
            stmts,
            subs,
            labels: std::collections::BTreeMap::new(),
        }
    }
    /// Поиск подпрограммы по цели (первое совпадение).
    pub fn find_sub(&self, target: &SubTarget) -> Option<&L2Sub> {
        self.subs.iter().find(|s| &s.target == target)
    }
}

/// Плейсхолдер-спан для тестов/синтезированных операторов (формат — как в
/// golden AST L1-парсера: 0.1-0.0; файл «eid-l2»). Реальные спаны —
/// обязанность парсеров B/C/D.
pub fn placeholder_span() -> SrcSpan {
    SrcSpan {
        file: std::sync::Arc::from("eid-l2"),
        line_start: 0,
        col_start: 1,
        line_end: 0,
        col_end: 0,
    }
}
