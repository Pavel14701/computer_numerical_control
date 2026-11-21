//! Словарь Fanuc mill и модальное состояние канонизации (этапы 1–2, §1.1 отчёта).
//! Этап 1 покрывал группы 01–06; этап 2 добавляет: циклы (09), retract (10),
//! WCS-выбор (12), path (13), cancel-коды G40/G49 (07/08) и one-shot группу 00
//! (G4/G10/G11/G52/G53/G92).
//!
//! Коды с чисто RAW-диспозицией (G28/G43/G65/G84/…) — НЕ в enum: их различение
//! не влияет на state/опы — таблица [`g_raw_disposition`] (фиксированные
//! COVERAGE-коды §2/§3). Неизвестные коды — generic-ведро Raw{SYNTAX} с
//! диагностикой текста (COVERAGE §3: «не молча»).
//!
//! G70/G71 — псевдонимы units (mill-профиль, ruling 6 оркестратора); на
//! токарных сериях — циклы (коллизия — этап 8, COVERAGE `?`-реестр).

use eid_core::ast::{PathMode, Units, Wcs};

/// G-код словаря Fanuc mill (структурные коды — нужны state/op-эффект).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GCode {
    // Группа 01 — движение (явный код отменяет активный цикл группы 09)
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
    // Группа 04 — центр дуги (опция 16i/30i, `?` §10(1))
    ArcCenterAbs, // G90.1
    ArcCenterInc, // G91.1
    // Группа 05 — подача: G94 ABSORB; G93 ABSORB (inverse-time, ruling 4 —
    // разрешение feed = len·F, SPEC §4; эмиссия всегда G94); G95 → RAW
    // (детект через g_raw_disposition, до apply)
    FeedPerMin,  // G94
    FeedInverse, // G93
    FeedPerRev,  // G95 (детект для RAW)
    // Группа 06 — единицы (META); псевдонимы G70/G71 (mill-профиль, ruling 6)
    UnitsIn,      // G20
    UnitsMm,      // G21
    UnitsInAlias, // G70 → inch
    UnitsMmAlias, // G71 → mm
    // Группа 09 — циклы (развёртка §5 отчёта / §7.7 SPEC, Fanuc-семантика)
    CycleDrill,       // G81
    CycleDrillDwell,  // G82 (P — целые мс!)
    CyclePeck,        // G83 (полный отвод; δ = параметр станка → 0.5mm + notice)
    CycleChipBreak,   // G73 (de-retract на Q; δ → 0.5mm + notice)
    CycleBoreFeedOut, // G85 (отвод на подаче)
    CycleBoreStop,    // G86 (шпиндель off на дне, рестарт — Q9)
    CycleBoreDwell,   // G89 (bore + dwell, feed out)
    CycleCancel,      // G80 (IGNORED_SAFE; отмена cycle-моды)
    // Группа 10 — retract циклов (дефолт G99, SPEC §7.5a)
    RetractG98,
    RetractG99,
    // Группа 12 — WCS выбор (state.wcs, p=1..6 ⊂ EID p=1..9)
    Wcs54,
    Wcs55,
    Wcs56,
    Wcs57,
    Wcs58,
    Wcs59,
    // Группа 13 — path (Fanuc-СДВИГ значения: G61 = exact_stop, НЕ exact_path!
    // COVERAGE §2.2; G64 без P-квантификатора)
    PathExactStop,  // G61
    PathExactStop2, // G61.1 (синоним на части серий, `?` реестр)
    PathBlend,      // G64 (bare blend)
    // Группа 07/08 — cancel-коды семейств (правило семейств §1.3 COVERAGE:
    // IGNORED_SAFE, если в файле нет активных кодов семейства, иначе RAW)
    CompCancel,    // G40 (семья G40 Fanuc: G41/.1/.2-.6, G42/.1/.2-.6, G38, G39)
    LenCompCancel, // G49 (семья G49 Fanuc: G43, G43.1, G43.4, G43.5, G44)
    // Группа 00 — one-shot
    Dwell,      // G4 (P — целые мс; X/U — секунды float)
    DataRecord, // G10 (L2 P1-6 → wcs_define; L20/P0/L1+ → RAW)
    DataCancel, // G11 (IGNORED_SAFE / RAW по семейству G11)
    LocalCs,    // G52 → Err(E_UNSUPPORTED_SEMANTICS) — единственный жёсткий отказ
    MachineCs,  // G53 (разовый ход в machine CS, правило §5.3 SPEC)
    Preset,     // G92 (пресетка до первого motion-опа; S-слово → RAW)
}

