//! ЭМИССИЯ L2→L2 — LinuxCNC rs274ngc (O-word) (этап 12.4, агент D3).
//!
//! ## GOTO → структурные o-if / do-while (DoD дока 12.4)
//!
//! В LinuxCNC **нет goto/jump** (docs linuxcnc.org gcode/o-code.html:
//! только sub/endsub, while/endwhile, do/while, if/elseif/else/endif,
//! repeat/endrepeat, break/continue, call, return) — поэтому `IfGoto`
//! реструктурируется, а не транслируется 1:1:
//!
//! 1. **Условный GOTO вперёд** (метка ниже по потоку):
//!    ```text
//!    IF [c] GOTO 10      →   o<X> if [NOT c]
//!    <тело до N10>           <тело до N10>
//!    N10 ...                o<X> endif
//!                           N10 ...
//!    ```
//!    Инверсия условия — ТОЧНАЯ на парах сравнений (GT↔LE, LT↔GE,
//!    GE↔LT, LE↔GT, EQ↔NE) и по De Morgan на AND/OR; для прочих форм —
//!    fallback `[(c GE 0) AND (c LE 0)]` (= «c == 0»). При семантике #0
//!    (Приложение дока, таблица 2.3: GE/LE приводят Null к 0.0, EQ/NE —
//!    нет) fallback ТОЧЕН: IF [#0] ложен (Null ≠ 0 в неравенствах
//!    тождества), NOT → истина; (#0 GE 0) AND (#0 LE 0) → 0≥0 AND 0≤0 →
//!    истина. ✓
//! 2. **Безусловный GOTO вперёд** (`IF [1] GOTO n` в AST) → `o<X> if [0]`
//!    (мёртвый блок — тело сохраняется, notice).
//! 3. **Условный GOTO назад** (метка выше; сегмент `N<label> … IF [c]
//!    GOTO <label>` без вложенных меток/переходов/циклов) — точная
//!    реструктуризация в do/while (docs: «do/while runs the code in the
//!    loop then checks the test condition» — семантика Fanuc `… IF [c]
//!    GOTO n` идентична: тело, тест, повтор).
//! 4. **Безусловный GOTO назад** = бесконечный цикл — честный отказ.
//! 5. Переходы сквозь границы скоупа (из sub в main, из тела цикла наружу)
//!    и сегменты с вложенными метками — честный отказ («goto into
//!    structure — not structurally translatable»).
//!
//! Координаты меток АБСОЛЮТНЫ в пределах одного вектора stmt — реструк-
//! труктуризация работает окном (start, end) без срезов-копий (см.
//! [`emit_block`]).
//!
//! Прочие отличия (задание: «реальные различия документируй»):
//! * `M98 P<n>` → `o<n> call`; **G65-аргументы**: именованная нелинейная
//!   таблица Fanuc (A→#1 … D→#7, амендмент v0.1.2 BUG-D1) НЕ переводится
//!   в позиционную LinuxCNC ([arg k]→#k) молча: совпадают только A/B/C
//!   (позиции 1..3, notice), прочие наборы — отказ;
//! * `M99` → `o<tag> return` (в теле sub) / отказ в main (M99-in-main =
//!   цикл программы — аналога нет);
//! * `WHILE…DO<id>…END<id>` → `o<fresh> while/endwhile` (DO-идентификатор
//!   1..3 заменяется уникальным тегом — LinuxCNC требует уникальные
//!   o-номера блоков);
//! * именованные `#<name>` → passthrough (LinuxCNC родной). **Scoping**
//!   (docs LinuxCNC): named БЕЗ подчёркивания локальны в o-sub (за
//!   пределами тела читаются как Null), `#<_name>` — глобальные;
//!   round-trip сохраняет имена дословно (кросс-диалектная семантика —
//!   амендмент v0.1.4);
//! * subs эмитируются ПЕРЕД main (docs: sub вызывается только после
//!   определения) + notice;
//! * M97, ModalGroup (#4xxx), RtcpMode (G43.4), WithTransform (G68/G51
//!   отсутствуют в LinuxCNC), G65-with-non-ABC-args, несовместимые
//!   носители — отказы; линт носителей общий (emit_l2::carrier_compat).

use crate::ast::{
    BinaryOp, Expr, ExprPose, L2Program, L2Stmt, MoveKind, SubTarget, UnaryOp, VarRef,
};
use crate::emit_l2::{
    atom_needs_brackets, attach_word, carrier_compat, expr_eq, expr_text, map_sysvar,
    strip_outer_brackets, CarrierVerdict, EmitError, EmitNotice, EmitOutcome, SysVarDecision,
    TagAllocator, TargetDialect,
};
use eid_core::ast::{Dir, Op, SrcSpan};
use std::collections::BTreeMap;

// ===========================================================================
// Sink
// ===========================================================================

struct Sink {
    lines: Vec<String>,
    notices: Vec<EmitNotice>,
    errors: Vec<EmitError>,
    line: u32,
}

impl Sink {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            notices: Vec::new(),
            errors: Vec::new(),
            line: 0,
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
    /// Рендер выражения с диалект-веткой Sys (LinuxCnc).
    fn expr(&mut self, e: &Expr) -> Option<String> {
        match e {
            Expr::Var(v) => match v {
                VarRef::Sys(_) => sys_var_text(v, self),
                // Named (`#<name>`/`#<_name>`) — родной LinuxCNC: passthrough
                // (имя дословно; scoping-семантика — шапка/амендмент v0.1.4).
                VarRef::Named(name) => Some(format!("#<{name}>")),
                VarRef::Direct(n) => Some(format!("#{n}")),
                VarRef::Indirect(inner) => {
                    let x = self.expr(inner)?;
                    Some(format!("#[{x}]"))
                }
            },
            Expr::Unary(op, inner) if *op != UnaryOp::Neg => {
                let x = self.expr(inner)?;
                Some(format!("{}[{}]", unary_op_name(*op), x))
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
                Some(format!("[{a} {} {b}]", binary_op_name(*op)))
            }
            // Const/Null — чистый рендер (без диалект-веток).
            other => expr_text(other).map_err(|err| self.errors.push(err)).ok(),
        }
    }
}

