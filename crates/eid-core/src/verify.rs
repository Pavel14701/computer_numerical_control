//! Верификатор канонического AST EID/L1 — отдельный pass (MLIR-паттерн,
//! итерация 4). Вызывается на ЛЮБОМ этапе пайплайна: после parse, после
//! emit (по повторно разобранному тексту), standalone. parser.rs и
//! emitter.rs делегируют сюда — единственная копия правил валидации.
//!
//! Итерация 5 (MLIR verify-паттерн): инварианты сгруппированы ПО ТИПУ ОПА —
//! [`verify_op`] (exhaustive match по вариантам Op: добавление нового варианта
//! Op = обязательный verify-арм, компилятор enforce) + [`verify_kept_op`]
//! (инварианты итогового AST после отбрасывания zero-length) + program-level
//! pre ([`verify_program_end`]) / post ([`check_linking`]). Поведение
//! идентично итерации 4 — рефактор под защитой тестов (48 golden + 36 unit).
//!
//! ===========================================================================
//! ЧЕК-ЛИСТ ОБЯЗАТЕЛЬСТВ SPEC-EID-L1 v0.17 → ТЕСТЫ (grep по obligation-словам
//! §2/§3/§5/§6/§10; трассируемость правило↔тест. Тесты: tests/spec_obligations.rs
//! [новые, spec_*], tests/roundtrip.rs, tests/grammar_conformance.rs):
//!   §2 принцип 2 (Q11):
//!     (a) первый motion без якоря — unknown легален / known нелегален
//!          → spec_3_q11_breakpoint_matrix [a-known, a-unknown, a-anchor-*]
//!     (b) смена wcs, критерий РАЗНОСТИ (обе системы in-text) —
//!          → spec_3_q11_breakpoint_matrix [b-unknown, b-known, b-computable-*],
//!            roundtrip::cross_wcs_linking_with_defines, known_at_uncomputable_rejected
//!     (c) machine-frame ход (вычислим ⇔ нет preset И origin in-text;
//!         L20-origin не считается; последующие цепляются) —
//!          → spec_3_q11_breakpoint_matrix [c-known-ok, c-unknown-err, c-noorigin-*,
//!            c-preset-*], roundtrip::machine_frame_linking
//!     (d) wcs-ход после machine (тот же критерий) —
//!          → spec_3_q11_breakpoint_matrix [d-withorigin-*, d-noorigin-*]
//!     unknown-оп обязан быть RAPID/LINEAR; ARC unknown → err
//!          → roundtrip::verify_broken_ast_exact_codes [#8], neg_arc_unknown_start
//!     связывание вне разрывов = end предыдущего (точное равенство/якоря) —
//!          → roundtrip::unknown_outside_break_rejected, preset_anchor_rules,
//!            set_here_anchor_rules
//!     частичный end (дефолт-0 в (a) / наследует неизвестное в (b)-(d) /
//!         machine — только полные) — N/A-core: end в EID AST — полный vec3,
//!         правило канонизатора диалекта (агенты C/D)
//!   §3:
//!     program_end ровно один / последний / единственный в минимальной —
//!          → spec_3_pend_invariants_three_modes (×3 режима: parse / validate /
//!            validate_with_notices), minimal_program_valid,
//!            verify_broken_ast_exact_codes [#1-3]
//!     linear/arc в итоговом AST требуют feed ≠ none (после отбрасывания!) —
//!          → spec_3_feed_final_ast, roundtrip::linear_feed_none_rejected,
//!            zero_length_linear_feed_none_dropped_not_error
//!     linear/arc: feed present-but-zero → refusal «zero feed rate on move»
//!          (реальные контроллеры отказывают ход: rs274 «Cannot do g1 with
//!          zero feed rate»; feed=None — прежнее правило без изменений;
//!          G93-resolved feed, заквантованный в 0.0000, ловится тем же
//!          правилом nval==0.0; rapid без ограничений) —
//!          → spec_3_feed_zero_on_move_refused, spec_3_feed_final_ast,
//!            spec_4_rapid_feed_no_constraint
//!     zero-length (start == end; arc: sweep≠360 + pitch=0; dwell 0) —
//!         drop+notice, не ошибка → spec_3_zero_length_all_kinds_with_preset,
//!         roundtrip::zero_length_rapid_dropped_with_notice, dwell_zero_*
//!     start == end + center ≠ start + sweep = 360 = полный круг (kept) —
//!          → spec_3_full_circle_sweep360_kept [+ 359-drop негатив]
//!     path_mode глобален (один на программу) —
//!          → spec_3_path_mode_single (дубликат строки = lexical err; AST —
//!            одно поле по конструкции)
//!     machine-frame — только полные end — N/A-core (см. §2 частичный end)
//!   §5 / D-правила (grammar.ebnf v0.3):
//!     D4 TOOL_CHANGE tool= == state.tool → spec_5_d4_tool_mismatch,
//!         roundtrip::d4_tool_change_mismatch, verify_broken [#12]
//!     D5 пресетки до первого motion-опа → roundtrip::d5_set_here_after_motion,
//!         verify_broken [#13]
//!     D6 RAW/PEND без state → grammar_conformance::op_pend_valid, op_raw_valid
//!     D7 sweep ∈ (0°,360°] / |normal|=1 / dwell ≥ 0 / p ∈ 1..9 →
//!         spec_5_d7_domains, roundtrip::arc_domains_and_geometry,
//!         verify_broken [#9,#10,#14,#15], grammar_conformance::p_domain_lexical_boundary
//!     D8 sub= только RAPID/LINEAR/ARC/DWELL → grammar_conformance::
//!         sub_valid_on_motion_ops, sub_invalid_on_non_motion
//!   §6 (в EID AST нет R-формы — R-правила знака/chord>2R/start==end —
//!      канонизатор C/D; инварианты РЕЗУЛЬТАата проверяются здесь):
//!     канонический знак нормали (PINNED канон-форма, аудит #13): normal =
//!          положительная ось плоскости (+X/+Y/+Z — ровно одна компонента
//!          ≈ +1, остальные ≈ 0; (0,0,-1)-перевёртыш и не-осевые — не канон:
//!          dir определён правой рукой относительно +normal, «переворачивать
//!          dir» запрещено; единая точка правила для всех адаптеров,
//!          зеркало plane_of validate_linuxcnc) —
//!          → spec_6_normal_positive_axis
//!     ρ_start ≈ ρ_end планиметрически (хорда > 2R → невозможная дуга) —
//!          → spec_6_impossible_arc, roundtrip::arc_domains_and_geometry
//!     (end−start)·normal ≈ pitch → verify_broken [#11-семейство],
//!         arc_domains_and_geometry
//!     уравнение позиции end ≈ center + Rot(n,dir,sweep)·(start−center) +
//!         pitch·normal (закрывает sweep vs 360°−sweep) →
//!         spec_6_arc_direction_must_match_rotation, verify_broken [#11]
//!     R+pitch валиден (pitch — обычное поле) → spec_6_arc_units_agnostic_inch
//!         [+ pitch≠0 во всех arc-тестах], golden #46
//!   §10:
//!     10.2 эмиссия 4dp/без экспоненты → roundtrip::fmt_num_rules
//!     10.4 eid_version = версия grammar → grammar_conformance::magic_*
//!     10.5 single-final-rounding — N/A-core (канонизатор C/D); emit-сторона:
//!         emitter::emit_num округляет одним fmt_num (без промежуточных)
//!     10.6 квантование входа к 4dp; zero-check ПОСЛЕ квантования —
//!          → spec_10_quantized_zero_check [-0.0000 ≡ 0.0000 → drop;
//!            0.0001 ≠ 0.0000 → kept]
//! ===========================================================================
//!
//! Источники правил: SPEC-EID-L1 v0.17 (§3 инварианты, §5 опы, §6 каноническая
//! форма дуги, §2 принцип 2 / Q11 — точки разрыва и связывание), grammar.ebnf
//! v0.4 (D4–D8; op_num 3+ цифр, экранирование quoted — back-compat superset),
//! допуски — EQUIVALENCE §4. Все диагностики pass'а помечены
//! [`Stage::Verify`] (interface v0.1.2).
//!
//! Два режима:
//!  * [`validate_with_notices`] — путь канонизации EID-текста: zero-length
//!    опы отбрасываются с notice (SPEC §3; CORPUS §4 — notice-файлы только
//!    R1-AST), остальные нарушения — диагностики;
//!  * [`validate`] — путь эмиссии/standalone: канонический AST обязан быть
//!    уже без zero-length (их отбросила канонизация) — наличие = диагностика.
//!
//! Замечания по интерпретации SPEC (утверждены оркестратором, SPEC v0.10):
//!  * точка разрыва (b) Q11 — критерий РАЗНОСТИ: start после смены wcs
//!    вычислим ⇔ ОБЕ системы (исходная и целевая) имеют in-text wcs_define
//!    того же P до точки хода; reading_new = prev_end + O_prev − O_new
//!    (EQUIVALENCE §1.3, Δ константен);
//!  * точки разрыва (c)/(d) Q11 — критерий вычислимости: machine-frame start
//!    вычислим ⇔ нет frame_preset (Δ=0) И origin активной WCS определён
//!    in-text (§5.3b; L20-происхождение origin не считается — M-производное);
//!    последующие machine-ходы цепляются: start = end предыдущего;
//!  * G10 L20 (wcs_set_here) задаёт чтение текущей позиции = delta, но
//!    машинное значение origin (O_p = M_here − delta) при M₀-неизвестности
//!    НЕ считается «известным из текста» для кросс-фреймовых преобразований;
//!  * N1 (E_RUNTIME_STATE за невыразимость machine-ходов, §5.3) — диспозиция
//!    канонизатора диалекта-источника; verify проверяет только каноническую
//!    форму (unknown в легальной точке разрыва валиден).

