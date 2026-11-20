//! sim — верификация программ, итерация 4 (IMPLEMENTATION-PLAN §2-E).
//!
//! Стратегия двух уровней:
//!  (a) **mock-sim** — обязателен для MVP-acceptance (критерий #8 «или mock
//!      с пометкой»): обратный parse эмитированного текста адаптером диалекта
//!      профиля (parse(emit(parse(f))) — R1-машина) + structural-проверки:
//!      program_end presence (ровно один, последний — SPEC §3), zero-length
//!      flood (эмиссия не должна реанимировать отброшенное), ходы в пределах
//!      `MachineProfile::axis_limits` (если заданы).
//!  (б) **real LinuxCNC sim** (rs274ngc) — детекция наличия в системе через
//!      [`real_sim_available`]; запуск/контейнер — решение после MVP. Если
//!      недоступен — «mock-only MVP» в REPORT (см. CLI verify).

use crate::profile::MachineProfile;
use crate::translate::AnyAdapter;
use eid_core::adapter::Adapter;
use eid_core::ast::{Op, Program, StartPoint};
use eid_core::diag::Diagnostic;

/// Порог «нулевой длины» для structural-проверки (не допуск эквивалентности!):
/// точное равенство после 4dp-эмиссии, порог — защита от float-шума.
const ZERO_LEN_EPS: f64 = 1e-9;

/// Bounding box опорных точек motion-опов: концы + центры дуг (консервативно;
/// экстремумы дуг между опорными точками не вычисляются — структурный MVP,
/// точная геометрия — real sim). unknown-start (Q11) — не входит.
#[derive(Debug, Clone, PartialEq)]
pub struct Bbox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

/// Отчёт симуляции: ошибки (fail), предупреждения (инфо о потерях), число
/// опсов, bbox. `errors` пуст ⇒ sim принял программу.
#[derive(Debug, Clone, Default)]
pub struct SimReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub ops_count: usize,
    pub bbox: Option<Bbox>,
}

