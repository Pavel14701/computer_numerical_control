//! EID/L2 — интерпретатор D0–D2 + минимальный Pass-1 lowering (этап 12.1).
//!
//! УРОВНИ (§4 дока EID-L2-DESIGN v0.1; D3–D5 — фазы 12.2/12.3):
//! * **D0** — прямолинейное исполнение: `Assign`, `Move` (Rapid/Linear),
//!   `L1Op`-passthrough;
//! * **D1** — ветвления: `IfThen` (одиночный then_stmt, без else — Macro B;
//!   cond: ≠0, `IF [#0]` — Null как 0.0 → false); `IfGoto{target_label}` —
//!   только носитель RAW (таблица меток N — 12.2);
//! * **D2** — циклы и подпрограммы: `While` (do_id-парность — на парсере,
//!   лимит шагов — здесь, T5), `CallSub` со стеком кадров #1–#33
//!   (args A→#1..Z→#26) и repeat, `RtcpMode`/`WithTransform` — носители RAW.
//!
//! LOWERING (§3 матрица, МИНИМАЛЬНО для 12.1; полный Pass-1 — фаза 12.2
//! в `crate::lowering`):
//! * константные Rapid/Linear (ни одной переменной, без ротационных осей,
//!   все X/Y/Z присутствуют) → L1-опы [`Op::Rapid`]/[`Op::Linear`]
//!   (start = Unknown, state — дефолт фазы 12.1; спан — src оператора);
//! * всё с переменными / осями A–C / дуги / трансформации → носитель
//!   [`RawTailItem`] (`Op::Raw` + маркер [`RawMarker`]);
//! * G31 / чтение SkipPos/AbsMachinePos (любой Sys-переменной) при
//!   неизвестном состоянии станка → останов `E_RUNTIME_STATE`
//!   ([`L2Error::RuntimeError`], §3 последняя строка матрицы).
//!
//! ГВАРД (T5): бюджет вычислений `L2Config::max_eval_steps` (дефолт 10000)
//! тикается на каждом узле выражения, операторе и итерации цикла —
//! бесконечный WHILE завершается ошибкой `MaxEvalSteps`
//! (класс `E_LIMIT_EXCEEDED`).
//!
//! Примечание (12.2 TODO): носители RAW в 12.1 несут hex debug-дампа
//! оператора; §3/T4 ожидает payload вида `G0 A5.0000\n` (исходный кадр с
//! подставленными значениями) — появится вместе с частичным lowering осей.

use crate::ast::{Expr, ExprPose, L2Program, L2Stmt, MoveKind, SubTarget};
use crate::eval::{eval_expr, write_var, L2Env, L2Error};
use eid_core::ast::{Num, Op, Point3, StartPoint, State};
use eid_core::diag::{Diagnostic, ErrorCode, Stage};

// ===========================================================================
// Конфиг и результат
// ===========================================================================

/// Конфигурация интерпретатора (этап 12.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L2Config {
    /// Бюджет шагов вычисления (узлы выражений + операторы + итерации
    /// циклов). Дефолт 10000 — T5: бесконечный WHILE → E_LIMIT_EXCEEDED.
    pub max_eval_steps: u64,
}

impl Default for L2Config {
    fn default() -> Self {
        Self {
            max_eval_steps: 10_000,
        }
    }
}

/// Маркер носителя RAW: что за L2-конструкция не lowered в Pass-1 (12.1)
/// и зачем. Полный разбор носителей — фаза 12.2 ([`crate::lowering`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawMarker {
    /// Ход зависит от переменных (параметрический).
    Parametric,
    /// Ротационная ось A/B/C в позе (без RTCP-контекста в L1).
    RotaryAxis,
    /// Дуга — геометрия (center/normal/sweep) не считалась в 12.1.
    ArcUnlowered,
    /// Константный ход, но поза неполная (отсутствующее/Null-слово —
    /// «слово игнорируется», координата неизвестна).
    IncompletePose,
    /// Трансформация (§2.3) — тело блока НЕ исполняется (иначе ходы
    /// эмитились бы без трансформации — неверная геометрия).
    Transform,
    /// IF-GOTO: нужна таблица меток N (12.2).
    ControlFlow,
    /// Модальный вызов подпрограммы (G66) — привязан к ходам (12.2).
    SubCall,
    /// RTCP-режим (G43.4/G43.5/G49).
    Rtcp,
}

impl RawMarker {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Parametric => "parametric-move",
            Self::RotaryAxis => "rotary-axis",
            Self::ArcUnlowered => "arc-geometry",
            Self::IncompletePose => "incomplete-pose",
            Self::Transform => "transform",
            Self::ControlFlow => "if-goto",
            Self::SubCall => "modal-subcall",
            Self::Rtcp => "rtcp",
        }
    }
}

/// Носитель не-lowered L2-конструкции: маркер + `Op::Raw` (lossless
/// payload — детерминированный debug-дамп оператора, причина, код
/// `E_RUNTIME_STATE`, спан оператора из AST).
#[derive(Debug, Clone, PartialEq)]
pub struct RawTailItem {
    pub marker: RawMarker,
    /// Всегда `Op::Raw` — носитель.
    pub op: Op,
}

