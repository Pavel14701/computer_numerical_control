//! generator.rs — AST-driven генератор валидных программ Tier A v1
//! (CORPUS §2): seeded LCG-RNG (детерминированные прогоны), честный
//! Q11-связывание (start = end предыдущего motion-опа / unknown в точках
//! разрыва с полным end), resolved-state корректность (feed модален,
//! none только до первого F; G0 не сбрасывает; linear/arc несут modal feed).

use eid_core::ast::*;

/// Порог защиты arc_center (#16): tan(sweep/2) с |t| ниже порога считаем
/// вырождением (sweep→0°/360°), центр = середина хорды. 1e-4 ⇒ потолок
/// |d| = 10⁴·half; sweep=359.99° даёт |t| = 8.7e-5 < порога (см. тест).
const MIN_TAN: f64 = 1e-4;

#[derive(Debug, Clone)]
pub struct GenConfig {
    pub seed: u64,
    /// Целевое число motion-опов (без PEND; 1..=64).
    pub max_ops: usize,
    /// Вероятностные веса (тысячные доли): доля arc среди motion, dwell/stop.
    pub arc_w: u32,
    pub dwell_w: u32,
    pub stop_w: u32,
    pub wcs_switch_w: u32,
    pub preset_w: u32,
}

impl Default for GenConfig {
    fn default() -> Self {
        Self {
            seed: 1,
            max_ops: 6,
            arc_w: 300,
            dwell_w: 100,
            stop_w: 60,
            wcs_switch_w: 80,
            preset_w: 300,
        }
    }
}

/// Детерминированный LCG (D.E. Knuth): не крипто, но стабилен между прогонами.
pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Self(
            seed.wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407)
                | 1,
        )
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    /// Равномерно [0, bound).
    pub fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next_u64() % bound as u64) as usize
        }
    }
    /// В тысячных долях: true с вероятностью permille/1000.
    pub fn permille(&mut self, permille: u32) -> bool {
        self.below(1000) < permille as usize
    }
}

fn num(v: f64) -> Num {
    // координаты кратны 0.25 -> канонические 4dp представимы точно
    Num::new(format!("{v:.4}"), v)
}

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3 {
        x: num(x),
        y: num(y),
        z: num(z),
    }
}

fn src(i: usize) -> SrcSpan {
    SrcSpan {
        file: format!("gen{i}.ngc"),
        line_start: 1,
        col_start: 1,
        line_end: 1,
        col_end: 1,
    }
}

fn state(feed: Option<f64>, wcs: Wcs) -> State {
    State {
        feed: feed.map(num),
        spindle: Spindle::Cw(num(8000.0)),
        coolant: Coolant::default(),
        tool: None,
        frame: Frame::Wcs,
        wcs,
    }
}

/// Согласованный центр дуги в плоскости Z=const: c = mid + perp·(L/2)/tan(sweep/2),
/// perp для dir=Ccw — слева от вектора S→E, для Cw — справа (normal +Z).
///
/// #16 (clamp знаменателя): при sweep→360° tan(sweep/2)→0 и d = half/tan
/// улетало в ±∞ (координаты центра ~1e16 и хуже). Порог [`MIN_TAN`]: при
/// |t| < 1e-4 (sweep ≈ 0° или ≈ 360° — дуга с почти полным оборотом при
/// ненулевой хорде геометрически нечестива) центр = середина хорды; при
/// |t| ≥ 1e-4 ограничение |d| ≤ half/MIN_TAN = 10⁴·half выполняется
/// автоматически. Координаты центра конечны при любом входе (включая
/// NaN/±∞ sweep). Генератор сам такие sweep не выдаёт (фиксированный набор
/// [90°, 180°, 270°]); порог — страховка pub-функции (прямые вызовы,
/// будущие расширения GenConfig).
pub fn arc_center(sx: f64, sy: f64, ex: f64, ey: f64, sweep_deg: f64, dir: Dir) -> (f64, f64) {
    let dx = ex - sx;
    let dy = ey - sy;
    let l2 = (dx * dx + dy * dy).sqrt();
    if l2 < 1e-9 {
        return (sx + 2.5, sy); // вырожденный случай: центр рядом со стартом (R=2.5)
    }
    let half = l2 / 2.0;
    let rad = sweep_deg.to_radians();
    let t = (rad / 2.0).tan();
    // #16: |t| ниже порога (sweep≈0/360°) или tan выродился в NaN/±∞
    // (|sweep|→inf) → центр в середине хорды, а не деление с улетанием.
    let d = if !t.is_finite() || t.abs() < MIN_TAN {
        0.0
    } else {
        half / t // |t| ≥ MIN_TAN ⇒ |d| ≤ half/MIN_TAN = 10⁴·half
    };
    // перпендикуляр единичной длины: ccw — слева, cw — справа
    let (px, py) = if dir == Dir::Ccw {
        (-dy / l2, dx / l2)
    } else {
        (dy / l2, -dx / l2)
    };
    let mx = (sx + ex) / 2.0;
    let my = (sy + ey) / 2.0;
    (mx + px * d, my + py * d)
}