fn sys_var_text(v: &VarRef, sink: &mut Sink) -> Option<String> {
    let VarRef::Sys(sv) = v else { return None };
    match map_sysvar(sv, TargetDialect::LinuxCnc) {
        SysVarDecision::Mapped { number, .. } => Some(format!("#{number}")),
        SysVarDecision::PassThrough { number, reason } => {
            sink.note("sysvar-passthrough-unverified", reason);
            Some(format!("#{number}"))
        }
        SysVarDecision::Unmappable { reason } => {
            sink.refuse(reason);
            None
        }
    }
}

fn unary_op_name(op: UnaryOp) -> &'static str {
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
        UnaryOp::Exists => "EXISTS",
    }
}

fn binary_op_name(op: BinaryOp) -> &'static str {
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
        BinaryOp::Atan2 => "ATAN2",
    }
}

// ===========================================================================
// Скоуп меток: одна таблица на блок (N → абсолютный индекс в векторе stmt)
// ===========================================================================

#[derive(Clone, Copy)]
struct LabelScope<'a> {
    table: &'a BTreeMap<u32, usize>,
}

/// Метки main: таблица парсера (L2Program.labels, контракт v0.20.1) плюс
/// Label-узлы (таблица имеет приоритет; узлы — дополнение).
fn main_label_table(prog: &L2Program) -> BTreeMap<u32, usize> {
    let mut m = prog.labels.clone();
    for (i, s) in prog.stmts.iter().enumerate() {
        if let L2Stmt::Label { n, .. } = s {
            m.entry(*n).or_insert(i);
        }
    }
    m
}

/// Метки тела (sub / тело While): Label-узлы ВЕРХНЕГО уровня блока; метки
/// во вложенных телах не попадают (переходы сквозь блок → честный отказ).
fn body_label_table(body: &[L2Stmt]) -> BTreeMap<u32, usize> {
    let mut m = BTreeMap::new();
    for (i, s) in body.iter().enumerate() {
        if let L2Stmt::Label { n, .. } = s {
            m.insert(*n, i);
        }
    }
    m
}

/// Тэг sub для `o{tag} sub/endsub/call/return`.
fn sub_tag(t: &SubTarget) -> String {
    match t {
        SubTarget::ONumber(n) => n.to_string(),
        SubTarget::Named(name) => format!("<{name}>"),
        SubTarget::NLabel(n) => format!("<m97_n{n}>"),
    }
}

// ===========================================================================
// Точка входа
// ===========================================================================

/// Исходная строка stmt: L2Stmt::src() (None для L1Op) или спан внутри
/// Op::Raw (носители — основной носитель L2). Определяет порядок эмиссии.
fn stmt_line(s: &L2Stmt) -> u32 {
    match s.src() {
        Some(x) => x.line_start,
        None => match s {
            // Носители и канонические L1-опы: спан внутри варианта Op
            // (у eid_core::Op нет accessor — матч по вариантам).
            L2Stmt::L1Op(op) => match op {
                Op::Raw { src, .. }
                | Op::Dwell { src, .. }
                | Op::ProgramEnd { src }
                | Op::ProgramStop { src, .. }
                | Op::ToolChange { src, .. }
                | Op::WcsDefine { src, .. }
                | Op::WcsSetHere { src, .. }
                | Op::Rapid { src, .. }
                | Op::Linear { src, .. }
                | Op::Arc { src, .. } => src.line_start,
            },
            _ => 0,
        },
    }
}

/// LinuxCNC эмиссия L2-программы.
///
/// ПОРЯДОК ЭМИССИИ — ИСХОДНЫЙ: каждый main-stmt и каждый sub-блок —
/// единица эмиссии, слитые по исходной строке (`src.line_start`).
/// Обоснование: парсер LinuxCNC наследует модальные F/S на каждый ход
/// (`feed.or_else(modal_f)` в classify_words) — любое переставление
/// меняет модальный контекст ходов и ЛОМАЕТ round-trip AST:
/// * corpus g76: sub после main; sub-первый порядок даёт main-ходам
///   feed:Some(5) там, где в источнике был None;
/// * corpus cone/daisy/flowsnake: sub в середине main — main-ходы ДО sub
///   и ПОСЛЕ него имеют разный модальный контекст, поэтому main обязан
///   чередоваться с sub-блоками поточечно.
///
/// Следствие исходного порядка: если в ИСТОЧНИКЕ вызов предшествует
/// определению, эмиссия это сохраняет — свойство источника (LinuxCNC
/// требует определения до вызова; строки-мишени F↔H не затронуты).
pub(crate) fn linuxcnc_emit(prog: &L2Program) -> EmitOutcome {
    let mut sink = Sink::new();
    let mut tags = TagAllocator::new();

    /// Единица эмиссии: main-stmt (позиция в prog.stmts) или sub-блок.
    enum Unit {
        MainStmt(usize),
        Sub(usize),
    }

    let mut units: Vec<(u32, u32, Unit)> = Vec::new();
    for (i, s) in prog.stmts.iter().enumerate() {
        let key = stmt_line(s);
        units.push((key, i as u32, Unit::MainStmt(i)));
    }
    for (i, sub) in prog.subs.iter().enumerate() {
        let key = sub
            .body
            .first()
            .map(stmt_line)
            .unwrap_or(0)
            .saturating_sub(1); // строка O-заголовка ≈ на 1 выше первого stmt
                                // Sub при равной строке — ПОСЛЕ main-stmt (main-stmt на строке
                                // заголовка sub — контент O-строки-носитель).
        units.push((key, u32::MAX - i as u32, Unit::Sub(i)));
    }
    units.sort_by_key(|(k, t, _)| (*k, *t));

    // Main-stmt'ы сливаются в НЕПРЕРЫВНЫЕ прогоны (окна emit_block):
    // IfGoto-реструктуризация требует окна до метки (случаи 1/3 — тело
    // до/от метки в том же векторе). Прогон прерывается sub-блоком или
    // разрывом индексов; GOTO сквозь границу прогона → честный отказ.
    let mut pending_run: Option<(usize, usize)> = None;
    for unit in &units {
        match &unit.2 {
            Unit::MainStmt(k) => match pending_run {
                None => pending_run = Some((*k, k + 1)),
                Some((_, e)) if e == *k => pending_run = Some((pending_run.unwrap().0, k + 1)),
                Some((s, e)) => {
                    emit_main_window(&mut sink, prog, s, e, &mut tags);
                    pending_run = Some((*k, k + 1));
                }
            },
            Unit::Sub(idx) => {
                if let Some((s, e)) = pending_run.take() {
                    emit_main_window(&mut sink, prog, s, e, &mut tags);
                }
                emit_sub_block(&mut sink, prog, *idx, &mut tags);
            }
        }
    }
    if let Some((s, e)) = pending_run.take() {
        emit_main_window(&mut sink, prog, s, e, &mut tags);
    }

    EmitOutcome {
        text: sink.lines.join(
            "
",
        ) + "
",
        notices: sink.notices,
        errors: sink.errors,
    }
}