/// Результат Pass-1 (заготовка фазы 12.1; полный lowering — 12.2).
#[derive(Debug, Clone, Default)]
pub struct LoweringResult {
    /// Успешно lowered L1-опы (в т.ч. passthrough `L2Stmt::L1Op`).
    pub l1_ops: Vec<Op>,
    /// Хвост носителей RAW (маркер + Op::Raw) — порядок исполнения.
    pub raw_tail: Vec<RawTailItem>,
    /// Диагностики пасса (Stage::Canonicalize; E_RUNTIME_STATE-носители,
    /// остановы уже отражены в Err-возврате `run`).
    pub diagnostics: Vec<Diagnostic>,
}

impl LoweringResult {
    fn push_raw(&mut self, marker: RawMarker, stmt: &L2Stmt, detail: &str) {
        let reason = format!("L2 pass-1 [{}]: {detail}", marker.as_str());
        self.raw_tail.push(RawTailItem {
            marker,
            op: raw_carrier_op(&reason, stmt),
        });
        self.diagnostics.push(Diagnostic {
            code: ErrorCode::RuntimeState,
            stage: Stage::Canonicalize,
            message: reason,
            // Спан оператора из AST (§2.4: src у всех управляющих вариантов).
            src: stmt.src().cloned(),
            options: Vec::new(),
            limit: None,
        });
    }
}

/// Op::Raw-носитель: hex детерминированного debug-дампа оператора,
/// спан — из оператора (плейсхолдер для без-spanных — L1Op сюда не
/// попадает; fallback сохранён для устойчивости).
fn raw_carrier_op(reason: &str, stmt: &L2Stmt) -> Op {
    let dump = format!("{stmt:?}");
    let payload_hex: String = dump.as_bytes().iter().map(|b| format!("{b:02X}")).collect();
    Op::Raw {
        payload_hex,
        reason: reason.to_string(),
        code: ErrorCode::RuntimeState,
        src: stmt
            .src()
            .cloned()
            .unwrap_or_else(crate::ast::placeholder_span),
    }
}

/// Состояние по умолчанию для синтезированных L1-опов (фаза 12.1 не
/// ведёт модель станка: spindle/coolant/tool — как в грамматике D3 до
/// первого слова; S-слово (spindle Expr) в 12.1 только влияет на
/// константность — направление Cw/Ccw требует M3/M4-контекста, 12.2).
fn l1_default_state(feed: Option<Num>) -> State {
    State {
        feed,
        spindle: eid_core::ast::Spindle::Off,
        coolant: eid_core::ast::Coolant::default(),
        tool: None,
        frame: eid_core::ast::Frame::Wcs,
        wcs: eid_core::ast::Wcs::G54,
        feed_restated: false,
    }
}

/// f64 → Num с детерминированным текстом (кратчайший Display; verbatim
/// текст для L2-ходов недоступен — в 12.1 не критично).
fn mk_num(v: f64) -> Num {
    Num::new(format!("{v}"), v)
}

// ===========================================================================
// pose_resolve: ExprPose → ResolvedPose (Null-компонента → None = слово
// игнорируется; ошибки выражений, включая Sys-чтение → E_RUNTIME_STATE,
// пробрасываются и ОСТАНАВЛИВАЮТ исполнение)
// ===========================================================================

/// Поза с разрешёнными значениями слов: None — слова нет или оно Null
/// (игнорируется), Some(v) — конкретное значение.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedPose {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
    pub a: Option<f64>,
    pub b: Option<f64>,
    pub c: Option<f64>,
}

/// Вычислить все слова позы (Приложение, таблица 2.1: слово с Null
/// игнорируется — «как будто не написано»).
pub fn pose_resolve(pose: &ExprPose, env: &mut L2Env) -> Result<ResolvedPose, L2Error> {
    let mut word = |e: &Option<Expr>| -> Result<Option<f64>, L2Error> {
        match e {
            None => Ok(None),
            Some(expr) => Ok(eval_expr(expr, env)?.resolve_word()),
        }
    };
    Ok(ResolvedPose {
        x: word(&pose.x)?,
        y: word(&pose.y)?,
        z: word(&pose.z)?,
        a: word(&pose.a)?,
        b: word(&pose.b)?,
        c: word(&pose.c)?,
    })
}

/// Выражение константно (нет Var-ссылок)? Const-арифметика/Null — да.
fn expr_is_const(e: &Expr) -> bool {
    match e {
        Expr::Const(_) | Expr::Null => true,
        Expr::Var(_) => false,
        Expr::Unary(_, inner) => expr_is_const(inner),
        Expr::Binary(_, a, b) => expr_is_const(a) && expr_is_const(b),
    }
}

// ===========================================================================
// L2Evaluator
// ===========================================================================

/// Интерпретатор L2, уровни D0–D2 (см. модульную шапку).
#[derive(Debug, Clone)]
pub struct L2Evaluator {
    config: L2Config,
    env: L2Env,
}

impl L2Evaluator {
    /// Новый интерпретатор с конфигом (бюджет навязывается окружению).
    pub fn new(config: L2Config) -> Self {
        let mut env = L2Env::new();
        env.set_budget(config.max_eval_steps);
        Self { config, env }
    }

    /// Окружение (переменные после исполнения — для тестов/диагностики).
    pub fn env(&self) -> &L2Env {
        &self.env
    }

    /// Конфиг.
    pub fn config(&self) -> &L2Config {
        &self.config
    }

    /// Исполнить программу (D0–D2) и собрать Pass-1. Любая `L2Error`
    /// (например Sys-чтение при неизвестном станке → E_RUNTIME_STATE,
    /// исчерпание бюджета → E_LIMIT_EXCEEDED) ОСТАНАВЛИВАЕТ исполнение.
    pub fn run(&mut self, program: &L2Program) -> Result<LoweringResult, L2Error> {
        self.env.set_budget(self.config.max_eval_steps);
        let mut out = LoweringResult::default();
        self.exec_block(&program.stmts, program, &mut out)?;
        Ok(out)
    }

