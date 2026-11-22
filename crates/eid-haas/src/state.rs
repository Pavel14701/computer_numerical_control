//! Словарь Haas mill NGC и модальное состояние канонизации (этапы 1–2, §1.1
//! контракта). Этап 1: лексер/движение/модальности 01–06+15. Этап 2: фреймы
//! (WCS-слоты G54–G59 + **G110–G112 → p=7..9** — нативная симметрия EID-слотов
//! #7–#9, G154 P1–P3 → 7..9, G113+ → Raw; G52-ERR; G10 L2/L20/P0; G92; G53),
//! циклы ×Haas-семантика (09 + retract 10: **L-повторы — НЕ K**, K = min-пек
//! G83, G73 Setting 22, P-dot dwell), компенсации (G40/G49 — Haas-составы
//! семейств), M-словарь (M0/M1/M6/M16; вендорское ведро M07-Shower, M10–M13
//! тормоза, M29-relay, M97/M98/M99, M19, M109).
//!
//! Коды с чисто RAW-диспозицией (G28/G31/G43/G65/G74/…) — НЕ в enum: их
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
//! * **K ≠ повтор цикла** (§1.2): K = min-пек G83 / угловой шаг G12; повторы —
//!   словом **L** (инверсия имён vs Fanuc — §5.2 отчёта).
//! * **G10 L2 P0 = запись слота G52** (Haas UK) — НЕ external-offset Fanuc;
//!   G10 L20 = запись origin G110–G129/G154 — НЕ wcs_set_here (ловушка №2b).

use eid_core::ast::{PathMode, Units, Wcs};

/// G-код словаря Haas mill NGC (структурные коды — нужны state/op-эффект).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GCode {
    // Группа 01 — движение (явный код отменяет активный цикл группы 09;
    // G01 с C/R-словами = фаска/скругление → Raw в канонизаторе)
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
    // инкрементальные от start). G90.1/G91.1 — вне словаря; ruling этапа 1 —
    // ABSORB как LinuxCNC, помечено `?` (несверено; дефолт — инкрементальный)
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
    // Группа 09 — циклы (развёртка §7.7 SPEC × Haas-семантика §5.4 отчёта:
    // L-повторы, K = min-пек G83, P-dot dwell, G73 δ = Setting 22)
    CycleDrill,       // G81
    CycleDrillDwell,  // G82 (P — dwell: точка = сек / без = мс)
    CyclePeck,        // G83 (полный отвод; Q ИЛИ I/J/K — mutually exclusive)
    CycleChipBreak,   // G73 (отвод δ = Setting 22 → канон 0.01in unit-scaled)
    CycleBoreFeedOut, // G85 (отвод на подаче)
    CycleBoreStop,    // G86 (шпиндель off на дне + рестарт; P обязателен v0.18)
    CycleBoreDwell,   // G89 (bore + dwell, feed out)
    CycleCancel,      // G80 (IGNORED_SAFE; отмена cycle-моды)
    // Группа 10 — retract циклов (дефолт G99, SPEC §7.5a)
    RetractG98,
    RetractG99,
    // Группа 12 — WCS выбор: G54–G59 → p=1..6; **G110–G112 → слоты #7–#9 =
    // ровно EID p=7..9** (нативная симметрия нумерации, эмиссия этапа 3 —
    // G110–G112 с notice; COVERAGE Haas-колонка v0.9 — RAW консервативно,
    // бриф этапа 2 — ABSORB p=7..9); G154 Pn — P1–P3 → 7..9 (алиасы
    // G110–G112), P4+ → Raw
    Wcs54,  // G54
    Wcs55,  // G55
    Wcs56,  // G56
    Wcs57,  // G57
    Wcs58,  // G58
    Wcs59,  // G59
    Wcs110, // G110 → слот #7 → Wcs::G59_1 (p7)
    Wcs111, // G111 → слот #8 → Wcs::G59_2 (p8)
    Wcs112, // G112 → слот #9 → Wcs::G59_3 (p9)
    Wcs154, // G154 Pn: P1–P3 → p=7..9, P4+ → Raw (канонизатор)
    // Группа 07/08 — cancel-коды семейств (правило семейств §1.3 COVERAGE,
    // HAAS-СОСТАВЫ: G40 = {G41, G42, G141}; G49 = {G43, G44, G143};
    // IGNORED_SAFE, если в файле нет активных кодов семейства, иначе RAW)
    CompCancel,    // G40
    LenCompCancel, // G49
    // Группа 00 — one-shot
    Dwell,      // G4 (P-dot dwell: точка = сек / без = мс; X/U-форм НЕТ)
    DataRecord, // G10 (L2 P1–P6 → wcs_define; L2 P0 = слот G52 → Raw RT;
                // L20 → Raw RT НЕ wcs_set_here; L1/L10–L13 → Raw RT)
    LocalCs,    // G52 → Err(E_UNSUPPORTED_SEMANTICS) — 2-й жёсткий отказ
    MachineCs,  // G53 (разовый ход в machine CS, правило §5.3 SPEC)
    Preset,     // G92 (только пресетка; S-слово → Raw SEM — кламп-формы нет)
    // Группа 15 — path (семейство exact-stop; СДВИГ значения vs LinuxCNC:
    // G61 = exact_stop, G64 = cancel G61 — bare, квантификатора t НЕТ)
    PathExactStop, // G61
    PathBlend,     // G64 (bare blend)
}

