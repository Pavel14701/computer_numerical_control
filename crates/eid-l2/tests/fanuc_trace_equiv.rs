//! fanuc-MB trace-equivalence (DoD 12.4, агент D) — ЗЕРКАЛЬНАЯ сверка
//! fanuc-стороны (linuxcnc-путь сделан в 12.2, `trace_equiv.rs`,
//! semantic_diff==0 на factorial/flowsnake/null_default).
//!
//! МЕТОД (hand-expansion mirror, тот же, что 12.2 flowsnake):
//!  * ОРАКУЛ (а): fanuc-MB макрос разворачивается ВРУЧНУЮ (args подставлены,
//!    IF/GOTO раскручены, Fanuc-специфика сконвертирована) в плоский
//!    linuxcnc-текст → прогон через РЕАЛЬНЫЙ rs274 (docker eid-linuxcnc-sim)
//!    → канон-поток (а);
//!  * L2-ПУТЬ (б): fanuc-парсер (B2) → L2Evaluator → L1-опы → emit
//!    linuxcnc → rs274 → канон-поток (б);
//!  * semantic_diff: сравнение канон-потоков по motion-подмножеству
//!    (KEEP_OPS), допуск EQUIVALENCE §4 (1e-3), arg[0] (provenance) вне
//!    сравнения.
//!
//! МИШЕНИ (обе — corpus/tier_b/inbox, парсятся B2-парсером Ok):
//!  1. `mpe_g65_macro_deep_drill.nc` — O7981 deep-drill: G65-args Z/D/S/F(+E),
//!     guard'ы IF[#x EQ #0] GOTO94x, dwell G4 X2., ветка slowdown F[#9*0.33],
//!     retract G0 Z#18 (#18=#5003). Два документированных вызова (no-E / with-E).
//!  2. `mpe_while_clear_variables.nc` — WHILE[#1 LE 147] DO1 #[#1]=#0 END1 —
//!     косвенная очистка 48 переменных.
//!  3. `mpe_g65_macro_centerline_probe.nc` — Renishaw centerline O7998:
//!     G65-цепочка P9832/P9810/P9812/P9833. TRACE-зеркало — ВНЕ ручного
//!     развёртывания (итерация 14, решение): тела Renishaw-макросов НЕ в
//!     корпусе (9810 = protected move с G31-семантикой; G38.x/G31 = RAW(SEM)
//!     по CONSTRUCT-REGISTRY #10; result-регистры #135..#148 — vendor),
//!     дословная развёртка потребовала бы изобретения проприетарных
//!     внутренностей, непроверяемых на rs274. Пинуется СТРУКТУРА:
//!     centerline_renishaw_call_chain_census (B2-parse Ok + цепочка вызовов
//!     + guard/alarm-каркас).
//!
//! ЧЕСТНЫЕ ОГОВОРКИ (см. стоп-отчёты; статус после фикса B2 v0.1.2):
//!  * BUG-D1 — ИСПРАВЛЕН (амендрмент дока v0.1.2): G65-привязка args в
//!    eid-l2 была ЛИНЕЙНОЙ A→#1..Z→#26 (ast.rs::call_arg_local_index, док
//!    §2.4 «A->#1 .. Z->#26») вместо НЕлинейной Fanuc-таблицы (D→#7, E→#8,
//!    F→#9, H→#11, I→#4, J→#5, K→#6). На deep-drill D-.4 уходила в #4 →
//!    guard IF[#7 EQ #0] срабатывал → GOTO943 → #3000=3 → alarm-путь.
//!    ФИКС: ast.rs Fanuc-таблица (Haas идентична, пин
//!    haas_g65_args_fanuc_table_binding_pin); TEST-side шим
//!    fanuc_letter_to_linear_slot_shim УДАЛЁН — тесты 3/4 и честный
//!    bug_d1 гоняют букв Ка В ФАЙЛЕ (shim=false → убран вовсе).
//!    LinuxCNC o-call — позиционная привязка сохранена (различитель
//!    A..-префикса, interp::call_arg_index; пин
//!    d1_positional_binding_linuxcnc_prefix_letters).
//!  * #5003 (Z конца предыдущего кадра, WorkPos) — в L2 Sys-чтение =
//!    E_RUNTIME_STATE-останов (§3, by design). В sim-home (0,0,0) G90
//!    offsets=0 семантически #5003=0.000 — TEST-side патч узла Assign
//!    #18=#5003 → #18=0 (ровно 1 узел, assert); оракул развёрнут с тем же
//!    значением вручную.
//!  * Fanuc-специфичные кадры тела (S#100 M4, M50, M5, M9) — Raw-носители
//!    (§2.4, honest) → канон их не несёт ни на одной стороне (allow-list
//!    «state-only»/«M50» пиннится).
//!  * Dwell G4 X2.: Fanuc X-слово = СЕКУНДЫ → L2 verbatim 2.0 = верно
//!    (пин deep_drill_dwell_x_word_is_seconds). BUG-D2 — ИСПРАВЛЕН:
//!    Fanuc bare P = МИЛЛИсекунды (P2000 = 2 s), L2 брал verbatim секунды
//!    (parser_macro_common шаг 5) → ×1000; теперь DwellSemantics (Fanuc /
//!    Haas P-dot-rule), #[ignore]-тест bug_d2 развёрнут и зелёный.
//!  * O-номер O7981: файл = ОДНО O-тело (парсер: main-stmts, subs пусты) →
//!    тест оборачивает parsed-stmts в L2Sub{ONumber(7981)} — зеркало
//!    Fanuc-семантики «программа 7981 вызывается по номеру»; вызов-драйвер
//!    синтезирован из документированной шапки файла (сам вызов в
//!    корпус-файле — комментарий).
//!  * while_clear — БЕЗ шима (итерация 14, зонд-верифицировано на rs274,
//!    файлы target/probe/fanuctrace/varprobe/): косвенное присваивание
//!    `#[1]=0` rs274 исполняет НАТИВНО (зонд ip1: X#[1] → X=0) → оракул =
//!    ЛИТЕРАЛЬНОЕ зеркало (очищенная переменная в слове, на НЕсмещённой
//!    оси: z#[100]) — semantic_diff==0 без ручного опускания слов.
//!    Единственная транслитерация: `#[#1]=#0` → `#[#1]=0` — LinuxCNC НЕ
//!    имеет #0/Null (зонд ip2: «Parameter number out of range», exit 1),
//!    т.е. корпусная Fanuc-форма непереносима дословно. РЕАЛЬНАЯ
//!    дивергенция диалектов зафиксирована зондом ip3: слово очищенной
//!    переменной на СМЕЩЁННОЙ оси (g0 y5 → g1 x148 y#[100]) — rs274
//!    ДВИГАЕТ Y в 0 (STRAIGHT_FEED(148,0,0)), L2/Fanuc опускает Null-слово
//!    и держит модальную 5 → на смещённой оси зеркало невозможно физически.
//!    Пины: indirect_assign_rs274_native_probe (натура rs274: a-d) +
//!    while_clear_null_drop_l2_pin (L2 Null-drop со смещением, чистый L2).
//!  * docker-гвард: без docker тест сообщает о пропуске и проходит.
//!  * semantic_diff (eid-harness, frozen) — локальный дубликат логики
//!    rs274_compare (тот же паттерн, что trace_equiv.rs 12.2).

