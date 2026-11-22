//! EID/L2 — Pass-1 lowering L2 → L1 (этап 12.2, агент A).
//!
//! МЕХАНИКА МАТРИЦЫ §3 дизайн-дока `EID-L2-DESIGN.md` v0.1 (+амендрт v0.1.1):
//! * табличная строка 1 (`#100 = 5, G0 X#100` → `Op::Rapid X5.0000`) —
//!   интерпретатор [`crate::interp::L2Evaluator`] (D0-разрешение);
//! * строка 2 (WHILE/IF/GOTO/G65 развёртка со стеком #1–#33) — там же
//!   (D1-кадры, D2-CFG);
//! * строка 3 (G68/G51 → `Op::Linear`/`Op::Arc` в базовой СК) —
//!   [`Affine3`] ниже (композиция трансформаций, точки/векторы/центры);
//! * строка 4 (G51.1/G101 зеркало: дефолт-отказ / `allow_mirror_bake` +
//!   Notice + Cw↔Ccw) — [`crate::interp`] (решение на WithTransform),
//!   флаг — [`crate::interp::L2Config::allow_mirror_bake`] (конфиг-структура
//!   lowering, НЕ MachineProfile — по заданию 12.2);
//! * строка 5 (G0 A#100 → `Op::Raw` с payload `G0 A5.0000\n`) —
//!   [`format_frame`]/[`payload_hex`]/[`raw_frame_op`]: payload — hex
//!   ASCII-текста кадра с подставленными значениями, 4dp, терминатор `\n`
//!   (чек-лист T4 ДОСЛОВНО: `G0 X15.0000 A15.0000\n`);
//! * строка 6 (G43.4/G68.2 → Raw с подставленными переменными) — там же;
//! * строка 7 (G31/чтение SkipPos → останов E_RUNTIME_STATE, программа
//!   остаётся в L2) — [`crate::interp::L2ExecError::Eval`]-останов +
//!   [`MachineTableRequired`](crate::interp::L2ExecError::MachineTableRequired)
//!   для WCS-таблиц ([`SysVar::WcsOffset`]).
//!
//! CFG (D2): таблицы меток N — [`collect_labels`] (скан узлов
//! [`crate::ast::L2Stmt::Label`]) + [`resolve_labels`] (слив с таблицей
//! парсера [`crate::ast::L2Program::labels`; парсер имеет приоритет).
//!
//! Точка входа конвейера: [`lower`] (сигнатура из заготовки 12.1).
//!
//! ЦЕНЗ-B (честно): в корпусе `WithTransform ×0` (G68/G51 нет в живых
//! файлах) — трансформации покрыты СИНТЕТИЧЕСКИМИ golden-тестами с ручным
//! расчётом (поворот 90° вокруг (10,10): (15,10) → (10,15) и т.п.);
//! корпусного драйвера для transform-lowering нет.

use crate::ast::{L2Program, L2Stmt, Plane, SubTarget};
use crate::interp::{L2Config, L2Evaluator, L2ExecError, LoweringResult};
use eid_core::ast::{Dir, Op, SrcSpan};
use eid_core::diag::ErrorCode;
use std::collections::BTreeMap;

// NOTE (гонка волн): механический алиас агента D2
// (`use crate::eval::L2Error as L2ExecError` — заглушка, пока
// interp::L2ExecError не существовал) ЗАМЕНЁН каноническим типом из
// crate::interp — как и было оговорено в их комментарии.

// ===========================================================================
// Точка входа Pass-1 (сигнатура — из заготовки 12.1)
// ===========================================================================

/// Pass-1: символьно-конкретное исполнение L2-программы с лимитом шагов
/// ([`L2Config`]) → [`LoweringResult`] (L1-опы + хвост носителей RAW +
/// диагностики). Ошибки исполнения/классы — [`L2ExecError`].
pub fn lower(program: &L2Program, cfg: L2Config) -> Result<LoweringResult, L2ExecError> {
    L2Evaluator::new(cfg).run(program)
}