/// M-код словаря Fanuc mill (этап 2, §3 отчёта / COVERAGE §3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MCode {
    ProgramEnd,    // M2 / M30 — различие вне L1 (SPEC §5.2)
    ProgramStop,   // M0 (optional=false)
    OptionalStop,  // M1 (optional=true; Q8)
    SpindleCw,     // M3
    SpindleCcw,    // M4
    SpindleOff,    // M5
    ToolChange,    // M6 (+T в блоке; одиночное T → RAW SEMANTICS — предвыбор)
    CoolantMist,   // M7 (опция)
    CoolantFlood,  // M8
    CoolantOff,    // M9
    SpindleOrient, // M19 → RAW{E_UNSUPPORTED_SEMANTICS} (энкодер)
    RigidTapMode,  // M29 → RAW{E_UNSUPPORTED_SEMANTICS} (paired G84)
    SubCall,       // M98 → RAW{E_UNSUPPORTED_SYNTAX} (Macro B / subs)
    SubReturn,     // M99 → RAW{E_UNSUPPORTED_SYNTAX}
    SubCallExt,    // M198 → RAW{E_UNSUPPORTED_SYNTAX} (внешний файл)
}

/// Слова-параметры словаря этапа 2. X/Y/Z — опы; I/J/K/R — дуги; F — подача;
/// S — обороты; P/Q/K/L — циклы/G4/G10; T — инструмент; H/D — компенсации
/// (в составе RAW G43/G41-строк); O — программа/subs (RAW SYNTAX);
/// U/V/W/A/B/C/E — вне скоупа (RAW SEMANTICS/SYNTAX; исключение: U в G04 =
/// dwell-секунды — разбор контекстно, §4.1 отчёта).
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
    U,
    V,
    W,
    A,
    B,
    C,
    E,
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
            700 => GCode::UnitsInAlias,
            710 => GCode::UnitsMmAlias,
            730 => GCode::CycleChipBreak,
            800 => GCode::CycleCancel,
            810 => GCode::CycleDrill,
            820 => GCode::CycleDrillDwell,
            830 => GCode::CyclePeck,
            850 => GCode::CycleBoreFeedOut,
            860 => GCode::CycleBoreStop,
            890 => GCode::CycleBoreDwell,
            980 => GCode::RetractG98,
            990 => GCode::RetractG99,
            540 => GCode::Wcs54,
            550 => GCode::Wcs55,
            560 => GCode::Wcs56,
            570 => GCode::Wcs57,
            580 => GCode::Wcs58,
            590 => GCode::Wcs59,
            610 => GCode::PathExactStop,
            611 => GCode::PathExactStop2,
            640 => GCode::PathBlend,
            400 => GCode::CompCancel,
            490 => GCode::LenCompCancel,
            40 => GCode::Dwell,
            100 => GCode::DataRecord,
            110 => GCode::DataCancel,
            520 => GCode::LocalCs,
            530 => GCode::MachineCs,
            920 => GCode::Preset,
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

    /// WCS-выбор группы 12 → Wcs (Fanuc 6 систем, p=1..6).
    pub fn wcs(&self) -> Option<Wcs> {
        Some(match self {
            GCode::Wcs54 => Wcs::G54,
            GCode::Wcs55 => Wcs::G55,
            GCode::Wcs56 => Wcs::G56,
            GCode::Wcs57 => Wcs::G57,
            GCode::Wcs58 => Wcs::G58,
            GCode::Wcs59 => Wcs::G59,
            _ => return None,
        })
    }

    /// Path-режим группы 13 — Fanuc-сдвиг значения: G61 = exact_stop
    /// (Fanuc G61 — останов в углу; LinuxCNC G61 = exact_path — COVERAGE §2.2).
    pub fn path_mode(&self) -> Option<PathMode> {
        Some(match self {
            GCode::PathExactStop | GCode::PathExactStop2 => PathMode::ExactStop,
            GCode::PathBlend => PathMode::Blend { t: None },
            _ => return None,
        })
    }
}

impl MCode {
    /// Значение M-слова → код словаря; None = вне словаря (generic-ведро).
    pub fn from_value(value: f64) -> Option<MCode> {
        let t = (value * 10.0).round() as i64;
        Some(match t {
            20 | 300 => MCode::ProgramEnd,
            0 => MCode::ProgramStop,
            10 => MCode::OptionalStop,
            30 => MCode::SpindleCw,
            40 => MCode::SpindleCcw,
            50 => MCode::SpindleOff,
            60 => MCode::ToolChange,
            70 => MCode::CoolantMist,
            80 => MCode::CoolantFlood,
            90 => MCode::CoolantOff,
            190 => MCode::SpindleOrient,
            290 => MCode::RigidTapMode,
            980 => MCode::SubCall,
            990 => MCode::SubReturn,
            1980 => MCode::SubCallExt,
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
            'U' => WordKind::U,
            'V' => WordKind::V,
            'W' => WordKind::W,
            'A' => WordKind::A,
            'B' => WordKind::B,
            'C' => WordKind::C,
            'E' => WordKind::E,
            _ => return None,
        })
    }
}

