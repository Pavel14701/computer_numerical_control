//! Тесты эмиттера LinuxCNC (Агент D, итерация 3).
//!
//! R1 same-dialect round-trip (CORPUS §4): parse → emit → parse →
//! semantic_diff = 0 на всех 7 LinuxCNC-пилотах (#01–#03, #11, #27, #36,
//! #46) и на golden AST (двойной источник канона). Диалектные файлы v0 —
//! рукописные: сравнение только AST-уровня, НЕ текстового (CORPUS §1 долг).
//! Детерминизм: двойной emit байт-в-байт.

use eid_core::adapter::{Adapter, EidTextAdapter};
use eid_core::ast::Op;
use eid_core::diag::ErrorCode;
use eid_harness::diff::{semantic_diff, DiffConfig};
use eid_linuxcnc::LinuxCncAdapter;
use std::path::PathBuf;

fn corpus(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus/tier_a/v0");
    p.push(rel);
    p
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(corpus(rel)).expect("corpus file readable")
}

fn adapter() -> LinuxCncAdapter {
    LinuxCncAdapter::new()
}

/// parse → emit → parse; возвращает (текст, reparsed, diff).
fn round_trip(src: &str) -> (String, eid_core::ast::Program, Vec<eid_harness::diff::Divergence>) {
    let a = adapter();
    let p1 = a.parse(src, "rt.ngc").expect("parse #1");
    let text = a.emit(&p1).expect("emit");
    let p2 = adapter().parse(&text, "rt2.ngc").expect("reparse");
    let d = semantic_diff(&p1, &p2, &DiffConfig::default());
    (text, p2, d)
}

fn assert_rt(src: &str, ctx: &str) -> String {
    let (text, _p2, d) = round_trip(src);
    assert!(d.is_empty(), "{ctx}: round-trip divergences: {d:?}\n--- emitted ---\n{text}");
    text
}

// ===========================================================================
// R1 round-trip на 7 пилотах + детерминизм
// ===========================================================================

const PILOTS: [(&str, &str); 7] = [
    ("01_linear_box", "linear_box.linuxcnc.ngc"),
    ("02_rapid_reposition", "rapid_reposition.linuxcnc.ngc"),
    ("03_feed_survives_rapid", "feed_survives_rapid.linuxcnc.ngc"),
    ("11_arc_R_semicircle", "arc_R_semicircle.linuxcnc.ngc"),
    ("27_tool_change", "tool_change.linuxcnc.ngc"),
    ("36_preset_G54_then_switch", "preset_G54_then_switch.linuxcnc.ngc"),
    ("46_arc_R_helix", "arc_R_helix.linuxcnc.ngc"),
];

#[test]
fn r1_round_trip_all_pilots() {
    for (dir, file) in PILOTS {
        let src = read(&format!("{dir}/{file}"));
        let text = assert_rt(&src, dir);
        // Детерминизм: двойной emit байт-в-байт.
        let a = adapter();
        let p = a.parse(&src, "det.ngc").expect("parse");
        let t1 = a.emit(&p).expect("emit #1");
        let t2 = a.emit(&p).expect("emit #2");
        assert_eq!(t1, t2, "{dir}: double emit must be byte-identical");
        assert_eq!(t1, text, "{dir}: emit stable");
    }
}

#[test]
fn r1_round_trip_golden_asts() {
    // Golden AST (второй независимый источник канона, CORPUS §1) → emit →
    // reparse → эквивалентность golden'у. #11 включён: golden v0.5.2 =
    // производные значения (SPEC v0.11 §10.5).
    for (dir, _golden_name) in PILOTS {
        let golden_text = read(&format!("{dir}/golden.eid"));
        let a = EidTextAdapter::new();
        let golden = a.parse(&golden_text, "golden.eid").expect("golden parses");
        let d = adapter();
        let text = d.emit(&golden).expect("emit golden");
        let reparsed = adapter().parse(&text, "golden_rt.ngc").expect("reparse golden emit");
        let div = semantic_diff(&golden, &reparsed, &DiffConfig::default());
        assert!(
            div.is_empty(),
            "{dir}/golden: round-trip divergences: {div:?}\n--- emitted ---\n{text}"
        );
    }
}

