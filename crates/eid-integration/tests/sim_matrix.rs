//! Sim-матрица (Агент E, итерация 5): mock-sim / check_program на
//! hand-built AST (в обход парсера — целевые ветки structural-проверок) +
//! обратный parse корпусного эмита. Именование sim_<case>.
//!
//! real_sim_available() == false задокументировано: на dev-машине и CI
//! LinuxCNC/rs274 отсутствует → «mock-only MVP» (REPORT, PLAN §2-E).

use eid_core::ast::{
    Coolant, Dir, Frame, Num, Op, Point3, Program, SrcSpan, Spindle, StartPoint, State, Units, Wcs,
};
use eid_integration::profile::{AxisLimits, AxisRange, MachineProfile};
use eid_integration::sim::{check_program, mock_sim, real_sim_available};
use std::path::Path;

// ---------------------------------------------------------------------------
// AST-фикстуры
// ---------------------------------------------------------------------------

fn num(v: f64) -> Num {
    Num {
        text: format!("{v:.4}"),
        value: v,
    }
}

fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3 {
        x: num(x),
        y: num(y),
        z: num(z),
    }
}

fn state() -> State {
    State {
        feed: None,
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        tool: None,
        frame: Frame::Wcs,
        wcs: Wcs::G54,
    }
}

fn src() -> SrcSpan {
    SrcSpan {
        file: "fixture".to_string(),
        line_start: 1,
        col_start: 1,
        line_end: 1,
        col_end: 1,
    }
}

fn program(ops: Vec<Op>) -> Program {
    Program {
        eid_version: "EID/L1 v0.3".to_string(),
        units: Units::Mm,
        path_mode: eid_core::ast::PathMode::ExactPath,
        frame_preset: None,
        ops,
    }
}

fn rapid(start: StartPoint, x: f64, y: f64, z: f64) -> Op {
    Op::Rapid {
        start,
        end: pt(x, y, z),
        state: state(),
        src: src(),
        sub: None,
    }
}

fn linear(start: StartPoint, x: f64, y: f64, z: f64) -> Op {
    Op::Linear {
        start,
        end: pt(x, y, z),
        state: state(),
        src: src(),
        sub: None,
    }
}

fn known(p: (f64, f64, f64)) -> StartPoint {
    StartPoint::Known(pt(p.0, p.1, p.2))
}

fn end_op() -> Op {
    Op::ProgramEnd { src: src() }
}

fn profile_with_limits(limits: Option<AxisLimits>) -> MachineProfile {
    let mut p = MachineProfile::linuxcnc_default();
    p.axis_limits = limits;
    p
}

fn limits(z: (f64, f64)) -> Option<AxisLimits> {
    Some(AxisLimits {
        x: AxisRange {
            min: -100.0,
            max: 100.0,
        },
        y: AxisRange {
            min: -100.0,
            max: 100.0,
        },
        z: AxisRange {
            min: z.0,
            max: z.1,
        },
    })
}

// ---------------------------------------------------------------------------
// mock-sim: Ok-программа
// ---------------------------------------------------------------------------

#[test]
fn sim_mock_ok_pilot_output() {
    // обратный parse корпусного эмита (пилот #01, linuxcnc) — sim принимает
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/tier_a/v0/01_linear_box/linear_box.linuxcnc.ngc")
            .canonicalize()
            .unwrap(),
    )
    .unwrap();
    let report = mock_sim(&MachineProfile::linuxcnc_default(), &text, "pilot.ngc");
    assert!(report.is_ok(), "errors: {:?}", report.errors);
    assert!(report.ops_count > 0);
    assert!(report.bbox.is_some());
}

#[test]
fn sim_check_ok_bbox() {
    // unknown-start rapid → (0,0,5) → linear (10,0,5) → end
    let p = program(vec![
        rapid(StartPoint::Unknown, 0.0, 0.0, 5.0),
        linear(known((0.0, 0.0, 5.0)), 10.0, 0.0, 5.0),
        end_op(),
    ]);
    let report = check_program(&profile_with_limits(limits((0.0, 150.0))), &p);
    assert!(report.is_ok(), "errors: {:?}", report.errors);
    assert_eq!(report.ops_count, 3);
    let b = report.bbox.expect("bbox");
    assert_eq!(b.min, [0.0, 0.0, 5.0]);
    assert_eq!(b.max, [10.0, 0.0, 5.0]);
}

// ---------------------------------------------------------------------------
// axis_limits: за max / за min
// ---------------------------------------------------------------------------

#[test]
fn sim_axis_out_max() {
    let p = program(vec![
        rapid(StartPoint::Unknown, 0.0, 0.0, 5.0),
        end_op(),
    ]);
    let report = check_program(&profile_with_limits(limits((0.0, 4.0))), &p);
    assert!(!report.is_ok());
    assert!(
        report
            .errors
            .iter()
            .all(|e| e.starts_with("[verify] ") && e.contains("axis_limits")),
        "тег/текст ошибок: {:?}",
        report.errors
    );
}

