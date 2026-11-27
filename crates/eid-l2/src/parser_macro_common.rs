//! ОБЩЕЕ ЯДРО Macro B (этап 12.1, агент B2) — лексер и statement-парсер
//! Fanuc Custom Macro B / Haas NGC для [`crate::parser_fanuc`] и
//! [`crate::parser_haas`]. Контракт парсеров (сигнатура `parse_program`)
//! зафиксирован агентом A и не меняется.
//!
//! Архитектура (по заданию оркестратора): общий Macro B core здесь +
//! диалект-надстройки в parser_fanuc/parser_haas через [`DialectCfg`]
//! (карта переменных, bare-IF, M97, G101-зеркало).
//!
//! Что парсится (§2.4 дословно):
//! * `#expr = expr` → [`L2Stmt::Assign`] (цель `#[expr]` — Indirect;
//!   литеральный `#0` в значении → `Expr::Null`);
//! * `IF [cond] GOTO n` → [`L2Stmt::IfGoto`]; `IF [cond] THEN stmt` →
//!   [`L2Stmt::IfThen`] (Haas bare-IF без THEN — по флагу
//!   `DialectCfg::allow_bare_if` оборачивается в IfThen{Assign}, правило 3
//!   рулинга оркестратора; bare-IF с не-присваиванием → L1Op-носитель;
//!   безусловный `GOTO n` — §2.4 варианта не имеет — кодируется как
//!   IfGoto с всегда-истинным cond = 1, семантика идентична);
//! * `WHILE [cond] DO m … END m` → [`L2Stmt::While`] (парность do_id 1..3 —
//!   здесь; DO без WHILE — бесконечный цикл, cond = 1);
//! * `G65/G66 P… (буквы-аргументы → локали по НЕлинейной Fanuc-таблице
//!   `ast::call_arg_local_index` — D→#7, F→#9, I→#4 …; амендмент v0.1.2,
//!   BUG-D1: линейная «A→#1..Z→#26» дока §2.4 отвергнута; G/L/N/O/P —
//!   не аргументы; привязка — interp)` →
//!   [`L2Stmt::CallSub`] (modal = G66); `M98 P…` (слип-формат
//!   `P52000` = 5×O2000) → CallSub; Haas `M97 P<n> L<expr>` →
//!   CallSub{SubTarget::NLabel} (правило 4);
//! * `N<n>`-метка → узел [`L2Stmt::Label`] (v0.20.1, no-op) + таблица
//!   `L2Program::labels` для главного тела (индекс узла Label — переход
//!   GOTO/M97 продолжает С него; метки в телах subs — только узлы,
//!   per-body сбор в lowering::collect_labels);
//! * `M99` → [`L2Stmt::Return`] (v0.20.1: «M99 → этот узел»; тела carved-
//!   subs заканчиваются ПЕРЕД Return — неявный return на исчерпании тела);
//! * `G0/G1/G2/G3`-кадры (в т.ч. модальное продолжение) → [`L2Stmt::Move`]
//!   с ExprPose; слово со значением `#0` → Expr::Null в позе (слово
//!   игнорируется — разрешение в interp::pose_resolve, таблица 2.1);
//! * `G43.4/G43.5/G49` → [`L2Stmt::RtcpMode`]; Haas `G101` →
//!   [`L2Stmt::WithTransform`] (Mirror);
//! * канонические L1-опы: `M30/M02` → ProgramEnd, `M0/M01` → ProgramStop,
//!   `Tn M6` → ToolChange (T обязана быть константой), `G4 P/X/U` → Dwell
//!   (единицы слова ДИАЛЕКТНЫЕ — [`DwellSemantics`]; амендмент v0.1.2,
//!   BUG-D2: Fanuc P = мс, Haas P-dot-rule, X/U = сек; канон — секунды);
//! * всё вне §2.4 (G31-пробинг, G154 P#, G10/G28/G53/G92, G103, циклы
//!   G70–G89, vendor M-коды, state-only кадры, `,R`-comma-слова,
//!   block-delete `/`) → **L1Op-носитель** ([`Op::Raw`] c lossless hex
//!   пейлоадом строки + reason) — правило 5 рулинга: «вне §2.4,
//!   Raw-носитель», НЕ парсится в L2Stmt.
//!
//! Ошибки — [`L2Error::RuntimeError`] (незакрытый DO, END без DO/с другим
//! id, IF без GOTO/THEN при allow_bare_if = false, невалидное выражение,
//! ненайденная N-метка M97). ОТКЛОНЕНИЕ от шапки заготовки агента A:
//! там заявлен класс E_INVALID_INPUT для DO-парности; добавление варианта
//! в `L2Error` — амендмент контракта eval.rs вне зоны B2, поэтому все
//! ошибки парсера идут как RuntimeError (класс E_RUNTIME_STATE);
//! задокументировано в стоп-отчёте B2.
//!
//! Приоритеты выражений (Fanuc Macro B; подтверждены идиомой Renishaw
//! `IF [#170 AND 4 EQ 4]` = `(170 AND 4) EQ 4`): сравнения EQ/NE/GT/LT/
//! GE/LE — низший уровень, выше AND/OR/XOR, выше + −, выше * / MOD,
//! выше унарный ±/функции; внутри уровня — слева направо.
//! v0.1.1 амендмент (корпус-ценз): `EXISTS[#var]` → UnaryOp::Exists,
//! `ATAN[y]/[x]` → BinaryOp::Atan2.
//!
//! Прочие решения:
//! * `;` до конца строки — комментарий (согласовано с lib.rs
//!   strip_comments); Haas `;`-EOB в целевых файлах ценза не встречается;
//! * незакрытая `(` комментирует до конца кадра, «висячая» `)`
//!   отбрасывается (толерантность к vultr_g152 `G152 … D22)`);
//! * Haas-битовый суффикс `#30057.25` кодируется индексом n*100+bits
//!   (→ VendorSpecific; битовая семантика вне §2.4);
//! * WCS G54–G59/G110+ и модальные G90/G91/G17–G19/G40/G94/G95/G98/G99
//!   на Move-кадрах абсорбируются (в ExprPose/Move слота нет — см.
//!   стоп-отчёт: строка «G54-G59/units»).

use crate::ast::{
    Axis, Expr, ExprPose, L2Program, L2Stmt, L2Sub, MoveKind, SubTarget, SysVar, TransformKind,
    VarRef,
};
use crate::eval::L2Error;
use eid_core::ast::{Coolant, Dir, Num, Op, Spindle, SrcSpan, State, Wcs};
use eid_core::diag::ErrorCode;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

// ===========================================================================
// Конфигурация диалекта
// ===========================================================================

/// Диалект-семантика dwell-слов G4 (амендрмент v0.1.2, BUG-D2 —
/// trace-зеркало agent D, коммит 841ab2c). Единое «P/X/U verbatim-секунды»
/// v0.1 было неверно для ОБОИХ Macro B диалектов (×1000 в разные стороны).
///
/// Канон [`Op::Dwell`] всегда несёт СЕКУНДЫ (eid-core emitter:
/// `DWELL seconds=…`); конверсия единиц — обязанность парсера:
/// * **Fanuc**: bare P = МИЛЛИсекунды всегда (`G4 P2000` = 2 s; десятичная
///   точка единицы P-слова не меняет), X/U = секунды (`G4 X2.` = 2 s);
/// * **Haas** (P-dot-rule, haas_dialect_analysis §1.2): P с десятичной
///   точкой = секунды (`P1.` = 1 s), без точки = МИЛЛИсекунды
///   (`P500` = 0.5 s); X/U-формы dwell у Haas не документированы —
///   читаются как секунды (симметрия с Fanuc; корпус renishaw-цепочки
///   `G04 P250` bare → 0.25 s).
///
/// LinuxCNC (P = секунды всегда) — в своём парсере (parser_linuxcnc),
/// сюда не входит.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DwellSemantics {
    /// Fanuc Macro B: P = мс (всегда), X/U = сек.
    Fanuc,
    /// Haas NGC: P-dot-rule (с точкой = сек, без = мс), X/U = сек.
    Haas,
}

impl DwellSemantics {
    /// Значение dwell-слова `letter` (verbatim-текст `text`, число
    /// `value`) в СЕКУНДАХ — единица канона [`Op::Dwell`].
    pub fn seconds(self, letter: char, text: &str, value: f64) -> f64 {
        match self {
            Self::Fanuc if letter == 'P' => value / 1000.0,
            Self::Haas if letter == 'P' && !text.contains('.') => value / 1000.0,
            // X/U (оба диалекта) и Haas-P с точкой — секунды verbatim.
            _ => value,
        }
    }
}

/// Диалект-надстройка над общим ядром (см. parser_fanuc/parser_haas).
pub struct DialectCfg {
    /// Имя диалекта — тег SrcSpan.file всех операторов парса.
    pub name: &'static str,
    /// Haas bare-IF без THEN: `IF [cond] #3000= 1` → IfThen{Assign}
    /// (правило 3 рулинга). Fanuc — false (IF без GOTO/THEN — ошибка).
    pub allow_bare_if: bool,
    /// Haas M97 P<n> L<expr> — локальный вызов по N-метке (правило 4).
    pub allow_m97: bool,
    /// Haas G101 — зеркало → WithTransform{TransformKind::Mirror}.
    pub mirror_g101: bool,
    /// Семантика dwell-слов G4 (амендрмент v0.1.2, BUG-D2): Fanuc —
    /// bare-P = мс; Haas — P-dot-rule. X/U = сек у обоих.
    pub dwell: DwellSemantics,
    /// Карта номера `#n` → [`VarRef`] (Direct / Sys(…)); #0 обрабатывается
    /// до карты (Expr::Null в значении, Direct(0) в цели присваивания).
    pub map_var: fn(u32) -> VarRef,
}

/// Ось для системных переменных позиции #5021+к (Fanuc/Haas: 4..6 → A/B/C).
fn axis6(i: u32) -> Axis {
    match i {
        0 => Axis::X,
        1 => Axis::Y,
        2 => Axis::Z,
        3 => Axis::A,
        4 => Axis::B,
        _ => Axis::C,
    }
}

/// Общая Fanuc/Haas-карта системных позиций/модальных групп/таблиц WCS
/// (#52xx → WcsOffset{p}: 0 = external, 1..6 = G54..G59).
pub(crate) fn map_common_system(n: u32) -> Option<VarRef> {
    Some(match n {
        4001..=4030 => VarRef::Sys(SysVar::ModalGroup((n - 4000) as u8)),
        5021..=5026 => VarRef::Sys(SysVar::AbsMachinePos(axis6(n - 5021))),
        5041..=5046 => VarRef::Sys(SysVar::WorkPos(axis6(n - 5041))),
        5061..=5066 => VarRef::Sys(SysVar::SkipPos(axis6(n - 5061))),
        5201..=5206 => VarRef::Sys(SysVar::WcsOffset {
            p: 0,
            axis: axis6(n - 5201),
        }),
        5221..=5226 => VarRef::Sys(SysVar::WcsOffset {
            p: 1,
            axis: axis6(n - 5221),
        }),
        5241..=5246 => VarRef::Sys(SysVar::WcsOffset {
            p: 2,
            axis: axis6(n - 5241),
        }),
        5261..=5266 => VarRef::Sys(SysVar::WcsOffset {
            p: 3,
            axis: axis6(n - 5261),
        }),
        5281..=5286 => VarRef::Sys(SysVar::WcsOffset {
            p: 4,
            axis: axis6(n - 5281),
        }),
        5301..=5306 => VarRef::Sys(SysVar::WcsOffset {
            p: 5,
            axis: axis6(n - 5301),
        }),
        5321..=5326 => VarRef::Sys(SysVar::WcsOffset {
            p: 6,
            axis: axis6(n - 5321),
        }),
        _ => return None,
    })
}