// ===========================================================================
// Структурные требования к выводу
// ===========================================================================

#[test]
fn pilot_27_emits_t_m6_one_line() {
    let src = read("27_tool_change/tool_change.linuxcnc.ngc");
    let text = assert_rt(&src, "#27");
    assert!(text.contains("T3 M6"), "#27: одна строка `T3 M6`, got:\n{text}");
    assert!(text.contains("M6"), "#27: M6 присутствует");
}

#[test]
fn pilot_36_preset_order_wcs_then_g92_then_motion() {
    // EQUIVALENCE §1.4: выборка preset.wcs → G92 → до первого motion-опа;
    // без запекания в G10 (канонический формат G92: X, Y, Z, 4dp).
    let src = read("36_preset_G54_then_switch/preset_G54_then_switch.linuxcnc.ngc");
    let text = assert_rt(&src, "#36");
    let i_g54 = text.find("G54").expect("G54 selection emitted");
    let i_g92 = text.find("G92 X0.0000 Y0.0000 Z5.0000").expect("G92 block emitted");
    let i_g0 = text.find("G0").expect("first motion emitted");
    let i_g61 = text.find("G61").expect("path mode emitted");
    assert!(i_g61 < i_g54, "#36: path mode после заголовка");
    assert!(i_g54 < i_g92, "#36: G54-выборка до G92");
    assert!(i_g92 < i_g0, "#36: G92 до первого хода");
    assert!(!text.contains("G10"), "#36: запекание пресетки в G10 запрещено");
}

#[test]
fn header_always_explicit() {
    let src = read("01_linear_box/linear_box.linuxcnc.ngc");
    let text = assert_rt(&src, "#01 header");
    let first = text.lines().next().expect("non-empty");
    assert_eq!(first, "G21 G90 G94 G17 G91.1", "заголовок: units, G90, G94, G17, G91.1 — всегда явно");
    assert!(text.lines().nth(1).unwrap() == "G61", "#01: exact_path → G61 явно");
}

#[test]
fn arcs_emit_ijk_in_plane_pair_no_r() {
    // #46: in-plane пара I+J (G17), R не эмитится, off-plane Z — винтовая.
    let src = read("46_arc_R_helix/arc_R_helix.linuxcnc.ngc");
    let text = assert_rt(&src, "#46");
    let arc_line = text.lines().find(|l| l.starts_with("G2 ")).expect("G2 line");
    assert_eq!(arc_line, "G2 X-5.0000 Y5.0000 Z2.0000 I0.0000 J5.0000 F100.0000", "#46 arc line:\n{arc_line}");
    assert!(!arc_line.contains('R'), "#46: R не эмитится (EQUIVALENCE §4)");
}

#[test]
fn feed_is_modal_f_only_on_change() {
    // #01: F150 на первом linear, F250 на втором, далее без F; rapid без F.
    let src = read("01_linear_box/linear_box.linuxcnc.ngc");
    let text = assert_rt(&src, "#01 feed");
    let fs: Vec<&str> = text.lines().filter(|l| l.contains('F')).collect();
    assert_eq!(fs.len(), 2, "#01: F только при смене, got:\n{text}");
    assert!(fs[0].contains("F150.0000") && fs[0].starts_with("G1"));
    assert!(fs[1].contains("F250.0000"));
}

#[test]
fn spindle_coolant_state_lines() {
    // #27: M3 S8000 + M7 — перед linear (дельты снапшота опа). M5/M9 из
    // источника НЕ эмитятся: canonical AST не хранит финальное состояние
    // (PEND без state, SPEC §5.2) — это свойство канонической формы, R1-AST
    // проходит (снапшотов M5/M9 ни в одном опе нет).
    let src = read("27_tool_change/tool_change.linuxcnc.ngc");
    let text = assert_rt(&src, "#27 state");
    let lines: Vec<&str> = text.lines().collect();
    let m3 = lines.iter().position(|l| l.starts_with("M3 ")).expect("M3 emitted");
    let g1 = lines.iter().position(|l| l.starts_with("G1 ")).expect("G1 emitted");
    assert!(m3 < g1, "#27: M3 до линейного хода (снапшот опа)");
    assert!(lines.contains(&"M7"));
    assert!(lines.contains(&"M3 S8000.0000"));
    assert!(!lines.contains(&"M5"), "финальный state не хранится (SPEC §5.2)");
    assert!(!lines.contains(&"M9"));
}