use eid_core::ast::{Num, Op};
use eid_l2::ast::{Expr, ExprPose, L2Program, L2Stmt, L2Sub, MoveKind, SubTarget, SysVar, VarRef};
use eid_l2::interp::{L2Config, L2Evaluator, RtcpMode};
use eid_l2::parser_fanuc::parse_program;
use std::path::Path;

// ===========================================================================
// Конфиг L2 (как trace_equiv.rs 12.2)
// ===========================================================================

fn trace_cfg() -> L2Config {
    L2Config {
        max_eval_steps: 2_000_000,
        max_loop_steps: 2_000_000,
        max_call_depth: 9,
        allow_mirror_bake: false,
        initial_pos: Some([0.0, 0.0, 0.0]), // rs274-sim: home (0,0,0)
        rtcp_mode: RtcpMode::Off,           // 12.5.2: дефолт — RTCP-профиль не задан
    }
}

fn corpus(rel: &str) -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/tier_b/");
    std::fs::read_to_string(format!("{path}{rel}"))
        .unwrap_or_else(|e| panic!("corpus file {rel} not found: {e}"))
}

// ===========================================================================
// Эмиттер L1-опов → плоский RS274 (сторона (б)); Dwell → G4 P (сек)
// ===========================================================================

fn fmt(v: f64) -> String {
    let s = format!("{v:.4}");
    if s == "-0.0000" {
        "0.0000".to_string()
    } else {
        s
    }
}

fn feed_suffix(state_feed: &Option<Num>) -> String {
    match state_feed {
        Some(f) => format!(" F{}", fmt(f.value())),
        None => String::new(),
    }
}

fn emit_ops(ops: &[Op]) -> String {
    // Заголовок без G21/G90-канона — mini.ini sim: mm/G90 по умолчанию
    // (сверено с 12.2: нативные программы без G90 дают тот же канон).
    let mut out = String::from("G90 G17\n");
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
            }
            Op::Linear { end, state, .. } => {
                out.push_str(&format!(
                    "G1 X{} Y{} Z{}{}\n",
                    fmt(end.x.value()),
                    fmt(end.y.value()),
                    fmt(end.z.value()),
                    feed_suffix(&state.feed)
                ));
            }
            Op::Dwell { seconds, .. } => {
                // G4 P — LinuxCNC P = СЕКУНДЫ; L1 Dwell.seconds уже в секундах
                // (Fanuc X-слово verbatim-верно; P-слово — BUG-D2, см. шапку).
                out.push_str(&format!("G4 P{}\n", fmt(seconds.value())));
            }
            _ => {} // Raw-носители (S#100 M4 / M50) — канона не дают (§2.4)
        }
    }
    out.insert_str(0, "%\n");
    out.push_str("%\n");
    out
}

// ===========================================================================
// rs274-прогон и канон-сравнение (логика 1:1 из eid-harness::rs274_compare /
// trace_equiv.rs 12.2 — дубликат до интеграции конвейера)
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
        let Some(close_r) = call.rfind(')') else {
            continue;
        };
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

/// Ожидание процесса с таймаутом (std::process не имеет wait_timeout).
/// Возврат Some(status) если процесс завершился вовремя, None — по таймауту
/// (процесс убивается). Защита от вечного зависания CLI docker при мёртвом
/// демоне (инцидент 2026-11-23: com.docker.backend умер → `docker --version`
/// висел на pipe-подключении 109 минут).
fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: std::time::Duration,
) -> Option<std::process::ExitStatus> {
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(_) => return None,
        }
    }
}

fn docker_available() -> bool {
    let mut child = match std::process::Command::new("docker").arg("--version").spawn() {
        Ok(c) => c,
        Err(_) => return false, // docker не установлен
    };
    wait_with_timeout(&mut child, std::time::Duration::from_secs(10)).is_some()
}

fn unique_out_name(stem: &str) -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{stem}.{}.{}.out", std::process::id(), n)
}

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
    // Таймаут 300 с на rs274-прогон: при смерти демона docker run виснет
    // вечно (инцидент 2026-11-23 — 109-минутное зависание сьюта).
    let mut child = std::process::Command::new("docker")
        .args([
            "run",
            "--rm",
            "-v",
            &mount,
            "eid-linuxcnc-sim",
            "rs274",
            "-g",
            &in_vm,
            &out_vm,
        ])
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("docker run failed: {e}"))?;
    let status = match wait_with_timeout(&mut child, std::time::Duration::from_secs(300)) {
        Some(s) => s,
        None => {
            let _ = std::fs::remove_file(dir.join(&out_name));
            return Err("docker run: таймаут 300с (демон мёртв или wedged)".to_string());
        }
    };
    let stderr = child
        .stderr
        .take()
        .map(|mut s| {
            use std::io::Read;
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            buf
        })
        .unwrap_or_default();
    if !status.success() {
        return Err(format!(
            "rs274 failed ({}): {}",
            status,
            stderr.trim()
        ));
    }
    std::fs::read_to_string(dir.join(&out_name)).map_err(|e| format!("canon read: {e}"))
}