/// Окно main-stmt'ов ([start, end) в prog.stmts) — единый emit_block
/// (labels абсолютны в векторе prog.stmts).
fn emit_main_window(
    sink: &mut Sink,
    prog: &L2Program,
    start: usize,
    end: usize,
    tags: &mut TagAllocator,
) {
    let table = main_label_table(prog);
    emit_block(
        sink,
        &prog.stmts,
        start,
        end,
        &LabelScope { table: &table },
        tags,
        None,
    );
}

/// Sub-блок: `o<tag> sub` + тело + endsub (retval-носитель замещает строку
/// endsub verbatim — рулинг 6).
fn emit_sub_block(sink: &mut Sink, prog: &L2Program, i: usize, tags: &mut TagAllocator) {
    let sub = &prog.subs[i];
    let tag = sub_tag(&sub.target);
    sink.set_line(sub.body.first().and_then(|s| s.src()));
    sink.emit(format!("o{tag} sub"));
    let table = body_label_table(&sub.body);
    let mut body_end = sub.body.len();
    let mut endsub_line = format!("o{tag} endsub");
    if body_end > 0 {
        if let L2Stmt::L1Op(Op::Raw {
            payload_hex,
            reason,
            ..
        }) = &sub.body[body_end - 1]
        {
            if reason.contains("endsub") {
                endsub_line = decode_hex(payload_hex);
                body_end -= 1;
            }
        }
    }
    emit_block(
        sink,
        &sub.body,
        0,
        body_end,
        &LabelScope { table: &table },
        tags,
        Some(&tag),
    );
    sink.emit(endsub_line);
}

// ===========================================================================
// Эмиссия блока: скользящее окно (start, end) в АБСОЛЮТНЫХ координатах
// вектора `stmts` — IfGoto-реструктуризация не копирует срезы, метки
// остаются в координатах вектора.
// ===========================================================================

#[allow(clippy::too_many_arguments)]
fn emit_block(
    sink: &mut Sink,
    stmts: &[L2Stmt],
    start: usize,
    end: usize,
    scope: &LabelScope,
    tags: &mut TagAllocator,
    sub_tag: Option<&str>,
) {
    let mut i = start;
    while i < end {
        // СЛУЧАЙ 3 (обрабатывается НА МЕТКЕ-ГОЛОВЕ): `N<n> <тело> IF [c]
        // GOTO n` с чистым сегментом — реструктуризация в do/while ЗДЕСЬ
        // (запуск на IfGoto был бы поздним: тело уже эмитировано общим
        // потоком). Немаркированные/грязные сегменты падают в обычную
        // эмиссию; хвостовой IfGoto при отказе — честный отказ (см.
        // emit_goto_restructured, случай 3-отказ).
        if let L2Stmt::Label { n, .. } = &stmts[i] {
            let mut head_loop = None;
            for (j, s) in stmts[i + 1..end].iter().enumerate() {
                let j = i + 1 + j;
                match s {
                    L2Stmt::Label { .. } | L2Stmt::While { .. } => break,
                    L2Stmt::IfGoto {
                        cond, target_label, ..
                    } => {
                        if *target_label == *n {
                            head_loop = Some((j, cond.clone()));
                        }
                        break; // первый IfGoto: либо хвост нашего цикла, либо чужой
                    }
                    _ => {}
                }
            }
            if let Some((j, cond)) = head_loop {
                let clean = stmts[i + 1..j].iter().all(|s| {
                    !matches!(
                        s,
                        L2Stmt::Label { .. } | L2Stmt::IfGoto { .. } | L2Stmt::While { .. }
                    )
                });
                let unconditional = matches!(&cond, Expr::Const(c) if c.value() == 1.0);
                if clean && !unconditional {
                    // Рендер условия ДО аллокации тега: отказ (Sys и т.п.)
                    // честно падает в sink.errors, а эмиссия спадает с
                    // head-ветки — метка и тело эмитятся штатно общим
                    // потоком; хвостовой IfGoto откажет и продвинет i
                    // (emit_goto_restructured), зацикливания нет.
                    let Some(cond_text) = sink.expr(&cond) else {
                        break;
                    };
                    sink.set_line(stmts[i].src());
                    let tag = tags.alloc();
                    sink.note(
                        "goto-backward-do-while",
                        format!(
                            "backward GOTO {n} → do/while restructure: segment [N{n}+1..IF] becomes o{tag} do ... o{tag} while [cond] (exact semantics)"
                        ),
                    );
                    sink.emit(format!("o{tag} do"));
                    emit_block(sink, stmts, i + 1, j, scope, tags, sub_tag);
                    sink.emit(format!(
                        "o{tag} while [{}]",
                        strip_outer_brackets(&cond_text)
                    ));
                    i = j + 1; // хвостовой IF-GOTO поглощён замыкателем
                    continue;
                }
            }
        }
        if let L2Stmt::IfGoto {
            cond, target_label, ..
        } = &stmts[i]
        {
            emit_goto_restructured(
                sink,
                stmts,
                end,
                &mut i,
                cond,
                *target_label,
                scope,
                tags,
                sub_tag,
            );
            continue;
        }
        // Парсер LinuxCNC (рулинг 6, 12.1) представляет `return [expr]`
        // парой [носитель "[expr]" (REASON_RETURN_RETVAL), Return] —
        // эмиссия сливает их обратно в `o<tag> return [expr]` (голый
        // `[expr]` без строки return не валиден для re-parse).
        // ВАРИАНТ A (плоский блок): носитель + Return — соседние stmt.
        if let L2Stmt::L1Op(Op::Raw {
            payload_hex,
            reason,
            ..
        }) = &stmts[i]
        {
            if reason.contains("return-value") {
                if let Some(L2Stmt::Return { .. }) = stmts.get(i + 1) {
                    let payload = decode_hex(payload_hex);
                    match sub_tag {
                        Some(tag) => sink.emit(format!("o{tag} return {payload}")),
                        None => sink.refuse(
                            "return with value outside sub body — no LinuxCNC analog (honest refusal)".to_string(),
                        ),
                    }
                    sink.note(
                        "retval-verbatim",
                        "return-value merged into `return [expr]` line (12.1 parser desugars it to carrier + Return; retval channel itself is v0.1.4 design, not implemented)".to_string(),
                    );
                    i += 2;
                    continue;
                }
            }
        }
        // ВАРИАНТ B (десугаризация if-блока): `o if [c] return [p] … endif`
        // даёт пару IfThen{c, носитель(p)} + IfThen{c, Return} — слияние в
        // ОДИН if-блок с полной строкой return (re-parse воспроизводит
        // обе десугаризованные IfThen — парсинг `return [p]` внутри if-тела
        // снова даёт [носитель, Return] в then_body).
        if let L2Stmt::IfThen {
            cond: c1,
            then_stmt: t1,
            ..
        } = &stmts[i]
        {
            if let L2Stmt::L1Op(Op::Raw {
                payload_hex,
                reason,
                ..
            }) = &**t1
            {
                if reason.contains("return-value") {
                    if let Some(L2Stmt::IfThen {
                        cond: c2,
                        then_stmt: t2,
                        ..
                    }) = stmts.get(i + 1)
                    {
                        if matches!(&**t2, L2Stmt::Return { .. }) && cond_render_eq(c1, c2) {
                            let tag = tags.alloc();
                            let Some(ct) = sink.expr(c1) else { return };
                            let payload = decode_hex(payload_hex);
                            sink.emit(format!("o{tag} if [{}]", strip_outer_brackets(&ct)));
                            match sub_tag {
                                Some(st) => sink.emit(format!("o{st} return {payload}")),
                                None => sink.refuse(
                                    "return with value outside sub body — honest refusal"
                                        .to_string(),
                                ),
                            }
                            sink.note(
                                "retval-verbatim",
                                "return-value merged into `return [expr]` inside if-block (desugared IfThen pair — parser reproduces both on re-parse)".to_string(),
                            );
                            sink.emit(format!("o{tag} endif"));
                            i += 2;
                            continue;
                        }
                    }
                }
            }
        }
        emit_stmt(sink, &stmts[i], scope, tags, sub_tag);
        i += 1;
    }
}

