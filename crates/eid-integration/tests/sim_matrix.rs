//! Sim-матрица (Агент E, итерация 5): mock-sim / check_program на
//! hand-built AST (в обход парсера — целевые ветки structural-проверок) +
//! обратный parse корпусного эмита. Именование sim_<case>.
//!
//! real_sim_available() == false задокументировано: на dev-машине и CI
//! LinuxCNC/rs274 отсутствует → «mock-only MVP» (REPORT, PLAN §2-E).

use eid_core::ast::{
    Coolant, Dir, Frame, Num, Op, Point3, Program, Spindle, SrcSpan, StartPoint, State, Units, Wcs,
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
    Point3::xyz(num(x), num(y), num(z))
}

fn state() -> State {
    State {
        feed: None,
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        tool: None,
        frame: Frame::Wcs,
        wcs: Wcs::G54,
        feed_restated: false,
    }
}

fn src() -> SrcSpan {
    SrcSpan {
        file: "fixture".into(),
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
        z: AxisRange { min: z.0, max: z.1 },
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
    let p = program(vec![rapid(StartPoint::Unknown, 0.0, 0.0, 5.0), end_op()]);
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
    let p = program(vec![rapid(StartPoint::Unknown, 0.0, 0.0, 5.0), end_op()]);
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
    let p = program(vec![end_op(), rapid(StartPoint::Unknown, 0.0, 0.0, 5.0)]);
    let report = check_program(&profile_with_limits(None), &p);
    assert!(!report.is_ok());
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains("program_end не последний")));
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
    assert!(
        report.is_ok(),
        "zero-length — не error: {:?}",
        report.errors
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("zero-length flood")),
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

// ---------------------------------------------------------------------------
// #27: units в axis_limits — масштабирование координат перед сравнением
// ---------------------------------------------------------------------------

/// Профиль с лимитами в заданных единицах (axis_limits_units); диапазон
/// (min, max) — один на все три оси (для юнит-тестов масштаба #27).
fn profile_in_units(units: Units, range: (f64, f64)) -> MachineProfile {
    let mut p = profile_with_limits(None);
    p.axis_limits = Some(AxisLimits {
        x: AxisRange {
            min: range.0,
            max: range.1,
        },
        y: AxisRange {
            min: range.0,
            max: range.1,
        },
        z: AxisRange {
            min: range.0,
            max: range.1,
        },
    });
    p.axis_limits_units = units;
    p
}

/// Программа в заданных единицах: rapid (0,0,5) → linear (30,0,5) → end.
fn program_in_units(units: Units) -> Program {
    let mut p = program(vec![
        rapid(StartPoint::Unknown, 0.0, 0.0, 5.0),
        linear(known((0.0, 0.0, 5.0)), 30.0, 0.0, 5.0),
        end_op(),
    ]);
    p.units = units;
    p
}

#[test]
fn sim_units_inch_program_mm_profile_762_ok() {
    // мм-профиль (axis_limits_units = mm, лимит z[0,1000]) + inch-программа:
    // X30 = 762 мм → в пределах → ok. y=0, z=5 inch = 127 мм → ок.
    let report = check_program(
        &profile_in_units(Units::Mm, (0.0, 1000.0)),
        &program_in_units(Units::In),
    );
    assert!(
        report.is_ok(),
        "762 мм < 1000 мм: errors: {:?}",
        report.errors
    );
    // bbox в единицах ПРОФИЛЯ: x = 30 inch × 25.4 = 762
    let b = report.bbox.expect("bbox");
    assert!((b.max[0] - 762.0).abs() < 1e-9, "bbox x max: {}", b.max[0]);
}

#[test]
fn sim_units_inch_program_mm_profile_500_refusal() {
    // тот же inch-файл против лимита 500 мм → отказ; сообщение несёт note
    // о приведении единиц.
    let report = check_program(
        &profile_in_units(Units::Mm, (0.0, 500.0)),
        &program_in_units(Units::In),
    );
    assert!(!report.is_ok(), "762 мм > 500 мм обязан отказать");
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("axis_limits") && e.contains("25.4")),
        "note о масштабе в ошибке: {:?}",
        report.errors
    );
}

#[test]
fn sim_units_mm_program_inch_profile() {
    // наоборот: inch-профиль (лимит x/y/z [0, 60] inch) + мм-программа:
    // X30 мм = 1.1811 inch → в пределах → ok; z 5 мм = 0.1969 inch → ок.
    let report = check_program(
        &profile_in_units(Units::In, (0.0, 60.0)),
        &program_in_units(Units::Mm),
    );
    assert!(
        report.is_ok(),
        "30 мм = 1.18 inch < 60 inch: errors: {:?}",
        report.errors
    );

    // против лимита 1.0 inch (25.4 мм): X30 мм = 1.181 inch > 1.0 → отказ.
    let report = check_program(
        &profile_in_units(Units::In, (0.0, 1.0)),
        &program_in_units(Units::Mm),
    );
    assert!(!report.is_ok(), "30 мм против 1 inch обязан отказать");
}

#[test]
fn sim_units_match_untouched() {
    // одинаковые единицы — масштаб 1.0, поведение как до #27: z=5 против
    // лимита z[0,4] → отказ (числа не пересчитываются).
    let report = check_program(
        &profile_in_units(Units::Mm, (0.0, 4.0)),
        &program_in_units(Units::Mm),
    );
    assert!(!report.is_ok());
    let report = check_program(
        &profile_in_units(Units::In, (0.0, 4.0)),
        &program_in_units(Units::In),
    );
    assert!(!report.is_ok());
    // и Inch-программа против Inch-лимитов [0,100] — ok
    let report = check_program(
        &profile_in_units(Units::In, (0.0, 100.0)),
        &program_in_units(Units::In),
    );
    assert!(report.is_ok(), "errors: {:?}", report.errors);
}