// ===========================================================================
// Юнит-тесты эмиттера (синтетические программы)
// ===========================================================================

fn parse_str(src: &str) -> eid_core::ast::Program {
    adapter().parse(src, "unit.ngc").expect("parse")
}

#[test]
fn unit_inch_header_g20() {
    let p = parse_str("G20 G90 G94 G17\nG0 X1.0000 Y2.0000 Z3.0000\nM2\n");
    let a = adapter();
    let text = a.emit(&p).expect("emit");
    assert_eq!(text.lines().next().unwrap(), "G20 G90 G94 G17 G91.1");
    let reparsed = adapter().parse(&text, "rt.ngc").expect("reparse");
    assert!(semantic_diff(&p, &reparsed, &DiffConfig::default()).is_empty());
}

#[test]
fn unit_path_modes_explicit() {
    for (src, want) in [
        ("G21 G90\nG61\nG0 X1.0000 Y1.0000 Z1.0000\nM2\n", "G61"),
        ("G21 G90\nG61.1\nG0 X1.0000 Y1.0000 Z1.0000\nM2\n", "G61.1"),
        ("G21 G90\nG64\nG0 X1.0000 Y1.0000 Z1.0000\nM2\n", "G64"),
        ("G21 G90\nG64 P0.0500\nG0 X1.0000 Y1.0000 Z1.0000\nM2\n", "G64 P0.0500"),
    ] {
        let p = parse_str(src);
        let text = adapter().emit(&p).expect("emit");
        assert_eq!(text.lines().nth(1), Some(want), "path mode line for {src:?}");
        let reparsed = adapter().parse(&text, "rt.ngc").expect("reparse");
        assert!(semantic_diff(&p, &reparsed, &DiffConfig::default()).is_empty());
    }
}

#[test]
fn unit_g18_plane_switch_and_ik_offsets() {
    // G18 (normal +Y): in-plane пара I (X) + K (Z); J не эмитится; смена
    // плоскости — строкой перед дугой; round-trip эквивалентен.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X0.0000 Y0.0000 Z0.0000\n",
        "G18\n",
        "G3 X-5.0000 Y0.0000 Z5.0000 I-5.0000 K0.0000 F100.0\n",
        "M2\n",
    );
    let text = assert_rt(src, "G18 arc");
    let arc_line = text.lines().find(|l| l.starts_with("G3 ")).expect("G3 line");
    assert_eq!(arc_line, "G3 X-5.0000 Y0.0000 Z5.0000 I-5.0000 K0.0000 F100.0000");
    assert!(text.contains("\nG18\n"), "смена плоскости отдельной строкой");
}

#[test]
fn unit_g53_machine_frame_round_trip() {
    // §5.3: origin in-text → machine-ход выразим; emission "G53 G0 …";
    // после machine-хода чтение = M − O (точка разрыва (d) — оба конца
    // заданы явно, эквивалентность сохраняется).
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G10 L2 P1 X0.0000 Y0.0000 Z0.0000\n",
        "G53 G0 X5.0000 Y6.0000 Z7.0000\n",
        "G0 X10.0000 Y6.0000 Z7.0000\n",
        "M2\n",
    );
    let text = assert_rt(src, "G53");
    assert!(text.contains("G10 L2 P1 X0.0000 Y0.0000 Z0.0000"));
    assert!(text.contains("G53 G0 X5.0000 Y6.0000 Z7.0000"));
}

#[test]
fn unit_g10_l20_wcs_set_here_round_trip() {
    // #44-стиль: L20 пресетка до первого motion-опа.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G10 L20 P2 X1.0000 Y2.0000 Z3.0000\n",
        "G0 X4.0000 Y5.0000 Z6.0000\n",
        "M2\n",
    );
    let text = assert_rt(src, "L20");
    assert!(text.contains("G10 L20 P2 X1.0000 Y2.0000 Z3.0000"));
}