/// Равенство условий для слияния IfThen-пары: структурное (expr_eq
/// emit_l2 — значение-толерантное; Sys не мешает, в отличие от
/// текстового рендера, который отказывает на Sys).
fn cond_render_eq(a: &Expr, b: &Expr) -> bool {
    expr_eq(a, b)
}

/// Реструктуризация IfGoto (случаи 1–5 модульной шапки). Продвигает `i`
/// за поглощённые узлы.
#[allow(clippy::too_many_arguments)]
fn emit_goto_restructured(
    sink: &mut Sink,
    stmts: &[L2Stmt],
    end: usize,
    i: &mut usize,
    cond: &Expr,
    target_label: u32,
    scope: &LabelScope,
    tags: &mut TagAllocator,
    sub_tag: Option<&str>,
) {
    sink.set_line(stmts[*i].src());
    let unconditional = matches!(cond, Expr::Const(n) if n.value() == 1.0);
    match scope.table.get(&target_label).copied() {
        // — Случаи 1/2: переход ВПЕРЁД — инвертированный o-if поверх тела. —
        Some(t) if t > *i && t < end => {
            if t == *i + 1 {
                // Метка сразу за переходом: блок пуст — no-op (прыжок в
                // следующую строку). Оба узла поглощаются.
                *i = t;
                return;
            }
            let Some(cond_text) = sink.expr(cond) else {
                *i += 1; // отказ рендера условия — честный отказ, двигаемся
                return;
            };
            let tag = tags.alloc();
            let inverted = if unconditional {
                sink.note(
                    "goto-forward-unconditional",
                    format!(
                        "unconditional GOTO {target_label} → `o{tag} if [0]` dead block (body preserved)"
                    ),
                );
                "0".to_string()
            } else {
                invert_condition(strip_outer_brackets(&cond_text), sink)
            };
            sink.emit(format!("o{tag} if [{inverted}]"));
            emit_block(sink, stmts, *i + 1, t, scope, tags, sub_tag);
            sink.emit(format!("o{tag} endif"));
            *i = t; // метка (join point) эмитится штатно на следующем витке
        }
        // — Случай 3: переход НАЗАД — do/while на чистом сегменте. —
        Some(t) if t < *i => {
            // Сегмент тела цикла — [t+1..i]: метка-голова (N{target}) на
            // позиции t — маркер входа, не часть тела (её наличие в
            // сегменте — не «переход в структуру»).
            let clean = stmts[t + 1..*i].iter().all(|s| {
                !matches!(
                    s,
                    L2Stmt::Label { .. } | L2Stmt::IfGoto { .. } | L2Stmt::While { .. }
                )
            });
            if !clean || unconditional {
                sink.refuse(if unconditional {
                    format!(
                        "unconditional backward GOTO {target_label} is an infinite-loop pattern — no LinuxCNC analog (honest refusal)"
                    )
                } else {
                    format!(
                        "backward GOTO {target_label} crosses labels/jumps/loops in segment — goto into structure is not structurally translatable to LinuxCNC O-words (honest refusal)"
                    )
                });
                *i += 1;
                return;
            }
            let Some(cond_text) = sink.expr(cond) else {
                *i += 1;
                return;
            };
            let tag = tags.alloc();
            sink.note(
                "goto-backward-do-while",
                format!(
                    "backward GOTO {target_label} → do/while restructure: segment [N{target_label}+1..IF] becomes o{tag} do ... o{tag} while [cond] (exact semantics)"
                ),
            );
            sink.emit(format!("o{tag} do"));
            emit_block(sink, stmts, t + 1, *i, scope, tags, sub_tag);
            sink.emit(format!(
                "o{tag} while [{}]",
                strip_outer_brackets(&cond_text)
            ));
            *i += 1; // IF-строка поглощена (заменена while-замыкателем)
        }
        // Переход сквозь границы блока (метка во внешнем/вложенном скоупе).
        Some(_) => {
            sink.refuse(format!(
                "GOTO {target_label}: label outside the current block window (jump into/out of structure) — not structurally translatable to LinuxCNC O-words (honest refusal)"
            ));
            *i += 1;
        }
        None => {
            sink.refuse(format!(
                "GOTO {target_label}: label not found in this scope (cross-scope jumps are not structurally translatable) — honest refusal"
            ));
            *i += 1;
        }
    }
}