// ===========================================================================
// Курсор (посимвольное сканирование строки/выражения)
// ===========================================================================

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            b: s.as_bytes(),
            i: 0,
        }
    }
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn peek_at(&self, k: usize) -> Option<u8> {
        self.b.get(self.i + k).copied()
    }
    fn bump(&mut self) -> Option<u8> {
        let c = self.peek();
        if c.is_some() {
            self.i += 1;
        }
        c
    }
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r')) {
            self.i += 1;
        }
    }
    fn eof(&self) -> bool {
        self.i >= self.b.len()
    }
    /// Идёт ли с текущей позиции keyword (case-insensitive) с границей
    /// слова (следующий символ — не буква).
    fn starts_kw(&self, kw: &str) -> bool {
        let kb = kw.as_bytes();
        if self.b.len() - self.i < kb.len() {
            return false;
        }
        if !self.b[self.i..self.i + kb.len()].eq_ignore_ascii_case(kb) {
            return false;
        }
        match self.peek_at(kb.len()) {
            Some(c) => !c.is_ascii_alphabetic(),
            None => true,
        }
    }
    /// Число: `123`, `123.`, `123.45`, `.45`. Возвращает verbatim-текст.
    fn scan_number(&mut self) -> Option<String> {
        let start = self.i;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        if self.i == start || self.b[start..self.i].iter().all(|&c| c == b'.') {
            self.i = start;
            return None;
        }
        Some(String::from_utf8_lossy(&self.b[start..self.i]).into_owned())
    }
    /// Verbatim-срез от start до текущей позиции.
    fn slice_from(&self, start: usize) -> String {
        String::from_utf8_lossy(&self.b[start..self.i]).into_owned()
    }
}

/// Захватить сбалансированный `[ … ]` (после пропуска ws; первая `[`).
/// Возвращает текст БЕЗ внешних скобок.
fn capture_bracket(c: &mut Cursor) -> Option<String> {
    c.skip_ws();
    if c.peek() != Some(b'[') {
        return None;
    }
    c.bump();
    let start = c.i;
    let mut depth = 1usize;
    while let Some(ch) = c.bump() {
        match ch {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(String::from_utf8_lossy(&c.b[start..c.i - 1]).into_owned());
                }
            }
            _ => {}
        }
    }
    None // незакрытая скобка
}

// ===========================================================================
// Парсер выражений (приоритеты Fanuc Macro B; EXISTS/Atan2 — v0.1.1)
// ===========================================================================

/// Глубина выражения — защита от переполнения стека на мусорном входе.
const MAX_EXPR_DEPTH: u32 = 200;

use crate::ast::{BinaryOp, UnaryOp};

/// Разобрать выражение из текста (значение слова, RHS присваивания, cond).
pub(crate) fn parse_expr_text(text: &str, map_var: fn(u32) -> VarRef) -> Result<Expr, String> {
    let mut c = Cursor::new(text);
    let e = parse_expr(&mut c, map_var, 0)?;
    c.skip_ws();
    if !c.eof() {
        return Err(format!(
            "trailing junk in expression: {:?}",
            &text[c.i.min(text.len())..]
        ));
    }
    Ok(e)
}

fn parse_expr(c: &mut Cursor, mv: fn(u32) -> VarRef, d: u32) -> Result<Expr, String> {
    if d > MAX_EXPR_DEPTH {
        return Err("expression nesting too deep".into());
    }
    // Уровень 5 (низший): сравнения EQ/NE/GT/LT/GE/LE.
    let mut l = parse_logic(c, mv, d)?;
    loop {
        c.skip_ws();
        let Some(op) = peek_cmp_op(c) else {
            return Ok(l);
        };
        c.bump();
        c.bump();
        let r = parse_logic(c, mv, d)?;
        l = Expr::Binary(op, Box::new(l), Box::new(r));
    }
}

fn peek_cmp_op(c: &Cursor) -> Option<BinaryOp> {
    let two: [u8; 2] = [
        c.peek().unwrap_or(0).to_ascii_uppercase(),
        c.peek_at(1).unwrap_or(0).to_ascii_uppercase(),
    ];
    // Граница слова: третий символ не буква.
    if c.peek_at(2).is_some_and(|ch| ch.is_ascii_alphabetic()) {
        return None;
    }
    match &two {
        b"EQ" => Some(BinaryOp::Eq),
        b"NE" => Some(BinaryOp::Ne),
        b"GT" => Some(BinaryOp::Gt),
        b"LT" => Some(BinaryOp::Lt),
        b"GE" => Some(BinaryOp::Ge),
        b"LE" => Some(BinaryOp::Le),
        _ => None,
    }
}

/// Уровень 4: побитовая логика AND/OR/XOR (выше сравнений — идиома
/// Renishaw `#170 AND 4 EQ 4` = `(170 AND 4) EQ 4`).
fn parse_logic(c: &mut Cursor, mv: fn(u32) -> VarRef, d: u32) -> Result<Expr, String> {
    let mut l = parse_add(c, mv, d)?;
    loop {
        c.skip_ws();
        let Some(op) = peek_logic_op(c) else {
            return Ok(l);
        };
        for _ in 0..3 {
            c.bump();
        }
        let r = parse_add(c, mv, d)?;
        l = Expr::Binary(op, Box::new(l), Box::new(r));
    }
}

fn peek_logic_op(c: &Cursor) -> Option<BinaryOp> {
    let three: [u8; 3] = [
        c.peek().unwrap_or(0).to_ascii_uppercase(),
        c.peek_at(1).unwrap_or(0).to_ascii_uppercase(),
        c.peek_at(2).unwrap_or(0).to_ascii_uppercase(),
    ];
    // Граница слова: четвёртый символ не буква (OR… vs ORDER).
    if c.peek_at(3).is_some_and(|ch| ch.is_ascii_alphabetic()) {
        return None;
    }
    match &three {
        b"AND" => Some(BinaryOp::And),
        b"XOR" => Some(BinaryOp::Xor),
        b"OR " | b"OR\t" | b"OR[" | b"OR]" | b"OR\x00" => Some(BinaryOp::Or),
        _ => None,
    }
}

/// Уровень 3: + −.
fn parse_add(c: &mut Cursor, mv: fn(u32) -> VarRef, d: u32) -> Result<Expr, String> {
    let mut l = parse_mul(c, mv, d)?;
    loop {
        c.skip_ws();
        let op = match c.peek() {
            Some(b'+') => BinaryOp::Add,
            Some(b'-') => BinaryOp::Sub,
            _ => return Ok(l),
        };
        c.bump();
        let r = parse_mul(c, mv, d)?;
        l = Expr::Binary(op, Box::new(l), Box::new(r));
    }
}

/// Уровень 2: * / MOD.
fn parse_mul(c: &mut Cursor, mv: fn(u32) -> VarRef, d: u32) -> Result<Expr, String> {
    let mut l = parse_unary(c, mv, d)?;
    loop {
        c.skip_ws();
        let (op, w) = match c.peek() {
            Some(b'*') => (BinaryOp::Mul, 1),
            Some(b'/') => (BinaryOp::Div, 1),
            Some(b'M') | Some(b'm') if starts_word(c, "MOD") => (BinaryOp::Mod, 3),
            _ => return Ok(l),
        };
        for _ in 0..w {
            c.bump();
        }
        let r = parse_unary(c, mv, d)?;
        l = Expr::Binary(op, Box::new(l), Box::new(r));
    }
}

fn starts_word(c: &Cursor, w: &str) -> bool {
    let wb = w.as_bytes();
    if c.b.len() - c.i < wb.len() {
        return false;
    }
    c.b[c.i..c.i + wb.len()].eq_ignore_ascii_case(wb)
        && !c
            .peek_at(wb.len())
            .is_some_and(|ch| ch.is_ascii_alphabetic())
}

/// Уровень 1 (высший): унарный ±, функции, скобки, числа, #var.
fn parse_unary(c: &mut Cursor, mv: fn(u32) -> VarRef, d: u32) -> Result<Expr, String> {
    c.skip_ws();
    let mut neg = false;
    while matches!(c.peek(), Some(b'-' | b'+')) {
        if c.peek() == Some(b'-') {
            neg = !neg;
        }
        c.bump();
        c.skip_ws();
    }
    let e = parse_atom(c, mv, d)?;
    Ok(if neg {
        Expr::Unary(UnaryOp::Neg, Box::new(e))
    } else {
        e
    })
}

/// Функции Macro B (case-insensitive) → UnaryOp. EXISTS — v0.1.1 амендмент.
fn peek_fn(c: &Cursor) -> Option<UnaryOp> {
    const NAMES: &[(&str, UnaryOp)] = &[
        ("SQRT", UnaryOp::Sqrt),
        ("ABS", UnaryOp::Abs),
        ("SIN", UnaryOp::Sin),
        ("COS", UnaryOp::Cos),
        ("TAN", UnaryOp::Tan),
        ("ASIN", UnaryOp::Asin),
        ("ACOS", UnaryOp::Acos),
        ("ATAN", UnaryOp::Atan),
        ("LN", UnaryOp::Ln),
        ("EXP", UnaryOp::Exp),
        ("ROUND", UnaryOp::Round),
        ("FIX", UnaryOp::Fix),
        ("FUP", UnaryOp::Fup),
        ("EXISTS", UnaryOp::Exists), // v0.1.1 амендмент (корпус-ценз g881)
    ];
    NAMES
        .iter()
        .find(|(name, _)| starts_word(c, name))
        .map(|(_, op)| *op)
}

fn fn_name_len(op: UnaryOp) -> usize {
    match op {
        UnaryOp::Exists => 6,
        UnaryOp::Round => 5,
        UnaryOp::Sqrt | UnaryOp::Asin | UnaryOp::Acos | UnaryOp::Exp => 4,
        _ => 3,
    }
}

fn parse_atom(c: &mut Cursor, mv: fn(u32) -> VarRef, d: u32) -> Result<Expr, String> {
    c.skip_ws();
    match c.peek() {
        Some(b'[') => {
            let inner = capture_bracket(c).ok_or("unbalanced '[' in expression")?;
            parse_expr_text(&inner, mv)
        }
        Some(b'#') => {
            c.bump();
            match scan_var_ref(c, mv, d)? {
                VarParse::Null => Ok(Expr::Null),
                VarParse::Ref(r) => Ok(Expr::Var(r)),
            }
        }
        Some(ch) if ch.is_ascii_digit() || ch == b'.' => {
            let text = c.scan_number().ok_or("invalid number")?;
            let v: f64 = text.parse().map_err(|_| format!("bad number {text:?}"))?;
            Ok(Expr::Const(Num::new(text, v)))
        }
        Some(ch) if ch.is_ascii_alphabetic() => {
            // ATAN — особый случай: двухаргументный ATAN[y]/[x]
            // (v0.1.1 амендмент) → BinaryOp::Atan2.
            if starts_word(c, "ATAN") && peek_fn(c) == Some(UnaryOp::Atan) {
                for _ in 0..4 {
                    c.bump();
                }
                let y = capture_bracket(c).ok_or("ATAN without [y]")?;
                let ye = parse_expr_text(&y, mv)?;
                c.skip_ws();
                if c.peek() == Some(b'/') {
                    c.bump();
                    let x = capture_bracket(c).ok_or("ATAN[y] without /[x]")?;
                    let xe = parse_expr_text(&x, mv)?;
                    return Ok(Expr::Binary(BinaryOp::Atan2, Box::new(ye), Box::new(xe)));
                }
                return Ok(Expr::Unary(UnaryOp::Atan, Box::new(ye)));
            }
            let op = peek_fn(c).ok_or_else(|| format!("unknown function at offset {}", c.i))?;
            for _ in 0..fn_name_len(op) {
                c.bump();
            }
            let arg = capture_bracket(c).ok_or("function without [arg]")?;
            let ae = parse_expr_text(&arg, mv)?;
            Ok(Expr::Unary(op, Box::new(ae)))
        }
        other => Err(format!("unexpected character {other:?} in expression")),
    }
}

/// Результат разбора `#…`: Null-литерал (#0) или ссылка.
enum VarParse {
    Null,
    Ref(VarRef),
}

/// Разобрать ссылку на переменную ПОСЛЕ `#`: `[expr]` (Indirect) или
/// число с опциональным Haas-битовым суффиксом `.b` → индекс n*100+b
/// (документированное кодирование VendorSpecific; битовая семантика
/// вне §2.4).
fn scan_var_ref(c: &mut Cursor, mv: fn(u32) -> VarRef, _d: u32) -> Result<VarParse, String> {
    c.skip_ws();
    if c.peek() == Some(b'[') {
        let inner = capture_bracket(c).ok_or("unbalanced '[' in #[] reference")?;
        let e = parse_expr_text(&inner, mv)?;
        return Ok(VarParse::Ref(VarRef::Indirect(Box::new(e))));
    }
    let num = c.scan_number().ok_or("missing variable number after '#'")?;
    let (n, bits): (u64, u64) = match num.split_once('.') {
        Some((a, b)) if !b.is_empty() => (
            a.parse().map_err(|_| format!("bad var number {num:?}"))?,
            b.parse().map_err(|_| format!("bad var bits {num:?}"))?,
        ),
        _ => (
            num.parse().map_err(|_| format!("bad var number {num:?}"))?,
            0,
        ),
    };
    if n > u32::MAX as u64 {
        return Err(format!("variable number {n} out of range"));
    }
    if n == 0 && bits == 0 {
        return Ok(VarParse::Null);
    }
    let idx = if bits > 0 { n * 100 + bits } else { n } as u32;
    Ok(VarParse::Ref(mv(idx)))
}

