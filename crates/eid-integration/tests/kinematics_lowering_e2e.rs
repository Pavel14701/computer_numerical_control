//! E2E фазы 12.5.3 (агент B2): MachineKinematics-профиль → L2-lowering →
//! L1-опы machine-осей (TCP→joints trt-пересчёт).
//!
//! Состав:
//! * TOML-профиль `[kinematics]` (`profiles/xyzac_trt_example.toml` —
//!   единственный источник схемы) загружается через `MachineProfile::load` —
//!   ТОТ ЖЕ путь, что вызывает CLI (`--profile`);
//! * маппинг `MachineProfile::l2_lowering_config()` →
//!   `eid_l2::interp::L2Config.rtcp_mode` (проброс b7d28bb-аналог);
//! * `eid_l2::lowering::lower` — ходы с полной TCP-позой опускаются в
//!   L1-опы machine-осей (TCP→joints, оракул trtfuncs);
//! * **оракул kinematics.rs-круга на КОРПУСЕ**: независимый Off-прогон тех
//!   же файлов даёт TCP-геометрию (линейные кадры — TCP-опы; ротационные —
//!   payload-носители с разрешёнными интерпретатором значениями слов,
//!   включая макро-переменные) и отслеженные углы; каждый trt-оп проверяется
//!   forward(joints, rot) ≈ TCP (круговой инвариант forward∘inverse на
//!   живых данных, 4dp-допуск);
//! * негатив: Triv-профиль (секция отсутствует) → ротационные кадры =
//!   Raw-носители «как прежде» (12.2), rtcp_active = false.
//!
//! Файлы — 5 мишеней ABC/RTCP-цензов инвентаря
//! (`corpus/tier_b/_l2_targets.md` §5): ncm_g434 (RTCP G43.4, ротационных
//! кадров ~44.6k), ncm_g43_g69, ncm_g41_d, gce_full_industrial_lathe
//! (B/A/C — 7 чужих B-слов под XYZAC → честные носители), vis_SmilinJack
//! (A-ось). Все 5 — fanuc-MB, без subs/calls/whiles (AST-порядок = порядок
//! исполнения — зонд 12.5.3), без G68/G51-трансформаций (ценз: G68-set ×0
//! в корпусе) ⇒ xform = identity.
//!
//! ГРАНИЦА (честно): CLI-бинарь `eid translate` L2-программы не
//! маршрутизирует (конвейер 12.4 — L1↔L1; маршрутизация L2 — отдельная
//! фаза), поэтому e2e выполнен через функции конвейера, которые вызывает
//! CLI: `MachineProfile::load` (паттерн `e2e.rs` — «те же функции, не
//! через бинарь») + библиотечный `eid_l2::lowering::lower` с конфигом из
//! профиля. Границы полноты — interp.rs/lowering.rs (дуги под trt —
//! носители: TCP-дуга не аффинна в machine-осях; head-head и др.
//! топологии — при оракуле формул, док §12.5 v0.1.5).

use eid_core::ast::Op;
use eid_integration::profile::{MachineKinematics, MachineProfile};
use eid_l2::ast::{L2Program, L2Stmt, MoveKind};
use eid_l2::interp::{L2Config, RawMarker, RtcpMode};
use eid_l2::kinematics::{xyzac_forward, Joints5, TrtParams};
use std::path::{Path, PathBuf};

/// Путь к файлу corpus/tier_b/<rel>.
fn corpus(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("corpus")
        .join("tier_b")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("corpus {rel}: {e}"))
}

/// Профиль из examples-TOML (тот же путь загрузки, что CLI `--profile`).
fn xyzac_profile() -> MachineProfile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("profiles")
        .join("xyzac_trt_example.toml");
    MachineProfile::load(&path).expect("example profile must load+validate")
}

/// Конфиг lowering из профиля + бюджеты больших файлов + home (0,0,0)
/// (та же конвенция initial_pos, что trace-тесты: rs274-сим home 0).
fn lowering_cfg(profile: &MachineProfile) -> L2Config {
    L2Config {
        max_eval_steps: 20_000_000,
        max_loop_steps: 2_000_000,
        initial_pos: Some([0.0, 0.0, 0.0]),
        ..profile.l2_lowering_config()
    }
}