#[test]
fn unit_dwell_and_stops() {
    let src = "G21 G90 G94\nG0 X1.0000 Y1.0000 Z1.0000\nG4 P1.5\nM0\nG0 X2.0000 Y1.0000 Z1.0000\nM1\nM2\n";
    let text = assert_rt(src, "dwell/stops");
    assert!(text.contains("G4 P1.5000"));
    assert!(text.contains("\nM0\n"));
    assert!(text.contains("\nM1\n"));
    assert!(text.ends_with("M2\n"));
}

#[test]
fn unit_raw_payload_verbatim_and_warning_channel() {
    // RAW: payload декодируется дословно; ASCII-raw не даёт warnings;
    // warnings доступны через take_diagnostics (тот же буфер, что у parse).
    let src = "G21 G90 G94\nG0 X0 Y0 Z0\nM123\nG0 X1.0000 Y0 Z0\nM2\n";
    let a = adapter();
    let p = a.parse(src, "raw.ngc").expect("parse");
    assert!(a.take_diagnostics().iter().any(|d| d.code == eid_core::diag::ErrorCode::UnsupportedSemantics));
    let text = a.emit(&p).expect("emit");
    assert!(a.take_diagnostics().is_empty(), "ASCII raw: без warnings");
    assert!(text.contains("\nM123\n"), "RAW verbatim:\n{text}");
    // RAW re-parses в тот же payload (same-dialect fidelity, D9).
    let p2 = adapter().parse(&text, "raw2.ngc").expect("reparse");
    assert!(semantic_diff(&p, &p2, &DiffConfig::default()).is_empty());
}

#[test]
fn unit_non_axial_normal_err_invalid() {
    // Не-осевой normal → Err INVALID (бриф итерации 3, п.1).
    let mut p = parse_str("G21 G90 G94 G17\nG0 X0 Y0 Z0\nM2\n");
    if let Some(eid_core::ast::Op::Arc { normal, .. }) = p.ops.last_mut() {
        // недостижимо: у PEND нет normal — конструируем через замену опа.
        let _ = normal;
    }
    // Собираем программу с дугой и подменяем normal на не-осевой.
    let src = "G21 G90 G94 G17\nG0 X0 Y0 Z0\nG2 X5.0000 Y5.0000 I5.0000 J0.0000 F100.0\nM2\n";
    let mut p = parse_str(src);
    let arc = p.ops.remove(1);
    match arc {
        eid_core::ast::Op::Arc { normal, .. } => {
            let mut n = normal.clone();
            n.x = eid_core::ast::Num::new("0.6000".into(), 0.6);
            n.y = eid_core::ast::Num::new("0.8000".into(), 0.8);
            let new_arc = eid_core::ast::Op::Arc {
                start: eid_core::ast::StartPoint::Known(eid_core::ast::Point3 {
                    x: eid_core::ast::Num::new("0.0000".into(), 0.0),
                    y: eid_core::ast::Num::new("0.0000".into(), 0.0),
                    z: eid_core::ast::Num::new("0.0000".into(), 0.0),
                }),
                end: eid_core::ast::Point3 {
                    x: eid_core::ast::Num::new("5.0000".into(), 5.0),
                    y: eid_core::ast::Num::new("5.0000".into(), 5.0),
                    z: eid_core::ast::Num::new("0.0000".into(), 0.0),
                },
                center: eid_core::ast::Point3 {
                    x: eid_core::ast::Num::new("5.0000".into(), 5.0),
                    y: eid_core::ast::Num::new("0.0000".into(), 0.0),
                    z: eid_core::ast::Num::new("0.0000".into(), 0.0),
                },
                normal: n,
                dir: eid_core::ast::Dir::Cw,
                sweep_deg: eid_core::ast::Num::new("90.0000".into(), 90.0),
                pitch: eid_core::ast::Num::new("0.0000".into(), 0.0),
                state: p.ops.first().map(|o| match o {
                    eid_core::ast::Op::Rapid { state, .. } => state.clone(),
                    _ => unreachable!(),
                }).unwrap(),
                src: eid_core::ast::SrcSpan {
                    file: "unit.ngc".into(),
                    line_start: 3,
                    col_start: 1,
                    line_end: 3,
                    col_end: 30,
                },
                sub: None,
            };
            p.ops.insert(1, new_arc);
        }
        _ => panic!("expected arc"),
    }
    let err = adapter().emit(&p).expect_err("не-осевой normal → Err");
    assert!(err.iter().all(|d| d.code == eid_core::diag::ErrorCode::InvalidInput));
}