// ===========================================================================
// Комментарии
// ===========================================================================

/// Вырезать скобочные комментарии `( … )` (незакрытая `(` комментирует
/// до конца кадра; «висячая» `)` вне комментария отбрасывается —
/// толерантность к vultr_g152 `G152 … D22)`), а также `;`-комментарий
/// до конца строки (согласовано с lib.rs::strip_comments; Haas `;`-EOB
/// в целевых файлах ценза не встречается — см. стоп-отчёт).
pub(crate) fn strip_line_comments(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut depth = 0usize;
    for ch in line.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                // висячая ')' без '(' (depth уже 0) — отбросить (толерантность)
            }
            ';' if depth == 0 => break, // ; -комментарий до EOL
            ch if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

// ===========================================================================
// Кадры: слова и диспетчеризация
// ===========================================================================

/// Слово кадра: буква + значение (Expr) + verbatim-текст значения.
struct FrameWord {
    letter: char,
    expr: Expr,
    #[allow(dead_code)]
    verbatim: String,
}

/// M-слово: числовой код (если константа) + выражение (для G65-аргументов:
/// `G65 … M1.` — M это АРГУМЕНТ #13, а не M-код).
struct MWord {
    code: Option<f64>,
    expr: Expr,
}

/// Разобранный кадр (строка без комментариев/метки).
struct Frame {
    gs: Vec<f64>,
    ms: Vec<MWord>,
    words: Vec<FrameWord>,
    junk: bool,
}

/// Модальное движение (группа 01).
#[derive(Debug, Clone, Copy, PartialEq)]
enum ModalMotion {
    Rapid,
    Linear,
    ArcCw,
    ArcCcw,
}

/// G-коды, абсорбируемые на Move-кадрах (модальные префиксы, не меняющие
/// представление хода; WCS G54–G59 и единицы G20/G21 абсорбируются молча —
/// в ExprPose/Move слота нет; см. стоп-отчёт «G54-G59/units»).
const ABSORB_G: &[f64] = &[
    20.0, 21.0, 17.0, 18.0, 19.0, 40.0, 54.0, 55.0, 56.0, 57.0, 58.0, 59.0, 60.0, 61.0, 64.0, 90.0,
    91.0, 94.0, 95.0, 98.0, 99.0,
];

/// Буквы, допустимые в Move-кадре (остальные → L1Op-носитель всей строки;
/// в т.ч. U/V/W — есть в Axis, но в §2.4 ExprPose слота нет).
const MOVE_LETTERS: &[char] = &[
    'X', 'Y', 'Z', 'A', 'B', 'C', // поза
    'I', 'J', 'K', 'R', // дуга
    'F', 'S', // подача/шпиндель
];

impl Frame {
    fn word(&self, letter: char) -> Option<&FrameWord> {
        self.words.iter().find(|w| w.letter == letter)
    }
    fn has_any(&self, letters: &[char]) -> bool {
        self.words.iter().any(|w| letters.contains(&w.letter))
    }
    fn m_present(&self, code: f64) -> bool {
        self.ms.iter().any(|m| m.code == Some(code))
    }
}

// ===========================================================================
// Значения слов
// ===========================================================================

/// Значение слова кадра: `[знак] (число | #var | [выражение])`.
/// Возвращает (Expr, verbatim-срез входа).
fn scan_word_value(c: &mut Cursor, mv: fn(u32) -> VarRef) -> Option<(Expr, String)> {
    c.skip_ws();
    let start = c.i;
    if matches!(c.peek(), Some(b'-' | b'+')) {
        c.bump();
        c.skip_ws();
    }
    match c.peek()? {
        b'0'..=b'9' | b'.' => {
            c.scan_number()?;
        }
        b'#' => {
            c.bump();
            scan_var_ref(c, mv, 0).ok()?;
        }
        b'[' => {
            capture_bracket(c)?;
        }
        _ => return None,
    }
    let verbatim = c.slice_from(start);
    let expr = parse_expr_text(&verbatim, mv).ok()?;
    Some((expr, verbatim))
}

/// Скан кадра: последовательность `буква значение` (пробел между буквой и
/// значением допускается: `S #[ #161 ]` — rensh-цензор «tolerant-пробелы»).
fn scan_frame(c: &mut Cursor, mv: fn(u32) -> VarRef) -> Frame {
    let mut fr = Frame {
        gs: Vec::new(),
        ms: Vec::new(),
        words: Vec::new(),
        junk: false,
    };
    loop {
        c.skip_ws();
        if c.eof() {
            break;
        }
        let Some(ch) = c.peek() else { break };
        if !ch.is_ascii_alphabetic() {
            c.bump();
            fr.junk = true;
            continue;
        }
        let letter = (c.bump().unwrap() as char).to_ascii_uppercase();
        let Some((expr, verbatim)) = scan_word_value(c, mv) else {
            fr.junk = true;
            continue;
        };
        match letter {
            'G' => match &expr {
                Expr::Const(n) => fr.gs.push(n.value()),
                _ => fr.junk = true, // G с переменной — вне §2.4-кадр
            },
            'M' => {
                let code = match &expr {
                    Expr::Const(n) => Some(n.value()),
                    _ => None,
                };
                fr.ms.push(MWord { code, expr });
            }
            _ => fr.words.push(FrameWord {
                letter,
                expr,
                verbatim,
            }),
        }
    }
    fr
}

// ===========================================================================
// Состояние парсера
// ===========================================================================

struct Builder<'a> {
    cfg: &'a DialectCfg,
    file: Arc<str>,
    errors: Vec<L2Error>,
    main: Vec<L2Stmt>,
    /// Номер строки каждого stmt главного тела (для вырезания M97-тел).
    main_lines: Vec<u32>,
    /// N-метка → индекс в main, с которого начинается блок метки.
    labels: BTreeMap<u32, usize>,
    /// Строки главного тела с M99 (терминаторы для вырезания M97-тел).
    m99_lines: BTreeSet<u32>,
    subs: Vec<L2Sub>,
    /// Текущее O-подпрограммное тело (после 2-го O-заголовка).
    in_sub: Option<L2Sub>,
    seen_o_header: bool,
    m97_targets: Vec<u32>,
    /// Стек незакрытых DO: (do_id, While-stmt с накапливаемым телом).
    do_stack: Vec<(u32, L2Stmt)>,
    motion: Option<ModalMotion>,
    /// Активен циклический G-код (G70–G89, кроме G80): продолжения
    /// `X… Y…` — не Move (циклы вне §2.4).
    canned: bool,
    cur_line: u32,
}

impl<'a> Builder<'a> {
    fn err(&mut self, msg: String) {
        self.errors.push(L2Error::RuntimeError(msg));
    }

    fn span(&self, len: usize) -> SrcSpan {
        SrcSpan {
            file: Arc::clone(&self.file),
            line_start: self.cur_line,
            col_start: 1,
            line_end: self.cur_line,
            col_end: len as u32 + 1,
        }
    }

    /// L1Op-носитель (Raw-класс): lossless hex всей строки кадра + reason
    /// (правило 5 рулинга: вне §2.4 → НЕ парсить в L2Stmt).
    fn carrier(&self, text: &str, reason: impl Into<String>) -> L2Stmt {
        L2Stmt::L1Op(Op::Raw {
            payload_hex: text.bytes().map(|b| format!("{b:02X}")).collect(),
            reason: reason.into(),
            code: ErrorCode::RuntimeState,
            src: self.span(text.len()),
        })
    }

    fn push_carrier(&mut self, text: &str, reason: impl Into<String>) {
        let c = self.carrier(text, reason);
        self.push(c);
    }

    /// Куда попадает следующий stmt: тело DO > тело O-sub > main.
    fn push(&mut self, s: L2Stmt) {
        if let Some((_, top)) = self.do_stack.last_mut() {
            if let L2Stmt::While { body, .. } = top {
                body.push(s);
            }
        } else if let Some(sub) = self.in_sub.as_mut() {
            sub.body.push(s);
        } else {
            self.main.push(s);
            self.main_lines.push(self.cur_line);
        }
    }

    /// Обработка M99 → `L2Stmt::Return` (v0.20.1: «M99 → этот узел»).
    /// Строка записывается в `m99_lines` на уровне main (терминатор для
    /// вырезания M97-тел; тело carved-sub заканчивается ПЕРЕД Return —
    /// неявный return на исчерпании тела, семантика та же).
    fn handle_m99(&mut self, text: &str) {
        let ret = L2Stmt::Return {
            src: self.span(text.len()),
        };
        self.push(ret);
        if self.in_sub.is_none() && self.do_stack.is_empty() {
            self.m99_lines.insert(self.cur_line);
        }
    }

    fn push_extra(&mut self, extra: Vec<L2Stmt>, m99_flag: bool, text: &str) {
        for s in extra {
            self.push(s);
        }
        if m99_flag {
            self.handle_m99(text);
        }
    }
}

/// Состояние по умолчанию для синтезированных L1-опов (Dwell/Stop/End/
/// ToolChange): модель станка в фазе 12.1 не ведётся (как
/// interp::l1_default_state).
fn plain_state() -> State {
    State {
        feed: None,
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        tool: None,
        frame: eid_core::ast::Frame::Wcs,
        wcs: Wcs::G54,
        feed_restated: false,
    }
}

// ===========================================================================
// Диспетчер кадра
// ===========================================================================