/// M-код словаря Haas mill NGC (этап 2, §3 отчёта / COVERAGE §3). Структурные
/// коды → опы (M0/M1/M2/M30/M6/M16); вендорские документированные — в словаре
/// ради ДИСПОЗИЦИИ (Raw-класс канонизатора), state НЕ меняют:
/// * M07 = **Shower** — второй канал СОЖ, НЕ mist (≠ Fanuc M07-mist —
///   коллизия!); state.coolant L1 = {mist, flood} — третий канал не представим;
/// * M10/M11 = тормоз 4-й оси, M12/M13 = тормоз 5-й оси (гипотеза
///   «M13 = шпиндель+СОЖ» опровергнута research'ом — generic Fanuc-идиома);
/// * M29 = **output relay** с M-Fin (≠ Fanuc M29 rigid-tap — коллизия! rigid
///   tap Haas = Setting 57 + G84 внутренне);
/// * M97/M98/M99/M96/M46 — control-flow/subs → SYNTAX; M109 — ввод оператора
///   → RUNTIME_STATE.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MCode {
    ProgramEnd,     // M2 / M30 — различие вне L1 (SPEC §5.2)
    ProgramStop,    // M0 (optional=false)
    OptionalStop,   // M1 (optional=true; Q8)
    SpindleCw,      // M3
    SpindleCcw,     // M4
    SpindleOff,     // M5
    ToolChange,     // M6 (+T в блоке; идиома `T01 M06`)
    ToolChangeAlt,  // M16 — альтернативный код смены
    CoolantFlood,   // M8 (1:1; M07 Shower/M88 TSC — другие каналы, Raw)
    CoolantOff,     // M9
    CoolantShower,  // M7 → Raw{SEMANTICS} — Shower, НЕ mist (≠ Fanuc!)
    Brake4Engage,   // M10 → Raw{SEMANTICS} — тормоз 4-й оси
    Brake4Release,  // M11 → Raw{SEMANTICS}
    Brake5Engage,   // M12 → Raw{SEMANTICS} — тормоз 5-й оси
    Brake5Release,  // M13 → Raw{SEMANTICS} (≠ Fanuc-идиома шпиндель+СОЖ)
    SpindleOrient,  // M19 → Raw{SEMANTICS} (энкодер)
    OutputRelay,    // M29 → Raw{SEMANTICS} — output relay (≠ Fanuc rigid-tap!)
    SubCallLocal,   // M97 → Raw{SYNTAX} — вызов по N-номеру (Haas-специфика)
    SubCall,        // M98 → Raw{SYNTAX} (subs, P = O#)
    SubReturn,      // M99 → Raw{SYNTAX} (возврат / loop в main)
    JumpLine,       // M46 → Raw{SYNTAX} — jump to line (`?` §10(12))
    JumpIfNoInput,  // M96 → Raw{SYNTAX} — условный переход по входу
    InteractiveInput, // M109 → Raw{RUNTIME_STATE} — ожидание ввода оператора
}