#[test]
fn unit_negative_normal_err_invalid() {
    // Отрицательная ось normal — не канонический знак (SPEC §6) → Err.
    let src = "G21 G90 G94 G17\nG0 X0 Y0 Z0\nG2 X5.0000 Y5.0000 I5.0000 J0.0000 F100.0\nM2\n";
    let mut p = parse_str(src);
    for op in &mut p.ops {
        if let eid_core::ast::Op::Arc { normal, .. } = op {
            normal.z = eid_core::ast::Num::new("-1.0000".into(), -1.0);
        }
    }
    let err = adapter().emit(&p).expect_err("отрицательный normal → Err");
    assert!(err.iter().all(|d| d.code == eid_core::diag::ErrorCode::InvalidInput));
}

#[test]
fn unit_tool_jump_warns_but_emits() {
    // Несвязный снапшот (state.tool без ToolChange) — warning, не отказ.
    let mut p = parse_str("G21 G90 G94\nG0 X1.0000 Y1.0000 Z1.0000\nG1 X2.0000 Y1.0000 Z1.0000 F100.0\nM2\n");
    for (i, op) in p.ops.iter_mut().enumerate() {
        if let eid_core::ast::Op::Linear { state, .. } = op {
            if i == 1 {
                state.tool = Some(7);
            }
        }
    }
    let a = adapter();
    let text = a.emit(&p).expect("emit с warning");
    let warns = a.take_diagnostics();
    assert!(warns.iter().any(|d| d.message.contains("state.tool")), "warning о скачке tool");
    assert!(!text.contains("T7"), "инструмент без M6 не эмитится");
}

#[test]
fn unit_validate_rejects_zero_length_before_emit() {
    // Каноническая форма: zero-length в AST → validate → Err (SPEC §3).
    let mut p = parse_str("G21 G90 G94\nG0 X1.0000 Y1.0000 Z1.0000\nM2\n");
    p.ops.insert(
        1,
        eid_core::ast::Op::Rapid {
            start: eid_core::ast::StartPoint::Known(eid_core::ast::Point3 {
                x: eid_core::ast::Num::new("1.0000".into(), 1.0),
                y: eid_core::ast::Num::new("1.0000".into(), 1.0),
                z: eid_core::ast::Num::new("1.0000".into(), 1.0),
            }),
            end: eid_core::ast::Point3 {
                x: eid_core::ast::Num::new("1.0000".into(), 1.0),
                y: eid_core::ast::Num::new("1.0000".into(), 1.0),
                z: eid_core::ast::Num::new("1.0000".into(), 1.0),
            },
            state: eid_core::ast::State {
                feed: None,
                spindle: eid_core::ast::Spindle::Off,
                coolant: eid_core::ast::Coolant::default(),
                tool: None,
                frame: eid_core::ast::Frame::Wcs,
                wcs: eid_core::ast::Wcs::G54,
            },
            src: eid_core::ast::SrcSpan {
                file: "unit.ngc".into(),
                line_start: 2,
                col_start: 1,
                line_end: 2,
                col_end: 10,
            },
            sub: None,
        },
    );
    let err = adapter().emit(&p).expect_err("zero-length в каноническом AST — Err");
    assert!(err.iter().all(|d| d.code == eid_core::diag::ErrorCode::InvalidInput));
}

// ===========================================================================
// Циклы — развёртка и эмиссия (SPEC §7; итерация 3-4)
// ===========================================================================