fn parse_fanuc(src: &str, name: &str) -> L2Program {
    eid_l2::parser_fanuc::parse_program(src)
        .unwrap_or_else(|e| panic!("{name}: parse failed ({:?})", e.first()))
}

/// Рекурсивный обход операторов (AST-порядок; файлы мишеней — без
/// subs/calls — порядок исполнения = AST-порядок). IfThen в мишенях
/// отсутствует (постпроцессорный вывод — прямолинейные программы): ветка с
/// Move в мёртвой ветке D3 сломала бы AST-порядок — честный отказ.
fn walk_moves<'a>(stmts: &'a [L2Stmt], f: &mut impl FnMut(&'a L2Stmt)) {
    for s in stmts {
        f(s);
        match s {
            L2Stmt::While { body, .. } | L2Stmt::WithTransform { body, .. } => walk_moves(body, f),
            L2Stmt::IfThen { then_stmt, .. } => {
                if matches!(**then_stmt, L2Stmt::Move { .. }) {
                    panic!("IfThen(move) in oracle target — AST-order oracle unsupported");
                }
            }
            _ => {}
        }
    }
}

fn is_rotary(target: &eid_l2::ast::ExprPose) -> bool {
    target.a.is_some() || target.b.is_some() || target.c.is_some()
}

/// Прямолинейность (для оракула AST-порядка): N-метки (Label) — no-op
/// маркеры, безвредны; IfGoto/While/CallSub меняют порядок исполнения —
/// с ними пофреймовый круг-оракул неприменим.
fn assert_straight_line(program: &L2Program, name: &str) {
    walk_moves(&program.stmts, &mut |s| match s {
        L2Stmt::IfGoto { .. } | L2Stmt::While { .. } | L2Stmt::CallSub { .. } => {
            panic!("{name}: control flow — AST-order circle oracle unsupported")
        }
        _ => {}
    });
    assert!(
        program.subs.is_empty(),
        "{name}: subs — AST-order oracle unsupported"
    );
}