/// Сгенерировать валидную программу (SPEC §3/§4/§6, Q11 честно).
pub fn generate(cfg: &GenConfig) -> Program {
    let mut rng = Lcg::new(cfg.seed);
    let mut wcs = Wcs::G54;
    let mut feed: Option<f64> = None; // modal: none до первого F; G0 не сбрасывает
    let mut cur = (0.0_f64, 0.0_f64, 5.0_f64);
    let mut start_known = false;

    let mut ops: Vec<Op> = Vec::new();
    let preset = rng.permille(cfg.preset_w);
    if preset {
        // пресетка: G92 читает текущую позицию как (px,py,pz) -> первый ход Known
        let (px, py, pz) = (0.0, 0.0, num_range(&mut rng, 2.0, 6.0));
        ops.push(Op::WcsSetHere {
            p: 1,
            delta: p3(px, py, pz),
            state: state(None, wcs),
            src: src(ops.len()),
        });
        cur = (px, py, pz);
        start_known = true; // якорь L20 той же WCS: reading = delta
    }

    let n_motion = 1 + rng.below(cfg.max_ops.max(1));
    for _ in 0..n_motion {
        let start = if start_known {
            StartPoint::Known(p3(cur.0, cur.1, cur.2))
        } else {
            StartPoint::Unknown
        };

        // редкая смена wcs: точка разрыва — start Unknown, end обязан быть полным
        if (start_known && rng.permille(cfg.wcs_switch_w)) || rng.permille(150) {
            if start_known || rng.permille(300) {
                // гарантированная смена на ДРУГУЮ СК (та же = фиктивная смена,
                // unknown вне точки разрыва — SPEC §2 v0.9)
                let others = [Wcs::G54, Wcs::G55, Wcs::G56]
                    .iter()
                    .copied()
                    .filter(|w| *w != wcs)
                    .collect::<Vec<_>>();
                wcs = others[rng.below(others.len())];
            }
            let end = rand_end(&mut rng, cur);
            ops.push(Op::Rapid {
                start: StartPoint::Unknown,
                end: p3(end.0, end.1, end.2),
                state: state(feed, wcs),
                src: src(ops.len()),
                sub: None,
            });
            cur = end;
            start_known = true;
            continue;
        }

        let end = rand_end(&mut rng, cur);
        let roll = rng.below(1000);

        if roll < cfg.arc_w as usize
            && feed.is_some()
            && start_known
            && (end.0 - cur.0).abs() + (end.1 - cur.1).abs() > 0.5
        {
            // плоская дуга XY (Z не меняется: pitch = 0, §6.2), центр согласован §6.3
            let sweeps = [90.0_f64, 180.0, 270.0];
            let sweep = sweeps[rng.below(3)];
            let dir = if rng.permille(500) { Dir::Ccw } else { Dir::Cw };
            let (cx, cy) = arc_center(cur.0, cur.1, end.0, end.1, sweep, dir);
            ops.push(Op::Arc {
                start,
                end: p3(end.0, end.1, cur.2),
                center: p3(cx, cy, cur.2),
                normal: p3(0.0, 0.0, 1.0),
                dir,
                sweep_deg: num(sweep),
                pitch: num(0.0),
                state: state(feed, wcs),
                src: src(ops.len()),
                sub: None,
            });
            cur = (end.0, end.1, cur.2);
            continue;
        }
        if roll < (cfg.arc_w + cfg.dwell_w) as usize {
            ops.push(Op::Dwell {
                seconds: num(rng.below(20) as f64 / 10.0 + 0.1),
                state: state(feed, wcs),
                src: src(ops.len()),
                sub: None,
            });
            continue; // dwell не двигает позицию
        }
        if roll < (cfg.arc_w + cfg.dwell_w + cfg.stop_w) as usize {
            ops.push(Op::ProgramStop {
                optional: rng.permille(500),
                state: state(feed, wcs),
                src: src(ops.len()),
            });
            continue; // stop не двигает позицию
        }
        let is_linear = rng.permille(600);
        if is_linear {
            if feed.is_none() {
                feed = Some(num_range(&mut rng, 80.0, 300.0));
            }
            ops.push(Op::Linear {
                start,
                end: p3(end.0, end.1, end.2),
                state: state(feed, wcs),
                src: src(ops.len()),
                sub: None,
            });
        } else {
            ops.push(Op::Rapid {
                start,
                end: p3(end.0, end.1, end.2),
                state: state(feed, wcs),
                src: src(ops.len()),
                sub: None,
            });
        }
        cur = end;
        start_known = true;
    }

    ops.push(Op::ProgramEnd {
        src: src(ops.len()),
    });
    Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: PathMode::ExactPath,
        frame_preset: None,
        ops,
    }
}

fn num_range(rng: &mut Lcg, lo: f64, hi: f64) -> f64 {
    let steps = ((hi - lo) / 0.5).round() as usize;
    lo + (rng.below(steps + 1) as f64) * 0.5
}