use crate::ast::*;
use crate::diag::{Diagnostic, ErrorCode, Stage};

// ===========================================================================
// Notices (drop-with-notice, SPEC §3 / §5.2 / §6). Notice — не Diagnostic:
// это не код ошибки, а флаг легального отброса (CORPUS §4: notice-файлы —
// только R1-AST).
// ===========================================================================

/// Класс отброшенного опа.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// rapid/linear: start == end (SPEC §3).
    ZeroLengthMotion,
    /// arc: start == end при sweep != 360° и pitch == 0 (SPEC §3/§6; включает
    /// sweep=0 — CORPUS вектор #13).
    ZeroLengthArc,
    /// dwell: seconds == 0 (SPEC §5.2).
    ZeroDwell,
}

/// Отброшенный оп + контекст отброса.
#[derive(Debug, Clone)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
    /// Индекс опа в исходном (до отбрасывания) списке `ops`.
    pub op_index: usize,
    /// Сам отброшенный оп («верни отброшенные»).
    pub op: Op,
}

/// Результат разбора: программа (zero-length отброшены) + notices.

// ===========================================================================
// Диагностика
// ===========================================================================

fn diag(code: ErrorCode, msg: impl Into<String>, src: Option<SrcSpan>) -> Diagnostic {
    Diagnostic {
        code,
        stage: Stage::Verify,
        message: msg.into(),
        src,
        options: Vec::new(),
        limit: None,
    }
}