impl<'a> Builder<'a> {
    /// Разобрать и диспетчеризовать кадр. `text` — строка без комментариев.
    fn dispatch_frame(&mut self, fr: Frame, text: &str) {
        // --- модальное состояние (обновляется ДО решения о носителе) ---
        let mut motion_now: Option<ModalMotion> = None;
        let mut rtcp: Option<bool> = None;
        for &g in &fr.gs {
            match g {
                0.0 => {
                    self.motion = Some(ModalMotion::Rapid);
                    motion_now = Some(ModalMotion::Rapid);
                    self.canned = false;
                }
                1.0 => {
                    self.motion = Some(ModalMotion::Linear);
                    motion_now = Some(ModalMotion::Linear);
                    self.canned = false;
                }
                2.0 => {
                    self.motion = Some(ModalMotion::ArcCw);
                    motion_now = Some(ModalMotion::ArcCw);
                    self.canned = false;
                }
                3.0 => {
                    self.motion = Some(ModalMotion::ArcCcw);
                    motion_now = Some(ModalMotion::ArcCcw);
                    self.canned = false;
                }
                43.4 | 43.5 => rtcp = Some(true),
                49.0 => rtcp = Some(false),
                g if g.fract() == 0.0 && (70.0..=89.0).contains(&g) && g != 80.0 => {
                    // Циклический G-код (G70–G89): вне §2.4; модальное
                    // движение консервативно сбрасывается.
                    self.canned = true;
                    self.motion = None;
                }
                80.0 => {
                    self.canned = false;
                    self.motion = None;
                }
                _ => {}
            }
        }

        // --- 1. RTCP (§2.4: G43.4/G43.5/G49 → RtcpMode) ---
        if let Some(enabled) = rtcp {
            let h = fr.word('H').and_then(|w| match &w.expr {
                Expr::Const(n) => Some(n.value() as u32),
                _ => None,
            });
            let stmt = L2Stmt::RtcpMode {
                enabled,
                tool_length_h: h,
                src: self.span(text.len()),
            };
            self.push(stmt);
            return;
        }

        // --- 2. G65/G66 → CallSub (M-слова — АРГУМЕНТЫ: G65 P9810 Z2. M1.) ---
        if fr.gs.contains(&65.0) || fr.gs.contains(&66.0) {
            let modal = fr.gs.contains(&66.0);
            self.dispatch_call_sub(&fr, modal, text);
            return;
        }

        // --- 3. M98 (оба диалекта) / M97 (Haas) → CallSub ---
        if fr.m_present(98.0) {
            self.dispatch_m98(&fr, text);
            return;
        }
        if self.cfg.allow_m97 && fr.m_present(97.0) {
            self.dispatch_m97(&fr, text);
            return;
        }

        // --- 4. M-флаги: M99/M30/M02/M0/M01/M6; vendor M → носитель ---
        let mut extra: Vec<L2Stmt> = Vec::new();
        let mut m99_flag = false;
        let mut carrier_reason: Option<String> = None;
        let mut tool_change_ok = false;
        for mw in &fr.ms {
            let Some(m) = mw.code else {
                carrier_reason = Some("M-word with variable value (Raw carrier)".into());
                continue;
            };
            match m {
                99.0 => m99_flag = true,
                30.0 | 2.0 => extra.push(L2Stmt::L1Op(Op::ProgramEnd {
                    src: self.span(text.len()),
                })),
                0.0 => extra.push(L2Stmt::L1Op(Op::ProgramStop {
                    optional: false,
                    state: plain_state(),
                    src: self.span(text.len()),
                })),
                1.0 => extra.push(L2Stmt::L1Op(Op::ProgramStop {
                    optional: true,
                    state: plain_state(),
                    src: self.span(text.len()),
                })),
                6.0 => match fr.word('T').map(|w| &w.expr) {
                    Some(Expr::Const(n)) => {
                        extra.push(L2Stmt::L1Op(Op::ToolChange {
                            tool: n.value() as u32,
                            state: plain_state(),
                            src: self.span(text.len()),
                        }));
                        tool_change_ok = true;
                    }
                    _ => {
                        carrier_reason = Some(
                            "M6 with missing/variable T word (outside §2.4, Raw carrier)".into(),
                        );
                    }
                },
                // M3/M4/M5/M8/M9 на кадрах движения абсорбируются
                // (состояние станка — вне §2.4; в L2 AST слота нет).
                3.0 | 4.0 | 5.0 | 8.0 | 9.0 => {}
                _ => {
                    carrier_reason = Some(format!(
                        "vendor M-code M{} outside §2.4 (Raw carrier)",
                        m as i64
                    ));
                }
            }
        }

        // --- 5. G4 → Dwell (канонический L1-оп). Единицы слова
        // ДИАЛЕКТНЫЕ (амендрмент v0.1.2, BUG-D2): канон Op::Dwell.seconds
        // всегда в СЕКУНДАХ — см. DwellSemantics::seconds. ---
        if fr.gs.contains(&4.0) {
            let dwell = ['P', 'X', 'U'].iter().find_map(|&l| match fr.word(l) {
                Some(FrameWord {
                    expr: Expr::Const(n),
                    verbatim,
                    ..
                }) => {
                    let secs = self.cfg.dwell.seconds(l, verbatim, n.value());
                    // Текст Num: слово, прочитанное verbatim (секунды), —
                    // исходный срез; сконвертированное (мс → с) — текст
                    // значения (инвариант Num: parse(text) == value).
                    let text = if secs == n.value() {
                        verbatim.trim_start_matches('+').to_string()
                    } else {
                        secs.to_string()
                    };
                    Some(Num::new(text, secs))
                }
                _ => None,
            });
            match dwell {
                Some(seconds) => {
                    extra.push(L2Stmt::L1Op(Op::Dwell {
                        seconds,
                        state: plain_state(),
                        src: self.span(text.len()),
                        sub: None,
                    }));
                    self.push_extra(extra, m99_flag, text);
                    return;
                }
                None => {
                    carrier_reason =
                        Some("G4 with variable/missing dwell value (Raw carrier)".into());
                }
            }
        }

        // --- 6. Haas G101 — зеркало → WithTransform{Mirror} ---
        if self.cfg.mirror_g101 && fr.gs.contains(&101.0) {
            let axis_word = ['X', 'Y', 'Z']
                .iter()
                .find_map(|&l| fr.word(l).map(|w| (l, w.expr.clone())));
            match axis_word {
                Some((l, pivot)) => {
                    let axis = match l {
                        'X' => Axis::X,
                        'Y' => Axis::Y,
                        _ => Axis::Z,
                    };
                    extra.push(L2Stmt::WithTransform {
                        transform: TransformKind::Mirror { axis, pivot },
                        body: Vec::new(),
                        src: self.span(text.len()),
                    });
                    self.push_extra(extra, m99_flag, text);
                    return;
                }
                None => {
                    carrier_reason = Some("G101 mirror without axis word (Raw carrier)".into());
                }
            }
        }

        // --- 7. Неабсорбируемый G-код → носитель всей строки ---
        if carrier_reason.is_none() {
            for &g in &fr.gs {
                let handled =
                    matches!(g, 0.0 | 1.0 | 2.0 | 3.0 | 4.0 | 101.0) || ABSORB_G.contains(&g);
                if !handled {
                    carrier_reason = Some(format!("non-Macro-B G{g} outside §2.4 (Raw carrier)"));
                    break;
                }
            }
        }

        // --- 8. Мусор / недопустимые буквы → носитель ---
        // (T, успешно разобранная в ToolChange, — не «недопустимая буква».)
        let bad_letters: String = fr
            .words
            .iter()
            .map(|w| w.letter)
            .filter(|l| !(MOVE_LETTERS.contains(l) || tool_change_ok && *l == 'T'))
            .collect();
        if carrier_reason.is_none() && (fr.junk || !bad_letters.is_empty()) {
            carrier_reason = Some(if bad_letters.is_empty() {
                "junk characters in frame (e.g. comma-words), Raw carrier".to_string()
            } else {
                format!("frame words outside §2.4 pose set ({bad_letters}), Raw carrier")
            });
        }

        if let Some(reason) = carrier_reason {
            self.push_carrier(text, reason);
            self.push_extra(extra, m99_flag, text);
            return;
        }

        // --- 9. Движение ---
        let axis_present = fr.has_any(&['X', 'Y', 'Z', 'A', 'B', 'C']);
        let arc_words = fr.has_any(&['I', 'J', 'K', 'R']);
        if !axis_present && !arc_words {
            // Кадр без хода: флаги уже собраны; чисто модальная строка
            // (только абсорбируемые G, без слов и M-флагов) — молча.
            if !extra.is_empty() || m99_flag {
                self.push_extra(extra, m99_flag, text);
                return;
            }
            let gs_modal_only = fr
                .gs
                .iter()
                .all(|g| ABSORB_G.contains(g) || matches!(g, 0.0 | 1.0 | 2.0 | 3.0));
            if gs_modal_only && fr.ms.is_empty() && fr.words.is_empty() && !fr.junk {
                return; // чистая модаль (G90/G54/G0-продолжение…)
            }
            self.push_carrier(text, "state-only frame outside §2.4 (Raw carrier)");
            return;
        }
        if self.canned {
            self.push_carrier(
                text,
                "canned-cycle continuation frame (cycles outside §2.4), Raw carrier",
            );
            self.push_extra(extra, m99_flag, text);
            return;
        }
        let Some(mode) = motion_now.or(self.motion) else {
            self.push_carrier(text, "move frame without motion mode, Raw carrier");
            self.push_extra(extra, m99_flag, text);
            return;
        };
        let mk = |w: char| fr.word(w).map(|fw| fw.expr.clone());
        let feed = mk('F');
        let spindle = mk('S');
        let kind = match mode {
            ModalMotion::Rapid => MoveKind::Rapid,
            ModalMotion::Linear => MoveKind::Linear,
            ModalMotion::ArcCw | ModalMotion::ArcCcw => {
                let ijk = [mk('I'), mk('J'), mk('K')];
                let r = mk('R');
                if ijk.iter().all(|e| e.is_none()) && r.is_none() {
                    self.push_carrier(text, "arc without I/J/K/R geometry, Raw carrier");
                    self.push_extra(extra, m99_flag, text);
                    return;
                }
                MoveKind::Arc {
                    ijk,
                    r,
                    dir: if mode == ModalMotion::ArcCw {
                        Dir::Cw
                    } else {
                        Dir::Ccw
                    },
                }
            }
        };
        let pose = ExprPose {
            x: mk('X'),
            y: mk('Y'),
            z: mk('Z'),
            a: mk('A'),
            b: mk('B'),
            c: mk('C'),
        };
        let mv = L2Stmt::Move {
            kind,
            target: pose,
            feed,
            spindle,
            src: self.span(text.len()),
        };
        self.push(mv);
        self.push_extra(extra, m99_flag, text);
    }

    /// G65/G66: P → ONumber (константа), L → repeat, прочие буквы
    /// (включая M-слова!) кроме G/L/N/O/P → аргументы CallSub (буква
    /// хранится как есть; привязка к локали — Fanuc-таблица
    /// `ast::call_arg_local_index` в interp, амендмент v0.1.2 BUG-D1 —
    /// НЕлинейная: D→#7, F→#9, I→#4…).
    fn dispatch_call_sub(&mut self, fr: &Frame, modal: bool, text: &str) {
        let target = match fr.word('P').map(|w| &w.expr) {
            Some(Expr::Const(n)) => SubTarget::ONumber(n.value() as u32),
            _ => {
                self.push_carrier(text, "G65/G66 with missing or variable P (Raw carrier)");
                return;
            }
        };
        let repeat = fr.word('L').map(|w| w.expr.clone());
        let mut args: Vec<(u8, Expr)> = fr
            .words
            .iter()
            .filter(|w| !matches!(w.letter, 'P' | 'L'))
            .map(|w| (w.letter as u8, w.expr.clone()))
            .collect();
        for mw in &fr.ms {
            args.push((b'M', mw.expr.clone()));
        }
        let stmt = L2Stmt::CallSub {
            target,
            repeat,
            args,
            modal,
            src: self.span(text.len()),
        };
        self.push(stmt);
    }

    /// M98 P<программа> [L<повтор>]; слип-формат P52000 = 5×O2000
    /// (≥5 цифр: младшие 4 — программа, старшие — повтор).
    fn dispatch_m98(&mut self, fr: &Frame, text: &str) {
        let Some(Expr::Const(n)) = fr.word('P').map(|w| &w.expr) else {
            self.push_carrier(text, "M98 with missing/variable P (Raw carrier)");
            return;
        };
        let p = n.value();
        let (target, auto_repeat) = if p >= 10000.0 {
            (
                SubTarget::ONumber((p % 10000.0) as u32),
                Some(Expr::num((p / 10000.0).trunc())),
            )
        } else {
            (SubTarget::ONumber(p as u32), None)
        };
        let repeat = fr.word('L').map(|w| w.expr.clone()).or(auto_repeat);
        let stmt = L2Stmt::CallSub {
            target,
            repeat,
            args: Vec::new(),
            modal: false,
            src: self.span(text.len()),
        };
        self.push(stmt);
    }

    /// Haas M97 P<n> [L<expr>] — вызов локальной подпрограммы по N-метке
    /// (правило 4 рулинга: CallSub{SubTarget::NLabel}).
    fn dispatch_m97(&mut self, fr: &Frame, text: &str) {
        let Some(Expr::Const(n)) = fr.word('P').map(|w| &w.expr) else {
            self.push_carrier(text, "M97 with missing/variable P (Raw carrier)");
            return;
        };
        let repeat = fr.word('L').map(|w| w.expr.clone());
        self.m97_targets.push(n.value() as u32);
        let stmt = L2Stmt::CallSub {
            target: SubTarget::NLabel(n.value() as u32),
            repeat,
            args: Vec::new(),
            modal: false,
            src: self.span(text.len()),
        };
        self.push(stmt);
    }
}

// ===========================================================================
// Верхнеуровневый разбор
// ===========================================================================