// ===========================================================================
// D2: таблицы меток N (контракт-решение для парсеров B2/D2)
// ===========================================================================

/// Pre-pass: собрать `N<метка> → индекс stmt` по узлам [`L2Stmt::Label`]
/// (позиция = индекс узла Label; исполнение Label — no-op, переход
/// продолжает С него). Тела циклов/трансформаций образуют СОБСТВЕННЫЕ
/// области меток — функция вызывается per-body (см. precompute в
/// [`crate::interp::L2Evaluator::run`]).
///
/// КОНТРАКТ ПАРСЕРОВ (B2/D2, решение (а) из задания 12.2): N-метки,
/// достижимые GOTO/M97, не отбрасываются — носитель либо узлы
/// [`L2Stmt::Label`] (эта функция), либо таблица
/// [`L2Program::labels`] главного блока (парсер заполняет сам;
/// при конфликте таблица парсера имеет приоритет — [`resolve_labels`]).
pub fn collect_labels(stmts: &[L2Stmt]) -> BTreeMap<u32, usize> {
    let mut map = BTreeMap::new();
    for (i, stmt) in stmts.iter().enumerate() {
        if let L2Stmt::Label { n, .. } = stmt {
            map.insert(*n, i);
        }
    }
    map
}

/// Таблица меток ГЛАВНОГО блока: [`collect_labels`] по `program.stmts`,
/// поверх — `program.labels` парсера (приоритет: явная таблица парсера
/// выигрывает при конфликте — парсер знает полный контекст кадра).
pub fn resolve_labels(program: &L2Program) -> BTreeMap<u32, usize> {
    let mut map = collect_labels(&program.stmts);
    for (n, idx) in &program.labels {
        map.insert(*n, *idx);
    }
    map
}

/// Поиск подпрограммы по цели (обёртка для сообщений об ошибке —
/// SubTarget::Named не Copy).
pub fn sub_target_name(t: &SubTarget) -> String {
    match t {
        SubTarget::ONumber(n) => format!("O{n}"),
        SubTarget::NLabel(n) => format!("N{n}"),
        SubTarget::Named(name) => format!("o<{name}>"),
    }
}

// ===========================================================================
// Аффинные трансформации (§3, строка 3): G68/G51/G51.1
// ===========================================================================

/// Аффинное преобразование 3D: `x ↦ m·x + t`. Только композиции
/// Rotate2D-в-плоскости / Scale-uniform / Mirror-по-оси (§2.3) —
/// обобщённой кинематики (RTCP/TWP) здесь нет (Pass-1 их не опускает).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine3 {
    m: [[f64; 3]; 3],
    t: [f64; 3],
}

impl Default for Affine3 {
    fn default() -> Self {
        Self::identity()
    }
}

impl Affine3 {
    /// Тождественное.
    pub fn identity() -> Self {
        Self {
            m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            t: [0.0; 3],
        }
    }

    /// Применить к ТОЧКЕ (линейная часть + трансляция).
    pub fn apply_point(&self, p: [f64; 3]) -> [f64; 3] {
        let mut out = [0.0; 3];
        for (r, row) in self.m.iter().enumerate() {
            out[r] = row[0] * p[0] + row[1] * p[1] + row[2] * p[2] + self.t[r];
        }
        out
    }

    /// Применить к ВЕКТОРУ СМЕЩЕНИЯ (только линейная часть — IJK-центры
    /// дуг задаются смещениями от старта: поворот/масштаб вектора без
    /// переноса; зеркало переворачивает компоненты).
    pub fn apply_vec(&self, v: [f64; 3]) -> [f64; 3] {
        let mut out = [0.0; 3];
        for (r, row) in self.m.iter().enumerate() {
            out[r] = row[0] * v[0] + row[1] * v[1] + row[2] * v[2];
        }
        out
    }