fn op_src(op: &Op) -> Option<SrcSpan> {
    match op {
        Op::Rapid { src, .. }
        | Op::Linear { src, .. }
        | Op::Arc { src, .. }
        | Op::Dwell { src, .. }
        | Op::ToolChange { src, .. }
        | Op::ProgramStop { src, .. }
        | Op::ProgramEnd { src }
        | Op::WcsDefine { src, .. }
        | Op::WcsSetHere { src, .. }
        | Op::Raw { src, .. } => Some(src.clone()),
    }
}

/// Номер системы p (1..9) ↔ имя WCS: G54=1 … G59=6, G59.1=7, G59.2=8, G59.3=9.
pub fn wcs_to_p(w: Wcs) -> u8 {
    match w {
        Wcs::G54 => 1,
        Wcs::G55 => 2,
        Wcs::G56 => 3,
        Wcs::G57 => 4,
        Wcs::G58 => 5,
        Wcs::G59 => 6,
        Wcs::G59_1 => 7,
        Wcs::G59_2 => 8,
        Wcs::G59_3 => 9,
    }
}

// ===========================================================================
// Публичный API pass'а
// ===========================================================================

/// Отчёт validate_with_notices: выжившие опы, notices (отброшенное),
/// диагностики (пусто = валидно).
#[derive(Debug, Clone)]
pub struct VerifyReport {
    /// Опы, выжившие после отбрасывания zero-length (исходный порядок).
    pub kept_ops: Vec<Op>,
    /// Отброшенные опы (drop-with-notice, SPEC §3/§5.2/§6).
    pub notices: Vec<Notice>,
    /// Нарушения канонической формы (Stage::Verify).
    pub diagnostics: Vec<Diagnostic>,
}

/// Верификация с семантикой канонизации: zero-length опы отбрасываются с
/// notice, остальные нарушения — диагностики. Путь parse/канонизации.
pub fn validate_with_notices(program: &Program) -> VerifyReport {
    let (kept_ops, notices, diagnostics) = run(program, false);
    VerifyReport { kept_ops, notices, diagnostics }
}

/// Верификация канонической формы (путь эмиссии/standalone): zero-length
/// опы в AST = диагностика («канонизация обязана была отбросить», SPEC §3).
pub fn validate(program: &Program) -> Vec<Diagnostic> {
    run(program, true).2
}

// ===========================================================================
// Валидация (SPEC §3 + D4–D8 + §6) — собирает все ошибки
// ===========================================================================

fn is_motion(op: &Op) -> bool {
    matches!(op, Op::Rapid { .. } | Op::Linear { .. } | Op::Arc { .. })
}

fn nval(n: &Num) -> f64 {
    n.value
}

/// Точное равенство значений (f64-зеркало; verbatim-текст гарантируется
/// эмиттером — SPEC §10.1). -0.0 == 0.0.
fn p3_eq(a: &Point3, b: &Point3) -> bool {
    nval(&a.x) == nval(&b.x) && nval(&a.y) == nval(&b.y) && nval(&a.z) == nval(&b.z)
}

fn motion_parts(op: &Op) -> Option<(&StartPoint, &Point3, &State)> {
    match op {
        Op::Rapid { start, end, state, .. }
        | Op::Linear { start, end, state, .. }
        | Op::Arc { start, end, state, .. } => Some((start, end, state)),
        _ => None,
    }
}

/// Контекст verify одного опа (MLIR-паттерн: pass'овое состояние, общее
/// для всех армов match'а).
struct VerifyCtx {
    /// Индекс опа в исходном (сыром) списке ops.
    index: usize,
    /// Индекс первого motion-опа сырого списка. FRAME_PRESET — поле заголовка
    /// (позиция структурная); WCS_SET_HERE — оп, проверяется по текстовому
    /// порядку (D5).
    first_motion_raw: Option<usize>,
}

/// Результат verify одного опа (до cross-op-проходов).
#[derive(Default)]
struct OpVerify {
    /// Нарушения, действительные независимо от отбрасывания zero-length
    /// (D4, D5, D7-домены).
    hard: Vec<Diagnostic>,
    /// zero-length кандидат (SPEC §3): notice в режиме канонизации,
    /// диагностика в режиме эмиссии.
    zero: Option<(NoticeKind, String)>,
}