/// Модальные группы (§1.1 отчёта, этап 2: 00 one-shot — без группы; 01–06, 07/08
/// cancel, 09 циклы, 10 retract, 12 wcs, 13 path). Один код на группу на блок;
/// нарушение — диагностика (block.modal_violation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalGroup {
    Motion01,    // G00/G01/G02/G03
    Plane02,     // G17/G18/G19
    Distance03,  // G90/G91
    ArcCenter04, // G90.1/G91.1 (опция; `?` §10(1))
    FeedMode05,  // G93/G94/G95
    Units06,     // G20/G21/G70/G71
    Comp07,      // G40 cancel (G41/G42 — вне enum, raw-таблица)
    LenComp08,   // G49 cancel (G43/G44 — вне enum, raw-таблица)
    Cycle09,     // G73/G80/G81-G89 (развёртываемые + cancel)
    Retract10,   // G98/G99
    Wcs12,       // G54–G59
    Path13,      // G61/G61.1/G64
}

pub fn modal_group(g: GCode) -> Option<ModalGroup> {
    Some(match g {
        GCode::Rapid | GCode::Linear | GCode::ArcCw | GCode::ArcCcw => ModalGroup::Motion01,
        GCode::PlaneXY | GCode::PlaneZX | GCode::PlaneYZ => ModalGroup::Plane02,
        GCode::DistAbs | GCode::DistInc => ModalGroup::Distance03,
        GCode::ArcCenterAbs | GCode::ArcCenterInc => ModalGroup::ArcCenter04,
        GCode::FeedPerMin | GCode::FeedInverse | GCode::FeedPerRev => ModalGroup::FeedMode05,
        GCode::UnitsIn | GCode::UnitsMm | GCode::UnitsInAlias | GCode::UnitsMmAlias => {
            ModalGroup::Units06
        }
        GCode::CompCancel => ModalGroup::Comp07,
        GCode::LenCompCancel => ModalGroup::LenComp08,
        GCode::CycleChipBreak
        | GCode::CycleCancel
        | GCode::CycleDrill
        | GCode::CycleDrillDwell
        | GCode::CyclePeck
        | GCode::CycleBoreFeedOut
        | GCode::CycleBoreStop
        | GCode::CycleBoreDwell => ModalGroup::Cycle09,
        GCode::RetractG98 | GCode::RetractG99 => ModalGroup::Retract10,
        GCode::Wcs54 | GCode::Wcs55 | GCode::Wcs56 | GCode::Wcs57 | GCode::Wcs58 | GCode::Wcs59 => {
            ModalGroup::Wcs12
        }
        GCode::PathExactStop | GCode::PathExactStop2 | GCode::PathBlend => ModalGroup::Path13,
        // Группа 00 (one-shot) — без модальной группы.
        GCode::Dwell
        | GCode::DataRecord
        | GCode::DataCancel
        | GCode::LocalCs
        | GCode::MachineCs
        | GCode::Preset => return None,
    })
}

/// Циклы Fanuc, разворачиваемые по канон-шаблону §7.7 SPEC (Fanuc-семантика
/// §5 отчёта). G74/G76/G84/G87/G88 — raw-таблица (SEMANTICS), не здесь.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleKind {
    Drill,       // G81
    DrillDwell,  // G82 (P мс)
    Peck,        // G83 (полный отвод к R)
    ChipBreak,   // G73 (de-retract на Q)
    BoreFeedOut, // G85
    BoreStop,    // G86 (шпиндель off + рестарт)
    BoreDwell,   // G89
}

impl CycleKind {
    pub fn from_gcode(g: GCode) -> Option<CycleKind> {
        Some(match g {
            GCode::CycleDrill => CycleKind::Drill,
            GCode::CycleDrillDwell => CycleKind::DrillDwell,
            GCode::CyclePeck => CycleKind::Peck,
            GCode::CycleChipBreak => CycleKind::ChipBreak,
            GCode::CycleBoreFeedOut => CycleKind::BoreFeedOut,
            GCode::CycleBoreStop => CycleKind::BoreStop,
            GCode::CycleBoreDwell => CycleKind::BoreDwell,
            _ => return None,
        })
    }

    /// Циклы с dwell-P (G82/G89 — обязателен, G86 — опционален `?` §10(6)).
    pub fn takes_p(&self) -> bool {
        matches!(self, CycleKind::DrillDwell | CycleKind::BoreDwell)
    }