    /// Блок операторов (последовательный; Err — останов).
    fn exec_block(
        &mut self,
        stmts: &[L2Stmt],
        program: &L2Program,
        out: &mut LoweringResult,
    ) -> Result<(), L2Error> {
        for stmt in stmts {
            self.exec_stmt(stmt, program, out)?;
        }
        Ok(())
    }

    /// Один оператор.
    fn exec_stmt(
        &mut self,
        stmt: &L2Stmt,
        program: &L2Program,
        out: &mut LoweringResult,
    ) -> Result<(), L2Error> {
        self.env.tick()?;
        match stmt {
            // ---------- D0 ----------
            L2Stmt::Assign { target, value, .. } => {
                let v = eval_expr(value, &mut self.env)?;
                // Присваивание копирует Null (таблица 2.1: `#100 = #0` —
                // переменная становится Val::Null, НЕ 0.0).
                write_var(target, v, &mut self.env)?;
                Ok(())
            }
            L2Stmt::Move { .. } => self.exec_move(stmt, program, out),
            L2Stmt::L1Op(op) => {
                out.l1_ops.push(op.clone());
                Ok(())
            }

            // ---------- D1 ----------
            L2Stmt::IfThen {
                cond, then_stmt, ..
            } => {
                // cond: ≠0 → true; IF [#0] — Null как 0.0 → false.
                let c = eval_expr(cond, &mut self.env)?;
                if c.truthy() {
                    self.exec_stmt(then_stmt, program, out)?;
                }
                Ok(())
            }
            L2Stmt::IfGoto { .. } => {
                // D1-минимум: переход требует таблицы меток N (12.2).
                out.push_raw(
                    RawMarker::ControlFlow,
                    stmt,
                    "label table not built in phase 12.1",
                );
                Ok(())
            }

            // ---------- D2 ----------
            L2Stmt::While { cond, body, .. } => {
                // Итерация тикается дважды: на операторе и на cond —
                // бесконечный цикл гарантированно исчерпывает бюджет (T5).
                loop {
                    self.env.tick()?;
                    let c = eval_expr(cond, &mut self.env)?;
                    if !c.truthy() {
                        break;
                    }
                    self.exec_block(body, program, out)?;
                }
                Ok(())
            }
            L2Stmt::CallSub { .. } => self.exec_call_sub(stmt, program, out),

            // ---------- носители RAW (12.2) ----------
            L2Stmt::WithTransform { .. } => {
                // Тело НЕ исполняется: без применения трансформации ходы
                // дали бы неверную геометрию (см. RawMarker::Transform).
                out.push_raw(
                    RawMarker::Transform,
                    stmt,
                    "body suppressed until transform application (12.2)",
                );
                Ok(())
            }
            L2Stmt::RtcpMode { .. } => {
                out.push_raw(
                    RawMarker::Rtcp,
                    stmt,
                    "RTCP lowering deferred to phase 12.2",
                );
                Ok(())
            }
        }
    }

    /// Move: разрешение позы и минимальный lowering (§3 матрица 12.1).
    /// Принимает оператор ЦЕЛИКОМ — носитель RAW обязан быть lossless.
    fn exec_move(
        &mut self,
        stmt: &L2Stmt,
        _program: &L2Program,
        out: &mut LoweringResult,
    ) -> Result<(), L2Error> {
        let (kind, target, feed, spindle) = match stmt {
            L2Stmt::Move {
                kind,
                target,
                feed,
                spindle,
                ..
            } => (kind, target, feed, spindle),
            _ => unreachable!("exec_move called on non-Move"),
        };
        // 1) Разрешение слов (ошибки выражений и Sys-чтения — останов).
        //    Даже для RAW-носителя: G31/SkipPos при неизвестном — останов
        //    E_RUNTIME_STATE, а не молчаливый RAW (§3 последняя строка).
        let rp = pose_resolve(target, &mut self.env)?;

        // 2) Константность statement'а (поза + feed + spindle + параметры дуги).
        let target_const = pose_exprs(target).iter().all(|e| expr_is_const(e));
        let feed_const = feed.as_ref().map(expr_is_const).unwrap_or(true);
        let spindle_const = spindle.as_ref().map(expr_is_const).unwrap_or(true);
        let arc_const = match kind {
            MoveKind::Arc { ijk, r, .. } => {
                ijk.iter()
                    .all(|e| e.as_ref().map(expr_is_const).unwrap_or(true))
                    && r.as_ref().map(expr_is_const).unwrap_or(true)
            }
            _ => true,
        };

        // 3) Маршрут lowering (§3).
        match kind {
            MoveKind::Rapid | MoveKind::Linear => {
                if !target_const || !feed_const || !spindle_const {
                    out.push_raw(RawMarker::Parametric, stmt, "move depends on variables");
                    return Ok(());
                }
                if target.has_rotary() {
                    out.push_raw(RawMarker::RotaryAxis, stmt, "rotary axis A/B/C present");
                    return Ok(());
                }
                match (rp.x, rp.y, rp.z) {
                    (Some(x), Some(y), Some(z)) => {
                        // 4) Константный ход → L1-оп (start Unknown — модели
                        //    станка в D0–D2 нет; связывание — канонизатор;
                        //    спан — src оператора).
                        let feed_num = match feed {
                            Some(e) => {
                                let v = eval_expr(e, &mut self.env)?;
                                v.resolve_word().map(mk_num)
                            }
                            None => None,
                        };
                        let end = Point3 {
                            x: mk_num(x),
                            y: mk_num(y),
                            z: mk_num(z),
                        };
                        let op = match kind {
                            MoveKind::Rapid => Op::Rapid {
                                start: StartPoint::Unknown,
                                end,
                                state: l1_default_state(feed_num),
                                src: stmt
                                    .src()
                                    .cloned()
                                    .unwrap_or_else(crate::ast::placeholder_span),
                                sub: None,
                            },
                            _ => Op::Linear {
                                start: StartPoint::Unknown,
                                end,
                                state: l1_default_state(feed_num),
                                src: stmt
                                    .src()
                                    .cloned()
                                    .unwrap_or_else(crate::ast::placeholder_span),
                                sub: None,
                            },
                        };
                        out.l1_ops.push(op);
                        Ok(())
                    }
                    _ => {
                        // Слово отсутствует или Null → игнорируется,
                        // координата неизвестна → носитель (таблица 2.1).
                        out.push_raw(
                            RawMarker::IncompletePose,
                            stmt,
                            "X/Y/Z word missing or Null (word ignored)",
                        );
                        Ok(())
                    }
                }
            }
            MoveKind::Arc { .. } => {
                out.push_raw(
                    RawMarker::ArcUnlowered,
                    stmt,
                    if arc_const {
                        "arc geometry (center/normal/sweep) deferred to 12.2"
                    } else {
                        "parametric arc deferred to 12.2"
                    },
                );
                Ok(())
            }
        }
    }