/// Program-level pre: §3 program_end — ровно один, последний; в минимальной
/// программе — единственный; опов после него нет.
fn verify_program_end(ops: &[Op], errors: &mut Vec<Diagnostic>) {
    if ops.is_empty() {
        errors.push(diag(
            ErrorCode::InvalidInput,
            "program has no ops: program_end (PEND) required exactly once as the last op (SPEC §3)",
            None,
        ));
    }
    let end_count = ops.iter().filter(|o| matches!(o, Op::ProgramEnd { .. })).count();
    if end_count == 0 && !ops.is_empty() {
        errors.push(diag(
            ErrorCode::InvalidInput,
            "missing program_end (PEND) — required exactly once, last (SPEC §3)",
            None,
        ));
    }
    if end_count > 1 {
        for o in ops.iter().filter(|o| matches!(o, Op::ProgramEnd { .. })) {
            errors.push(diag(
                ErrorCode::InvalidInput,
                "duplicate program_end (SPEC §3: ровно один PEND)",
                op_src(o),
            ));
        }
    }
    if let Some(last) = ops.last() {
        if !matches!(last, Op::ProgramEnd { .. }) {
            errors.push(diag(
                ErrorCode::InvalidInput,
                "program_end must be the last op (SPEC §3)",
                op_src(last),
            ));
        }
    }
}

/// Per-op verify (MLIR verify-паттерн, итерация 5): инварианты сгруппированы
/// ПО ТИПУ ОПА (exhaustive match по вариантам Op — добавление нового варианта
/// требует арма здесь, компилятор enforce), не по типу правила.
/// zero-кандидат вынесен отдельно: его судьба (notice vs диагностика) решает
/// run() по режиму прохода.
fn verify_op(op: &Op, ctx: &VerifyCtx) -> OpVerify {
    let mut v = OpVerify::default();
    match op {
        Op::Rapid { start, end, .. } => {
            v.zero = zero_motion(start, end);
        }
        Op::Linear { start, end, .. } => {
            v.zero = zero_motion(start, end);
        }
        Op::Arc { start, end, sweep_deg, pitch, .. } => {
            // start == end при sweep != 360° и pitch == 0 — нулевой ход,
            // не дуга (SPEC §3/§6; sweep=0 сюда же — CORPUS #13).
            v.zero = zero_arc(start, end, sweep_deg, pitch);
        }
        Op::Dwell { seconds, .. } => {
            if nval(seconds) < 0.0 {
                v.hard.push(diag(
                    ErrorCode::InvalidInput,
                    format!("dwell seconds={} < 0 (D7, SPEC §5.2)", seconds.text),
                    op_src(op),
                ));
            }
            if nval(seconds) == 0.0 {
                v.zero = Some((NoticeKind::ZeroDwell, "zero dwell dropped (seconds == 0, SPEC §5.2)".into()));
            }
        }
        Op::ToolChange { tool, state, .. } => {
            // D4: значение op-поля tool= равно state-полю tool=
            if state.tool != Some(*tool) {
                v.hard.push(diag(
                    ErrorCode::InvalidInput,
                    format!("TOOL_CHANGE tool={tool} != state tool={} (D4)", state.tool.map(|t| t.to_string()).unwrap_or_else(|| "none".into())),
                    op_src(op),
                ));
            }
        }
        Op::ProgramStop { .. } => {}
        Op::ProgramEnd { .. } => {} // программа-уровень: verify_program_end
        Op::WcsDefine { p, .. } => {
            check_p_domain(op, *p, &mut v.hard);
        }
        Op::WcsSetHere { p, .. } => {
            check_p_domain(op, *p, &mut v.hard);
            // D5: только до первого motion-опа (SPEC §3/§5.3)
            if let Some(fm) = ctx.first_motion_raw {
                if ctx.index > fm {
                    v.hard.push(diag(
                        ErrorCode::InvalidInput,
                        "WCS_SET_HERE after first motion op (D5, SPEC §3/§5.3: only a preset before the first motion)",
                        op_src(op),
                    ));
                }
            }
        }
        Op::Raw { .. } => {}
    }
    v
}

/// D7: домен номера системы p (1..9: G54..G59.3).
fn check_p_domain(op: &Op, p: u8, errors: &mut Vec<Diagnostic>) {
    if !(1..=9).contains(&p) {
        errors.push(diag(
            ErrorCode::InvalidInput,
            format!("p={p} out of domain 1..9 (D7: G54..G59.3)"),
            op_src(op),
        ));
    }
}

/// zero-length candidate для rapid/linear.
fn zero_motion(start: &StartPoint, end: &Point3) -> Option<(NoticeKind, String)> {
    if let StartPoint::Known(s) = start {
        if p3_eq(s, end) {
            return Some((NoticeKind::ZeroLengthMotion, "zero-length motion dropped (start == end, SPEC §3)".into()));
        }
    }
    None
}

/// zero-length candidate для arc.
fn zero_arc(start: &StartPoint, end: &Point3, sweep_deg: &Num, pitch: &Num) -> Option<(NoticeKind, String)> {
    if let StartPoint::Known(s) = start {
        if p3_eq(s, end) && nval(sweep_deg) != 360.0 && nval(pitch) == 0.0 {
            return Some((NoticeKind::ZeroLengthArc, "zero-length arc dropped (start == end, sweep != 360°, pitch == 0; SPEC §3/§6)".into()));
        }
    }
    None
}