    /// Определитель линейной части: < 0 ⇔ ориентация перевёрнута
    /// (зеркало / отрицательный масштаб) → направление дуг Cw↔Ccw (§3
    /// строка 4, T6).
    pub fn det(&self) -> f64 {
        let m = &self.m;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    /// Композиция: `self.compose(inner) == "сначала inner, затем self"` —
    /// стек трансформаций интерпретатора: активная = внешняя ∘ новая
    /// (G51 внутри G68: точка сначала масштабируется в повёрнутой системе,
    /// затем результат поворачивается — семантика вложенных кадров G-кода).
    pub fn compose(&self, inner: &Affine3) -> Affine3 {
        let mut m = [[0.0f64; 3]; 3];
        for (r, row) in m.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = self.m[r][0] * inner.m[0][c]
                    + self.m[r][1] * inner.m[1][c]
                    + self.m[r][2] * inner.m[2][c];
            }
        }
        let t = self.apply_point(inner.t);
        Affine3 { m, t }
    }

    /// G68: поворот на `angle_deg` (CCW-положительный) вокруг `center`
    /// в плоскости `plane`. Пары осей — канонические (LinuxCNC/grbl):
    /// XY → (X,Y), ZX → (Z,X), YZ → (Y,Z); угловая база — первая ось пары.
    pub fn rotate_plane(plane: Plane, center: [f64; 2], angle_deg: f64) -> Affine3 {
        let (a0, a1) = plane_pair(plane);
        let rad = angle_deg.to_radians();
        let (c, s) = (rad.cos(), rad.sin());
        // В паре (a0, a1): [p0'; p1'] = [[c, -s], [s, c]] · [p0-c0; p1-c1] + center.
        let mut m = Affine3::identity().m;
        m[a0][a0] = c;
        m[a0][a1] = -s;
        m[a1][a0] = s;
        m[a1][a1] = c;
        let mut t = [0.0f64; 3];
        t[a0] = center[0] - (c * center[0] - s * center[1]);
        t[a1] = center[1] - (s * center[0] + c * center[1]);
        Affine3 { m, t }
    }

    /// G51: равномерный масштаб `factor` вокруг `center` (3D).
    /// `factor == 0` — вырожденный (всё сжимается в точку): отклоняется
    /// вызывающей стороной (RuntimeError); `factor < 0` — допускается
    /// (det < 0 → переворот направления дуг).
    pub fn scale_uniform(center: [f64; 3], factor: f64) -> Affine3 {
        let mut a = Affine3::identity();
        // Диагональ m + трансляция t; r — индекс строки (итератор по строкам).
        for (r, ((m_row, t_cell), c)) in
            a.m.iter_mut()
                .zip(a.t.iter_mut())
                .zip(center.iter())
                .enumerate()
        {
            m_row[r] = factor;
            *t_cell = c - factor * c;
        }
        a
    }

    /// G51.1/G101: зеркало по оси `axis` относительно `pivot`
    /// (координата по оси: x ↦ 2·pivot − x). det = −1 → Cw↔Ccw (T6).
    pub fn mirror_axis(axis: usize, pivot: f64) -> Affine3 {
        let mut a = Affine3::identity();
        a.m[axis][axis] = -1.0;
        a.t[axis] = 2.0 * pivot;
        a
    }
}

/// Пара осей плоскости (a0, a1) — канонические пары LinuxCNC/grbl
/// (G17 = XY, G18 = ZX, G19 = YZ); сверено с eid-fanuc::state::plane_axes.
pub fn plane_pair(plane: Plane) -> (usize, usize) {
    match plane {
        Plane::XY => (0, 1),
        Plane::ZX => (2, 0),
        Plane::YZ => (1, 2),
    }
}

/// Внеплоскостной индекс (ось винтовой компоненты/нормали).
pub fn plane_linear(plane: Plane) -> usize {
    match plane {
        Plane::XY => 2,
        Plane::ZX => 1,
        Plane::YZ => 0,
    }
}

/// Нормаль активной плоскости — канонический +axis (SPEC §6):
/// XY → +Z, ZX → +Y, YZ → +X.
pub fn plane_normal(plane: Plane) -> [f64; 3] {
    let mut n = [0.0f64; 3];
    n[plane_linear(plane)] = 1.0;
    n
}

