//! Словарь Fanuc mill и модальное состояние канонизации (этап 1, §1.1 отчёта).
//! Этап 1 покрывает группы 01–06: движение (01), плоскости (02), G90/G91 (03),
//! G90.1/G91.1 (04 — редкая опция, поглощать как LinuxCNC), G94/G95 (05),
//! G20/G21 (+ псевдонимы G70/G71) (06). Остальные G-коды — вне словаря этапа 1
//! (from_value → None): блок уходит в line-level Raw (матрица §2/§3 отчёта —
//! этап 2/3; provisional Raw{E_UNSUPPORTED_SYNTAX} в канонизаторе, молчаливый
//! пропуск запрещён).
//!
//! G70/G71 — псевдонимы units (§2.2: «псевдонимы G70/G71 распознавать»);
//! на токарных сериях G70/G71 — циклы (коллизия задокументирована в отчёте,
//! скоуп §0 — milling).

use eid_core::ast::Units;

/// G-код словаря этапа 1 (Fanuc mill).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GCode {
    // Группа 01 — движение
    Rapid,   // G00
    Linear,  // G01
    ArcCw,   // G02
    ArcCcw,  // G03
    // Группа 02 — плоскости (ABSORB; валидация нормали — SPEC §6)
    PlaneXY, // G17
    PlaneZX, // G18 (ZX — те же пары, что LinuxCNC; §2.2)
    PlaneYZ, // G19
    // Группа 03 — abs/inc
    DistAbs, // G90
    DistInc, // G91
    // Группа 04 — центр дуги (опция 16i/30i, `?` §10(1); по умолчанию Fanuc =
    // incremental (G91.1-эквивалент); при наличии — поглощать как LinuxCNC)
    ArcCenterAbs, // G90.1
    ArcCenterInc, // G91.1
    // Группа 05 — подача: G94 ABSORB; G95 → RAW(E_UNSUPPORTED_SEMANTICS) (§2.2)
    FeedPerMin, // G94
    FeedPerRev, // G95 (детект для RAW)
    // Группа 06 — единицы (META); псевдонимы G70/G71 распознавать (§2.2)
    UnitsIn, // G20
    UnitsMm, // G21
    UnitsInAlias,  // G70 → inch
    UnitsMmAlias,  // G71 → mm
}

/// M-код словаря этапа 1. Полная матрица M-кодов (§3 отчёта) — этап 2.
/// Этап 1 реализует только структурно обязательный program end (SPEC §3:
/// program_end ровно один раз, последний; отсутствие → auto-PEND + notice).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MCode {
    ProgramEnd, // M2 / M30 — различие вне L1 (SPEC §5.2)
}

/// Слова-параметры словаря этапа 1. X/Y/Z — опы; I/J/K/R — дуги; F — подача;
/// S — обороты (сохранение при M3 — этап 2; голое S — легальное поглощение,
/// §3 отчёта). Остальные буквы (P/Q/T/H/D/L/O/A…E/U/V/W) — вне словаря
/// этапа 1 → unsupported → line-level Raw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordKind {
    F, I, J, K, R, S, X, Y, Z,
}

impl GCode {
    /// Значение G-слова → код словаря; None = вне словаря этапа 1.
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
            940 => GCode::FeedPerMin,
            950 => GCode::FeedPerRev,
            200 => GCode::UnitsIn,
            210 => GCode::UnitsMm,
            700 => GCode::UnitsInAlias,
            710 => GCode::UnitsMmAlias,
            _ => return None,
        })
    }

    /// Единицы для кодов группы 06 (включая псевдонимы).
    pub fn units(&self) -> Option<Units> {
        match self {
            GCode::UnitsIn | GCode::UnitsInAlias => Some(Units::In),
            GCode::UnitsMm | GCode::UnitsMmAlias => Some(Units::Mm),
            _ => None,
        }
    }
}

