//! Словарь Haas mill NGC и модальное состояние канонизации (этап 1, §9.1
//! контракта: лексер/движение/модальности 01–06+15). Этап 2 добавит: фреймы
//! (WCS-слоты G54–G59 + G110–G112 → p=7..9, G52-ERR, G10-формы L2/L20/P0),
//! циклы ×Haas-семантика (L-повторы, K=min-пек G83, G73 Setting 22, P-dot
//! dwell), M-словарь (M13-тормоз/M29-relay/M07-Shower), компенсации.
//!
//! Коды с чисто RAW-диспозицией (G28/G31/G70/G71/G72/…) — НЕ в enum: их
//! различение не влияет на state/опы — таблица [`g_raw_disposition`]
//! (фиксированные COVERAGE-коды §2/§3). Неизвестные коды — generic-ведро
//! Raw{SYNTAX} с диагностикой текста (COVERAGE §2.7: «не молча»).
//!
//! ГЛАВНЫЕ ОТЛИЧИЯ ОТ FANUC (контракт §0.1/§1.1):
//! * **G70/G71/G72 = болтовые паттерны** (Bolt Hole Circle/Arc/Holes Along an
//!   Angle) — НЕ units-псевдонимы G20/G21 (≠ Fanuc mill!) — raw-таблица
//!   E_UNSUPPORTED_SEMANTICS.
//! * **Группы 04 в словаре НЕТ** — I/J/K всегда инкрементальные от start
//!   (официальный ман). G90.1/G91.1 — вне словаря Haas; по ruling'у этапа 1
//!   реализованы ABSORB-как-LinuxCNC с пометкой `?` (Haas-колонка несверено,
//!   реестр §10(4)-класса) — консервативно: дефолт инкрементальный.
//! * **Группа 15 = семейство exact-stop**: G61 = exact stop (совпадает по
//!   значению с Fanuc), G64 = **cancel G61, bare** (НЕ blend(t) LinuxCNC!).
//! * Группа 05: G93/G94 ABSORB; G95 → Raw{SEMANTICS} (детект до apply).

use eid_core::ast::{PathMode, Units, Wcs};

/// G-код словаря Haas mill NGC (структурные коды — нужны state/op-эффект).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GCode {
    // Группа 01 — движение (явный код отменяет активный цикл группы 09 —
    // этап 2; G01 с C/R-словами = фаска/скругление → Raw в канонизаторе)
    Rapid,  // G00
    Linear, // G01
    ArcCw,  // G02
    ArcCcw, // G03
    // Группа 02 — плоскости (ABSORB; валидация нормали — SPEC §6)
    PlaneXY, // G17
    PlaneZX, // G18 (ZX — те же пары, что LinuxCNC; §2.2)
    PlaneYZ, // G19
    // Группа 03 — abs/inc
    DistAbs, // G90
    DistInc, // G91
    // Группа 04 — центр дуги: В СЛОВАРЕ HAAS ОТСУТСТВУЕТ (§1.1: I/J/K всегда
    // инкрементальные). G90.1/G91.1 — вне словаря; ruling этапа 1 — ABSORB
    // как LinuxCNC, помечено `?` (несверено; дефолт — инкрементальный)
    ArcCenterAbs, // G90.1 `?`
    ArcCenterInc, // G91.1 `?`
    // Группа 05 — подача: G94 ABSORB; G93 ABSORB (inverse-time, опция 5-осевая —
    // §2.2; эмиссия всегда G94); G95 → RAW (детект через g95-путь канонизатора,
    // до apply)
    FeedPerMin,  // G94
    FeedInverse, // G93
    FeedPerRev,  // G95 (детект для RAW; feed/rev — rpm от override)
    // Группа 06 — единицы (META). ПСЕВДОНИМОВ G70/G71 НЕТ — заняты болтовыми
    // паттернами (≠ Fanuc mill; §2.2/§2.6)!
    UnitsIn, // G20
    UnitsMm, // G21
    // Группа 15 — path (семейство exact-stop; СДВИГ значения vs LinuxCNC:
    // G61 = exact_stop, G64 = cancel G61 — bare, квантификатора t НЕТ)
    PathExactStop, // G61
    PathBlend,     // G64 (bare blend)
}