#[test]
fn sim_axis_out_min() {
    let p = program(vec![
        rapid(StartPoint::Unknown, 0.0, 0.0, 5.0),
        end_op(),
    ]);
    let report = check_program(&profile_with_limits(limits((6.0, 150.0))), &p);
    assert!(!report.is_ok());
    assert!(report.errors.iter().any(|e| e.contains("axis_limits")));
}

// ---------------------------------------------------------------------------
// program_end: missing / not-last / duplicate
// ---------------------------------------------------------------------------

#[test]
fn sim_end_missing() {
    let p = program(vec![
        rapid(StartPoint::Unknown, 0.0, 0.0, 5.0),
        linear(known((0.0, 0.0, 5.0)), 10.0, 0.0, 5.0),
    ]);
    let report = check_program(&profile_with_limits(None), &p);
    assert!(!report.is_ok());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.starts_with("[verify]") && e.contains("program_end отсутствует")),
        "{:?}",
        report.errors
    );
}

#[test]
fn sim_end_not_last() {
    let p = program(vec![
        end_op(),
        rapid(StartPoint::Unknown, 0.0, 0.0, 5.0),
    ]);
    let report = check_program(&profile_with_limits(None), &p);
    assert!(!report.is_ok());
    assert!(report.errors.iter().any(|e| e.contains("program_end не последний")));
}

#[test]
fn sim_end_duplicate() {
    let p = program(vec![end_op(), end_op()]);
    let report = check_program(&profile_with_limits(None), &p);
    assert!(!report.is_ok());
    assert!(report.errors.iter().any(|e| e.contains("продублирован")));
}

// ---------------------------------------------------------------------------
// zero-length flood: warning, не отказ
// ---------------------------------------------------------------------------

#[test]
fn sim_zero_flood_warning() {
    // start == end (нулевой ход) — если он прорвался в AST, это баг эмиссии:
    // структурная проверка даёт warning, ошибкой не является
    let p = program(vec![
        rapid(known((0.0, 0.0, 5.0)), 0.0, 0.0, 5.0),
        linear(known((0.0, 0.0, 5.0)), 10.0, 0.0, 5.0),
        end_op(),
    ]);
    let report = check_program(&profile_with_limits(None), &p);
    assert!(report.is_ok(), "zero-length — не error: {:?}", report.errors);
    assert!(
        report.warnings.iter().any(|w| w.contains("zero-length flood")),
        "warnings: {:?}",
        report.warnings
    );
}

// ---------------------------------------------------------------------------
// axis_limits None = проверка отключена
// ---------------------------------------------------------------------------

#[test]
fn sim_axis_none_off() {
    let p = program(vec![
        rapid(StartPoint::Unknown, 99999.0, -99999.0, 5000.0),
        end_op(),
    ]);
    let report = check_program(&profile_with_limits(None), &p);
    assert!(report.is_ok(), "None = off: {:?}", report.errors);
    assert!(report.bbox.is_some()); // bbox считается, но не проверяется
}

// ---------------------------------------------------------------------------
// mock-sim с axis-профилем на корпусном эмите → [verify] ошибки
// ---------------------------------------------------------------------------

#[test]
fn sim_mock_axis_out() {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/tier_a/v0/01_linear_box/linear_box.linuxcnc.ngc")
            .canonicalize()
            .unwrap(),
    )
    .unwrap();
    let report = mock_sim(&profile_with_limits(limits((0.0, 4.0))), &text, "pilot.ngc");
    assert!(!report.is_ok());
    assert!(report.errors.iter().all(|e| e.starts_with("[verify]")));
}

// ---------------------------------------------------------------------------
// real sim: детекция (задокументировано: LinuxCNC нет → mock-only MVP)
// ---------------------------------------------------------------------------

#[test]
fn sim_real_sim_unavailable_documented() {
    // dev-машина и CI без LinuxCNC/rs274: стратегия (б) PLAN §2-E —
    // real sim недоступен, MVP принимается с mock (REPORT).
    assert!(
        !real_sim_available(),
        "в среде появился linuxcnc/rs274 — пересмотреть стратегию sim (PLAN §2-E, REPORT)"
    );
}

// ---------------------------------------------------------------------------
// sweep/dir-фикстура: дуга не валит structural-проверки (центр в лимитах)
// ---------------------------------------------------------------------------

#[test]
fn sim_arc_center_in_limits() {
    let op = Op::Arc {
        start: known((10.0, 0.0, 5.0)),
        end: pt(0.0, 10.0, 5.0),
        center: pt(0.0, 0.0, 5.0),
        normal: pt(0.0, 0.0, 1.0),
        dir: Dir::Ccw,
        sweep_deg: num(90.0),
        pitch: num(0.0),
        state: state(),
        src: src(),
        sub: None,
    };
    let p = program(vec![
        rapid(StartPoint::Unknown, 10.0, 0.0, 5.0),
        op,
        end_op(),
    ]);
    let report = check_program(&profile_with_limits(limits((0.0, 150.0))), &p);
    assert!(report.is_ok(), "errors: {:?}", report.errors);
    // bbox консервативно включает центр дуги
    let b = report.bbox.expect("bbox");
    assert_eq!(b.max, [10.0, 10.0, 5.0]);
    assert_eq!(b.min, [0.0, 0.0, 5.0]);
}