/// Kept-pass (exhaustive match по вариантам Op): инварианты «итогового AST» —
/// применяются только к выжившим после отбрасывания zero-length (SPEC §3:
/// feed для linear/arc; D7-домены дуги; ARC с unknown-стартом) + §6-геометрия
/// (eps_pos — по выжившим endpoint'ам, EQUIVALENCE §4).
fn verify_kept_op(op: &Op, eps_pos: f64, errors: &mut Vec<Diagnostic>) {
    match op {
        Op::Linear { state, .. } => {
            check_feed_modal(op, "linear", state, errors);
        }
        Op::Arc { start, end, center, normal, dir, sweep_deg, pitch, state, .. } => {
            if matches!(start, StartPoint::Unknown) {
                errors.push(diag(
                    ErrorCode::InvalidInput,
                    "arc with start=unknown is invalid (SPEC §2 Q11: arc requires a numeric start)",
                    op_src(op),
                ));
            }
            let s = nval(sweep_deg);
            if s <= 0.0 || s > 360.0 {
                errors.push(diag(
                    ErrorCode::InvalidInput,
                    format!("sweep={} out of domain (0°, 360°] (D7, SPEC §6)", sweep_deg.text),
                    op_src(op),
                ));
            }
            let len = vlen(&vec_vals(normal));
            if (len - 1.0).abs() > 0.001 {
                // 0.001 — допуск 4dp-квантования компонент единичного вектора (D7)
                errors.push(diag(
                    ErrorCode::InvalidInput,
                    format!("|normal| = {len:.6} != 1 (D7)"),
                    op_src(op),
                ));
            }
            check_normal_positive_axis(op, normal, errors);
            check_feed_modal(op, "arc", state, errors);
            if let StartPoint::Known(sk) = start {
                check_arc_geometry(op, sk, end, center, normal, *dir, nval(sweep_deg), nval(pitch), eps_pos, errors);
            }
        }
        Op::Rapid { .. }
        | Op::Dwell { .. }
        | Op::ToolChange { .. }
        | Op::ProgramStop { .. }
        | Op::ProgramEnd { .. }
        | Op::WcsDefine { .. }
        | Op::WcsSetHere { .. }
        | Op::Raw { .. } => {}
    }
}

/// feed на linear/arc (итоговый AST, SPEC §3 + аудит #32): модальное значение
/// обязано присутствовать (feed=none — прежнее правило, без изменений) И быть
/// ненулевым: feed present-but-zero → refusal «zero feed rate on move»
/// (реальные контроллеры отказывают ход: rs274 «Cannot do g1 with zero feed
/// rate»). G93-resolved feed, заквантованный в 0.0000, ловится тем же правилом
/// (nval == 0.0 покрывает и -0.0). Rapid без ограничений (SPEC §4, G0).
/// Проверка — по выжившим: zero-length оп с feed=0/none отброшен канонизацией
/// и не диагностируется (spec_3_feed_final_ast).
fn check_feed_modal(op: &Op, what: &str, state: &State, errors: &mut Vec<Diagnostic>) {
    match &state.feed {
        None => errors.push(diag(
            ErrorCode::InvalidInput,
            format!(
                "{what} with feed=none (SPEC §3: linear/arc in the final AST require modal feed)"
            ),
            op_src(op),
        )),
        Some(f) if nval(f) == 0.0 => errors.push(diag(
            ErrorCode::InvalidInput,
            format!(
                "{what} with feed={} — zero feed rate on move (feed must be > 0; real controllers refuse the move: rs274 \"Cannot do g1 with zero feed rate\")",
                f.text
            ),
            op_src(op),
        )),
        Some(_) => {}
    }
}

/// Канонический знак нормали дуги (SPEC §6, PINNED канон-форма — аудит #13):
/// normal = положительная ось плоскости (+X/+Y/+Z) — ровно одна компонента
/// ≈ +1, остальные ≈ 0 (допуск 0.001 — 4dp-квантование компонент). Отрицательная
/// ось (напр. (0,0,-1)) — не канон: dir определён правилом правой руки
/// относительно +normal, «переворачивать dir» запрещено ruling'ом. Не-осевые
/// нормали не выражают G17/G18/G19. Единая точка правила для всех адаптеров
/// (зеркало plane_of validate_linuxcnc — эмиттеры дают свою диагностику
/// независимо; verify обязателен для любого источника AST).
fn check_normal_positive_axis(op: &Op, normal: &Point3, errors: &mut Vec<Diagnostic>) {
    let nv = vec_vals(normal);
    let t = 0.001_f64;
    let pos_axis = ((nv.0 - 1.0).abs() <= t && nv.1.abs() <= t && nv.2.abs() <= t)
        || (nv.0.abs() <= t && (nv.1 - 1.0).abs() <= t && nv.2.abs() <= t)
        || (nv.0.abs() <= t && nv.1.abs() <= t && (nv.2 - 1.0).abs() <= t);
    if !pos_axis {
        errors.push(diag(
            ErrorCode::InvalidInput,
            format!(
                "arc normal = ({:.4},{:.4},{:.4}) is not a canonical +axis unit vector (SPEC §6: G17→+Z, G18→+Y, G19→+X; flipped or off-axis normal is not canonical)",
                nv.0, nv.1, nv.2
            ),
            op_src(op),
        ));
    }
}