/// M-код словаря Haas mill NGC — ЭТАП 1 (минимум для структуры + §8.6-
/// абсорбции). Этап 2 расширит: M00/M01, M06/M16+T, M19 (orient), M29
/// (output relay — НЕ rigid-tap, коллизия с Fanuc!), M10–M13 (осевые тормоза),
/// M07 (Shower — второй канал СОЖ, НЕ mist!), M97/M98/M99 (subs → SYNTAX).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MCode {
    ProgramEnd,  // M2 / M30 — различие вне L1 (SPEC §5.2)
    SpindleCw,   // M3
    SpindleCcw,  // M4
    SpindleOff,  // M5
    CoolantFlood, // M8 (1:1; M07 Shower/M88 TSC — другие каналы, этап 2)
    CoolantOff,  // M9
}

/// Слова-параметры словаря. X/Y/Z — опы; I/J/K/R — дуги (I/J/K контекстно-
/// зависимы: центр дуги / пеки G83 (этап 2) / радиус-угол G12 — §4.1);
/// F — подача (Setting 77 масштабирует bare-integer — §1.2); S — обороты;
/// C — фаска угла в G01 (Haas-специфика!); P — dwell (мс/сек по точке,
/// этап 2); Q — пек/shift/feed-% (контекстно, этап 2); L — повтор цикла
/// (НЕ K!, этап 2); T/H/D — инструмент/таблицы; E — accuracy G187;
/// U/W — Setting 162 lathe-наследие `?`; A/B/C — ротационные оси (этап 12).
/// V — вне словаря Haas (не входит в Setting 162-домен) → generic-ведро.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordKind {
    F,
    I,
    J,
    K,
    R,
    S,
    X,
    Y,
    Z,
    P,
    Q,
    T,
    H,
    D,
    L,
    O,
    E,
    U,
    W,
    A,
    B,
    C,
}

impl GCode {
    /// Значение G-слова → код словаря; None = вне словаря (raw-таблица /
    /// generic-ведро — канонизатор).
    pub fn from_value(value: f64) -> Option<GCode> {
        // *10 без потерь: словарь имеет максимум один десятичный знак.
        let t = (value * 10.0).round() as i64;
        if (value * 10.0).round() != value * 10.0 {
            return None; // лишние десятичные знаки — вне словаря
        }
        Some(match t {
            0 => GCode::Rapid,
            10 => GCode::Linear,
            20 => GCode::ArcCw,
            30 => GCode::ArcCcw,
            170 => GCode::PlaneXY,
            180 => GCode::PlaneZX,
            190 => GCode::PlaneYZ,
            900 => GCode::DistAbs,
            910 => GCode::DistInc,
            901 => GCode::ArcCenterAbs,
            911 => GCode::ArcCenterInc,
            930 => GCode::FeedInverse,
            940 => GCode::FeedPerMin,
            950 => GCode::FeedPerRev,
            200 => GCode::UnitsIn,
            210 => GCode::UnitsMm,
            610 => GCode::PathExactStop,
            640 => GCode::PathBlend,
            _ => return None,
        })
    }

    /// Единицы для кодов группы 06. G70/G71/G72 здесь НЕТ (болтовые
    /// паттерны — raw-таблица §2.6).
    pub fn units(&self) -> Option<Units> {
        match self {
            GCode::UnitsIn => Some(Units::In),
            GCode::UnitsMm => Some(Units::Mm),
            _ => None,
        }
    }

    /// WCS-выбор группы 12 (этап 2; поле enum не занимает — v0.1-скоуп
    /// этапа 1 покрывает только группы 01–06+15; G54–G59 → этап 2).
    pub fn wcs(&self) -> Option<Wcs> {
        None
    }

    /// Path-режим группы 15 — семейство exact-stop, сдвиг значения vs
    /// LinuxCNC (§2.2): G61 = exact_stop (совпадает с Fanuc G61);
    /// G64 = cancel G61 → bare blend (t-квантификатора в словаре НЕТ).
    pub fn path_mode(&self) -> Option<PathMode> {
        Some(match self {
            GCode::PathExactStop => PathMode::ExactStop,
            GCode::PathBlend => PathMode::Blend { t: None },
            _ => return None,
        })
    }
}

