//! trace-equivalence микросьюит (DoD 12.2, агент A): развёртка макросов
//! через L2Evaluator → L1-опы → плоский RS274 → прогон через РЕАЛЬНЫЙ
//! интерпретатор rs274 (LinuxCNC, docker eid-linuxcnc-sim) против ТОГО ЖЕ
//! макроса, исполненного нативно (LinuxCNC O-word/Macro-B-подобный синтаксис).
//! Канон-потоки сравниваются по motion-подмножеству с допуском
//! EQUIVALENCE §4 (eps_abs-класс 4dp-квантования, 1e-3).
//!
//! СЮИТ (golden-векторы corpus/tier_b/local + синтетика):
//! 1. `factorial_accum` — рекурсия G65 (глубина 5 ≤ 9, M99, IF-GOTO-метки)
//!    против o-word-рекурсии LinuxCNC;
//! 2. `flowsnake` — corpus/tier_b/local/lcnc_nc_files_flowsnake.ngc
//!    (рекурсивная кривая, level 5 → 3072+ ходов): L2-зеркало с локальными
//!    слотами #10..#20 против САМОГО корпусного файла в rs274;
//! 3. `null_default` — Null-аргумент G65 (таблица 2.1) + дефолт через
//!    IF [EQ #0] против o-call с непереданным аргументом (=0.0).
//!
//! ЧЕСТНЫЕ ОГОВОРКИ (см. стоп-отчёт):
//! * парсеры B2/D2 в полёте — L2-программы собраны ВРУЧНУЮ (AST-зеркала
//!   исходников); corpus factorial использует `return [expr]`
//!   (endsub-retval — ценз-пробел D0–D3) → семантическое зеркало с
//!   глобальным аккумулятором #100/#10;
//! * flowsnake: локальные именованные #<var> LinuxCNC ↔ frame-local слоты
//!   L2 #10..#20 (стек #1–#33); if/else LinuxCNC ↔ идиома Macro B
//!   IF-GOTO+метки (в §2.4 else нет); corpus-файл копируется во временный
//!   каталог для docker-mount;
//! * начальная позиция rs274-sim = (0,0,0) → `L2Config::initial_pos`;
//! * semantic_diff (eid-harness, frozen v0.1) недостижим из eid-l2 без
//!   правки manifest (запрещена заданием) — сравнение канон-потоков
//!   локальное, логика 1:1 из eid-harness::rs274_compare (дубликат до
//!   интеграции конвейера — точка входа `lowering::lower` готова);
//! * docker-гвард: без docker тест сообщает о пропуске и проходит.

use eid_core::ast::{Dir, Op, StartPoint};
use eid_l2::ast::{
    call_arg_local_index, placeholder_span, BinaryOp, Expr, ExprPose, L2Program, L2Stmt, L2Sub,
    MoveKind, SubTarget, VarRef,
};
use eid_l2::interp::{L2Config, L2Evaluator};
use std::path::Path;

// ===========================================================================
// Конфиг L2 для trace-прогонов
// ===========================================================================

fn trace_cfg() -> L2Config {
    L2Config {
        max_eval_steps: 2_000_000,
        max_loop_steps: 2_000_000,
        max_call_depth: 9, // дизайн §5 12.2
        allow_mirror_bake: false,
        initial_pos: Some([0.0, 0.0, 0.0]), // rs274-sim: home (0,0,0)
    }
}

// ===========================================================================
// Мини-эмиттер L1-опов → плоский RS274 (сторона (а): развёрнутая программа)
// ===========================================================================

fn fmt(v: f64) -> String {
    // L1-граница: Num.value уже 4dp (mk_num4, SPEC §10.5) — печать 4dp.
    let s = format!("{v:.4}");
    if s == "-0.0000" {
        "0.0000".to_string()
    } else {
        s
    }
}

fn feed_suffix(state_feed: &Option<eid_core::ast::Num>) -> String {
    match state_feed {
        Some(f) => format!(" F{}", fmt(f.value())),
        None => String::new(),
    }
}

