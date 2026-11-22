//! Парсер L2 для диалекта **LinuxCNC (O-word / RS274NGC параметрика)**.
//! Фаза 12.1, агент D2. Контракт зафиксирован оркестратором (сигнатура не
//! меняется):
//!
//! ```text
//! pub fn parse_program(src: &str) -> Result<L2Program, Vec<L2Error>>
//! ```
//!
//! Ценз: `corpus/tier_b/_l2_targets.md` §3/§8 — linuxcnc ×34 файла
//! (factorial/flowsnake/gosper — рекурсия; ngcgui_lib_* — арки; probe_basic ×3
//! — EXISTS/ATAN2; remap-subroutines ×2 — elseif/return; gmoccapy ×3 —
//! `#<_ini[..]>`/`#<_hal[..]>`; m61/m70/m73 — демо M-кодов; daisy/cone/gridprobe
//! — циклы и ротационные оси; polar_at_angle — `@radius^angle`; good-arc ×2 —
//! выражения в arc-словах).
//!
//! # Маппинг конструкций (рулинги оркестратора по цензу)
//!
//! | Конструкция LinuxCNC | L2 |
//! |---|---|
//! | `o<name|num> sub … endsub` | `L2Sub` (число → `SubTarget::ONumber`, имя → `SubTarget::Named`, амендмент v0.1.2) |
//! | `o… if [cond] / elseif / else / endif` | `L2Stmt::IfThen` — дублирующая десугаризация (решение 1 ниже) |
//! | `o… while [cond] … endwhile` | `L2Stmt::While` (`do_id` — числовой O-тег из 1..=3, иначе 1) |
//! | `o… call [a][b]… (L<expr>)` | `L2Stmt::CallSub{target, args: позиционные → буквы A..Z, repeat: L-слово, modal: false}` |
//! | `EXISTS[#<var>]` | `UnaryOp::Exists` (амендмент v0.1.1, внесён агентом B2) |
//! | `ATAN[y]/[x]` (и `ATAN [y] / [x]` с пробелами) | `BinaryOp::Atan2(y, x)` (амендмент v0.1.1, внесён агентом B2) |
//! | `(debug,…)/(print,…)/(msg,…)/(AXIS,notify,…)` | носитель `L2Stmt::L1Op(Op::Raw)` — байты verbatim (рулинг 4; msg/AXIS,notify — то же семейство исполняемых remarks LinuxCNC, corpus: on_abort/probe/arc1/db25/backlash/g76*) |
//! | `@radius^angle`, `^[expr]` | носитель `Op::Raw` всей строки (рулинг 5) |
//! | `return [expr]`, `endsub [expr]` | `L2Stmt::Return` (v0.20.1) + носитель `Op::Raw` для `[expr]` (рулинг 6: retval-канал — 12.2; bare `return` — только Return-узел; `endsub` в конце тела — неявный возврат, узел не нужен — контракт агента A v0.20.1) |
//! | M98 вне O-структуры | носитель `Op::Raw` (рулинг; греп-ценз: в 34 целевых файлах M98/M99 не встречаются) |
//! | обычные ходы G0/G1/G2/G3 (в т.ч. внутри O-блоков) | `L2Stmt::Move` (I/J/K/R → Arc) |
//! | `M0/M1/M2/M30` | точные L1-опы `Op::ProgramStop`/`Op::ProgramEnd` |
//! | `G4 P<конст>` | точный L1-оп `Op::Dwell` |
//!
//! # Решения по СТОП-вопросам задания (задокументированы, см. стоп-отчёт)
//!
//! 1. **else/elseif**: у `IfThen` v0.1 нет ветки else (Macro B) и нет узла
//!    блока → дублирующая десугаризация: каждый оператор ветки оборачивается
//!    в свой `IfThen`: `if c … else … endif` → `IfThen{c, s}` для каждого s
//!    then-ветки, `IfThen{!c, s}` — для else; цепочка elseif: cond ветки k =
//!    `AND(!c1, …, !c[k-1], c[k])`. Логическое НЕ —
//!    `!c := AND(c GE 0, c LE 0)` (true ⇔ c «falsy», включая Null → 0.0;
//!    сравнения дают 0/1, AND побитовый ⇒ точен на 0/1). Семантика
//!    эквивалентна для тел без операторов «выхода» (в корпусе ×34 — так;
//!    `return` внутри веток — носитель, решение 3).
//! 2. **break/continue**: `L2Stmt` v0.1 вариантов не имеет; десугаризация без
//!    узла блока невозможна → **ошибка парсa** с внятным сообщением.
//!    Греп-ценз: в 34 целевых файлах `break`/`continue` НЕ встречаются
//!    (grep-совпадения — только внутри комментариев `(debug, … to continue)`).
//! 3. **`return [expr]` / `endsub [expr]`** (обновлено по AST v0.20.1):
//!    bare `return` → узел `L2Stmt::Return` (тот же контракт, что M99 у
//!    агента A); `return [expr]` → Return-узел + носитель `Op::Raw` с
//!    verbatim-байтами `[expr]` (рулинг 6: retval-канал — 12.2);
//!    `endsub [expr]` → только носитель `[expr]` (конец тела — неявный
//!    возврат, pop кадра делает интерпретатор). До появления v0.20.1
//!    рулинг 6 предписывал носитель целиком — теперь control-flow несёт
//!    типизированный узел, а value-канал остаётся lossless-носителем.
//! 4. **do-while** (`o… do … endwhile` без while-строки): post-test цикл
//!    невыразим через pre-test `While` без флаговой переменной → **ошибка
//!    парсa**; хвостовой `do` после `while [cond]` и «декоративный» `o… do`
//!    внутри активного while-тела — пропускаются. В корпусе ×34 не
//!    встречается.
//! 5. **Строки, не выразимые в L2 v0.1** → целостный носитель `Op::Raw`
//!    (байты строки verbatim, lossless): все не-ходовые G (G20/G90/G91/G43/
//!    G49/G53/G10/G92/G28/G38.x/G33/G76/G81/G64/G61/G92.1/G43.1/…), M ≠
//!    {0,1,2,30}, параметрические `g#<expr>`/`m#<expr>`, слова T/U/V/W
//!    (у `ExprPose` v0.1 нет UVW), блочный delete `/…`. В т.ч. кадр
//!    «G91 + G0 + слова» — носитель целиком (модальному флагу дистанции нет
//!    места в Move v0.1; разбор — 12.2).
//! 6. **Модальный контекст G0–G3/F/S** — текстовое отслеживание в порядке
//!    чтения (последний ходовой G; последний F/S — со слов любого кадра).
//!    Строка ходов без явного G получает модальный kind (нет ни одного —
//!    Rapid); F/S-строки без ходов — носители, но модаль обновляют. Это
//!    ПАРС-аппроксимация: точная модальная развязка (пути вызовов subs) —
//!    12.2.
//! 7. **Именованные переменные**: `#<x>` и `#<_x>` → `VarRef::Named`
//!    (имя verbatim, подчёркивание сохраняется — различие локальных/глобальных
//!    имён LinuxCNC живёт в строке имени). **Отклонение от шапки-заготовки**
//!    («системные `#<_…>` → `VarRef::Sys`»): у `SysVar::VendorSpecific(u32)`
//!    нет носителя для строковых имён `#<_hal[…]>`/`#<_ini[…]>`/`#<_task>`,
//!    а рулинг ценза требует `#<_x>` → Named ⇒ ВСЕ `#<…>` → `Named`.
//!    Чтение отсутствующей именованной даёт Null (мягкая деградация в
//!    D0–D2 вместо останова E_RUNTIME_STATE) — стоп-отчёт, расхождение 1.
//! 8. **Числовые параметры**: 0 → `Expr::Null`; 1..=33 и 100..=599 →
//!    `Direct` (диапазоны `L2Env`); `#[expr]` → `Indirect`; известные
//!    системные диапазоны LinuxCNC/Fanuc → `SysVar` (5021..26 AbsMachinePos,
//!    5041..46 и 5420..26 WorkPos, 5061..66 SkipPos, 5221+/5241+/… WcsOffset);
//!    прочие (5220, 5399, 5400, 5410, …) → `Sys(VendorSpecific(n))` —
//!    чтение при неизвестном станке честно даёт E_RUNTIME_STATE.
//! 9. **Позиционные аргументы call** `[a][b]…` → буквы `b'A'+i-1` (i-й
//!    аргумент → локальная `#i`; максимум 26 — дальше букв нет, ошибка;
//!    max по корпусу — 20, arc2). `L<expr>` после/среди скобок → `repeat`.
//! 10. **O-теги**: `o<num>` → `ONumber`, `o<name>` → `Named`
//!     (`o<110>` канонизируется в ONumber(110) — probe-файлы). Парность
//!     тегов if/elseif/else/endif, while/endwhile, sub/endsub проверяется
//!     (расхождение тегов — ошибка; у factorial if-ветка носит СВОЙ тег
//!     `<factorialif>` — парится внутри себя, это легально).
//! 11. **Ошибки парсa** — `L2Error::RuntimeError` (класс E_RUNTIME_STATE):
//!     в `L2Error` v0.1 нет отдельного parse-варианта (стоп-отчёт,
//!     расхождение 2); все ошибки собираются в Vec (не fail-fast).
//! 12. **Спаны** — построчные: file="eid-l2" (метка как у
//!     `placeholder_span`), реальные line 1.., col 1..len+1.
//!
//! Не коммитить (по заданию).

use crate::ast::{
    Axis, BinaryOp, Expr, ExprPose, L2Program, L2Stmt, L2Sub, MoveKind, SubTarget, SysVar,
    UnaryOp, UnaryOp::Exists, VarRef,
};
use crate::eval::L2Error;
use eid_core::ast::{Dir, Num, Op, SrcSpan, State};
use eid_core::diag::ErrorCode;

/// Разобрать программу LinuxCNC в L2 AST (контракт — см. модульную шапку).
pub fn parse_program(src: &str) -> Result<L2Program, Vec<L2Error>> {
    let mut p = Parser::new(src);
    let (stmts, _hit) = p.parse_block(false, &[]);
    if p.errors.is_empty() {
        // labels: у LinuxCNC O-word диалекта НЕТ goto/N-переходов (M97 —
        // Haas-only) — N-слова это номера строк-метаданные, поэтому таблица
        // меток главного блока пуста (решение в шапке, контракт v0.20.1).
        Ok(L2Program::new(stmts, p.subs))
    } else {
        Err(p.errors)
    }
}

// ===========================================================================
// Ключевые слова O-управления (стоп-наборы рекурсивного спуска)
// ===========================================================================

/// Замыкающее ключевое слово, на котором останавливается [`Parser::parse_block`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctrl {
    Endsub,
    Endwhile,
    Else,
    Elseif,
    Endif,
}

/// Факт останова: ключевое слово + тег + хвост строки (cond у elseif,
/// `[retval]` у endsub) + verbatim-текст строки стоп-ключа (для носителя
/// retval — позиция парсера к моменту обработки уже на следующей строке).
struct Hit {
    ctrl: Ctrl,
    tag: Option<SubTarget>,
    line: u32,
    cond: Option<Expr>,
    retval: Option<Expr>,
    text: String,
}

/// Слово кадра: буква (lowercase) + значение-выражение.
struct Word {
    letter: u8,
    expr: Expr,
}

/// Результат разбора O-control строки.
enum OOutcome {
    /// Строка — стоп-ключ; вызывающий обрабатывает Hit.
    Hit(Hit),
    /// Управляющий БЛОК разобран (sub/while/if); позиция — начало СЛЕДУЮЩЕЙ
    /// строки.
    HandledBlock,
    /// Управляющий ОПЕРАТОР разобран (call/return/do); позиция — в середине
    /// строки (допить до конца).
    HandledLine,
    /// Ошибка записана, строку бросаем.
    Failed,
}

/// Итог разбора одной строки: statements + позиция парсера.
enum LineOut {
    /// Обычная строка (позиция в конце строки — допить).
    Stmt,
    /// Строка была стоп-ключом (позиция после хвоста ключа).
    Hit(Hit),
    /// Управляющий блок разобран (позиция уже на следующей строке).
    Done,
}

// ===========================================================================
// Парсер
// ===========================================================================

