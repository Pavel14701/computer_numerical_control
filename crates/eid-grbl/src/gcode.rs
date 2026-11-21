//! Словарь GRBL 1.1 — классификация G/M-слов и слов-параметров.
//! Источник: COVERAGE §2 (колонка GRBL 1.1), §4. Коды вне словаря → None
//! (канонизатор, этап 2, выдаст RAW по правилу двух классов SPEC §8).
//! Диспозиции: G28/28.1/30/30.1 → REQUIRES_MACHINE_TABLE; G38.x, G43.1,
//! G92.1 → UNSUPPORTED_SEMANTICS; G40/G49/G80 → IGNORED_SAFE; G10 → узел
//! (L2 = wcs_define, L20 = wcs_set_here); M6/G43/G64/G90.1/G59.x — N/A.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GCode {
    Rapid,            // G0
    Linear,           // G1
    ArcCw,            // G2
    ArcCcw,           // G3
    Dwell,            // G4 (P сек, COVERAGE §7.3)
    SetCoordSystem,   // G10 — L2/L20 различает block::g10_kind
    PlaneXY,          // G17
    PlaneZX,          // G18
    PlaneYZ,          // G19
    UnitsIn,          // G20
    UnitsMm,          // G21
    Home,             // G28  → RAW(REQUIRES_MACHINE_TABLE)
    SetHome,          // G28.1
    Home2,            // G30
    SetHome2,         // G30.1
    Probe(u8),        // G38.2/.3/.4/.5 → RAW(UNSUPPORTED_SEMANTICS)
    CutterCompOff,    // G40 (правило семейств §1.3)
    ToolLenDynamic,   // G43.1 → RAW(UNSUPPORTED_SEMANTICS)
    ToolLenOff,       // G49 (правило семейств §1.3)
    MachineFrame,     // G53 — модификатор кадра
    Wcs(u8),          // G54..G59 (u8 = 1..=6)
    PathExact,        // G61 → META(exact_path), Q3
    CycleCancel,      // G80 → IGNORED_SAFE
    DistAbs,          // G90
    DistInc,          // G91
    ArcDistInc,       // G91.1 (единственный режим центра дуг в GRBL)
    FramePreset,      // G92 → META(frame_preset)
    ResetFramePreset, // G92.1 → RAW(UNSUPPORTED_SEMANTICS)
    FeedInverse,      // G93
    FeedPerMin,       // G94
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MCode {
    ProgramStop,  // M0 (optional=false)
    OptionalStop, // M1 (optional=true, Q8)
    ProgramEnd,   // M2 / M30 — различие вне L1 (SPEC §5.2)
    SpindleCw,    // M3
    SpindleCcw,   // M4
    SpindleOff,   // M5
    MistOn,       // M7
    FloodOn,      // M8
    CoolantOff,   // M9
}

/// Слова-параметры словаря GRBL (COVERAGE §3 T-слово, §4; interface.md).
/// Остальные буквы (A/B/C/U/V/W/H/Q/D/O...) словарём не классифицируются.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordKind {
    F,
    I,
    J,
    K,
    L,
    P,
    R,
    S,
    T,
    X,
    Y,
    Z,
}

impl GCode {
    /// Значение G-слова → код словаря; None = вне словаря GRBL 1.1.
    pub fn from_value(value: f64) -> Option<GCode> {
        // *10 без потерь: словарь имеет максимум один десятичный знак.
        let t = (value * 10.0).round() as i64;
        if (value * 10.0).round() != value * 10.0 {
            return None; // лишние десятичные знаки — вне словаря (G54.11 и т.п.)
        }
        Some(match t {
            0 => GCode::Rapid,
            10 => GCode::Linear,
            20 => GCode::ArcCw,
            30 => GCode::ArcCcw,
            40 => GCode::Dwell,
            100 => GCode::SetCoordSystem,
            170 => GCode::PlaneXY,
            180 => GCode::PlaneZX,
            190 => GCode::PlaneYZ,
            200 => GCode::UnitsIn,
            210 => GCode::UnitsMm,
            280 => GCode::Home,
            281 => GCode::SetHome,
            300 => GCode::Home2,
            301 => GCode::SetHome2,
            382 | 383 | 384 | 385 => GCode::Probe((t - 380) as u8),
            400 => GCode::CutterCompOff,
            431 => GCode::ToolLenDynamic,
            490 => GCode::ToolLenOff,
            530 => GCode::MachineFrame,
            // Ровно G54/G55/G56/G57/G58/G59: G54.1 (541) — вне словаря GRBL.
            540 | 550 | 560 | 570 | 580 | 590 => GCode::Wcs(((t - 540) / 10 + 1) as u8),
            610 => GCode::PathExact,
            800 => GCode::CycleCancel,
            900 => GCode::DistAbs,
            910 => GCode::DistInc,
            911 => GCode::ArcDistInc,
            920 => GCode::FramePreset,
            921 => GCode::ResetFramePreset,
            930 => GCode::FeedInverse,
            940 => GCode::FeedPerMin,
            _ => return None,
        })
    }
}