fn emit_ops(ops: &[Op]) -> String {
    // ЗАГОЛОВОК БЕЗ G21: rs274-sim (mini.ini) — mm по умолчанию; G21 дал бы
    // лишнюю канон-строку USE_LENGTH_UNITS, которой нет у нативных программ
    // (сравнение честное: только ходы). G90/G17 — дефолт, канон не пишут.
    let mut out = String::from("G90 G17\n");
    let mut cur: Option<(f64, f64, f64)> = Some((0.0, 0.0, 0.0)); // rs274-sim home
    for op in ops {
        match op {
            Op::Rapid { end, state, .. } => {
                out.push_str(&format!(
                    "G0 X{} Y{} Z{}{}\n",
                    fmt(end.x.value()),
                    fmt(end.y.value()),
                    fmt(end.z.value()),
                    feed_suffix(&state.feed)
                ));
                cur = Some((end.x.value(), end.y.value(), end.z.value()));
            }
            Op::Linear { end, state, .. } => {
                out.push_str(&format!(
                    "G1 X{} Y{} Z{}{}\n",
                    fmt(end.x.value()),
                    fmt(end.y.value()),
                    fmt(end.z.value()),
                    feed_suffix(&state.feed)
                ));
                cur = Some((end.x.value(), end.y.value(), end.z.value()));
            }
            Op::Arc {
                start,
                end,
                center,
                dir,
                state,
                ..
            } => {
                let (sx, sy, _sz) = match start {
                    StartPoint::Known(p) => (p.x.value(), p.y.value(), p.z.value()),
                    StartPoint::Unknown => cur.unwrap_or((0.0, 0.0, 0.0)),
                };
                let i = center.x.value() - sx;
                let j = center.y.value() - sy;
                out.push_str(&format!(
                    "{} X{} Y{} Z{} I{} J{}{}\n",
                    if *dir == Dir::Cw { "G2" } else { "G3" },
                    fmt(end.x.value()),
                    fmt(end.y.value()),
                    fmt(end.z.value()),
                    fmt(i),
                    fmt(j),
                    feed_suffix(&state.feed)
                ));
                cur = Some((end.x.value(), end.y.value(), end.z.value()));
            }
            _ => {}
        }
    }
    // ТЕРМИНАТОР % (полный делимитер, corpus-стиль: ведущий + завершающий):
    // rs274 требует конца программы («File ended with no percent sign or
    // program end»); голый % в середине — «Bad character '%' used». В
    // отличие от M2, конец по % НЕ даёт end-reset-строк канона
    // (SET_XY_ROTATION/SET_FEED_RATE) — сверено с corpus flowsnake.
    // Поэтому нативные программы в этом файле тоже обёрнуты в %, а
    // sub-определения перенесены В ГОЛОВУ (corpus-стиль).
    out.insert_str(0, "%
");
    out.push_str("%
");
    out
}

// ===========================================================================
// rs274-прогон и канон-сравнение (логика 1:1 из eid-harness::rs274_compare —
// дубликат до интеграции, см. шапку)
// ===========================================================================

#[derive(Debug, Clone, PartialEq)]
enum ArgVal {
    Num(f64),
    Sym(String),
}

#[derive(Debug, Clone, PartialEq)]
struct CanonLine {
    op: String,
    args: Vec<ArgVal>,
}

const KEEP_OPS: [&str; 11] = [
    "STRAIGHT_TRAVERSE",
    "STRAIGHT_FEED",
    "ARC_FEED",
    "SET_FEED_RATE",
    "DWELL",
    "PAUSE",
    "SET_G92_OFFSET",
    "SET_G5X_OFFSET",
    "SET_XY_ROTATION",
    "USE_LENGTH_UNITS",
    "SET_FEED_REFERENCE",
];

fn parse_canonical(text: &str) -> Vec<CanonLine> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        let Some(npos) = line.find("N.....") else {
            continue;
        };
        let call = line[npos + "N.....".len()..].trim();
        let Some(open) = call.find('(') else { continue };
        let Some(close_r) = call.rfind(')') else { continue };
        if close_r < open {
            continue;
        }
        let op = call[..open].trim().to_string();
        if !KEEP_OPS.contains(&op.as_str()) {
            continue;
        }
        let args_text = &call[open + 1..close_r];
        let mut args = Vec::new();
        for piece in args_text.split(',') {
            let piece = piece.trim();
            if piece.is_empty() {
                continue;
            }
            match piece.parse::<f64>() {
                Ok(v) => args.push(ArgVal::Num(v)),
                Err(_) => args.push(ArgVal::Sym(piece.to_string())),
            }
        }
        out.push(CanonLine { op, args });
    }
    out
}