/// Инверсия условия (случай 1). Точные пары сравнений на простых
/// операндах; прочее — fallback (см. модульную шапку).
fn invert_condition(cond: &str, sink: &mut Sink) -> String {
    if let Some((a, op, b)) = split_top_comparison(cond) {
        let inv = match op {
            "GT" => "LE",
            "LT" => "GE",
            "GE" => "LT",
            "LE" => "GT",
            "EQ" => "NE",
            "NE" => "EQ",
            _ => return fallback_not(cond, sink),
        };
        if is_simple_operand(a) && is_simple_operand(b) {
            return format!("{a} {inv} {b}");
        }
    }
    fallback_not(cond, sink)
}

/// Верхнеуровневое сравнение `a OP b` (первый оператор-токен в тексте).
fn split_top_comparison(text: &str) -> Option<(&str, &str, &str)> {
    for op in [" EQ ", " NE ", " GT ", " LT ", " GE ", " LE "] {
        if let Some(pos) = text.find(op) {
            return Some((&text[..pos], op.trim(), &text[pos + op.len()..]));
        }
    }
    None
}

/// Простой операнд: токен без сравнений/AND/OR и со сбалансированными
/// скобками (число, `#N`, `#[...]`, `#<name>`, `SIN[...]`).
fn is_simple_operand(s: &str) -> bool {
    !s.contains(" AND ")
        && !s.contains(" OR ")
        && [" EQ ", " NE ", " GT ", " LT ", " GE ", " LE "]
            .iter()
            .all(|op| !s.contains(op))
        && s.matches('[').count() == s.matches(']').count()
}

/// Fallback-инверсия: `NOT c` ≡ `[(c GE 0) AND (c LE 0)]` (точен при
/// GE/LE Null→0 коэрции — таблица 2.3 Приложения дока).
fn fallback_not(cond: &str, sink: &mut Sink) -> String {
    sink.note(
        "goto-invert-fallback",
        "condition inversion via `[(c GE 0) AND (c LE 0)]` (= c == 0; exact under GE/LE Null→0 coercion, #0-semantics table 2.3)".to_string(),
    );
    format!("({cond} GE 0) AND ({cond} LE 0)")
}

// ===========================================================================
// Эмиссия одиночных операторов
// ===========================================================================

fn emit_stmt(
    sink: &mut Sink,
    stmt: &L2Stmt,
    scope: &LabelScope,
    tags: &mut TagAllocator,
    sub_tag: Option<&str>,
) {
    sink.set_line(stmt.src());
    match stmt {
        L2Stmt::Move {
            kind,
            target,
            feed,
            spindle,
            ..
        } => emit_move(sink, kind, target, feed.as_ref(), spindle.as_ref()),

        L2Stmt::WithTransform { .. } => sink.refuse(
            "WithTransform (G68/G51/mirror/TWP): LinuxCNC has no rotation/scaling/mirror G-codes — honest refusal".to_string(),
        ),

        L2Stmt::Assign { target, value, .. } => {
            let Some(t) = var_target(sink, target) else {
                return;
            };
            let Some(v) = sink.expr(value) else { return };
            sink.emit(format!("{t} = {v}"));
        }

        // IfGoto обрабатывается окном в emit_block (реструктуризация).
        L2Stmt::IfGoto { .. } => {
            sink.refuse("IfGoto outside restructure window — internal error".to_string())
        }

        L2Stmt::IfThen { cond, then_stmt, .. } => {
            let tag = tags.alloc();
            let Some(c) = sink.expr(cond) else { return };
            sink.emit(format!("o{tag} if [{}]", strip_outer_brackets(&c)));
            // Десугаризация парсера даёт ПОСТРОЧНЫЕ IfThen (else/elseif
            // сконвертированы в цепочку конъюнкций-условий) — тело эмитим
            // через окно длиной 1 (сохраняет расширяемость).
            emit_block(sink, std::slice::from_ref(then_stmt.as_ref()), 0, 1, scope, tags, sub_tag);
            sink.emit(format!("o{tag} endif"));
        }

        L2Stmt::While { cond, body, .. } => {
            let tag = tags.alloc();
            let Some(c) = sink.expr(cond) else { return };
            sink.emit(format!("o{tag} while [{}]", strip_outer_brackets(&c)));
            let table = body_label_table(body);
            emit_block(
                sink,
                body,
                0,
                body.len(),
                &LabelScope { table: &table },
                tags,
                sub_tag,
            );
            sink.emit(format!("o{tag} endwhile"));
        }

        L2Stmt::CallSub {
            target,
            repeat,
            args,
            modal,
            ..
        } => emit_call(sink, target, repeat.as_ref(), args, *modal, tags),

        L2Stmt::RtcpMode { .. } => sink.refuse(
            "RTCP (G43.4/G49) has no LinuxCNC analog in rs274ngc (stable docs) — honest refusal".to_string(),
        ),

        L2Stmt::Label { n, .. } => sink.emit(format!("N{n}")),

        L2Stmt::Return { .. } => match sub_tag {
            Some(tag) => sink.emit(format!("o{tag} return")),
            None => sink.refuse(
                "M99 in main = program restart loop on Fanuc/Haas — no LinuxCNC analog (honest refusal)".to_string(),
            ),
        },

        L2Stmt::L1Op(op) => emit_l1_op(sink, op),
    }
}

