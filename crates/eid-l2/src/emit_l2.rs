//! ЭМИССИЯ L2→L2 — ядро + Macro-B ветви (Fanuc ↔ Haas) (этап 12.4, агент D3).
//! Источник: DESIGN-док `EID-L2-DESIGN.md` §5 фаза 12.4 (DoD: «Валидный
//! round-trip L2-программ с G31 probing и таблицами маппинга SysVar между
//! Fanuc и Haas»), §2.1–§2.4 AST. Смежное: haas_dialect_analysis §4.3
//! («нумерация system vars отличается от Fanuc — перенос без таблицы
//! соответствий невозможен» — этот модуль и есть таблица соответствий).
//!
//! Архитектура (решение по заданию: общий emit-слой с диалект-ветками):
//! * [`emit_l2`] — этот файл: типы outcomes/notices/errors, ТАБЛИЦА
//!   SysVar-маппинга [`map_sysvar`], общий рендер выражений (скобочный
//!   синтаксис `[ ]` одинаков во всех трёх диалектах), Macro-B эмиссия
//!   (Fanuc/Haas — общий ядро, как parser_macro_common) и линт носителей
//!   (verbatim-совместимость по диалекту);
//! * [`emit_linuxcnc`] — O-word эмиссия LinuxCNC (пере-структуризация
//!   GOTO → o-if / do-while — DOD «включая трансляцию GOTO в структурные
//!   o-if/o-for LinuxCNC»).
//!
//! ## Таблица SysVar-маппинга (канон §2.2 → номер диалекта)
//!
//! | Канон | Fanuc | Haas NGC | LinuxCNC | решение |
//! |---|---|---|---|---|
//! | AbsMachinePos(a) | #5021+off | #5021+off | #5021+off | MAPPED(direct) ×3 |
//! | WorkPos(a) | #5041+off | #5041+off | **#5420+off** | F↔H direct; →L via-offset |
//! | SkipPos(a) | #5061+off | #5061+off | #5061+off | MAPPED(direct) ×3 |
//! | ModalGroup(g) | #4000+g | #4000+g | — | →L UNMAPPABLE (нет модальных переменных) |
//! | WcsOffset{p=0} | #5201+off | #5201+off | — | →L UNMAPPABLE (external ≠ G52/G92) |
//! | WcsOffset{p=1..6} | #5221+20(p−1)+off | то же | то же | MAPPED(direct) ×3 |
//! | WcsOffset{p=7..26} | — | — | — | UNMAPPABLE ×3 (нумерация доп. WCS вне 52xx — опция/станок-зависимая, '?') |
//! | VendorSpecific(3000..3004) | #n | #n | #n | PassThrough+notice ×3 (Renishaw dual-platform: zapcon #3004, rensh #3001; same-dialect LCNC — родные номера) |
//! | VendorSpecific(прочее) | #n | #n | #n | PassThrough+notice ×3 (номер сохранён: same-dialect точен; кросс-диалектная семантика НЕ сверена — не молча) |
//!
//! Источники строк таблицы: Fanuc/Haas — map_common_system (парсеры 12.1,
//! Renishaw-корпус на обоих диалектах); LinuxCNC — docs linuxcnc.org
//! gcode/overview.html: #5021–#5029 machine pos, #5420–#5428 current pos
//! (active CS), #5061–#5069 probe result, #5221+20k координатные системы
//! G54..G59 (base = Fanuc), модальных #4xxx НЕТ.
//!
//! ## Честные отказы (не молча)
//! * носители (Op::Raw, вне §2.4): verbatim + линт диалект-совместимости
//!   (`;` → Fanuc отказ; `,`-слова → LinuxCNC отказ; G154/G110-G129/M97/
//!   G103/G31/G65/G66/G43.4 → не-Haas/не-Macro-B отказы или notice —
//!   см. [`carrier_compat`]);
//! * M97 (Haas local-call) → Fanuc/LinuxCNC: отказ (нет аналога);
//! * Named `#<var>` → Fanuc/Haas: отказ (нет именованных переменных);
//! * G65-аргументы НЕ из {A,B,C}-позиций → LinuxCNC: отказ (именованная
//!   нелинейная таблица Fanuc ≠ позиционная LinuxCNC — задание требует
//!   задокументировать: A→#1 B→#2 C→#3 совпадают с позиционными 1..3,
//!   D→#7 Fanuc ≠ позиция 4);
//! * Return в main → LinuxCNC: отказ (M99-in-main = цикл программы).
//!
//! НОМЕРА ГЛАВНОЙ ПРОГРАММЫ В AST НЕТ (12.1 каркас: первый O-заголовок
//! парсер поглощает) — эмиссия main идёт без O-заголовка, subs — со
//! своими O{номер}. Пробел задокументирован в стоп-отчёте.
//!
//! Амендмент дока v0.1.4 (2026-11-26, EID-L2-DESIGN §5.2): настоящая
//! таблица SysVar-маппинга + решения GOTO-реструктуризации — design
//! intent фазы 12.4; расширяет док БЕЗ изменения AST (ast.rs заморожен).

use crate::ast::{
    Axis, BinaryOp, Expr, ExprPose, L2Program, L2Stmt, MoveKind, Plane, SubTarget, SysVar,
    TransformKind, UnaryOp, VarRef,
};
use eid_core::ast::{Dir, Num, Op, SrcSpan};

// ===========================================================================
// Целевой диалект
// ===========================================================================

/// Целевой диалект эмиссии L2→L2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetDialect {
    /// FANUC Custom Macro B.
    Fanuc,
    /// HAAS NGC (Macro B-совместимый + расширения).
    Haas,
    /// LinuxCNC rs274ngc (O-word + именованные переменные).
    LinuxCnc,
}

impl TargetDialect {
    /// Имя диалекта (диагностика/тэги).
    pub fn name(self) -> &'static str {
        match self {
            Self::Fanuc => "fanuc-MB",
            Self::Haas => "haas-MB",
            Self::LinuxCnc => "linuxcnc",
        }
    }
    /// Macro B семейство (Fanuc/Haas — общий эмиссионный контур).
    pub fn is_macro_b(self) -> bool {
        matches!(self, Self::Fanuc | Self::Haas)
    }
}

// ===========================================================================
// Результаты эмиссии
// ===========================================================================

/// Предупреждение эмиссии (конструкция переведена, но с документированной
/// оговоркой — «не молча»).
#[derive(Debug, Clone, PartialEq)]
pub struct EmitNotice {
    /// Строка исходника (src.line_start stmt'а).
    pub line: u32,
    /// Класс оговорки (стабильный тег для тестов/аналитики).
    pub kind: &'static str,
    /// Человекочитаемое пояснение.
    pub message: String,
}

/// Жёсткий отказ эмиссии конкретной конструкции (задание: «честный отказ»).
/// Весь файл при наличии error считается отказавшей ногой — текст эмитируется
/// до места отказа включительно, но признак [`EmitOutcome::ok`] = false.
#[derive(Debug, Clone, PartialEq)]
pub struct EmitError {
    /// Строка исходника.
    pub line: u32,
    /// Причина отказа (класс + пояснение).
    pub reason: String,
}

/// Результат эмиссии L2-программы в текст целевого диалекта.
#[derive(Debug, Clone, PartialEq)]
pub struct EmitOutcome {
    /// Текст программы (кадры через `\n`).
    pub text: String,
    /// Оговорки (переведено с оговоркой).
    pub notices: Vec<EmitNotice>,
    /// Отказы (конструкция непереводима — см. модульную шапку).
    pub errors: Vec<EmitError>,
}

impl EmitOutcome {
    /// Нога эмиссии валидна: отказов нет.
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

// ===========================================================================
// ТАБЛИЦА SysVar-маппинга (ядро 12.4)
// ===========================================================================

/// Как получен номер целевого диалекта.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapVia {
    /// Прямое соответствие (одинаковое или канон-задокументированное
    /// число в целевом диалекте).
    Direct,
    /// Через смещение кода (разные диапазоны, фиксированная дельта).
    ViaOffset,
}

/// Решение маппинга канонической SysVar в номер целевого диалекта
/// (задание: «Решение на каждый SysVar: MAPPED(direct) / MAPPED(via-offset) /
/// UNMAPPABLE→Raw (не молча)»).
#[derive(Debug, Clone, PartialEq)]
pub enum SysVarDecision {
    /// Существует верифицированный аналог — эмитировать `#number`.
    Mapped { number: u32, via: MapVia },
    /// Верифицированного аналога нет, но синтаксис `#n` в целевом диалекте
    /// валиден и номер сохраняется (round-trip AST держится); семантика
    /// НЕ сверена — сопровождается notice (PassThrough — «не молча»).
    PassThrough { number: u32, reason: String },
    /// Аналога нет И эмуляция молчаливым переносом номера была бы ЛОЖНОЙ
    /// (другая семья номеров) — честный отказ.
    Unmappable { reason: String },
}

/// Смещение канона по оси: порядок axis6 парсеров (#5021+к → X,Y,Z,A,B,C).
pub(crate) fn axis_off(a: Axis) -> u32 {
    match a {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
        Axis::A => 3,
        Axis::B => 4,
        Axis::C => 5,
        // U/V/W: Fanuc #5021..#5026 покрывают 6 осей; U/V/W системных
        // позиций не имеют → маппинг как C-хвост (не встречается в корпусе).
        Axis::U | Axis::V | Axis::W => 5,
    }
}