#[derive(Debug)]
#[allow(dead_code)] // поля читаются Debug-выводом дивергенций в panic-сообщениях
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
        // arg[0] — provenance-класс (номер строки исходника rs274 / первый
        // числовой арг), семантики движения не несёт → вне сравнения
        // (EQUIVALENCE §5; тот же рулинг, что trace_equiv.rs 12.2).
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

// ===========================================================================
// L2-хелперы
// ===========================================================================

fn num(v: f64) -> Expr {
    Expr::num(v)
}
fn var(i: u32) -> Expr {
    Expr::var(i)
}

/// Пин-срез без passthrough-Raw носителей (S#100 M4 / M50 и т.п. — §2.4,
/// канона не дают; в l1_ops они идут как L1Op-passthrough).
fn non_raw(ops: &[Op]) -> Vec<&Op> {
    ops.iter()
        .filter(|o| !matches!(o, Op::Raw { .. }))
        .collect()
}

/// Прогнать L2-программу: evaluator → l1_ops; raw_tail (рантайм-носители)
/// запрещён; passthrough-Raw из парсера — только известный allow-list
/// («state-only» spindle/coolant кадры, vendor M50 deep-drill).
fn run_l2(name: &str, program: &L2Program) -> Vec<Op> {
    let mut ev = L2Evaluator::new(trace_cfg());
    let res = ev
        .run(program)
        .unwrap_or_else(|e| panic!("{name}: L2 run failed: {}", e.message()));
    assert!(
        res.raw_tail.is_empty(),
        "{name}: unexpected runtime RAW carriers: {:?}",
        res.raw_tail.iter().map(|c| c.marker).collect::<Vec<_>>()
    );
    for op in &res.l1_ops {
        if let Op::Raw { reason, .. } = op {
            assert!(
                reason.contains("state-only") || reason.contains("M50"),
                "{name}: unexpected RAW carrier outside allow-list: {reason}"
            );
            eprintln!("[{name}] RAW carrier (allow-list): {reason}");
        }
    }
    res.l1_ops
}