struct Parser<'a> {
    src: &'a str,
    b: &'a [u8],
    pos: usize,
    line: u32,
    line_start: usize,
    errors: Vec<L2Error>,
    subs: Vec<L2Sub>,
    /// Глубина сбора sub (вложенный sub — ошибка).
    sub_depth: u32,
    /// Модальный ходовой G (0..=3) — текстовая аппроксимация (решение 6).
    modal_g: Option<u32>,
    /// Модальный F — Expr (решение 6).
    modal_f: Option<Expr>,
    /// Модальный S — Expr (решение 6).
    modal_s: Option<Expr>,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            b: src.as_bytes(),
            pos: 0,
            line: 1,
            line_start: 0,
            errors: Vec::new(),
            subs: Vec::new(),
            sub_depth: 0,
            modal_g: None,
            modal_f: None,
            modal_s: None,
        }
    }

    // ----- низкоуровневые примитивы ----------------------------------------

    fn eof(&self) -> bool {
        self.pos >= self.b.len()
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }

    fn peek_at(&self, off: usize) -> Option<u8> {
        self.b.get(self.pos + off).copied()
    }

    /// Пропустить пробелы/табуляции/CR в пределах строки.
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\t' || c == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn at_line_end(&self) -> bool {
        self.eof() || self.peek() == Some(b'\n')
    }

    fn skip_to_eol(&mut self) {
        while !self.at_line_end() {
            self.pos += 1;
        }
    }

    /// Допить строку и перевести курсор на следующую.
    fn finish_line(&mut self) {
        self.skip_to_eol();
        if self.peek() == Some(b'\n') {
            self.pos += 1;
            self.line += 1;
            self.line_start = self.pos;
        }
    }

    /// Пропустить пустые строки (whitespace + новые строки).
    fn skip_blank(&mut self) {
        loop {
            self.skip_ws();
            if self.peek() == Some(b'\n') {
                self.pos += 1;
                self.line += 1;
                self.line_start = self.pos;
            } else {
                break;
            }
        }
    }

    /// Текст текущей строки (от line_start до EOL), обрезанный по краям.
    fn trimmed_line_text(&self) -> &'a str {
        let eol = self.b[self.line_start..]
            .iter()
            .position(|&c| c == b'\n')
            .map(|i| self.line_start + i)
            .unwrap_or(self.b.len());
        self.src[self.line_start..eol].trim()
    }

    fn line_span(&self, ln: u32) -> SrcSpan {
        SrcSpan {
            file: std::sync::Arc::from("eid-l2"),
            line_start: ln,
            col_start: 1,
            line_end: ln,
            col_end: self.trimmed_line_text().len() as u32 + 1,
        }
    }

    fn err(&mut self, ln: u32, msg: impl std::fmt::Display) {
        self.errors.push(L2Error::RuntimeError(format!(
            "parse_linuxcnc: line {ln}: {msg}"
        )));
    }

    /// Носитель `Op::Raw`: байты verbatim (решения 3/5, рулинги 4/5/6).
    fn raw_carrier(&self, ln: u32, payload: &str, reason: &str) -> L2Stmt {
        let payload_hex: String =
            payload.as_bytes().iter().map(|b| format!("{b:02X}")).collect();
        L2Stmt::L1Op(Op::Raw {
            payload_hex,
            reason: reason.to_string(),
            code: ErrorCode::RuntimeState,
            src: self.line_span(ln),
        })
    }

    // ----- блок/строки ------------------------------------------------------

    /// Разобрать последовательность строк до стоп-ключа (или EOF).
    /// `in_while` — внутри тела while (декоративный `o… do` разрешён).
    fn parse_block(&mut self, in_while: bool, stops: &[Ctrl]) -> (Vec<L2Stmt>, Option<Hit>) {
        let mut acc = Vec::new();
        loop {
            self.skip_blank();
            if self.eof() {
                return (acc, None);
            }
            let mut sink = Vec::new();
            match self.parse_line(&mut sink, in_while, stops) {
                LineOut::Hit(h) => {
                    acc.append(&mut sink);
                    self.finish_line(); // допить строку стоп-ключа
                    return (acc, Some(h));
                }
                LineOut::Stmt => {
                    acc.append(&mut sink);
                    self.finish_line();
                }
                // блок (sub/while/if) разобран целиком — позиция уже на
                // следующей строке; finish_line НЕ вызывать
                LineOut::Done => acc.append(&mut sink),
            }
        }
    }

    /// Разобрать ОДНУ логическую строку: remarks, присваивания, O-control,
    /// слова ходов.
    fn parse_line(
        &mut self,
        sink: &mut Vec<L2Stmt>,
        in_while: bool,
        stops: &[Ctrl],
    ) -> LineOut {
        let ln = self.line;
        let mut words: Vec<Word> = Vec::new();
        loop {
            self.skip_ws();
            if self.at_line_end() {
                break;
            }
            let c = self.peek().unwrap();
            match c {
                // `;`-комментарий (LinuxCNC EOB/коммент) и `%`-маркер программы.
                b';' | b'%' => {
                    self.skip_to_eol();
                    break;
                }
                // Блочный delete `/…` — целостный носитель (решение 5).
                b'/' => {
                    let text = self.trimmed_line_text();
                    sink.push(self.raw_carrier(ln, text, REASON_BLOCK_DELETE));
                    self.skip_to_eol();
                    return LineOut::Stmt;
                }
                // Полярные координаты `@radius^angle` / `^[expr]` — носитель
                // всей строки (рулинг 5).
                b'@' | b'^' => {
                    let text = self.trimmed_line_text();
                    sink.push(self.raw_carrier(ln, text, REASON_POLAR));
                    self.skip_to_eol();
                    return LineOut::Stmt;
                }
                // Paren-remark: комментарий или исполняемый statement
                // (debug/print/msg/AXIS,notify — рулинг 4).
                b'(' => match self.scan_paren(ln) {
                    Ok(remark) => {
                        if let Some(reason) = remark_family(remark) {
                            sink.push(self.raw_carrier(ln, remark, reason));
                        }
                    }
                    Err(()) => {
                        self.skip_to_eol();
                        break;
                    }
                },
                // Присваивание `#var = expr` (несколько на строку — ок).
                b'#' => match self.parse_assign(ln) {
                    Some(a) => sink.push(a),
                    None => {
                        self.skip_to_eol();
                        break;
                    }
                },
                // O-control: sub/while/if/call/return/… и стоп-ключи.
                b'o' | b'O' => match self.o_control(sink, in_while, stops, ln) {
                    OOutcome::Hit(h) => return LineOut::Hit(h),
                    // блок разобран целиком — мы уже на следующей строке
                    OOutcome::HandledBlock => return LineOut::Done,
                    OOutcome::HandledLine | OOutcome::Failed => {
                        self.skip_to_eol();
                        break;
                    }
                },
                _ if c.is_ascii_alphabetic() => match self.parse_word(ln) {
                    Some(w) => words.push(w),
                    None => {
                        self.skip_to_eol();
                        break;
                    }
                },
                _ => {
                    self.err(ln, format!("unexpected character '{}'", c as char));
                    self.skip_to_eol();
                    break;
                }
            }
        }
        if !words.is_empty() {
            self.classify_words(words, ln, sink);
        }
        LineOut::Stmt
    }

    // ----- paren-remarks ----------------------------------------------------

    /// Скан `(` … `)` (замечание не пересекает строку). Возвращает текст
    /// БЕЗ скобок. Ошибка — незакрытая скобка (записана, строка бросается).
    fn scan_paren(&mut self, ln: u32) -> Result<&'a str, ()> {
        self.pos += 1; // '('
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == b')' || c == b'\n' {
                break;
            }
            self.pos += 1;
        }
        if self.peek() != Some(b')') {
            self.err(ln, "unterminated `(...)` remark");
            return Err(());
        }
        let text = &self.src[start..self.pos];
        self.pos += 1; // ')'
        Ok(text)
    }

    // ----- присваивания -----------------------------------------------------

    /// `#<var> = expr` (target может быть Named/Direct/Indirect).
    fn parse_assign(&mut self, ln: u32) -> Option<L2Stmt> {
        let target_expr = self.parse_varref(ln)?;
        let target = match target_expr {
            Expr::Var(r) => r,
            Expr::Null => {
                self.err(ln, "assignment to literal #0 is forbidden");
                return None;
            }
            _ => {
                self.err(ln, "assignment target must be a parameter reference");
                return None;
            }
        };
        self.skip_ws();
        if self.peek() != Some(b'=') {
            self.err(ln, "expected `=` after parameter reference");
            return None;
        }
        self.pos += 1;
        self.skip_ws();
        let value = self.parse_expr(ln);
        Some(L2Stmt::Assign {
            target,
            value,
            src: self.line_span(ln),
        })
    }

    // ----- слова кадра ------------------------------------------------------

    /// Одно слово: буква + значение (`x#<v>`, `g0`, `z[expr]`, `Z-[…]`,
    /// `z-3.05`, `F #<_hal[…]>`). N-слово (номер строки) потребляется,
    /// но в классификацию не попадает.
    fn parse_word(&mut self, ln: u32) -> Option<Word> {
        let letter = self.b[self.pos].to_ascii_lowercase();
        self.pos += 1;
        self.skip_ws();
        let mut neg = false;
        let raw = match self.peek() {
            // знак перед числом: уходит в verbatim-текст Num
            // (`z-3.05` → Const("-3.05"); parse_number съедает знак сам)
            Some(b'-') | Some(b'+')
                if matches!(self.peek_at(1), Some(c) if c.is_ascii_digit() || c == b'.') =>
            {
                self.parse_number(ln)?
            }
            // знак перед переменной/скобкой — унарный (`Y-[#<w>]`)
            Some(b'-') => {
                self.pos += 1;
                neg = true;
                self.skip_ws();
                self.parse_word_value(ln)?
            }
            _ => self.parse_word_value(ln)?,
        };
        let expr = if neg {
            Expr::Unary(UnaryOp::Neg, Box::new(raw))
        } else {
            raw
        };
        // Модальный трекинг (решение 6): G0..G3 / F / S — со слов любого кадра.
        match letter {
            b'g' => {
                if let Some(v) = const_int(&expr) {
                    if (0..=3).contains(&v) {
                        self.modal_g = Some(v);
                    }
                }
            }
            b'f' => self.modal_f = Some(expr.clone()),
            b's' => self.modal_s = Some(expr.clone()),
            _ => {}
        }
        if letter == b'n' {
            // N-номер — метаданные кадра, в AST/classификацию не входит.
            return Some(Word {
                letter: b'n',
                expr: Expr::Null,
            });
        }
        Some(Word { letter, expr })
    }

    /// Значение слова: `#var` | `[expr]` | число.
    fn parse_word_value(&mut self, ln: u32) -> Option<Expr> {
        match self.peek() {
            Some(b'#') => self.parse_varref(ln),
            Some(b'[') => Some(self.parse_bracketed(ln)),
            Some(c) if c.is_ascii_digit() || c == b'.' => self.parse_number(ln),
            _ => {
                self.err(ln, "word letter without value");
                None
            }
        }
    }

    // ----- числа ------------------------------------------------------------

    /// Число: `10.`, `.5`, `0.00099`, `-180`. Знак (до/после пробелов)
    /// входит в verbatim-текст.
    fn parse_number(&mut self, ln: u32) -> Option<Expr> {
        let mut neg = false;
        if matches!(self.peek(), Some(b'-') | Some(b'+')) {
            neg = self.peek() == Some(b'-');
            self.pos += 1;
            self.skip_ws();
        }
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if self.pos == start {
            self.err(ln, "malformed number");
            return None;
        }
        let digits = &self.src[start..self.pos];
        let text = if neg {
            format!("-{digits}")
        } else {
            digits.to_string()
        };
        let v: f64 = text.parse().ok()?;
        Some(Expr::Const(Num::new(text, v)))
    }

    // ----- переменные -------------------------------------------------------

    /// `#<name>` / `#123` / `#[expr]` / `#0` (решение 7/8).
    fn parse_varref(&mut self, ln: u32) -> Option<Expr> {
        self.pos += 1; // '#'
        if self.peek() == Some(b'<') {
            self.pos += 1;
            let start = self.pos;
            while let Some(c) = self.peek() {
                if c == b'>' || c == b'\n' {
                    break;
                }
                self.pos += 1;
            }
            if self.peek() != Some(b'>') {
                self.err(ln, "unterminated `#<name>` reference");
                return None;
            }
            let name = &self.src[start..self.pos];
            self.pos += 1; // '>'
            return Some(Expr::Var(VarRef::Named(name.to_string())));
        }
        // косвенная адресация #[expr] — скобка сразу после '#'
        if self.peek() == Some(b'[') {
            let inner = self.parse_bracketed(ln);
            return Some(Expr::Var(VarRef::Indirect(Box::new(inner))));
        }
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.pos == start {
            self.err(ln, "malformed parameter reference after `#`");
            return None;
        }
        let idx: u32 = self.src[start..self.pos].parse().ok()?;
        if idx == 0 {
            return Some(Expr::Null);
        }
        Some(Expr::Var(number_var(idx)))
    }

    // ----- выражения --------------------------------------------------------

    /// `[expr]` (пробелы перед скобкой разрешены — `TAN [29]`,
    /// `ATAN [y] / [x]` в корпусе).
    fn parse_bracketed(&mut self, ln: u32) -> Expr {
        self.skip_ws();
        if self.peek() == Some(b'[') {
            self.pos += 1;
        } else {
            self.err(ln, "expected `[`");
            return Expr::num(0.0);
        }
        let e = self.parse_expr(ln);
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
        } else {
            self.err(ln, "expected `]`");
        }
        e
    }

    /// Выражение (тиры: AND/OR/XOR < сравнения < + - < * / MOD < унарные).
    fn parse_expr(&mut self, ln: u32) -> Expr {
        let mut lhs = self.parse_tier_and(ln);
        while let Some(op) = self.peek_word_op(&[("OR", BinaryOp::Or), ("XOR", BinaryOp::Xor)]) {
            let rhs = self.parse_tier_and(ln);
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        lhs
    }

    fn parse_tier_and(&mut self, ln: u32) -> Expr {
        let mut lhs = self.parse_tier_rel(ln);
        while let Some(op) = self.peek_word_op(&[("AND", BinaryOp::And)]) {
            let rhs = self.parse_tier_rel(ln);
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        lhs
    }

    fn parse_tier_rel(&mut self, ln: u32) -> Expr {
        let mut lhs = self.parse_tier_add(ln);
        while let Some(op) = self.peek_word_op(&[
            ("EQ", BinaryOp::Eq),
            ("NE", BinaryOp::Ne),
            ("LT", BinaryOp::Lt),
            ("LE", BinaryOp::Le),
            ("GT", BinaryOp::Gt),
            ("GE", BinaryOp::Ge),
        ]) {
            let rhs = self.parse_tier_add(ln);
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        lhs
    }

    fn parse_tier_add(&mut self, ln: u32) -> Expr {
        let mut lhs = self.parse_tier_mul(ln);
        loop {
            self.skip_ws();
            let op = match self.peek() {
                Some(b'+') => Some(BinaryOp::Add),
                Some(b'-') => Some(BinaryOp::Sub),
                _ => None,
            };
            match op {
                Some(op) => {
                    self.pos += 1;
                    let rhs = self.parse_tier_mul(ln);
                    lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
                }
                None => break,
            }
        }
        lhs
    }

    fn parse_tier_mul(&mut self, ln: u32) -> Expr {
        let mut lhs = self.parse_unary(ln);
        loop {
            self.skip_ws();
            let op = if self.peek() == Some(b'*') {
                self.pos += 1;
                Some(BinaryOp::Mul)
            } else if self.peek() == Some(b'/') {
                self.pos += 1;
                Some(BinaryOp::Div)
            } else {
                self.peek_word_op(&[("MOD", BinaryOp::Mod)])
            };
            match op {
                Some(op) => {
                    let rhs = self.parse_unary(ln);
                    lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
                }
                None => break,
            }
        }
        lhs
    }

    fn parse_unary(&mut self, ln: u32) -> Expr {
        self.skip_ws();
        match self.peek() {
            Some(b'-') => {
                // минус перед числом → verbatim «-90» в Const (parse_number
                // съедает знак сам); иначе — унарный Neg
                let mut probe = self.pos + 1;
                while matches!(self.b.get(probe), Some(b' ') | Some(b'\t')) {
                    probe += 1;
                }
                if matches!(self.b.get(probe), Some(c) if c.is_ascii_digit() || *c == b'.') {
                    return self.parse_number(ln).unwrap_or(Expr::num(0.0));
                }
                self.pos += 1;
                let inner = self.parse_unary(ln);
                Expr::Unary(UnaryOp::Neg, Box::new(inner))
            }
            Some(b'+') => {
                self.pos += 1;
                self.parse_unary(ln)
            }
            _ => self.parse_primary(ln),
        }
    }

    fn parse_primary(&mut self, ln: u32) -> Expr {
        self.skip_ws();
        match self.peek() {
            Some(b'#') => self.parse_varref(ln).unwrap_or(Expr::num(0.0)),
            Some(b'[') => self.parse_bracketed(ln),
            Some(c) if c.is_ascii_digit() || c == b'.' => {
                self.parse_number(ln).unwrap_or(Expr::num(0.0))
            }
            Some(c) if c.is_ascii_alphabetic() => self.parse_function(ln),
            _ => {
                self.err(ln, "expected expression term");
                Expr::num(0.0)
            }
        }
    }

    /// Функция выражения: SIN/COS/TAN/ASIN/ACOS/SQRT/LN/EXP/ABS/ROUND/FIX/
    /// FUP (одноаргументные, скобка обязательна), EXISTS, ATAN (+ форма
    /// `ATAN[y]/[x]` — амендмент v0.1.1). Регистронезависимо; пробелы
    /// между именем и скобкой разрешены (`TAN [29]`, `ATAN [#a] / [#b]`).
    fn parse_function(&mut self, ln: u32) -> Expr {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        let name = self.src[start..self.pos].to_ascii_uppercase();
        match name.as_str() {
            "ATAN" => {
                let y = self.parse_bracketed(ln);
                // двух-аргументная форма `ATAN[y]/[x]` (LinuxCNC: `/[…]`
                // сразу после скобки ATAN — второй аргумент, не деление)
                let save = self.pos;
                self.skip_ws();
                if self.peek() == Some(b'/') {
                    self.pos += 1;
                    self.skip_ws();
                    if self.peek() == Some(b'[') {
                        let x = self.parse_bracketed(ln);
                        return Expr::Binary(BinaryOp::Atan2, Box::new(y), Box::new(x));
                    }
                    self.pos = save;
                }
                Expr::Unary(UnaryOp::Atan, Box::new(y))
            }
            "EXISTS" => {
                let inner = self.parse_bracketed(ln);
                Expr::Unary(Exists, Box::new(inner))
            }
            "SIN" => unary_arg(self, ln, UnaryOp::Sin),
            "COS" => unary_arg(self, ln, UnaryOp::Cos),
            "TAN" => unary_arg(self, ln, UnaryOp::Tan),
            "ASIN" => unary_arg(self, ln, UnaryOp::Asin),
            "ACOS" => unary_arg(self, ln, UnaryOp::Acos),
            "SQRT" => unary_arg(self, ln, UnaryOp::Sqrt),
            "LN" => unary_arg(self, ln, UnaryOp::Ln),
            "EXP" => unary_arg(self, ln, UnaryOp::Exp),
            "ABS" => unary_arg(self, ln, UnaryOp::Abs),
            "ROUND" => unary_arg(self, ln, UnaryOp::Round),
            "FIX" => unary_arg(self, ln, UnaryOp::Fix),
            "FUP" => unary_arg(self, ln, UnaryOp::Fup),
            _ => {
                self.pos = start; // откат: слово не функция — его увидит строка
                self.err(ln, format!("unknown word `{name}` inside expression"));
                Expr::num(0.0)
            }
        }
    }

    // ----- операторы-слова --------------------------------------------------

    /// Word-оператор (AND/OR/XOR/EQ/…)? Если да — потребить и вернуть.
    fn peek_word_op(&mut self, table: &[(&str, BinaryOp)]) -> Option<BinaryOp> {
        let save = self.pos;
        self.skip_ws();
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        if self.pos == start {
            self.pos = save;
            return None;
        }
        let word = self.src[start..self.pos].to_ascii_uppercase();
        // граница слова: следующий символ не буквенно-цифровой
        if matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric()) {
            self.pos = save;
            return None;
        }
        for (kw, op) in table {
            if *kw == word {
                return Some(*op);
            }
        }
        self.pos = save;
        None
    }

    // ----- O-control ----------------------------------------------------------

    fn o_control(
        &mut self,
        sink: &mut Vec<L2Stmt>,
        in_while: bool,
        stops: &[Ctrl],
        ln: u32,
    ) -> OOutcome {
        self.pos += 1; // 'o'/'O'
        self.skip_ws();
        let tag = match self.parse_o_tag(ln) {
            Some(t) => t,
            None => return OOutcome::Failed,
        };
        self.skip_ws();
        let kw_start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        if self.pos == kw_start {
            self.err(ln, "O-word without control keyword (sub/while/if/call/…)");
            return OOutcome::Failed;
        }
        let kw = self.src[kw_start..self.pos].to_ascii_lowercase();

        // стоп-ключи
        let stop = match kw.as_str() {
            "endsub" => Some(Ctrl::Endsub),
            "endwhile" => Some(Ctrl::Endwhile),
            "else" => Some(Ctrl::Else),
            "elseif" => Some(Ctrl::Elseif),
            "endif" => Some(Ctrl::Endif),
            _ => None,
        };
        if let Some(sc) = stop {
            if stops.contains(&sc) {
                let mut cond = None;
                let mut retval = None;
                match sc {
                    Ctrl::Elseif => {
                        self.skip_ws();
                        if self.peek() == Some(b'[') {
                            cond = Some(self.parse_bracketed(ln));
                        } else {
                            self.err(ln, "`elseif` without `[cond]`");
                        }
                    }
                    Ctrl::Endsub => {
                        self.skip_ws();
                        if self.peek() == Some(b'[') {
                            retval = Some(self.parse_bracketed(ln));
                        }
                    }
                    _ => {}
                }
                let text = self.trimmed_line_text().to_string();
                return OOutcome::Hit(Hit {
                    ctrl: sc,
                    tag: Some(tag),
                    line: ln,
                    cond,
                    retval,
                    text,
                });
            }
            self.err(ln, format!("`o… {kw}` without matching opener"));
            return OOutcome::Failed;
        }

        match kw.as_str() {
            "sub" => self.handle_sub(tag, ln),
            "while" => self.handle_while(sink, tag, ln),
            "do" => self.handle_do(in_while, ln),
            "if" => self.handle_if(sink, tag, ln),
            "call" => self.handle_call(sink, tag, ln),
            "return" => self.handle_return(sink, ln),
            "break" | "continue" => {
                // решение 2: вариантов в L2Stmt v0.1 нет; в корпусе ×34 не
                // встречаются (греп-ценз) — ошибка с объяснением.
                self.err(
                    ln,
                    format!(
                        "`o… {kw}` is not representable in L2 v0.1 (no Break/Continue \
                         statement variant); corpus ×34 does not use it — defer to 12.2"
                    ),
                );
                OOutcome::Failed
            }
            _ => {
                self.err(ln, format!("unknown O-control keyword `{kw}`"));
                OOutcome::Failed
            }
        }
    }

    fn parse_o_tag(&mut self, ln: u32) -> Option<SubTarget> {
        if self.peek() == Some(b'<') {
            self.pos += 1;
            let start = self.pos;
            while let Some(c) = self.peek() {
                if c == b'>' || c == b'\n' {
                    break;
                }
                self.pos += 1;
            }
            if self.peek() != Some(b'>') {
                self.err(ln, "unterminated `o<name>` tag");
                return None;
            }
            let name = &self.src[start..self.pos];
            self.pos += 1; // '>'
            return Some(canonical_target(name));
        }
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b'.')
        {
            self.pos += 1;
        }
        if self.pos == start {
            self.err(ln, "`o` without tag");
            return None;
        }
        Some(canonical_target(&self.src[start..self.pos]))
    }

    /// `o… sub … endsub` → L2Sub (решение 3: retval — носитель).
    fn handle_sub(&mut self, tag: SubTarget, ln: u32) -> OOutcome {
        if self.sub_depth > 0 {
            self.err(ln, "nested `sub` definition is not supported");
            // проглотить тело до парного endsub (best-effort)
            let _ = self.parse_block(false, &[Ctrl::Endsub]);
            return OOutcome::HandledBlock;
        }
        self.sub_depth += 1;
        let (mut body, hit) = self.parse_block(false, &[Ctrl::Endsub]);
        self.sub_depth -= 1;
        match hit {
            Some(Hit {
                ctrl: Ctrl::Endsub,
                tag: t2,
                line: l2,
                retval,
                text,
                ..
            }) => {
                self.check_tag(&tag, t2.as_ref(), l2, "sub/endsub");
                if retval.is_some() {
                    // рулинг 6: `endsub [expr]` — носитель retval; конец тела —
                    // неявный возврат (контракт агента A v0.20.1)
                    body.push(self.raw_carrier(l2, &text, REASON_ENDSUB_RETVAL));
                }
                self.subs.push(L2Sub { target: tag, body });
            }
            _ => self.err(ln, "unterminated `sub` — no matching `endsub`"),
        }
        OOutcome::HandledBlock
    }

    /// `o… while [cond] … endwhile` → While (решение 4: хвостовой `do` —
    /// декорация).
    fn handle_while(&mut self, sink: &mut Vec<L2Stmt>, tag: SubTarget, ln: u32) -> OOutcome {
        self.skip_ws();
        let cond = if self.peek() == Some(b'[') {
            self.parse_bracketed(ln)
        } else {
            self.err(ln, "`while` without `[cond]`");
            Expr::num(0.0)
        };
        // опциональный декоративный `do` в хвосте while-строки
        self.skip_ws();
        if self.peek_word_is("do") {
            self.pos += 2;
        }
        let src = self.line_span(ln);
        let (body, hit) = self.parse_block(true, &[Ctrl::Endwhile]);
        match hit {
            Some(Hit {
                ctrl: Ctrl::Endwhile,
                tag: t2,
                line: l2,
                ..
            }) => {
                self.check_tag(&tag, t2.as_ref(), l2, "while/endwhile");
                sink.push(L2Stmt::While {
                    cond,
                    do_id: do_id_of(&tag),
                    body,
                    src,
                });
            }
            _ => self.err(ln, "unterminated `while` — no matching `endwhile`"),
        }
        OOutcome::HandledBlock
    }

    /// `o… do`: декоративный внутри while-тела; самостоятельный (post-test
    /// do-while) — не поддержан (решение 4).
    fn handle_do(&mut self, in_while: bool, ln: u32) -> OOutcome {
        if in_while {
            OOutcome::HandledLine
        } else {
            self.err(
                ln,
                "post-tested `o… do … endwhile` (do-while) is not representable \
                 in L2 v0.1 (While is pre-tested); corpus ×34 does not use it",
            );
            OOutcome::Failed
        }
    }

    /// `o… if [cond]` → цепочка IfThen (решение 1).
    fn handle_if(&mut self, sink: &mut Vec<L2Stmt>, tag: SubTarget, ln: u32) -> OOutcome {
        self.skip_ws();
        let cond = if self.peek() == Some(b'[') {
            self.parse_bracketed(ln)
        } else {
            self.err(ln, "`if` without `[cond]`");
            return OOutcome::Failed;
        };
        let stmts = self.parse_if_chain(Vec::new(), cond, tag, ln);
        sink.extend(stmts);
        OOutcome::HandledBlock
    }

    /// Рекурсивная десугаризация if/elseif/else/endif (решение 1).
    /// `negs` — конъюнкция отрицаний условий внешних веток.
    fn parse_if_chain(
        &mut self,
        mut negs: Vec<Expr>,
        head_cond: Expr,
        tag: SubTarget,
        open_ln: u32,
    ) -> Vec<L2Stmt> {
        let mut out = Vec::new();
        let (then_body, hit) = self.parse_block(false, &[Ctrl::Elseif, Ctrl::Else, Ctrl::Endif]);
        // then-ветка: AND(negs…, head_cond)
        let cond_then = and_all(negs.iter().cloned().chain(std::iter::once(head_cond.clone())));
        for s in then_body {
            let src = s
                .src()
                .cloned()
                .unwrap_or_else(crate::ast::placeholder_span);
            out.push(L2Stmt::IfThen {
                cond: cond_then.clone(),
                then_stmt: Box::new(s),
                src,
            });
        }
        let not_head = logical_not(&head_cond);
        match hit {
            Some(Hit {
                ctrl: Ctrl::Endif,
                tag: t2,
                line: l2,
                ..
            }) => {
                self.check_tag(&tag, t2.as_ref(), l2, "if/endif");
                out
            }
            Some(Hit {
                ctrl: Ctrl::Else,
                tag: t2,
                line: l2,
                ..
            }) => {
                self.check_tag(&tag, t2.as_ref(), l2, "if/else");
                let (else_body, hit2) = self.parse_block(false, &[Ctrl::Endif]);
                let cond_else =
                    and_all(negs.iter().cloned().chain(std::iter::once(not_head.clone())));
                for s in else_body {
                    let src = s
                        .src()
                        .cloned()
                        .unwrap_or_else(crate::ast::placeholder_span);
                    out.push(L2Stmt::IfThen {
                        cond: cond_else.clone(),
                        then_stmt: Box::new(s),
                        src,
                    });
                }
                match hit2 {
                    Some(Hit {
                        ctrl: Ctrl::Endif,
                        tag: t3,
                        line: l3,
                        ..
                    }) => {
                        self.check_tag(&tag, t3.as_ref(), l3, "if/endif");
                    }
                    _ => self.err(open_ln, "unterminated `if` — no matching `endif`"),
                }
                out
            }
            Some(Hit {
                ctrl: Ctrl::Elseif,
                tag: t2,
                line: l2,
                cond: Some(c2),
                ..
            }) => {
                self.check_tag(&tag, t2.as_ref(), l2, "if/elseif");
                negs.push(not_head);
                out.extend(self.parse_if_chain(negs, c2, tag, open_ln));
                out
            }
            Some(Hit {
                ctrl: Ctrl::Elseif,
                ..
            }) => out, // отсутствующий cond уже отловлен при создании Hit
            None => {
                self.err(open_ln, "unterminated `if` — no matching `endif`");
                out
            }
            _ => out,
        }
    }

    /// `o… call [a][b]… [L<expr>]` → CallSub (решение 9).
    fn handle_call(&mut self, sink: &mut Vec<L2Stmt>, tag: SubTarget, ln: u32) -> OOutcome {
        let src = self.line_span(ln);
        let mut args: Vec<(u8, Expr)> = Vec::new();
        let mut repeat = None;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'[') => {
                    if args.len() >= 26 {
                        self.err(
                            ln,
                            "more than 26 positional call arguments (letters A..Z exhausted)",
                        );
                        let _ = self.parse_bracketed(ln);
                        continue;
                    }
                    let e = self.parse_bracketed(ln);
                    let letter = b'A' + args.len() as u8;
                    args.push((letter, e));
                }
                Some(c) if c.eq_ignore_ascii_case(&b'l') => {
                    self.pos += 1;
                    self.skip_ws();
                    if let Some(e) = self.parse_word_value(ln) {
                        repeat = Some(e);
                    }
                }
                _ => break,
            }
        }
        sink.push(L2Stmt::CallSub {
            target: tag,
            repeat,
            args,
            modal: false,
            src,
        });
        OOutcome::HandledLine
    }

    /// `o… return [expr]` → `L2Stmt::Return` (v0.20.1) + носитель `[expr]`
    /// (решение 3, рулинг 6: retval-канал — 12.2, байты verbatim).
    fn handle_return(&mut self, sink: &mut Vec<L2Stmt>, ln: u32) -> OOutcome {
        if self.sub_depth == 0 {
            self.err(ln, "`return` outside of a sub body");
            return OOutcome::Failed;
        }
        self.skip_ws();
        if self.peek() == Some(b'[') {
            let start = self.pos;
            let _ = self.parse_bracketed(ln);
            let payload = self.src[start..self.pos].trim().to_string();
            sink.push(self.raw_carrier(ln, &payload, REASON_RETURN_RETVAL));
        }
        sink.push(L2Stmt::Return {
            src: self.line_span(ln),
        });
        OOutcome::HandledLine
    }

    // ----- классификация слов строки (решение 5/6) --------------------------

    fn classify_words(&mut self, words: Vec<Word>, ln: u32, sink: &mut Vec<L2Stmt>) {
        let mut kind_g: Option<u32> = None;
        let mut has_g53 = false;
        let mut has_g4 = false;
        let mut other_g = false;
        let mut parametric_g = false;
        let mut end_m: Option<u32> = None;
        let mut bad_m = false;
        let mut bad_word = false;
        let mut pose = ExprPose::empty();
        let mut ijk: [Option<Expr>; 3] = [None, None, None];
        let mut r_word: Option<Expr> = None;
        let mut feed: Option<Expr> = None;
        let mut spindle: Option<Expr> = None;
        let mut g4p: Option<Expr> = None;

        for w in words {
            match w.letter {
                b'g' => match &w.expr {
                    Expr::Const(n) => {
                        let v = n.value();
                        if v.fract() == 0.0 && (0.0..=255.0).contains(&v) {
                            match v as u32 {
                                0..=3 => kind_g = Some(v as u32),
                                4 => has_g4 = true,
                                53 => has_g53 = true,
                                _ => other_g = true,
                            }
                        } else {
                            // G38.2, G92.1, G43.1, … — фиксированный спец-код
                            other_g = true;
                        }
                    }
                    _ => parametric_g = true,
                },
                b'm' => match &w.expr {
                    Expr::Const(n) => {
                        let v = n.value();
                        if v.fract() == 0.0 && (0.0..=30.0).contains(&v) {
                            let v = v as u32;
                            if matches!(v, 0 | 1 | 2 | 30) {
                                end_m = Some(v);
                            } else {
                                bad_m = true;
                            }
                        } else {
                            bad_m = true;
                        }
                    }
                    _ => bad_m = true,
                },
                b'x' => pose.x = Some(w.expr),
                b'y' => pose.y = Some(w.expr),
                b'z' => pose.z = Some(w.expr),
                b'a' => pose.a = Some(w.expr),
                b'b' => pose.b = Some(w.expr),
                b'c' => pose.c = Some(w.expr),
                b'i' => ijk[0] = Some(w.expr),
                b'j' => ijk[1] = Some(w.expr),
                b'k' => ijk[2] = Some(w.expr),
                b'r' => r_word = Some(w.expr),
                b'f' => feed = Some(w.expr),
                b's' => spindle = Some(w.expr),
                b'p' => g4p = Some(w.expr),
                b'n' => {}
                _ => {
                    // T/U/V/W/Q/E/D/H/L/… — в Move v0.1 негде (решение 5)
                    bad_word = true;
                }
            }
        }

        // Строгий ранний выход: строка целиком — носитель (решение 5).
        let carrier_reason = if parametric_g {
            Some(REASON_PARAMETRIC_GM)
        } else if has_g53 {
            Some(REASON_G53)
        } else if other_g {
            Some(REASON_SPECIAL_G)
        } else if bad_m {
            Some(REASON_SPECIAL_M)
        } else if bad_word {
            Some(REASON_UNREPRESENTABLE_WORD)
        } else {
            None
        };
        if let Some(reason) = carrier_reason {
            let text = self.trimmed_line_text();
            sink.push(self.raw_carrier(ln, text, reason));
            return;
        }

        // G4 (Dwell): точный L1-оп только для константного P в чистом кадре.
        if has_g4 {
            let clean = pose_is_empty(&pose)
                && ijk.iter().all(|e| e.is_none())
                && r_word.is_none()
                && feed.is_none()
                && spindle.is_none()
                && kind_g.is_none();
            if clean {
                if let Some(Expr::Const(p)) = &g4p {
                    sink.push(L2Stmt::L1Op(Op::Dwell {
                        seconds: Num::new(p.text.clone(), p.value()),
                        state: default_state(),
                        src: self.line_span(ln),
                        sub: None,
                    }));
                    return;
                }
            }
            let text = self.trimmed_line_text();
            sink.push(self.raw_carrier(ln, text, REASON_SPECIAL_G));
            return;
        }

        // M0/M1/M2/M30 — точные L1-опы (в чистом кадре).
        if let Some(m) = end_m {
            let clean = pose_is_empty(&pose)
                && ijk.iter().all(|e| e.is_none())
                && r_word.is_none()
                && feed.is_none()
                && spindle.is_none()
                && kind_g.is_none();
            if clean {
                let src = self.line_span(ln);
                let stmt = match m {
                    2 | 30 => L2Stmt::L1Op(Op::ProgramEnd { src }),
                    _ => L2Stmt::L1Op(Op::ProgramStop {
                        optional: m == 1,
                        state: default_state(),
                        src,
                    }),
                };
                sink.push(stmt);
                return;
            }
            let text = self.trimmed_line_text();
            sink.push(self.raw_carrier(ln, text, REASON_SPECIAL_M));
            return;
        }

        // Ход: явный G0–G3 или слова осей/дуг с модальным kind (решение 6).
        let has_motion_content = kind_g.is_some()
            || !pose_is_empty(&pose)
            || ijk.iter().any(|e| e.is_some())
            || r_word.is_some();
        if has_motion_content {
            let effective = kind_g.or(self.modal_g).unwrap_or(0);
            let kind = match effective {
                1 => MoveKind::Linear,
                2 => MoveKind::Arc {
                    ijk,
                    r: r_word,
                    dir: Dir::Cw,
                },
                3 => MoveKind::Arc {
                    ijk,
                    r: r_word,
                    dir: Dir::Ccw,
                },
                _ => MoveKind::Rapid,
            };
            let feed = feed.or_else(|| self.modal_f.clone());
            let spindle = spindle.or_else(|| self.modal_s.clone());
            sink.push(L2Stmt::Move {
                kind,
                target: pose,
                feed,
                spindle,
                src: self.line_span(ln),
            });
            return;
        }

        // F/S-строка без ходов — модальная установка → носитель (модаль уже
        // обновлена в parse_word; решение 6).
        let text = self.trimmed_line_text();
        sink.push(self.raw_carrier(ln, text, REASON_MODAL_FS));
    }

    // ----- вспомогательные ---------------------------------------------------

    fn check_tag(&mut self, expected: &SubTarget, found: Option<&SubTarget>, ln: u32, what: &str) {
        match found {
            Some(t) if t == expected => {}
            Some(t) => self.err(
                ln,
                format!("{what}: tag mismatch — expected {expected:?}, found {t:?}"),
            ),
            None => self.err(ln, format!("{what}: missing O-tag")),
        }
    }

    /// Слово `kw` начинается в текущей позиции (регистронезависимо, с границей)?
    fn peek_word_is(&self, kw: &str) -> bool {
        let kb = kw.as_bytes();
        let mut i = self.pos;
        for &k in kb {
            match self.b.get(i) {
                Some(&c) if c.to_ascii_lowercase() == k => i += 1,
                _ => return false,
            }
        }
        match self.b.get(i) {
            Some(&c) => !c.is_ascii_alphanumeric() && c != b'_',
            None => true,
        }
    }
}