/// Общий вход: разобрать Macro B текст в L2Program (для обоих диалектов).
pub fn parse_macro_b(cfg: &DialectCfg, src: &str) -> Result<L2Program, Vec<L2Error>> {
    let mut b = Builder {
        cfg,
        file: Arc::from(cfg.name),
        errors: Vec::new(),
        main: Vec::new(),
        main_lines: Vec::new(),
        labels: BTreeMap::new(),
        m99_lines: BTreeSet::new(),
        subs: Vec::new(),
        in_sub: None,
        seen_o_header: false,
        m97_targets: Vec::new(),
        do_stack: Vec::new(),
        motion: None,
        canned: false,
        cur_line: 0,
    };

    for (idx, raw_line) in src.lines().enumerate() {
        b.cur_line = (idx + 1) as u32;
        let line = strip_line_comments(raw_line);
        let text = line.trim();
        if text.is_empty() || text == "%" {
            continue;
        }
        let mut c = Cursor::new(text);

        // Block-delete: строка `/…` — опциональное исполнение (вне §2.4).
        if c.peek() == Some(b'/') {
            b.push_carrier(text, "block-delete line (outside §2.4), Raw carrier");
            continue;
        }

        // N-метка (префикс): N942#3000=2 … / N10 G1 Z#1 … / N100.
        // v0.20.1 (контракт B2/D2, агент A): N-метка → узел Label (no-op)
        // + таблица L2Program::labels (главное тело; индекс узла Label —
        // переход GOTO/M97 продолжает С него).
        if matches!(c.peek(), Some(b'N' | b'n'))
            && matches!(c.peek_at(1), Some(ch) if ch.is_ascii_digit())
        {
            c.bump();
            let num = c.scan_number().unwrap_or_default();
            let mut label_stmt: Option<L2Stmt> = None;
            if let Ok(n) = num.parse::<u32>() {
                let src = b.span(text.len());
                // Таблица — только для главного тела (M97-мишени; метки в
                // телах subs — через узлы Label, per-body сбор lowering).
                if b.in_sub.is_none() && b.do_stack.is_empty() {
                    b.labels.insert(n, b.main.len());
                }
                label_stmt = Some(L2Stmt::Label { n, src });
            }
            if let Some(s) = label_stmt {
                b.push(s);
            }
            c.skip_ws();
            if c.eof() {
                continue; // строка-метка (узел Label уже испущен)
            }
        }

        // O-заголовок: первый → главное тело продолжается, последующие —
        // новые подпрограммы (O#### … M99).
        if matches!(c.peek(), Some(b'O' | b'o'))
            && matches!(c.peek_at(1), Some(ch) if ch.is_ascii_digit())
        {
            c.bump();
            let num = c.scan_number().unwrap_or_default();
            let onum = num.parse::<u32>().unwrap_or(0);
            if b.seen_o_header {
                if let Some(s) = b.in_sub.take() {
                    b.subs.push(s);
                }
                b.in_sub = Some(L2Sub {
                    target: SubTarget::ONumber(onum),
                    body: Vec::new(),
                });
            } else {
                b.seen_o_header = true;
            }
            c.skip_ws();
            if c.eof() {
                continue;
            }
            // Контент на строке O-заголовка — носитель.
            b.push_carrier(text, "content on O-header line, Raw carrier");
            continue;
        }

        // Keyword-диспетчер (case-insensitive).
        c.skip_ws();
        if c.starts_kw("IF") {
            c.bump();
            c.bump();
            parse_if_stmt(&mut b, &mut c, text);
            continue;
        }
        if c.starts_kw("WHILE") {
            for _ in 0..5 {
                c.bump();
            }
            parse_while_stmt(&mut b, &mut c, text);
            continue;
        }
        if c.starts_kw("GOTO") {
            for _ in 0..4 {
                c.bump();
            }
            c.skip_ws();
            match c.scan_number().and_then(|s| s.parse::<u32>().ok()) {
                Some(target) => {
                    // Безусловный GOTO: варианта §2.4 нет — IfGoto с
                    // всегда-истинным cond (семантика идентична).
                    let stmt = L2Stmt::IfGoto {
                        cond: Expr::num(1.0),
                        target_label: target,
                        src: b.span(text.len()),
                    };
                    b.push(stmt);
                }
                None => b.err(format!("line {}: GOTO without target", b.cur_line)),
            }
            continue;
        }
        if c.starts_kw("DO") {
            c.bump();
            c.bump();
            // DO m без WHILE — бесконечный цикл (Fanuc Macro B).
            parse_do_open(&mut b, &mut c, text, Expr::num(1.0));
            continue;
        }
        if c.starts_kw("END") {
            c.bump();
            c.bump();
            c.bump();
            parse_end_close(&mut b, &mut c, text);
            continue;
        }

        // Присваивание: `#var = expr` / `#[expr] = expr`.
        if c.peek() == Some(b'#') {
            c.bump();
            match scan_var_ref(&mut c, cfg.map_var, 0) {
                Ok(VarParse::Ref(target)) => {
                    c.skip_ws();
                    if c.peek() == Some(b'=') {
                        c.bump();
                        let rhs = &text[c.i.min(text.len())..];
                        match parse_expr_text(rhs, cfg.map_var) {
                            Ok(value) => {
                                let stmt = L2Stmt::Assign {
                                    target,
                                    value,
                                    src: b.span(text.len()),
                                };
                                b.push(stmt);
                            }
                            Err(e) => b.err(format!(
                                "line {}: bad assignment expression: {e}",
                                b.cur_line
                            )),
                        }
                    } else {
                        b.err(format!(
                            "line {}: expected '=' after assignment target",
                            b.cur_line
                        ));
                    }
                }
                Ok(VarParse::Null) => {
                    b.err(format!(
                        "line {}: assignment to #0 is forbidden",
                        b.cur_line
                    ));
                }
                Err(e) => b.err(format!("line {}: bad variable reference: {e}", b.cur_line)),
            }
            continue;
        }

        // Кадр (слова G/M/буквы) → диспетчер.
        let fr = scan_frame(&mut c, cfg.map_var);
        b.dispatch_frame(fr, text);
    }

    // Незакрытые DO — ошибка (тест-требование «незакрытый DO»).
    let open: Vec<u32> = b.do_stack.iter().map(|(id, _)| *id).collect();
    for id in open {
        b.errors.push(L2Error::RuntimeError(format!(
            "unterminated DO{id} (no matching END{id})"
        )));
    }

    // Финализация текущей O-sub.
    if let Some(s) = b.in_sub.take() {
        b.subs.push(s);
    }

    // Вырезание тел M97-подпрограмм (Haas): от N-метки до первого M99;
    // stmts остаются и в main (поток исполнения — как в источнике).
    let m97: Vec<u32> = {
        let mut seen = BTreeSet::new();
        b.m97_targets
            .iter()
            .filter(|t| seen.insert(**t))
            .copied()
            .collect()
    };
    for t in m97 {
        if b.subs.iter().any(|s| s.target == SubTarget::NLabel(t)) {
            continue;
        }
        let Some(&start) = b.labels.get(&t) else {
            b.errors.push(L2Error::RuntimeError(format!(
                "M97 P{t}: label N{t} not found in main body"
            )));
            continue;
        };
        let end = (start..b.main.len())
            .find(|&k| b.m99_lines.contains(&b.main_lines[k]))
            .unwrap_or(b.main.len());
        b.subs.push(L2Sub {
            target: SubTarget::NLabel(t),
            body: b.main[start..end].to_vec(),
        });
    }

    if b.errors.is_empty() {
        Ok(L2Program {
            stmts: b.main,
            subs: b.subs,
            labels: b.labels,
        })
    } else {
        Err(b.errors)
    }
}

/// `IF [cond] …` → IfGoto | IfThen. После cond:
/// * `GOTO n` → IfGoto;
/// * `THEN #a = …` → IfThen{Assign};
/// * Haas bare-IF + присваивание → IfThen{Assign} (правило 3);
/// * Haas bare-IF + прочее → L1Op-носитель (вне §2.4);
/// * Fanuc bare-IF → ошибка.
fn parse_if_stmt(b: &mut Builder, c: &mut Cursor, text: &str) {
    c.skip_ws();
    let Some(cond_text) = capture_bracket(c) else {
        b.err(format!("line {}: IF without [condition]", b.cur_line));
        return;
    };
    let cond = match parse_expr_text(&cond_text, b.cfg.map_var) {
        Ok(e) => e,
        Err(e) => {
            b.err(format!("line {}: bad IF condition: {e}", b.cur_line));
            return;
        }
    };
    c.skip_ws();
    if c.starts_kw("GOTO") {
        for _ in 0..4 {
            c.bump();
        }
        c.skip_ws();
        match c.scan_number().and_then(|s| s.parse::<u32>().ok()) {
            Some(target) => {
                let stmt = L2Stmt::IfGoto {
                    cond,
                    target_label: target,
                    src: b.span(text.len()),
                };
                b.push(stmt);
            }
            None => b.err(format!("line {}: IF GOTO without target", b.cur_line)),
        }
        return;
    }
    if c.starts_kw("THEN") {
        for _ in 0..4 {
            c.bump();
        }
        if let Err(e) = parse_then_assign(b, c, text, cond) {
            b.err(format!("line {}: {e}", b.cur_line));
        }
        return;
    }
    // bare-IF (без THEN/GOTO).
    c.skip_ws();
    if b.cfg.allow_bare_if {
        if c.peek() == Some(b'#') {
            // `IF [cond] #3000= 1` → IfThen{Assign} (правило 3 рулинга).
            if let Err(e) = parse_then_assign(b, c, text, cond) {
                b.err(format!("line {}: {e}", b.cur_line));
            }
        } else if !c.eof() {
            // `IF [cond] S800 M04` — тело не выразимо в §2.4 → носитель.
            b.push_carrier(
                text,
                "bare-IF with non-assignable body (outside §2.4), Raw carrier",
            );
        } else {
            b.push_carrier(text, "IF without body (Raw carrier)");
        }
        return;
    }
    b.err(format!(
        "line {}: IF without GOTO/THEN is not Fanuc Macro B syntax",
        b.cur_line
    ));
}

/// Разобрать `#target = expr` после THEN/bare-IF → IfThen{Assign}.
fn parse_then_assign(
    b: &mut Builder,
    c: &mut Cursor,
    text: &str,
    cond: Expr,
) -> Result<(), String> {
    c.skip_ws();
    if c.peek() != Some(b'#') {
        return Err("THEN/bare-IF body must be an assignment (#var = expr)".into());
    }
    c.bump();
    let target = match scan_var_ref(c, b.cfg.map_var, 0)? {
        VarParse::Ref(r) => r,
        VarParse::Null => return Err("assignment to #0 is forbidden".into()),
    };
    c.skip_ws();
    if c.peek() != Some(b'=') {
        return Err("expected '=' in THEN assignment".into());
    }
    c.bump();
    let rhs = &text[c.i.min(text.len())..];
    let value = parse_expr_text(rhs, b.cfg.map_var)?;
    let inner = L2Stmt::Assign {
        target,
        value,
        src: b.span(text.len()),
    };
    b.push(L2Stmt::IfThen {
        cond,
        then_stmt: Box::new(inner),
        src: b.span(text.len()),
    });
    Ok(())
}

/// `WHILE [cond] DO m` — открыть тело (m в 1..=3).
fn parse_while_stmt(b: &mut Builder, c: &mut Cursor, text: &str) {
    let Some(cond_text) = capture_bracket(c) else {
        b.err(format!("line {}: WHILE without [condition]", b.cur_line));
        return;
    };
    let cond = match parse_expr_text(&cond_text, b.cfg.map_var) {
        Ok(e) => e,
        Err(e) => {
            b.err(format!("line {}: bad WHILE condition: {e}", b.cur_line));
            return;
        }
    };
    parse_do_open(b, c, text, cond);
}

/// `DO m` (после WHILE или самостоятельно) — открыть While-слот.
fn parse_do_open(b: &mut Builder, c: &mut Cursor, text: &str, cond: Expr) {
    c.skip_ws();
    if c.starts_kw("DO") {
        c.bump();
        c.bump();
        c.skip_ws();
    }
    let Some(digits) = c.scan_number() else {
        b.err(format!("line {}: DO without level number", b.cur_line));
        return;
    };
    let Ok(id) = digits.parse::<u32>() else {
        b.err(format!("line {}: bad DO level {digits:?}", b.cur_line));
        return;
    };
    if !(1..=3).contains(&id) {
        b.err(format!(
            "line {}: DO level {id} outside 1..=3 (Macro B nesting limit)",
            b.cur_line
        ));
        return;
    }
    let stmt = L2Stmt::While {
        cond,
        do_id: id,
        body: Vec::new(),
        src: b.span(text.len()),
    };
    b.do_stack.push((id, stmt));
}