    /// Пекерующие (Q обязателен, > 0).
    pub fn takes_q(&self) -> bool {
        matches!(self, CycleKind::Peck | CycleKind::ChipBreak)
    }
}

// ---------------------------------------------------------------------------
// RAW-диспозиции G-кодов вне структурного словаря (COVERAGE §2 Fanuc-колонка)
// ---------------------------------------------------------------------------

/// RAW-код + причина для G-кода вне enum-словаря. `t = round(value*10)`.
/// Возврат None → generic-ведро Raw{E_UNSUPPORTED_SYNTAX} с текстом кода
/// (COVERAGE §3: «не молча»).
pub fn g_raw_disposition(value: f64) -> Option<(eid_core::diag::ErrorCode, &'static str)> {
    use eid_core::diag::ErrorCode::{
        RequiresMachineTable as RT, UnsupportedSemantics as US, UnsupportedSyntax as UX,
    };
    let t = (value * 10.0).round() as i64;
    if (value * 10.0).round() != value * 10.0 {
        return None; // лишние десятичные знаки — generic-ведро
    }
    Some(match t {
        // Home/reference (Q1) — COVERAGE §2.3
        270 => (RT, "G27 reference return check — home position from machine parameters (COVERAGE §2.3; Q1)"),
        280 => (RT, "G28 reference return via intermediate point — reference position outside text (COVERAGE §2.3; Q1)"),
        290 => (RT, "G29 return from reference — reference position outside text (COVERAGE §2.3; Q1)"),
        300 => (RT, "G30 2nd/3rd/4th reference return — home position outside text (COVERAGE §2.3; Q1)"),
        301 => (RT, "G30.1 floating reference — machine-side data (COVERAGE §2.3; Q1 `?`)"),
        // Таблицы станка
        450..=480 => (RT, "G45–G48 ±H-offset positioning — offset value from machine table (COVERAGE §2.6)"),
        // Path/override one-shot — COVERAGE §2.2
        90 => (US, "G09 exact stop one-shot — path_mode is program-global in L1 (COVERAGE §2.2)"),
        600 => (US, "G60 one-shot positioning — approach geometry shift not representable (COVERAGE §2.2)"),
        620 => (US, "G62 automatic corner override — feed cut from machine parameters (COVERAGE §2.2)"),
        630 => (US, "G63 tapping mode — paired with G84 spindle sync (COVERAGE §2.2)"),
        // CSS (токарное) — COVERAGE §2.6
        960 => (US, "G96 constant surface speed — lathe CSS, out of scope (COVERAGE §2.6)"),
        970 => (US, "G97 CSS cancel — lathe, out of scope (COVERAGE §2.6)"),
        // Трансформации СК — кандидаты этапа 12 — COVERAGE §2.6
        680 => (US, "G68 coordinate system rotation — stage-12 decomposition candidate (COVERAGE §2.6)"),
        690 => (US, "G69 rotation cancel — family G68 (COVERAGE §2.6)"),
        500 => (US, "G50 scaling cancel / spindle clamp — stage-12 candidate (COVERAGE §2.6)"),
        510 => (US, "G51 scaling — stage-12 decomposition candidate (COVERAGE §2.6)"),
        501 => (US, "G50.1 mirror cancel — mirror flips climb/conventional, stage-12 (COVERAGE §2.6)"),
        511 => (US, "G51.1 programmable mirror — stage-12 candidate (COVERAGE §2.6)"),
        512 => (US, "G51.2 polygon turning — lathe (COVERAGE §2.6)"),
        502 => (US, "G50.2 polygon turning cancel — lathe (COVERAGE §2.6)"),
        150 => (US, "G15 polar coordinates cancel — CS transformation, class G68 (COVERAGE §2.2)"),
        160 => (US, "G16 polar coordinates — CS transformation, class G68 (COVERAGE §2.2)"),
        // Софт-лимиты / мониторинг — COVERAGE §2.2
        220 => (US, "G22 stored stroke check — soft limits from machine parameters (COVERAGE §2.2)"),
        230 => (US, "G23 stroke check off — machine-side (COVERAGE §2.2)"),
        250 => (US, "G25 spindle fluctuation detect — monitoring, not motion (COVERAGE §2.2)"),
        260 => (US, "G26 spindle fluctuation detect — monitoring (COVERAGE §2.2)"),
        // Сенсорные — COVERAGE §2.1/§2.2
        310 => (US, "G31 skip function (probing) — sensor semantics, stage 12 (COVERAGE §2.1)"),
        318 => (UX, "G31.8 EGB-skip — vendor option (COVERAGE §2.1)"),
        370 => (US, "G37 auto tool length measurement — sensor semantics (COVERAGE §2.2)"),
        // Threading (синхронизация со шпинделем) — COVERAGE §2.1
        330 => (US, "G33 threading — spindle sync (COVERAGE §2.1)"),
        340 => (US, "G34 variable lead threading — spindle sync (COVERAGE §2.1)"),
        350 => (US, "G35 circular threading — spindle sync (COVERAGE §2.1)"),
        360 => (US, "G36 circular threading — spindle sync (COVERAGE §2.1)"),
        // Опции интерполяции — COVERAGE §2.1
        22 => (US, "G02.2 evolvent interpolation — option, stage-12 candidate (COVERAGE §2.1)"),
        32 => (US, "G03.2 evolvent interpolation — option (COVERAGE §2.1)"),
        23 => (US, "G02.3 exponential interpolation — option (COVERAGE §2.1)"),
        33 => (US, "G03.3 exponential interpolation — option (COVERAGE §2.1)"),
        24 => (US, "G02.4 3D coordinate conversion — 5-axis option (COVERAGE §2.1)"),
        34 => (US, "G03.4 3D coordinate conversion — 5-axis option (COVERAGE §2.1)"),
        62 => (US, "G06.2 NURBS interpolation — Raw class per SPEC phase 1.2 (COVERAGE §2.1)"),
        // Вендорские высокоточные режимы — COVERAGE §2.1
        50 => (UX, "G05 AI contour — vendor high-precision mode, outside static contract (COVERAGE §2.1)"),
        51 => (UX, "G05.1 AI contour control — vendor mode (COVERAGE §2.1)"),
        80 => (UX, "G08 Nano smoothing — vendor mode (COVERAGE §2.6)"),
        70 => (US, "G07 hypothetical axis — nonlinear axis mapping (COVERAGE §2.1)"),
        71 => (US, "G07.1 cylindrical interpolation — option (COVERAGE §2.1)"),
        121 => (US, "G12.1 polar interpolation — option (COVERAGE §2.1)"),
        131 => (US, "G13.1 polar interpolation cancel — option (COVERAGE §2.1)"),
        531 => (US, "G53.1 tool axis direction control (RTCP) — 5-axis, stage 12 (COVERAGE §2.3)"),
        682 => (US, "G68.2 feature CS — 5-axis (COVERAGE §2.3)"),
        // G10-семейство (не L2) — COVERAGE §2.3/§2.6
        106 => (US, "G10.6 retract & recover — tool load/unload (COVERAGE §2.6)"),
        109 => (US, "G10.9 diameter/radius switch — lathe-related (COVERAGE §2.6)"),
        // Figure copy — COVERAGE §2.6
        721 => (US, "G72.1 figure rotation copy — subprogram unroll, outside L1 (COVERAGE §2.6)"),
        722 => (US, "G72.2 figure translation copy — subprogram unroll (COVERAGE §2.6)"),
        // Циклы-отказы — COVERAGE §2.5
        740 => (US, "G74 left-hand tapping — spindle sync/reverse (COVERAGE §2.5)"),
        760 => (US, "G76 fine boring w/ shift — spindle orientation (M19-class) + Q-shift from machine parameter 5101, double refusal (COVERAGE §2.5; Q6 closed)"),
        840 => (US, "G84 right-hand tapping — spindle sync (COVERAGE §2.5)"),
        842 => (US, "G84.2 rigid tapping — spindle sync (COVERAGE §2.5)"),
        843 => (US, "G84.3 rigid tapping — spindle sync (COVERAGE §2.5)"),
        870 => (US, "G87 back boring — spindle orientation + I/J shift + manual intervention (COVERAGE §2.5)"),
        880 => (US, "G88 bore manual stop — feed hold/jog not static (COVERAGE §2.5)"),
        805 => (UX, "G80.5 high-speed retract — vendor option 30i (COVERAGE §2.5)"),
        808 => (UX, "G80.8 high-speed retract — vendor option 30i (COVERAGE §2.5)"),
        811 => (UX, "G81.1 chopping — vendor option 30i (COVERAGE §2.5)"),
        815 => (UX, "G81.5 EGB — vendor option (COVERAGE §2.5)"),
        818 => (UX, "G81.8 EGB — vendor option (COVERAGE §2.5)"),
        // G92.1/.2/.3 (Q4) — COVERAGE §2.3
        921 => (US, "G92.1 WCS-shift reset — Q4 (COVERAGE §2.3)"),
        922 => (US, "G92.2 WCS-shift suspend — Q4 `?` (COVERAGE §2.3)"),
        923 => (US, "G92.3 WCS-shift resume — Q4 `?` (COVERAGE §2.3)"),
        // G54.1 Pn — вне wcs-enum L1 — COVERAGE §2.3
        541 => (US, "G54.1 Pn extended WCS — outside L1 wcs enum; stage-12 candidate (COVERAGE §2.3)"),
        // Macro B — COVERAGE §4
        650 => (UX, "G65 macro call — Macro B parametric, line-level Raw (COVERAGE §4)"),
        660 => (UX, "G66 modal macro call — Macro B (COVERAGE §4)"),
        661 => (UX, "G66.1 modal macro call — Macro B (COVERAGE §4)"),
        670 => (UX, "G67 modal macro cancel — Macro B (COVERAGE §4)"),
        // Компенсации (активные коды семейств) — COVERAGE §2.4
        410 => (US, "G41 cutter comp left +D — comp geometry not a goal (COVERAGE §2.4)"),
        420 => (US, "G42 cutter comp right +D — comp geometry (COVERAGE §2.4)"),
        411 => (US, "G41.1 cutter comp — option form (COVERAGE §2.4)"),
        421 => (US, "G42.1 cutter comp — option form (COVERAGE §2.4)"),
        412..=416 => (US, "G41.2–.6 high-precision comp — option (COVERAGE §1.3 G40-family)"),
        422..=426 => (US, "G42.2–.6 high-precision comp — option (COVERAGE §1.3 G40-family)"),
        380 => (US, "G38 preserve vector — cutter-comp class (COVERAGE §2.4)"),
        390 => (US, "G39 corner circular — cutter-comp class (COVERAGE §2.4)"),
        // G38.2–.5 (ruling оркестратора этап 3): пробинг — сенсорная семантика,
        // класс = G31/G37 (COVERAGE §2.1: «пробинг — этап 12»); stop-position
        // зависит от контакта датчика — не статично.
        382..=385 => (US, "G38.2–.5 probing — sensor semantics, class G31/G37 (ruling stage 3; COVERAGE §2.1)"),
        // TLO из таблицы — COVERAGE §2.4
        430 => (RT, "G43 tool length offset +H — offset from machine table (COVERAGE §2.4)"),
        431 => (RT, "G43.1 dynamic TLO — option `?`; candidate revision `tlo` (COVERAGE §2.4)"),
        434 => (US, "G43.4 TCP — 5-axis tool center point, stage 12 (COVERAGE §2.4)"),
        435 => (US, "G43.5 tool axis direction control — 5-axis (COVERAGE §2.4)"),
        440 => (RT, "G44 tool length offset −H — offset from machine table (COVERAGE §2.4)"),
        // G95 feed/rev — COVERAGE §2.2
        950 => (US, "G95 feed-per-rev: resolution needs spindle rpm (override-dependent) — not representable (COVERAGE §2.2)"),
        _ => return None,
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

/// Модальное состояние канонизации. Дефолты Fanuc mill (COVERAGE §0.3:
/// модальные значения на входе = дефолты контроллера): G17, G90,
/// G91.1-эквивалент (инкрементальный центр), G94, mm, G54, G99 (SPEC §7.5a),
/// exact_path (отсутствие G61/G64 — SPEC §3 v0.12).
#[derive(Debug, Clone, PartialEq)]
pub struct ModalState {
    pub motion: Option<GCode>,   // группа 01 (модальное продолжение, §2.7)
    pub plane: GCode,            // группа 02
    pub dist_abs: bool,          // группа 03 (true = G90)
    pub arc_center_abs: bool,    // группа 04 (true = G90.1)
    pub feed_per_min: bool,      // группа 05 (true = G94)
    pub feed_inverse: bool,      // группа 05: G93 (ruling 4 — ABSORB)
    pub units: Units,            // группа 06
    pub units_established: bool, // первое слово units устанавливает (SPEC §2.5)
    pub wcs: Wcs,                // группа 12 (G54 дефолт контроллера)
    pub retract_g98: bool,       // группа 10 (дефолт G99 — SPEC §7.5a)
    pub path: Option<PathMode>,  // группа 13 (первое вхождение устанавливает)
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
            retract_g98: false,
            path: None,
        }
    }
}

impl ModalState {
    /// Поглощение кода модальной группы (вызов — уже после проверки violation,
    /// unit-switch и path-switch в canonicalize; G95/cycle-коды — до apply).
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
            GCode::UnitsIn | GCode::UnitsInAlias => self.units = Units::In,
            GCode::UnitsMm | GCode::UnitsMmAlias => self.units = Units::Mm,
            GCode::RetractG98 => self.retract_g98 = true,
            GCode::RetractG99 => self.retract_g98 = false,
            GCode::Wcs54
            | GCode::Wcs55
            | GCode::Wcs56
            | GCode::Wcs57
            | GCode::Wcs58
            | GCode::Wcs59 => {
                if let Some(w) = g.wcs() {
                    self.wcs = w;
                }
            }
            // Path-режим — в канонизаторе (switch → Err); cycle-коды — Ctx.
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
        assert_eq!(g(90.1), Some(GCode::ArcCenterAbs));
        assert_eq!(g(91.1), Some(GCode::ArcCenterInc));
        assert_eq!(
            modal_group(GCode::ArcCenterAbs),
            Some(ModalGroup::ArcCenter04)
        );
        assert_eq!(g(93.0), Some(GCode::FeedInverse));
        assert_eq!(g(94.0), Some(GCode::FeedPerMin));
        assert_eq!(g(95.0), Some(GCode::FeedPerRev));
        assert_eq!(
            modal_group(GCode::FeedInverse),
            Some(ModalGroup::FeedMode05)
        );
        // Псевдонимы units G70/G71 (§2.2, ruling 6).
        assert_eq!(g(20.0), Some(GCode::UnitsIn));
        assert_eq!(g(21.0), Some(GCode::UnitsMm));
        assert_eq!(g(70.0), Some(GCode::UnitsInAlias));
        assert_eq!(g(71.0), Some(GCode::UnitsMmAlias));
        assert_eq!(GCode::UnitsInAlias.units(), Some(Units::In));
        assert_eq!(GCode::UnitsMmAlias.units(), Some(Units::Mm));
        assert_eq!(modal_group(GCode::UnitsMmAlias), Some(ModalGroup::Units06));
    }

    #[test]
    fn cycles_retract_wcs_path_groups() {
        // Группа 09 (циклы) + G80.
        for (v, k) in [
            (73.0, CycleKind::ChipBreak),
            (81.0, CycleKind::Drill),
            (82.0, CycleKind::DrillDwell),
            (83.0, CycleKind::Peck),
            (85.0, CycleKind::BoreFeedOut),
            (86.0, CycleKind::BoreStop),
            (89.0, CycleKind::BoreDwell),
        ] {
            assert_eq!(g(v).and_then(CycleKind::from_gcode), Some(k), "G{v}");
            assert_eq!(modal_group(g(v).unwrap()), Some(ModalGroup::Cycle09));
        }
        assert_eq!(g(80.0), Some(GCode::CycleCancel));
        assert_eq!(CycleKind::from_gcode(GCode::CycleCancel), None);
        // Группа 10 (retract), дефолт G99.
        assert_eq!(g(98.0), Some(GCode::RetractG98));
        assert_eq!(g(99.0), Some(GCode::RetractG99));
        // Группа 12 (wcs).
        assert_eq!(g(54.0).and_then(|c| c.wcs()), Some(Wcs::G54));
        assert_eq!(g(59.0).and_then(|c| c.wcs()), Some(Wcs::G59));
        assert_eq!(modal_group(GCode::Wcs57), Some(ModalGroup::Wcs12));
        // Группа 13 (path) — Fanuc value shift: G61 = exact_stop.
        assert_eq!(
            g(61.0).and_then(|c| c.path_mode()),
            Some(PathMode::ExactStop)
        );
        assert_eq!(
            g(61.1).and_then(|c| c.path_mode()),
            Some(PathMode::ExactStop)
        );
        assert_eq!(
            g(64.0).and_then(|c| c.path_mode()),
            Some(PathMode::Blend { t: None })
        );
        // Группа 00 one-shot + cancel-коды.
        assert_eq!(g(4.0), Some(GCode::Dwell));
        assert_eq!(g(10.0), Some(GCode::DataRecord));
        assert_eq!(g(11.0), Some(GCode::DataCancel));
        assert_eq!(g(52.0), Some(GCode::LocalCs));
        assert_eq!(g(53.0), Some(GCode::MachineCs));
        assert_eq!(g(92.0), Some(GCode::Preset));
        assert_eq!(g(40.0), Some(GCode::CompCancel));
        assert_eq!(g(49.0), Some(GCode::LenCompCancel));
        assert_eq!(modal_group(GCode::CompCancel), Some(ModalGroup::Comp07));
        assert_eq!(
            modal_group(GCode::LenCompCancel),
            Some(ModalGroup::LenComp08)
        );
    }

    #[test]
    fn out_of_dictionary_is_none_but_raw_table_known() {
        // G2.2/G33/G43/G65/G84/G92.1/G54.1/G28 → вне enum (raw-таблица/generic).
        for v in [2.2, 33.0, 43.0, 65.0, 84.0, 92.1, 54.1, 28.0, 96.0] {
            assert_eq!(g(v), None, "G{v} — вне enum");
        }
        // Raw-таблица даёт фиксированные коды.
        let (c, _) = g_raw_disposition(43.0).unwrap();
        assert_eq!(c, ErrorCode::RequiresMachineTable);
        let (c, _) = g_raw_disposition(84.0).unwrap();
        assert_eq!(c, ErrorCode::UnsupportedSemantics);
        let (c, _) = g_raw_disposition(65.0).unwrap();
        assert_eq!(c, ErrorCode::UnsupportedSyntax);
        let (c, _) = g_raw_disposition(52.1).unwrap_or((ErrorCode::InvalidInput, ""));
        // G52 — enum (Err в канонизаторе), G52.x — generic (SYNTAX).
        assert_eq!(g_raw_disposition(52.0), None);
        let _ = c;
        // Неизвестный код → None (generic-ведро SYNTAX).
        assert_eq!(g_raw_disposition(88.8), None);
        // G95 — raw-таблица (SEMANTICS).
        let (c, r) = g_raw_disposition(95.0).unwrap();
        assert_eq!(c, ErrorCode::UnsupportedSemantics);
        assert!(r.contains("override"));
    }

    #[test]
    fn m_codes_stage2_dictionary() {
        assert_eq!(MCode::from_value(2.0), Some(MCode::ProgramEnd));
        assert_eq!(MCode::from_value(30.0), Some(MCode::ProgramEnd));
        assert_eq!(MCode::from_value(0.0), Some(MCode::ProgramStop));
        assert_eq!(MCode::from_value(1.0), Some(MCode::OptionalStop));
        assert_eq!(MCode::from_value(3.0), Some(MCode::SpindleCw));
        assert_eq!(MCode::from_value(5.0), Some(MCode::SpindleOff));
        assert_eq!(MCode::from_value(6.0), Some(MCode::ToolChange));
        assert_eq!(MCode::from_value(8.0), Some(MCode::CoolantFlood));
        assert_eq!(MCode::from_value(9.0), Some(MCode::CoolantOff));
        assert_eq!(MCode::from_value(19.0), Some(MCode::SpindleOrient));
        assert_eq!(MCode::from_value(29.0), Some(MCode::RigidTapMode));
        assert_eq!(MCode::from_value(98.0), Some(MCode::SubCall));
        assert_eq!(MCode::from_value(99.0), Some(MCode::SubReturn));
        assert_eq!(MCode::from_value(198.0), Some(MCode::SubCallExt));
        // M64/M90/M730 — builder-specific generic-ведро → None.
        for v in [64.0, 90.0, 730.0] {
            assert_eq!(MCode::from_value(v), None, "M{v}");
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
            ('U', WordKind::U),
            ('V', WordKind::V),
            ('W', WordKind::W),
            ('A', WordKind::A),
            ('B', WordKind::B),
            ('C', WordKind::C),
            ('E', WordKind::E),
        ] {
            assert_eq!(WordKind::from_letter(c), Some(k), "{c}");
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
        // Дефолты: G17/G90/инкрементальный центр/G94/mm/G54/G99.
        assert_eq!(m.plane, GCode::PlaneXY);
        assert!(m.dist_abs);
        assert!(!m.arc_center_abs);
        assert!(m.feed_per_min);
        assert!(!m.feed_inverse);
        assert_eq!(m.units, Units::Mm);
        assert!(!m.units_established);
        assert_eq!(m.wcs, Wcs::G54);
        assert!(!m.retract_g98);
        assert_eq!(m.path, None);
        assert_eq!(m.motion, None);
        m.apply(GCode::ArcCcw);
        m.apply(GCode::PlaneYZ);
        m.apply(GCode::DistInc);
        m.apply(GCode::ArcCenterAbs);
        m.apply(GCode::UnitsInAlias);
        m.apply(GCode::Wcs57);
        m.apply(GCode::RetractG98);
        assert_eq!(m.motion, Some(GCode::ArcCcw));
        assert_eq!(m.plane, GCode::PlaneYZ);
        assert!(!m.dist_abs);
        assert!(m.arc_center_abs);
        assert_eq!(m.units, Units::In);
        assert_eq!(m.wcs, Wcs::G57);
        assert!(m.retract_g98);
        // G93/G94 (ruling 4).
        m.apply(GCode::FeedInverse);
        assert!(m.feed_inverse);
        assert!(!m.feed_per_min);
        m.apply(GCode::FeedPerMin);
        assert!(!m.feed_inverse);
        assert!(m.feed_per_min);
        m.apply(GCode::UnitsMm);
        assert_eq!(m.units, Units::Mm);
        m.apply(GCode::Wcs54);
        assert_eq!(m.wcs, Wcs::G54);
    }
}