    /// CallSub: стек кадров #1–#33, маппинг args A→#1..Z→#26, repeat.
    fn exec_call_sub(
        &mut self,
        stmt: &L2Stmt,
        program: &L2Program,
        out: &mut LoweringResult,
    ) -> Result<(), L2Error> {
        let (target, repeat, args, modal) = match stmt {
            L2Stmt::CallSub {
                target,
                repeat,
                args,
                modal,
                ..
            } => (target, repeat, args, modal),
            _ => unreachable!("exec_call_sub called on non-CallSub"),
        };
        // G66 (модальный вызов) исполняется на каждом ходе — вне D0–D2.
        if *modal {
            out.push_raw(
                RawMarker::SubCall,
                stmt,
                "modal call (G66) binding to moves deferred to 12.2",
            );
            return Ok(());
        }
        let sub = program.find_sub(target).ok_or_else(|| {
            L2Error::RuntimeError(format!(
                "subprogram target not found: {}",
                match target {
                    SubTarget::ONumber(n) => format!("O{n}"),
                    SubTarget::NLabel(n) => format!("N{n}"),
                }
            ))
        })?;
        // Число повторений (L/K-слово; Null/отсутствует → 1).
        let times = match repeat {
            Some(e) => {
                let v = eval_expr(e, &mut self.env)?;
                let n = v.as_num();
                if n < 0.0 {
                    return Err(L2Error::RuntimeError(format!(
                        "negative CALL repeat count: {n}"
                    )));
                }
                n.trunc() as u64
            }
            None => 1,
        };
        // Аргументы вычисляются в КОНТЕКСТЕ ВЫЗЫВАЮЩЕГО (в т.ч. Null —
        // таблица 2.1: опциональный аргумент пробрасывается как Null),
        // затем кадр пушится и значения пишутся в локальные #1..#26 нового
        // кадра.
        let mut bound = Vec::with_capacity(args.len());
        for (letter, value) in args {
            let idx = crate::ast::call_arg_local_index(*letter).ok_or_else(|| {
                L2Error::RuntimeError(format!(
                    "CALL argument letter must be ASCII A..Z, got {letter:#x}"
                ))
            })?;
            let v = eval_expr(value, &mut self.env)?;
            bound.push((idx, v));
        }
        for _ in 0..times {
            self.env.push_frame();
            for (idx, v) in &bound {
                self.env.write_direct(*idx, *v)?;
            }
            let res = self.exec_block(&sub.body, program, out);
            self.env.pop_frame()?;
            res?;
        }
        Ok(())
    }
}

/// Все выражения позы (для проверки константности).
fn pose_exprs(pose: &ExprPose) -> Vec<&Expr> {
    [&pose.x, &pose.y, &pose.z, &pose.a, &pose.b, &pose.c]
        .into_iter()
        .flatten()
        .collect()
}