/// `END m` — закрыть тело DO с проверкой парности.
fn parse_end_close(b: &mut Builder, c: &mut Cursor, _text: &str) {
    c.skip_ws();
    let Some(digits) = c.scan_number() else {
        b.err(format!("line {}: END without level number", b.cur_line));
        return;
    };
    let Ok(id) = digits.parse::<u32>() else {
        b.err(format!("line {}: bad END level {digits:?}", b.cur_line));
        return;
    };
    match b.do_stack.pop() {
        Some((open_id, stmt)) if open_id == id => {
            b.push(stmt);
        }
        Some((open_id, _)) => {
            b.err(format!(
                "line {}: END{id} does not match open DO{open_id}",
                b.cur_line
            ));
        }
        None => {
            b.err(format!("line {}: END{id} without DO", b.cur_line));
        }
    }
}

// ===========================================================================
// Тесты общего ядра (лексер, выражения, семантика операторов)
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn fanuc_map(n: u32) -> VarRef {
        match n {
            1..=33 | 100..=999 => VarRef::Direct(n),
            _ => map_common_system(n).unwrap_or(VarRef::Sys(SysVar::VendorSpecific(n))),
        }
    }

    fn cfg() -> DialectCfg {
        DialectCfg {
            name: "test-MB",
            allow_bare_if: false,
            allow_m97: false,
            mirror_g101: false,
            dwell: DwellSemantics::Fanuc,
            map_var: fanuc_map,
        }
    }

    fn e(text: &str) -> Expr {
        parse_expr_text(text, fanuc_map).unwrap()
    }

    fn v(n: u32) -> Expr {
        Expr::var(n)
    }

    /// Значение константы (verbatim-текст Num может отличаться от
    /// `Expr::num` — сравниваем ПО ЗНАЧЕНИЮ, SPEC §10.1/EQUIVALENCE §4).
    fn const_of(e: &Expr) -> Option<f64> {
        match e {
            Expr::Const(n) => Some(n.value()),
            _ => None,
        }
    }

    fn bin(op: BinaryOp, a: Expr, b: Expr) -> Expr {
        Expr::Binary(op, Box::new(a), Box::new(b))
    }

    // ---------- выражения ----------

    #[test]
    fn expr_number_verbatim() {
        match e("10.5") {
            Expr::Const(n) => {
                assert_eq!(n.value(), 10.5);
                assert_eq!(n.text, "10.5");
            }
            _ => panic!("const expected"),
        }
        match e(".04") {
            Expr::Const(n) => assert_eq!((n.value(), n.text.as_str()), (0.04, ".04")),
            _ => panic!("const expected"),
        }
        match e("150.") {
            Expr::Const(n) => assert_eq!(n.text, "150."),
            _ => panic!("const expected"),
        }
    }

    #[test]
    fn expr_unary_minus_and_nested() {
        // -.985 — verbatim-текст с ведущей точкой сохраняется (ncm_g434).
        match e("-.985") {
            Expr::Unary(UnaryOp::Neg, inner) => {
                assert_eq!(const_of(&inner), Some(0.985));
                if let Expr::Const(n) = &*inner {
                    assert_eq!(n.text, ".985");
                } else {
                    panic!("const expected");
                }
            }
            _ => panic!("neg expected"),
        }
        // [-#21] — вложенный унарный минус в скобках (mpe_g65_material_verify).
        assert_eq!(e("[-#21]"), Expr::Unary(UnaryOp::Neg, Box::new(v(21))));
        assert_eq!(
            e("-[#1 + #2]"),
            Expr::Unary(UnaryOp::Neg, Box::new(bin(BinaryOp::Add, v(1), v(2))))
        );
    }

    #[test]
    fn expr_precedence_and_vs_eq() {
        // Идиома Renishaw: `#170 AND 4 EQ 4` = (170 AND 4) EQ 4 —
        // AND ВЫШЕ сравнения.
        assert_eq!(
            e("#170 AND 4 EQ 4"),
            bin(
                BinaryOp::Eq,
                bin(BinaryOp::And, v(170), Expr::num(4.0)),
                Expr::num(4.0)
            )
        );
    }

    #[test]
    fn expr_precedence_arith() {
        assert_eq!(e("#9 * 0.33"), bin(BinaryOp::Mul, v(9), Expr::num(0.33)));
        // 3006 - ((170 AND 8) / 8 * 6) — rensh_g04p:118, косвенная цель.
        assert_eq!(
            e("3006 - [ [ #170 AND 8 ] / 8 * 6 ]"),
            bin(
                BinaryOp::Sub,
                Expr::num(3006.0),
                bin(
                    BinaryOp::Mul,
                    bin(
                        BinaryOp::Div,
                        bin(BinaryOp::And, v(170), Expr::num(8.0)),
                        Expr::num(8.0)
                    ),
                    Expr::num(6.0)
                )
            )
        );
    }

    #[test]
    fn expr_cmp_lowest() {
        // ABS[#5063 - #199] - #28 LT 0.05 * #29 (zapcon_g31:47);
        // #5063 в тестовой карте — Sys(SkipPos(Z)) (5061+2 → ось Z).
        let abs = Expr::Unary(
            UnaryOp::Abs,
            Box::new(bin(
                BinaryOp::Sub,
                Expr::Var(VarRef::Sys(SysVar::SkipPos(Axis::Z))),
                v(199),
            )),
        );
        let lhs = bin(BinaryOp::Sub, abs, v(28));
        let rhs = bin(BinaryOp::Mul, Expr::num(0.05), v(29));
        assert_eq!(
            e("ABS[ #5063 - #199 ] - #28 LT 0.05 * #29"),
            bin(BinaryOp::Lt, lhs, rhs)
        );
    }

    #[test]
    fn expr_functions_case_insensitive() {
        assert_eq!(e("abs[#17]"), Expr::Unary(UnaryOp::Abs, Box::new(v(17))));
        assert_eq!(
            e("SQRT[9]"),
            Expr::Unary(UnaryOp::Sqrt, Box::new(Expr::num(9.0)))
        );
    }

    #[test]
    fn expr_null_literal_and_indirect() {
        assert_eq!(e("#0"), Expr::Null);
        // #[#161 + 2] (rensh_g04p:34) — косвенная адресация.
        assert_eq!(
            e("#[#161 + 2]"),
            Expr::Var(VarRef::Indirect(Box::new(bin(
                BinaryOp::Add,
                v(161),
                Expr::num(2.0)
            ))))
        );
    }

    #[test]
    fn expr_exists_amendment() {
        // v0.1.1 амендмент: EXISTS[#var] → UnaryOp::Exists (аргумент —
        // обычная ссылка, здесь #5303 → Sys-карта теста).
        assert_eq!(
            e("EXISTS[#5303]"),
            Expr::Unary(
                UnaryOp::Exists,
                Box::new(Expr::Var(VarRef::Sys(SysVar::WcsOffset {
                    p: 5,
                    axis: Axis::Z
                })))
            )
        );
    }

    #[test]
    fn expr_atan2_amendment() {
        // v0.1.1 амендмент: ATAN[y]/[x] → BinaryOp::Atan2.
        assert_eq!(
            e("ATAN[1]/[1]"),
            bin(BinaryOp::Atan2, Expr::num(1.0), Expr::num(1.0))
        );
        // Одноаргументный ATAN остаётся Unary.
        assert_eq!(
            e("ATAN[1]"),
            Expr::Unary(UnaryOp::Atan, Box::new(Expr::num(1.0)))
        );
    }

    #[test]
    fn expr_errors() {
        assert!(parse_expr_text("#1 +", fanuc_map).is_err());
        assert!(parse_expr_text("FOO[#1]", fanuc_map).is_err());
        assert!(parse_expr_text("[#1", fanuc_map).is_err());
    }

    // ---------- eval амендментов (v0.1.1) ----------

    #[test]
    fn eval_exists_named_and_direct() {
        use crate::eval::{eval_expr, L2Env};
        let mut env = L2Env::new();
        env.write_named("defined", crate::ast::Val::Num(1.0));
        let ex_def = Expr::Unary(
            UnaryOp::Exists,
            Box::new(Expr::Var(VarRef::Named("defined".into()))),
        );
        let ex_undef = Expr::Unary(
            UnaryOp::Exists,
            Box::new(Expr::Var(VarRef::Named("nope".into()))),
        );
        assert_eq!(
            eval_expr(&ex_def, &mut env).unwrap(),
            crate::ast::Val::Num(1.0)
        );
        assert_eq!(
            eval_expr(&ex_undef, &mut env).unwrap(),
            crate::ast::Val::Num(0.0)
        );
        // EXISTS[#5303] → Num(1.0/0.0): 5303 вне диапазонов чтения → 0.0.
        let ex = Expr::Unary(UnaryOp::Exists, Box::new(v(5303)));
        assert_eq!(eval_expr(&ex, &mut env).unwrap(), crate::ast::Val::Num(0.0));
        // В диапазоне → 1.0.
        let ex = Expr::Unary(UnaryOp::Exists, Box::new(v(100)));
        assert_eq!(eval_expr(&ex, &mut env).unwrap(), crate::ast::Val::Num(1.0));
    }

    #[test]
    fn eval_atan2_degrees() {
        use crate::eval::{eval_expr, L2Env};
        let mut env = L2Env::new();
        let a = bin(BinaryOp::Atan2, Expr::num(1.0), Expr::num(1.0));
        assert!((eval_expr(&a, &mut env).unwrap().as_num() - 45.0).abs() < 1e-12);
        let b = bin(BinaryOp::Atan2, Expr::num(1.0), Expr::num(-1.0));
        assert!((eval_expr(&b, &mut env).unwrap().as_num() - 135.0).abs() < 1e-12);
    }

    // ---------- лексер/комментарии ----------

    #[test]
    fn strip_comments_balanced_and_stray() {
        assert_eq!(strip_line_comments("G1 X10 (note #5) Y2"), "G1 X10  Y2");
        // висячая ')' отбрасывается (vultr_g152: `G152 … D22)`).
        assert_eq!(
            strip_line_comments("G152 I2.81 J8 Z-.8 D22)"),
            "G152 I2.81 J8 Z-.8 D22"
        );
        // незакрытая '(' комментирует до конца кадра.
        assert_eq!(strip_line_comments("G1 X1 (oops"), "G1 X1 ");
        // ';' — комментарий до конца строки.
        assert_eq!(strip_line_comments("G1 X1 ; tail"), "G1 X1 ");
        assert_eq!(strip_line_comments("(IF NEEDED) G0"), " G0");
    }

    #[test]
    fn var_bits_suffix_encoding() {
        // #30057.25 → кодирование n*100+bits = 3005725 (VendorSpecific).
        let mut c = Cursor::new("#30057.25");
        c.bump();
        match scan_var_ref(&mut c, fanuc_map, 0).unwrap() {
            VarParse::Ref(VarRef::Sys(SysVar::VendorSpecific(n))) => assert_eq!(n, 3005725),
            _ => panic!("VendorSpecific expected"),
        }
    }

    #[test]
    fn keyword_detection_case_insensitive() {
        let c = Cursor::new("while [#1 le 147] do1");
        assert!(c.starts_kw("WHILE"));
        let c = Cursor::new("IF[#1EQ#0]GOTO942");
        assert!(c.starts_kw("IF"));
        let c = Cursor::new("goto100");
        assert!(c.starts_kw("GOTO"));
        let c = Cursor::new("END1");
        assert!(c.starts_kw("END"));
        // Кадр G1 — не keyword IF/WHILE/GOTO/DO/END.
        let c = Cursor::new("G1 X0");
        assert!(!c.starts_kw("IF"));
        assert!(!c.starts_kw("GOTO"));
        assert!(!c.starts_kw("DO"));
        assert!(!c.starts_kw("END"));
        assert!(!c.starts_kw("WHILE"));
    }

    // ---------- statement-парсер ----------

    #[test]
    fn parse_assign_plain_and_indirect() {
        let p = parse_macro_b(&cfg(), "#26=-.2\n#[#1]=#0\n#1 = #1 + 1\n").unwrap();
        assert_eq!(p.stmts.len(), 3);
        match &p.stmts[0] {
            L2Stmt::Assign { target, value, .. } => {
                assert_eq!(*target, VarRef::Direct(26));
                // verbatim: `-.2` → Neg(Const(".2")) — значение 0.2.
                match value {
                    Expr::Unary(UnaryOp::Neg, inner) => {
                        assert_eq!(const_of(inner), Some(0.2));
                        if let Expr::Const(n) = &**inner {
                            assert_eq!(n.text, ".2");
                        }
                    }
                    _ => panic!("neg expected"),
                }
            }
            _ => panic!("assign expected"),
        }
        // #[#1]=#0 — косвенная цель + Null-значение (#0-семантика сквозная).
        match &p.stmts[1] {
            L2Stmt::Assign { target, value, .. } => {
                assert_eq!(*target, VarRef::Indirect(Box::new(v(1))));
                assert_eq!(*value, Expr::Null);
            }
            _ => panic!("assign expected"),
        }
    }

    #[test]
    fn parse_while_do_end_pairing() {
        let p = parse_macro_b(
            &cfg(),
            "#1 = 100\nWHILE[ #1 LE 147] DO1\n#[#1]=#0\n#1 = #1 + 1\nEND1\n",
        )
        .unwrap();
        assert_eq!(p.stmts.len(), 2);
        match &p.stmts[1] {
            L2Stmt::While {
                cond, do_id, body, ..
            } => {
                assert_eq!(*do_id, 1);
                assert_eq!(*cond, bin(BinaryOp::Le, v(1), Expr::num(147.0)));
                assert_eq!(body.len(), 2); // #[#1]=#0 ; #1 = #1 + 1
            }
            _ => panic!("while expected"),
        }
    }

    #[test]
    fn unclosed_do_is_error() {
        let err = parse_macro_b(&cfg(), "WHILE [#1 LT 5] DO1\n#1 = #1 + 1\n").unwrap_err();
        assert!(err.iter().any(|e| e.message().contains("unterminated DO1")));
    }

    #[test]
    fn end_mismatch_is_error() {
        let err = parse_macro_b(&cfg(), "WHILE [#1 LT 5] DO1\nEND2\n").unwrap_err();
        assert!(err.iter().any(|e| e.message().contains("does not match")));
    }

    #[test]
    fn do_level_bounds() {
        let err = parse_macro_b(&cfg(), "WHILE [#1 LT 5] DO4\nEND4\n").unwrap_err();
        assert!(err.iter().any(|e| e.message().contains("outside 1..=3")));
    }

    #[test]
    fn nested_while() {
        let p = parse_macro_b(
            &cfg(),
            "WHILE [#1 LT 2] DO1\nWHILE [#2 LT 2] DO2\n#2 = #2 + 1\nEND2\n#1 = #1 + 1\nEND1\n",
        )
        .unwrap();
        match &p.stmts[0] {
            L2Stmt::While { do_id, body, .. } => {
                assert_eq!(*do_id, 1);
                assert_eq!(body.len(), 2);
                assert!(matches!(&body[0], L2Stmt::While { do_id: 2, .. }));
            }
            _ => panic!("outer while expected"),
        }
    }

    #[test]
    fn if_goto_and_cond() {
        let p = parse_macro_b(&cfg(), "IF[#26 EQ #0] GOTO942\nN942\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::IfGoto {
                cond, target_label, ..
            } => {
                assert_eq!(*target_label, 942);
                // IF [#26 EQ #0] — #0-семантика: cond = Eq(Var, Null).
                assert_eq!(*cond, bin(BinaryOp::Eq, v(26), Expr::Null));
            }
            _ => panic!("ifgoto expected"),
        }
        // N942 (строка-метка) — узел Label + запись в таблице.
        assert!(matches!(&p.stmts[1], L2Stmt::Label { n: 942, .. }));
        assert_eq!(p.labels.get(&942), Some(&1));
    }

    #[test]
    fn if_without_goto_then_is_fanuc_error() {
        let err = parse_macro_b(&cfg(), "IF [#1 GT 0] #1=1\n").unwrap_err();
        assert!(err
            .iter()
            .any(|e| e.message().contains("without GOTO/THEN")));
    }

    #[test]
    fn then_assign_wrap() {
        let cfg = DialectCfg {
            allow_bare_if: true,
            ..cfg()
        };
        let p = parse_macro_b(&cfg, "IF [ #27 LT 0 ] THEN #27= [ -1 * #27 ]\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::IfThen {
                cond, then_stmt, ..
            } => {
                assert_eq!(*cond, bin(BinaryOp::Lt, v(27), Expr::num(0.0)));
                assert!(matches!(**then_stmt, L2Stmt::Assign { .. }));
            }
            _ => panic!("ifthen expected"),
        }
    }

    #[test]
    fn haas_bare_if_assign_wrap() {
        // Правило 3 рулинга: `IF [#3026 GT 0] #3026=0` → IfThen{Assign}
        // (#3026 в тестовой fanuc-карте — VendorSpecific).
        let cfg = DialectCfg {
            allow_bare_if: true,
            ..cfg()
        };
        let p = parse_macro_b(&cfg, "IF [#3026 GT 0] #3026=0\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::IfThen { then_stmt, .. } => match &**then_stmt {
                L2Stmt::Assign { target, value, .. } => {
                    assert_eq!(*target, VarRef::Sys(SysVar::VendorSpecific(3026)));
                    assert_eq!(*value, Expr::num(0.0));
                }
                _ => panic!("assign expected"),
            },
            _ => panic!("ifthen expected"),
        }
    }

    #[test]
    fn bare_if_non_assign_body_is_carrier() {
        let cfg = DialectCfg {
            allow_bare_if: true,
            ..cfg()
        };
        let p = parse_macro_b(&cfg, "IF [ #4 LT 0 ] S800 M04\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::L1Op(Op::Raw { reason, .. }) => assert!(reason.contains("bare-IF")),
            _ => panic!("carrier expected"),
        }
    }

    #[test]
    fn unconditional_goto_is_ifgoto_true() {
        let p = parse_macro_b(&cfg(), "GOTO100\nN100\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::IfGoto {
                cond, target_label, ..
            } => {
                assert_eq!(*target_label, 100);
                assert_eq!(*cond, Expr::num(1.0));
            }
            _ => panic!("ifgoto expected"),
        }
    }

    #[test]
    fn g65_args_mapping() {
        let p = parse_macro_b(&cfg(), "G65 P9810 X2.5 Y-1. U.3 M1.\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::CallSub {
                target,
                args,
                modal,
                repeat,
                ..
            } => {
                assert_eq!(*target, SubTarget::ONumber(9810));
                assert!(!*modal);
                assert!(repeat.is_none());
                // A→#1 … Z→#26: X→#24, Y→#25, U→#21, M→#13 (M-слово — тоже
                // аргумент в G65-контексте: `G65 P9810 Z2. M1.`).
                assert_eq!(args[0].0, b'X');
                assert_eq!(const_of(&args[0].1), Some(2.5));
                assert_eq!(args[1].0, b'Y');
                assert!(
                    matches!(&args[1].1, Expr::Unary(UnaryOp::Neg, inner) if const_of(inner) == Some(1.0))
                );
                assert_eq!(args[2].0, b'U');
                assert_eq!(const_of(&args[2].1), Some(0.3));
                assert_eq!(args[3].0, b'M');
                assert_eq!(const_of(&args[3].1), Some(1.0));
            }
            _ => panic!("callsub expected"),
        }
    }

    #[test]
    fn g65_repeat_l() {
        let p = parse_macro_b(&cfg(), "G65 P9016 L2\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::CallSub { repeat, .. } => assert_eq!(*repeat, Some(Expr::num(2.0))),
            _ => panic!("callsub expected"),
        }
    }

    #[test]
    fn g66_modal_and_g67_carrier() {
        let p = parse_macro_b(&cfg(), "G66 P1000 A1.\nG67\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::CallSub { modal, args, .. } => {
                assert!(*modal);
                assert_eq!(args[0].0, b'A');
                assert_eq!(const_of(&args[0].1), Some(1.0));
            }
            _ => panic!("callsub expected"),
        }
        assert!(
            matches!(&p.stmts[1], L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G67"))
        );
    }

    #[test]
    fn m98_slip_format() {
        // P52000 = 5×O2000 (задание: M98-слип).
        let p = parse_macro_b(&cfg(), "M98 P52000\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::CallSub { target, repeat, .. } => {
                assert_eq!(*target, SubTarget::ONumber(2000));
                assert_eq!(*repeat, Some(Expr::num(5.0)));
            }
            _ => panic!("callsub expected"),
        }
        // Обычный формат: P2102 → O2102, repeat нет (cncsim_morsas).
        let p = parse_macro_b(&cfg(), "M98 P2102\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::CallSub { target, repeat, .. } => {
                assert_eq!(*target, SubTarget::ONumber(2102));
                assert!(repeat.is_none());
            }
            _ => panic!("callsub expected"),
        }
    }

    #[test]
    fn o_sub_collection() {
        let src = "%\nO2002 (MAIN)\nM98 P2102\nM30\nO2102 (SUB)\nG0 Z5.\nM99\n%\n";
        let p = parse_macro_b(&cfg(), src).unwrap();
        assert_eq!(p.subs.len(), 1);
        assert_eq!(p.subs[0].target, SubTarget::ONumber(2102));
        // M99 внутри sub → Return-узел (v0.20.1), тело = [Move, Return].
        assert_eq!(p.subs[0].body.len(), 2);
        assert!(matches!(&p.subs[0].body[0], L2Stmt::Move { .. }));
        assert!(matches!(&p.subs[0].body[1], L2Stmt::Return { .. }));
    }

    #[test]
    fn move_words_and_null_word_semantics() {
        // X#0 — слово со значением Null остаётся в AST как Expr::Null
        // (разрешение «слово игнорируется» — interp::pose_resolve, §2.1).
        let p = parse_macro_b(&cfg(), "G1 Z[#8 + .25] F#9\nG0 X#0 Y10.\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::Move {
                kind, target, feed, ..
            } => {
                assert!(matches!(kind, MoveKind::Linear));
                match &target.z {
                    Some(Expr::Binary(BinaryOp::Add, a, b)) => {
                        assert_eq!(**a, v(8));
                        assert_eq!(const_of(b), Some(0.25));
                    }
                    _ => panic!("z expr expected"),
                }
                assert_eq!(target.x, None);
                assert_eq!(*feed, Some(v(9)));
            }
            _ => panic!("move expected"),
        }
        match &p.stmts[1] {
            L2Stmt::Move { target, .. } => {
                assert_eq!(target.x, Some(Expr::Null));
                match &target.y {
                    Some(Expr::Const(n)) => assert_eq!(n.value(), 10.0),
                    _ => panic!("y const expected"),
                }
            }
            _ => panic!("move expected"),
        }
    }

    #[test]
    fn modal_motion_and_arc_r() {
        // Латть: G3X92.3035Z-1.407R10.5 — дуга R-формы; продолжение без G,
        // но и без I/J/K/R при модальной дуге → носитель «arc without».
        let p = parse_macro_b(&cfg(), "G3X92.3035Z-1.407R10.5\nX145.1Z-17.795\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::Move { kind, .. } => match kind {
                MoveKind::Arc { r, dir, ijk } => {
                    assert_eq!(*dir, Dir::Ccw);
                    match r.as_ref().unwrap() {
                        Expr::Const(n) => assert_eq!(n.text, "10.5"),
                        _ => panic!("const radius expected"),
                    }
                    assert!(ijk.iter().all(|e| e.is_none()));
                }
                _ => panic!("arc expected"),
            },
            _ => panic!("move expected"),
        }
        match target_of(&p.stmts[0]).x.as_ref().unwrap() {
            Expr::Const(n) => assert_eq!(n.text, "92.3035"),
            _ => panic!("const x expected"),
        }
        assert!(
            matches!(&p.stmts[1], L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("arc without"))
        );
    }

    fn target_of(s: &L2Stmt) -> &ExprPose {
        match s {
            L2Stmt::Move { target, .. } => target,
            _ => panic!("move expected"),
        }
    }

    #[test]
    fn modal_linear_continuation() {
        let p = parse_macro_b(&cfg(), "G0 Y0\nX151Z5.0\nG1X-2F0.25\nZ-28.\n").unwrap();
        assert!(matches!(
            &p.stmts[0],
            L2Stmt::Move {
                kind: MoveKind::Rapid,
                ..
            }
        ));
        assert!(matches!(
            &p.stmts[1],
            L2Stmt::Move {
                kind: MoveKind::Rapid,
                ..
            }
        ));
        match &p.stmts[2] {
            L2Stmt::Move { kind, feed, .. } => {
                assert!(matches!(kind, MoveKind::Linear));
                assert_eq!(*feed, Some(Expr::num(0.25)));
            }
            _ => panic!("move expected"),
        }
        assert!(matches!(
            &p.stmts[3],
            L2Stmt::Move {
                kind: MoveKind::Linear,
                ..
            }
        ));
    }

    #[test]
    fn rtcp_g434_and_g49() {
        // N-префиксы дают узлы Label (v0.20.1) перед RtcpMode.
        let p = parse_macro_b(&cfg(), "N120 G43.4\nG43.4 H1\nN110 G49\n").unwrap();
        // [Label{120}, RtcpMode+, RtcpMode+H1, Label{110}, RtcpMode-]
        assert!(matches!(&p.stmts[0], L2Stmt::Label { n: 120, .. }));
        assert!(matches!(
            &p.stmts[1],
            L2Stmt::RtcpMode {
                enabled: true,
                tool_length_h: None,
                ..
            }
        ));
        assert!(matches!(
            &p.stmts[2],
            L2Stmt::RtcpMode {
                enabled: true,
                tool_length_h: Some(1),
                ..
            }
        ));
        assert!(matches!(&p.stmts[3], L2Stmt::Label { n: 110, .. }));
        assert!(matches!(
            &p.stmts[4],
            L2Stmt::RtcpMode { enabled: false, .. }
        ));
    }

    #[test]
    fn carriers_for_out_of_scope() {
        let src = "G103 P1\nG31 Z#27 F500.\nG10 L20 P1 X0.\nM96 P10 Q17\nG28 U0V0\nS1000 M03\nG81 G98 Z1.5 Q0.1 R2.\nX1. Y-1.\nG80 Z2.\nG1 Y40. ,R4.\n/M58 (effort)\n";
        let p = parse_macro_b(&cfg(), src).unwrap();
        let reasons: Vec<&str> = p
            .stmts
            .iter()
            .filter_map(|s| match s {
                L2Stmt::L1Op(Op::Raw { reason, .. }) => Some(reason.as_str()),
                _ => None,
            })
            .collect();
        // G103; G31; G10; M96; G28; state-only (S1000 M03); G81;
        // продолжение цикла; G80; comma-word; block-delete.
        assert_eq!(reasons.len(), 11, "reasons: {reasons:?}");
        assert!(reasons[0].contains("G103"));
        assert!(reasons[1].contains("G31"));
        assert!(reasons[2].contains("G10"));
        assert!(reasons[3].contains("M96"));
        assert!(reasons[4].contains("G28"));
        assert!(reasons[5].contains("state-only"));
        assert!(reasons[6].contains("G81"));
        assert!(reasons[7].contains("canned-cycle"));
        assert!(reasons[8].contains("G80"));
        assert!(reasons[9].contains("junk"));
        assert!(reasons[10].contains("block-delete"));
        // Lossless: пейлоад — hex исходной строки.
        match &p.stmts[1] {
            L2Stmt::L1Op(Op::Raw { payload_hex, .. }) => {
                let decoded: Vec<u8> = (0..payload_hex.len() / 2)
                    .map(|i| u8::from_str_radix(&payload_hex[i * 2..i * 2 + 2], 16).unwrap())
                    .collect();
                assert_eq!(decoded, b"G31 Z#27 F500.");
            }
            _ => panic!("raw expected"),
        }
    }

    #[test]
    fn carrier_payload_is_lossless_hex() {
        let p = parse_macro_b(&cfg(), "M59 P2 (OTS Enable)\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::L1Op(Op::Raw {
                payload_hex, src, ..
            }) => {
                let decoded: String = (0..payload_hex.len() / 2)
                    .map(|i| {
                        u8::from_str_radix(&payload_hex[i * 2..i * 2 + 2], 16).unwrap() as char
                    })
                    .collect();
                assert_eq!(decoded, "M59 P2");
                assert_eq!(src.line_start, 1);
            }
            _ => panic!("carrier expected"),
        }
    }

    #[test]
    fn dwell_and_stop_and_end() {
        // cfg() — Fanuc-семантика dwell (амендрмент v0.1.2, BUG-D2):
        // P = МИЛЛИсекунды (и с точкой, и без), X/U = секунды; канон
        // Op::Dwell.seconds — всегда секунды.
        let src = "G4 P5.\nG04 P1\nG4 X2.\nG4 U0.5\nM01\nM0\nM30\n";
        let p = parse_macro_b(&cfg(), src).unwrap();
        assert_eq!(p.stmts.len(), 7);
        let secs: Vec<f64> = p.stmts[..4]
            .iter()
            .map(|s| match s {
                L2Stmt::L1Op(Op::Dwell { seconds, .. }) => seconds.value(),
                _ => panic!("dwell expected: {s:?}"),
            })
            .collect();
        assert_eq!(secs, vec![0.005, 0.001, 2.0, 0.5]);
        // Текст сконвертированных (мс→с) слов — текст значения; X/U —
        // verbatim-срез.
        assert_eq!(
            match &p.stmts[0] {
                L2Stmt::L1Op(Op::Dwell { seconds, .. }) => &seconds.text,
                _ => panic!(),
            },
            "0.005"
        );
        assert_eq!(
            match &p.stmts[2] {
                L2Stmt::L1Op(Op::Dwell { seconds, .. }) => &seconds.text,
                _ => panic!(),
            },
            "2."
        );
        assert!(matches!(
            &p.stmts[4],
            L2Stmt::L1Op(Op::ProgramStop { optional: true, .. })
        ));
        assert!(matches!(
            &p.stmts[5],
            L2Stmt::L1Op(Op::ProgramStop {
                optional: false,
                ..
            })
        ));
        assert!(matches!(&p.stmts[6], L2Stmt::L1Op(Op::ProgramEnd { .. })));
    }

    #[test]
    fn tool_change() {
        let p = parse_macro_b(&cfg(), "T42 M06\nT2 M6\nT#3026 M06\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::L1Op(Op::ToolChange { tool, .. }) => assert_eq!(*tool, 42),
            _ => panic!("toolchange expected"),
        }
        assert!(matches!(
            &p.stmts[1],
            L2Stmt::L1Op(Op::ToolChange { tool: 2, .. })
        ));
        // T с переменной → носитель (вне §2.4-формы).
        assert!(
            matches!(&p.stmts[2], L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("M6"))
        );
    }

    #[test]
    fn m99_is_return_node_v0201() {
        let src = "O100 (M)\nM99\nO200 (S)\nM99\n";
        let p = parse_macro_b(&cfg(), src).unwrap();
        // v0.20.1: M99 → L2Stmt::Return и в main, и в теле sub.
        assert_eq!(p.stmts.len(), 1);
        assert!(matches!(&p.stmts[0], L2Stmt::Return { .. }));
        assert_eq!(p.subs.len(), 1);
        assert_eq!(p.subs[0].body.len(), 1);
        assert!(matches!(&p.subs[0].body[0], L2Stmt::Return { .. }));
    }

    #[test]
    fn label_nodes_and_table() {
        // v0.20.1: N-метка → узел Label + таблица labels (главное тело).
        let src = "GOTO10\nN10 G1 Z1.\nN20\n";
        let p = parse_macro_b(&cfg(), src).unwrap();
        // stmts: [IfGoto, Label{10}, Move, Label{20}].
        assert_eq!(p.stmts.len(), 4);
        assert!(matches!(&p.stmts[1], L2Stmt::Label { n: 10, .. }));
        assert!(matches!(&p.stmts[2], L2Stmt::Move { .. }));
        assert!(matches!(&p.stmts[3], L2Stmt::Label { n: 20, .. }));
        // Таблица: N10 → индекс узла Label (переход продолжается С него);
        // N20 (строка-метка) → индекс её узла.
        assert_eq!(p.labels.get(&10), Some(&1));
        assert_eq!(p.labels.get(&20), Some(&3));
    }

    #[test]
    fn alarm_assignment_n_prefix() {
        // N941#3000=1(STOCK...) — метка-префикс + присваивание (mpe_deep_drill:41).
        let p = parse_macro_b(&cfg(), "N941#3000=1(STOCK ABOVE Z0 MUST BE POSITIVE)\n").unwrap();
        assert!(matches!(&p.stmts[0], L2Stmt::Label { n: 941, .. }));
        match &p.stmts[1] {
            L2Stmt::Assign { target, value, .. } => {
                assert_eq!(*target, VarRef::Sys(SysVar::VendorSpecific(3000)));
                assert_eq!(*value, Expr::num(1.0));
            }
            _ => panic!("assign expected"),
        }
        assert_eq!(p.labels.get(&941), Some(&0));
    }

    #[test]
    fn sys_var_mapping_common() {
        let p = parse_macro_b(
            &cfg(),
            "#27= #5021 - #5041\nIF [ #4011 EQ 50 ] GOTO30\n#1= #5221\n",
        )
        .unwrap();
        match &p.stmts[0] {
            L2Stmt::Assign { value, .. } => assert_eq!(
                *value,
                bin(
                    BinaryOp::Sub,
                    Expr::Var(VarRef::Sys(SysVar::AbsMachinePos(Axis::X))),
                    Expr::Var(VarRef::Sys(SysVar::WorkPos(Axis::X)))
                )
            ),
            _ => panic!("assign expected"),
        }
        match &p.stmts[1] {
            L2Stmt::IfGoto {
                cond, target_label, ..
            } => {
                assert_eq!(
                    *cond,
                    bin(
                        BinaryOp::Eq,
                        Expr::Var(VarRef::Sys(SysVar::ModalGroup(11))),
                        Expr::num(50.0)
                    )
                );
                assert_eq!(*target_label, 30);
            }
            _ => panic!("ifgoto expected"),
        }
        match &p.stmts[2] {
            L2Stmt::Assign { value, .. } => assert_eq!(
                *value,
                Expr::Var(VarRef::Sys(SysVar::WcsOffset {
                    p: 1,
                    axis: Axis::X
                }))
            ),
            _ => panic!("assign expected"),
        }
    }

    #[test]
    fn labels_do_not_record_in_sub() {
        // Метки внутри O-sub не попадают в таблицу M97-мишеней main (метка
        // N10 внутри O2 — только узел Label, per-body сбор lowering).
        let src = "O1 (M)\nM30\nO2 (S)\nN10 G0 Z1.\nM99\n";
        let p = parse_macro_b(&cfg(), src).unwrap();
        assert!(p.labels.is_empty());
        assert_eq!(p.subs.len(), 1);
        // Тело sub: [Label{10}, Move, Return].
        assert_eq!(p.subs[0].body.len(), 3);
        assert!(matches!(&p.subs[0].body[0], L2Stmt::Label { n: 10, .. }));
    }

    #[test]
    fn o_header_with_content_is_carrier() {
        let src = "O5 (M)\nO7 G1 X0\nM30\nO9 (S)\nM99\n";
        let p = parse_macro_b(&cfg(), src).unwrap();
        // Строка `O7 G1 X0` — контент на заголовке → носитель (в теле sub O7).
        assert!(p.stmts.is_empty());
        assert_eq!(p.subs.len(), 2);
        assert!(
            matches!(&p.subs[0].body[0], L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("O-header"))
        );
    }

    #[test]
    fn word_value_with_spaces() {
        // `S #[ #161 ]` — пробел между буквой и значением (rensh-цензор).
        let p = parse_macro_b(&cfg(), "G65 P9726 X#24 S #[ #161 ]\n").unwrap();
        match &p.stmts[0] {
            L2Stmt::CallSub { args, .. } => {
                assert_eq!(args.len(), 2);
                assert_eq!(args[0].0, b'X');
                assert_eq!(args[1].0, b'S');
                assert_eq!(args[1].1, Expr::Var(VarRef::Indirect(Box::new(v(161)))));
            }
            _ => panic!("callsub expected"),
        }
    }

    #[test]
    fn block_delete_line() {
        let p = parse_macro_b(&cfg(), "/M58 (CTRL EFFORT OT-ON)\n").unwrap();
        assert!(
            matches!(&p.stmts[0], L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("block-delete"))
        );
    }

    #[test]
    fn pure_modal_lines_are_silent() {
        let p = parse_macro_b(&cfg(), "G90\nG21 G17\nG54\nG0\n").unwrap();
        assert!(p.stmts.is_empty());
    }

    #[test]
    fn state_only_line_is_carrier() {
        let p = parse_macro_b(&cfg(), "S#19 M3\nM5\nM8(ARROSAGE-ON)\n").unwrap();
        assert_eq!(p.stmts.len(), 3);
        for s in &p.stmts {
            assert!(
                matches!(s, L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("state-only")),
                "{s:?}"
            );
        }
    }
}