impl MCode {
    /// Значение M-слова → код словаря; None = вне словаря этапа 1
    /// (generic-ведро SYNTAX provisional; этап 2 добавит вендорское ведро —
    /// документированные Haas M-коды → Raw{SEMANTICS}).
    pub fn from_value(value: f64) -> Option<MCode> {
        let t = (value * 10.0).round() as i64;
        Some(match t {
            20 | 300 => MCode::ProgramEnd,
            30 => MCode::SpindleCw,
            40 => MCode::SpindleCcw,
            50 => MCode::SpindleOff,
            80 => MCode::CoolantFlood,
            90 => MCode::CoolantOff,
            _ => return None,
        })
    }
}

impl WordKind {
    pub fn from_letter(letter: char) -> Option<WordKind> {
        Some(match letter.to_ascii_uppercase() {
            'F' => WordKind::F,
            'I' => WordKind::I,
            'J' => WordKind::J,
            'K' => WordKind::K,
            'R' => WordKind::R,
            'S' => WordKind::S,
            'X' => WordKind::X,
            'Y' => WordKind::Y,
            'Z' => WordKind::Z,
            'P' => WordKind::P,
            'Q' => WordKind::Q,
            'T' => WordKind::T,
            'H' => WordKind::H,
            'D' => WordKind::D,
            'L' => WordKind::L,
            'O' => WordKind::O,
            'E' => WordKind::E,
            'U' => WordKind::U,
            'W' => WordKind::W,
            'A' => WordKind::A,
            'B' => WordKind::B,
            'C' => WordKind::C,
            _ => return None,
        })
    }
}

/// Модальные группы (§1.1 контракта; этап 1 — 01–06 + 15; один код на группу
/// на блок, нарушение — диагностика block.modal_violation). Групп 07/08/09/
/// 10/11/12/14/16/23 — этап 2+. Группы 13/17/18 Fanuc-типа НЕТ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalGroup {
    Motion01,    // G00/G01/G02/G03
    Plane02,     // G17/G18/G19
    Distance03,  // G90/G91
    ArcCenter04, // G90.1/G91.1 — В СЛОВАРЕ НЕТ `?`; ABSORB-как-LinuxCNC ruling
    FeedMode05,  // G93/G94/G95
    Units06,     // G20/G21 (G70/G71/G72 — болтовые, вне enum)
    Path15,      // G61/G64 (exact-stop family)
}

pub fn modal_group(g: GCode) -> Option<ModalGroup> {
    Some(match g {
        GCode::Rapid | GCode::Linear | GCode::ArcCw | GCode::ArcCcw => ModalGroup::Motion01,
        GCode::PlaneXY | GCode::PlaneZX | GCode::PlaneYZ => ModalGroup::Plane02,
        GCode::DistAbs | GCode::DistInc => ModalGroup::Distance03,
        GCode::ArcCenterAbs | GCode::ArcCenterInc => ModalGroup::ArcCenter04,
        GCode::FeedPerMin | GCode::FeedInverse | GCode::FeedPerRev => ModalGroup::FeedMode05,
        GCode::UnitsIn | GCode::UnitsMm => ModalGroup::Units06,
        GCode::PathExactStop | GCode::PathBlend => ModalGroup::Path15,
    })
}

// ---------------------------------------------------------------------------
// RAW-диспозиции G-кодов вне структурного словаря (COVERAGE §2 Haas-колонка;
// этап 1 — таблица движения-класса и болтовых паттернов; этап 2 добавит
// G52-ERR-путь, G10-формы, G110–G129/G154, циклы-отказы G74/G76/G77/G84/…)
// ---------------------------------------------------------------------------