fn run(program: &Program, reject_zero_length: bool) -> (Vec<Op>, Vec<Notice>, Vec<Diagnostic>) {
    let mut errors: Vec<Diagnostic> = Vec::new();
    let ops = &program.ops;

    // --- Program-level pre: §3 program_end ----------------------------------
    verify_program_end(ops, &mut errors);
    let first_motion_raw = ops.iter().position(is_motion);

    // --- Per-op pass (MLIR verify-паттерн: exhaustive match по Op) ----------
    //     Отбрасывание zero-length: Parse-путь — drop+notice; Emit-путь —
    //     ошибка (SPEC §3).
    let mut kept: Vec<Op> = Vec::new();
    let mut notices: Vec<Notice> = Vec::new();
    for (i, o) in ops.iter().enumerate() {
        let ctx = VerifyCtx { index: i, first_motion_raw };
        let ov = verify_op(o, &ctx);
        errors.extend(ov.hard);
        match ov.zero {
            Some((kind, message)) => {
                if reject_zero_length {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!(
                            "{} must be dropped by canonicalization before emission (SPEC §3)",
                            message
                        ),
                        op_src(o),
                    ));
                    kept.push(o.clone());
                } else {
                    notices.push(Notice { kind, message, op_index: i, op: o.clone() });
                }
            }
            None => kept.push(o.clone()),
        }
    }

    // --- Kept-pass: инварианты итогового AST + §6-геометрия -----------------
    let eps_pos = eps_for(&kept);
    for o in kept.iter() {
        verify_kept_op(o, eps_pos, &mut errors);
    }

    // --- Program-level post: связывание start (SPEC §2 принцип 2, Q11; v0.9
    //     обязанность) по выжившим -------------------------------------------
    check_linking(program, &kept, eps_pos, &mut errors);

    (kept, notices, errors)
}

// ===========================================================================
// §6 — геометрия дуги (допуски — EQUIVALENCE §4)
// ===========================================================================

type V3 = (f64, f64, f64);

fn vec_vals(p: &Point3) -> V3 {
    (nval(&p.x), nval(&p.y), nval(&p.z))
}

fn p3_vals(p: &Point3) -> V3 {
    (nval(&p.x), nval(&p.y), nval(&p.z))
}

fn vlen(v: &V3) -> f64 {
    v.0.hypot(v.1).hypot(v.2)
}

fn vsub(a: &V3, b: &V3) -> V3 {
    (a.0 - b.0, a.1 - b.1, a.2 - b.2)
}

fn vadd(a: &V3, b: &V3) -> V3 {
    (a.0 + b.0, a.1 + b.1, a.2 + b.2)
}

fn vscale(a: &V3, k: f64) -> V3 {
    (a.0 * k, a.1 * k, a.2 * k)
}

fn vdot(a: &V3, b: &V3) -> f64 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

fn vcross(a: &V3, b: &V3) -> V3 {
    (a.1 * b.2 - a.2 * b.1, a.2 * b.0 - a.0 * b.2, a.0 * b.1 - a.1 * b.0)
}

fn vdist(a: &V3, b: &V3) -> f64 {
    vlen(&vsub(a, b))
}

/// Масштаб L (диагональ bbox всех endpoint'ов motion-опов, EQUIVALENCE §4) →
/// ε_pos = max(0.001, 1e-7·L).
fn eps_for(ops: &[Op]) -> f64 {
    let mut min = (f64::MAX, f64::MAX, f64::MAX);
    let mut max = (f64::MIN, f64::MIN, f64::MIN);
    let mut any = false;
    let mut take = |v: V3| {
        any = true;
        min.0 = min.0.min(v.0); min.1 = min.1.min(v.1); min.2 = min.2.min(v.2);
        max.0 = max.0.max(v.0); max.1 = max.1.max(v.1); max.2 = max.2.max(v.2);
    };
    for o in ops {
        if let Some((start, end, _)) = motion_parts(o) {
            if let StartPoint::Known(k) = start {
                take(p3_vals(k));
            }
            take(p3_vals(end));
        }
    }
    let l = if any {
        vlen(&(max.0 - min.0, max.1 - min.1, max.2 - min.2))
    } else {
        0.0
    };
    (0.001_f64).max(1e-7 * l)
}

/// Rodrigues: поворот v вокруг единичной оси n на угол θ (ccw — положительный).
fn rotate(v: &V3, n: &V3, theta: f64) -> V3 {
    let (s, c) = theta.sin_cos();
    let nxv = vcross(n, v);
    let nv = vdot(n, v);
    vadd(
        &vadd(
            &vscale(v, c),
            &vscale(&nxv, s),
        ),
        &vscale(n, nv * (1.0 - c)),
    )
}