impl MCode {
    /// Значение M-слова → код словаря этапа 1; прочие — этап 2 (None).
    pub fn from_value(value: f64) -> Option<MCode> {
        let t = (value * 10.0).round() as i64;
        Some(match t {
            20 | 300 => MCode::ProgramEnd,
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
            _ => return None,
        })
    }
}

/// Модальные группы (§1.1 отчёта, этап 1: 01–06). Один код на группу на блок —
/// как GRBL/LinuxCNC; нарушение — диагностика (block.modal_violation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalGroup {
    Motion01,     // G00/G01/G02/G03
    Plane02,      // G17/G18/G19
    Distance03,   // G90/G91
    ArcCenter04,  // G90.1/G91.1 (опция; `?` §10(1))
    FeedMode05,   // G94/G95
    Units06,      // G20/G21/G70/G71
}

pub fn modal_group(g: GCode) -> Option<ModalGroup> {
    Some(match g {
        GCode::Rapid | GCode::Linear | GCode::ArcCw | GCode::ArcCcw => ModalGroup::Motion01,
        GCode::PlaneXY | GCode::PlaneZX | GCode::PlaneYZ => ModalGroup::Plane02,
        GCode::DistAbs | GCode::DistInc => ModalGroup::Distance03,
        GCode::ArcCenterAbs | GCode::ArcCenterInc => ModalGroup::ArcCenter04,
        GCode::FeedPerMin | GCode::FeedPerRev => ModalGroup::FeedMode05,
        GCode::UnitsIn | GCode::UnitsMm | GCode::UnitsInAlias | GCode::UnitsMmAlias => {
            ModalGroup::Units06
        }
    })
}

/// Плоскость → (axis_0, axis_1, axis_linear) — пары как LinuxCNC/GRBL
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

/// Модальное состояние канонизации (группы 01–06). Дефолты Fanuc mill:
/// G17, G90, G91.1-эквивалент (инкрементальный центр), G94, mm
/// (дефолт units mm = SPEC §2.5; первое слово G20/G21 устанавливает).
#[derive(Debug, Clone, PartialEq)]
pub struct ModalState {
    pub motion: Option<GCode>,      // группа 01 (модальное продолжение, §2.7)
    pub plane: GCode,               // группа 02
    pub dist_abs: bool,             // группа 03 (true = G90)
    pub arc_center_abs: bool,       // группа 04 (true = G90.1)
    pub feed_per_min: bool,         // группа 05 (true = G94; G95 → Raw, не сюда)
    pub units: Units,               // группа 06
    pub units_established: bool,    // первое слово units устанавливает (SPEC §2.5)
}

impl Default for ModalState {
    fn default() -> Self {
        Self {
            motion: None,
            plane: GCode::PlaneXY,
            dist_abs: true,
            arc_center_abs: false,
            feed_per_min: true,
            units: Units::Mm,
            units_established: false,
        }
    }
}