// ===========================================================================
// Геометрия дуги → Op::Arc (IJK/R-формы, канонические соглашения eid-core)
// ===========================================================================

/// Вычисленная геометрия дуги в базовой СК (поля — как в eid-core Op::Arc).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcGeom {
    pub center: [f64; 3],
    pub normal: [f64; 3],
    /// Положительная величина в градусах, (0°, 360°] — канон
    /// eid-core (dir несёт направление).
    pub sweep_deg: f64,
    /// Внеплоскостная компонента хода (винт): end[al] − start[al].
    pub pitch: f64,
}

/// Причины отказа геометрии дуги (носитель RAW с кодом на месте).
#[derive(Debug, Clone, PartialEq)]
pub enum ArcGeomError {
    /// Внеплоскостная компонента IJK ≠ 0 — центр вне стартовой плоскости.
    OffPlaneComponent { letter: char, value: f64 },
    /// Ни IJK-пары в плоскости, ни R — геометрия не задана.
    NoGeometry,
    /// Хорда > 2R — невозможная дуга (SPEC §6).
    ChordTooLarge { chord: f64, r: f64 },
    /// R-форма при совпадающих in-plane start/end — центр неоднозначен.
    RAmbiguousFullCircle,
}

/// IJK-форма: center = start + in-plane смещения; смещения задаются
/// БУКВАМИ I↔X, J↔Y, K↔Z (`offs[0..3]` — значения I/J/K; отсутствующее
/// слово = 0.0, канон RS274). Sweep — по полярным углам от центра
/// (полная окружность при start == end в плоскости: (0°,360°]).
/// `offs` — УЖЕ ПРЕОБРАЗОВАННЫЕ (базовая СК) смещения.
pub fn arc_from_ijk(
    start: [f64; 3],
    end: [f64; 3],
    offs: [f64; 3],
    plane: Plane,
    dir: Dir,
) -> Result<ArcGeom, ArcGeomError> {
    let (a0, a1) = plane_pair(plane);
    let al = plane_linear(plane);
    // Валидация нормали (SPEC §4/§6): компонента вне плоскости ≈ 0.
    if offs[al].abs() > 1e-9 {
        return Err(ArcGeomError::OffPlaneComponent {
            letter: ['I', 'J', 'K'][al],
            value: offs[al],
        });
    }
    let mut center = start;
    center[a0] = start[a0] + offs[a0];
    center[a1] = start[a1] + offs[a1];
    let sweep = sweep_from_angles(start, end, center, a0, a1, dir);
    Ok(ArcGeom {
        center,
        normal: plane_normal(plane),
        sweep_deg: sweep,
        pitch: end[al] - start[al],
    })
}

/// R-форма: центр = середина хорды ± h·p̂ (знак — направление дуги + знак
/// R: R>0 ≤180°, R<0 >180°; формула LinuxCNC/grbl, сверена с
/// eid-fanuc::canonicalize). Sweep — закон хорд 2·asin(c/2R).
/// `radius` — УЖЕ ПРЕОБРАЗОВАННЫЙ (масштабированный) радиус.
pub fn arc_from_r(
    start: [f64; 3],
    end: [f64; 3],
    radius: f64,
    plane: Plane,
    dir: Dir,
) -> Result<ArcGeom, ArcGeomError> {
    let (a0, a1) = plane_pair(plane);
    let al = plane_linear(plane);
    let d0 = end[a0] - start[a0];
    let d1 = end[a1] - start[a1];
    let chord = (d0 * d0 + d1 * d1).sqrt();
    if chord < 1e-9 {
        return Err(ArcGeomError::RAmbiguousFullCircle);
    }
    if radius.abs() < 1e-12 {
        return Err(ArcGeomError::NoGeometry);
    }
    let tol_r = 0.001f64.max(1e-6 * radius.abs());
    let h2 = 4.0 * radius * radius - chord * chord;
    if h2 < -tol_r * 4.0 * radius.abs().max(1.0) {
        return Err(ArcGeomError::ChordTooLarge { chord, r: radius });
    }
    let mut sgn = if dir == Dir::Cw { -1.0 } else { 1.0 };
    if radius < 0.0 {
        sgn = -sgn;
    }
    let h = h2.max(0.0).sqrt() / 2.0;
    let (px, py) = (-d1 / chord, d0 / chord);
    let mut center = start;
    center[a0] = (end[a0] + start[a0]) / 2.0 + sgn * h * px;
    center[a1] = (end[a1] + start[a1]) / 2.0 + sgn * h * py;
    let sw = 2.0 * (chord / (2.0 * radius.abs())).clamp(0.0, 1.0).asin();
    let sweep_deg = if radius < 0.0 {
        360.0 - sw.to_degrees()
    } else {
        sw.to_degrees()
    };
    Ok(ArcGeom {
        center,
        normal: plane_normal(plane),
        sweep_deg,
        pitch: end[al] - start[al],
    })
}