/// Слова-параметры словаря. X/Y/Z — опы; I/J/K/R — дуги (I/J/K контекстно-
/// зависимы: центр дуги / пеки G83 (I первый, J уменьшение, K минимум) /
/// радиус-угол G12 — §4.1); F — подача (Setting 77 масштабирует bare-integer —
/// §1.2); S — обороты; C — фаска угла в G01 (Haas-специфика!); P — dwell
/// (мс/сек по точке — P-dot rule) / P в G10/G154/G103; Q — пек/shift/feed-%
/// (контекстно); L — повтор цикла/M97/M98 (НЕ K!); T/H/D — инструмент/таблицы;
/// E — accuracy G187; U/W — Setting 162 lathe-наследие `?`; A/B/C — ротационные
/// оси (этап 12). V — вне словаря Haas → generic-ведро.
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
            // Группа 09 (циклы) + G80.
            730 => GCode::CycleChipBreak,
            800 => GCode::CycleCancel,
            810 => GCode::CycleDrill,
            820 => GCode::CycleDrillDwell,
            830 => GCode::CyclePeck,
            850 => GCode::CycleBoreFeedOut,
            860 => GCode::CycleBoreStop,
            890 => GCode::CycleBoreDwell,
            // Группа 10 (retract; дефолт G99 — SPEC §7.5a).
            980 => GCode::RetractG98,
            990 => GCode::RetractG99,
            // Группа 12 (WCS): G54–G59 p=1..6; G110–G112 p=7..9; G154 Pn.
            540 => GCode::Wcs54,
            550 => GCode::Wcs55,
            560 => GCode::Wcs56,
            570 => GCode::Wcs57,
            580 => GCode::Wcs58,
            590 => GCode::Wcs59,
            1100 => GCode::Wcs110,
            1110 => GCode::Wcs111,
            1120 => GCode::Wcs112,
            1540 => GCode::Wcs154,
            // Группа 07/08 (cancel-коды семейств).
            400 => GCode::CompCancel,
            490 => GCode::LenCompCancel,
            // Группа 00 (one-shot).
            40 => GCode::Dwell,
            100 => GCode::DataRecord,
            520 => GCode::LocalCs,
            530 => GCode::MachineCs,
            920 => GCode::Preset,
            // Группа 15 (path exact-stop family).
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

    /// WCS-выбор группы 12 → Wcs (EID enum p=1..9): G54–G59 → 1:1;
    /// G110/G111/G112 → слоты #7–#9 = Wcs::G59_1/G59_2/G59_3 (нативная
    /// симметрия нумерации — эмиссия этапа 3 отдаст G110–G112 с notice).
    /// Wcs154 — контекстен по P-слову (канонизатор).
    pub fn wcs(&self) -> Option<Wcs> {
        Some(match self {
            GCode::Wcs54 => Wcs::G54,
            GCode::Wcs55 => Wcs::G55,
            GCode::Wcs56 => Wcs::G56,
            GCode::Wcs57 => Wcs::G57,
            GCode::Wcs58 => Wcs::G58,
            GCode::Wcs59 => Wcs::G59,
            GCode::Wcs110 => Wcs::G59_1,
            GCode::Wcs111 => Wcs::G59_2,
            GCode::Wcs112 => Wcs::G59_3,
            _ => return None,
        })
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
    /// Значение M-слова → код словаря; None = вне словаря (generic-ведро
    /// SYNTAX + диагностика — COVERAGE §3 «не молча»).
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
            160 => MCode::ToolChangeAlt,
            70 => MCode::CoolantShower,
            80 => MCode::CoolantFlood,
            90 => MCode::CoolantOff,
            100 => MCode::Brake4Engage,
            110 => MCode::Brake4Release,
            120 => MCode::Brake5Engage,
            130 => MCode::Brake5Release,
            190 => MCode::SpindleOrient,
            290 => MCode::OutputRelay,
            970 => MCode::SubCallLocal,
            980 => MCode::SubCall,
            990 => MCode::SubReturn,
            460 => MCode::JumpLine,
            960 => MCode::JumpIfNoInput,
            1090 => MCode::InteractiveInput,
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

/// Модальные группы (§1.1 контракта; этапы 1–2: 01–06, 07/08 cancel, 09
/// циклы, 10 retract, 12 wcs, 15 path; группа 00 — one-shot, без группы).
/// Один код на группу на блок; нарушение — диагностика
/// (block.modal_violation). Групп 13/17/18 Fanuc-типа НЕТ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalGroup {
    Motion01,    // G00/G01/G02/G03
    Plane02,     // G17/G18/G19
    Distance03,  // G90/G91
    ArcCenter04, // G90.1/G91.1 — В СЛОВАРЕ НЕТ `?`; ABSORB-как-LinuxCNC ruling
    FeedMode05,  // G93/G94/G95
    Units06,     // G20/G21 (G70/G71/G72 — болтовые, вне enum)
    Comp07,      // G40 cancel (G41/G42/G141 — вне enum, raw-таблица)
    LenComp08,   // G49 cancel (G43/G44/G143 — вне enum, raw-таблица)
    Cycle09,     // G73/G80/G81–G86/G89 (развёртываемые + cancel)
    Retract10,   // G98/G99
    Wcs12,       // G54–G59, G110–G112, G154 (G52 «00 или 12» `?` — Err путь)
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
        GCode::Wcs54
        | GCode::Wcs55
        | GCode::Wcs56
        | GCode::Wcs57
        | GCode::Wcs58
        | GCode::Wcs59
        | GCode::Wcs110
        | GCode::Wcs111
        | GCode::Wcs112
        | GCode::Wcs154 => ModalGroup::Wcs12,
        GCode::PathExactStop | GCode::PathBlend => ModalGroup::Path15,
        // Группа 00 (one-shot) — без модальной группы.
        GCode::Dwell | GCode::DataRecord | GCode::LocalCs | GCode::MachineCs
        | GCode::Preset => return None,
    })
}