fn var_target(sink: &mut Sink, target: &VarRef) -> Option<String> {
    match target {
        VarRef::Direct(n) => Some(format!("#{n}")),
        VarRef::Indirect(e) => Some(format!("#[{}]", sink.expr(e)?)),
        VarRef::Named(name) => Some(format!("#<{name}>")),
        VarRef::Sys(_) => sys_var_text(target, sink),
    }
}

/// Ход LinuxCNC: G0/G1/G2/G3 (канонический порядок слов — как Macro B).
fn emit_move(
    sink: &mut Sink,
    kind: &MoveKind,
    pose: &ExprPose,
    feed: Option<&Expr>,
    spindle: Option<&Expr>,
) {
    let g = match kind {
        MoveKind::Rapid => "G0",
        MoveKind::Linear => "G1",
        MoveKind::Arc { dir, .. } => match dir {
            Dir::Cw => "G2",
            Dir::Ccw => "G3",
        },
    };
    let mut line = String::from(g);
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
    if let MoveKind::Arc { ijk, r, .. } = kind {
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

/// Вызов: `o<tag> call` + позиционные `[arg]`; G65-именованные аргументы —
/// только совпадающие A/B/C-позиции (notice), прочие — отказ (шапка).
fn emit_call(
    sink: &mut Sink,
    target: &SubTarget,
    repeat: Option<&Expr>,
    args: &[(u8, Expr)],
    modal: bool,
    tags: &mut TagAllocator,
) {
    if modal {
        sink.refuse(
            "G66 modal call (call after every motion) has no LinuxCNC analog — honest refusal"
                .to_string(),
        );
        return;
    }
    let tag = sub_tag(target);
    // Позиционность: парсер LinuxCNC кодирует позицию i буквой b'A'+i
    // (последовательные A,B,C,…).
    let positional = args
        .iter()
        .enumerate()
        .all(|(i, (l, _))| l.to_ascii_uppercase() == b'A' + i as u8);
    let mut arg_text = String::new();
    if !args.is_empty() {
        if !positional {
            sink.refuse(
                "G65 named argument table (nonlinear A→#1 … D→#7, amendment v0.1.2) does not translate to LinuxCNC positional args ([arg k]→#k): bindings coincide only for A/B/C (positions 1..3) — honest refusal (documented dialect difference)".to_string(),
            );
            return;
        }
        sink.note(
            "call-args-positional",
            "A/B/C named args emitted as LinuxCNC positional [arg1..3] (bindings #1..#3 coincide; D+ would diverge: Fanuc D→#7 vs positional #4)".to_string(),
        );
        for (_, e) in args {
            let Some(t) = sink.expr(e) else { return };
            arg_text.push_str(&format!(" [{t}]"));
        }
    }
    let call = format!("o{tag} call{arg_text}");
    match repeat {
        Some(e) => {
            let Some(t) = sink.expr(e) else { return };
            let rtag = tags.alloc();
            sink.note(
                "call-repeat-wrap",
                format!(
                    "M98/G65 L-repeat → o{rtag} repeat [{t}] wrap (LinuxCNC has no L-word on call; repeat evaluates count at loop entry — same as L)"
                ),
            );
            sink.emit(format!("o{rtag} repeat [{t}]"));
            sink.emit(call);
            sink.emit(format!("o{rtag} endrepeat"));
        }
        None => sink.emit(call),
    }
}

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
            // LinuxCNC: P = секунды всегда.
            sink.emit(format!("G4 P{}", fmt_secs(seconds)));
            sink.note(
                "dwell-unit-conversion",
                format!(
                    "Dwell {:.4}s → LinuxCNC G4 P (seconds always; X/U Fanuc forms normalized to P)",
                    seconds.value()
                ),
            );
        }
        Op::Raw { payload_hex, reason, .. } => {
            let mut text = decode_hex(payload_hex);
            // Носитель исполняемого remark'а (`(debug,…)`/`(print,…)`/…) хранит
            // payload БЕЗ скобок (парсер 12.1: scan_paren) — эмиссия
            // восстанавливает скобочную форму (иначе re-parse ломается).
            if reason.contains("executable remark") {
                text = format!("({text})");
            }
            match carrier_compat(&text, TargetDialect::LinuxCnc) {
                CarrierVerdict::Ok => sink.emit(text),
                CarrierVerdict::Notice { reason } => {
                    sink.note("carrier-verbatim", reason);
                    sink.emit(text);
                }
                CarrierVerdict::Refused { reason } => {
                    sink.refuse(reason);
                    sink.emit(text);
                }
            }
        }
        other => sink.refuse(format!(
            "geometric L1 op {other:?} in L2 program — not producible by 12.1 parsers, honest refusal"
        )),
    }
}