fn docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn unique_out_name(stem: &str) -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{stem}.{}.{}.out", std::process::id(), n)
}

/// Прогон файла через rs274 (docker), канонический текст на выходе.
fn run_rs274(ngc_path: &Path) -> Result<String, String> {
    let dir = ngc_path
        .parent()
        .ok_or_else(|| "file has no parent dir".to_string())?
        .to_path_buf();
    let file_name = ngc_path.file_name().unwrap().to_string_lossy().into_owned();
    let stem = std::path::PathBuf::from(&file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| "file has no stem".to_string())?;
    let out_name = unique_out_name(&stem);
    #[cfg(windows)]
    let dir_str = dir.to_string_lossy().replace('/', "\\");
    #[cfg(not(windows))]
    let dir_str = dir.to_string_lossy().into_owned();
    let mount = format!("{dir_str}:/work");
    let in_vm = format!("/work/{file_name}");
    let out_vm = format!("/work/{out_name}");
    let output = std::process::Command::new("docker")
        .args([
            "run", "--rm", "-v", &mount, "eid-linuxcnc-sim", "rs274", "-g", &in_vm, &out_vm,
        ])
        .output()
        .map_err(|e| format!("docker run failed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "rs274 failed ({}): {} {}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    std::fs::read_to_string(dir.join(&out_name)).map_err(|e| format!("canon read: {e}"))
}

#[derive(Debug)]
struct CanonDivergence {
    index: usize,
    detail: String,
}

fn compare_canonical(a: &[CanonLine], b: &[CanonLine], tol: f64) -> Vec<CanonDivergence> {
    let mut div = Vec::new();
    if a.len() != b.len() {
        div.push(CanonDivergence {
            index: 0,
            detail: format!("canon length: {} vs {}", a.len(), b.len()),
        });
    }
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        if x.op != y.op {
            div.push(CanonDivergence {
                index: i,
                detail: format!("op mismatch: {} vs {}", x.op, y.op),
            });
            continue;
        }
        if x.args.len() != y.args.len() {
            div.push(CanonDivergence {
                index: i,
                detail: format!("{}: arity {} vs {}", x.op, x.args.len(), y.args.len()),
            });
            continue;
        }
        // arg[0] КАЖДОЙ канон-строки — номер строки ИСХОДНИКА rs274: у двух
        // разных программ (развёрнутая vs нативный макрос) он по построению
        // различается и семантики движения не несёт → исключается из
        // сравнения (provenance-класс, EQUIVALENCE §5).
        for (k, (av, bv)) in x.args.iter().zip(y.args.iter()).enumerate().skip(1) {
            match (av, bv) {
                (ArgVal::Num(xn), ArgVal::Num(yn)) => {
                    if (xn - yn).abs() > tol {
                        div.push(CanonDivergence {
                            index: i,
                            detail: format!(
                                "{}: arg[{k}] {xn} vs {yn} (Δ = {:.6} > {tol})",
                                x.op,
                                (xn - yn).abs()
                            ),
                        });
                    }
                }
                (ArgVal::Sym(xs), ArgVal::Sym(ys)) => {
                    if xs != ys {
                        div.push(CanonDivergence {
                            index: i,
                            detail: format!("{}: arg[{k}] {xs} vs {ys}", x.op),
                        });
                    }
                }
                _ => div.push(CanonDivergence {
                    index: i,
                    detail: format!("{}: arg[{k}] num-vs-sym", x.op),
                }),
            }
        }
    }
    div
}

fn require_docker() -> bool {
    if docker_available() {
        true
    } else {
        eprintln!("SKIP (no docker): rs274 trace-equivalence unavailable");
        false
    }
}

// ===========================================================================
// Хелперы L2 AST (ручная сборка — парсеры B2/D2 в полёте)
// ===========================================================================

fn num(v: f64) -> Expr {
    Expr::num(v)
}
fn var(i: u32) -> Expr {
    Expr::var(i)
}
fn bin(op: BinaryOp, a: Expr, b: Expr) -> Expr {
    Expr::Binary(op, Box::new(a), Box::new(b))
}
fn assign(t: u32, v: Expr) -> L2Stmt {
    L2Stmt::Assign {
        target: VarRef::Direct(t),
        value: v,
        src: placeholder_span(),
    }
}
fn mv(
    kind: MoveKind,
    x: Option<Expr>,
    y: Option<Expr>,
    z: Option<Expr>,
    feed: Option<Expr>,
) -> L2Stmt {
    L2Stmt::Move {
        kind,
        target: ExprPose {
            x,
            y,
            z,
            a: None,
            b: None,
            c: None,
        },
        feed,
        spindle: None,
        src: placeholder_span(),
    }
}
fn call_o(o: u32, args: Vec<(u8, Expr)>) -> L2Stmt {
    L2Stmt::CallSub {
        target: SubTarget::ONumber(o),
        repeat: None,
        args,
        modal: false,
        src: placeholder_span(),
    }
}
fn arg(letter: u8, e: Expr) -> (u8, Expr) {
    debug_assert!(call_arg_local_index(letter).is_some());
    (letter, e)
}
fn sub_o(o: u32, body: Vec<L2Stmt>) -> L2Sub {
    L2Sub {
        target: SubTarget::ONumber(o),
        body,
    }
}
fn label(n: u32) -> L2Stmt {
    L2Stmt::Label {
        n,
        src: placeholder_span(),
    }
}
fn goto(n: u32, cond: Expr) -> L2Stmt {
    L2Stmt::IfGoto {
        cond,
        target_label: n,
        src: placeholder_span(),
    }
}

/// Прогнать L2-программу через L2Evaluator → L1-опы (строго, без RAW-хвоста).
fn lower_strict(name: &str, program: &L2Program) -> Vec<Op> {
    let mut ev = L2Evaluator::new(trace_cfg());
    let res = ev
        .run(program)
        .unwrap_or_else(|e| panic!("{name}: L2 lowering failed: {}", e.message()));
    assert!(
        res.raw_tail.is_empty(),
        "{name}: unexpected RAW carriers (trace must fully lower): {:?}",
        res.raw_tail
            .iter()
            .map(|c| c.marker)
            .collect::<Vec<_>>()
    );
    res.l1_ops
}

// ===========================================================================
// Макро 1: factorial-рекурсия (G65, глубина 5, M99, IF-GOTO)
// ===========================================================================

#[test]
fn trace_factorial_recursion_g65_vs_oword() {
    if !require_docker() {
        return;
    }
    // L2 (Fanuc-семантика): #100 — глобальный аккумулятор; #1 — локальный
    // аргумент (G65 A#1); рекурсия O9000 глубиной 5 (лимит 9 не задет).
    let main = vec![
        assign(100, num(1.0)),
        assign(1, num(5.0)),
        call_o(9000, vec![arg(b'A', var(1))]),
        // Значение в ход: G1 X#100 (Y/Z модально наследуются из (0,0,0)).
        mv(
            MoveKind::Linear,
            Some(var(100)),
            Some(num(0.0)),
            Some(num(0.0)),
            Some(num(100.0)),
        ),
    ];
    let fact = vec![
        assign(100, bin(BinaryOp::Mul, var(100), var(1))),
        // IF [#1 GT 1] GOTO 10 — иначе M99 (лист).
        goto(10, bin(BinaryOp::Gt, var(1), num(1.0))),
        L2Stmt::Return {
            src: placeholder_span(),
        },
        label(10),
        assign(1, bin(BinaryOp::Sub, var(1), num(1.0))),
        call_o(9000, vec![arg(b'A', var(1))]),
        L2Stmt::Return {
            src: placeholder_span(),
        },
    ];
    let program = L2Program::new(main, vec![sub_o(9000, fact)]);
    let ops = lower_strict("factorial", &program);
    // 5! = 120 (семантический пин до rs274).
    assert_eq!(ops.len(), 1);
    if let Op::Linear { end, .. } = &ops[0] {
        assert_eq!(end.x.value(), 120.0);
    }

    // Натив (LinuxCNC): o-word рекурсия, глобальный #<_n>, аккумулятор #10.
    // %-делимитер (corpus-стиль: ведущий + завершающий % С переводом
    // строки), sub-определение в голове — linear-чтение его перескакивает.
    let native = "\
%
o9000 sub
#10 = [#10 * #<_n>]
o9000 if [#<_n> GT 1]
#<_n> = [#<_n> - 1]
o9000 call
o9000 endif
o9000 endsub
#10 = 1
#<_n> = 5
o9000 call
g1 x#10 y0 z0 f100
%
";
    let (div, n) = trace_equivalence("factorial", &ops, native, 1e-3)
        .unwrap_or_else(|e| panic!("factorial trace run failed: {e}"));
    eprintln!("[factorial] canon lines: {n}, divergences: {}", div.len());
    assert!(
        div.is_empty(),
        "factorial trace-equivalence FAILED (semantic_diff != 0): {div:?}"
    );
}

/// Полный цикл (а)↔(б): lowered-L2 → плоский ngc → rs274 против нативного
/// ngc → rs274; возвращает (расхождения, число канон-строк стороны (а)).
fn trace_equivalence(
    name: &str,
    lowered_ops: &[Op],
    native_ngc: &str,
    tol: f64,
) -> Result<(Vec<CanonDivergence>, usize), String> {
    let dir = std::env::temp_dir().join(format!("eid_l2_trace_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
    let expanded = dir.join(format!("{name}_expanded_l2.ngc"));
    let native = dir.join(format!("{name}_native.ngc"));
    std::fs::write(&expanded, emit_ops(lowered_ops)).map_err(|e| format!("write: {e}"))?;
    std::fs::write(&native, native_ngc).map_err(|e| format!("write: {e}"))?;
    let canon_a = parse_canonical(&run_rs274(&expanded)?);
    let canon_b = parse_canonical(&run_rs274(&native)?);
    let div = compare_canonical(&canon_a, &canon_b, tol);
    if !div.is_empty() {
        let dbg = std::env::temp_dir().join(format!("eid_l2_{name}_canon_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dbg);
        let _ = std::fs::write(dbg.join("a.canon"), format!("{canon_a:#?}"));
        let _ = std::fs::write(dbg.join("b.canon"), format!("{canon_b:#?}"));
        eprintln!("[{name}] canon dumps: {}", dbg.display());
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok((div, canon_a.len()))
}

// ===========================================================================
// Макро 2: flowsnake (corpus golden, рекурсия level 5 → 3072 ходов)
// ===========================================================================

#[test]
fn trace_flowsnake_corpus_level5() {
    if !require_docker() {
        return;
    }
    // L2-зеркало corpus/tier_b/local/lcnc_nc_files_flowsnake.ngc:
    // локальные named LinuxCNC ↔ frame-local слоты L2 (#1–#33 стек):
    // #10=level, #11=startX, #12=startY, #13=endX, #14=endY,
    // #15..#20 = p1X,p1Y,p2X,p2Y,p3X,p3Y. Аргументы call — A..E → #1..#5.
    // if/else LinuxCNC ↔ идиома Macro B: IF-GOTO + метки N20/N30.
    let sqrt12 = 12.0f64.sqrt();
    let level = 5.0;

    let p1x = assign(
        15,
        bin(
            BinaryOp::Div,
            bin(
                BinaryOp::Add,
                bin(BinaryOp::Mul, num(2.0), var(11)),
                var(13),
            ),
            num(3.0),
        ),
    );
    let p1y = assign(
        16,
        bin(
            BinaryOp::Div,
            bin(
                BinaryOp::Add,
                bin(BinaryOp::Mul, num(2.0), var(12)),
                var(14),
            ),
            num(3.0),
        ),
    );
    let p2x = assign(
        17,
        bin(
            BinaryOp::Add,
            bin(
                BinaryOp::Div,
                bin(BinaryOp::Add, var(11), var(13)),
                num(2.0),
            ),
            bin(
                BinaryOp::Div,
                bin(BinaryOp::Sub, var(14), var(12)),
                num(sqrt12),
            ),
        ),
    );
    let p2y = assign(
        18,
        bin(
            BinaryOp::Sub,
            bin(
                BinaryOp::Div,
                bin(BinaryOp::Add, var(12), var(14)),
                num(2.0),
            ),
            bin(
                BinaryOp::Div,
                bin(BinaryOp::Sub, var(13), var(11)),
                num(sqrt12),
            ),
        ),
    );
    let p3x = assign(
        19,
        bin(
            BinaryOp::Div,
            bin(
                BinaryOp::Add,
                var(11),
                bin(BinaryOp::Mul, num(2.0), var(13)),
            ),
            num(3.0),
        ),
    );
    let p3y = assign(
        20,
        bin(
            BinaryOp::Div,
            bin(
                BinaryOp::Add,
                var(12),
                bin(BinaryOp::Mul, num(2.0), var(14)),
            ),
            num(3.0),
        ),
    );
    let recursion_arg = || bin(BinaryOp::Sub, var(10), num(1.0));
    let c1 = call_o(
        1000,
        vec![
            arg(b'A', recursion_arg()),
            arg(b'B', var(11)),
            arg(b'C', var(12)),
            arg(b'D', var(15)),
            arg(b'E', var(16)),
        ],
    );
    let c2 = call_o(
        1000,
        vec![
            arg(b'A', recursion_arg()),
            arg(b'B', var(15)),
            arg(b'C', var(16)),
            arg(b'D', var(17)),
            arg(b'E', var(18)),
        ],
    );
    let c3 = call_o(
        1000,
        vec![
            arg(b'A', recursion_arg()),
            arg(b'B', var(17)),
            arg(b'C', var(18)),
            arg(b'D', var(19)),
            arg(b'E', var(20)),
        ],
    );
    let c4 = call_o(
        1000,
        vec![
            arg(b'A', recursion_arg()),
            arg(b'B', var(19)),
            arg(b'C', var(20)),
            arg(b'D', var(13)),
            arg(b'E', var(14)),
        ],
    );

    // o1000 sub:
    //   level/start/end ← args #1..#5
    //   IF [#10 NE 0] GOTO 20   (else-ветка)
    //   g1 f10 x endX y endY     (level == 0)
    //   GOTO 30
    //   N20: p1..p3; 4 рекурсии
    //   N30: (конец — неявный return)
    let sub_body = vec![
        assign(10, var(1)),
        assign(11, var(2)),
        assign(12, var(3)),
        assign(13, var(4)),
        assign(14, var(5)),
        goto(20, bin(BinaryOp::Ne, var(10), num(0.0))),
        mv(
            MoveKind::Linear,
            Some(var(13)),
            Some(var(14)),
            None,
            Some(num(10.0)),
        ),
        goto(30, num(1.0)),
        label(20),
        p1x,
        p1y,
        p2x,
        p2y,
        p3x,
        p3y,
        c1,
        c2,
        c3,
        c4,
        label(30),
    ];

    let main = vec![
        // g0 z1 / g0 x.25 y1.0 / g1 f10 z0 (позиция — home (0,0,0)).
        mv(MoveKind::Rapid, None, None, Some(num(1.0)), None),
        mv(MoveKind::Rapid, Some(num(0.25)), Some(num(1.0)), None, None),
        mv(
            MoveKind::Linear,
            Some(num(0.25)),
            Some(num(1.0)),
            Some(num(0.0)),
            Some(num(10.0)),
        ),
        call_o(
            1000,
            vec![
                arg(b'A', num(level)),
                arg(b'B', num(0.25)),
                arg(b'C', num(1.0)),
                arg(b'D', num(3.75)),
                arg(b'E', num(1.0)),
            ],
        ),
        call_o(
            1000,
            vec![
                arg(b'A', num(level)),
                arg(b'B', num(3.75)),
                arg(b'C', num(1.0)),
                arg(b'D', num(2.0)),
                arg(b'E', num(3.95)),
            ],
        ),
        call_o(
            1000,
            vec![
                arg(b'A', num(level)),
                arg(b'B', num(2.0)),
                arg(b'C', num(3.95)),
                arg(b'D', num(0.25)),
                arg(b'E', num(1.0)),
            ],
        ),
        // g0 z1 (M5/S-слова — вне motion-множества канона).
        mv(MoveKind::Rapid, None, None, Some(num(1.0)), None),
    ];
    let program = L2Program::new(main, vec![sub_o(1000, sub_body)]);
    let ops = lower_strict("flowsnake", &program);
    assert!(
        ops.len() >= 3074,
        "flowsnake level 5 must expand to >=3074 motion ops, got {}",
        ops.len()
    );

    // Натив: САМ corpus-файл (копия во временный каталог для docker-mount).
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/tier_b/local/lcnc_nc_files_flowsnake.ngc");
    let native_src = std::fs::read_to_string(&corpus)
        .unwrap_or_else(|e| panic!("corpus flowsnake read {}: {e}", corpus.display()));
    let dir = std::env::temp_dir().join(format!("eid_l2_flowsnake_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let native = dir.join("flowsnake_native.ngc");
    std::fs::write(&native, &native_src).unwrap();
    let expanded = dir.join("flowsnake_expanded_l2.ngc");
    std::fs::write(&expanded, emit_ops(&ops)).unwrap();
    let canon_a = parse_canonical(&run_rs274(&expanded).expect("rs274 expanded"));
    let canon_b = parse_canonical(&run_rs274(&native).expect("rs274 native"));
    let div = compare_canonical(&canon_a, &canon_b, 1e-3);
    if !div.is_empty() {
        // Отладочная выгрузка канон-потоков (вне удаляемого каталога).
        let dbg =
            std::env::temp_dir().join(format!("eid_l2_flowsnake_canon_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dbg);
        let _ = std::fs::write(dbg.join("a.canon"), format!("{canon_a:#?}"));
        let _ = std::fs::write(dbg.join("b.canon"), format!("{canon_b:#?}"));
        eprintln!("[flowsnake] canon dumps: {}", dbg.display());
    }
    eprintln!(
        "[flowsnake] canon lines: {} vs {}, divergences: {}",
        canon_a.len(),
        canon_b.len(),
        div.len()
    );
    for d in div.iter().take(5) {
        eprintln!("  div[{}]: {}", d.index, d.detail);
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        div.is_empty(),
        "flowsnake trace-equivalence FAILED ({} divergences, first: {:?})",
        div.len(),
        div.first().map(|d| (&d.index, &d.detail))
    );
}

// ===========================================================================
// Макро 3: Null-аргумент G65 + дефолт (таблица 2.1)
// ===========================================================================

#[test]
fn trace_null_arg_default_g65_vs_oword() {
    if !require_docker() {
        return;
    }
    // L2: G65 P9010 (без X → #24 = Null) + G65 P9010 X5.
    // Sub: IF [#24 EQ #0] THEN #24 = 1.0 (Null-строгость: Null ≠ 0!);
    //      G1 X#24 F50.
    let sub = vec![
        L2Stmt::IfThen {
            cond: bin(BinaryOp::Eq, var(24), Expr::Null),
            then_stmt: Box::new(assign(24, num(1.0))),
            src: placeholder_span(),
        },
        mv(
            MoveKind::Linear,
            Some(var(24)),
            Some(num(0.0)),
            Some(num(0.0)),
            Some(num(50.0)),
        ),
    ];
    let program = L2Program::new(
        vec![call_o(9010, vec![]), call_o(9010, vec![arg(b'X', num(5.0))])],
        vec![sub_o(9010, sub)],
    );
    let ops = lower_strict("null_default", &program);
    assert_eq!(ops.len(), 2);
    if let Op::Linear { end, .. } = &ops[0] {
        assert_eq!(end.x.value(), 1.0); // Null-дефолт
    }
    if let Op::Linear { end, .. } = &ops[1] {
        assert_eq!(end.x.value(), 5.0);
    }

    // Натив: непереданный аргумент o-call = 0.0 → IF [#1 EQ 0].
    // %-делимитер — corpus-стиль (ведущий + завершающий % с \n).
    let native = "\
%
o9010 sub
o9010 if [#1 EQ 0]
#1 = 1
o9010 endif
g1 x#1 y0 z0 f50
o9010 endsub
o9010 call
o9010 call [5]
%
";
    let (div, n) = trace_equivalence("null_default", &ops, native, 1e-3)
        .unwrap_or_else(|e| panic!("null_default trace run failed: {e}"));
    eprintln!("[null_default] canon lines: {n}, divergences: {}", div.len());
    assert!(
        div.is_empty(),
        "null_default trace-equivalence FAILED: {div:?}"
    );
}