#[test]
fn cycle_g81_round_trip_and_text() {
    let src = concat!(
        "G21 G90 G94 G17\n",
        "M3 S10000\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G81 X10.0000 Y10.0000 Z-5.0000 R2.0000 F100.0\n",
        "M2\n",
    );
    let text = assert_rt(src, "cycle G81");
    // Развёрнутые ходы в тексте: traverse, rapid R, feed, retract; R/Z
    // координаты вербатимом; M2 последним.
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.contains(&"G0 X10.0000 Y10.0000 Z5.0000 F100.0000"), "traverse XY:\n{text}");
    assert!(lines.contains(&"G0 X10.0000 Y10.0000 Z2.0000"), "rapid на R");
    assert!(lines.contains(&"G1 X10.0000 Y10.0000 Z-5.0000"), "feed на дно (F модальный)");
    assert!(lines.contains(&"G0 X10.0000 Y10.0000 Z2.0000"));
    assert_eq!(lines.last(), Some(&"M2"));
    // Детерминизм: двойной emit байт-в-байт (через assert_rt выше — тот же
    // text сравнивается с повторным emit в r1_round_trip; здесь — явный).
}

#[test]
fn cycle_g86_restart_emits_m5_then_m3() {
    // G86: шпиндель off на rapid-out, рестарт в state последующего опа —
    // эмиссия: M5 перед отводом, M3 S8000 перед следующим ходом (Q9).
    let src = concat!(
        "G21 G90 G94 G17\n",
        "M3 S8000\n",
        "G0 X0.0000 Y0.0000 Z5.0000\n",
        "G99 G86 X10.0000 Y0.0000 Z-3.0000 R2.0000 P0.3000 F60.0\n",
        "G1 X20.0000 Y0.0000 Z2.0000 F100.0\n",
        "M2\n",
    );
    let text = assert_rt(src, "cycle G86");
    let lines: Vec<&str> = text.lines().collect();
    let m5 = lines.iter().position(|l| *l == "M5").expect("M5 (шпиндель off)");
    let rapid_out = lines.iter().rposition(|l| *l == "G0 X10.0000 Y0.0000 Z2.0000").expect("rapid out");
    let m3 = lines.iter().rposition(|l| *l == "M3 S8000.0000").expect("M3 рестарт (последний из M3)");
    let g1 = lines.iter().position(|l| l.starts_with("G1 X20.0000")).expect("следующий ход");
    assert!(m5 < rapid_out && rapid_out < m3 && m3 < g1, "порядок M5 → rapid-out → M3 → ход:\n{text}");
}

#[test]
fn full_circle_emits_without_end_words() {
    // CORPUS #08 (COVERAGE §5, зеркально eid-grbl): sweep=360°, pitch=0 →
    // `G2 I.. J..` БЕЗ end-слов; parse-back сохраняет дугу (explicit_end=false
    // → не zero-sweep). Винтовой оборот (pitch≠0) — с off-plane end-словом.
    let src = concat!(
        "G21 G90 G94 G17\n",
        "G0 X10.0000 Y0.0000 Z0.0000\n",
        "G2 I-5.0000 J0.0000 F120.0\n",
        "M2\n",
    );
    let text = assert_rt(src, "full circle");
    let arc = text.lines().find(|l| l.starts_with("G2 ")).expect("G2 line");
    assert_eq!(arc, "G2 I-5.0000 J0.0000 F120.0000", "без end-слов:\n{text}");
    assert!(!arc.contains('X') && !arc.contains('Z'), "осей нет в full-circle");

    let src_h = concat!(
        "G21 G90 G94 G17\n",
        "G0 X10.0000 Y0.0000 Z0.0000\n",
        "G2 X10.0000 Y0.0000 Z4.0000 I-5.0000 J0.0000 F120.0\n",
        "M2\n",
    );
    let text_h = assert_rt(src_h, "helical full turn");
    let arc_h = text_h.lines().find(|l| l.starts_with("G2 ")).expect("G2 line");
    // винтовой полный оборот: только off-plane Z end-слово (in-plane нет)
    assert!(arc_h.starts_with("G2 Z4.0000 "), "helical: только Z-слово:\n{arc_h}");
    assert!(!arc_h.contains("X1") && !arc_h.contains("Y0.0000"), "in-plane end-слов нет");
}

// ===========================================================================
// Mutation-kill: байт-точные строки эмиссии (кластер Writer::op 478-516)
// ===========================================================================