/// Sweep по полярным углам (IJK-форма): (a_e − a_s) против часовой,
/// Cw → дополнение до 360°; результат в (0°, 360°] (совпадение в
/// плоскости = полная окружность — канон eid-core/SPEC §6).
fn sweep_from_angles(
    start: [f64; 3],
    end: [f64; 3],
    center: [f64; 3],
    a0: usize,
    a1: usize,
    dir: Dir,
) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    let a_s = (start[a1] - center[a1]).atan2(start[a0] - center[a0]);
    let a_e = (end[a1] - center[a1]).atan2(end[a0] - center[a0]);
    let mut sweep_ccw = (a_e - a_s).rem_euclid(two_pi);
    if sweep_ccw < 1e-9 {
        sweep_ccw = two_pi;
    }
    let sw = if dir == Dir::Cw {
        two_pi - sweep_ccw
    } else {
        sweep_ccw
    };
    let deg = sw.to_degrees();
    if deg < 1e-9 {
        360.0
    } else {
        deg
    }
}

// ===========================================================================
// Форматирование кадра-носителя (§3 строка 5, чек-лист T4 ДОСЛОВНО)
// ===========================================================================

/// Текст кадра с подставленными значениями: `code` (`"G0"`/`"G1"`/`"G2"`/…),
/// затем слова в порядке перечисления, каждое `L%.4f` через один пробел,
/// в конце терминатор `\n`. Числа — канонический 4dp
/// [`eid_core::emitter::fmt_num`] (`-0.0000` → `0.0000`). Слова передаются
/// ТОЛЬКО присутствующие в кадре (Null/отсутствующие опущены — семантика
/// адресных слов, таблица 2.1).
pub fn format_frame(code: &str, words: &[(char, f64)]) -> String {
    let mut s = String::from(code);
    for (letter, v) in words {
        s.push(' ');
        s.push(*letter);
        s.push_str(&eid_core::emitter::fmt_num(*v));
    }
    s.push('\n');
    s
}

/// Hex-кодирование payload (lossless-носитель `Op::Raw.payload_hex`):
/// детерминированный hex ASCII-байтов (включая терминатор `\n`).
pub fn payload_hex(text: &str) -> String {
    text.as_bytes().iter().map(|b| format!("{b:02X}")).collect()
}