/// payload-hex носителя → текст кадра (обратимо: lowering::payload_hex).
fn payload_text(op: &Op) -> String {
    let Op::Raw { payload_hex, .. } = op else {
        panic!("expected Raw carrier");
    };
    let bytes: Vec<u8> = (0..payload_hex.len() / 2)
        .map(|i| u8::from_str_radix(&payload_hex[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    String::from_utf8(bytes).unwrap()
}

/// Слова кадра из payload-текста: `G0 X241.781 Y286.\n` → [(X, 241.781), …]
/// (кодовые буквы отфильтрованы — нужны только координаты/углы).
fn payload_words(op: &Op) -> Vec<(char, f64)> {
    let mut out = Vec::new();
    for tok in payload_text(op).split_whitespace() {
        let mut ch = tok.chars();
        let (Some(letter), rest) = (ch.next(), ch.as_str()) else {
            continue;
        };
        let letter = letter.to_ascii_uppercase();
        if matches!(
            letter,
            'G' | 'F' | 'M' | 'N' | 'O' | 'H' | 'S' | 'T' | 'D' | 'P' | 'L'
        ) {
            continue;
        }
        if let Ok(v) = rest.parse::<f64>() {
            out.push((letter, v));
        }
    }
    out
}

/// Значение слова по букве.
fn word_of(words: &[(char, f64)], letter: char) -> Option<f64> {
    words.iter().find(|(l, _)| *l == letter).map(|(_, v)| *v)
}

fn op_end_xyz(op: &Op) -> [f64; 3] {
    match op {
        Op::Rapid { end, .. } | Op::Linear { end, .. } | Op::Arc { end, .. } => {
            [end.x.value(), end.y.value(), end.z.value()]
        }
        _ => panic!("expected motion op"),
    }
}

/// Полный прогон одного файла: профиль → L2Config → lowering (Trt + Off) →
/// структурные пины + оракул kinematics-круга на КАЖДОМ trt-опе.
///
/// Оракул: независимый Off-прогон даёт TCP-геометрию (линейные кадры —
/// TCP-опы; ротационные — payload-носители с разрешёнными значениями
/// слов) и отслеженные углы (A/C из payload'ов, модальность). Проверка:
/// каждый trt-оп machine-осей forward(joints, rot) ≈ TCP-позиция кадра
/// (круговой инвариант forward∘inverse на данных корпуса).
fn run_file_circle_oracle(rel: &str, params: &TrtParams, name: &str) {
    let src = corpus(rel);
    let program = parse_fanuc(&src, name);
    assert_straight_line(&program, name);
    let profile = xyzac_profile();

    // Два прогона: trt-профиль (joints) и Off-профиль (TCP ground truth).
    let trt = eid_l2::lowering::lower(&program, lowering_cfg(&profile))
        .unwrap_or_else(|e| panic!("{name}: trt lowering failed: {e:?}"));
    let mut off_cfg = lowering_cfg(&profile);
    off_cfg.rtcp_mode = RtcpMode::Off;
    let off = eid_l2::lowering::lower(&program, off_cfg)
        .unwrap_or_else(|e| panic!("{name}: off lowering failed: {e:?}"));

    // ---- структурные пины ----
    assert!(trt.rtcp_active, "{name}: trt run must mark rtcp_active");
    assert!(
        !off.rtcp_active,
        "{name}: Off run must NOT mark rtcp_active"
    );
    // Классификация кадров (AST-порядок = порядок исполнения) + L1Op-
    // passthrough и RtcpMode-носители (G43.4/G49 — §3 строка 6, в обоих
    // режимах).
    let (mut total_frames, mut rotary_frames, mut arc_frames, mut foreign_b) = (0, 0, 0, 0);
    let (mut l1op_stmts, mut rtcp_stmts) = (0usize, 0usize);
    walk_moves(&program.stmts, &mut |s| match s {
        L2Stmt::Move { kind, target, .. } => {
            if matches!(kind, MoveKind::Arc { .. }) {
                arc_frames += 1;
            } else {
                total_frames += 1;
            }
            if is_rotary(target) {
                rotary_frames += 1;
                if target.b.is_some() {
                    foreign_b += 1;
                }
            }
        }
        L2Stmt::L1Op(_) => l1op_stmts += 1,
        L2Stmt::RtcpMode { .. } => rtcp_stmts += 1,
        _ => {}
    });
    // Каждый Move-кадр — РОВНО один исход (op или носитель); L1Op — один
    // passthrough-оп; RtcpMode — один носитель.
    assert_eq!(
        trt.l1_ops.len() + trt.raw_tail.len(),
        total_frames + arc_frames + l1op_stmts + rtcp_stmts,
        "{name}: every frame must yield exactly one outcome (trt)"
    );
    assert_eq!(
        off.l1_ops.len() + off.raw_tail.len(),
        total_frames + arc_frames + l1op_stmts + rtcp_stmts,
        "{name}: every frame must yield exactly one outcome (off)"
    );
    // Trt-носители: только честные границы (дуги + чужие B-слова +
    // RtcpMode-носители §3).
    for item in &trt.raw_tail {
        match item.marker {
            RawMarker::ArcUnlowered | RawMarker::RotaryAxis | RawMarker::Rtcp => {}
            other => panic!("{name}: unexpected trt carrier marker {other:?}"),
        }
    }
    let trt_arc_carriers = trt
        .raw_tail
        .iter()
        .filter(|i| i.marker == RawMarker::ArcUnlowered)
        .count();
    let trt_rotary_carriers = trt
        .raw_tail
        .iter()
        .filter(|i| i.marker == RawMarker::RotaryAxis)
        .count();
    let trt_rtcp_carriers = trt
        .raw_tail
        .iter()
        .filter(|i| i.marker == RawMarker::Rtcp)
        .count();
    assert_eq!(
        trt_arc_carriers, arc_frames,
        "{name}: arcs → carriers (trt, честная граница)"
    );
    assert_eq!(
        trt_rotary_carriers, foreign_b,
        "{name}: trt rotary carriers == foreign B-words only"
    );
    assert_eq!(trt_rtcp_carriers, rtcp_stmts, "{name}: RtcpMode carriers");

    // ---- оракул kinematics-круга (по каждому кадру, порядок исполнения) ----
    let mut off_ops = off.l1_ops.iter();
    let mut off_carriers = off.raw_tail.iter();
    let mut trt_ops = trt.l1_ops.iter();
    let mut trt_carriers = trt.raw_tail.iter();
    let mut tcp = [0.0f64; 3]; // initial_pos home (rs274-сим-конвенция)
    let mut rot = (0.0f64, 0.0f64); // home-допущение lowering'а (12.5.3)
    let mut checked = 0usize;
    // 4dp-канон: joints/TCP округлены до 5e-5; изометрия поворота не
    // усиливает ошибку → 1e-3 с запасом.
    const TOL: f64 = 1e-3;
    walk_moves(&program.stmts, &mut |s| match s {
        L2Stmt::RtcpMode { .. } => {
            // G43.4/G49 — носитель §3 строки 6 в ОБОИХ режимах; позицию
            // и углы не меняет.
            let off_c = off_carriers.next().expect("{name}: off carriers");
            let trt_c = trt_carriers.next().expect("{name}: trt carriers");
            assert_eq!(off_c.marker, RawMarker::Rtcp);
            assert_eq!(trt_c.marker, RawMarker::Rtcp);
        }
        L2Stmt::L1Op(_) => {
            // Passthrough-оп в обоих прогонах; на TCP/углы не влияет.
            let off_o = off_ops.next().expect("{name}: off ops");
            let trt_o = trt_ops.next().expect("{name}: trt ops");
            assert!(!matches!(
                off_o,
                Op::Rapid { .. } | Op::Linear { .. } | Op::Arc { .. }
            ));
            let _ = trt_o;
        }
        L2Stmt::Move { kind, target, .. } => {
            let arc = matches!(kind, MoveKind::Arc { .. });
            let rotary = is_rotary(target);
            let foreign_b = target.b.is_some();
            // --- Off-прогон: TCP ground truth + углы ---
            if arc {
                // Дуга в Off: lowered (Op::Arc, pos_prog = end) ИЛИ честный
                // носитель (геометрия не выводима) — позиция не обновляется.
                let is_arc_op = off_ops
                    .clone()
                    .next()
                    .is_some_and(|op| matches!(op, Op::Arc { .. }));
                if is_arc_op {
                    let op = off_ops.next().unwrap();
                    tcp = op_end_xyz(op);
                } else {
                    let carrier = off_carriers.next().expect("{name}: off carriers");
                    assert_eq!(carrier.marker, RawMarker::ArcUnlowered);
                }
            } else if rotary {
                let carrier = off_carriers.next().expect("{name}: off carriers");
                assert_eq!(
                    carrier.marker,
                    RawMarker::RotaryAxis,
                    "{name}: off rotary carrier"
                );
                let words = payload_words(&carrier.op);
                if let Some(x) = word_of(&words, 'X') {
                    tcp[0] = x;
                }
                if let Some(y) = word_of(&words, 'Y') {
                    tcp[1] = y;
                }
                if let Some(z) = word_of(&words, 'Z') {
                    tcp[2] = z;
                }
                // Углы: СВОИ A/C-слова носителя (чужой B-кадр семантики
                // профиля не задаёт — углы не трогает, как и lowering).
                if !foreign_b {
                    rot = (
                        word_of(&words, 'A').unwrap_or(rot.0),
                        word_of(&words, 'C').unwrap_or(rot.1),
                    );
                }
            } else {
                let op = off_ops.next().expect("{name}: off ops");
                assert!(
                    matches!(op, Op::Rapid { .. } | Op::Linear { .. }),
                    "{name}: off linear op"
                );
                tcp = op_end_xyz(op);
            }
            // --- Trt-прогон: joints-оп или честный носитель ---
            if arc || foreign_b {
                let carrier = trt_carriers.next().expect("{name}: trt carriers");
                let want = if arc {
                    RawMarker::ArcUnlowered
                } else {
                    RawMarker::RotaryAxis
                };
                assert_eq!(carrier.marker, want, "{name}: trt carrier marker");
            } else {
                let op = trt_ops.next().expect("{name}: trt ops");
                let end = op_end_xyz(op);
                // ОРАКУЛ: forward(inverse(TCP)) == TCP — kinematics-круг
                // (joints — 4dp-канон op, углы pass-through).
                let fwd =
                    xyzac_forward(params, &Joints5::new(end[0], end[1], end[2], rot.0, rot.1));
                for (axis, got, want) in
                    [(0, fwd.x, tcp[0]), (1, fwd.y, tcp[1]), (2, fwd.z, tcp[2])]
                {
                    assert!(
                        (got - want).abs() <= TOL,
                        "{name}: circle oracle axis {axis}: forward(joints)={got} != TCP {want} (rot {rot:?})"
                    );
                }
                checked += 1;
            }
        }
        _ => {}
    });
    assert!(
        off_ops.next().is_none() && off_carriers.next().is_none(),
        "{name}: off outputs must be fully consumed"
    );
    assert!(
        trt_ops.next().is_none() && trt_carriers.next().is_none(),
        "{name}: trt outputs must be fully consumed"
    );
    println!(
        "{name}: frames={} (rotary {rotary_frames}, arcs {arc_frames}, foreign-B {foreign_b}); trt ops={} carriers={} — circle oracle checked {checked} ops",
        total_frames + arc_frames,
        trt.l1_ops.len(),
        trt.raw_tail.len()
    );
}

/// Структурный прогон файла с control-flow (gce_full: IfGoto-прыжки —
/// порядок исполнения ≠ AST-порядок, пофреймовый круг-оракул неприменим):
/// идентичность исполнения в обоих прогонах (число исходов), passthrough-
/// анкеры (Op::Raw/ToolChange L1Op — байт-в-байт в обоих потоках, полный
/// порядок), состав носителей, флаги режимов.
fn run_file_structural(rel: &str, name: &str) {
    let src = corpus(rel);
    let program = parse_fanuc(&src, name);
    let profile = xyzac_profile();
    let trt = eid_l2::lowering::lower(&program, lowering_cfg(&profile))
        .unwrap_or_else(|e| panic!("{name}: trt lowering failed: {e:?}"));
    let mut off_cfg = lowering_cfg(&profile);
    off_cfg.rtcp_mode = RtcpMode::Off;
    let off = eid_l2::lowering::lower(&program, off_cfg)
        .unwrap_or_else(|e| panic!("{name}: off lowering failed: {e:?}"));

    assert!(trt.rtcp_active, "{name}: trt run must mark rtcp_active");
    assert!(!off.rtcp_active, "{name}: Off must not mark");
    // Одинаковый набор исполняемых кадров ⇒ одинаковое число исходов.
    assert_eq!(
        trt.l1_ops.len() + trt.raw_tail.len(),
        off.l1_ops.len() + off.raw_tail.len(),
        "{name}: identical execution in both modes"
    );
    // Passthrough-анкеры: L1Op-элементы (Op::Raw / ToolChange — не motion)
    // в l1_ops обоих прогонов совпадают попарно (полный порядок исполнения
    // не нарушен lowering'ом).
    fn passthrough(ops: &[Op]) -> Vec<&Op> {
        ops.iter()
            .filter(|op| !matches!(op, Op::Rapid { .. } | Op::Linear { .. } | Op::Arc { .. }))
            .collect()
    }
    let pt_trt = passthrough(&trt.l1_ops);
    let pt_off = passthrough(&off.l1_ops);
    assert_eq!(
        pt_trt.len(),
        pt_off.len(),
        "{name}: passthrough anchor count"
    );
    for (t, o) in pt_trt.iter().zip(pt_off.iter()) {
        assert_eq!(t, o, "{name}: passthrough anchors equal");
    }
    // Носители trt — только честные границы.
    for item in &trt.raw_tail {
        match item.marker {
            RawMarker::ArcUnlowered | RawMarker::RotaryAxis | RawMarker::Rtcp => {}
            other => panic!("{name}: unexpected trt carrier marker {other:?}"),
        }
    }
    println!(
        "{name}: structural — trt ops={} carriers={}; off ops={} carriers={}; passthrough anchors {}",
        trt.l1_ops.len(),
        trt.raw_tail.len(),
        off.l1_ops.len(),
        off.raw_tail.len(),
        pt_trt.len()
    );
}

/// 5 мишеней ABC/RTCP-цензов инвентаря через профиль-конвейер: полный
/// kinematics-круг на каждом trt-опе прямолинейных файлов + структурная
/// сверка файла с control-flow (gce_full).
#[test]
fn e2e_kinematics_5_inventory_rtcp_files_circle_oracle() {
    let profile = xyzac_profile();
    let RtcpMode::XyzacTrt { params } = profile.l2_lowering_config().rtcp_mode else {
        panic!("example profile must map to XyzacTrt");
    };
    // Прямолинейные: полный круг-оракул на КАЖДОМ trt-опе.
    for f in [
        "local/ncm_g434_rtcp_5x_milling.nc",
        "local/ncm_g43_g69_3d5x_milling.nc",
        "local/ncm_g41_d_2d5_milling.nc",
        "inbox/vis_SmilinJack_-_ThreeAxisFlatMachine_Relief.tap",
    ] {
        run_file_circle_oracle(f, &params, f);
    }
    // Control-flow (IfGoto-прыжки): структурная сверка.
    run_file_structural("inbox/gce_full_industrial_lathe.g", "gce_full");
}

/// Негатив (Triv): ротационные кадры — Raw-носители «как прежде» (12.2),
/// rtcp_active = false; дуги опускаются TCP-опами; payload носителя —
/// дословный кадр с подставленными значениями (T4).
#[test]
fn e2e_kinematics_triv_negative_rotary_raw_as_before() {
    let src = corpus("local/ncm_g41_d_2d5_milling.nc");
    let program = parse_fanuc(&src, "ncm_g41_d");
    let triv = MachineProfile::linuxcnc_default(); // секции [kinematics] нет
    assert_eq!(triv.kinematics, MachineKinematics::Triv);
    let cfg = lowering_cfg(&triv);
    assert_eq!(cfg.rtcp_mode, RtcpMode::Off);
    let res = eid_l2::lowering::lower(&program, cfg).unwrap();
    // Ротационные кадры (`A0 C0` ×4) → носители RotaryAxis «как прежде»;
    // rtcp_active = false. (3 arc-geometry-носителя в файле — Off-граница
    // 12.2, режим не зависит.)
    assert_eq!(
        res.raw_tail
            .iter()
            .filter(|i| i.marker == RawMarker::RotaryAxis)
            .count(),
        4
    );
    assert!(!res.rtcp_active);
    // Payload носителя — дословный кадр с подставленными значениями (T4).
    assert_eq!(payload_text(&res.raw_tail[0].op), "G0 A0.0000 C0.0000\n");
    // Дуги под Triv опускаются TCP-опами (прежнее поведение).
    assert!(res.l1_ops.iter().any(|op| matches!(op, Op::Arc { .. })));
}

/// Ручной golden на живом корпусе: ncm_g434 — первый кадр с НЕнулевым
/// углом: joints trt-прогона ≠ TCP (пивот example-профиля −345 мм), круг
/// forward(joints, углы из Off-носителя) замыкается в TCP. Макро-переменные
/// файла разрешены интерпретатором в обоих прогонах — сверка значений.
#[test]
fn e2e_kinematics_ncm_g434_first_nonzero_rotary_golden() {
    let profile = xyzac_profile();
    let cfg = lowering_cfg(&profile);
    let RtcpMode::XyzacTrt { params } = profile.l2_lowering_config().rtcp_mode else {
        unreachable!()
    };
    let src = corpus("local/ncm_g434_rtcp_5x_milling.nc");
    let program = parse_fanuc(&src, "ncm_g434");
    let trt = eid_l2::lowering::lower(&program, cfg.clone()).unwrap();
    let mut off_cfg = cfg;
    off_cfg.rtcp_mode = RtcpMode::Off;
    let off = eid_l2::lowering::lower(&program, off_cfg).unwrap();
    let mut off_ops = off.l1_ops.iter();
    let mut off_carriers = off.raw_tail.iter();
    let mut trt_ops = trt.l1_ops.iter();
    let mut trt_carriers = trt.raw_tail.iter();
    let mut tcp = [0.0f64; 3];
    let mut rot = (0.0f64, 0.0f64);
    let mut found = None;
    walk_moves(&program.stmts, &mut |s| {
        if found.is_some() {
            return;
        }
        match s {
            // G43.4/G49 — носители в обоих режимах.
            L2Stmt::RtcpMode { .. } => {
                off_carriers.next().unwrap();
                trt_carriers.next().unwrap();
            }
            // Passthrough-оп в обоих прогонах.
            L2Stmt::L1Op(_) => {
                off_ops.next().unwrap();
                trt_ops.next().unwrap();
            }
            L2Stmt::Move { kind, target, .. } => {
                let arc = matches!(kind, MoveKind::Arc { .. });
                let rotary = is_rotary(target);
                let foreign_b = target.b.is_some();
                if arc {
                    let is_arc_op = off_ops
                        .clone()
                        .next()
                        .is_some_and(|op| matches!(op, Op::Arc { .. }));
                    if is_arc_op {
                        let op = off_ops.next().unwrap();
                        tcp = op_end_xyz(op);
                    } else {
                        off_carriers.next().unwrap();
                    }
                    trt_carriers.next().unwrap(); // trt: дуга → носитель
                    return;
                }
                if !rotary {
                    let op = off_ops.next().unwrap();
                    tcp = op_end_xyz(op);
                    let _ = trt_ops.next().unwrap();
                    return;
                }
                let carrier = off_carriers.next().unwrap();
                let words = payload_words(&carrier.op);
                if let Some(x) = word_of(&words, 'X') {
                    tcp[0] = x;
                }
                if let Some(y) = word_of(&words, 'Y') {
                    tcp[1] = y;
                }
                if let Some(z) = word_of(&words, 'Z') {
                    tcp[2] = z;
                }
                rot = (
                    word_of(&words, 'A').unwrap_or(rot.0),
                    word_of(&words, 'C').unwrap_or(rot.1),
                );
                if foreign_b {
                    trt_carriers.next().unwrap();
                } else {
                    let op = trt_ops.next().unwrap();
                    if rot.0.abs() > 1e-9 || rot.1.abs() > 1e-9 {
                        let end = op_end_xyz(op);
                        let diff = (0..3)
                            .map(|i| (end[i] - tcp[i]).abs())
                            .fold(0.0f64, f64::max);
                        // Пивот −345 мм ⇒ joints != TCP на ненулевой угол.
                        assert!(
                            diff > 1.0,
                            "expected visible joints-vs-TCP divergence at first non-zero rotary, got {diff}"
                        );
                        // Круг: forward(joints, углы) == TCP (example-геометрия).
                        let fwd = xyzac_forward(
                            &params,
                            &Joints5::new(end[0], end[1], end[2], rot.0, rot.1),
                        );
                        for (got, want) in [(fwd.x, tcp[0]), (fwd.y, tcp[1]), (fwd.z, tcp[2])] {
                            assert!(
                                (got - want).abs() <= 1e-2,
                                "circle: {got} != {want} (rot {rot:?})"
                            );
                        }
                        found = Some((tcp, rot, end));
                    }
                }
            }
            _ => {}
        }
    });
    let (tcp, rot, joints) = found.expect("ncm_g434 must contain a non-zero rotary frame");
    println!(
        "ncm_g434 первый ненулевой ротационный кадр: TCP {tcp:?} rot {rot:?} → joints {joints:?}"
    );
}