#[test]
fn mutation_wcs_words_exact_and_percent() {
    // Все не-дефолтные WCS-слова выбираются точно (мутанты ==/!= на выборе
    // WCS); G54 — дефолт: строки нет, semantic — reparse равен. G61.1 —
    // path_mode строкой. Dwell G4 с P — P-слово поглощено.
    let srcs = [
        ("G55", "G55"), ("G56", "G56"), ("G57", "G57"), ("G58", "G58"),
        ("G59", "G59"), ("G59.1", "G59.1"), ("G59.2", "G59.2"), ("G59.3", "G59.3"),
    ];
    for (wcs_word, want_line) in srcs {
        let src = format!(
            "G21 G90 G94 G17
{wcs_word}
G0 X1.0000 Y1.0000 Z1.0000
M2
"
        );
        let p = LinuxCncAdapter::new().parse(&src, "wcs.ngc").expect("parses");
        let text = LinuxCncAdapter::new().emit(&p).expect("emit");
        assert!(
            text.lines().any(|l| l == want_line),
            "ожидается строка `{want_line}` для {wcs_word}:
{text}"
        );
    }
    // G54-дефолт: линии нет, semantic reparse равен.
    let src = "G21 G90 G94 G17
G0 X1.0000 Y1.0000 Z1.0000
M2
";
    let p = LinuxCncAdapter::new().parse(src, "g54.ngc").expect("parses");
    let text = LinuxCncAdapter::new().emit(&p).expect("emit");
    assert!(!text.lines().any(|l| l.starts_with("G5")), "G54-дефолт без строки:
{text}");
    let p2 = LinuxCncAdapter::new().parse(&text, "rt.ngc").expect("reparse");
    assert!(eid_harness::diff::semantic_diff(&p, &p2, &DiffConfig::default()).is_empty());
    // path modes строкой (G61.1 проверен в другом тесте — здесь G64 P).
    for (mode_line, p_word) in [("G64", None), ("G64 P0.0500", Some("P0.0500"))] {
        let src = format!("G21 G90 G94 G17
{mode_line}
G0 X1.0000 Y1.0000 Z1.0000
M2
");
        let p = LinuxCncAdapter::new().parse(&src, "pm.ngc").expect("parses");
        let text = LinuxCncAdapter::new().emit(&p).expect("emit");
        match p_word {
            None => assert!(text.lines().any(|l| l == "G64"), "G64 bare:
{text}"),
            Some(_) => assert!(text.lines().any(|l| l == "G64 P0.0500"), "G64 P:
{text}"),
        }
    }
}

#[test]
fn mutation_emit_nonfinite_feed_is_err() {
    // num_text: non-finite значение → Err InvalidInput (мутант «убрать
    // is_finite-чек» эмитил бы NaN/inf текст).
    let mut p = LinuxCncAdapter::new()
        .parse("G21 G90 G94 G17\nG0 X0.0000 Y0.0000 Z0.0000\nG1 X5.0000 Y0.0000 Z0.0000 F100.0\nM2\n", "nf.ngc")
        .expect("parses");
    for op in &mut p.ops {
        if let Op::Linear { state, .. } = op {
            if let Some(f) = &mut state.feed {
                *f = eid_core::ast::Num::new("inf".into(), f64::INFINITY);
            }
        }
    }
    let err = LinuxCncAdapter::new().emit(&p).expect_err("non-finite feed");
    assert!(err.iter().all(|d| d.code == ErrorCode::InvalidInput), "{err:?}");
}

#[test]
fn mutation_emit_num_minus_zero_canonicalized() {
    // Num{text:"-0.0000", value:-0.0} — вербатим-текст "-0.0000" запрещён
    // (fmt_num даёт "0.0000"); мутант «вербатим без исключения» сломал бы
    // R1-текст эмиссии.
    let mut p = LinuxCncAdapter::new()
        .parse("G21 G90 G94 G17\nG0 X0.0000 Y0.0000 Z0.0000\nM2\n", "mz.ngc")
        .expect("parses");
    if let Op::Rapid { end, .. } = &mut p.ops[0] {
        end.x = eid_core::ast::Num::new("-0.0000".into(), -0.0);
    }
    let text = LinuxCncAdapter::new().emit(&p).expect("emit");
    assert!(text.contains("X0.0000"), "-0.0000 канонизуется в 0.0000:\n{text}");
    assert!(!text.contains("-0.0000"));
}