impl SimReport {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Structural-проверки на AST
// ---------------------------------------------------------------------------

/// Структурные проверки разобранной программы (без обратного parse):
/// program_end presence, zero-length flood, axis limits.
pub fn check_program(profile: &MachineProfile, program: &Program) -> SimReport {
    let mut report = SimReport {
        ops_count: program.ops.len(),
        ..SimReport::default()
    };

    // --- program_end: ровно один и последний (SPEC §3) ----------------------
    let end_count = program
        .ops
        .iter()
        .filter(|op| matches!(op, Op::ProgramEnd { .. }))
        .count();
    match end_count {
        0 => report
            .errors
            .push("program_end отсутствует (SPEC §3: ровно один, последний)".to_string()),
        1 => {
            if !matches!(program.ops.last(), Some(Op::ProgramEnd { .. })) {
                report
                    .errors
                    .push("program_end не последний оп (SPEC §3)".to_string());
            }
        }
        n => report
            .errors
            .push(format!("program_end продублирован: {n} шт (SPEC §3: ровно один)")),
    }

    // --- motion-опы: zero-length flood + axis limits + bbox -----------------
    let mut zero_len = 0usize;
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut any_point = false;
    for (i, op) in program.ops.iter().enumerate() {
        let (start, end, center) = match op {
            Op::Rapid { start, end, .. } => (start, end, None),
            Op::Linear { start, end, .. } => (start, end, None),
            Op::Arc {
                start, end, center, ..
            } => (start, end, Some(center)),
            _ => continue,
        };
        let (sx, sy, sz) = point(start);
        let (ex, ey, ez) = (end.x.value(), end.y.value(), end.z.value());
        if let StartPoint::Known(s) = start {
            let dx = s.x.value() - ex;
            let dy = s.y.value() - ey;
            let dz = s.z.value() - ez;
            if dx * dx + dy * dy + dz * dz < ZERO_LEN_EPS * ZERO_LEN_EPS {
                zero_len += 1;
            }
        }
        // axis limits: концы + центр дуги (консервативно; экстремумы дуги — real sim)
        let mut points: Vec<(f64, f64, f64)> = vec![(sx, sy, sz), (ex, ey, ez)];
        if let Some(c) = center {
            points.push((c.x.value(), c.y.value(), c.z.value()));
        }
        for (x, y, z) in points {
            // unknown-start (Q11) даёт NaN — символическое значение, вне
            // bbox/лимитов не участвует.
            if x.is_nan() || y.is_nan() || z.is_nan() {
                continue;
            }
            min[0] = min[0].min(x);
            min[1] = min[1].min(y);
            min[2] = min[2].min(z);
            max[0] = max[0].max(x);
            max[1] = max[1].max(y);
            max[2] = max[2].max(z);
            any_point = true;
            if let Some(limits) = &profile.axis_limits {
                if !limits.contains(x, y, z) {
                    report.errors.push(format!(
                        "axis_limits: ход вне лимитов профиля (op {i}): x={x} y={y} z={z}"
                    ));
                }
            }
        }
    }
    if zero_len > 0 {
        report.warnings.push(format!(
            "zero-length flood: {zero_len} нулевых ходов в эмиссии (SPEC §3: drop-with-notice)"
        ));
    }
    if any_point {
        report.bbox = Some(Bbox { min, max });
    }
    report
}

// ---------------------------------------------------------------------------
// mock-sim: обратный parse + structural-проверки
// ---------------------------------------------------------------------------

/// Mock-симуляция (MVP, критерий #8 — с пометкой «mock»): текст диалекта
/// профиля → обратный parse адаптером диалекта → structural-проверки.
/// Ошибки обратного parse (или валидации eid_core) = ошибки sim.
pub fn mock_sim(profile: &MachineProfile, emitted: &str, file: &str) -> SimReport {
    let mut adapter = match AnyAdapter::for_name(profile.dialect.as_name()) {
        Ok(a) => a,
        Err(_) => {
            // Профиль ссылается на диалект без адаптера (grbl-hal/fluidnc) —
            // обратный parse невозможен, structural-проверки недоступны.
            return SimReport {
                errors: vec![format!(
                    "mock-sim: адаптер диалекта {} недоступен (pending C)",
                    profile.dialect.as_name()
                )],
                warnings: Vec::new(),
                ops_count: 0,
                bbox: None,
            };
        }
    };
    let program = match adapter.parse(emitted, file) {
        Ok(p) => p,
        Err(diags) => {
            return SimReport {
                errors: diags.iter().map(crate::translate::format_diagnostic).collect(),
                warnings: adapter.take_notices(),
                ops_count: 0,
                bbox: None,
            };
        }
    };
    let mut report = check_program(profile, &program);
    let notices = adapter.take_notices();
    if !notices.is_empty() {
        report
            .warnings
            .push(format!("re-parse notices: {}", notices.join("; ")));
    }
    report
}

/// eid_core-валидация AST (замороженный парсер, агент A) — используется
/// CLI verify после parse.
pub fn validate_program(program: &Program) -> Result<(), Vec<Diagnostic>> {
    eid_core::parser::validate(program)
}

// ---------------------------------------------------------------------------
// real sim (rs274ngc) — детекция наличия
// ---------------------------------------------------------------------------

/// Есть ли LinuxCNC/rs274 в системе (PATH)? Запуск — отдельное решение после
/// MVP (ини-файл, контейнер); false ⇒ «mock-only MVP» в REPORT.
pub fn real_sim_available() -> bool {
    let names: &[&str] = if cfg!(windows) {
        &["linuxcnc.exe", "rs274.exe", "linuxcnc", "rs274"]
    } else {
        &["linuxcnc", "rs274"]
    };
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        names
            .iter()
            .any(|n| dir.join(n).is_file())
    })
}

fn point(start: &StartPoint) -> (f64, f64, f64) {
    match start {
        StartPoint::Known(p) => (p.x.value(), p.y.value(), p.z.value()),
        // unknown-start (Q11): в bbox/лимиты не входит — символическое значение.
        StartPoint::Unknown => (f64::NAN, f64::NAN, f64::NAN),
    }
}