// ===========================================================================
// Свободные хелперы
// ===========================================================================

/// O-тег → SubTarget: целое → ONumber (канонизация `o<110>` — решение 10),
/// иначе Named (имя verbatim).
fn canonical_target(name: &str) -> SubTarget {
    match name.trim().parse::<u32>() {
        Ok(n) => SubTarget::ONumber(n),
        Err(_) => SubTarget::Named(name.to_string()),
    }
}

/// do_id для While: числовой O-тег из 1..=3 → тег (gridprobe O1/O2/O3),
/// иначе 1 (парность структурная; interp do_id не использует).
fn do_id_of(tag: &SubTarget) -> u32 {
    match tag {
        SubTarget::ONumber(n) if (1..=3).contains(n) => *n,
        _ => 1,
    }
}

/// Числовая константа с интегральным значением → u32.
fn const_int(e: &Expr) -> Option<u32> {
    if let Expr::Const(n) = e {
        let v = n.value();
        if v.fract() == 0.0 && (0.0..=u32::MAX as f64).contains(&v) {
            return Some(v as u32);
        }
    }
    None
}

/// Логическое НЕ (решение 1): `!c := AND(c GE 0, c LE 0)` — true ⇔ c
/// «falsy» (включая Null → 0.0); на результатах сравнений (0/1) — точное.
fn logical_not(c: &Expr) -> Expr {
    Expr::Binary(
        BinaryOp::And,
        Box::new(Expr::Binary(BinaryOp::Ge, Box::new(c.clone()), Box::new(Expr::num(0.0)))),
        Box::new(Expr::Binary(BinaryOp::Le, Box::new(c.clone()), Box::new(Expr::num(0.0)))),
    )
}