/// RAW-код + причина для G-кода вне enum-словаря. `t = round(value*10)`.
/// Возврат None → generic-ведро Raw{E_UNSUPPORTED_SYNTAX} с текстом кода
/// (COVERAGE §2.7: «не молча»).
pub fn g_raw_disposition(value: f64) -> Option<(eid_core::diag::ErrorCode, &'static str)> {
    use eid_core::diag::ErrorCode::{
        RequiresMachineTable as RT, UnsupportedSemantics as US, UnsupportedSyntax as UX,
    };
    let t = (value * 10.0).round() as i64;
    if (value * 10.0).round() != value * 10.0 {
        return None; // лишние десятичные знаки — generic-ведро
    }
    Some(match t {
        // Home/reference (Q1) — COVERAGE §2.3 (позиция machine zero вне текста)
        280 => (RT, "G28 return to machine zero — reference position outside text (COVERAGE §2.3; Q1)"),
        290 => (RT, "G29 return from reference point — position outside text (COVERAGE §2.3; Q1)"),
        // One-shot path-поведение — COVERAGE §2.2
        90 => (US, "G09 exact stop one-shot — path_mode is program-global in L1 (COVERAGE §2.2)"),
        600 => (US, "G60 uni-directional positioning — approach geometry shift not representable (COVERAGE §2.2)"),
        // Haas-генераторы траекторий / болтовые паттерны — COVERAGE §2.1/§2.6
        // G70/G71/G72 — КОЛЛИЗИЯ АДРЕСОВ: НЕ units-псевдонимы (≠ Fanuc mill)!
        700 => (US, "G70 bolt hole circle — Haas bolt pattern, NOT a G20 units alias (address collision vs Fanuc; COVERAGE §2.6)"),
        710 => (US, "G71 bolt hole arc — Haas bolt pattern, NOT a G21 units alias (address collision vs Fanuc; COVERAGE §2.6)"),
        720 => (US, "G72 holes along an angle — Haas bolt pattern (COVERAGE §2.6)"),
        120 => (US, "G12 circular pocket milling CW — Haas trajectory generator, stage-12 candidate (COVERAGE §2.6)"),
        130 => (US, "G13 circular pocket milling CCW — Haas trajectory generator, stage-12 candidate (COVERAGE §2.6)"),
        // Сенсорные (probe/tool setter) — COVERAGE §2.1
        310 => (US, "G31 feed until skip — probing class, skip position runtime (COVERAGE §2.1)"),
        350 => (US, "G35 auto tool diameter measurement — sensor, writes machine tables (COVERAGE §2.1)"),
        360 => (US, "G36 auto work offset measurement — sensor, writes machine tables (COVERAGE §2.3)"),
        370 => (US, "G37 auto tool length measurement — sensor, writes machine tables (COVERAGE §2.1)"),
        1360 => (US, "G136 automatic work offset center measurement — probing, M75 reference (COVERAGE §2.1)"),
        // Macro B (§4.3) — Fanuc-совместимый: line-level Raw{SYNTAX}
        650 => (UX, "G65 macro call — Macro B parametric, line-level Raw (COVERAGE §4)"),
        660 => (UX, "G66 modal macro call — Macro B `?` NGC/CHC support (COVERAGE §4; реестр §10(11))"),
        670 => (UX, "G67 modal macro cancel — Macro B (COVERAGE §4)"),
        _ => return None,
    })
}

/// raw-таблица: G-коды, чей блок с осями — ход движения (motion-класс; SPEC
/// §8.6 правило 6: motion-режим на raw-motion-G НЕ продолжается + notice
/// W_MODAL_ON_RAW). t = round(value*10). Home G28/G29; probing G31;
/// измерения G35/G36/G37/G136; генераторы G12/G13; болтовые паттерны
/// G70/G71/G72 (генерируют позиции активного цикла).
pub const RAW_MOTION_G: &[i64] = &[
    280, 290,   // G28/G29 — home
    310,        // G31 — probing skip
    350, 360, 370, 1360, // G35/G36/G37/G136 — автоизмерение
    120, 130,   // G12/G13 — circular pocket
    700, 710, 720, // G70/G71/G72 — болтовые паттерны
];

/// Плоскость → (axis_0, axis_1, axis_linear) — пары как LinuxCNC/GRBL/Fanuc
/// (§2.2: G18 = ZX, G19 = YZ). Нормаль = +axis_linear (SPEC §6).
pub fn plane_axes(g: GCode) -> (usize, usize, usize) {
    match g {
        GCode::PlaneXY => (0, 1, 2),
        GCode::PlaneZX => (2, 0, 1),
        GCode::PlaneYZ => (1, 2, 0),
        _ => (0, 1, 2),
    }
}

/// Нормаль активной плоскости — канонический +axis (SPEC §6).
pub fn plane_normal(g: GCode) -> [f64; 3] {
    match g {
        GCode::PlaneXY => [0.0, 0.0, 1.0],
        GCode::PlaneZX => [0.0, 1.0, 0.0],
        GCode::PlaneYZ => [1.0, 0.0, 0.0],
        _ => [0.0, 0.0, 1.0],
    }
}