/// Собрать `Op::Raw`-носитель из текста кадра (обратимо: hex → текст).
pub fn raw_frame_op(text: &str, reason: String, code: ErrorCode, src: SrcSpan) -> Op {
    Op::Raw {
        payload_hex: payload_hex(text),
        reason,
        code,
        src,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    // ---------- Affine3: golden-расчёты (синтетика — ценз B: WithTransform ×0) --

    #[test]
    fn rotate90_around_10_10_golden() {
        // Бриф-пин: поворот 90° вокруг (10,10): (15,10) → (10,15).
        let r = Affine3::rotate_plane(Plane::XY, [10.0, 10.0], 90.0);
        let p = r.apply_point([15.0, 10.0, 0.0]);
        assert!(approx(p[0], 10.0, 1e-9));
        assert!(approx(p[1], 15.0, 1e-9));
        // И симметричный кадр: (10,15) → (5,10).
        let p2 = r.apply_point([10.0, 15.0, 0.0]);
        assert!(approx(p2[0], 5.0, 1e-9));
        assert!(approx(p2[1], 10.0, 1e-9));
    }

    #[test]
    fn rotate45_around_origin_golden() {
        // 45° вокруг (0,0): (10,0) → (10/√2, 10/√2) ≈ (7.0711, 7.0711).
        let r = Affine3::rotate_plane(Plane::XY, [0.0, 0.0], 45.0);
        let p = r.apply_point([10.0, 0.0, 5.0]);
        assert!(approx(p[0], 7.07106781, 1e-7));
        assert!(approx(p[1], 7.07106781, 1e-7));
        assert!(approx(p[2], 5.0, 1e-12)); // внеплоскостная ось не тронута
                                           // CCW-отрицательный: −90° вокруг (0,0): (0,10) → (10,0).
        let rm = Affine3::rotate_plane(Plane::XY, [0.0, 0.0], -90.0);
        let p2 = rm.apply_point([0.0, 10.0, 0.0]);
        assert!(approx(p2[0], 10.0, 1e-9));
        assert!(approx(p2[1], 0.0, 1e-9));
    }

    #[test]
    fn rotate_zx_plane_pair_golden() {
        // G18 (ZX): пара (a0=Z, a1=X) — как в plane_axes eid-fanuc; CCW-поворот
        // в полярных координатах пары [[c,−s],[s,c]]·[p0;p1]:
        // точка (z=0, x=1) → p0' = c·0 − s·1 = −1, p1' = s·0 + c·1 ≈ 0
        // → (x≈0, z=−1) (точка на оси X уходит на −Z — согласовано с
        // atan2(a1,a0)-конвенцией свипа дуг).
        let r = Affine3::rotate_plane(Plane::ZX, [0.0, 0.0], 90.0);
        let p = r.apply_point([1.0, 7.0, 0.0]); // x=1, y=7, z=0
        assert!(approx(p[0], 0.0, 1e-9)); // x' = p1' ≈ 0
        assert!(approx(p[1], 7.0, 1e-12)); // Y не в плоскости
        assert!(approx(p[2], -1.0, 1e-9)); // z' = p0' = −1
                                           // И симметрично: точка на Z (z=1, x=0) → ось X (CCW: первая ось
                                           // пары поворачивается ко второй): p0' = c·1 = 0, p1' = s·1 = 1.
        let p2 = r.apply_point([0.0, 0.0, 1.0]);
        assert!(approx(p2[0], 1.0, 1e-9));
        assert!(approx(p2[2], 0.0, 1e-9));
    }

    #[test]
    fn scale_around_center_golden() {
        // Масштаб 2 вокруг (10,10,10): (15,10,5) → (20,10,0).
        let s = Affine3::scale_uniform([10.0, 10.0, 10.0], 2.0);
        let p = s.apply_point([15.0, 10.0, 5.0]);
        assert_eq!(p, [20.0, 10.0, 0.0]);
        // Вектор смещения масштабируется БЕЗ переноса: (3,0,0) → (6,0,0).
        let v = s.apply_vec([3.0, 0.0, 0.0]);
        assert_eq!(v, [6.0, 0.0, 0.0]);
        // det = f³.
        assert!(approx(s.det(), 8.0, 1e-12));
    }

    #[test]
    fn mirror_axis_det_and_point() {
        // Зеркало X с пивотом 5: x ↦ 10 − x; det = −1 (Cw↔Ccw-маркер).
        let m = Affine3::mirror_axis(0, 5.0);
        let p = m.apply_point([8.0, 3.0, 1.0]);
        assert_eq!(p, [2.0, 3.0, 1.0]);
        assert!(approx(m.det(), -1.0, 1e-12));
    }

    #[test]
    fn compose_order_inner_first() {
        // G51 (scale 2, origin) ВНУТРИ G68 (rotate 90°, origin): точка (1,0):
        // сначала scale → (2,0), затем rotate → (0,2). (Активная = внешняя ∘ новая.)
        let rot = Affine3::rotate_plane(Plane::XY, [0.0, 0.0], 90.0);
        let scale = Affine3::scale_uniform([0.0; 3], 2.0);
        let active = rot.compose(&scale);
        let p = active.apply_point([1.0, 0.0, 0.0]);
        assert!(approx(p[0], 0.0, 1e-9));
        assert!(approx(p[1], 2.0, 1e-9));
        // det композиции = произведение.
        assert!(approx(active.det(), rot.det() * scale.det(), 1e-9));
    }

    // ---------- Геометрия дуг (канон eid-core) ------------------------------

    #[test]
    fn arc_ijk_quarter_ccw() {
        // Старт (10,0), I-5 J0 → центр (5,0), конец (5,5), G3 (CCW):
        // sweep 90°, нормаль +Z, pitch 0.
        let g = arc_from_ijk(
            [10.0, 0.0, 2.0],
            [5.0, 5.0, 2.0],
            [-5.0, 0.0, 0.0],
            Plane::XY,
            Dir::Ccw,
        )
        .unwrap();
        assert_eq!(g.center, [5.0, 0.0, 2.0]);
        assert_eq!(g.normal, [0.0, 0.0, 1.0]);
        assert!(approx(g.sweep_deg, 90.0, 1e-9));
        assert!(approx(g.pitch, 0.0, 1e-12));
    }

    #[test]
    fn arc_ijk_full_circle_when_start_eq_end() {
        // start == end в плоскости, IJK → полная окружность 360°.
        let g = arc_from_ijk(
            [10.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [-5.0, 0.0, 0.0],
            Plane::XY,
            Dir::Ccw,
        )
        .unwrap();
        assert!(approx(g.sweep_deg, 360.0, 1e-9));
    }

    #[test]
    fn arc_ijk_cw_complement() {
        // Те же точки, G2 (CW): sweep = 360 − 90 = 270.
        let g = arc_from_ijk(
            [10.0, 0.0, 0.0],
            [5.0, 5.0, 0.0],
            [-5.0, 0.0, 0.0],
            Plane::XY,
            Dir::Cw,
        )
        .unwrap();
        assert!(approx(g.sweep_deg, 270.0, 1e-9));
    }

    #[test]
    fn arc_ijk_helical_pitch() {
        // K ≠ 0 — винт: pitch = внеплоскостная компонента хода; сама K-
        // компонента ЦЕНТРА обязана быть 0 (иначе OffPlaneComponent).
        let g = arc_from_ijk(
            [10.0, 0.0, 0.0],
            [5.0, 5.0, 3.0],
            [-5.0, 0.0, 0.0],
            Plane::XY,
            Dir::Ccw,
        )
        .unwrap();
        assert!(approx(g.pitch, 3.0, 1e-12));
        let bad = arc_from_ijk(
            [10.0, 0.0, 0.0],
            [5.0, 5.0, 3.0],
            [-5.0, 0.0, 3.0],
            Plane::XY,
            Dir::Ccw,
        );
        assert_eq!(
            bad,
            Err(ArcGeomError::OffPlaneComponent {
                letter: 'K',
                value: 3.0
            })
        );
    }

    #[test]
    fn arc_r_semicircle_and_sign() {
        // Полукруг R5: (0,0) → (10,0), CCW R>0: центр (5,0), sweep 180.
        let g = arc_from_r([0.0, 0.0, 0.0], [10.0, 0.0, 0.0], 5.0, Plane::XY, Dir::Ccw).unwrap();
        assert!(approx(g.center[0], 5.0, 1e-9));
        assert!(approx(g.center[1], 0.0, 1e-9));
        assert!(approx(g.sweep_deg, 180.0, 1e-9));
        // R<0: дуга > 180° (360 − 180 = 180 — на полукруге знак не виден,
        // берём четверть: (0,0) → (5,5), R=5 CCW R>0: sweep 90;
        // R<0 → 270.
        let q = arc_from_r([0.0, 0.0, 0.0], [5.0, 5.0, 0.0], 5.0, Plane::XY, Dir::Ccw).unwrap();
        assert!(approx(q.sweep_deg, 90.0, 1e-9));
        let qn = arc_from_r([0.0, 0.0, 0.0], [5.0, 5.0, 0.0], -5.0, Plane::XY, Dir::Ccw).unwrap();
        assert!(approx(qn.sweep_deg, 270.0, 1e-9));
        // Хорда > 2R — невозможная.
        let bad = arc_from_r([0.0, 0.0, 0.0], [10.0, 0.0, 0.0], 4.0, Plane::XY, Dir::Ccw);
        assert!(matches!(bad, Err(ArcGeomError::ChordTooLarge { .. })));
        // R при start == end — неоднозначно.
        let amb = arc_from_r([5.0, 0.0, 0.0], [5.0, 0.0, 0.0], 5.0, Plane::XY, Dir::Ccw);
        assert_eq!(amb, Err(ArcGeomError::RAmbiguousFullCircle));
    }

    // ---------- Формат кадра T4 (дословно) ----------------------------------

    #[test]
    fn t4_frame_format_verbatim() {
        // Чек-лист T4: payload «G0 X15.0000 A15.0000\n» — ДОСЛОВНО.
        let text = format_frame("G0", &[('X', 15.0), ('A', 15.0)]);
        assert_eq!(text, "G0 X15.0000 A15.0000\n");
        // Hex — обратим и детерминирован.
        let hex = payload_hex(&text);
        assert_eq!(hex, payload_hex(&text));
        let decoded: Vec<u8> = (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        assert_eq!(String::from_utf8(decoded).unwrap(), text);
        // Терминатор \n присутствует в hex-хвосте (0A).
        assert!(hex.ends_with("0A"));
        // -0.0 → канонический 0.0000 (emitter::fmt_num).
        assert_eq!(format_frame("G1", &[('Y', -0.0)]), "G1 Y0.0000\n");
        // 4dp-округление.
        assert_eq!(format_frame("G1", &[('X', 1.0 / 3.0)]), "G1 X0.3333\n");
    }

    // ---------- Таблицы меток (D2 CFG) ---------------------------------------

    #[test]
    fn collect_labels_scans_label_nodes() {
        use crate::ast::{placeholder_span, Expr};
        let stmts = vec![
            L2Stmt::Label {
                n: 10,
                src: placeholder_span(),
            },
            L2Stmt::Assign {
                target: crate::ast::VarRef::Direct(100),
                value: Expr::num(1.0),
                src: placeholder_span(),
            },
            L2Stmt::Label {
                n: 20,
                src: placeholder_span(),
            },
        ];
        let m = collect_labels(&stmts);
        assert_eq!(m.get(&10), Some(&0));
        assert_eq!(m.get(&20), Some(&2));
        assert_eq!(m.get(&30), None);
    }

    #[test]
    fn resolve_labels_parser_table_wins() {
        use crate::ast::{placeholder_span, Expr};
        // Парсерская таблица указывает на stmt-индекс (N-слово без узла
        // Label) и ПЕРЕЗАПИСЫВАЕТ собранное по узлам при конфликте.
        let mut labels = BTreeMap::new();
        labels.insert(10u32, 1usize);
        let program = L2Program {
            stmts: vec![L2Stmt::Label {
                n: 10,
                src: placeholder_span(),
            }],
            subs: Vec::new(),
            labels: labels.clone(), // МЕХ. фикс (D2): labels используются ниже
        };
        let m = resolve_labels(&program);
        assert_eq!(m.get(&10), Some(&1)); // парсер победил
        labels.insert(99, 5);
        let mut program = program;
        program.labels = labels;
        assert_eq!(resolve_labels(&program).get(&99), Some(&5));
        let _ = Expr::num(0.0);
    }
}