// ---------------------------------------------------------------------------
// RAW-диспозиции G-кодов вне структурного словаря (COVERAGE §2 Haas-колонка;
// этап 1 — таблица движения-класса и болтовых паттернов; этап 2 — циклы-
// отказы G74/G76/G77/G84/G156/G174/G184, компенсации G41/G42/G141/G43/G44/
// G143/G234, расширенный WCS G113–G129)
// ---------------------------------------------------------------------------

/// RAW-код + причина для G-кода вне enum-словаря. `t = round(value*10)`.
/// Возврат None → generic-ведро Raw{E_UNSUPPORTED_SYNTAX} с текстом кода
/// (COVERAGE §2.7: «не молча»).
pub fn g_raw_disposition(value: f64) -> Option<(eid_core::diag::ErrorCode, &'static str)> {
    use eid_core::diag::ErrorCode::{
        RequiresMachineTable as RT, RuntimeState as RS, UnsupportedSemantics as US,
        UnsupportedSyntax as UX,
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
        1030 => (US, "G103 limit block buffering P0–15 — look-ahead/corner-rounding control (COVERAGE §2.7)"),
        1670 => (RS, "G167 modify setting mid-program — later ops depend on machine setting (COVERAGE §2.7)"),
        1870 => (US, "G187 smoothness level — accuracy/rounding tolerance control (COVERAGE §2.7)"),
        2660 => (US, "G266 visible-axes rapid % — rapid speed not representable in L1 (COVERAGE §2.7)"),
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
        // Циклы-отказы — COVERAGE §2.5 (Haas-словарь: tapping rigid внутренне —
        // Setting 57, M29 НЕ используется; G76 — направление сдвига I/J В
        // ТЕКСТЕ — 3-й смысл кода в матрице §7.16; G77 = Haas-замена G87)
        740 => (US, "G74 reverse tap — spindle sync/reverse; rigid internally (Setting 57), M29 not used (COVERAGE §2.5)"),
        760 => (US, "G76 fine boring w/ shift — spindle orientation (M19-class); shift direction from I/J words IN TEXT (3rd meaning of the code, COVERAGE §2.5 §7.16)"),
        840 => (US, "G84 tap — rigid internally (Setting 57 / NGC), no M29 needed (Fanuc-M29 collision; COVERAGE §2.5)"),
        770 => (US, "G77 back bore — orientation + shift + wall avoidance; Haas replacement of Fanuc G87 (COVERAGE §2.5)"),
        1560 => (US, "G156 broaching — tool mechanics, args `?` (COVERAGE §2.5)"),
        1740 => (US, "G174 CCW non-vertical rigid tap — 4/5-axis threading, class G84 (COVERAGE §2.5)"),
        1840 => (US, "G184 CW non-vertical rigid tap — 4/5-axis threading, class G84 (COVERAGE §2.5)"),
        // Расширенный WCS G113–G129 — слоты #10–#26 вне EID p-домена (p ≤ 9);
        // G110–G112 (слоты #7–#9) — в enum (p=7..9, нативная симметрия).
        1130..=1290 => (US, "extended WCS slot outside EID p-domain (Haas slots #10–#26 = G113–G129; EID wcs p ≤ 9; COVERAGE §2.3)"),
        // Трансформации/генераторы — COVERAGE §2.6 (кандидаты этапа 12)
        680 => (US, "G68 coordinate system rotation — stage-12 decomposition candidate (COVERAGE §2.6)"),
        690 => (US, "G69 rotation cancel — family G68 (COVERAGE §2.6)"),
        500 => (US, "G50 scaling cancel — group 11, option rotation & scaling (COVERAGE §2.6)"),
        510 => (US, "G51 scaling — group 11, stage-12 candidate (COVERAGE §2.6)"),
        1000 => (US, "G100 mirror image cancel — Settings 45–48/80/250 outside text; flips climb/conventional (COVERAGE §2.6)"),
        1010 => (US, "G101 mirror image enable — Settings outside text; flips climb/conventional (COVERAGE §2.6)"),
        1070 => (US, "G107 cylindrical mapping — nonlinear axis mapping (COVERAGE §2.6)"),
        470 => (US, "G47 text engraving — glyph tables of the control (COVERAGE §2.6)"),
        1500 => (UX, "G150 general purpose pocket milling — geometry is a subprogram (P = O#), subs mechanics (COVERAGE §2.6)"),
        // 5-осевое — COVERAGE §2.4/§2.7
        2340 => (US, "G234 tool center point control (TCPC/RTCP) — 5-axis, stage 12 (COVERAGE §2.4)"),
        2530 => (US, "G253 orient spindle normal to feature CS — 5-axis M19-class (COVERAGE §2.7)"),
        2540 => (US, "G254 dynamic work offset enable — 5-axis (COVERAGE §2.7)"),
        2550 => (US, "G255 dynamic work offset cancel — 5-axis (COVERAGE §2.7)"),
        2680 => (US, "G268 feature CS enable — 5-axis (COVERAGE §2.7)"),
        2690 => (US, "G269 feature CS cancel — 5-axis (COVERAGE §2.7)"),
        // Компенсации (активные коды семейств; HAAS-СОСТАВЫ §1.3: G40 =
        // {G41, G42, G141}, G49 = {G43, G44, G143} — Fanuc-расширений нет)
        410 => (US, "G41 cutter comp left +D — comp geometry not a goal; D = table number (Setting 40 diameter/radius; COVERAGE §2.4)"),
        420 => (US, "G42 cutter comp right +D — comp geometry; D = table number (COVERAGE §2.4)"),
        1410 => (US, "G141 3D+ cutter compensation — 5-axis class, stage 12; G40-family member (COVERAGE §2.4)"),
        430 => (RT, "G43 tool length offset +H — offset from machine table; G49-family member (COVERAGE §2.4)"),
        440 => (RT, "G44 tool length offset −H — offset from machine table; G49-family member (COVERAGE §2.4)"),
        1430 => (RT, "G143 5-axis tool length compensation + — from machine table; G49-family member (COVERAGE §2.4)"),
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
/// G70/G71/G72; **циклы-отказы G74/G76/G77/G84/G156/G174/G184** (этап 2 —
/// кадр цикла = ход к отверстию); rapid-% G266.
pub const RAW_MOTION_G: &[i64] = &[
    280, 290,   // G28/G29 — home
    310,        // G31 — probing skip
    350, 360, 370, 1360, // G35/G36/G37/G136 — автоизмерение
    120, 130,   // G12/G13 — circular pocket
    700, 710, 720, // G70/G71/G72 — болтовые паттерны
    740, 760, 770, 840, 1560, 1740, 1840, // циклы-отказы (этап 2)
    2660,       // G266 — rapid %
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

/// Циклы Haas, разворачиваемые по канон-шаблону §7.7 SPEC (Haas-семантика
/// §5.4 отчёта: L-повторы, P-dot dwell, G73 δ = Setting 22 → канон 0.01in
/// unit-scaled, G83 Q ИЛИ I/J/K). G74/G76/G77/G84/G156/G174/G184 —
/// raw-таблица (SEMANTICS), не здесь.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleKind {
    Drill,       // G81
    DrillDwell,  // G82 (P-dot dwell)
    Peck,        // G83 (полный отвод к R; Q ИЛИ I/J/K)
    ChipBreak,   // G73 (одиночный rapid d+δ; δ = Setting 22 → канон)
    BoreFeedOut, // G85 (отвод на подаче)
    BoreStop,    // G86 (шпиндель off + рестарт; P обязателен v0.18)
    BoreDwell,   // G89 (bore + dwell, feed out)
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

    /// Циклы с dwell-P: G82/G89 — обязателен; G86 — обязателен (SPEC §7.4a
    /// v0.18: rs274 отказывает без P — «Dwell time p word missing with g86»).
    pub fn takes_p(&self) -> bool {
        matches!(
            self,
            CycleKind::DrillDwell | CycleKind::BoreStop | CycleKind::BoreDwell
        )
    }

    /// Пекерующие (Q или I/J/K для G83; G73 — только Q `?`).
    pub fn takes_peck(&self) -> bool {
        matches!(self, CycleKind::Peck | CycleKind::ChipBreak)
    }
}

/// Модальное состояние канонизации. Дефолты Haas NGC (§4.4 контракта:
/// power-on G00/G17/G40/G49/G54/G80/G90/G94 + units по Setting `?` —
/// mm-дефолт консервативен; на эквивалентность не влияет — assumption
/// самодостаточности, EQUIVALENCE §1.2): G17, G90, **инкрементальный центр**
/// (группы 04 нет — I/J/K от start всегда; G90.1 `?` — ABSORB ruling),
/// G94, mm (`?` Setting-зависимый), G54, **G99** (дефолт retract — SPEC §7.5a),
/// exact_path при отсутствии G61/G64 (SPEC §3 v0.12).
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
    pub wcs: Wcs,                // группа 12 (G54 дефолт контроллера)
    pub retract_g98: bool,       // группа 10 (дефолт G99 — SPEC §7.5a)
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
            GCode::UnitsIn => self.units = Units::In,
            GCode::UnitsMm => self.units = Units::Mm,
            GCode::RetractG98 => self.retract_g98 = true,
            GCode::RetractG99 => self.retract_g98 = false,
            // Группа 12: G54–G59 p=1..6; G110–G112 → p=7..9 (нативная
            // симметрия слотов #7–#9); G154 — канонизатор (контекстен по P).
            GCode::Wcs54
            | GCode::Wcs55
            | GCode::Wcs56
            | GCode::Wcs57
            | GCode::Wcs58
            | GCode::Wcs59
            | GCode::Wcs110
            | GCode::Wcs111
            | GCode::Wcs112 => {
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
        // G28/G31/G43/G65/G84/G92.1/G113 → вне enum (raw-таблица/generic).
        for v in [28.0, 31.0, 43.0, 65.0, 84.0, 92.1, 113.0, 129.0, 141.0, 143.0] {
            assert_eq!(g(v), None, "G{v} — вне enum");
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
        // Циклы-отказы — SEMANTICS (этап 2).
        for t in [74.0, 76.0, 77.0, 84.0, 156.0, 174.0, 184.0] {
            let (c, _) = g_raw_disposition(t).unwrap();
            assert_eq!(c, ErrorCode::UnsupportedSemantics, "G{t}");
        }
        // Компенсации: G41/G42/G141 SEMANTICS; G43/G44/G143 MACHINE_TABLE.
        assert_eq!(g_raw_disposition(41.0).unwrap().0, ErrorCode::UnsupportedSemantics);
        assert_eq!(g_raw_disposition(42.0).unwrap().0, ErrorCode::UnsupportedSemantics);
        assert_eq!(g_raw_disposition(141.0).unwrap().0, ErrorCode::UnsupportedSemantics);
        assert_eq!(g_raw_disposition(43.0).unwrap().0, ErrorCode::RequiresMachineTable);
        assert_eq!(g_raw_disposition(44.0).unwrap().0, ErrorCode::RequiresMachineTable);
        assert_eq!(g_raw_disposition(143.0).unwrap().0, ErrorCode::RequiresMachineTable);
        assert_eq!(g_raw_disposition(234.0).unwrap().0, ErrorCode::UnsupportedSemantics);
        // G113–G129 — слоты вне EID p-домена.
        let (c, r) = g_raw_disposition(113.0).unwrap();
        assert_eq!(c, ErrorCode::UnsupportedSemantics);
        assert!(r.contains("p-domain"));
        let (c, _) = g_raw_disposition(129.0).unwrap();
        assert_eq!(c, ErrorCode::UnsupportedSemantics);
        // Неизвестный код → None (generic-ведро SYNTAX).
        assert_eq!(g_raw_disposition(88.8), None);
        assert_eq!(g_raw_disposition(107.0).map(|(c, _)| c), Some(ErrorCode::UnsupportedSemantics), "G107 — таблица");
        // RAW_MOTION_G покрывает движение-класс (включая циклы-отказы этапа 2).
        for t in [280, 290, 310, 350, 360, 370, 1360, 120, 130, 700, 710, 720, 740, 760, 770, 840, 1560, 1740, 1840, 2660] {
            assert!(RAW_MOTION_G.contains(&t), "t={t}");
        }
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
        assert_eq!(MCode::from_value(16.0), Some(MCode::ToolChangeAlt));
        assert_eq!(MCode::from_value(8.0), Some(MCode::CoolantFlood));
        assert_eq!(MCode::from_value(9.0), Some(MCode::CoolantOff));
        // Вендорское ведро этапа 2 (коллизии vs Fanuc в диспозициях).
        assert_eq!(MCode::from_value(7.0), Some(MCode::CoolantShower));
        assert_eq!(MCode::from_value(10.0), Some(MCode::Brake4Engage));
        assert_eq!(MCode::from_value(11.0), Some(MCode::Brake4Release));
        assert_eq!(MCode::from_value(12.0), Some(MCode::Brake5Engage));
        assert_eq!(MCode::from_value(13.0), Some(MCode::Brake5Release));
        assert_eq!(MCode::from_value(19.0), Some(MCode::SpindleOrient));
        assert_eq!(MCode::from_value(29.0), Some(MCode::OutputRelay));
        assert_eq!(MCode::from_value(97.0), Some(MCode::SubCallLocal));
        assert_eq!(MCode::from_value(98.0), Some(MCode::SubCall));
        assert_eq!(MCode::from_value(99.0), Some(MCode::SubReturn));
        assert_eq!(MCode::from_value(46.0), Some(MCode::JumpLine));
        assert_eq!(MCode::from_value(96.0), Some(MCode::JumpIfNoInput));
        assert_eq!(MCode::from_value(109.0), Some(MCode::InteractiveInput));
        // M64/M730 — вне словаря Haas → generic-ведро None.
        for v in [64.0, 730.0, 199.0] {
            assert_eq!(MCode::from_value(v), None, "M{v}");
        }
    }

    #[test]
    fn cycles_retract_wcs_groups() {
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
        // Группа 12 (wcs): p=1..6 + расширенные p=7..9.
        assert_eq!(g(54.0).and_then(|c| c.wcs()), Some(Wcs::G54));
        assert_eq!(g(59.0).and_then(|c| c.wcs()), Some(Wcs::G59));
        // НАТИВНАЯ СИММЕТРИЯ: G110–G112 → слоты #7–#9 = Wcs::G59_1..G59_3.
        assert_eq!(g(110.0).and_then(|c| c.wcs()), Some(Wcs::G59_1), "G110 → p7");
        assert_eq!(g(111.0).and_then(|c| c.wcs()), Some(Wcs::G59_2), "G111 → p8");
        assert_eq!(g(112.0).and_then(|c| c.wcs()), Some(Wcs::G59_3), "G112 → p9");
        assert_eq!(modal_group(GCode::Wcs110), Some(ModalGroup::Wcs12));
        assert_eq!(modal_group(GCode::Wcs154), Some(ModalGroup::Wcs12));
        assert_eq!(g(154.0), Some(GCode::Wcs154));
        // Группа 00 one-shot + cancel-коды.
        assert_eq!(g(4.0), Some(GCode::Dwell));
        assert_eq!(g(10.0), Some(GCode::DataRecord));
        assert_eq!(g(52.0), Some(GCode::LocalCs));
        assert_eq!(g(53.0), Some(GCode::MachineCs));
        assert_eq!(g(92.0), Some(GCode::Preset));
        assert_eq!(g(40.0), Some(GCode::CompCancel));
        assert_eq!(g(49.0), Some(GCode::LenCompCancel));
        assert_eq!(modal_group(GCode::CompCancel), Some(ModalGroup::Comp07));
        assert_eq!(modal_group(GCode::LenCompCancel), Some(ModalGroup::LenComp08));
        // One-shot коды — без модальной группы.
        for v in [4.0, 10.0, 52.0, 53.0, 92.0] {
            assert_eq!(modal_group(g(v).unwrap()), None, "G{v} — one-shot");
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
        // Дефолты NGC (§4.4): G17/G90/инкрементальный центр/G94/mm/G54/G99.
        assert_eq!(m.plane, GCode::PlaneXY);
        assert!(m.dist_abs);
        assert!(!m.arc_center_abs, "дефолт — инкрементальный центр (группы 04 нет)");
        assert!(m.feed_per_min);
        assert!(!m.feed_inverse);
        assert_eq!(m.units, Units::Mm);
        assert!(!m.units_established);
        assert_eq!(m.wcs, Wcs::G54);
        assert!(!m.retract_g98, "дефолт G99 (SPEC §7.5a)");
        assert_eq!(m.path, None);
        assert_eq!(m.motion, None);
        m.apply(GCode::ArcCcw);
        m.apply(GCode::PlaneYZ);
        m.apply(GCode::DistInc);
        m.apply(GCode::ArcCenterAbs);
        m.apply(GCode::UnitsIn);
        m.apply(GCode::Wcs57);
        m.apply(GCode::RetractG98);
        assert_eq!(m.motion, Some(GCode::ArcCcw));
        assert_eq!(m.plane, GCode::PlaneYZ);
        assert!(!m.dist_abs);
        assert!(m.arc_center_abs);
        assert_eq!(m.units, Units::In);
        assert_eq!(m.wcs, Wcs::G57);
        assert!(m.retract_g98);
        // G93/G94 (ABSORB, §2.2).
        m.apply(GCode::FeedInverse);
        assert!(m.feed_inverse);
        assert!(!m.feed_per_min);
        m.apply(GCode::FeedPerMin);
        assert!(!m.feed_inverse);
        assert!(m.feed_per_min);
        m.apply(GCode::UnitsMm);
        assert_eq!(m.units, Units::Mm);
        // Расширенные слоты p=7..9 + возврат на G54.
        m.apply(GCode::Wcs110);
        assert_eq!(m.wcs, Wcs::G59_1);
        m.apply(GCode::Wcs112);
        assert_eq!(m.wcs, Wcs::G59_3);
        m.apply(GCode::Wcs54);
        assert_eq!(m.wcs, Wcs::G54);
    }
}