/// ТАБЛИЦА: каноническая SysVar (§2.2) → номер в целевом диалекте.
/// Строки таблицы задокументированы в модульной шапке.
pub fn map_sysvar(sv: &SysVar, target: TargetDialect) -> SysVarDecision {
    match sv {
        // #5021–#5026: Fanuc/Haas NGC/LinuxCNC — machine position (одинаковые
        // номера во всех трёх; LCNC docs «#5021–#5029 current absolute
        // machine position (G53)»).
        SysVar::AbsMachinePos(a) => SysVarDecision::Mapped {
            number: 5021 + axis_off(*a),
            via: MapVia::Direct,
        },
        // WorkPos: Fanuc/Haas #5041+ (NGC Fanuc-совместим; Renishaw Haas-макросы
        // corpus rensh_* читают #5041-43); LinuxCNC — #5420+ (docs: «#5420–#5428
        // current position in the active coordinate system») → via-offset.
        SysVar::WorkPos(a) => match target {
            TargetDialect::LinuxCnc => SysVarDecision::Mapped {
                number: 5420 + axis_off(*a),
                via: MapVia::ViaOffset,
            },
            _ => SysVarDecision::Mapped {
                number: 5041 + axis_off(*a),
                via: MapVia::Direct,
            },
        },
        // SkipPos (результат G31/G38-прохода): #5061+ во всех трёх
        // (LCNC docs «#5061–#5069 coordinates of a G38 probe result»).
        SysVar::SkipPos(a) => SysVarDecision::Mapped {
            number: 5061 + axis_off(*a),
            via: MapVia::Direct,
        },
        // Модальные группы: Fanuc/Haas #4001+ (Haas NGC Fanuc-совместим,
        // map_common_system обоих парсеров). LinuxCNC модальных переменных
        // НЕ имеет (docs не содержат семейства 4xxx) — молчаливый перенос
        // #4003 дал бы чтение постороннего числа → честный отказ.
        SysVar::ModalGroup(g) => match target {
            TargetDialect::LinuxCnc => SysVarDecision::Unmappable {
                reason: format!(
                    "modal info variable #{} has no LinuxCNC analog (rs274ngc has no 4xxx modal-family variables)",
                    4000 + *g as u32
                ),
            },
            _ => SysVarDecision::Mapped {
                number: 4000 + *g as u32,
                via: MapVia::Direct,
            },
        },
        // Таблицы WCS. p=0 — external offset (Fanuc/Haas #5201+; LinuxCNC
        // external-сдвига в виде #52xx не имеет: G52/G92 → #5211-9 с другой
        // семантикой) → отказ.
        SysVar::WcsOffset { p: 0, axis } => match target {
            TargetDialect::LinuxCnc => SysVarDecision::Unmappable {
                reason: "external WCS offset (#5201+ Fanuc/Haas) has no LinuxCNC analog (G52/G92 shared register #5211-#5219 has different semantics)".to_string(),
            },
            _ => SysVarDecision::Mapped {
                number: 5201 + axis_off(*axis),
                via: MapVia::Direct,
            },
        },
        // p=1..6 — G54..G59: ОДИНАКОВЫЕ базы #5221+20(p−1) во всех трёх
        // (LCNC docs: «#5221–#5230 Coordinate System 1 (G54)», шаг 20,
        // base совпадает с Fanuc; 12.1-парсеры канонизируют 5221..5326 → p).
        SysVar::WcsOffset { p: p @ 1..=6, axis } => SysVarDecision::Mapped {
            number: 5221 + 20 * (*p as u32 - 1) + axis_off(*axis),
            via: MapVia::Direct,
        },
        // p=7..26 — доп. WCS (Haas G110–G129/G154; Fanuc G54.1P). Нумерация
        // переменных за пределами 52xx опциона/станок-зависима (Haas #7001+
        // c недосверенным шагом '?'; Fanuc G54.1P → #11401+/25001+ семья) —
        // честный отказ (реестр `?` амендмента v0.1.4).
        SysVar::WcsOffset { p: p @ 7..=26, axis } => SysVarDecision::Unmappable {
            reason: format!(
                "extended WCS slot p={} ({}): target dialect variable numbering beyond G59 is option/machine-dependent (Haas G110-G129 #7001+ stride unverified '?', Fanuc G54.1P #11401+ family) — honest refusal",
                p,
                axis.letter()
            ),
        },
        SysVar::WcsOffset { p: _, .. } => SysVarDecision::Unmappable {
            reason: "WCS slot p out of 0..=26 — not representable".to_string(),
        },
        // VendorSpecific: верифицированного соответствия нет. F↔H: номер
        // сохраняется (оба парсера канонизируют #n → VendorSpecific(n) —
        // round-trip AST держится), семантика сопровождается notice
        // («не молча»). Диапазон 3000..=3004 — alarms/timers, используемые
        // Renishaw dual-platform макросами на обоих диалектах (corpus:
        // zapcon_g31 #3004, rensh #3001/#3000) — тот же PassThrough с
        // отдельной пометкой. →LinuxCNC: отказ (семейства номеров LF: 5399
        // #5400 #5220 и пр. имеют ДРУГУЮ семантику в rs274ngc).
        SysVar::VendorSpecific(n) => {
            // PassThrough для ВСЕХ целей: синтаксис `#n` валиден во всех
            // трёх диалектах, номер сохраняется (round-trip AST держится,
            // включая same-dialect LinuxCNC: #5220/#5399/#5400 — родные
            // номера rs274ngc, канонизированные в VendorSpecific парсером).
            // Семантика при КРОСС-диалекте не сверена — notice («не молча»).
            let reason = if (3000..=3004).contains(n) {
                format!(
                    "vendor variable #{n} (alarm/timer band): passed through verbatim, semantics assumed per Renishaw dual-platform macros (corpus evidence) — NOT verified against {} docs",
                    target.name()
                )
            } else {
                format!(
                    "vendor variable #{n}: passed through verbatim — number preserved (same-dialect exact; CROSS-dialect semantics UNVERIFIED, numbering families diverge)"
                )
            };
            SysVarDecision::PassThrough { number: *n, reason }
        }

    }
}

// ===========================================================================
// Общий рендер выражений (скобочный синтаксис одинаков F/H/LCNC)
// ===========================================================================

/// Рендер выражения в текст (единый для трёх диалектов — скобки `[ ]`,
/// операторы EQ/NE/GT/…, функции SIN[..] идентичны). Каноническая форма:
/// * Binary → `[a OP b]` — ПОЛНОЕ скобочное вложение. Обоснование: у Fanuc
///   Macro B НЕТ приоритетов операторов (строго слева направо), у LinuxCNC
///   приоритеты есть — полная скобочная нормализация сохраняет AST-дерево
///   во всех трёх диалектах;
/// * Unary → `OP[...]`; Neg → `-` + атом;
/// * Atan2 → `ATAN[y]/[x]` (v0.1.1 амендмент);
/// * Const → verbatim `Num.text`; Null → `#0`.
pub(crate) fn expr_text(e: &Expr) -> Result<String, EmitError> {
    match e {
        Expr::Const(n) => Ok(n.text.clone()),
        Expr::Null => Ok("#0".to_string()),
        Expr::Var(v) => var_text(v),
        Expr::Unary(op, inner) => {
            let x = expr_text(inner)?;
            if *op == UnaryOp::Neg {
                // -атом: атомы, начинающиеся с #/цифры/знака, — без скобок.
                Ok(if atom_needs_brackets(&x) {
                    format!("-[{x}]")
                } else {
                    format!("-{x}")
                })
            } else {
                Ok(format!("{}[{}]", unary_op_text(*op), x))
            }
        }
        Expr::Binary(BinaryOp::Atan2, y, x) => {
            // Двухаргументный ATAN (v0.1.1): ATAN[y]/[x] — форма одинакова
            // в Fanuc/Haas/LinuxCNC.
            Ok(format!("ATAN[{}]/[{}]", expr_text(y)?, expr_text(x)?))
        }
        Expr::Binary(op, a, b) => Ok(format!(
            "[{} {} {}]",
            expr_text(a)?,
            binary_op_text(*op),
            expr_text(b)?
        )),
    }
}

/// Атом требует скобок при составлении слова/унарного минуса (не начинается
/// с `#`, цифры, `.`, `+`, `-` — например `SIN[...]`).
pub(crate) fn atom_needs_brackets(text: &str) -> bool {
    !text.starts_with('#')
        && !text.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '.' | '+' | '-'))
}

/// Рендер VarRef (диалект-независимая часть; Sys — через [`map_sysvar`] —
/// диалект передаётся вызывающим контуром через отказ/notice в sink).
pub(crate) fn var_text(v: &VarRef) -> Result<String, EmitError> {
    match v {
        VarRef::Direct(n) => Ok(format!("#{n}")),
        VarRef::Indirect(e) => Ok(format!("#[{}]", expr_text(e)?)),
        VarRef::Named(name) => Err(EmitError {
            line: 0,
            reason: format!(
                "named variable #<{name}> has no Macro B syntax (Fanuc/Haas use numbered #N) — honest refusal"
            ),
        }),
        // Sys обрабатывается диалектным sink'ом (var_ref_text → map_sysvar);
        // прямой вызов var_text с Sys — программная ошибка вызывающего,
        // но не паника: честный Err с пояснением.
        VarRef::Sys(sv) => Err(EmitError {
            line: 0,
            reason: format!(
                "internal: Sys var {sv:?} must be routed through the dialect sink (map_sysvar)"
            ),
        }),
    }
}

fn unary_op_text(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Abs => "ABS",
        UnaryOp::Sin => "SIN",
        UnaryOp::Cos => "COS",
        UnaryOp::Tan => "TAN",
        UnaryOp::Asin => "ASIN",
        UnaryOp::Acos => "ACOS",
        UnaryOp::Atan => "ATAN",
        UnaryOp::Sqrt => "SQRT",
        UnaryOp::Ln => "LN",
        UnaryOp::Exp => "EXP",
        UnaryOp::Round => "ROUND",
        UnaryOp::Fix => "FIX",
        UnaryOp::Fup => "FUP",
        // EXISTS — только LinuxCNC (emit_linuxcnc проверяет до рендера:
        // Macro B ветка отказывает на узле Exists).
        UnaryOp::Exists => "EXISTS",
    }
}

fn binary_op_text(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "MOD",
        BinaryOp::And => "AND",
        BinaryOp::Or => "OR",
        BinaryOp::Xor => "XOR",
        BinaryOp::Eq => "EQ",
        BinaryOp::Ne => "NE",
        BinaryOp::Gt => "GT",
        BinaryOp::Lt => "LT",
        BinaryOp::Ge => "GE",
        BinaryOp::Le => "LE",
        // Atan2 рендерится отдельной веткой expr_text — сюда не попадает.
        BinaryOp::Atan2 => "ATAN2",
    }
}

/// Слово `буква+значение`: атомы (#, число, знак) — слитно, прочее — в
/// скобках (`X[SIN[#1]]`). Условие-атом определено в [`atom_needs_brackets`].
pub(crate) fn attach_word(letter: char, rendered: &str) -> String {
    if atom_needs_brackets(rendered) {
        format!("{letter}[{rendered}]")
    } else {
        format!("{letter}{rendered}")
    }
}