#[allow(clippy::too_many_arguments)]
fn check_arc_geometry(
    op: &Op,
    start: &Point3,
    end: &Point3,
    center: &Point3,
    normal: &Point3,
    dir: Dir,
    sweep: f64,
    pitch: f64,
    eps_pos: f64,
    errors: &mut Vec<Diagnostic>,
) {
    let src = op_src(op);
    let s = p3_vals(start);
    let e = p3_vals(end);
    let c = p3_vals(center);
    let n_raw = vec_vals(normal);
    let nlen = vlen(&n_raw);
    let n = vscale(&n_raw, 1.0 / nlen); // |n|≈1 проверен доменом (D7)

    // 1. Планиметрический радиус: ρ_start ≈ ρ_end (проекция ⟂ normal).
    let planar = |d: &V3| -> V3 { vsub(d, &vscale(&n, vdot(&n, d))) };
    let rho_s = vlen(&planar(&vsub(&s, &c)));
    let rho_e = vlen(&planar(&vsub(&e, &c)));
    let tol_r = eps_pos.max(1e-6 * rho_s);
    if (rho_s - rho_e).abs() > tol_r {
        errors.push(diag(
            ErrorCode::InvalidInput,
            format!(
                "arc radius mismatch: |start| = {rho_s:.6}, |end| = {rho_e:.6}, tol = {tol_r:.6} (SPEC §6.1)"
            ),
            src.clone(),
        ));
    }

    // 2. Нормальная компонента хорды ≈ pitch.
    let chord_n = vdot(&vsub(&e, &s), &n);
    if (chord_n - pitch).abs() > eps_pos {
        errors.push(diag(
            ErrorCode::InvalidInput,
            format!(
                "arc chord·normal = {chord_n:.6} != pitch = {pitch:.6} (tol {eps_pos:.6}, SPEC §6.2)"
            ),
            src.clone(),
        ));
    }

    // 3. Полное уравнение позиции: end ≈ center + Rot(n, dir, sweep)·(start−center) + pitch·n.
    let theta = sweep.to_radians() * if dir == Dir::Cw { -1.0 } else { 1.0 };
    let rotated = rotate(&vsub(&s, &c), &n, theta);
    let expected = vadd(&vadd(&c, &rotated), &vscale(&n, pitch));
    let d = vdist(&expected, &e);
    if d > eps_pos {
        errors.push(diag(
            ErrorCode::InvalidInput,
            format!(
                "arc endpoint inconsistent with (center, normal, dir, sweep, pitch): |Δ| = {d:.6} > {eps_pos:.6} (SPEC §6.3)"
            ),
            src,
        ));
    }
}

// ===========================================================================
// Связывание start — SPEC §2 принцип 2 / Q11, v0.9 (точки разрыва (a)–(d))
// ===========================================================================

/// Ожидаемый start: точное значение (verbatim-цепочка), значение с допуском
/// (вычислено через origin-арифметику) или обязательный unknown.
#[derive(Debug, Clone)]
enum StartExpect {
    Exact(Point3),
    Tol(Point3),
    UnknownRequired,
}

struct PrevMotion {
    end: Point3,
    frame: Frame,
    wcs: Wcs,
}

fn p3_build(v: V3) -> Point3 {
    let mk = |x: f64| Num::new(crate::emitter::fmt_num(x), x);
    Point3 { x: mk(v.0), y: mk(v.1), z: mk(v.2) }
}

#[allow(clippy::too_many_arguments)]
fn expect_start(
    is_first: bool,
    frame: Frame,
    wcs: Wcs,
    preset: Option<&FramePreset>,
    set_here: &[Option<Point3>; 10],
    known_origin: &[Option<Point3>; 10],
    prev: Option<&PrevMotion>,
) -> StartExpect {
    let op_o = known_origin[wcs_to_p(wcs) as usize].clone();
    if !is_first {
        let prev = prev.expect("prev exists when not first");
        let prev_o = known_origin[wcs_to_p(prev.wcs) as usize].clone();
        return match (prev.frame, frame) {
            (Frame::Machine, Frame::Machine) => StartExpect::Exact(prev.end.clone()),
            (Frame::Wcs, Frame::Wcs) if prev.wcs == wcs => StartExpect::Exact(prev.end.clone()),
            (Frame::Wcs, Frame::Wcs) => match (&prev_o, &op_o) {
                // start = prev_end + O_prev − O_new (Δ константен, EQUIVALENCE §1.3)
                (Some(o_prev), Some(o_new)) => StartExpect::Tol(p3_build(vadd(
                    &p3_vals(&prev.end),
                    &vsub(&p3_vals(o_prev), &p3_vals(o_new)),
                ))),
                _ => StartExpect::UnknownRequired, // (b): разность origin'ов не вычислима
            },
            (Frame::Wcs, Frame::Machine) => {
                // M = reading + O_wcs + S; S = 0 только без frame_preset (SPEC §5.3)
                if preset.is_none() {
                    if let Some(op_) = &prev_o {
                        StartExpect::Tol(p3_build(vadd(&p3_vals(&prev.end), &p3_vals(op_))))
                    } else {
                        StartExpect::UnknownRequired
                    }
                } else {
                    StartExpect::UnknownRequired // (c): M₀ внешний
                }
            }
            (Frame::Machine, Frame::Wcs) => {
                // reading = M − O_wcs − S
                if preset.is_none() {
                    if let Some(on) = &op_o {
                        StartExpect::Tol(p3_build(vsub(&p3_vals(&prev.end), &p3_vals(on))))
                    } else {
                        StartExpect::UnknownRequired // (d)
                    }
                } else {
                    StartExpect::UnknownRequired // (d): Δ содержит M₀
                }
            }
        };
    }

    // Первый (выживший) motion-оп программы.
    if frame == Frame::Machine {
        // (c): стартовая позиция станка M₀ внешняя
        return StartExpect::UnknownRequired;
    }
    let sh = set_here[wcs_to_p(wcs) as usize].clone();
    match (sh, preset) {
        // G92 задаёт Δ = M₀ − O₀ − preset ≠ 0 → L20-чтение = delta − Δ содержит M₀
        (Some(_), Some(_)) => StartExpect::UnknownRequired,
        // (a)-якорь L20 (без пресетки): текущая позиция читается как delta
        (Some(delta), None) => StartExpect::Exact(delta),
        (None, Some(pr)) => {
            if pr.wcs == wcs {
                // (a)-якорь G92 в той же системе: reading = preset (golden 36)
                StartExpect::Exact(pr.values.clone())
            } else if known_origin[wcs_to_p(pr.wcs) as usize].is_some() && op_o.is_some() {
                // reading = preset + O₀ − O_wcs (обе системы определены в тексте)
                let o0 = known_origin[wcs_to_p(pr.wcs) as usize].clone().unwrap();
                StartExpect::Tol(p3_build(vadd(&p3_vals(&pr.values), &vsub(&p3_vals(&o0), &p3_vals(&op_o.unwrap())))))
            } else {
                // (b): смена на систему, разность origin'ов которой не вычислима
                StartExpect::UnknownRequired
            }
        }
        (None, None) => StartExpect::UnknownRequired, // (a): без якоря
    }
}