/// Полный цикл (а)↔(б): flat-(б) + native-(а) → rs274 → semantic_diff.
/// Возвращает (расхождения, число канон-строк стороны (б)).
fn trace_equivalence(
    name: &str,
    flat_b: &str,
    native_a: &str,
    tol: f64,
) -> Result<(Vec<CanonDivergence>, usize), String> {
    let dir = std::env::temp_dir().join(format!("eid_l2_fanuc_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
    let expanded = dir.join(format!("{name}_l2.ngc"));
    let native = dir.join(format!("{name}_oracle.ngc"));
    std::fs::write(&expanded, flat_b).map_err(|e| format!("write: {e}"))?;
    std::fs::write(&native, native_a).map_err(|e| format!("write: {e}"))?;
    let canon_a = parse_canonical(&run_rs274(&native)?);
    let canon_b = parse_canonical(&run_rs274(&expanded)?);
    let div = compare_canonical(&canon_a, &canon_b, tol);
    if !div.is_empty() {
        // Отладочная выгрузка — ПЕРСИСТЕНТНАЯ (target/probe/fanuctrace,
        // docker-mount зона), чтобы расхождение можно было разобрать.
        let dbg =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/probe/fanuctrace/canon_debug");
        let _ = std::fs::create_dir_all(&dbg);
        let _ = std::fs::write(dbg.join("a_oracle.canon"), format!("{canon_a:#?}"));
        let _ = std::fs::write(dbg.join("b_l2.canon"), format!("{canon_b:#?}"));
        let _ = std::fs::write(dbg.join("b_l2.ngc"), flat_b);
        eprintln!("[{name}] canon dumps: {}", dbg.display());
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok((div, canon_b.len()))
}

// ===========================================================================
// BUG-D1 шим УДАЛЁН (фикс v0.1.2 в крейте): fanuc_letter_to_linear_slot_shim
// (D→'G', E→'H', F→'I', H→'K', I→'D', J→'E', K→'F' — TEST-side ребиндинг
// букв на линейные слоты) больше не нужен — ast::call_arg_local_index
// несёт Fanuc-таблицу (D→#7, E→#8, F→#9, H→#11, I→#4, J→#5, K→#6),
// Haas идентична (пин parser_haas::haas_g65_args_fanuc_table_binding_pin).
// ===========================================================================

/// Патч `#18=#5003` → `#18=0`: Sys-чтение в L2 — E_RUNTIME_STATE-останов
/// (§3 by design); sim-home (0,0,0)/G90/offsets=0 даёт #5003 = 0.000 — то же
/// значение, что зашито в hand-оракуле. НАБЛЮДЕНИЕ (стоп-отчёт): #5003
/// (Fanuc 5001..5008 = позиция конца ПРЕДЫДУЩЕГО кадра) в L2-карте НЕ
/// каноничен — map_common_system покрывает только 5021+/5041+/5061+, поэтому
/// #5003 парсится как VendorSpecific(5003) (семантика чтения та же — останов,
/// но классификация/диагностика вендорская).
fn patch_5003_sim_home(stmts: &mut [L2Stmt], name: &str) {
    let mut patched = 0usize;
    for s in stmts.iter_mut() {
        let hit = matches!(
            s,
            L2Stmt::Assign {
                target: VarRef::Direct(18),
                value: Expr::Var(VarRef::Sys(SysVar::VendorSpecific(5003))),
                ..
            }
        );
        if hit {
            if let L2Stmt::Assign { value, .. } = s {
                *value = num(0.0);
            }
            patched += 1;
        }
    }
    assert_eq!(
        patched, 1,
        "{name}: expected exactly one #18=#5003 (WorkPos Z) node, got {patched}"
    );
}

/// Собрать O7981 как sub: файл = одно O-тело (парсер: main, subs пусты) →
/// зеркало Fanuc «программа 7981» + драйвер G65 P7981 … Аргументы драйвера
/// — буквы КАК В ФАЙЛЕ (шим BUG-D1 удалён: привязка Fanuc-таблицей в крейте).
fn assemble_deep_drill(driver_src: &str, name: &str) -> L2Program {
    let mut macro_prog = parse_program(&corpus("inbox/mpe_g65_macro_deep_drill.nc"))
        .unwrap_or_else(|e| panic!("{name}: fanuc parse failed: {e:?}"));
    assert!(
        macro_prog.subs.is_empty(),
        "{name}: single-O file must parse to main body"
    );
    patch_5003_sim_home(&mut macro_prog.stmts, name);
    let body = macro_prog.stmts;

    let driver =
        parse_program(driver_src).unwrap_or_else(|e| panic!("{name}: driver parse failed: {e:?}"));
    assert_eq!(driver.stmts.len(), 1, "{name}: driver = one G65 call");
    L2Program::new(
        driver.stmts,
        vec![L2Sub {
            target: SubTarget::ONumber(7981),
            body,
        }],
    )
}

// ===========================================================================
// 1. Парс-цензус deep-drill (зелёный пин структуры B2-AST)
// ===========================================================================

#[test]
fn deep_drill_parse_census() {
    let src = corpus("inbox/mpe_g65_macro_deep_drill.nc");
    let prog = parse_program(&src).unwrap();
    let mut census = std::collections::BTreeMap::<&'static str, usize>::new();
    let mut raw_reasons: Vec<String> = Vec::new();
    for s in &prog.stmts {
        *census
            .entry(match s {
                L2Stmt::Move { .. } => "Move",
                L2Stmt::Assign { .. } => "Assign",
                L2Stmt::IfGoto { .. } => "IfGoto",
                L2Stmt::Label { .. } => "Label",
                L2Stmt::Return { .. } => "Return",
                L2Stmt::L1Op(Op::Dwell { .. }) => "Dwell",
                L2Stmt::L1Op(Op::Raw { .. }) => "RawCarrier",
                L2Stmt::L1Op(_) => "L1OpOther",
                _ => "Other",
            })
            .or_insert(0) += 1;
        if let L2Stmt::L1Op(Op::Raw { reason, .. }) = s {
            raw_reasons.push(reason.clone());
        }
    }
    eprintln!("[deep_drill] census: {census:?}");
    eprintln!("[deep_drill] raw reasons: {raw_reasons:?}");
    // Структурный пин: тело O7981 — 5 ходов, 5 guard-GOTO (942..946) +
    // IF[#8 EQ #0] GOTO100, dwell, M99-возвраты, alarm-блок после M99.
    assert_eq!(census.get("Move"), Some(&5));
    assert_eq!(census.get("IfGoto"), Some(&6));
    assert_eq!(census.get("Dwell"), Some(&1));
    assert_eq!(census.get("Return"), Some(&2)); // M99 тела + M99 после alarm-блока
    assert_eq!(census.get("Label"), Some(&6)); // 100, 942..946
    assert_eq!(census.get("Assign"), Some(&7)); // #18,#100 + 5×#3000
                                                // Raw-носители честные: 4 state-only (S#100 M4, S#19 M3, M5, M9) +
                                                // vendor M50.
    assert_eq!(census.get("RawCarrier"), Some(&5));
    for r in &raw_reasons {
        assert!(
            r.contains("state-only") || r.contains("M50"),
            "unexpected raw: {r}"
        );
    }
}

// ===========================================================================
// 2. Dwell X-слово (Fanuc секунды) — зелёный пин (BUG-D2 касается только P)
// ===========================================================================

#[test]
fn deep_drill_dwell_x_word_is_seconds() {
    let prog = parse_program("G4 X2.\n").unwrap();
    let mut ev = L2Evaluator::new(trace_cfg());
    let res = ev.run(&prog).unwrap();
    assert_eq!(res.l1_ops.len(), 1);
    match &res.l1_ops[0] {
        Op::Dwell { seconds, .. } => {
            assert_eq!(seconds.value(), 2.0, "Fanuc G4 X = seconds, verbatim");
        }
        other => panic!("dwell op expected, got {other:?}"),
    }
}

// ===========================================================================
// 3. trace: G65 P7981 Z-7.88 D-.4 S6000 F60. (no E) — semantic_diff == 0
// ===========================================================================

const ORACLE_NO_E: &str = "\
%
;ORACLE-a hand-expansion mirror: corpus/tier_b/inbox/mpe_g65_macro_deep_drill.nc
;call: G65 P7981 Z-7.88 D-.4 S6000 F60. - documented header call, O7981
;guards all pass; #8 (E) = Null -> GOTO 100 skips E slowdown branch
;#18=#5003=0 (sim-home); G4 X2. Fanuc-seconds -> G4 P2 linuxcnc
g1 z-0.4 f20
g4 p2
g1 z-7.88 f60
g0 z0
%
";

#[test]
fn deep_drill_trace_g65_no_e() {
    if !docker_available() {
        eprintln!("SKIP (no docker): rs274 trace-equivalence unavailable");
        return;
    }
    // Честные буквы КАК В ФАЙЛЕ: D→#7, F→#9 (Fanuc-таблица, фикс BUG-D1);
    // Z/S совпадают с линейной (#26/#19).
    let program = assemble_deep_drill("G65 P7981 Z-7.88 D-.4 S6000 F60.\n", "dd_no_e");
    let ops = run_l2("dd_no_e", &program);
    // Семантический пин ДО rs274 (вне passthrough-Raw): pilot -0.4 @F20 →
    // dwell 2 → -7.88 @F60 → retract Z0 (rapid).
    let motion = non_raw(&ops);
    assert_eq!(
        motion.len(),
        4,
        "no-E: 4 канон-носителя (2 Linear, Dwell, Rapid); passthrough-Raw: {}",
        ops.len() - motion.len()
    );
    match motion[0] {
        Op::Linear { end, state, .. } => {
            assert_eq!(end.z.value(), -0.4);
            assert_eq!(state.feed.as_ref().map(|f| f.value()), Some(20.0));
        }
        other => panic!("linear pilot expected, got {other:?}"),
    }
    match motion[1] {
        Op::Dwell { seconds, .. } => assert_eq!(seconds.value(), 2.0),
        other => panic!("dwell expected, got {other:?}"),
    }
    match motion[2] {
        Op::Linear { end, state, .. } => {
            assert_eq!(end.z.value(), -7.88);
            assert_eq!(state.feed.as_ref().map(|f| f.value()), Some(60.0));
        }
        other => panic!("linear final expected, got {other:?}"),
    }
    match motion[3] {
        Op::Rapid { end, state, .. } => {
            assert_eq!(end.z.value(), 0.0);
            assert!(state.feed.is_none(), "G0 без F-слова — modal, не snapshot");
        }
        other => panic!("rapid retract expected, got {other:?}"),
    }

    let flat = emit_ops(&ops);
    let expected = "\
%
G90 G17
G1 X0.0000 Y0.0000 Z-0.4000 F20.0000
G4 P2.0000
G1 X0.0000 Y0.0000 Z-7.8800 F60.0000
G0 X0.0000 Y0.0000 Z0.0000
%
";
    assert_eq!(flat, expected, "flat (б) no-E");

    let (div, n) = trace_equivalence("fanuc_dd_no_e", &flat, ORACLE_NO_E, 1e-3)
        .unwrap_or_else(|e| panic!("dd_no_e trace run failed: {e}"));
    eprintln!("[dd_no_e] canon lines: {n}, divergences: {}", div.len());
    assert!(
        div.is_empty(),
        "deep-drill no-E trace FAILED (semantic_diff != 0): {div:?}"
    );
}

// ===========================================================================
// 4. trace: G65 P7981 Z-7.88 E-7.75 D-.4 S6000 F60. (with E) — diff == 0
// ===========================================================================

const ORACLE_WITH_E: &str = "\
%
;ORACLE-a hand-expansion mirror: corpus/tier_b/inbox/mpe_g65_macro_deep_drill.nc
;call: G65 P7981 Z-7.88 E-7.75 D-.4 S6000 F60. - header line 5
;#8=-7.75 -> slowdown branch: Z-7.5 F60; Z-7.88 F19.8; N100 Z-7.88 F60 -zero len
g1 z-0.4 f20
g4 p2
g1 z-7.5 f60
g1 z-7.88 f19.8
g1 z-7.88 f60
g0 z0
%
";

#[test]
fn deep_drill_trace_g65_with_e() {
    if !docker_available() {
        eprintln!("SKIP (no docker): rs274 trace-equivalence unavailable");
        return;
    }
    // Честные буквы КАК В ФАЙЛЕ: E→#8, D→#7, F→#9 (Fanuc-таблица, фикс
    // BUG-D1).
    let program = assemble_deep_drill(
        "G65 P7981 Z-7.88 E-7.75 D-.4 S6000 F60.\n",
        "dd_with_e",
    );
    let ops = run_l2("dd_with_e", &program);
    let motion = non_raw(&ops);
    assert_eq!(
        motion.len(),
        6,
        "with-E: 6 канон-носителей; passthrough-Raw: {}",
        ops.len() - motion.len()
    );
    // slowdown-ветка: Z[#8+.25] = -7.5 F60, потом F[60*0.33]=19.8.
    match motion[2] {
        Op::Linear { end, state, .. } => {
            assert_eq!(end.z.value(), -7.5);
            assert_eq!(state.feed.as_ref().map(|f| f.value()), Some(60.0));
        }
        other => panic!("slowdown first move expected, got {other:?}"),
    }
    match motion[3] {
        Op::Linear { end, state, .. } => {
            assert_eq!(end.z.value(), -7.88);
            assert_eq!(state.feed.as_ref().map(|f| f.value()), Some(19.8));
        }
        other => panic!("slowdown 0.33 feed expected, got {other:?}"),
    }
    // N100: нулевая длина, но кадр несёт Z-слово → Op::Linear присутствует.
    match motion[4] {
        Op::Linear { end, state, .. } => {
            assert_eq!(end.z.value(), -7.88);
            assert_eq!(state.feed.as_ref().map(|f| f.value()), Some(60.0));
        }
        other => panic!("N100 zero-length restated feed expected, got {other:?}"),
    }

    let flat = emit_ops(&ops);
    let expected = "\
%
G90 G17
G1 X0.0000 Y0.0000 Z-0.4000 F20.0000
G4 P2.0000
G1 X0.0000 Y0.0000 Z-7.5000 F60.0000
G1 X0.0000 Y0.0000 Z-7.8800 F19.8000
G1 X0.0000 Y0.0000 Z-7.8800 F60.0000
G0 X0.0000 Y0.0000 Z0.0000
%
";
    assert_eq!(flat, expected, "flat (б) with-E");

    let (div, n) = trace_equivalence("fanuc_dd_with_e", &flat, ORACLE_WITH_E, 1e-3)
        .unwrap_or_else(|e| panic!("dd_with_e trace run failed: {e}"));
    eprintln!("[dd_with_e] canon lines: {n}, divergences: {}", div.len());
    assert!(
        div.is_empty(),
        "deep-drill with-E trace FAILED (semantic_diff != 0): {div:?}"
    );
}

// ===========================================================================
// 5. trace: while_clear_variables — WHILE + косвенная очистка + probe-хвост
// ===========================================================================

/// Оракул = ЛИТЕРАЛЬНОЕ зеркало L2-текста (rs274-диалект: o1 while/...),
/// единственная транслитерация — `#[#1]=#0` → `#[#1]=0` (LinuxCNC НЕ имеет
/// #0/Null: зонд ip2, indirect_assign_rs274_native_probe (c)). Очищенная
/// переменная несётся В СЛОВЕ на НЕсмещённой оси (z#[100]): rs274 нативно
/// двигает Z в 0, L2 опускает Null-слово (модальный Z=0) — канон совпадает.
/// На СМЕЩЁННОЙ оси диалекты расходятся физически (rs274 двигает в 0 vs
/// Fanuc-модал) — зонд ip3 (d) + while_clear_null_drop_l2_pin.
const ORACLE_WHILE_CLEAR: &str = "\
%
;ORACLE-a literal mirror (shimless, iter-14): corpus/tier_b/inbox/mpe_while_clear_variables.nc
;WHILE #1 LE 147 DO1 / #[#1]=#0 / END1 -> o1 while/endwhile, 48 iterations
;only transliteration: #[#1]=#0 -> #[#1]=0 (LinuxCNC has no #0/Null; probe ip2
;says: Parameter number out of range) - see indirect_assign_rs274_native_probe
;cleared variable IN the word on the undisplaced axis: z#[100] - rs274 moves
;Z to 0 natively, L2 drops the Null word (modal Z=0) - canon equal
;#200 outside 100..147 -> NOT cleared -> z#[200]=3
#100 = 777
#200 = 3
g0 y5
#1 = 100
o1 while [#1 LE 147]
#[#1] = 0
#1 = [#1 + 1]
o1 endwhile
g1 x#1 z#[100] f60
g1 z#[200] f30
%
";

#[test]
fn while_clear_variables_trace() {
    if !docker_available() {
        eprintln!("SKIP (no docker): rs274 trace-equivalence unavailable");
        return;
    }
    let mut main_prog = parse_program(&corpus("inbox/mpe_while_clear_variables.nc"))
        .unwrap_or_else(|e| panic!("while_clear: fanuc parse failed: {e:?}"));
    assert_eq!(main_prog.stmts.len(), 2, "#1=100 + WHILE");
    assert!(main_prog.subs.is_empty());

    // Probe-хвост: очищенная переменная В СЛОВЕ на НЕсмещённой оси (z#[100] —
    // Z в этот момент модальный 0), потом z#[200] (вне диапазона очистки).
    let mut stmts = vec![
        L2Stmt::Assign {
            target: VarRef::Direct(100),
            value: num(777.0),
            src: eid_l2::ast::placeholder_span(),
        },
        L2Stmt::Assign {
            target: VarRef::Direct(200),
            value: num(3.0),
            src: eid_l2::ast::placeholder_span(),
        },
        L2Stmt::Move {
            kind: MoveKind::Rapid,
            target: ExprPose {
                x: None,
                y: Some(num(5.0)),
                z: None,
                a: None,
                b: None,
                c: None,
            },
            feed: None,
            spindle: None,
            src: eid_l2::ast::placeholder_span(),
        },
    ];
    stmts.append(&mut main_prog.stmts);
    stmts.push(L2Stmt::Move {
        kind: MoveKind::Linear,
        target: ExprPose {
            x: Some(var(1)),
            y: None,
            z: Some(Expr::Var(VarRef::Indirect(Box::new(num(100.0))))),
            a: None,
            b: None,
            c: None,
        },
        feed: Some(num(60.0)),
        spindle: None,
        src: eid_l2::ast::placeholder_span(),
    });
    stmts.push(L2Stmt::Move {
        kind: MoveKind::Linear,
        target: ExprPose {
            x: None,
            y: None,
            z: Some(Expr::Var(VarRef::Indirect(Box::new(num(200.0))))),
            a: None,
            b: None,
            c: None,
        },
        feed: Some(num(30.0)),
        spindle: None,
        src: eid_l2::ast::placeholder_span(),
    });
    let program = L2Program::new(stmts, vec![]);
    let ops = run_l2("while_clear", &program);
    let motion = non_raw(&ops);
    assert_eq!(motion.len(), 3, "traverse + 2 linear probe");

    // Конвергентная точка зеркала: z#[100] на несмещённой оси — обе семантики
    // численно равны (L2 Null-drop → модальный 0; rs274 native assign → 0).
    // НАБЛЮДАЕМЫЙ Null-drop со смещением — отдельные пины:
    // while_clear_null_drop_l2_pin (L2) + ip3 в indirect_assign_rs274_native_probe.
    match motion[1] {
        Op::Linear { end, .. } => {
            assert_eq!(end.x.value(), 148.0, "48 итераций: #1 = 148");
            assert_eq!(end.y.value(), 5.0, "Y модальный 5 (g0 y5, Y-слов в хвосте нет)");
            assert_eq!(end.z.value(), 0.0, "z#[100]: cleared — drop→модал 0 / rs274→0");
        }
        other => panic!("probe linear expected, got {other:?}"),
    }
    match motion[2] {
        Op::Linear { end, state, .. } => {
            assert_eq!(end.z.value(), 3.0, "#200 вне диапазона очистки → 3");
            assert_eq!(state.feed.as_ref().map(|f| f.value()), Some(30.0));
        }
        other => panic!("z probe expected, got {other:?}"),
    }

    let flat = emit_ops(&ops);
    let expected = "\
%
G90 G17
G0 X0.0000 Y5.0000 Z0.0000
G1 X148.0000 Y5.0000 Z0.0000 F60.0000
G1 X148.0000 Y5.0000 Z3.0000 F30.0000
%
";
    assert_eq!(flat, expected, "flat (б) while_clear");

    let (div, n) = trace_equivalence("fanuc_while_clear", &flat, ORACLE_WHILE_CLEAR, 1e-3)
        .unwrap_or_else(|e| panic!("while_clear trace run failed: {e}"));
    eprintln!("[while_clear] canon lines: {n}, divergences: {}", div.len());
    assert!(
        div.is_empty(),
        "while_clear trace FAILED (semantic_diff != 0): {div:?}"
    );
}

// ===========================================================================
// 5a. Зонд rs274: косвенное присваивание — НАТИВНО; #0 — ошибка; очищенная
//     переменная в слове на смещённой оси = ДВИЖЕНИЕ в 0 (дивергенция)
// ===========================================================================

/// Прогнать текст программы через реальный rs274 (temp-файл) → канон.
fn run_rs274_text(name: &str, text: &str) -> Result<Vec<CanonLine>, String> {
    let dir = std::env::temp_dir().join(format!("eid_l2_probe_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
    let f = dir.join(format!("{name}.ngc"));
    std::fs::write(&f, text).map_err(|e| format!("write: {e}"))?;
    let res = run_rs274(&f).map(|out| parse_canonical(&out));
    let _ = std::fs::remove_dir_all(&dir);
    res
}

/// Числовой аргумент канон-строки (паника с дампом — только из тестов).
fn feed_arg(line: &CanonLine, k: usize) -> f64 {
    match line.args.get(k) {
        Some(ArgVal::Num(v)) => *v,
        other => panic!("feed arg[{k}] not a number: {other:?}"),
    }
}

/// Итерация 14, зонд-пины (доказательная база безшимного while_clear-зеркала;
/// файлы-оригиналы target/probe/fanuctrace/varprobe/ip*.ngc, docker eid-linuxcnc-sim):
///  (a) `#[1]=0` — косвенное присваивание rs274 исполняет НАТИВНО: G1 X#[1]
///      → X=0.0;
///  (b) ненулевое `#[1]=12` → X=12.0 — arg[0] канона = X-координата (не
///      provenance-номер: g1 стоит в 4-й строке файла, канон несёт 12);
///  (c) корпусная Fanuc-форма `#[1]=#0` — НЕПЕРЕНОСИМА: LinuxCNC не имеет
///      #0/Null → жёсткая ошибка «Parameter number out of range», exit 1
///      (оправдывает транслитерацию `#0`→`0` в ORACLE_WHILE_CLEAR);
///  (d) слово очищенной переменной на СМЕЩЁННОЙ оси: g0 y5 → цикл →
///      g1 x#1 y#[100] — rs274 ДВИГАЕТ Y в 0 (STRAIGHT_FEED(148,0,0));
///      L2/Fanuc опускает Null-слово → модальная 5 (см.
///      while_clear_null_drop_l2_pin). Дивергенция диалектов реальна →
///      зеркало с очищенной переменной в слове возможно ТОЛЬКО на
///      несмещённой оси (как в ORACLE_WHILE_CLEAR).
#[test]
fn indirect_assign_rs274_native_probe() {
    if !docker_available() {
        eprintln!("SKIP (no docker)");
        return;
    }
    // (a) косвенное присваивание нативно
    let canon = run_rs274_text("ip1_indirect_assign", "%\n#1 = 5\n#[1] = 0\ng1 x#[1] f10\n%\n")
        .unwrap_or_else(|e| panic!("ip1: rs274 failed: {e}"));
    let feeds: Vec<&CanonLine> = canon.iter().filter(|l| l.op == "STRAIGHT_FEED").collect();
    assert_eq!(feeds.len(), 1, "ip1: ровно один G1-канон: {canon:?}");
    assert_eq!(
        feed_arg(feeds[0], 0),
        0.0,
        "ip1: #[1]=0 нативно → X#[1] = 0 (косвенное присваивание исполняется)"
    );
    // (b) ненулевое значение + доказательство что arg[0] = X
    let canon = run_rs274_text(
        "ip1b_indirect_assign_nz",
        "%\n#1 = 5\n#[1] = 12\ng1 x#[1] f10\n%\n",
    )
    .unwrap_or_else(|e| panic!("ip1b: rs274 failed: {e}"));
    let feeds: Vec<&CanonLine> = canon.iter().filter(|l| l.op == "STRAIGHT_FEED").collect();
    assert_eq!(feeds.len(), 1, "ip1b: ровно один G1-канон: {canon:?}");
    assert_eq!(
        feed_arg(feeds[0], 0),
        12.0,
        "ip1b: X#[1] = 12 — arg[0] канона = X (g1 — 4-я строка исходника, не 12)"
    );
    // (c) #0 как значение — ошибка (нет Null в LinuxCNC)
    let err = run_rs274_text("ip2_hash0", "%\n#1 = 5\n#[1] = #0\ng1 x#[1] f10\n%\n")
        .expect_err("ip2: #[1]=#0 обязан падать на rs274 (нет #0/Null)");
    assert!(
        err.contains("Parameter number out of range"),
        "ip2: ждём «Parameter number out of range», got: {err}"
    );
    // (d) очищенная переменная в слове на СМЕЩЁННОЙ оси — rs274 двигает в 0
    let canon = run_rs274_text(
        "ip3_cleared_y_word",
        "%\n#100 = 777\ng0 y5\n#1 = 100\no1 while [#1 LE 147]\n#[#1] = 0\n#1 = [#1 + 1]\no1 endwhile\ng1 x#1 y#[100] f60\n%\n",
    )
    .unwrap_or_else(|e| panic!("ip3: rs274 failed: {e}"));
    let feeds: Vec<&CanonLine> = canon.iter().filter(|l| l.op == "STRAIGHT_FEED").collect();
    assert_eq!(feeds.len(), 1, "ip3: ровно один G1-канон: {canon:?}");
    assert_eq!(feed_arg(feeds[0], 0), 148.0, "ip3: X#1 = 148 (48 итераций)");
    assert_eq!(
        feed_arg(feeds[0], 1),
        0.0,
        "ip3: rs274 ДВИГАЕТ Y в 0 (y#[100], #100=0 после цикла) — vs L2/Fanuc: Null-drop → модальная 5"
    );
}

/// Чистый L2-пин (без docker): Fanuc Null-drop со СМЕЩЕНИЕМ — та точка,
/// где зеркало физически невозможно (зонд ip3 (d)): слово очищенной
/// переменной опускается (D0 pose_resolve), модальная координата
/// удерживается: X148 при Y-модале 5 (не 777 — цикл очистил; не 0 — drop).
#[test]
fn while_clear_null_drop_l2_pin() {
    let mut main_prog = parse_program(&corpus("inbox/mpe_while_clear_variables.nc"))
        .unwrap_or_else(|e| panic!("while_clear: fanuc parse failed: {e:?}"));
    assert_eq!(main_prog.stmts.len(), 2, "#1=100 + WHILE");
    let mut stmts = vec![
        L2Stmt::Assign {
            target: VarRef::Direct(100),
            value: num(777.0),
            src: eid_l2::ast::placeholder_span(),
        },
        L2Stmt::Assign {
            target: VarRef::Direct(200),
            value: num(3.0),
            src: eid_l2::ast::placeholder_span(),
        },
        L2Stmt::Move {
            kind: MoveKind::Rapid,
            target: ExprPose {
                x: None,
                y: Some(num(5.0)),
                z: None,
                a: None,
                b: None,
                c: None,
            },
            feed: None,
            spindle: None,
            src: eid_l2::ast::placeholder_span(),
        },
    ];
    stmts.append(&mut main_prog.stmts);
    // Смещённая ось + слово очищенной переменной: Y#[100] (Null → drop).
    stmts.push(L2Stmt::Move {
        kind: MoveKind::Linear,
        target: ExprPose {
            x: Some(var(1)),
            y: Some(Expr::Var(VarRef::Indirect(Box::new(num(100.0))))),
            z: None,
            a: None,
            b: None,
            c: None,
        },
        feed: Some(num(60.0)),
        spindle: None,
        src: eid_l2::ast::placeholder_span(),
    });
    // #200 вне диапазона очистки — слово живое (значение 3).
    stmts.push(L2Stmt::Move {
        kind: MoveKind::Linear,
        target: ExprPose {
            x: None,
            y: None,
            z: Some(Expr::Var(VarRef::Indirect(Box::new(num(200.0))))),
            a: None,
            b: None,
            c: None,
        },
        feed: Some(num(30.0)),
        spindle: None,
        src: eid_l2::ast::placeholder_span(),
    });
    let program = L2Program::new(stmts, vec![]);
    let ops = run_l2("while_clear_drop", &program);
    let motion = non_raw(&ops);
    assert_eq!(motion.len(), 3, "traverse + 2 linear");
    match motion[1] {
        Op::Linear { end, .. } => {
            assert_eq!(end.x.value(), 148.0, "48 итераций: #1 = 148");
            assert_eq!(
                end.y.value(),
                5.0,
                "Null-drop: Y-слово #[100] опущено, Y модальный 5 (не 777, не 0)"
            );
        }
        other => panic!("probe linear expected, got {other:?}"),
    }
    match motion[2] {
        Op::Linear { end, state, .. } => {
            assert_eq!(end.z.value(), 3.0, "#200 не очищена → 3");
            assert_eq!(state.feed.as_ref().map(|f| f.value()), Some(30.0));
        }
        other => panic!("z probe expected, got {other:?}"),
    }
}

// ===========================================================================
// 6. BUG-D1 (рас-unignore после фикса v0.1.2): честные буквы D/F/E драйвера
// ===========================================================================

/// ЧЕСТНЫЙ вариант deep-drill no-E с буквами КАК В КОРПУСЕ (D/F), БЕЗ
/// шима. ДО фикса (v0.1.2): D-.4 → #4 (линейно), Fanuc кладёт в #7 →
/// guard IF[#7 EQ #0] TRUE → GOTO943 → #3000=3 → запись в Sys запрещена →
/// L2 run error (стоп-отчёт agent D). ПОСЛЕ фикса
/// (ast::call_arg_local_index → Fanuc-таблица): D→#7, F→#9 — guards
/// проходят, trace semantic_diff == 0.
#[test]
fn deep_drill_trace_honest_letters_bug_d1() {
    if !docker_available() {
        eprintln!("SKIP (no docker)");
        return;
    }
    let program = assemble_deep_drill("G65 P7981 Z-7.88 D-.4 S6000 F60.\n", "dd_honest");
    let ops = run_l2("dd_honest", &program);
    let flat = emit_ops(&ops);
    let (div, n) = trace_equivalence("fanuc_dd_honest", &flat, ORACLE_NO_E, 1e-3)
        .unwrap_or_else(|e| panic!("dd_honest trace run failed: {e}"));
    eprintln!("[dd_honest] canon lines: {n}, divergences: {}", div.len());
    assert!(div.is_empty(), "honest-letters trace FAILED: {div:?}");
    // Корневой пин фикса: Fanuc D=7, E=8, F=9, I=4, J=5, K=6, H=11
    // (сработает вторым — после того, как trace пройдёт на исправной
    // привязке).
    assert_eq!(eid_l2::ast::call_arg_local_index(b'D'), Some(7));
    assert_eq!(eid_l2::ast::call_arg_local_index(b'E'), Some(8));
    assert_eq!(eid_l2::ast::call_arg_local_index(b'F'), Some(9));
    assert_eq!(eid_l2::ast::call_arg_local_index(b'I'), Some(4));
    assert_eq!(eid_l2::ast::call_arg_local_index(b'J'), Some(5));
    assert_eq!(eid_l2::ast::call_arg_local_index(b'K'), Some(6));
    assert_eq!(eid_l2::ast::call_arg_local_index(b'H'), Some(11));
}

// ===========================================================================
// 7. BUG-D2 (рас-unignore после фикса v0.1.2): Fanuc G4 P = миллисекунды
// ===========================================================================

/// Fanuc Custom Macro B: G04 P-слово = МИЛЛИсекунды (X/U = секунды).
/// ДО фикса L2-парсер брал P verbatim как секунды (parser_macro_common
/// шаг 5, единый для fanuc/haas) → ×1000. ПОСЛЕ фикса — диалект-семантика
/// parser_macro_common::DwellSemantics::Fanuc (bare P /1000; Haas —
/// P-dot-rule, пин parser_haas::haas_g04_p_dot_rule_bare_ms_dotted_sec).
#[test]
fn dwell_p_word_fanuc_ms_bug_d2() {
    let prog = parse_program("G4 P2000\n").unwrap();
    let mut ev = L2Evaluator::new(trace_cfg());
    let res = ev.run(&prog).unwrap();
    match &res.l1_ops[0] {
        Op::Dwell { seconds, .. } => {
            assert_eq!(
                seconds.value(),
                2.0,
                "Fanuc G4 P2000 = 2000 ms = 2 s (BUG-D2 фикс: P/1000)"
            );
        }
        other => panic!("dwell op expected, got {other:?}"),
    }
}