/// Условие: снять внешние скобки Binary-канона (`[a GT b]` → `a GT b`),
/// чтобы получить `IF [a GT b]`, а не `IF [[a GT b]]`.
pub(crate) fn strip_outer_brackets(text: &str) -> &str {
    if text.starts_with('[') && text.ends_with(']') && text.len() >= 2 {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

// ===========================================================================
// Точка входа
// ===========================================================================

/// Эмиссия L2-программы в текст целевого диалекта. Нога валидна ⟺
/// [`EmitOutcome::ok`]; при отказах текст эмитируется до места отказа.
pub fn emit(prog: &L2Program, target: TargetDialect) -> EmitOutcome {
    match target {
        TargetDialect::Fanuc | TargetDialect::Haas => macro_b_emit(prog, target),
        TargetDialect::LinuxCnc => crate::emit_linuxcnc::linuxcnc_emit(prog),
    }
}

// ===========================================================================
// Macro-B эмиссия (Fanuc / Haas — общий контур)
// ===========================================================================

/// Sink строк + оговорок/отказов (общий для Macro-B ветви).
struct Sink {
    lines: Vec<String>,
    notices: Vec<EmitNotice>,
    errors: Vec<EmitError>,
    /// Текущая строка исходника (src.line_start активного stmt).
    line: u32,
    target: TargetDialect,
}

impl Sink {
    fn new(target: TargetDialect) -> Self {
        Self {
            lines: Vec::new(),
            notices: Vec::new(),
            errors: Vec::new(),
            line: 0,
            target,
        }
    }
    fn emit(&mut self, text: String) {
        self.lines.push(text);
    }
    fn note(&mut self, kind: &'static str, message: String) {
        self.notices.push(EmitNotice {
            line: self.line,
            kind,
            message,
        });
    }
    fn refuse(&mut self, reason: String) {
        self.errors.push(EmitError {
            line: self.line,
            reason,
        });
    }
    fn set_line(&mut self, src: Option<&SrcSpan>) {
        if let Some(s) = src {
            self.line = s.line_start;
        }
    }
    /// Рендер Sys-ссылки через таблицу ([`map_sysvar`]) + notice/отказ.
    fn sys_var_text(&mut self, sv: &SysVar) -> Option<String> {
        match map_sysvar(sv, self.target) {
            SysVarDecision::Mapped { number, .. } => Some(format!("#{number}")),
            SysVarDecision::PassThrough { number, reason } => {
                let kind = if (3000..=3004).contains(&number) {
                    "sysvar-passthrough-alarm-band"
                } else {
                    "sysvar-passthrough-unverified"
                };
                self.note(kind, reason);
                Some(format!("#{number}"))
            }
            SysVarDecision::Unmappable { reason } => {
                self.refuse(reason);
                None
            }
        }
    }
    /// Рендер любой VarRef с диалект-веткой Sys.
    fn var_ref_text(&mut self, v: &VarRef) -> Option<String> {
        match v {
            VarRef::Sys(sv) => self.sys_var_text(sv),
            // Indirect: внутреннее выражение может содержать Sys — рендер
            // диалект-зависимой рекурсией (не expr_text, где Sys panic).
            VarRef::Indirect(e) => {
                let t = self.expr(e)?;
                Some(format!("#[{t}]"))
            }
            other => match var_text(other) {
                Ok(t) => Some(t),
                Err(e) => {
                    self.errors.push(EmitError {
                        line: self.line,
                        reason: e.reason,
                    });
                    None
                }
            },
        }
    }
    /// Рендер произвольного Expr с диалект-веткой Sys внутри Var.
    fn expr(&mut self, e: &Expr) -> Option<String> {
        // expr_text не знает диалекта (Sys внутри Var) — рекурсивный спуск
        // только для Var::Sys; остальные ветви через чистый expr_text.
        match e {
            Expr::Var(v) => self.var_ref_text(v),
            Expr::Unary(op, inner) if *op != UnaryOp::Neg => {
                let x = self.expr(inner)?;
                Some(format!("{}[{}]", unary_op_text(*op), x))
            }
            Expr::Unary(_op, inner) => {
                let x = self.expr(inner)?;
                Some(if atom_needs_brackets(&x) {
                    format!("-[{x}]")
                } else {
                    format!("-{x}")
                })
            }
            Expr::Binary(BinaryOp::Atan2, y, x) => {
                let y = self.expr(y)?;
                let x = self.expr(x)?;
                Some(format!("ATAN[{y}]/[{x}]"))
            }
            Expr::Binary(op, a, b) => {
                let a = self.expr(a)?;
                let b = self.expr(b)?;
                Some(format!("[{a} {} {b}]", binary_op_text(*op)))
            }
            other => expr_text(other).map_err(|e| self.errors.push(e)).ok(),
        }
    }
}

/// Macro-B эмиссия программы: main-тело, затем subs (O-заголовки).
/// NLabel-subs (Haas M97) НЕ эмитируются отдельно: их тела парсер 12.1
/// оставил и в main (дублирование AST) — вторичная эмиссия продублировала
/// бы кадры; заметка фиксирует решение.
pub(crate) fn macro_b_emit(prog: &L2Program, target: TargetDialect) -> EmitOutcome {
    let mut sink = Sink::new(target);
    // Главный O-заголовок синтезируется: номер главной программы НЕ входит
    // в L2Program (парсер 12.1 поглощает первый O-заголовок молча) — без
    // строки `O1` re-parse принял бы ПЕРВЫЙ O-заголовок sub за заголовок
    // main и сдвинул бы границу main/subs (нарушение round-trip AST).
    // Номер 1 выбирается детерминированно и не участвует в AST.
    sink.emit("O1".to_string());
    sink.note(
        "main-o-number-synthesized",
        "main program number is not represented in L2Program (12.1 skeleton absorbs the first O-header) — synthesized `O1`; replace before production use".to_string(),
    );
    emit_stmt_list(&mut sink, &prog.stmts, &mut TagAllocator::new(), 0);

    for sub in &prog.subs {
        match &sub.target {
            SubTarget::ONumber(n) => {
                sink.set_line(sub.body.first().and_then(|s| s.src()));
                sink.emit(format!("O{n}"));
                emit_stmt_list(&mut sink, &sub.body, &mut TagAllocator::new(), 1);
            }
            // M97-тела живут в main (парсер 12.1 дублирует) — отдельно
            // не эмитируются; на Fanuc сам вызов M97 отклоняется на CallSub.
            SubTarget::NLabel(n) => sink.note(
                "m97-body-in-main",
                format!(
                    "M97 local sub N{n}: body kept in main (12.1 parser duplicates it); not emitted as separate sub"
                ),
            ),
            SubTarget::Named(name) => sink.refuse(format!(
                "named sub o<{name}> has no Macro B analog (O-number only) — honest refusal"
            )),
        }
    }

    EmitOutcome {
        text: sink.lines.join("\n") + "\n",
        notices: sink.notices,
        errors: sink.errors,
    }
}

/// Эмиссия списка stmt (main или тело sub). `depth` — вложенность WHILE
/// (лимит DO1..DO3 Fanuc/Haas).
///
/// LOOKAHEAD (инвариант парсера 12.1): строка `N41 G10 …` даёт пару
/// `[Label{41}, Raw("N41G10…")]` (Label + носитель ЦЕЛИКОЙ строки, с
/// N-префиксом в payload). Отдельная строка `N41` перед таким носителем
/// дала бы при re-parse ВТОРОЙ Label{41} (сдвиг AST) — поэтому для пары
/// «Label + носитель с тем же N-префиксом» метка не эмитируется отдельно:
/// носитель уже несёт её дословно.
fn emit_stmt_list(sink: &mut Sink, stmts: &[L2Stmt], tags: &mut TagAllocator, depth: usize) {
    for (i, stmt) in stmts.iter().enumerate() {
        if let L2Stmt::Label { n, .. } = stmt {
            let next_carrier_prefix = match stmts.get(i + 1) {
                Some(L2Stmt::L1Op(Op::Raw { payload_hex, .. })) => {
                    let text = decode_hex(payload_hex);
                    let t = text.trim_start();
                    if t.starts_with('N') || t.starts_with('n') {
                        Some(t[1..].trim_start().to_string())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(digits) = next_carrier_prefix {
                let n_str = n.to_string();
                if digits.starts_with(n_str.as_str())
                    && digits[n_str.len()..].starts_with(|c: char| !c.is_ascii_digit())
                {
                    // Метка уже в payload носителя — пропустить
                    // отдельную строку N{m}.
                    continue;
                }
            }
        }
        emit_stmt(sink, stmt, tags, depth);
    }
}

/// Эмиссия одного оператора Macro B.
fn emit_stmt(sink: &mut Sink, stmt: &L2Stmt, tags: &mut TagAllocator, depth: usize) {
    sink.set_line(stmt.src());
    match stmt {
        L2Stmt::Move {
            kind,
            target,
            feed,
            spindle,
            ..
        } => emit_move(sink, kind, target, feed.as_ref(), spindle.as_ref()),

        L2Stmt::WithTransform { transform, body, .. } => {
            emit_with_transform(sink, transform, body, tags, depth)
        }

        L2Stmt::Assign { target, value, .. } => emit_assign(sink, target, value),

        L2Stmt::IfGoto {
            cond,
            target_label,
            ..
        } => {
            // Macro B: IF [cond] GOTO n — прямая конструкция; реструктуризация
            // нужна только LinuxCNC (emit_linuxcnc).
            let c = match sink.expr(cond) {
                Some(t) => strip_outer_brackets(&t).to_string(),
                None => return,
            };
            sink.emit(format!("IF [{c}] GOTO {target_label}"));
        }

        L2Stmt::IfThen { cond, then_stmt, .. } => {
            let c = match sink.expr(cond) {
                Some(t) => strip_outer_brackets(&t).to_string(),
                None => return,
            };
            // Macro B: IF-THEN принимает только присваивание (парсер 12.1
            // порождает Assign; Haas bare-IF десугарен в тот же узел).
            // THEN ВСЕГДА эмитируется: bare-IF `IF [c] #3000=1` — синтаксис
            // Haas; Fanuc требует THEN (zapcon_g31 re-parse: «IF without
            // GOTO/THEN is not Fanuc Macro B syntax»). THEN-форма валидна
            // на обоих диалектах.
            match &**then_stmt {
                L2Stmt::Assign { target, value, .. } => {
                    let Some(t) = var_target_text(sink, target) else {
                        return;
                    };
                    let Some(v) = sink.expr(value) else {
                        return;
                    };
                    sink.emit(format!("IF [{c}] THEN {t}={v}"));
                }
                other => sink.refuse(format!(
                    "IF-THEN body {:?} is not an assignment — Macro B IF-THEN accepts assignments only (honest refusal)",
                    stmt_variant(other)
                )),
            }
        }

        L2Stmt::While { cond, do_id, body, .. } => {
            if depth >= 3 {
                sink.refuse(format!(
                    "WHILE DO nesting depth {depth} exceeds Macro B limit (DO1..DO3) — honest refusal"
                ));
                return;
            }
            let c = match sink.expr(cond) {
                Some(t) => strip_outer_brackets(&t).to_string(),
                None => return,
            };
            sink.emit(format!("WHILE [{c}] DO{do_id}"));
            emit_stmt_list(sink, body, tags, depth + 1);
            sink.emit(format!("END{do_id}"));
        }

        L2Stmt::CallSub {
            target,
            repeat,
            args,
            modal,
            ..
        } => emit_call_sub(sink, target, repeat.as_ref(), args, *modal),

        L2Stmt::RtcpMode {
            enabled,
            tool_length_h,
            ..
        } => {
            if *enabled {
                match tool_length_h {
                    Some(h) => sink.emit(format!("G43.4 H{h}")),
                    None => sink.emit("G43.4".to_string()),
                }
                sink.note(
                    "rtcp-dialect-option",
                    "G43.4 RTCP: Fanuc 30i — paid option; Haas NGC — UMC models. Verify machine option before use".to_string(),
                );
            } else {
                sink.emit("G49".to_string());
            }
        }

        L2Stmt::Label { n, .. } => sink.emit(format!("N{n}")),

        L2Stmt::Return { .. } => sink.emit("M99".to_string()),

        L2Stmt::L1Op(op) => emit_l1_op(sink, op),
    }
}

/// Имя варианта stmt (диагностика отказов).
fn stmt_variant(s: &L2Stmt) -> &'static str {
    match s {
        L2Stmt::Move { .. } => "Move",
        L2Stmt::WithTransform { .. } => "WithTransform",
        L2Stmt::Assign { .. } => "Assign",
        L2Stmt::IfGoto { .. } => "IfGoto",
        L2Stmt::IfThen { .. } => "IfThen",
        L2Stmt::While { .. } => "While",
        L2Stmt::CallSub { .. } => "CallSub",
        L2Stmt::RtcpMode { .. } => "RtcpMode",
        L2Stmt::Label { .. } => "Label",
        L2Stmt::Return { .. } => "Return",
        L2Stmt::L1Op(_) => "L1Op",
    }
}

/// Цель присваивания (VarRef с диалект-веткой Sys).
fn var_target_text(sink: &mut Sink, target: &VarRef) -> Option<String> {
    sink.var_ref_text(target)
}

/// Присваивание: `#target=значение`. RHS — канонический рендер (Binary —
/// со скобками; Const/Var — слитно). Форма `#100=[#101 + 10]` — допустимая
/// Macro B скобочная нормализация (парсеры 12.1 принимают).
fn emit_assign(sink: &mut Sink, target: &VarRef, value: &Expr) {
    let Some(t) = var_target_text(sink, target) else {
        return;
    };
    let Some(v) = sink.expr(value) else {
        return;
    };
    sink.emit(format!("{t}={v}"));
}

/// Ход: G0/G1/G2/G3 + слова позы (X Y Z A B C), дуга (I J K R), F/S.
/// Порядок слов канонический (AST не сохраняет исходный порядок — верbatim
/// порядок слов не входит в AST 12.1; re-parse AST-эквивалентен).
fn emit_move(
    sink: &mut Sink,
    kind: &MoveKind,
    pose: &ExprPose,
    feed: Option<&Expr>,
    spindle: Option<&Expr>,
) {
    let g = match &kind {
        MoveKind::Rapid => "G0",
        MoveKind::Linear => "G1",
        MoveKind::Arc { dir, .. } => match dir {
            Dir::Cw => "G2",
            Dir::Ccw => "G3",
        },
    };
    let mut line = String::from(g);
    // Поза: X Y Z A B C. Null-слово (`Y#0`) эмитируется ДОСЛОВНО: «слово
    // игнорируется» — семантика ИСПОЛНЕНИЯ (Приложение дока, таблица 2.1),
    // парсеры 12.1 сохраняют Move с Null-значением — пропуск слова сделал
    // бы кадр чисто-модальным (парсер молча отбрасывает) и сломал бы
    // round-trip AST.
    for (letter, word) in [
        ('X', &pose.x),
        ('Y', &pose.y),
        ('Z', &pose.z),
        ('A', &pose.a),
        ('B', &pose.b),
        ('C', &pose.c),
    ] {
        if let Some(e) = word {
            if matches!(e, Expr::Null) {
                sink.note(
                    "null-word-verbatim",
                    format!("{letter}#0 (Null): at runtime the word is ignored (doc table 2.1) — emitted verbatim for AST round-trip"),
                );
            }
            let Some(t) = sink.expr(e) else { return };
            line.push(' ');
            line.push_str(&attach_word(letter, &t));
        }
    }
    if let MoveKind::Arc { ijk, r, .. } = &kind {
        for (letter, w) in [('I', &ijk[0]), ('J', &ijk[1]), ('K', &ijk[2])] {
            if let Some(e) = w {
                let Some(t) = sink.expr(e) else { return };
                line.push(' ');
                line.push_str(&attach_word(letter, &t));
            }
        }
        if let Some(e) = r {
            let Some(t) = sink.expr(e) else { return };
            line.push(' ');
            line.push_str(&attach_word('R', &t));
        }
    }
    if let Some(f) = feed {
        let Some(t) = sink.expr(f) else { return };
        line.push(' ');
        line.push_str(&attach_word('F', &t));
    }
    if let Some(s) = spindle {
        let Some(t) = sink.expr(s) else { return };
        line.push(' ');
        line.push_str(&attach_word('S', &t));
    }
    sink.emit(line);
}

/// Блок трансформации: заголовок + тело + явная отмена. Корпусных мишеней
/// НЕТ (ценз: G68/G51 ×0) — эмиссия каноническая с notice о конвенции.
fn emit_with_transform(
    sink: &mut Sink,
    transform: &TransformKind,
    body: &[L2Stmt],
    tags: &mut TagAllocator,
    depth: usize,
) {
    match transform {
        TransformKind::Rotate2D {
            center,
            angle_deg,
            plane,
        } => {
            let (Some(cx), Some(cy)) = (sink.expr(&center[0]), sink.expr(&center[1])) else {
                return;
            };
            let Some(r) = sink.expr(angle_deg) else { return };
            let plane_code = match plane {
                Plane::XY => "G17",
                Plane::ZX => "G18",
                Plane::YZ => "G19",
            };
            sink.emit(format!("{plane_code} G68 X{cx} Y{cy} R{r}"));
            sink.note(
                "transform-convention",
                "G68 rotation: center/angle emitted in canonical order; plane selected explicitly; cancel G69 emitted after body".to_string(),
            );
            emit_stmt_list(sink, body, tags, depth);
            sink.emit("G69".to_string());
        }
        TransformKind::Scale { center, factor } => {
            let (Some(cx), Some(cy), Some(cz)) = (
                sink.expr(&center[0]),
                sink.expr(&center[1]),
                sink.expr(&center[2]),
            ) else {
                return;
            };
            let Some(p) = sink.expr(factor) else { return };
            sink.emit(format!("G51 X{cx} Y{cy} Z{cz} P{p}"));
            sink.note(
                "transform-convention",
                "G51 scaling: cancel G50 emitted after body".to_string(),
            );
            emit_stmt_list(sink, body, tags, depth);
            sink.emit("G50".to_string());
        }
        TransformKind::Mirror { axis, pivot } => {
            let Some(p) = sink.expr(pivot) else { return };
            let l = axis.letter();
            match sink.target {
                // Haas: G101 X0 (нулевое значение) отменяет зеркало по X.
                TargetDialect::Haas => {
                    sink.emit(format!("G101 {l}{p}"));
                    sink.note(
                        "transform-convention",
                        "G101 mirror (Haas): cancel emitted as G101 {axis}0 (zero pivot)".to_string(),
                    );
                    emit_stmt_list(sink, body, tags, depth);
                    sink.emit(format!("G101 {l}0"));
                }
                // Fanuc: G51.1 X0 — вкл; отмена — G50.1 (все зеркала).
                _ => {
                    sink.emit(format!("G51.1 {l}{p}"));
                    sink.note(
                        "transform-convention",
                        "G51.1 mirror (Fanuc): cancel G50.1 emitted after body (cancels all mirrors)".to_string(),
                    );
                    emit_stmt_list(sink, body, tags, depth);
                    sink.emit("G50.1".to_string());
                }
            }
        }
        TransformKind::TiltedWorkPlane { origin, euler } => {
            // G68.2 X Y Z R1 R2 R3 — эйлеровы углы в R1/R2/R3 (порядок
            // применения углов — конвенция 30i; notice обязателен).
            let (Some(ox), Some(oy), Some(oz)) = (
                sink.expr(&origin[0]),
                sink.expr(&origin[1]),
                sink.expr(&origin[2]),
            ) else {
                return;
            };
            let (Some(r1), Some(r2), Some(r3)) = (
                sink.expr(&euler[0]),
                sink.expr(&euler[1]),
                sink.expr(&euler[2]),
            ) else {
                return;
            };
            sink.emit(format!("G68.2 X{ox} Y{oy} Z{oz} R1{r1} R2{r2} R3{r3}"));
            sink.note(
                "transform-convention",
                "G68.2 TWP: euler angle application order is 30i-convention (R1,R2,R3); cancel G69 emitted after body".to_string(),
            );
            emit_stmt_list(sink, body, tags, depth);
            sink.emit("G69".to_string());
        }
    }
}

/// Вызов подпрограммы (Macro B): M98 (без аргументов) / G65 (с аргументами)
/// / G66 (модальный) / M97 (Haas N-метка; на Fanuc — отказ).
fn emit_call_sub(
    sink: &mut Sink,
    target: &SubTarget,
    repeat: Option<&Expr>,
    args: &[(u8, Expr)],
    modal: bool,
) {
    let l_word = match repeat {
        // attach_word уже ставит букву L (для не-атомов — в скобках `L[expr]`).
        Some(e) => match sink.expr(e) {
            Some(t) => format!(" {}", attach_word('L', &t)),
            None => return,
        },
        None => String::new(),
    };
    let mut arg_text = String::new();
    for (letter, e) in args {
        let Some(t) = sink.expr(e) else { return };
        arg_text.push(' ');
        arg_text.push_str(&attach_word(*letter as char, &t));
    }
    match target {
        SubTarget::ONumber(n) => {
            if modal {
                sink.emit(format!("G66 P{n}{l_word}{arg_text}"));
                sink.note(
                    "modal-call",
                    "G66 modal call: executes after every motion block until G67 (cancel is a carrier — 12.4 contract note, amendment v0.1.4)".to_string(),
                );
            } else if args.is_empty() {
                sink.emit(format!("M98 P{n}{l_word}"));
            } else {
                sink.emit(format!("G65 P{n}{l_word}{arg_text}"));
            }
        }
        SubTarget::NLabel(n) => match sink.target {
            TargetDialect::Haas => sink.emit(format!("M97 P{n}{l_word}")),
            _ => sink.refuse(format!(
                "M97 P{n} local call (Haas) has no Fanuc analog — honest refusal"
            )),
        },
        SubTarget::Named(name) => sink.refuse(format!(
            "named sub o<{name}> call has no Macro B analog (O-number only) — honest refusal"
        )),
    }
}

/// L1-опы: ProgramEnd/Stop/ToolChange — канонические кадры; Dwell —
/// диалект-единицы (амендрмент v0.1.2: Fanuc P = мс, Haas P-dot-rule);
/// Raw — verbatim-носитель с линтом диалект-совместимости.
fn emit_l1_op(sink: &mut Sink, op: &Op) {
    match op {
        Op::ProgramEnd { .. } => sink.emit("M30".to_string()),
        Op::ProgramStop { optional, .. } => sink.emit(if *optional {
            "M01".to_string()
        } else {
            "M0".to_string()
        }),
        Op::ToolChange { tool, .. } => sink.emit(format!("T{tool} M6")),
        Op::Dwell { seconds, .. } => {
            let word = match sink.target {
                // Fanuc: P = миллисекунды всегда (P2000 = 2 s).
                TargetDialect::Fanuc => format!("G4 P{}", fmt_ms(seconds)),
                // Haas: P-dot-rule — секунды с точкой (P2. = 2 s).
                TargetDialect::Haas => format!("G4 P{}", fmt_secs_dot(seconds)),
                _ => unreachable!("Macro B sink never sees LinuxCnc"),
            };
            sink.emit(word);
            sink.note(
                "dwell-unit-conversion",
                format!(
                    "Dwell {:.4}s → {} G4 word (BUG-D2 dialect units: Fanuc P=ms, Haas P-dot-rule); X/U forms normalized to P",
                    seconds.value(),
                    sink.target.name()
                ),
            );
        }
        Op::Raw { payload_hex, .. } => {
            let text = decode_hex(payload_hex);
            match carrier_compat(&text, sink.target) {
                CarrierVerdict::Ok => sink.emit(text),
                CarrierVerdict::Notice { reason } => {
                    sink.note("carrier-verbatim", reason);
                    sink.emit(text);
                }
                CarrierVerdict::Refused { reason } => {
                    sink.refuse(reason);
                    sink.emit(text); // текст сохраняем для диагностики; нога отказана
                }
            }
        }
        // Остальные L1-опы (Rapid/Linear/Arc с точками) в L2 AST не
        // встречаются (парсеры 12.1 не производят геометрические L1-опы) —
        // честный отказ на всякий случай.
        other => sink.refuse(format!(
            "geometric L1 op {other:?} in L2 program — not producible by 12.1 parsers, honest refusal"
        )),
    }
}

/// Формат миллисекунд для Fanuc G4 P: ms = s*1000, дробная часть после
/// 6 знаков срезается (f64-шум 1.5000000000000002 → "1.5").
fn fmt_ms(secs: &Num) -> String {
    fmt_trimmed(secs.value() * 1000.0)
}

/// Формат секунд с точкой для Haas P-dot-rule: `2.` (целые) / `0.25`.
fn fmt_secs_dot(secs: &Num) -> String {
    let v = secs.value();
    if v.fract() == 0.0 {
        format!("{v}.")
    } else {
        fmt_trimmed(v)
    }
}

/// Число с обрезкой шума f64 (6 значимых знаков после точки).
fn fmt_trimmed(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    s.to_string()
}

fn decode_hex(payload_hex: &str) -> String {
    let bytes: Vec<u8> = (0..payload_hex.len() / 2)
        .map(|i| u8::from_str_radix(&payload_hex[i * 2..i * 2 + 2], 16).unwrap_or(b'?'))
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

// ===========================================================================
// Линт носителей (verbatim-совместимость по целевому диалекту)
// ===========================================================================

/// Вердикт линта носителя: Ok / Notice / Refused (см. модульную шапку).
pub(crate) enum CarrierVerdict {
    Ok,
    Notice { reason: String },
    Refused { reason: String },
}

/// Синтаксис-несовместимость → отказ; семантика-неверифицируемость → notice.
/// Проверки по подстрокам носителя (носитель — целая строка кадра).
pub(crate) fn carrier_compat(text: &str, target: TargetDialect) -> CarrierVerdict {
    // Носители с вердиктом Ok проходят verbatim без notice: payload
    // байт-в-байт сохраняется hex'ом (round-trip AST держится); «не молча»
    // относится к вердикту совместимости — помечаются только расхождения.
    let refused = |reason: &str| CarrierVerdict::Refused {
        reason: reason.to_string(),
    };
    let notice = |reason: &str| CarrierVerdict::Notice {
        reason: reason.to_string(),
    };
    let upper = text.to_ascii_uppercase();
    match target {
        TargetDialect::Fanuc => {
            if upper.contains(';') {
                return refused("`;`-comment (Haas/LinuxCNC EOB-comment) is not valid Fanuc syntax");
            }
            if starts_with_control_keyword(&upper) {
                return refused(
                    "carrier is a bare-IF/WHILE control line (Haas bare-IF without THEN, e.g. `IF [#4 LT 0] S800 M04`) — Fanuc rejects bare-IF (allow_bare_if=false) and LinuxCNC has no bare-IF either; honest refusal",
                );
            }
            if has_comma_word(&upper) {
                return notice(
                    "comma-word (`,C`/`,R` Fanuc-LATHE corner form): on Fanuc MILL builds semantics unverified — carried verbatim with operator awareness",
                );
            }
            if has_any_g(&upper, &["G154"]) || has_g_range(&upper, 110.0, 129.0) {
                return refused(
                    "Haas extended WCS (G154/G110-G129) has no direct Fanuc code (Fanuc analog G54.1 Pn — slot translation out of 12.4 scope)",
                );
            }
            if has_word(&upper, "M97") {
                return refused("M97 local call is Haas-only (no Fanuc analog)");
            }
            if has_any_g(&upper, &["G103"]) {
                return notice(
                    "G103 block-delete suppress: no verified Fanuc analog (Fanuc block-delete behavior is parameter-set) — carried verbatim, semantics unverified",
                );
            }
            CarrierVerdict::Ok
        }
        TargetDialect::Haas => {
            if has_comma_word(&upper) {
                return notice(
                    "comma-word (`,C`/`,R` Fanuc-lathe corner form): Haas mill semantics unverified — carried verbatim",
                );
            }
            if has_any_g(&upper, &["G43.4"]) {
                return notice(
                    "G43.4 RTCP: Haas NGC supports it on UMC (5-axis) models only — verify machine",
                );
            }
            CarrierVerdict::Ok
        }
        TargetDialect::LinuxCnc => {
            if starts_with_control_keyword(&upper) {
                return refused(
                    "carrier is a bare-IF/WHILE control line (Haas bare-IF without THEN) — not LinuxCNC syntax (O-if blocks only); honest refusal",
                );
            }
            if has_comma_word(&upper) {
                return refused("comma-words (`,C`/`,R`) are not LinuxCNC syntax");
            }
            if has_any_g(&upper, &["G154"]) || has_g_range(&upper, 110.0, 129.0) {
                return refused("Haas extended WCS (G154/G110-G129) does not exist in LinuxCNC");
            }
            if has_word(&upper, "M97") || has_word(&upper, "M98") || has_word(&upper, "M99") {
                return refused(
                    "M97/M98/M99 subroutine system is Fanuc/Haas; LinuxCNC uses O-word call/endsub — carrier translation out of 12.4 scope",
                );
            }
            if has_any_g(&upper, &["G65", "G66", "G67"]) {
                return refused("G65/G66/G67 macro calls do not exist in LinuxCNC (O-call instead)");
            }
            if has_any_g(&upper, &["G31"]) {
                return refused(
                    "G31 skip-probe is Fanuc/Haas; LinuxCNC probing is the G38.2-G38.5 family — probe carrier translation out of 12.4 scope",
                );
            }
            if has_any_g(&upper, &["G103"]) {
                return refused("G103 block-delete suppress does not exist in LinuxCNC");
            }
            if has_any_g(&upper, &["G43.4"]) {
                return refused("G43.4 RTCP does not exist in LinuxCNC (no RTCP in rs274ngc)");
            }
            CarrierVerdict::Ok
        }
    }
}

/// Есть ли `,буква`-слово в кадре (Fanuc-lathe corner-формы). Скобочные
/// комментарии вырезаются перед проверкой (false-positive на
/// `(debug, interpol…)` LinuxCNC-remark'ов).
fn has_comma_word(upper: &str) -> bool {
    let stripped = strip_paren_comments(upper);
    let b = stripped.as_bytes();
    b.windows(2)
        .any(|w| w[0] == b',' && w[1].is_ascii_alphabetic())
}

/// Носитель начинается с ключевого слова потока управления (IF/WHILE/DO/
/// END/GOTO): такие строки — Haas bare-IF-носители (парсер 12.1 выносит
/// не-присваивающий bare-IF в носитель); на Fanuc/LCNC синтаксис невалиден.
fn starts_with_control_keyword(upper: &str) -> bool {
    for kw in ["IF", "WHILE", "GOTO", "DO", "END"] {
        if let Some(rest) = upper.strip_prefix(kw) {
            // Граница слова: за ключевым словом — пробел/скобка/цифра.
            if rest.starts_with(|c: char| c.is_whitespace() || c == '[' || c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// Вырезать `(...)`-комментарии (скобки не вложим — как lib.rs).
fn strip_paren_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut depth = 0usize;
    for ch in src.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Есть ли слово `M<digits>` (префикс) — грубый тест на M97/M98/M99.
fn has_word(upper: &str, word: &str) -> bool {
    upper.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| tok == word)
}

/// Есть ли G-код из списка (`G31` как токен с границей по неалфанумерику).
fn has_any_g(upper: &str, codes: &[&str]) -> bool {
    upper
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| codes.contains(&tok))
}

/// Есть ли G-код в диапазоне [lo, hi] (токен G<число>).
fn has_g_range(upper: &str, lo: f64, hi: f64) -> bool {
    upper
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| {
            tok.len() >= 2
                && (tok.starts_with('G'))
                && tok[1..]
                    .parse::<f64>()
                    .map(|v| (lo..=hi).contains(&v))
                    .unwrap_or(false)
        })
}

// ===========================================================================
// semantic_diff — сравнение AST с точностью до спанов/текста Num (DoD 12.4:
// «semantic_diff на L1-Raw-носителях»); носители сравниваются БАЙТ-В-БАЙТ
// (payload_hex lossless).
// ===========================================================================

/// Семантическое равенство L2-программ: структура AST эквивалентна, спаны
/// игнорируются, числа сравниваются по значению (допуск 1e-9 относительный),
/// носители Op::Raw — по payload_hex (lossless). Используется round-trip
/// DoD (tests/roundtrip_l2.rs).
pub fn ast_semantic_eq(a: &L2Program, b: &L2Program) -> bool {
    stmts_eq(&a.stmts, &b.stmts)
        && a.subs.len() == b.subs.len()
        && a.subs
            .iter()
            .zip(&b.subs)
            .all(|(x, y)| x.target == y.target && stmts_eq(&x.body, &y.body))
}

fn stmts_eq(a: &[L2Stmt], b: &[L2Stmt]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| stmt_eq(x, y))
}

fn num_eq(a: &Num, b: &Num) -> bool {
    let scale = a.value().abs().max(b.value().abs()).max(1.0);
    (a.value() - b.value()).abs() <= 1e-9 * scale
}

pub(crate) fn expr_eq(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Const(x), Expr::Const(y)) => num_eq(x, y),
        (Expr::Null, Expr::Null) => true,
        (Expr::Var(x), Expr::Var(y)) => var_eq(x, y),
        (Expr::Unary(op1, x1), Expr::Unary(op2, x2)) => op1 == op2 && expr_eq(x1, x2),
        (
            Expr::Binary(op1, a1, b1),
            Expr::Binary(op2, a2, b2),
        ) => op1 == op2 && expr_eq(a1, a2) && expr_eq(b1, b2),
        _ => false,
    }
}

fn var_eq(a: &VarRef, b: &VarRef) -> bool {
    match (a, b) {
        (VarRef::Direct(x), VarRef::Direct(y)) => x == y,
        (VarRef::Named(x), VarRef::Named(y)) => x == y,
        (VarRef::Sys(x), VarRef::Sys(y)) => x == y,
        (VarRef::Indirect(x), VarRef::Indirect(y)) => expr_eq(x, y),
        _ => false,
    }
}

fn pose_eq(a: &ExprPose, b: &ExprPose) -> bool {
    let cmp = |x: &Option<Expr>, y: &Option<Expr>| match (x, y) {
        (None, None) => true,
        (Some(x), Some(y)) => expr_eq(x, y),
        _ => false,
    };
    cmp(&a.x, &b.x)
        && cmp(&a.y, &b.y)
        && cmp(&a.z, &b.z)
        && cmp(&a.a, &b.a)
        && cmp(&a.b, &b.b)
        && cmp(&a.c, &b.c)
}

fn stmt_eq(a: &L2Stmt, b: &L2Stmt) -> bool {
    match (a, b) {
        (
            L2Stmt::Move {
                kind: k1,
                target: t1,
                feed: f1,
                spindle: s1,
                ..
            },
            L2Stmt::Move {
                kind: k2,
                target: t2,
                feed: f2,
                spindle: s2,
                ..
            },
        ) => {
            move_kind_eq(k1, k2)
                && pose_eq(t1, t2)
                && opt_expr_eq(f1.as_ref(), f2.as_ref())
                && opt_expr_eq(s1.as_ref(), s2.as_ref())
        }
        (
            L2Stmt::WithTransform {
                transform: x1,
                body: b1,
                ..
            },
            L2Stmt::WithTransform {
                transform: x2,
                body: b2,
                ..
            },
        ) => transform_eq(x1, x2) && stmts_eq(b1, b2),
        (
            L2Stmt::Assign {
                target: t1,
                value: v1,
                ..
            },
            L2Stmt::Assign {
                target: t2,
                value: v2,
                ..
            },
        ) => var_eq(t1, t2) && expr_eq(v1, v2),
        (
            L2Stmt::IfGoto {
                cond: c1,
                target_label: l1,
                ..
            },
            L2Stmt::IfGoto {
                cond: c2,
                target_label: l2,
                ..
            },
        ) => l1 == l2 && expr_eq(c1, c2),
        (
            L2Stmt::IfThen {
                cond: c1,
                then_stmt: s1,
                ..
            },
            L2Stmt::IfThen {
                cond: c2,
                then_stmt: s2,
                ..
            },
        ) => expr_eq(c1, c2) && stmt_eq(s1, s2),
        (
            L2Stmt::While {
                cond: c1,
                body: b1,
                ..
            },
            L2Stmt::While {
                cond: c2,
                body: b2,
                ..
            },
        ) => {
            // do_id НЕ сравнивается (semantic_diff-класс метаданных): в
            // Macro B это идентификатор пары DO/END, в LinuxCNC — АРТЕФАКТ
            // числового O-тега (parser_linuxcnc::do_id_of: ONumber 1..=3 →
            // номер, прочее → 1). Парность DO/END гарантируется СТРУКТУРОЙ
            // тела (незакрытый DO = ошибка парсера), поэтому do_id не несёт
            // семантики сверх вложенности body. Эмиссия LinuxCNC ставит
            // свежие уникальные теги (9000+) — требование уникальности
            // O-номеров блоков — do_id нормализуется в 1 (см. corpus
            // gridprobe O1/O2 → fresh tags).
            expr_eq(c1, c2) && stmts_eq(b1, b2)
        }
        (
            L2Stmt::CallSub {
                target: t1,
                repeat: r1,
                args: a1,
                modal: m1,
                ..
            },
            L2Stmt::CallSub {
                target: t2,
                repeat: r2,
                args: a2,
                modal: m2,
                ..
            },
        ) => {
            t1 == t2
                && m1 == m2
                && opt_expr_eq(r1.as_ref(), r2.as_ref())
                && a1.len() == a2.len()
                && a1.iter().zip(a2).all(|((l1, e1), (l2, e2))| l1 == l2 && expr_eq(e1, e2))
        }
        (
            L2Stmt::RtcpMode {
                enabled: e1,
                tool_length_h: h1,
                ..
            },
            L2Stmt::RtcpMode {
                enabled: e2,
                tool_length_h: h2,
                ..
            },
        ) => e1 == e2 && h1 == h2,
        (L2Stmt::Label { n: n1, .. }, L2Stmt::Label { n: n2, .. }) => n1 == n2,
        (L2Stmt::Return { .. }, L2Stmt::Return { .. }) => true,
        (L2Stmt::L1Op(x), L2Stmt::L1Op(y)) => l1_op_eq(x, y),
        _ => false,
    }
}

fn opt_expr_eq(a: Option<&Expr>, b: Option<&Expr>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => expr_eq(x, y),
        _ => false,
    }
}

fn move_kind_eq(a: &MoveKind, b: &MoveKind) -> bool {
    match (a, b) {
        (MoveKind::Rapid, MoveKind::Rapid) | (MoveKind::Linear, MoveKind::Linear) => true,
        (
            MoveKind::Arc {
                ijk: i1,
                r: r1,
                dir: d1,
            },
            MoveKind::Arc {
                ijk: i2,
                r: r2,
                dir: d2,
            },
        ) => {
            d1 == d2
                && opt_expr_eq(r1.as_ref(), r2.as_ref())
                && i1
                    .iter()
                    .zip(i2.iter())
                    .all(|(x, y)| opt_expr_eq(x.as_ref(), y.as_ref()))
        }
        _ => false,
    }
}

fn transform_eq(a: &TransformKind, b: &TransformKind) -> bool {
    match (a, b) {
        (
            TransformKind::Rotate2D {
                center: c1,
                angle_deg: a1,
                plane: p1,
            },
            TransformKind::Rotate2D {
                center: c2,
                angle_deg: a2,
                plane: p2,
            },
        ) => p1 == p2 && expr_eq(a1, a2) && expr_eq(&c1[0], &c2[0]) && expr_eq(&c1[1], &c2[1]),
        (TransformKind::Scale { center: c1, factor: f1 }, TransformKind::Scale { center: c2, factor: f2 }) => {
            expr_eq(f1, f2)
                && (0..3).all(|i| expr_eq(&c1[i], &c2[i]))
        }
        (
            TransformKind::Mirror { axis: a1, pivot: p1 },
            TransformKind::Mirror { axis: a2, pivot: p2 },
        ) => a1 == a2 && expr_eq(p1, p2),
        (
            TransformKind::TiltedWorkPlane { origin: o1, euler: e1 },
            TransformKind::TiltedWorkPlane { origin: o2, euler: e2 },
        ) => {
            (0..3).all(|i| expr_eq(&o1[i], &o2[i]) && expr_eq(&e1[i], &e2[i]))
        }
        _ => false,
    }
}

/// L1-оп: геометрические сравниваются по числам с допуском (SPANS/state
/// провенанс игнорируются — semantic_diff-класс, EQUIVALENCE §5); Raw —
/// по payload_hex БАЙТ-В-БАЙТ (носитель обязан пережить round-trip).
fn l1_op_eq(a: &Op, b: &Op) -> bool {
    match (a, b) {
        (
            Op::Rapid {
                start: s1,
                end: e1,
                state: st1,
                ..
            },
            Op::Rapid {
                start: s2,
                end: e2,
                state: st2,
                ..
            },
        )
        | (
            Op::Linear {
                start: s1,
                end: e1,
                state: st1,
                ..
            },
            Op::Linear {
                start: s2,
                end: e2,
                state: st2,
                ..
            },
        ) => {
            start_eq(s1, s2)
                && point_eq(e1, e2)
                && state_eq(st1, st2)
        }
        (
            Op::Arc {
                start: s1,
                end: e1,
                center: c1,
                dir: d1,
                sweep_deg: sw1,
                pitch: p1,
                state: st1,
                ..
            },
            Op::Arc {
                start: s2,
                end: e2,
                center: c2,
                dir: d2,
                sweep_deg: sw2,
                pitch: p2,
                state: st2,
                ..
            },
        ) => {
            d1 == d2
                && start_eq(s1, s2)
                && point_eq(e1, e2)
                && point_eq(c1, c2)
                && num_eq(sw1, sw2)
                && num_eq(p1, p2)
                && state_eq(st1, st2)
        }
        (Op::Dwell { seconds: s1, .. }, Op::Dwell { seconds: s2, .. }) => num_eq(s1, s2),
        (Op::ToolChange { tool: t1, .. }, Op::ToolChange { tool: t2, .. }) => t1 == t2,
        (
            Op::ProgramStop { optional: o1, .. },
            Op::ProgramStop { optional: o2, .. },
        ) => o1 == o2,
        (Op::ProgramEnd { .. }, Op::ProgramEnd { .. }) => true,
        (
            Op::WcsDefine { p: p1, origin: o1, .. },
            Op::WcsDefine { p: p2, origin: o2, .. },
        ) => p1 == p2 && point_eq(o1, o2),
        (
            Op::WcsSetHere { p: p1, delta: d1, .. },
            Op::WcsSetHere { p: p2, delta: d2, .. },
        ) => p1 == p2 && vec_eq(d1, d2),
        (
            Op::Raw { payload_hex: x1, .. },
            Op::Raw { payload_hex: x2, .. },
        ) => x1 == x2,
        _ => false,
    }
}

fn start_eq(
    a: &eid_core::ast::StartPoint,
    b: &eid_core::ast::StartPoint,
) -> bool {
    use eid_core::ast::StartPoint;
    match (a, b) {
        (StartPoint::Unknown, StartPoint::Unknown) => true,
        (StartPoint::Known(x), StartPoint::Known(y)) => point_eq(x, y),
        _ => false,
    }
}

fn point_eq(
    a: &eid_core::ast::Point3,
    b: &eid_core::ast::Point3,
) -> bool {
    vec_eq(a, b)
}

fn vec_eq(a: &eid_core::ast::Point3, b: &eid_core::ast::Point3) -> bool {
    // Point3 = {x,y,z: Num} (eid-core): сравнение компонент по значению
    // (num_eq: допуск 1e-9 относительный, текст verbatim игнорируется).
    num_eq(&a.x, &b.x) && num_eq(&a.y, &b.y) && num_eq(&a.z, &b.z)
}

fn state_eq(a: &eid_core::ast::State, b: &eid_core::ast::State) -> bool {
    let feed = match (&a.feed, &b.feed) {
        (None, None) => true,
        (Some(x), Some(y)) => num_eq(x, y),
        _ => false,
    };
    feed
        && a.spindle == b.spindle
        && a.coolant == b.coolant
        && a.tool == b.tool
        && a.frame == b.frame
        && a.wcs == b.wcs
        // feed_restated — провенанс (EQUIVALENCE §5): не сравнивается.
}

// ===========================================================================
// Заготовка аллокатора уникальных o-тегов (используется emit_linuxcnc;
// живёт здесь, чтобы Macro-B ветка не зависела от LinuxCNC-модуля).
// ===========================================================================

/// Детерминированный аллокатор уникальных o-тегов LinuxCNC (реструктуризация
/// GOTO/IfThen/repeat). Стартует с 9000 (corpus sub-номера малы) и растёт
/// монотонно — гарантия уникальности внутри одной эмиссии.
#[derive(Debug, Clone)]
pub(crate) struct TagAllocator {
    next: u32,
}

impl TagAllocator {
    pub(crate) fn new() -> Self {
        Self { next: 9000 }
    }
    pub(crate) fn alloc(&mut self) -> u32 {
        let t = self.next;
        self.next += 1;
        t
    }
}

// ===========================================================================
// Тесты 12.4: таблица SysVar-маппинга ×варианты, Macro-B эмиссия
// (emit→parse AST-eq), честные отказы, dwell-единицы
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::placeholder_span;
    use crate::parser_fanuc;
    use crate::parser_haas;

    fn vs(n: u32) -> SysVar {
        SysVar::VendorSpecific(n)
    }

    // ---------- ТАБЛИЦА SysVar-маппинга (ядро DoD 12.4) ----------

    #[test]
    fn sysvar_positions_mapped_all_dialects() {
        // AbsMachinePos/SkipPos: #5021+/`#5061+ — ОДИНАКОВЫЕ номера во всех
        // трёх диалектах (Fanuc/Haas NGC Fanuc-совместим; LinuxCNC docs
        // #5021-#5029 machine pos, #5061-#5069 probe result).
        for target in [TargetDialect::Fanuc, TargetDialect::Haas, TargetDialect::LinuxCnc] {
            for (k, axis) in [
                (0, Axis::X),
                (1, Axis::Y),
                (2, Axis::Z),
                (3, Axis::A),
                (4, Axis::B),
                (5, Axis::C),
            ] {
                let d = map_sysvar(&SysVar::AbsMachinePos(axis), target);
                assert_eq!(
                    d,
                    SysVarDecision::Mapped {
                        number: 5021 + k,
                        via: MapVia::Direct
                    },
                    "{target:?} AbsMachinePos({axis:?})"
                );
                let d = map_sysvar(&SysVar::SkipPos(axis), target);
                assert_eq!(
                    d,
                    SysVarDecision::Mapped {
                        number: 5061 + k,
                        via: MapVia::Direct
                    },
                    "{target:?} SkipPos({axis:?})"
                );
            }
        }
    }

    #[test]
    fn sysvar_workpos_via_offset_for_linuxcnc() {
        // WorkPos: Fanuc/Haas #5041+ (direct); LinuxCNC #5420+ (via-offset,
        // docs: «#5420-#5428 current position in active CS»).
        for target in [TargetDialect::Fanuc, TargetDialect::Haas] {
            let d = map_sysvar(&SysVar::WorkPos(Axis::Z), target);
            assert_eq!(
                d,
                SysVarDecision::Mapped {
                    number: 5043,
                    via: MapVia::Direct
                }
            );
        }
        let d = map_sysvar(&SysVar::WorkPos(Axis::Z), TargetDialect::LinuxCnc);
        assert_eq!(
            d,
            SysVarDecision::Mapped {
                number: 5422,
                via: MapVia::ViaOffset
            }
        );
    }

    #[test]
    fn sysvar_modal_group_unmappable_on_linuxcnc() {
        for target in [TargetDialect::Fanuc, TargetDialect::Haas] {
            let d = map_sysvar(&SysVar::ModalGroup(3), target);
            assert_eq!(
                d,
                SysVarDecision::Mapped {
                    number: 4003,
                    via: MapVia::Direct
                }
            );
        }
        // LinuxCNC не имеет модальных переменных 4xxx — честный отказ.
        let d = map_sysvar(&SysVar::ModalGroup(3), TargetDialect::LinuxCnc);
        assert!(matches!(d, SysVarDecision::Unmappable { .. }));
    }

    #[test]
    fn sysvar_wcs_table() {
        // p=0 (external): Fanuc/Haas #5201+; LinuxCNC — unmappable.
        assert_eq!(
            map_sysvar(&SysVar::WcsOffset { p: 0, axis: Axis::X }, TargetDialect::Fanuc),
            SysVarDecision::Mapped {
                number: 5201,
                via: MapVia::Direct
            }
        );
        assert!(matches!(
            map_sysvar(&SysVar::WcsOffset { p: 0, axis: Axis::X }, TargetDialect::LinuxCnc),
            SysVarDecision::Unmappable { .. }
        ));
        // p=1..6 (G54..G59): #5221+20(p−1)+off — ОДИНАКОВО во всех трёх
        // (LinuxCNC CS-базы совпадают с Fanuc; Haas 52xx Fanuc-совместим).
        for target in [TargetDialect::Fanuc, TargetDialect::Haas, TargetDialect::LinuxCnc] {
            assert_eq!(
                map_sysvar(&SysVar::WcsOffset { p: 1, axis: Axis::X }, target),
                SysVarDecision::Mapped {
                    number: 5221,
                    via: MapVia::Direct
                }
            );
            assert_eq!(
                map_sysvar(&SysVar::WcsOffset { p: 6, axis: Axis::C }, target),
                SysVarDecision::Mapped {
                    number: 5326,
                    via: MapVia::Direct
                }
            );
        }
        // p=7..26 (Haas G110-G129 / Fanuc G54.1P): нумерация опциона/станок-
        // зависима ('?' реестра) — честный отказ на всех диалектах.
        for target in [TargetDialect::Fanuc, TargetDialect::Haas, TargetDialect::LinuxCnc] {
            assert!(matches!(
                map_sysvar(&SysVar::WcsOffset { p: 7, axis: Axis::X }, target),
                SysVarDecision::Unmappable { .. }
            ));
        }
    }

    #[test]
    fn sysvar_vendor_passthrough_not_silent() {
        // VendorSpecific: PassThrough с notice (номер сохранён — round-trip
        // держится; семантика помечена «не молча»), включая alarm-диапазон.
        for target in [TargetDialect::Fanuc, TargetDialect::Haas, TargetDialect::LinuxCnc] {
            let d = map_sysvar(&vs(3000), target);
            match &d {
                SysVarDecision::PassThrough { number, reason } => {
                    assert_eq!(*number, 3000);
                    assert!(reason.contains("Renishaw"), "{target:?}: {reason}");
                }
                other => panic!("{target:?}: expected PassThrough, got {other:?}"),
            }
            let d = map_sysvar(&vs(1133), target);
            assert!(matches!(d, SysVarDecision::PassThrough { number: 1133, .. }));
        }
    }

    // ---------- Macro-B эмиссия: emit→parse AST-eq ----------

    #[test]
    fn macro_b_emit_parse_roundtrip_mini() {
        let src = "#100=5.\n\
                   WHILE [#100 LT 10] DO1\n\
                   G1 X#100 F#101\n\
                   #100=[#100 + 1]\n\
                   END1\n\
                   IF [#100 GE 10] GOTO 20\n\
                   G0 Y#0\n\
                   N20 M30\n";
        let p0 = parser_fanuc::parse_program(src).unwrap();
        for target in [TargetDialect::Fanuc, TargetDialect::Haas] {
            let out = emit(&p0, target);
            assert!(out.ok(), "{target:?}: {:?}", out.errors);
            let p1 = if target == TargetDialect::Fanuc {
                parser_fanuc::parse_program(&out.text).unwrap()
            } else {
                parser_haas::parse_program(&out.text).unwrap()
            };
            assert!(
                ast_semantic_eq(&p0, &p1),
                "{target:?}: emit→parse semantic_diff != 0\n{}",
                out.text
            );
        }
    }

    #[test]
    fn g65_nonlinear_args_preserved() {
        // BUG-D1 таблица (амендрмент v0.1.2): Z→#26 D→#7 S→#19 F→#9 —
        // буквы-аргументы переживают эмиссию дословно.
        let src = "O7981\nG65 P7981 Z-7.88 D-.4 S6000 F60.\nM99\n";
        let p0 = parser_fanuc::parse_program(src).unwrap();
        let out = emit(&p0, TargetDialect::Fanuc);
        assert!(out.ok());
        assert!(
            out.text.contains("G65 P7981 Z-7.88 D-.4 S6000 F60."),
            "args must be emitted verbatim:\n{}",
            out.text
        );
        let p1 = parser_fanuc::parse_program(&out.text).unwrap();
        assert!(ast_semantic_eq(&p0, &p1));
    }

    #[test]
    fn m98_with_repeat_and_g31_carrier_roundtrip() {
        // DoD-ядро: G31-probing носитель переживает round-trip байт-в-байт.
        let src = "O100\nG31 X10. F50.\nM98 P200 L3\nM99\n";
        let p0 = parser_fanuc::parse_program(src).unwrap();
        let out = emit(&p0, TargetDialect::Haas);
        assert!(out.ok(), "{:?}", out.errors);
        assert!(
            out.text.contains("G31 X10. F50."),
            "probe carrier verbatim:\n{}",
            out.text
        );
        assert!(out.text.contains("M98 P200 L3"), "emitted:\n{}", out.text);
        let p1 = parser_haas::parse_program(&out.text).unwrap();
        assert!(ast_semantic_eq(&p0, &p1));
    }

    #[test]
    fn haas_bare_if_emits_then_form_for_fanuc() {
        // Haas bare-IF-assign (десугарен в IfThen) → Fanuc-мишень обязана
        // получить THEN-форму (Fanuc bare-IF невалиден).
        let src = "O9000\nIF [#3026 GT 0] #3026=0\nM99\n";
        let p0 = parser_haas::parse_program(src).unwrap();
        let out = emit(&p0, TargetDialect::Fanuc);
        assert!(out.ok(), "{:?}", out.errors);
        assert!(
            out.text.contains("IF [#3026 GT 0] THEN #3026=0"),
            "THEN-form required:\n{}",
            out.text
        );
        let p1 = parser_fanuc::parse_program(&out.text).unwrap();
        assert!(ast_semantic_eq(&p0, &p1));
    }

    #[test]
    fn dwell_dialect_units() {
        // BUG-D2: Fanuc P=мс (2000 → 2.0s); Haas P-dot-rule (P2. = 2s).
        // `G4 P2000` fanuc → emit haas → `G4 P2.` → обратный round-trip.
        let src = "O1\nG4 P2000\nM30\n";
        let p0 = parser_fanuc::parse_program(src).unwrap();
        let out_h = emit(&p0, TargetDialect::Haas);
        assert!(out_h.text.contains("G4 P2."), "Haas P-dot-rule:\n{}", out_h.text);
        let p1 = parser_haas::parse_program(&out_h.text).unwrap();
        assert!(ast_semantic_eq(&p0, &p1), "round-trip dwell value");
        let out_f = emit(&p1, TargetDialect::Fanuc);
        assert!(out_f.text.contains("G4 P2000"), "Fanuc ms:\n{}", out_f.text);
        let p2 = parser_fanuc::parse_program(&out_f.text).unwrap();
        assert!(ast_semantic_eq(&p0, &p2));
    }

    // ---------- Честные отказы («не молча») ----------

    #[test]
    fn refusals_macro_b_targets() {
        // Named-переменная (LinuxCNC-источник) → Fanuc: нет синтаксиса.
        let p_lcnc = crate::parser_linuxcnc::parse_program("#<myvar>=5.\nM30\n").unwrap();
        let out = emit(&p_lcnc, TargetDialect::Fanuc);
        assert!(!out.ok());
        assert!(out.errors[0].reason.contains("no Macro B syntax"));

        // M97 → Fanuc: отказ; → Haas: ok.
        let m97_src = "O1\nM97 P301\nM99\nN301\nM99\n";
        let p_h = parser_haas::parse_program(m97_src).unwrap();
        let out_f = emit(&p_h, TargetDialect::Fanuc);
        assert!(!out_f.ok());
        assert!(out_f.errors[0].reason.contains("M97"));
        let out_h = emit(&p_h, TargetDialect::Haas);
        assert!(out_h.ok(), "{:?}", out_h.errors);

        // Named sub → Macro B: отказ.
        let named_sub = "o<probe> sub\nM99\no<probe> endsub\no<probe> call\nM30\n";
        let p_l = crate::parser_linuxcnc::parse_program(named_sub).unwrap();
        let out = emit(&p_l, TargetDialect::Haas);
        assert!(!out.ok());
        assert!(out.errors.iter().any(|e| e.reason.contains("no Macro B analog")));

        // WHILE-вложенность 4 → лимит DO1..DO3 (честный отказ). 4-я
        // вложенность переиспользует DO1 (лимит — 3 уровня); END{n} закрывает
        // ВНУТРЕННИЙ открытый DO{n} — порядок END1/END3/END2/END1 корректен
        // по Fanuc-парности.
        let deep_src = "O1\n\
                        WHILE [#1 LT 1] DO1\n\
                        WHILE [#1 LT 1] DO2\n\
                        WHILE [#1 LT 1] DO3\n\
                        WHILE [#1 LT 1] DO1\n\
                        END1\n\
                        END3\n\
                        END2\n\
                        END1\nM30\n";
        let p_d = parser_fanuc::parse_program(deep_src).unwrap();
        let out = emit(&p_d, TargetDialect::Fanuc);
        assert!(!out.ok());
        assert!(out.errors[0].reason.contains("DO nesting depth"));
    }

    #[test]
    fn carrier_lints_by_target() {
        let mk = |text: &str| {
            L2Program::new(
                vec![L2Stmt::L1Op(eid_core::ast::Op::Raw {
                    payload_hex: text.bytes().map(|b| format!("{b:02X}")).collect(),
                    reason: "test carrier".into(),
                    code: eid_core::diag::ErrorCode::RuntimeState,
                    src: placeholder_span(),
                })],
                vec![],
            )
        };
        // `;`-комментарий → Fanuc отказ; Haas/LCNC ok.
        let semi = mk("G0 X5. ; fast");
        assert!(!emit(&semi, TargetDialect::Fanuc).ok());
        assert!(emit(&semi, TargetDialect::Haas).ok());
        assert!(emit(&semi, TargetDialect::LinuxCnc).ok());
        // G154-носитель → Fanuc/LCNC отказ; Haas ok.
        let g154 = mk("G154 P98");
        assert!(!emit(&g154, TargetDialect::Fanuc).ok());
        assert!(!emit(&g154, TargetDialect::LinuxCnc).ok());
        assert!(emit(&g154, TargetDialect::Haas).ok());
        // G31-носитель → LCNC отказ (G38.2-семейство); F/H ok.
        let g31 = mk("G31 X10. F50.");
        assert!(!emit(&g31, TargetDialect::LinuxCnc).ok());
        assert!(emit(&g31, TargetDialect::Fanuc).ok());
        // bare-IF-носитель → Fanuc/LCNC отказ; Haas ok.
        let bare = mk("IF [#4 LT 0] S800 M04");
        assert!(!emit(&bare, TargetDialect::Fanuc).ok());
        assert!(!emit(&bare, TargetDialect::LinuxCnc).ok());
        assert!(emit(&bare, TargetDialect::Haas).ok());
        // `(debug, …)` remark — скобочный комментарий НЕ триггерит comma-линт.
        let dbg = mk("(debug, in main, state now:)");
        assert!(
            emit(&dbg, TargetDialect::LinuxCnc).ok(),
            "paren remark must pass"
        );
        // M98-носитель → LCNC отказ.
        let m98 = mk("M98 P100");
        assert!(!emit(&m98, TargetDialect::LinuxCnc).ok());
    }

    #[test]
    fn semantic_eq_ignores_spans_and_num_text() {
        // semantic_diff: спаны и текст Num не участвуют; значения с
        // допуском.
        let a = parser_fanuc::parse_program("#100=2.0\nM30\n").unwrap();
        let b = parser_fanuc::parse_program("#100=2.\nM30\n").unwrap();
        assert!(ast_semantic_eq(&a, &b), "Num text verbatim ignored");
        let c = parser_fanuc::parse_program("#100=3.0\nM30\n").unwrap();
        assert!(!ast_semantic_eq(&a, &c), "different values must differ");
    }
}