impl MCode {
    /// Значение M-слова → код словаря; M6 и прочие — вне словаря GRBL (None).
    pub fn from_value(value: f64) -> Option<MCode> {
        let t = (value * 10.0).round() as i64;
        Some(match t {
            0 => MCode::ProgramStop,
            10 => MCode::OptionalStop,
            20 | 300 => MCode::ProgramEnd,
            30 => MCode::SpindleCw,
            40 => MCode::SpindleCcw,
            50 => MCode::SpindleOff,
            70 => MCode::MistOn,
            80 => MCode::FloodOn,
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
            'L' => WordKind::L,
            'P' => WordKind::P,
            'R' => WordKind::R,
            'S' => WordKind::S,
            'T' => WordKind::T,
            'X' => WordKind::X,
            'Y' => WordKind::Y,
            'Z' => WordKind::Z,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(v: f64) -> Option<GCode> {
        GCode::from_value(v)
    }

    #[test]
    fn motion_and_plane_and_units() {
        assert_eq!(g(0.0), Some(GCode::Rapid));
        assert_eq!(g(1.0), Some(GCode::Linear));
        assert_eq!(g(2.0), Some(GCode::ArcCw));
        assert_eq!(g(3.0), Some(GCode::ArcCcw));
        assert_eq!(g(17.0), Some(GCode::PlaneXY));
        assert_eq!(g(18.0), Some(GCode::PlaneZX));
        assert_eq!(g(19.0), Some(GCode::PlaneYZ));
        assert_eq!(g(20.0), Some(GCode::UnitsIn));
        assert_eq!(g(21.0), Some(GCode::UnitsMm));
    }

    #[test]
    fn frames_modes_and_specials() {
        assert_eq!(g(4.0), Some(GCode::Dwell));
        assert_eq!(g(10.0), Some(GCode::SetCoordSystem));
        assert_eq!(g(28.1), Some(GCode::SetHome));
        assert_eq!(g(30.1), Some(GCode::SetHome2));
        assert_eq!(g(38.2), Some(GCode::Probe(2)));
        assert_eq!(g(38.5), Some(GCode::Probe(5)));
        assert_eq!(g(40.0), Some(GCode::CutterCompOff));
        assert_eq!(g(43.1), Some(GCode::ToolLenDynamic));
        assert_eq!(g(49.0), Some(GCode::ToolLenOff));
        assert_eq!(g(53.0), Some(GCode::MachineFrame));
        assert_eq!(g(54.0), Some(GCode::Wcs(1)));
        assert_eq!(g(59.0), Some(GCode::Wcs(6)));
        assert_eq!(g(61.0), Some(GCode::PathExact));
        assert_eq!(g(80.0), Some(GCode::CycleCancel));
        assert_eq!(g(90.0), Some(GCode::DistAbs));
        assert_eq!(g(91.1), Some(GCode::ArcDistInc));
        assert_eq!(g(92.1), Some(GCode::ResetFramePreset));
        assert_eq!(g(93.0), Some(GCode::FeedInverse));
        assert_eq!(g(94.0), Some(GCode::FeedPerMin));
    }

    #[test]
    fn out_of_dictionary_is_none() {
        // N/A-колонка COVERAGE §2/§3: G5, G43 (только G43.1), G64, G90.1,
        // G54.1, G59.1, G92.2, G96, M6 (нет в словаре GRBL), M19.
        for v in [5.0, 43.0, 64.0, 90.1, 54.1, 59.1, 92.2, 96.0] {
            assert_eq!(g(v), None, "G{v}");
        }
        assert_eq!(MCode::from_value(6.0), None);
        assert_eq!(MCode::from_value(19.0), None);
    }

    #[test]
    fn m_codes_and_words() {
        assert_eq!(MCode::from_value(0.0), Some(MCode::ProgramStop));
        assert_eq!(MCode::from_value(1.0), Some(MCode::OptionalStop));
        assert_eq!(MCode::from_value(2.0), Some(MCode::ProgramEnd));
        assert_eq!(MCode::from_value(30.0), Some(MCode::ProgramEnd));
        assert_eq!(MCode::from_value(3.0), Some(MCode::SpindleCw));
        assert_eq!(MCode::from_value(8.0), Some(MCode::FloodOn));
        assert_eq!(MCode::from_value(9.0), Some(MCode::CoolantOff));
        assert_eq!(WordKind::from_letter('L'), Some(WordKind::L));
        assert_eq!(WordKind::from_letter('z'), Some(WordKind::Z));
        assert_eq!(WordKind::from_letter('H'), None);
    }
}