/// Лево-ассоциированная конъюнкция (одноэлементная цепочка → сам элемент).
fn and_all<I: IntoIterator<Item = Expr>>(it: I) -> Expr {
    let mut it = it.into_iter();
    let first = it.next().expect("and_all of non-empty chain");
    it.fold(first, |acc, e| {
        Expr::Binary(BinaryOp::And, Box::new(acc), Box::new(e))
    })
}

/// Одноаргументная функция со скобкой (скобка обязательна — LinuxCNC).
fn unary_arg(p: &mut Parser, ln: u32, op: UnaryOp) -> Expr {
    let inner = p.parse_bracketed(ln);
    Expr::Unary(op, Box::new(inner))
}

/// Числовой параметр → VarRef (решение 8).
fn number_var(idx: u32) -> VarRef {
    fn axis_of(off: u32) -> Option<Axis> {
        match off {
            0 => Some(Axis::X),
            1 => Some(Axis::Y),
            2 => Some(Axis::Z),
            3 => Some(Axis::A),
            4 => Some(Axis::B),
            5 => Some(Axis::C),
            _ => None,
        }
    }
    match idx {
        1..=33 | 100..=599 => VarRef::Direct(idx),
        5021..=5026 => VarRef::Sys(SysVar::AbsMachinePos(axis_of(idx - 5021).unwrap_or(Axis::X))),
        5041..=5046 => VarRef::Sys(SysVar::WorkPos(axis_of(idx - 5041).unwrap_or(Axis::X))),
        5061..=5066 => VarRef::Sys(SysVar::SkipPos(axis_of(idx - 5061).unwrap_or(Axis::X))),
        5420..=5426 => VarRef::Sys(SysVar::WorkPos(axis_of(idx - 5420).unwrap_or(Axis::X))),
        5221..=5229 | 5241..=5249 | 5261..=5269 | 5281..=5289 | 5301..=5309 | 5321..=5329 => {
            let base = 5221 + ((idx - 5221) / 20) * 20;
            let p = (1 + (base - 5221) / 20) as u8;
            match axis_of(idx - base) {
                Some(a) => VarRef::Sys(SysVar::WcsOffset { p, axis: a }),
                None => VarRef::Sys(SysVar::VendorSpecific(idx)),
            }
        }
        _ => VarRef::Sys(SysVar::VendorSpecific(idx)),
    }
}