fn rand_end(rng: &mut Lcg, cur: (f64, f64, f64)) -> (f64, f64, f64) {
    // сдвиг на случайную полусетку в радиусе ~10, не совпадающий с cur
    let mut e = cur;
    for _ in 0..8 {
        e = (
            num_range(rng, cur.0 - 10.0, cur.0 + 10.0),
            num_range(rng, cur.1 - 10.0, cur.1 + 10.0),
            num_range(rng, 0.0, 6.0),
        );
        if (e.0 - cur.0).abs() > 0.25 || (e.1 - cur.1).abs() > 0.25 || (e.2 - cur.2).abs() > 0.25 {
            break;
        }
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use eid_core::adapter::{Adapter, EidTextAdapter};

    #[test]
    fn generated_programs_are_valid_and_byte_stable() {
        let eid = EidTextAdapter::new();
        for seed in 1..=20u64 {
            let cfg = GenConfig {
                seed,
                ..GenConfig::default()
            };
            let p = generate(&cfg);
            let text = eid
                .emit(&p)
                .unwrap_or_else(|d| panic!("seed {seed}: emit: {d:?}"));
            let back = eid
                .parse(&text, "gen")
                .unwrap_or_else(|d| panic!("seed {seed}: parse: {d:?}"));
            assert!(
                eid.take_notices().is_empty(),
                "seed {seed}: notices — генератор обязан выдавать каноничный AST"
            );
            let text2 = eid.emit(&back).unwrap();
            assert_eq!(text, text2, "seed {seed}: emit нестабилен");
        }
    }

    #[test]
    fn determinism_same_seed_same_program() {
        let a = generate(&GenConfig {
            seed: 42,
            max_ops: 10,
            ..GenConfig::default()
        });
        let b = generate(&GenConfig {
            seed: 42,
            max_ops: 10,
            ..GenConfig::default()
        });
        assert_eq!(a, b);
        let c = generate(&GenConfig {
            seed: 43,
            max_ops: 10,
            ..GenConfig::default()
        });
        assert_ne!(a, c, "разные seed — разные программы (sanity)");
    }

    /// #16: sweep→360° не даёт inf/nan/улетания координат центра. До фикса
    /// sweep=359.99° давал d = half/tan(179.995°) ≈ -5.1e4 (и ~1e16 при
    /// ровно 360°, tan(π) = -1.2e-16 > порога 1e-9).
    #[test]
    fn arc_center_stays_finite_near_full_circle() {
        // точный воспроизводитель находки: sweep=359.99°
        let (cx, cy) = arc_center(0.0, 0.0, 8.0, 4.0, 359.99, Dir::Ccw);
        assert!(cx.is_finite() && cy.is_finite(), "({cx}, {cy})");
        assert!(
            cx.abs() < 1e6 && cy.abs() < 1e6,
            "центр улетел: ({cx}, {cy})"
        );

        // вся окрестность полного оборота + вырождения, оба направления;
        // потолок |d| = half/MIN_TAN (центр не дальше 10⁴·half от середины)
        let half = 8.0_f64.hypot(4.0) / 2.0;
        let bound = 1e4 * half + 1e-6;
        for sweep in [
            0.0, 0.001, 45.0, 90.0, 180.0, 270.0, 359.0, 359.9, 359.99, 359.999, 359.9999, 360.0,
            360.001, 400.0,
        ] {
            for dir in [Dir::Ccw, Dir::Cw] {
                let (cx, cy) = arc_center(0.0, 0.0, 8.0, 4.0, sweep, dir);
                assert!(
                    cx.is_finite() && cy.is_finite(),
                    "sweep={sweep} dir={dir:?}: центр ({cx}, {cy})"
                );
                let d = ((cx - 4.0).powi(2) + (cy - 2.0).powi(2)).sqrt();
                assert!(d <= bound, "sweep={sweep} dir={dir:?}: |d|={d} > {bound}");
            }
        }

        // NaN/±∞ sweep не отравляют результат (центр = середина хорды)
        for sweep in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let (cx, cy) = arc_center(0.0, 0.0, 8.0, 4.0, sweep, Dir::Cw);
            assert!(
                cx.is_finite() && cy.is_finite(),
                "sweep={sweep}: ({cx}, {cy})"
            );
        }
    }

    /// #16: генерация с дугой sweep=359.99 (через pub arc_center) даёт
    /// конечные координаты, пригодные для Num-эмиссии (без inf/nan в тексте).
    #[test]
    fn generated_arc_359_99_center_representable() {
        let (cx, cy) = arc_center(0.0, 0.0, 6.0, 2.0, 359.99, Dir::Ccw);
        assert!(cx.is_finite() && cy.is_finite());
        // num() форматирует как {v:.4}: строка не должна содержать inf/nan
        let s = format!("{cx:.4}{cy:.4}");
        assert!(!s.contains("inf") && !s.contains("NaN"), "{s}");
    }
}