fn check_linking(program: &Program, kept: &[Op], eps_pos: f64, errors: &mut Vec<Diagnostic>) {
    let mut known_origin: [Option<Point3>; 10] = Default::default();
    let mut set_here: [Option<Point3>; 10] = Default::default();
    let mut prev: Option<PrevMotion> = None;
    let mut saw_motion = false;

    for o in kept.iter() {
        match o {
            Op::WcsDefine { p, origin, .. } => {
                if (*p as usize) < known_origin.len() {
                    known_origin[*p as usize] = Some(origin.clone());
                }
            }
            Op::WcsSetHere { p, delta, .. } => {
                if (*p as usize) < set_here.len() {
                    set_here[*p as usize] = Some(delta.clone());
                }
            }
            _ if is_motion(o) => {
                let (start, end, state) = motion_parts(o).expect("motion op");
                let expect = expect_start(
                    !saw_motion,
                    state.frame,
                    state.wcs,
                    program.frame_preset.as_ref(),
                    &set_here,
                    &known_origin,
                    prev.as_ref(),
                );
                match (start, expect) {
                    (StartPoint::Unknown, StartExpect::UnknownRequired) => {}
                    (StartPoint::Unknown, StartExpect::Exact(_) | StartExpect::Tol(_)) => {
                        errors.push(diag(
                            ErrorCode::InvalidInput,
                            "start=unknown outside frame-break points (SPEC §2 v0.9: канонизатор обязан вычислить start = end предыдущего motion-опа)",
                            op_src(o),
                        ));
                    }
                    (StartPoint::Known(_v), StartExpect::UnknownRequired) => {
                        errors.push(diag(
                            ErrorCode::InvalidInput,
                            "start must be unknown here: позиция не вычислима из текста (точка разрыва фрейма, SPEC §2 Q11)",
                            op_src(o),
                        ));
                    }
                    (StartPoint::Known(v), StartExpect::Exact(exp)) => {
                        if !p3_eq(v, &exp) {
                            errors.push(diag(
                                ErrorCode::InvalidInput,
                                format!(
                                    "start {} != expected {} (точное равенство: start = end предыдущего motion-опа / якорь, SPEC §2 v0.9)",
                                    fmt_point(v), fmt_point(&exp)
                                ),
                                op_src(o),
                            ));
                        }
                    }
                    (StartPoint::Known(v), StartExpect::Tol(exp)) => {
                        let d = vdist(&p3_vals(v), &p3_vals(&exp));
                        if d > eps_pos {
                            // ε_pos из EQUIVALENCE §4: max(0.001, 1e-7·L) — тот же
                            // допуск, что и §6-геометрия (аудит #4: раньше был
                            // захардкожен floor 0.001; поведение на L < 10000
                            // идентично — floor активен)
                            errors.push(diag(
                                ErrorCode::InvalidInput,
                                format!(
                                    "start {} != frame-adjusted previous end {} (|Δ| = {d:.6} > {eps_pos:.6}, SPEC §2 v0.9)",
                                    fmt_point(v), fmt_point(&exp)
                                ),
                                op_src(o),
                            ));
                        }
                    }
                }
                saw_motion = true;
                prev = Some(PrevMotion { end: end.clone(), frame: state.frame, wcs: state.wcs });
            }
            _ => {}
        }
    }
}

fn fmt_point(p: &Point3) -> String {
    format!("({} {} {})", p.x.text, p.y.text, p.z.text)
}