/// Пустая поза (нет ни одного слова).
fn pose_is_empty(pose: &ExprPose) -> bool {
    pose.x.is_none()
        && pose.y.is_none()
        && pose.z.is_none()
        && pose.a.is_none()
        && pose.b.is_none()
        && pose.c.is_none()
}

/// Дефолтное состояние L1-опов (как в interp::l1_default_state — модель
/// станка в D0–D2 не ведётся; 12.2).
fn default_state() -> State {
    State {
        feed: None,
        spindle: eid_core::ast::Spindle::Off,
        coolant: eid_core::ast::Coolant::default(),
        tool: None,
        frame: eid_core::ast::Frame::Wcs,
        wcs: eid_core::ast::Wcs::G54,
        feed_restated: false,
    }
}

// Причины носителей (диагностика/стоп-отчёт).
const REASON_POLAR: &str =
    "linuxcnc polar coordinate line (@radius^angle / ^[expr]) — verbatim carrier (censor ruling 5)";
const REASON_BLOCK_DELETE: &str = "linuxcnc block-delete line (/…) — verbatim carrier";
const REASON_ENDSUB_RETVAL: &str =
    "linuxcnc `endsub [expr]` return-value — verbatim carrier; retval channel deferred to 12.2 (censor ruling 6)";
const REASON_RETURN_RETVAL: &str =
    "linuxcnc `return [expr]` return-value — verbatim carrier; retval channel deferred to 12.2 (censor ruling 6, ast v0.20.1 Return carries control flow)";
const REASON_PARAMETRIC_GM: &str =
    "parametric G/M word (g#<expr>/m#<expr>) — verbatim carrier (motion kind not statically known)";
const REASON_G53: &str =
    "G53 machine-frame motion — verbatim carrier (MoveKind has no frame flag; deferred to 12.2)";
const REASON_SPECIAL_G: &str =
    "non-motion G-code line (modal/cycle/probe/dwell-var) — verbatim carrier";
const REASON_SPECIAL_M: &str = "non-canonical M-code line — verbatim carrier";
const REASON_UNREPRESENTABLE_WORD: &str =
    "line with word letters unrepresentable in ExprPose/L2Stmt v0.1 (T/U/V/W/P/Q/E/…) — verbatim carrier";
const REASON_MODAL_FS: &str =
    "modal F/S line without motion — verbatim carrier (feed/spindle attached textually per solution 6)";

/// Семейство исполняемых paren-remarks (рулинг 4 + семейство LinuxCNC):
/// debug/print/msg/AXIS,notify. Остальные `(...)` — комментарии.
fn remark_family(text: &str) -> Option<&'static str> {
    let t = text.trim();
    let mut parts = t.splitn(2, ',');
    let first = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    let rest = parts.next().unwrap_or("");
    match first.as_str() {
        "debug" => Some(
            "linuxcnc executable remark (debug,…) — verbatim carrier; parameter interpolation deferred to 12.2 (censor ruling 4)",
        ),
        "print" => Some(
            "linuxcnc executable remark (print,…) — verbatim carrier; parameter interpolation deferred to 12.2 (censor ruling 4)",
        ),
        "msg" => Some("linuxcnc executable remark (msg,…) — verbatim carrier"),
        "axis" if rest.trim().to_ascii_lowercase().starts_with("notify") => {
            Some("linuxcnc executable remark (AXIS,notify,…) — verbatim carrier")
        }
        _ => None,
    }
}