// ===========================================================================
// Тесты: T1-заготовка, D0–D2-матрица, T5, пины Приложения
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        placeholder_span, Axis, Plane, SubTarget, SysVar, TransformKind, UnaryOp, VarRef,
    };
    use eid_core::ast::Dir;

    fn cfg() -> L2Config {
        L2Config::default()
    }

    fn ev() -> L2Evaluator {
        L2Evaluator::new(cfg())
    }

    fn target_const(x: f64, y: f64, z: f64) -> ExprPose {
        ExprPose {
            x: Some(Expr::num(x)),
            y: Some(Expr::num(y)),
            z: Some(Expr::num(z)),
            a: None,
            b: None,
            c: None,
        }
    }

    // ---------- T1-заготовка: L1-программа проходит конвейер ----------

    #[test]
    fn t1_l1_only_program_passes_through() {
        // L2-программа из чистых L1-опов: замер структуры — опы проходят
        // 1:1, хвост RAW пуст, диагностик нет.
        let l1 = vec![
            Op::Rapid {
                start: StartPoint::Unknown,
                end: Point3 {
                    x: Num::new("0".into(), 0.0),
                    y: Num::new("0".into(), 0.0),
                    z: Num::new("5".into(), 5.0),
                },
                state: l1_default_state(None),
                src: placeholder_span(),
                sub: None,
            },
            Op::Linear {
                start: StartPoint::Unknown,
                end: Point3 {
                    x: Num::new("10".into(), 10.0),
                    y: Num::new("0".into(), 0.0),
                    z: Num::new("5".into(), 5.0),
                },
                state: l1_default_state(Some(Num::new("100".into(), 100.0))),
                src: placeholder_span(),
                sub: None,
            },
            Op::ProgramEnd {
                src: placeholder_span(),
            },
        ];
        let program = L2Program {
            stmts: l1.iter().cloned().map(L2Stmt::L1Op).collect(),
            subs: Vec::new(),
        };
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 3);
        assert_eq!(res.l1_ops, l1); // passthrough 1:1 (структура)
        assert!(res.raw_tail.is_empty());
        assert!(res.diagnostics.is_empty());
    }

    // ---------- D0: Assign / Move ----------

    #[test]
    fn d0_assign_and_null_copy() {
        // #100 = 5; #101 = #0 (Null копируется); #102 = -#100.
        let program = L2Program {
            stmts: vec![
                L2Stmt::Assign {
                    target: VarRef::Direct(100),
                    value: Expr::num(5.0),
                    src: placeholder_span(),
                },
                L2Stmt::Assign {
                    target: VarRef::Direct(101),
                    value: Expr::Null,
                    src: placeholder_span(),
                },
                L2Stmt::Assign {
                    target: VarRef::Direct(102),
                    value: Expr::Unary(UnaryOp::Neg, Box::new(Expr::var(100))),
                    src: placeholder_span(),
                },
            ],
            subs: Vec::new(),
        };
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(100).unwrap(), crate::ast::Val::Num(5.0));
        // ПИН (таблица 2.1, строка 1): присваивание копирует именно Null.
        assert_eq!(e.env().read_direct(101).unwrap(), crate::ast::Val::Null);
        assert_eq!(
            e.env().read_direct(102).unwrap(),
            crate::ast::Val::Num(-5.0)
        );
    }

    #[test]
    fn d0_const_rapid_linear_to_l1() {
        let program = L2Program {
            stmts: vec![
                L2Stmt::Move {
                    kind: MoveKind::Rapid,
                    target: target_const(1.0, 2.0, 3.0),
                    feed: None,
                    spindle: None,
                    src: placeholder_span(),
                },
                L2Stmt::Move {
                    kind: MoveKind::Linear,
                    target: target_const(4.0, 5.0, 6.0),
                    feed: Some(Expr::num(120.0)),
                    spindle: None,
                    src: placeholder_span(),
                },
            ],
            subs: Vec::new(),
        };
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 2);
        assert!(res.raw_tail.is_empty());
        match (&res.l1_ops[0], &res.l1_ops[1]) {
            (
                Op::Rapid { end, .. },
                Op::Linear {
                    end: end2, state, ..
                },
            ) => {
                assert_eq!(
                    (end.x.value(), end.y.value(), end.z.value()),
                    (1.0, 2.0, 3.0)
                );
                assert_eq!(
                    (end2.x.value(), end2.y.value(), end2.z.value()),
                    (4.0, 5.0, 6.0)
                );
                assert_eq!(state.feed.as_ref().map(|f| f.value()), Some(120.0));
            }
            _ => panic!("expected Rapid+Linear"),
        }
    }

    #[test]
    fn d0_parametric_move_to_raw() {
        // X от переменной → носитель Parametric (поза не lowered).
        let program = L2Program {
            stmts: vec![L2Stmt::Move {
                kind: MoveKind::Linear,
                target: ExprPose {
                    x: Some(Expr::var(100)),
                    y: Some(Expr::num(0.0)),
                    z: Some(Expr::num(5.0)),
                    a: None,
                    b: None,
                    c: None,
                },
                feed: None,
                spindle: None,
                src: placeholder_span(),
            }],
            subs: Vec::new(),
        };
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert!(res.l1_ops.is_empty());
        assert_eq!(res.raw_tail.len(), 1);
        assert_eq!(res.raw_tail[0].marker, RawMarker::Parametric);
        assert!(matches!(res.raw_tail[0].op, Op::Raw { .. }));
        assert_eq!(res.diagnostics.len(), 1);
        // Спан носителя — спан оператора из AST (§2.4).
        assert!(res.diagnostics[0].src.is_some());
    }

    #[test]
    fn d0_spindle_parametric_to_raw_and_const_ignored_in_state() {
        // §2.4 Move.spindle: параметрический S → RAW; константный S не
        // ломает lowering (в State не пишется — направление Cw/Ccw
        // требует M3/M4-контекста, 12.2).
        let mk = |spindle: Option<Expr>| L2Stmt::Move {
            kind: MoveKind::Rapid,
            target: target_const(1.0, 2.0, 3.0),
            feed: None,
            spindle,
            src: placeholder_span(),
        };
        let program = L2Program {
            stmts: vec![mk(Some(Expr::var(500)))],
            subs: Vec::new(),
        };
        let res = ev().run(&program).unwrap();
        assert!(res.l1_ops.is_empty());
        assert_eq!(res.raw_tail[0].marker, RawMarker::Parametric);

        let program = L2Program {
            stmts: vec![mk(Some(Expr::num(1200.0)))],
            subs: Vec::new(),
        };
        let res = ev().run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 1);
        assert!(res.raw_tail.is_empty());
        if let Op::Rapid { state, .. } = &res.l1_ops[0] {
            assert_eq!(state.spindle, eid_core::ast::Spindle::Off);
        } else {
            panic!("expected Rapid");
        }
    }

    #[test]
    fn d0_rotary_axis_to_raw() {
        // Константная A-ось — всё равно RAW (нет RTCP-контекста; §3
        // частичный lowering осей — 12.2/T4).
        let program = L2Program {
            stmts: vec![L2Stmt::Move {
                kind: MoveKind::Rapid,
                target: ExprPose {
                    x: Some(Expr::num(1.0)),
                    y: Some(Expr::num(2.0)),
                    z: Some(Expr::num(3.0)),
                    a: Some(Expr::num(90.0)),
                    b: None,
                    c: None,
                },
                feed: None,
                spindle: None,
                src: placeholder_span(),
            }],
            subs: Vec::new(),
        };
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert!(res.l1_ops.is_empty());
        assert_eq!(res.raw_tail[0].marker, RawMarker::RotaryAxis);
    }

    #[test]
    fn d0_null_word_ignored_incomplete_pose() {
        // Y = Null → слово игнорируется (таблица 2.1) → поза неполная →
        // носитель (координата неизвестна).
        let program = L2Program {
            stmts: vec![L2Stmt::Move {
                kind: MoveKind::Rapid,
                target: ExprPose {
                    x: Some(Expr::num(1.0)),
                    y: Some(Expr::Null),
                    z: Some(Expr::num(3.0)),
                    a: None,
                    b: None,
                    c: None,
                },
                feed: None,
                spindle: None,
                src: placeholder_span(),
            }],
            subs: Vec::new(),
        };
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert!(res.l1_ops.is_empty());
        assert_eq!(res.raw_tail[0].marker, RawMarker::IncompletePose);
    }

    #[test]
    fn d0_arc_to_raw() {
        let program = L2Program {
            stmts: vec![L2Stmt::Move {
                kind: MoveKind::Arc {
                    ijk: [Some(Expr::num(5.0)), Some(Expr::num(0.0)), None],
                    r: None,
                    dir: Dir::Cw,
                },
                target: target_const(10.0, 0.0, 0.0),
                feed: Some(Expr::num(50.0)),
                spindle: None,
                src: placeholder_span(),
            }],
            subs: Vec::new(),
        };
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert_eq!(res.raw_tail[0].marker, RawMarker::ArcUnlowered);
    }

    // ---------- D1: IfThen (одиночный then_stmt), условия с Null ----------

    #[test]
    fn d1_if_then_branches() {
        // IF 1 THEN ... — исполняется; IF 0 / IF #0 (Null-cond как 0.0 →
        // false) — не исполняется. (then_stmt одиночный; else в Macro B нет.)
        let assign = |idx: u32| -> Box<L2Stmt> {
            Box::new(L2Stmt::Assign {
                target: VarRef::Direct(idx),
                value: Expr::num(1.0),
                src: placeholder_span(),
            })
        };
        let program = L2Program {
            stmts: vec![
                L2Stmt::IfThen {
                    cond: Expr::num(1.0),
                    then_stmt: assign(100),
                    src: placeholder_span(),
                },
                L2Stmt::IfThen {
                    cond: Expr::num(0.0),
                    then_stmt: assign(101),
                    src: placeholder_span(),
                },
                L2Stmt::IfThen {
                    cond: Expr::Null, // IF [#0] — false
                    then_stmt: assign(102),
                    src: placeholder_span(),
                },
            ],
            subs: Vec::new(),
        };
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(100).unwrap(), crate::ast::Val::Num(1.0));
        assert_eq!(e.env().read_direct(101).unwrap(), crate::ast::Val::Null);
        assert_eq!(e.env().read_direct(102).unwrap(), crate::ast::Val::Null);
    }

    #[test]
    fn d1_if_goto_is_raw_carrier() {
        let program = L2Program {
            stmts: vec![L2Stmt::IfGoto {
                cond: Expr::num(1.0),
                target_label: 10,
                src: placeholder_span(),
            }],
            subs: Vec::new(),
        };
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert_eq!(res.raw_tail[0].marker, RawMarker::ControlFlow);
    }

    // ---------- D2: While (T5) и CallSub ----------

    #[test]
    fn d2_bounded_while_executes() {
        // #1 = 3; WHILE [#1 GT 0] DO1 #1 = #1 - 1 END1 → #1 == 0.
        let program = L2Program {
            stmts: vec![
                L2Stmt::Assign {
                    target: VarRef::Direct(1),
                    value: Expr::num(3.0),
                    src: placeholder_span(),
                },
                L2Stmt::While {
                    cond: Expr::Binary(
                        crate::ast::BinaryOp::Gt,
                        Box::new(Expr::var(1)),
                        Box::new(Expr::num(0.0)),
                    ),
                    do_id: 1,
                    body: vec![L2Stmt::Assign {
                        target: VarRef::Direct(1),
                        value: Expr::Binary(
                            crate::ast::BinaryOp::Sub,
                            Box::new(Expr::var(1)),
                            Box::new(Expr::num(1.0)),
                        ),
                        src: placeholder_span(),
                    }],
                    src: placeholder_span(),
                },
            ],
            subs: Vec::new(),
        };
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(1).unwrap(), crate::ast::Val::Num(0.0));
    }

    #[test]
    fn t5_infinite_while_limit_exceeded() {
        // #100 = 1; WHILE [#100 GT 0] DO1 END1 — бесконечный цикл
        // (чек-лист T5: WHILE [1 EQ 1] DO 1 → E_LIMIT_EXCEEDED).
        let program = L2Program {
            stmts: vec![
                L2Stmt::Assign {
                    target: VarRef::Direct(100),
                    value: Expr::num(1.0),
                    src: placeholder_span(),
                },
                L2Stmt::While {
                    cond: Expr::Binary(
                        crate::ast::BinaryOp::Gt,
                        Box::new(Expr::var(100)),
                        Box::new(Expr::num(0.0)),
                    ),
                    do_id: 1,
                    body: vec![],
                    src: placeholder_span(),
                },
            ],
            subs: Vec::new(),
        };
        let mut e = ev(); // дефолт 10000
        let err = e.run(&program).unwrap_err();
        assert!(matches!(err, L2Error::MaxEvalSteps { limit: 10_000 }));
        // Класс E_LIMIT_EXCEEDED (чек-лист T5).
        assert_eq!(err.error_code(), ErrorCode::LimitExceeded);
        // Малый конфигурационный бюджет тоже работает.
        let mut e2 = L2Evaluator::new(L2Config { max_eval_steps: 50 });
        assert!(matches!(
            e2.run(&program),
            Err(L2Error::MaxEvalSteps { limit: 50 })
        ));
    }

    #[test]
    fn d2_call_sub_args_and_frames() {
        // Main: G65 P100 A5. B7. → sub O100: #100 = #1 + #2 (локальные!).
        let program = L2Program {
            stmts: vec![L2Stmt::CallSub {
                target: SubTarget::ONumber(100),
                repeat: None,
                args: vec![(b'A', Expr::num(5.0)), (b'b', Expr::num(7.0))], // lowercase → #2
                modal: false,
                src: placeholder_span(),
            }],
            subs: vec![crate::ast::L2Sub {
                target: SubTarget::ONumber(100),
                body: vec![
                    L2Stmt::Assign {
                        target: VarRef::Direct(100),
                        value: Expr::Binary(
                            crate::ast::BinaryOp::Add,
                            Box::new(Expr::var(1)),
                            Box::new(Expr::var(2)),
                        ),
                        src: placeholder_span(),
                    },
                    // Запись в локальную подпрограммы — свой кадр.
                    L2Stmt::Assign {
                        target: VarRef::Direct(1),
                        value: Expr::num(0.0),
                        src: placeholder_span(),
                    },
                ],
            }],
        };
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(
            e.env().read_direct(100).unwrap(),
            crate::ast::Val::Num(12.0)
        );
        // Стек вернулся к корню; локальная #1 главного блока не тронута.
        assert_eq!(e.env().frame_depth(), 1);
        assert_eq!(e.env().read_direct(1).unwrap(), crate::ast::Val::Null);
    }

    #[test]
    fn d2_pin_call_sub_null_arg_passes_through() {
        // ПИН (таблица 2.1, строка 4): «G65 P1000 X#0 — внутри макроса #24
        // получает Val::Null» (буква X → #24). Контраст: X0.0 → Num(0.0).
        let mk = |x_val: Expr| L2Program {
            stmts: vec![L2Stmt::CallSub {
                target: SubTarget::ONumber(100),
                repeat: None,
                args: vec![(b'X', x_val)],
                modal: false,
                src: placeholder_span(),
            }],
            subs: vec![crate::ast::L2Sub {
                target: SubTarget::ONumber(100),
                body: vec![L2Stmt::Assign {
                    target: VarRef::Direct(500),
                    value: Expr::var(24),
                    src: placeholder_span(),
                }],
            }],
        };
        let mut e = ev();
        e.run(&mk(Expr::Null)).unwrap();
        assert_eq!(e.env().read_direct(500).unwrap(), crate::ast::Val::Null);
        let mut e2 = ev();
        e2.run(&mk(Expr::num(0.0))).unwrap();
        assert_eq!(
            e2.env().read_direct(500).unwrap(),
            crate::ast::Val::Num(0.0)
        );
    }

    #[test]
    fn d2_call_sub_repeat_and_unknown_target() {
        // repeat = L2: подпрограмма O200 дважды инкрементит глобальную #500.
        let mk_call = |r: Option<Expr>| L2Stmt::CallSub {
            target: SubTarget::ONumber(200),
            repeat: r,
            args: vec![],
            modal: false,
            src: placeholder_span(),
        };
        let program = L2Program {
            stmts: vec![mk_call(Some(Expr::num(2.0)))],
            subs: vec![crate::ast::L2Sub {
                target: SubTarget::ONumber(200),
                body: vec![L2Stmt::Assign {
                    target: VarRef::Direct(500),
                    value: Expr::Binary(
                        crate::ast::BinaryOp::Add,
                        Box::new(Expr::var(500)),
                        Box::new(Expr::num(1.0)),
                    ),
                    src: placeholder_span(),
                }],
            }],
        };
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(500).unwrap(), crate::ast::Val::Num(2.0));
        // Неизвестная цель → RuntimeError (E_RUNTIME_STATE класс).
        let bad = L2Program {
            stmts: vec![mk_call(None)],
            subs: vec![],
        };
        let err = ev().run(&bad).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::RuntimeState);
    }

    #[test]
    fn d2_modal_call_is_raw() {
        let program = L2Program {
            stmts: vec![L2Stmt::CallSub {
                target: SubTarget::ONumber(300),
                repeat: None,
                args: vec![],
                modal: true,
                src: placeholder_span(),
            }],
            subs: vec![crate::ast::L2Sub {
                target: SubTarget::ONumber(300),
                body: vec![],
            }],
        };
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert_eq!(res.raw_tail[0].marker, RawMarker::SubCall);
        // Модальный вызов НЕ исполнялся.
        assert!(res.l1_ops.is_empty());
    }

    // ---------- §3 последняя строка: Sys-чтение → E_RUNTIME_STATE останов ----

    #[test]
    fn g31_sysvar_read_stops_with_runtime_state() {
        // Ход с X = #SKIP[X] — останов E_RUNTIME_STATE.
        let mk = |v: Expr| L2Program {
            stmts: vec![L2Stmt::Move {
                kind: MoveKind::Linear,
                target: ExprPose {
                    x: Some(v),
                    y: Some(Expr::num(0.0)),
                    z: Some(Expr::num(0.0)),
                    a: None,
                    b: None,
                    c: None,
                },
                feed: Some(Expr::num(10.0)),
                spindle: None,
                src: placeholder_span(),
            }],
            subs: Vec::new(),
        };
        let skip = mk(Expr::Var(VarRef::Sys(SysVar::SkipPos(Axis::X))));
        let err = ev().run(&skip).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::RuntimeState);

        let abs = mk(Expr::Var(VarRef::Sys(SysVar::AbsMachinePos(Axis::Z))));
        assert_eq!(
            ev().run(&abs).unwrap_err().error_code(),
            ErrorCode::RuntimeState
        );
        // Доходить до lowering не должно: останов до l1_ops/raw_tail.
    }

    // ---------- Трансформации / RTCP ----------

    #[test]
    fn transform_and_rtcp_are_raw_carriers() {
        let program = L2Program {
            stmts: vec![
                L2Stmt::RtcpMode {
                    enabled: true,
                    tool_length_h: Some(1),
                    src: placeholder_span(),
                },
                L2Stmt::WithTransform {
                    // §2.3 дословно: Rotate2D { center: [Expr; 2],
                    // angle_deg: Expr, plane: Plane }.
                    transform: TransformKind::Rotate2D {
                        center: [Expr::num(0.0), Expr::num(0.0)],
                        angle_deg: Expr::num(45.0),
                        plane: Plane::XY,
                    },
                    body: vec![L2Stmt::Move {
                        kind: MoveKind::Linear,
                        target: target_const(10.0, 0.0, 0.0),
                        feed: None,
                        spindle: None,
                        src: placeholder_span(),
                    }],
                    src: placeholder_span(),
                },
            ],
            subs: Vec::new(),
        };
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert_eq!(res.raw_tail.len(), 2);
        assert_eq!(res.raw_tail[0].marker, RawMarker::Rtcp);
        assert_eq!(res.raw_tail[1].marker, RawMarker::Transform);
        // Тело трансформации подавлено: ни одного L1-опа.
        assert!(res.l1_ops.is_empty());
    }

    // ---------- Инфраструктура ----------

    #[test]
    fn raw_carrier_payload_is_deterministic_hex() {
        let stmt = L2Stmt::IfGoto {
            cond: Expr::num(1.0),
            target_label: 5,
            src: placeholder_span(),
        };
        let a = raw_carrier_op("r", &stmt);
        let b = raw_carrier_op("r", &stmt);
        assert_eq!(a, b); // детерминизм
        if let Op::Raw { payload_hex, .. } = &a {
            assert!(!payload_hex.is_empty());
            assert!(payload_hex.chars().all(|c| c.is_ascii_hexdigit()));
        } else {
            panic!("carrier must be Op::Raw");
        }
    }

    #[test]
    fn l2_stmt_src_accessor() {
        let mv = L2Stmt::Move {
            kind: MoveKind::Rapid,
            target: ExprPose::empty(),
            feed: None,
            spindle: None,
            src: placeholder_span(),
        };
        assert!(mv.src().is_some());
        assert!(L2Stmt::L1Op(Op::ProgramEnd {
            src: placeholder_span()
        })
        .src()
        .is_none());
    }

    #[test]
    fn call_arg_local_index_mapping() {
        // A→#1 .. Z→#26 (§2.4, G65), регистронезависимо; не-буква → None.
        assert_eq!(crate::ast::call_arg_local_index(b'A'), Some(1));
        assert_eq!(crate::ast::call_arg_local_index(b'X'), Some(24));
        assert_eq!(crate::ast::call_arg_local_index(b'Z'), Some(26));
        assert_eq!(crate::ast::call_arg_local_index(b'a'), Some(1));
        assert_eq!(crate::ast::call_arg_local_index(b'1'), None);
    }

    #[test]
    fn expr_is_const_matrix() {
        assert!(expr_is_const(&Expr::num(1.0)));
        assert!(expr_is_const(&Expr::Null));
        assert!(expr_is_const(&Expr::Binary(
            crate::ast::BinaryOp::Add,
            Box::new(Expr::num(1.0)),
            Box::new(Expr::Null)
        )));
        assert!(!expr_is_const(&Expr::var(100)));
        assert!(!expr_is_const(&Expr::Unary(
            UnaryOp::Abs,
            Box::new(Expr::var(1))
        )));
    }
}