/// Модальное состояние канонизации. Дефолты Haas NGC (§4.4 контракта:
/// power-on G00/G17/G40/G49/G54/G80/G90/G94 + units по Setting `?` —
/// mm-дефолт консервативен; на эквивалентность не влияет — assumption
/// самодостаточности, EQUIVALENCE §1.2): G17, G90, **инкрементальный центр**
/// (группы 04 нет — I/J/K от start всегда; G90.1 `?` — ABSORB ruling),
/// G94, mm (`?` Setting-зависимый), G54, exact_path при отсутствии G61/G64
/// (SPEC §3 v0.12).
#[derive(Debug, Clone, PartialEq)]
pub struct ModalState {
    pub motion: Option<GCode>,   // группа 01 (модальное продолжение, §1.3)
    pub plane: GCode,            // группа 02
    pub dist_abs: bool,          // группа 03 (true = G90)
    pub arc_center_abs: bool,    // группа 04 `?` (true = G90.1; дефолт false)
    pub feed_per_min: bool,      // группа 05 (true = G94)
    pub feed_inverse: bool,      // группа 05: G93 (ABSORB, опция 5-осевая)
    pub units: Units,            // группа 06
    pub units_established: bool, // первое слово units устанавливает (SPEC §2.5)
    pub wcs: Wcs,                // группа 12 — этап 2 (дефолт G54 контроллера)
    pub path: Option<PathMode>,  // группа 15 (первое вхождение устанавливает)
}

impl Default for ModalState {
    fn default() -> Self {
        Self {
            motion: None,
            plane: GCode::PlaneXY,
            dist_abs: true,
            arc_center_abs: false,
            feed_per_min: true,
            feed_inverse: false,
            units: Units::Mm,
            units_established: false,
            wcs: Wcs::G54,
            path: None,
        }
    }
}