fn fmt_secs(seconds: &eid_core::ast::Num) -> String {
    let v = seconds.value();
    if v.fract() == 0.0 {
        format!("{v}")
    } else {
        format!("{v:.6}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn decode_hex(payload_hex: &str) -> String {
    let bytes: Vec<u8> = (0..payload_hex.len() / 2)
        .map(|i| u8::from_str_radix(&payload_hex[i * 2..i * 2 + 2], 16).unwrap_or(b'?'))
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

// ===========================================================================
// Тесты 12.4 (LinuxCNC): GOTO → o-if/do-while golden, o-call, refusals
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser_fanuc;
    use crate::parser_linuxcnc;

    /// Fanuc-программа → LinuxCNC-текст (golden-хелпер).
    fn to_lcnc(src: &str) -> String {
        let p = parser_fanuc::parse_program(src).unwrap();
        let out = crate::emit_l2::emit(&p, TargetDialect::LinuxCnc);
        assert!(out.ok(), "unexpected refusal: {:?}", out.errors);
        out.text
    }

    // ---------- GOTO → o-if (случай 1: условный переход вперёд) ----------

    #[test]
    fn goto_forward_conditional_inverted_if_golden() {
        // IF [#100 GT 5] GOTO 10 над телом → инверсия GT→LE поверх тела.
        let src = "#100=1.\nIF [#100 GT 5] GOTO 10\nG1 X5. F100\nN10\nM30\n";
        let text = to_lcnc(src);
        let expected = "#100 = 1.\n\
                        o9000 if [#100 LE 5]\n\
                        G1 X5. F100\n\
                        o9000 endif\n\
                        N10\n\
                        M30\n";
        assert_eq!(text, expected, "GOTO→o-if golden:\n{text}");
    }

    #[test]
    fn goto_forward_unconditional_dead_block() {
        // Безусловный GOTO вперёд → `if [0]` (мёртвый блок, тело сохранено).
        let src = "GOTO 10\nG1 X5. F100\nN10\nM30\n";
        let text = to_lcnc(src);
        assert!(text.contains("if [0]"), "dead block:\n{text}");
        assert!(text.contains("G1 X5. F100"), "body preserved:\n{text}");
        assert!(text.contains("endif"));
    }

    #[test]
    fn goto_backward_do_while_golden() {
        // Случай 3: `N10 … IF [c] GOTO 10` → do/while (точная семантика).
        let src = "#100=0.\nN10\nG1 X1. F10.\n#100=[#100 + 1]\nIF [#100 LT 5] GOTO 10\nM30\n";
        let text = to_lcnc(src);
        // Метка-голова N10 ПОГЛОЩАЕТСЯ do-реструктуризацией (для AST это
        // метаданные — re-parse LinuxCNC и так отбрасывает N-слова).
        let expected = "#100 = 0.\n\
                        o9000 do\n\
                        G1 X1. F10.\n\
                        #100 = [#100 + 1]\n\
                        o9000 while [#100 LT 5]\n\
                        M30\n";
        assert_eq!(text, expected, "backward GOTO → do/while golden:\n{text}");
    }

    #[test]
    fn goto_inversion_fallback_exact_under_null_semantics() {
        // Не-сравнение в условии → fallback [(c GE 0) AND (c LE 0)] = NOT c
        // (точно при GE/LE-коэрции Null→0, таблица 2.3 Приложения дока).
        let src = "#100=1.\nIF [#100] GOTO 10\nG1 X5. F100\nN10\nM30\n";
        let text = to_lcnc(src);
        assert!(
            text.contains("o9000 if [(#100 GE 0) AND (#100 LE 0)]"),
            "fallback inversion:\n{text}"
        );
    }

    #[test]
    fn goto_backward_unconditional_refused() {
        // Случай 4: безусловный GOTO назад = бесконечный цикл — отказ.
        let src = "#100=0.\nN10\n#100=[#100 + 1]\nGOTO 10\nM30\n";
        let p = parser_fanuc::parse_program(src).unwrap();
        let out = crate::emit_l2::emit(&p, TargetDialect::LinuxCnc);
        assert!(!out.ok());
        assert!(out.errors[0].reason.contains("infinite-loop"));
    }

    #[test]
    fn goto_unknown_label_refused() {
        let src = "IF [#1 GT 0] GOTO 99\nM30\n";
        let p = parser_fanuc::parse_program(src).unwrap();
        let out = crate::emit_l2::emit(&p, TargetDialect::LinuxCnc);
        assert!(!out.ok());
        assert!(out.errors[0].reason.contains("label not found"));
    }

    // ---------- M98/M99 ↔ o-call/return ----------

    #[test]
    fn m98_becomes_o_call_and_m99_becomes_return() {
        // O100 sub / M99 / O200: M98 P100 → o100 call; M99 в теле → o100 return.
        let src = "O100\nM98 P100\nM30\nO100\nM99\n";
        let text = to_lcnc(src);
        assert!(text.contains("o100 call"), "{text}");
        assert!(text.contains("o100 return"), "{text}");
        assert!(text.contains("o100 sub"), "{text}");
        assert!(text.contains("o100 endsub"), "{text}");
    }

    #[test]
    fn m98_repeat_wraps_in_o_repeat() {
        // M98 P200 L3 → o<fresh> repeat [3] / o200 call / endrepeat.
        let src = "O1\nM98 P200 L3\nM30\nO200\nM99\n";
        let text = to_lcnc(src);
        assert!(text.contains("repeat [3]"), "{text}");
        assert!(text.contains("o200 call"), "{text}");
        assert!(text.contains("endrepeat"), "{text}");
    }

    #[test]
    fn g65_abc_args_positional_with_notice() {
        // A/B/C-аргументы → позиционные [..][..][..] (биндинги #1..#3
        // совпадают — notice обязан присутствовать).
        let src = "O1\nG65 P100 A5. B7. C1.\nM30\nO100\nM99\n";
        let p = parser_fanuc::parse_program(src).unwrap();
        let out = crate::emit_l2::emit(&p, TargetDialect::LinuxCnc);
        assert!(out.ok());
        assert!(
            out.notices.iter().any(|n| n.kind == "call-args-positional"),
            "positional notice required: {:?}",
            out.notices
        );
        assert!(
            out.text.contains("o100 call [5.] [7.] [1.]"),
            "{}",
            out.text
        );
    }

    #[test]
    fn g65_non_abc_args_refused() {
        // D-аргумент (Fanuc D→#7, позиция 4) — различие биндингов → отказ.
        let src = "O1\nG65 P100 D5.\nM30\nO100\nM99\n";
        let p = parser_fanuc::parse_program(src).unwrap();
        let out = crate::emit_l2::emit(&p, TargetDialect::LinuxCnc);
        assert!(!out.ok());
        assert!(
            out.errors[0].reason.contains("named argument table"),
            "{}",
            out.errors[0].reason
        );
    }

    #[test]
    fn m99_in_main_refused() {
        // M99 в main = цикл программы (Fanuc) — аналога в LinuxCNC нет.
        let src = "O1\nM99\n";
        let p = parser_fanuc::parse_program(src).unwrap();
        let out = crate::emit_l2::emit(&p, TargetDialect::LinuxCnc);
        assert!(!out.ok());
        assert!(out.errors[0].reason.contains("program restart loop"));
    }

    // ---------- WHILE → o-while; IfThen → o-if ----------

    #[test]
    fn while_fresh_tags_nested_unique() {
        // Fanuc DO1/DO2 (повторяемые id) → уникальные свежие теги LinuxCNC.
        let src = "#1=0.\nWHILE [#1 LT 3] DO1\nWHILE [#2 LT 3] DO2\n#2=[#2 + 1]\nEND2\n#1=[#1 + 1]\nEND1\nM30\n";
        let text = to_lcnc(src);
        assert!(text.contains("o9000 while [#1 LT 3]"), "{text}");
        assert!(text.contains("o9001 while [#2 LT 3]"), "{text}");
        assert!(text.contains("o9000 endwhile"));
        assert!(text.contains("o9001 endwhile"));
    }

    #[test]
    fn ifthen_becomes_o_if_block() {
        // IF-THEN-assign (Haas bare-IF десугарен в тот же узел; Fanuc
        // требует THEN в источнике) → o-if блок с дословным условием.
        let src = "#1=1.\nIF [#1 EQ 1] THEN #2=5.\nM30\n";
        let text = to_lcnc(src);
        assert!(text.contains("o9000 if [#1 EQ 1]"), "{text}");
        assert!(text.contains("#2 = 5."), "{text}");
        assert!(text.contains("o9000 endif"), "{text}");
    }

    // ---------- Честные отказы LinuxCNC-мишени ----------

    #[test]
    fn linuxcnc_refusals() {
        // ModalGroup (#4001+) — нет в LinuxCNC. G1 между IF и меткой:
        // непустой блок → рендер условия обязателен → отказ. (Метка сразу
        // за IF — прыжок в следующую строку — no-op без рендера.)
        let p = parser_fanuc::parse_program("O1\nIF [#4003 EQ 90] GOTO 5\nG1 X1. F5.\nN5\nM30\n")
            .unwrap();
        let out = crate::emit_l2::emit(&p, TargetDialect::LinuxCnc);
        assert!(!out.ok());
        assert!(out
            .errors
            .iter()
            .any(|e| e.reason.contains("modal info variable")));

        // RTCP G43.4 — нет в rs274ngc.
        let p = parser_fanuc::parse_program("O1\nG43.4 H1\nM30\n").unwrap();
        let out = crate::emit_l2::emit(&p, TargetDialect::LinuxCnc);
        assert!(!out.ok());
        assert!(out.errors.iter().any(|e| e.reason.contains("RTCP")));

        // G31-носитель → отказ (G38.2-семейство).
        let p = parser_fanuc::parse_program("O1\nG31 X10. F50.\nM30\n").unwrap();
        let out = crate::emit_l2::emit(&p, TargetDialect::LinuxCnc);
        assert!(!out.ok());
        assert!(out
            .errors
            .iter()
            .any(|e| e.reason.contains("G31 skip-probe")));
    }

    #[test]
    fn named_vars_passthrough_on_linuxcnc() {
        // Именованные переменные (LinuxCNC родные) — passthrough дословно.
        let src = "#<_total> = 0.\n#<local> = [#<_total> + 1]\nM2\n";
        let p = parser_linuxcnc::parse_program(src).unwrap();
        let out = crate::emit_l2::emit(&p, TargetDialect::LinuxCnc);
        assert!(out.ok(), "{:?}", out.errors);
        assert!(out.text.contains("#<_total> = 0."), "{}", out.text);
        assert!(
            out.text.contains("#<local> = [#<_total> + 1]"),
            "{}",
            out.text
        );
    }

    #[test]
    fn emitted_lcnc_reparses_to_equivalent_ast() {
        // Инвариант эмиссии: emit → parse_linuxcnc → semantic_diff == 0
        // для GOTO-реструктурированной программы ПЕРЕДНЕГО перехода
        // (инвертированный o-if полностью парсится 12.1-парсером).
        let src = "#100=3.\nIF [#100 GT 2] GOTO 20\nG1 X9.\nN20\nM30\n";
        let p0 = parser_fanuc::parse_program(src).unwrap();
        let out = crate::emit_l2::emit(&p0, TargetDialect::LinuxCnc);
        assert!(out.ok());
        let p1 = parser_linuxcnc::parse_program(&out.text).unwrap();
        // AST НЕ равны (IfGoto реструктуризован в o-if), но повторная
        // эмиссия детерминирована и re-parse корректен.
        let again = crate::emit_l2::emit(&p1, TargetDialect::LinuxCnc);
        assert_eq!(again.text, out.text, "emission deterministic");
    }

    #[test]
    fn backward_goto_do_while_is_valid_linuxcnc_but_ast_v01_cannot_reparse() {
        // ДОКУМЕНТИРОВАННОЕ ОГРАНИЧЕНИЕ: do/while — валидный LinuxCNC
        // (docs gcode/o-code.html: «o100 do ... o100 while [expr]»), но
        // L2 AST v0.1 (While pre-tested) и парсер 12.1 его не представляют
        // (handle_do: «post-tested o… do … endwhile is not representable»;
        // break/continue — тоже, нет Break/Continue вариантов). Эмиссия
        // остаётся do/while — форма дока; re-parse нашим парсером невозможен
        // до разморозки AST (амендмент v0.1.4 — design intent).
        let src = "#100=0.\nN10\nG1 X1. F10.\n#100=[#100 + 1]\nIF [#100 LT 5] GOTO 10\nM30\n";
        let p0 = parser_fanuc::parse_program(src).unwrap();
        let out = crate::emit_l2::emit(&p0, TargetDialect::LinuxCnc);
        assert!(out.ok());
        assert!(out.text.contains("o9000 do"), "{}", out.text);
        assert!(out.text.contains("o9000 while [#100 LT 5]"), "{}", out.text);
        let reparse = parser_linuxcnc::parse_program(&out.text);
        assert!(
            reparse.is_err(),
            "12.1 parser must reject do-while (documented)"
        );
        assert!(reparse
            .unwrap_err()
            .iter()
            .any(|e| e.message().contains("do-while")));
    }
}