// ===========================================================================
// Тесты: golden-пары из целевых файлов ценза + корпус ×34 статистика
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::parse_program;
    use crate::ast::{BinaryOp, Expr, L2Program, L2Stmt, MoveKind, SubTarget, VarRef};
    use crate::eval::L2Error;
    use crate::interp::{L2Config, L2Evaluator};
    use eid_core::ast::Op;

    // ----- утилиты тестов ----------------------------------------------------

    /// (moves, assigns, ifthen, whiles, calls, carriers(L1Op), subs)
    fn counts(p: &L2Program) -> (usize, usize, usize, usize, usize, usize, usize) {
        let mut c = (0usize, 0, 0, 0, 0, 0, p.subs.len());
        fn walk(stmts: &[L2Stmt], c: &mut (usize, usize, usize, usize, usize, usize, usize)) {
            for s in stmts {
                match s {
                    L2Stmt::Move { .. } => c.0 += 1,
                    L2Stmt::Assign { .. } => c.1 += 1,
                    L2Stmt::IfThen { then_stmt, .. } => {
                        c.2 += 1;
                        walk(std::slice::from_ref(then_stmt), c);
                    }
                    L2Stmt::While { body, .. } => {
                        c.3 += 1;
                        walk(body, c);
                    }
                    L2Stmt::CallSub { .. } => c.4 += 1,
                    L2Stmt::L1Op(_) => c.5 += 1,
                    L2Stmt::WithTransform { body, .. } => walk(body, c),
                    _ => {}
                }
            }
        }
        walk(&p.stmts, &mut c);
        for s in &p.subs {
            walk(&s.body, &mut c);
        }
        c
    }

    fn expect_ok(src: &str) -> L2Program {
        match parse_program(src) {
            Ok(p) => p,
            Err(es) => panic!("parse failed: {es:?}"),
        }
    }

    fn dump(p: &L2Program) -> String {
        format!("{p:?}")
    }

    // ----- 1. factorial: рекурсия, именованный sub, return-носители ----------

    const FACTORIAL: &str = "\
; recursive function example
; factorial
o<factorial> sub
o<factorialif> if [[#1] EQ 0]
o<factorial> \t  return [1]
o<factorialif> else
o<factorial>      call [[#1] - 1]
o<factorial>      return  [#<_value> * #1]
o<factorialif> endif
o<factorial> endsub
m2
";

    #[test]
    fn t01_factorial_recursion_named_sub_structure() {
        let p = expect_ok(FACTORIAL);
        assert_eq!(p.subs.len(), 1);
        let sub = &p.subs[0];
        assert_eq!(sub.target, SubTarget::Named("factorial".into()));
        // десугаризация if/else: then [ret-носитель, Return] + else
        // [CallSub, ret-носитель, Return] = 5 IfThen
        assert_eq!(sub.body.len(), 5, "if/else desugar → 5 IfThen");
        // then-ветка: return-носитель + Return-узел
        assert!(matches!(&sub.body[0], L2Stmt::IfThen { then_stmt, .. }
            if matches!(**then_stmt, L2Stmt::L1Op(_))));
        assert!(matches!(&sub.body[1], L2Stmt::IfThen { then_stmt, .. }
            if matches!(**then_stmt, L2Stmt::Return { .. })));
        // else-ветка оператор 1: РЕКУРСИВНЫЙ CallSub c 1 аргументом
        let call_ok = matches!(&sub.body[2], L2Stmt::IfThen { cond, then_stmt, .. }
            if matches!(&**then_stmt, L2Stmt::CallSub { target, args, .. }
                if target == &SubTarget::Named("factorial".into()) && args.len() == 1)
            && matches!(cond, Expr::Binary(BinaryOp::And, _, _)));
        assert!(call_ok, "recursive CallSub must be inside negated IfThen");
        // рекурсивный аргумент [[#1] - 1]
        if let L2Stmt::IfThen { then_stmt, .. } = &sub.body[2] {
            if let L2Stmt::CallSub { args, .. } = &**then_stmt {
                assert!(matches!(
                    &args[0].1,
                    Expr::Binary(BinaryOp::Sub, lhs, _)
                        if matches!(&**lhs, Expr::Var(VarRef::Direct(1)))
                ));
            }
        }
        // main: m2 → ProgramEnd; носители = 2 retval + 1 ProgramEnd
        assert_eq!(p.stmts.len(), 1);
        assert!(matches!(&p.stmts[0], L2Stmt::L1Op(Op::ProgramEnd { .. })));
        let (.., carriers, subs) = counts(&p);
        assert_eq!((carriers, subs), (3, 1));
    }

    // ----- 2. factorial: eval — рекурсия CallSub завершается ------------------

    #[test]
    fn t02_factorial_eval_recursion_terminates() {
        let p = expect_ok(FACTORIAL);
        let mut prog = p.clone();
        prog.stmts.insert(
            0,
            L2Stmt::CallSub {
                target: SubTarget::Named("factorial".into()),
                repeat: None,
                args: vec![(b'A', Expr::num(5.0))],
                modal: false,
                src: crate::ast::placeholder_span(),
            },
        );
        let mut e = L2Evaluator::new(L2Config::default());
        e.run(&prog).expect("recursion must terminate without error");
        assert_eq!(e.env().frame_depth(), 1, "frames must unwind");
    }

    // ----- 3. flowsnake: числовой sub, if/else, рекурсивные вызовы, debug ----

    #[test]
    fn t03_flowsnake_structure() {
        let src = "\
%
(Program to mill a flowsnake)
o1000 sub
#<level> = #1
#<startX> = #2
  o1001 if [#<level> EQ 0]
    g1 f10 x#<endX> y#<endY>
  o1001 else
    #<p1X> = [[#<startX> * 2 + #<endX>]/3]
    #<p2X> = [[#<startX> + #<endX>]/2 + [#<endY> - #<startY>]/[SQRT[12.0]]]
    o1000 call [#<level>-1] [#<startX>] [#<startY>] [#<p1X>] [#<p1Y>]
    o1000 call [#<level>-1] [#<p1X>] [#<p1Y>] [#<p2X>] [#<p2Y>]
  o1001 endif
o1000 endsub
S1M3
g0 z1
#<level> = 5
#<_foobar> = 729
(debug, param 2:#2)
(debug, level:#<level>)
(debug, _foobar:#<_foo bar>)
o1000 call [#<level>] [.25] [1.0] [3.75] [1.0]
g0 z1
M5
%
";
        let p = expect_ok(src);
        assert_eq!(p.subs.len(), 1);
        assert_eq!(p.subs[0].target, SubTarget::ONumber(1000));
        let (_, _, ifthen, _, calls, carriers, _) = counts(&p);
        // вызовы: 2 в else-ветке + 1 в main
        assert_eq!(calls, 3);
        // IfThen: 1 (then g1) + 4 (else: 2 assign + 2 call)
        assert_eq!(ifthen, 5);
        // носители: S1M3 + 3 debug + M5
        assert_eq!(carriers, 5);
        // деление на [SQRT[12.0]] (else-assign p2X — второй IfThen-assign)
        let d = dump(&p);
        assert!(d.contains("Div") && d.contains("Sqrt"));
        // 5-арг позиционный вызов → буквы A..E
        assert!(d.contains("\"A\"") || d.contains("\'A\'") || d.contains("65"));
    }

    // ----- 4. gosper: ATAN[y]/[x], именованные с двоеточием, s#<rpm> ---------

    #[test]
    fn t04_gosper_atan2_named_with_colon() {
        let src = "\
o<gosper> sub
#<rpm>      =  #1
o<i0> if [#<scale> EQ 0]
        #<scale> = 1.0
o<i0> endif
        #<theta> = ATAN [#<dely>]/[#<delx>]  (degrees)
        #<p3x>   = [#<x1> + #<len> * COS[#<theta> + 60]]
        s#<rpm> M3 (spindle on)
o<spin> call [#<rpm>]
        g0 z #<zsafe>
#<x1>      = #<_move:x>
o<gosper> endsub
";
        let p = expect_ok(src);
        let d = dump(&p);
        // ATAN[y]/[x] → BinaryOp::Atan2
        assert!(d.contains("Atan2"), "ATAN[y]/[x] must become BinaryOp::Atan2");
        // именованная с двоеточием #<_move:x>
        assert!(d.contains("move:x"));
        // s#<rpm> M3 → носитель (параметрический M)
        let (.., carriers, _) = counts(&p);
        assert_eq!(carriers, 1);
        // g0 z #<zsafe> — Move с Named-переменной в Z
        assert!(p.subs[0]
            .body
            .iter()
            .any(|s| matches!(s, L2Stmt::Move { target, .. }
                if matches!(&target.z, Some(Expr::Var(VarRef::Named(n))) if n == "zsafe"))));
    }

    // ----- 5. arc2: ATAN внутри выражения + call с 20 позиционными аргументами

    #[test]
    fn t05_arc2_atan_in_expr_and_20_args() {
        let src = "\
o<arc2> sub
 #<arc_r> = [SQRT[#<dely>*#<dely> + #<delx>*#<delx>]]
#<rotate> = [-90 + ATAN[#<dely>]/[#<delx>]]
o<if0> if [#<arc_r> LT #<min_separation>]
         (print, arc2: separation too small: #<arc_r> - EXITING)
         m2
o<if0> endif
o<arc1>call[#<toolno>][#<dir>][#<inside>][#<rpm>][#<feedrate>][#<zincr>][#<cutdepth>][#<zsafe>][#<zstart>][#<width>][#<angle>][#<arc_r>][#<rotate>][#<xoff>][#<yoff>][#<scale>][#<spin_notify>][#<use_g43>][#<h_for_g43>][#<verbose>]
o<arc2> endsub
";
        let p = expect_ok(src);
        // 20 позиционных аргументов → буквы A..T (1..=20)
        let mut args = None;
        for s in &p.subs[0].body {
            if let L2Stmt::CallSub { args: a, target, .. } = s {
                assert_eq!(target, &SubTarget::Named("arc1".into()));
                args = Some(a.clone());
            }
        }
        let args = args.expect("call must exist");
        assert_eq!(args.len(), 20);
        assert_eq!(args[0].0, b'A');
        assert_eq!(args[19].0, b'T');
        // [-90 + ATAN[..]/[..]] — Atan2 как операнд суммы; -90 — verbatim
        let d = dump(&p);
        assert!(d.contains("Atan2"));
        assert!(d.contains("-90"));
        // (print, …) + m2 → носители
        let (.., carriers, _) = counts(&p);
        assert_eq!(carriers, 2);
    }

    // ----- 6. arc1: AND-условия, арки в while, g#<dir> — носитель ------------

    #[test]
    fn t06_arc1_and_conditions_and_arcs() {
        let src = "\
o<arc1> sub
o<i1> if [[#<dir> NE 2] AND [#<dir> NE 3]]
        m2
o<i1> endif
        #<x0> = [#<slot_r> + #<arc_r> * [1 - sin[#<phi>]]]
o<wh1>  while [#<zcurrent> GT [0 - #<cutdepth>]]
          g#<dir1> x#<x1> y#<y1> i#<vx0> j#<vy0> z #<zcurrent>
          #<zcurrent>=[#<zcurrent>-#<zincr>]
o<wh1>  endwhile
        g3 x3.05 z-3.05 i0 k-3.05
o<arc1> endsub
";
        let p = expect_ok(src);
        let d = dump(&p);
        // AND-условие
        assert!(matches!(&p.subs[0].body[0], L2Stmt::IfThen { cond, .. }
            if matches!(cond, Expr::Binary(BinaryOp::And, _, _))));
        // while: ход с параметрическим g# → носитель
        assert!(d.contains("parametric G/M word"));
        // модальная дуга g3 i/k — Ccw, I+K без J
        assert!(p.subs[0]
            .body
            .iter()
            .any(|s| matches!(s, L2Stmt::Move {
                kind: MoveKind::Arc { ijk, dir: eid_core::ast::Dir::Ccw, r },
                ..
            } if ijk[0].is_some() && ijk[2].is_some() && ijk[1].is_none() && r.is_none())));
        // lowercase sin[...]
        assert!(d.contains("Sin"));
    }

    // ----- 7. db25: bare-числа, tan ------------------------------------------

    #[test]
    fn t07_db25_bare_assign_tan() {
        let src = "\
o<db25>  sub
 #<xlong> = 1.635
 #<xlong> = 1.675 ; adjust
     #<y> = 0.432
#<xshort> = [#<xlong> - 2 * #<y> * tan[10]]
o<db25>  endsub
";
        let p = expect_ok(src);
        let (_, assigns, ..) = counts(&p);
        assert_eq!(assigns, 4);
        assert!(dump(&p).contains("Tan"));
    }

    // ----- 8. backlash: if/else внутри while ---------------------------------

    #[test]
    fn t08_backlash_if_else_inside_while() {
        let src = "\
o<backlash> sub
o<l10> if [#<p1> GT #<p2>]
         #<vmin> = #<p2>
o<l10> else
         #<vmin> = #<p1>
o<l10> endif
       #<ok> = 0
o<whl> while [#<count> GT 0]
o<xxx>   if [#<axis> EQ 0]
           x #<vmin>
o<xxx>   endif
         #<count> = [#<count> - 1]
o<whl> endwhile
o<backlash> endsub
";
        let p = expect_ok(src);
        let w = p
            .subs
            .clone()
            .into_iter()
            .flat_map(|s| s.body)
            .find(|s| matches!(s, L2Stmt::While { .. }))
            .expect("while must exist");
        if let L2Stmt::While { body, do_id, .. } = &w {
            assert_eq!(body.len(), 2);
            assert_eq!(*do_id, 1); // named tag → do_id 1
            assert!(matches!(&body[0], L2Stmt::IfThen { then_stmt, .. }
                if matches!(&**then_stmt, L2Stmt::Move { .. })));
        }
    }

    // ----- 9. g881: EXISTS ×N, endsub [1] ------------------------------------

    #[test]
    fn t09_g881_exists_and_endsub_retval() {
        let src = "\
o<g881> sub
(debug, in g881); x=[#1] y=[#2] z=[#3] r=[#4])
o<g881_havex> if [EXISTS[#<x>]]
(debug, X param set: #<x>)
o<g881_havex> endif
o<g881_haver> if [EXISTS[#<r>]]
(debug, R param set: #<r>)
o<g881_haver> endif
o<g881> endsub [1]
m2
";
        let p = expect_ok(src);
        let d = dump(&p);
        assert_eq!(d.matches("Exists").count(), 2, "EXISTS[..] → UnaryOp::Exists");
        let (.., carriers, _) = counts(&p);
        // debug×3 + endsub[1]-retval + m2 = 5
        assert_eq!(carriers, 5);
        // retval-носитель последним в теле sub
        assert!(matches!(p.subs[0].body.last(), Some(L2Stmt::L1Op(_))));
    }

    // ----- 10. gridprobe: вложенные while, fix[], else ------------------------

    #[test]
    fn t10_gridprobe_nested_while_fix() {
        let src = "\
G20   (Inches)
F4    (probe speed)
#1=0  (X start)
#9=0 #10=0
G0Z#7
O1 while [#9 lt #6]
    #10=0
    G0 Y[#4+#5*#9]
    O2 while [#10 lt #3]
        O3 if [[#9/2] - fix[#9/2] eq 0]
            G0X[#1+#2*#10]
        O3 else
            G0X[#1+#2*[#3-#10-1]]
        O3 endif
        G38.2Z#8
        #10=[#10+1]
    O2 endwhile
    #9=[#9+1]
O1 endwhile
(PROBECLOSE)
G0Z#7
M2
";
        let p = expect_ok(src);
        let (_, _, _, whiles, _, carriers, _) = counts(&p);
        assert_eq!(whiles, 2);
        // do_id 1 (верхний) и 2 (вложенный) из числовых тегов
        let ids: Vec<u32> = p
            .stmts
            .iter()
            .filter_map(|s| match s {
                L2Stmt::While { do_id, .. } => Some(*do_id),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec![1]);
        // носители: G20 + F4 + G38.2 (M2 — ProgramEnd, тоже L1Op → 4)
        assert_eq!(carriers, 4);
        // ходы: G0Z + G0Y + G0X(then) + G0X(else) + G0Z = 5
        let (moves, _, _, _, _, _, _) = counts(&p);
        assert_eq!(moves, 5);
        // вложенный if/else внутри внутреннего while
        fn find_while(stmts: &[L2Stmt], id: u32) -> Option<&Vec<L2Stmt>> {
            for s in stmts {
                match s {
                    L2Stmt::While { do_id, body, .. } if *do_id == id => return Some(body),
                    L2Stmt::While { body, .. } => {
                        if let Some(f) = find_while(body, id) {
                            return Some(f);
                        }
                    }
                    L2Stmt::IfThen { then_stmt, .. } => {
                        if let Some(f) = find_while(std::slice::from_ref(then_stmt), id) {
                            return Some(f);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        let inner = find_while(&p.stmts, 2).expect("nested while do_id=2");
        assert!(matches!(&inner[0], L2Stmt::IfThen { .. }));
        assert!(matches!(&inner[1], L2Stmt::IfThen { .. })); // else-ветка G0X
        // G38.2 — носитель; fix[] распознан
        assert!(matches!(&inner[2], L2Stmt::L1Op(_)));
        assert!(dump(&p).contains("Fix"));
    }

    // ----- 11. remap change: return [-1] — носитель ---------------------------

    #[test]
    fn t11_remap_change_return_carrier() {
        let src = "\
o<change> sub
(debug, change tool_in_spindle=#<tool_in_spindle> pocket=#<pocket>)
	o<testabort> if [#<pocket> EQ 3]
		     o<testabort> return [-1]
	o<testabort> endif
o<change> endsub [1]
m2
";
        let p = expect_ok(src);
        // body[0] — debug-носитель ДО if; if-тело: return-носитель +
        // Return-узел в IfThen-обёртках (body[1]/body[2])
        assert!(matches!(&p.subs[0].body[0], L2Stmt::L1Op(_)));
        assert!(matches!(&p.subs[0].body[1], L2Stmt::IfThen { then_stmt, .. }
            if matches!(**then_stmt, L2Stmt::L1Op(_))));
        assert!(matches!(&p.subs[0].body[2], L2Stmt::IfThen { then_stmt, .. }
            if matches!(**then_stmt, L2Stmt::Return { .. })));
        let (.., carriers, _) = counts(&p);
        // debug + retval[-1] + endsub-retval + m2 = 4
        assert_eq!(carriers, 4);
    }

    // ----- 12. remap on_abort: elseif-цепочка → конъюнкция отрицаний ----------

    #[test]
    fn t12_remap_on_abort_elseif_chain() {
        let src = "\
o<on_abort> sub
o100 if [#1 eq 5]
    #<a> = 1
o100 elseif [#1 eq 6]
    #<a> = 2
o100 elseif [#1 eq 7]
    #<a> = 3
o100 else
    #<a> = 4
o100 endif
o<on_abort> endsub
m2
";
        let p = expect_ok(src);
        let body = &p.subs[0].body;
        assert_eq!(body.len(), 4, "if + 2 elseif + else → 4 IfThen");
        let conds: Vec<Expr> = body
            .iter()
            .map(|s| match s {
                L2Stmt::IfThen { cond, .. } => cond.clone(),
                _ => unreachable!(),
            })
            .collect();
        // ветка 1 — исходное условие без обёртки
        assert!(matches!(&conds[0], Expr::Binary(BinaryOp::Eq, _, _)));
        // ветки 2..4 — деревья And
        assert!(matches!(&conds[1], Expr::Binary(BinaryOp::And, _, _)));
        assert!(matches!(&conds[3], Expr::Binary(BinaryOp::And, _, _)));
        // глубина конъюнкций растёт: NOT — сам And (GE/LE), цепочка добавляет
        fn and_depth(e: &Expr) -> usize {
            match e {
                Expr::Binary(BinaryOp::And, a, b) => 1 + and_depth(a).max(and_depth(b)),
                Expr::Unary(_, i) => and_depth(i),
                Expr::Binary(_, a, b) => and_depth(a).max(and_depth(b)),
                _ => 0,
            }
        }
        assert_eq!(and_depth(&conds[0]), 0);
        assert_eq!(and_depth(&conds[1]), 2); // AND(NOT(c1), c2), NOT = AND(GE,LE)
        assert_eq!(and_depth(&conds[2]), 3); // AND(n1, AND(n2-обёртка…))
        assert_eq!(and_depth(&conds[3]), 3);
        // отрицание — AND(GE, LE)
        let d = dump(&p);
        assert!(d.contains("Ge") && d.contains("Le"));
    }

    // ----- 13. gmoccapy: #<_ini[..]>/#<_hal[..]>, G53/M6/G38.2 — носители -----

    #[test]
    fn t13_gmoccapy_ini_hal_named_and_carriers() {
        let src = "\
o<change> sub
#<tool> = #<selected_tool>
O100 if [#<_task> EQ 0]
        (debug, Task ist Null)
O100     return [999]
O100 endif
G53 G0 Z[#<_ini[CHANGE_POSITION]Z>]
M6
O200 if [#<_hal[gmoccapy.toolmeasurement]> EQ 0]
O200 return [3]
O200 endif
F #<_hal[gmoccapy.searchvel]>
G91
G38.2 Z #<_ini[TOOLSENSOR]MAXPROBE>
#<touch_result> = #5063
G10 L1 P#<tool> Z[#<touch_result> - #<_hal[gmoccapy.blockheight]>]
o<change> endsub [1]
";
        let p = expect_ok(src);
        let d = dump(&p);
        // именованные с [..] внутри имени: hal-чтение живёт в cond IfThen;
        // _ini-имена — внутри ЦЕЛОСТНЫХ носителей строк (G53/G38.2) — только
        // hex-пейлоад, открытого текста в AST нет (решение 5)
        assert!(d.contains("_hal[gmoccapy.toolmeasurement]"));
        // носители: debug, return[999], G53-строка, M6, return[3], F-строка,
        // G91, G38.2, G10, endsub[1]-retval = 10
        let (.., carriers, _) = counts(&p);
        assert_eq!(carriers, 10);
        // #5063 → Sys(SkipPos(Z))
        assert!(d.contains("SkipPos"));
        assert!(d.contains("selected_tool"));
    }

    // ----- 14. m61: именованный sub, return в середине тела -------------------

    #[test]
    fn t14_m61_return_mid_body() {
        let src = "\
O<m61demo> sub
(DEBUG,m61demo.ngc:  M61 Q=#1)
M66 P2 L0
O<m61demo_test_fail> if [#5399 EQ 1]
    O<m61demo> return [-1]
O<m61demo_test_fail> endif
O<m61demo> endsub [#1]
m2
";
        let p = expect_ok(src);
        let (.., carriers, _) = counts(&p);
        // DEBUG + M66 + return + endsub[#1]-retval + m2 = 5
        assert_eq!(carriers, 5);
        // #5399 → Sys(VendorSpecific(5399))
        assert!(dump(&p).contains("VendorSpecific(5399)"));
    }

    // ----- 15. m70/m72: именованные вызовы, M70/M72 — носители ----------------

    #[test]
    fn t15_m70_m72_named_calls() {
        let src = "\
O<showstate> sub
(DEBUG, imperial=#<_imperial> absolute=#<_absolute>)
O<showstate> endsub
O<imperialsub> sub
g20 (imperial)
o<showstate> call
O<imperialsub> endsub
g21 (metric)
o<showstate> call
M70 (save caller state in at global level)
O<imperialsub> call
M72 (explicitly restore state)
o<showstate> call
m2
";
        let p = expect_ok(src);
        assert_eq!(p.subs.len(), 2);
        let (_, _, _, _, calls, carriers, _) = counts(&p);
        assert_eq!(calls, 4); // 1 в imperialsub + 3 в main
        // g20, DEBUG, g21, M70, M72, m2 = 6
        assert_eq!(carriers, 6);
    }

    // ----- 16. daisy: числовые subs, 4 присваивания в строке, F[expr], G4P2 ---

    #[test]
    fn t16_daisy_multiassign_feed_expr_dwell() {
        let src = "\
O100 sub ([f0] [f1] [f2] [f3])
#61=#1
O110 if [#2 gt 0]
#62=#2
O110 else
#62=#1
O110 endif
#41=0 #42=0 #43=0 #44=0
G0  X0Y0Z0A0
G93
O100 endsub
O200 sub ([t] [p1] [p2])
#2=[#2*#1/#61]
G1 X#41 Y#42 Z#43 A#44 F[60/#1]
O200 endsub
#100 = [440*1.781797]  (Key of G)
O100 call [6400] [6400] [8000] [80]
G4P2
O200 call [1]     [#107]     [#100/2.0] [#104/2.0]
M2
";
        let p = expect_ok(src);
        assert_eq!(p.subs.len(), 2);
        assert_eq!(p.subs[0].target, SubTarget::ONumber(100));
        let (_, assigns, ifthen, _, calls, carriers, _) = counts(&p);
        assert_eq!(assigns, 9); // #61 + #62(if) + #62(else) + #41..#44 + #2 + #100
        assert_eq!(ifthen, 2);
        assert_eq!(calls, 2);
        // G93 + Dwell + M2 = 3 (L1Op-категория в counts)
        assert_eq!(carriers, 3);
        // G4P2 → Op::Dwell
        assert!(p
            .stmts
            .iter()
            .any(|s| matches!(s, L2Stmt::L1Op(Op::Dwell { .. }))));
        // A-ось в позе (Rapid X0Y0Z0A0)
        assert!(p.subs[0]
            .body
            .iter()
            .any(|s| matches!(s, L2Stmt::Move { target, .. } if target.a.is_some())));
        // F[60/#1] — feed-выражение деления
        assert!(dump(&p).contains("Div"));
    }

    // ----- 17. cone: ротационные B/C в позе, W — носитель ----------------------

    #[test]
    fn t17_cone_rotary_and_w_carrier() {
        let src = "\
o200 sub
g0 b0 c0
#100=-180
o100 while [ #100 le 180 ]
    g1 x[100*cos[#100]] y[100*sin[#100]] c#100
    #100=[#100+10]
o100 endwhile
o200 endsub
g1 w100 f2000 (drill)
o200 call [50]
m2
";
        let p = expect_ok(src);
        let (_, _, _, _, _, carriers, _) = counts(&p);
        // g1 w100… (W-слово) + m2 = 2
        assert_eq!(carriers, 2);
        // g0 b0 c0 — Move с B и C
        assert!(p.subs[0]
            .body
            .iter()
            .any(|s| matches!(s, L2Stmt::Move { target, .. }
                if target.b.is_some() && target.c.is_some())));
        // c#100 → Var(Direct(100)) в pose.c (ход внутри While.body)
        fn find_move_c(stmts: &[L2Stmt]) -> bool {
            stmts.iter().any(|s| match s {
                L2Stmt::Move { target, .. } => {
                    matches!(&target.c, Some(Expr::Var(VarRef::Direct(100))))
                }
                L2Stmt::While { body, .. } => find_move_c(body),
                _ => false,
            })
        }
        assert!(find_move_c(&p.subs[0].body));
        assert!(dump(&p).contains("Cos"));
        // #100=-180 — verbatim отрицательная константа
        assert!(dump(&p).contains("-180"));
    }

    // ----- 18. increment: строка «g91 g0 x#1 y#2» — носитель (решение 5) -------

    #[test]
    fn t18_increment_g91_move_line_carrier() {
        let src = "\
O<increment> sub
g91 g0 x#1 y#2
g90
O<increment> endsub
";
        let p = expect_ok(src);
        let (moves, _, _, _, _, carriers, _) = counts(&p);
        // g91+g0+оси → целостный носитель; g90 — носитель
        assert_eq!((moves, carriers), (0, 2));
    }

    // ----- 19. g33: модальные bare-axis ходы, G33 — носители, sqrt ------------

    #[test]
    fn t19_g33_modal_bare_axis_and_g33_carriers() {
        let src = "\
#1=.05
o101 while [#1 GE .04]
g0z.7
g1x.020
g1z[#1 + .65]
z[#1 + .54]x.13
#1=[#1-.008]
o101 endwhile
g33z[.1 + #7]k#4
g33x[.1 + #1 - #6]z#7k[sqrt[2]*[#4]]
m2
";
        let p = expect_ok(src);
        let (moves, _, _, _, _, carriers, _) = counts(&p);
        // ходы: g0z.7, g1x.020, g1z[..], z[..]x.13 (модальная G1) = 4
        assert_eq!(moves, 4);
        // G33 ×2 + m2 = 3
        assert_eq!(carriers, 3);
        // последняя ходовая строка (z[..]x.13) — модальная Linear
        let mut last_kind = None;
        fn walk<'a>(stmts: &'a [L2Stmt], out: &mut Option<&'a MoveKind>) {
            for s in stmts {
                match s {
                    L2Stmt::Move { kind, .. } => *out = Some(kind),
                    L2Stmt::While { body, .. } => walk(body, out),
                    _ => {}
                }
            }
        }
        walk(&p.stmts, &mut last_kind);
        for s in &p.subs {
            walk(&s.body, &mut last_kind);
        }
        assert!(matches!(last_kind, Some(MoveKind::Linear)));
        // sqrt живёт в hex-носителе строки G33 (решение 5) — в AST его нет;
        // `[#1-.008]` — минус бинарный (Sub), verbatim-отрицательные
        // константы покрыты в t05 ("-90") и t17 ("-180")
    }

    // ----- 20. g76: арки R и I/K, call без аргументов, G92 — носитель ----------

    #[test]
    fn t20_g76_arcs_call_no_args() {
        let src = "\
o100 sub
g2 x0 z0 r1
g3 x3.05 z-3.05 i0 k-3.05
g91
g2 z-1.5 x1 r1.5
o100 endsub
o100 call
g92z0
m2
";
        let p = expect_ok(src);
        let (moves, _, _, _, calls, carriers, _) = counts(&p);
        assert_eq!(moves, 3);
        assert_eq!(calls, 1);
        // g91 + g92z0 + m2 = 3
        assert_eq!(carriers, 3);
        // call без скобок — 0 аргументов
        let call = p
            .stmts
            .iter()
            .find_map(|s| match s {
                L2Stmt::CallSub { target, args, .. } => {
                    assert_eq!(target, &SubTarget::ONumber(100));
                    Some(args.len())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(call, 0);
    }

    // ----- 21. g76base: параметрические G#/M#, ABS, «lT», вложенный if -------

    #[test]
    fn t21_g76base_parametric_gm_carriers() {
        let src = "\
o<g76base> sub
o<if00> if [#<p> LT 0]
          #<p> = ABS[1 / #<p>]
o<if00> endif
o<if20> if [#<_vmajor> lT 2.6]
o<if21>   if [#<mode> NE 8]
            m2
o<if21>   endif
o<if20> else
o<if22>   if [#<_lathe_diameter_mode> EQ 1]
            #<restore_mode> = 7
o<if22>   endif
o<if20> endif
G#<mode>
M#<spindir> S#<rpm>
G76 P#<p> Z#<z> I#<i> J#<j> R#<r> K#<k> Q#<q> H#<h> E#<e> L#<l>
o<loadtool> call [#<toolno>]
o<g76base> endsub
";
        let p = expect_ok(src);
        let (_, _, ifthen, _, calls, carriers, _) = counts(&p);
        assert_eq!(ifthen, 5); // walk считает вложенные IfThen (if21/if22 внутри веток if20)
        assert_eq!(calls, 1);
        // G#<mode>, M#<spindir>…, G76… + m2-ProgramEnd внутри if21 = 4
        assert_eq!(carriers, 4);
        let d = dump(&p);
        assert!(d.contains("Abs"));
        assert!(d.contains("restore_mode"));
        // «lT» — регистронезависимый LT
        assert!(d.contains("Lt"));
    }

    // ----- 22. g76diam: вложенный if/else в else-ветке -------------------------

    #[test]
    fn t22_g76diam_nested_if_in_else() {
        let src = "\
o<g76diam> sub
o<if7> if [#<mode> EQ 8]
         #<k>   = [ABS[#<majordiameter> - #<minordiameter>]/2]
o<if7> else
o<if9>   if [#<i> LE 0]
            #<xi> = [#<majordiameter> - #<i>]
o<if9>   else
            #<xi> = [#<minordiameter> - #<i>]
o<if9>   endif
o<if7> endif
o<g76base> call [#<toolno>]
o<g76diam> endsub
";
        let p = expect_ok(src);
        let body = &p.subs[0].body;
        // ветка 1: k-assign; ветки else: 2 IfThen (вложенный if/else)
        assert_eq!(body.len(), 4); // 3 IfThen + CallSub
        assert!(matches!(&body[1], L2Stmt::IfThen { then_stmt, .. }
            if matches!(&**then_stmt, L2Stmt::IfThen { .. })));
    }

    // ----- 23. probe edge angle: #[expr]-косвенность, ATAN2, MSG ---------------

    #[test]
    fn t23_probe_indirect_atan2_msg() {
        let src = "\
o<probe_back_edge_angle> sub
  #<workspace_y> = #[5202 + [20 * #5220]]
  o<110> if [#5400 NE #<probe_tool_number>]
  (MSG, Specified probe tool #<probe_tool_number> not in spindle, aborting)
   o<probe_back_edge_angle> return
  o<110> endif
  #<edge_angle> = [ATAN [#<edge_delta>] / [#<edge_width>]]
  G10 L2 P#5220 X[#<x_start_position>] Y[#<y_minus_zero_edge_start> + #<workspace_y>] R[#<edge_angle>]
o<probe_back_edge_angle> endsub
M2 (end program)
";
        let p = expect_ok(src);
        let d = dump(&p);
        // косвенная адресация #[expr] → Indirect
        assert!(d.contains("Indirect"));
        // ATAN2 с пробелами вокруг '/'
        assert!(d.contains("Atan2"));
        // bare return → Return-узел (без носителя)
        assert!(d.contains("Return"));
        // носители: MSG + G10 + M2 = 3
        let (.., carriers, _) = counts(&p);
        assert_eq!(carriers, 3);
    }

    // ----- 24. probe left inside: #5420 → Sys(WorkPos(X)) ----------------------

    #[test]
    fn t24_probe_sysvar_workpos_mapping() {
        let src = "\
o<probe_back_left_inside> sub
  #<x_start> = #5420
  #<y_start> = #5421
  #<w> = #[5201 + [20 * #5220]]
o<probe_back_left_inside> endsub
";
        let p = expect_ok(src);
        let d = dump(&p);
        assert!(d.contains("WorkPos"));
        assert!(d.contains("Indirect"));
    }

    // ----- 25. 3D_Chips: N-слова, масштабные выражения, модальная G1/F ---------

    #[test]
    fn t25_3dchips_n_words_modal_continuity() {
        let src = "\
#<xscale> = 1.0
#<zscale> = 1.0
#<fscale> = 10000.0
N30 G21
N40G90
N50T#<toolno>M6
N70S#<rpm>M3
N90G0Z[#<zscale>*10.]
N100G1Z[#<zscale>*-25.372]F[#<fscale>*100]
N120Y[#<yscale>*-56.12]Z[#<zscale>*-27.725]
M2
";
        let p = expect_ok(src);
        let (moves, _, _, _, _, carriers, _) = counts(&p);
        assert_eq!(moves, 3); // G0Z + G1Z + N120 (модальная G1)
        assert_eq!(carriers, 5); // G21, G90, T#var+M6, S#var+M3, M2
        // N120 без G — модальная Linear; feed от N100 модальный
        let last_move = p
            .stmts
            .iter()
            .filter_map(|s| match s {
                L2Stmt::Move { kind, feed, .. } => Some((kind, feed)),
                _ => None,
            })
            .next_back()
            .unwrap();
        assert!(matches!(last_move.0, MoveKind::Linear));
        assert!(last_move.1.is_some(), "modal F from N100 must attach");
    }

    // ----- 26. polar: все строки-носители --------------------------------------

    #[test]
    fn t26_polar_carriers() {
        let src = "\
G20 G90
G0 X2.5 Y0
G81 R.1 Z-0.5 F60
^120
^240
G90 @1 ^33
G91 G81 @1 R.1 Z-0.5 L5
G90 G0 X2 Y0
M2
";
        let p = expect_ok(src);
        let (moves, _, _, _, _, carriers, _) = counts(&p);
        // `G90 G0 X2 Y0` — G90 в кадре → носитель строки целиком (решение 5)
        assert_eq!(moves, 1); // только `G0 X2.5 Y0`
        // G20G90, G81, ^120, ^240, G90@1^33, G91G81@…, G90G0X2Y0, M2 = 8
        assert_eq!(carriers, 8);
        assert!(p
            .stmts
            .iter()
            .any(|s| matches!(s, L2Stmt::L1Op(Op::ProgramEnd { .. }))));
    }

    // ----- 27. good-arc imperial: выражение в I-слове дуги ----------------------

    #[test]
    fn t27_good_arc_i_expr() {
        let src = "\
g20
f1
g0x0y0z0
g2 x600 i[300 + [100 * [0.00099 * sqrt[2]]]]
m2
";
        let p = expect_ok(src);
        let (moves, _, _, _, _, carriers, _) = counts(&p);
        assert_eq!(moves, 2);
        assert_eq!(carriers, 3); // g20 + f1 + m2
        let arc = p
            .stmts
            .iter()
            .find_map(|s| match s {
                L2Stmt::Move {
                    kind: kind @ MoveKind::Arc { .. },
                    ..
                } => Some(kind),
                _ => None,
            })
            .unwrap();
        if let MoveKind::Arc { ijk, dir, r } = arc {
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
            assert!(r.is_none());
            assert!(ijk[0].is_some());
            // вложенные скобки с sqrt[2]
            assert!(format!("{:?}", ijk[0]).contains("Sqrt"));
        } else {
            panic!("expected Arc move");
        }
    }

    // ----- 28. eval: if/else-десугаризация исполняет правильные ветки ----------

    #[test]
    fn t28_eval_if_else_branches() {
        let src = "\
#<_m> = 0
o9 if [1 GT 2]
  #<_m> = 1
o9 else
  #<_m> = 2
o9 endif
#<_n> = 0
o10 if [2 GT 1]
  #<_n> = 3
o10 else
  #<_n> = 4
o10 endif
";
        let p = expect_ok(src);
        let mut e = L2Evaluator::new(L2Config::default());
        e.run(&p).unwrap();
        assert_eq!(e.env().read_named("_m"), crate::ast::Val::Num(2.0));
        assert_eq!(e.env().read_named("_n"), crate::ast::Val::Num(3.0));
    }

    // ----- 29. eval: EXISTS / ATAN2 численно ------------------------------------

    #[test]
    fn t29_eval_exists_atan2() {
        let src = "\
#<_acc> = 0
#<_i> = 3
o1 while [#<_i> GT 0]
  #<_acc> = #<_acc> + #<_i>
  #<_i> = [#<_i> - 1]
o1 endwhile
#<_ex> = EXISTS[#<_acc>]
#<_nex> = EXISTS[#<_missing>]
#<_a> = ATAN[1]/[1]
";
        let p = expect_ok(src);
        let mut e = L2Evaluator::new(L2Config::default());
        e.run(&p).unwrap();
        assert_eq!(e.env().read_named("_acc"), crate::ast::Val::Num(6.0));
        assert_eq!(e.env().read_named("_ex"), crate::ast::Val::Num(1.0));
        assert_eq!(e.env().read_named("_nex"), crate::ast::Val::Num(0.0));
        let a = match e.env().read_named("_a") {
            crate::ast::Val::Num(v) => v,
            _ => panic!(),
        };
        assert!((a - 45.0).abs() < 1e-9, "atan2(1,1) = 45deg, got {a}");
    }

    // ----- 30. ошибки: незакрытая скобка/тег/break/do-while/stray endif ---------

    #[test]
    fn t30_error_cases() {
        // незакрытая скобка выражения
        let errs = parse_program("#1 = [2 + 3\nm2\n").unwrap_err();
        assert!(errs.iter().any(|e| e.message().contains(']')));
        // несовпадение тегов if/endif
        let errs = parse_program("o10 if [#1]\n#2=1\no11 endif\n").unwrap_err();
        assert!(errs.iter().any(|e| e.message().contains("mismatch")));
        // break — не представим (решение 2)
        let errs = parse_program("o1 while [#1]\no1 break\no1 endwhile\n").unwrap_err();
        assert!(errs.iter().any(|e| e.message().contains("Break/Continue")));
        // do-while — не представим (решение 4)
        let errs = parse_program("o1 do\n#2=1\no1 endwhile\n").unwrap_err();
        assert!(errs.iter().any(|e| e.message().contains("do-while")));
        // stray endif
        let errs = parse_program("o1 endif\n").unwrap_err();
        assert!(errs.iter().any(|e| e.message().contains("without matching")));
        // незакрытый while
        let errs = parse_program("o1 while [#1 GT 0]\n#2=1\n").unwrap_err();
        assert!(errs
            .iter()
            .any(|e| e.message().contains("unterminated `while`")));
        // вложенный sub
        let errs = parse_program("o1 sub\no2 sub\no2 endsub\no1 endsub\n").unwrap_err();
        assert!(errs.iter().any(|e| e.message().contains("nested")));
        // >26 аргументов
        let many: String = (0..27).map(|_| "[1]").collect();
        let errs = parse_program(&format!("o1 sub\no1 endsub\no1 call {many}\n")).unwrap_err();
        assert!(errs.iter().any(|e| e.message().contains("26 positional")));
        // все ошибки — класс E_RUNTIME_STATE (решение 11)
        assert!(errs.iter().all(|e| matches!(e, L2Error::RuntimeError(_))));
    }

    // ----- 31. реальные спаны строк ---------------------------------------------

    #[test]
    fn t31_spans_are_real_lines() {
        let text = "#1 = 5\n#2 = 6\nm2\n";
        let p = expect_ok(text);
        if let L2Stmt::Assign { src, .. } = &p.stmts[1] {
            assert_eq!(src.line_start, 2);
            assert_eq!(src.line_end, 2);
            assert_eq!(src.col_start, 1);
        } else {
            panic!("expected assign");
        }
        assert!(matches!(&p.stmts[2], L2Stmt::L1Op(Op::ProgramEnd { src }) if src.line_start == 3));
    }

    // ----- 32. корпус ×34: парс всех целевых файлов + статистика ----------------

    #[test]
    fn t32_corpus_linuxcnc_34_parse_stats() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/tier_b/local");
        if !root.exists() {
            // корпус отсутствует (тонкий checkout) — тест пропускается
            return;
        }
        let files: &[&str] = &[
            "lcnc_configs_sim_gmoccapy_macros_change.ngc",
            "lcnc_configs_sim_gmoccapy_macros_change_g43.ngc",
            "lcnc_configs_sim_gmoccapy_macros_go_to_position.ngc",
            "lcnc_g33_threading.ngc",
            "lcnc_m61_output_demo.ngc",
            "lcnc_m70_m72_demo.ngc",
            "lcnc_m73_demo.ngc",
            "lcnc_nc_files_3D_Chips.ngc",
            "lcnc_nc_files_cone.ngc",
            "lcnc_nc_files_daisy.ngc",
            "lcnc_nc_files_factorial.ngc",
            "lcnc_nc_files_flowsnake.ngc",
            "lcnc_nc_files_g20sub.ngc",
            "lcnc_nc_files_g76.ngc",
            "lcnc_nc_files_g881.ngc",
            "lcnc_nc_files_g881min.ngc",
            "lcnc_nc_files_g882.ngc",
            "lcnc_nc_files_gridprobe.ngc",
            "lcnc_nc_files_increment.ngc",
            "lcnc_nc_files_ngcgui_lib_arc1.ngc",
            "lcnc_nc_files_ngcgui_lib_arc2.ngc",
            "lcnc_nc_files_ngcgui_lib_backlash.ngc",
            "lcnc_nc_files_ngcgui_lib_db25.ngc",
            "lcnc_nc_files_ngcgui_lib_gosper.ngc",
            "lcnc_nc_files_ngcgui_lib_lathe_g76base.ngc",
            "lcnc_nc_files_ngcgui_lib_lathe_g76diam.ngc",
            "lcnc_nc_files_probe_basic_probe_macros_probe_back_edge_angle.ngc",
            "lcnc_nc_files_probe_basic_probe_macros_probe_back_left_inside.ngc",
            "lcnc_nc_files_probe_basic_probe_macros_probe_back_left_outside.ngc",
            "lcnc_nc_files_remap-subroutines_change.ngc",
            "lcnc_nc_files_remap-subroutines_on_abort.ngc",
            "lcnc_polar_at_angle.ngc",
            "lcnc_tests_interp_good_good-arc.big.imperial.center-format.ngc",
            "lcnc_tests_interp_good_good-arc.big.metric.center-format.ngc",
        ];
        let mut ok = 0usize;
        let mut failures: Vec<(String, Vec<L2Error>)> = Vec::new();
        println!(
            "{:<62} {:>5} {:>5} {:>6} {:>5} {:>5} {:>5} {:>5} {:>6}",
            "file", "mv", "asg", "ifthn", "whil", "call", "carr", "subs", "stmts"
        );
        for f in files {
            let path = root.join(f);
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => panic!("corpus file {} unreadable: {e}", path.display()),
            };
            match parse_program(&text) {
                Ok(p) => {
                    ok += 1;
                    let (mv, asg, ifthn, whil, call, carr, subs) = counts(&p);
                    let total = p.stmts.len() + p.subs.iter().map(|s| s.body.len()).sum::<usize>();
                    println!(
                        "{f:<62} {mv:>5} {asg:>5} {ifthn:>6} {whil:>5} {call:>5} {carr:>5} {subs:>5} {total:>6}"
                    );
                }
                Err(es) => {
                    println!("{:<62} FAIL: {} error(s)", f, es.len());
                    failures.push(((*f).to_string(), es));
                }
            }
        }
        println!("TOTAL: {ok}/34 parse-ok");
        assert!(
            failures.is_empty(),
            "parse failures: {:#?}",
            failures
                .iter()
                .map(|(f, es)| (
                    f,
                    es.iter()
                        .take(3)
                        .map(|e| e.message())
                        .collect::<Vec<_>>()
                ))
                .collect::<Vec<_>>()
        );
        assert_eq!(ok, 34);
    }

    // ----- 33. g20sub: M70/M71-носители, ход с константами ----------------------

    #[test]
    fn t33_g20sub_m_carriers() {
        let src = "\
O<g20sub> sub
M70 (save in current call context)
g20 (imperial)
f4  (much faster feed)
G0 X1.0 Y1.0 (move absolute to 1in/1in)
M71 (restore to caller context)
O<g20sub> endsub
m2
";
        let p = expect_ok(src);
        let (moves, _, _, _, _, carriers, _) = counts(&p);
        assert_eq!(moves, 1);
        // M70 + g20 + f4 + M71 + m2 = 5
        assert_eq!(carriers, 5);
        assert!(p
            .stmts
            .iter()
            .any(|s| matches!(s, L2Stmt::L1Op(Op::ProgramEnd { .. }))));
    }

    // ----- 34. eval: ход с локальными параметрами (pose-разрешение) --------------

    #[test]
    fn t34_eval_move_words_with_locals() {
        // X#1 с локальным #1=10 — поза параметрическая → Parametric-носитель
        let src = "\
#1 = 10
g0 x#1 y0
";
        let p = expect_ok(src);
        let mut e = L2Evaluator::new(L2Config::default());
        e.run(&p).unwrap();
        // способ lowering параметрических ходов — зона агента A/12.2; тест
        // проверяет исполнение и окружение, не привязан к Raw/L1-маркерам
        assert_eq!(e.env().read_direct(1), Ok(crate::ast::Val::Num(10.0)));
    }
}