impl ModalState {
    /// Поглощение кода модальной группы (вызов — уже после проверки violation,
    /// unit-switch и path-switch в canonicalize; G95 — до apply).
    pub fn apply(&mut self, g: GCode) {
        match g {
            GCode::Rapid | GCode::Linear | GCode::ArcCw | GCode::ArcCcw => self.motion = Some(g),
            GCode::PlaneXY | GCode::PlaneZX | GCode::PlaneYZ => self.plane = g,
            GCode::DistAbs => self.dist_abs = true,
            GCode::DistInc => self.dist_abs = false,
            GCode::ArcCenterAbs => self.arc_center_abs = true,
            GCode::ArcCenterInc => self.arc_center_abs = false,
            GCode::FeedPerMin => {
                self.feed_per_min = true;
                self.feed_inverse = false;
            }
            GCode::FeedInverse => {
                self.feed_per_min = false;
                self.feed_inverse = true;
            }
            // G95 в state не попадает (Raw в канонизаторе до apply).
            GCode::FeedPerRev => {}
            GCode::UnitsIn => self.units = Units::In,
            GCode::UnitsMm => self.units = Units::Mm,
            // Группа 12 — этап 2 (wcs() пока всегда None).
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eid_core::diag::ErrorCode;

    fn g(v: f64) -> Option<GCode> {
        GCode::from_value(v)
    }

    #[test]
    fn motion_plane_distance_groups() {
        assert_eq!(g(0.0), Some(GCode::Rapid));
        assert_eq!(g(1.0), Some(GCode::Linear));
        assert_eq!(g(2.0), Some(GCode::ArcCw));
        assert_eq!(g(3.0), Some(GCode::ArcCcw));
        assert_eq!(g(17.0), Some(GCode::PlaneXY));
        assert_eq!(g(18.0), Some(GCode::PlaneZX));
        assert_eq!(g(19.0), Some(GCode::PlaneYZ));
        assert_eq!(g(90.0), Some(GCode::DistAbs));
        assert_eq!(g(91.0), Some(GCode::DistInc));
        assert_eq!(modal_group(GCode::ArcCw), Some(ModalGroup::Motion01));
        assert_eq!(modal_group(GCode::PlaneZX), Some(ModalGroup::Plane02));
        assert_eq!(modal_group(GCode::DistInc), Some(ModalGroup::Distance03));
    }

    #[test]
    fn arc_center_feed_units_groups() {
        // Группа 04 — В СЛОВАРЕ НЕТ; G90.1/G91.1 — ABSORB ruling `?`.
        assert_eq!(g(90.1), Some(GCode::ArcCenterAbs));
        assert_eq!(g(91.1), Some(GCode::ArcCenterInc));
        assert_eq!(
            modal_group(GCode::ArcCenterAbs),
            Some(ModalGroup::ArcCenter04)
        );
        assert_eq!(g(93.0), Some(GCode::FeedInverse));
        assert_eq!(g(94.0), Some(GCode::FeedPerMin));
        assert_eq!(g(95.0), Some(GCode::FeedPerRev));
        assert_eq!(modal_group(GCode::FeedInverse), Some(ModalGroup::FeedMode05));
        // Единицы — БЕЗ псевдонимов (§2.2: псевдонимов G70/G71 НЕТ).
        assert_eq!(g(20.0), Some(GCode::UnitsIn));
        assert_eq!(g(21.0), Some(GCode::UnitsMm));
        assert_eq!(g(70.0), None, "G70 — болтовой паттерн, НЕ units");
        assert_eq!(g(71.0), None, "G71 — болтовой паттерн, НЕ units");
        assert_eq!(g(72.0), None, "G72 — болтовой паттерн");
        assert_eq!(modal_group(GCode::UnitsMm), Some(ModalGroup::Units06));
    }

    #[test]
    fn path15_value_shift() {
        // Группа 15 — сдвиг значения vs LinuxCNC: G61 = exact_stop,
        // G64 = bare blend (cancel G61; t-квантификатора НЕТ).
        assert_eq!(
            g(61.0).and_then(|c| c.path_mode()),
            Some(PathMode::ExactStop)
        );
        assert_eq!(
            g(64.0).and_then(|c| c.path_mode()),
            Some(PathMode::Blend { t: None })
        );
        assert_eq!(modal_group(GCode::PathExactStop), Some(ModalGroup::Path15));
    }

    #[test]
    fn out_of_dictionary_is_none_but_raw_table_known() {
        // G28/G31/G43/G65/G84/G92/G54.1/G110/G154 → вне enum (raw-таблица/
        // generic; фреймы/циклы — этап 2).
        for v in [28.0, 31.0, 43.0, 84.0, 92.0, 54.0, 54.1, 110.0, 154.0, 10.0, 4.0, 52.0] {
            assert_eq!(g(v), None, "G{v} — вне enum этапа 1");
        }
        // Raw-таблица даёт фиксированные коды.
        let (c, r) = g_raw_disposition(28.0).unwrap();
        assert_eq!(c, ErrorCode::RequiresMachineTable);
        assert!(r.contains("outside text"));
        let (c, _) = g_raw_disposition(31.0).unwrap();
        assert_eq!(c, ErrorCode::UnsupportedSemantics);
        // Болтовые паттерны — US с текстом о коллизии адресов.
        for t in [70.0, 71.0, 72.0] {
            let (c, r) = g_raw_disposition(t).unwrap();
            assert_eq!(c, ErrorCode::UnsupportedSemantics);
            assert!(r.contains("bolt") || r.contains("holes"), "G{t}");
        }
        let (c, _) = g_raw_disposition(12.0).unwrap();
        assert_eq!(c, ErrorCode::UnsupportedSemantics);
        let (c, _) = g_raw_disposition(136.0).unwrap();
        assert_eq!(c, ErrorCode::UnsupportedSemantics);
        let (c, _) = g_raw_disposition(65.0).unwrap();
        assert_eq!(c, ErrorCode::UnsupportedSyntax);
        // Неизвестный код → None (generic-ведро SYNTAX).
        assert_eq!(g_raw_disposition(88.8), None);
        assert_eq!(g_raw_disposition(107.0), None, "G107 — этап 2+ ведро");
        // RAW_MOTION_G покрывает движение-класс.
        for t in [280, 290, 310, 350, 360, 370, 1360, 120, 130, 700, 710, 720] {
            assert!(RAW_MOTION_G.contains(&t), "t={t}");
        }
    }

    #[test]
    fn m_codes_stage1_dictionary() {
        assert_eq!(MCode::from_value(2.0), Some(MCode::ProgramEnd));
        assert_eq!(MCode::from_value(30.0), Some(MCode::ProgramEnd));
        assert_eq!(MCode::from_value(3.0), Some(MCode::SpindleCw));
        assert_eq!(MCode::from_value(4.0), Some(MCode::SpindleCcw));
        assert_eq!(MCode::from_value(5.0), Some(MCode::SpindleOff));
        assert_eq!(MCode::from_value(8.0), Some(MCode::CoolantFlood));
        assert_eq!(MCode::from_value(9.0), Some(MCode::CoolantOff));
        // Этап 2: M0/M1/M6/M7-Shower/M13-тормоз/M19/M29-relay/M97-99 —
        // вне словаря этапа 1 (generic provisional; M07 — НЕ mist!).
        for v in [0.0, 1.0, 6.0, 7.0, 13.0, 19.0, 29.0, 98.0, 99.0] {
            assert_eq!(MCode::from_value(v), None, "M{v} — этап 2");
        }
    }

    #[test]
    fn words_and_plane_axes() {
        for (c, k) in [
            ('F', WordKind::F),
            ('I', WordKind::I),
            ('J', WordKind::J),
            ('K', WordKind::K),
            ('R', WordKind::R),
            ('S', WordKind::S),
            ('x', WordKind::X),
            ('y', WordKind::Y),
            ('z', WordKind::Z),
            ('P', WordKind::P),
            ('q', WordKind::Q),
            ('T', WordKind::T),
            ('H', WordKind::H),
            ('D', WordKind::D),
            ('L', WordKind::L),
            ('O', WordKind::O),
            ('E', WordKind::E),
            ('U', WordKind::U),
            ('W', WordKind::W),
            ('A', WordKind::A),
            ('B', WordKind::B),
            ('C', WordKind::C),
        ] {
            assert_eq!(WordKind::from_letter(c), Some(k), "{c}");
        }
        // V — вне словаря Haas (Setting 162-домен: X,Y,Z,A,B,C,E,F,I,J,K,U,W).
        assert_eq!(WordKind::from_letter('V'), None);
        // Пары осей: G17=(X,Y|Z), G18=(Z,X|Y), G19=(Y,Z|X) — как LinuxCNC.
        assert_eq!(plane_axes(GCode::PlaneXY), (0, 1, 2));
        assert_eq!(plane_axes(GCode::PlaneZX), (2, 0, 1));
        assert_eq!(plane_axes(GCode::PlaneYZ), (1, 2, 0));
        assert_eq!(plane_normal(GCode::PlaneXY), [0.0, 0.0, 1.0]);
        assert_eq!(plane_normal(GCode::PlaneZX), [0.0, 1.0, 0.0]);
        assert_eq!(plane_normal(GCode::PlaneYZ), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn modal_defaults_and_apply() {
        let mut m = ModalState::default();
        // Дефолты NGC (§4.4): G17/G90/инкрементальный центр/G94/mm/G54.
        assert_eq!(m.plane, GCode::PlaneXY);
        assert!(m.dist_abs);
        assert!(!m.arc_center_abs, "дефолт — инкрементальный центр (группы 04 нет)");
        assert!(m.feed_per_min);
        assert!(!m.feed_inverse);
        assert_eq!(m.units, Units::Mm);
        assert!(!m.units_established);
        assert_eq!(m.wcs, Wcs::G54);
        assert_eq!(m.path, None);
        assert_eq!(m.motion, None);
        m.apply(GCode::ArcCcw);
        m.apply(GCode::PlaneYZ);
        m.apply(GCode::DistInc);
        m.apply(GCode::ArcCenterAbs);
        m.apply(GCode::UnitsIn);
        assert_eq!(m.motion, Some(GCode::ArcCcw));
        assert_eq!(m.plane, GCode::PlaneYZ);
        assert!(!m.dist_abs);
        assert!(m.arc_center_abs);
        assert_eq!(m.units, Units::In);
        // G93/G94 (ABSORB, §2.2).
        m.apply(GCode::FeedInverse);
        assert!(m.feed_inverse);
        assert!(!m.feed_per_min);
        m.apply(GCode::FeedPerMin);
        assert!(!m.feed_inverse);
        assert!(m.feed_per_min);
        m.apply(GCode::UnitsMm);
        assert_eq!(m.units, Units::Mm);
        // Группа 12 — этап 2: wcs() всегда None на этапе 1.
        assert_eq!(g(54.0).and_then(|c| c.wcs()), None);
    }
}