impl ModalState {
    /// Поглощение кода модальной группы (вызов — уже после проверки
    /// violation и unit-switch в canonicalize).
    pub fn apply(&mut self, g: GCode) {
        match g {
            GCode::Rapid | GCode::Linear | GCode::ArcCw | GCode::ArcCcw => self.motion = Some(g),
            GCode::PlaneXY | GCode::PlaneZX | GCode::PlaneYZ => self.plane = g,
            GCode::DistAbs => self.dist_abs = true,
            GCode::DistInc => self.dist_abs = false,
            GCode::ArcCenterAbs => self.arc_center_abs = true,
            GCode::ArcCenterInc => self.arc_center_abs = false,
            GCode::FeedPerMin => self.feed_per_min = true,
            // G95 в state не попадает (Raw в канонизаторе до apply).
            GCode::FeedPerRev => {}
            GCode::UnitsIn | GCode::UnitsInAlias => self.units = Units::In,
            GCode::UnitsMm | GCode::UnitsMmAlias => self.units = Units::Mm,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(g(90.1), Some(GCode::ArcCenterAbs));
        assert_eq!(g(91.1), Some(GCode::ArcCenterInc));
        assert_eq!(modal_group(GCode::ArcCenterAbs), Some(ModalGroup::ArcCenter04));
        assert_eq!(g(94.0), Some(GCode::FeedPerMin));
        assert_eq!(g(95.0), Some(GCode::FeedPerRev));
        assert_eq!(modal_group(GCode::FeedPerRev), Some(ModalGroup::FeedMode05));
        // Псевдонимы units G70/G71 (§2.2).
        assert_eq!(g(20.0), Some(GCode::UnitsIn));
        assert_eq!(g(21.0), Some(GCode::UnitsMm));
        assert_eq!(g(70.0), Some(GCode::UnitsInAlias));
        assert_eq!(g(71.0), Some(GCode::UnitsMmAlias));
        assert_eq!(GCode::UnitsInAlias.units(), Some(Units::In));
        assert_eq!(GCode::UnitsMmAlias.units(), Some(Units::Mm));
        assert_eq!(modal_group(GCode::UnitsMmAlias), Some(ModalGroup::Units06));
    }

    #[test]
    fn out_of_stage1_dictionary_is_none() {
        // G4/G10/G28/G40/G43/G53/G54/G64/G80/G81/G92 → этап 2/3; G2.2/G33 —
        // RAW-классы; всё вне словаря этапа 1 → None.
        for v in [4.0, 10.0, 28.0, 40.0, 43.0, 53.0, 54.0, 64.0, 80.0, 81.0, 92.0, 2.2, 33.0, 96.0] {
            assert_eq!(g(v), None, "G{v}");
        }
    }

    #[test]
    fn m_codes_stage1_only_end() {
        assert_eq!(MCode::from_value(2.0), Some(MCode::ProgramEnd));
        assert_eq!(MCode::from_value(30.0), Some(MCode::ProgramEnd));
        // M0/M1/M3/M5/M6/M8/M9/M19/M98 → этап 2 (None сейчас).
        for v in [0.0, 1.0, 3.0, 5.0, 6.0, 8.0, 9.0, 19.0, 98.0] {
            assert_eq!(MCode::from_value(v), None, "M{v}");
        }
    }

    #[test]
    fn words_and_plane_axes() {
        for (c, k) in [('F', WordKind::F), ('I', WordKind::I), ('J', WordKind::J),
                       ('K', WordKind::K), ('R', WordKind::R), ('S', WordKind::S),
                       ('x', WordKind::X), ('y', WordKind::Y), ('z', WordKind::Z)] {
            assert_eq!(WordKind::from_letter(c), Some(k));
        }
        // P/Q/T/H/D/L/O — вне словаря этапа 1 (unsupported).
        for c in ['P', 'Q', 'T', 'H', 'D', 'L', 'O', 'A', 'U', 'E'] {
            assert_eq!(WordKind::from_letter(c), None, "{c}");
        }
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
        // Дефолты: G17/G90/инкрементальный центр/G94/mm.
        assert_eq!(m.plane, GCode::PlaneXY);
        assert!(m.dist_abs);
        assert!(!m.arc_center_abs);
        assert!(m.feed_per_min);
        assert_eq!(m.units, Units::Mm);
        assert!(!m.units_established);
        assert_eq!(m.motion, None);
        m.apply(GCode::ArcCcw);
        m.apply(GCode::PlaneYZ);
        m.apply(GCode::DistInc);
        m.apply(GCode::ArcCenterAbs);
        m.apply(GCode::UnitsInAlias);
        assert_eq!(m.motion, Some(GCode::ArcCcw));
        assert_eq!(m.plane, GCode::PlaneYZ);
        assert!(!m.dist_abs);
        assert!(m.arc_center_abs);
        assert_eq!(m.units, Units::In);
        m.apply(GCode::ArcCenterInc);
        assert!(!m.arc_center_abs);
        m.apply(GCode::UnitsMm);
        assert_eq!(m.units, Units::Mm);
    }
}
