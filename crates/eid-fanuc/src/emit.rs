//! Эмиттер EID/L1 → текст Fanuc mill (Агент C, этап 3; COVERAGE §5.1 v0.8,
//! fanuc_dialect_analysis.md §8 Reverse gap, EQUIVALENCE §1.4/§2/§4).
//!
//! Структура вывода (детерминировано: один AST → одни байты; двойной emit —
//! байт-в-байт):
//!  1. заголовок `G21|G20 G90 G94 G17 G91.1` — всегда явно: units (§2.5),
//!     G90/G94 — дефолты, G17 — плоскость (дуги её переустанавливают),
//!     G91.1 — режим центров дуг ИНКРЕМЕНТАЛЬНЫЙ pinned (эмиссия всегда
//!     IJK-оффсеты от старта, EQUIVALENCE §4; опция G90.1/.91.1 на части
//!     серий — `?` реестр 15 анализа — задокументировано).
//!  2. path_mode — всегда явно, одна строка (SPEC §3: одно значение):
//!     exact_stop → `G61` (Fanuc G61 = exact stop — value shift, COVERAGE
//!     §2.2); exact_path → **warning + `G61`** (консервативный дефолт —
//!     «Fanuc exact_path эквивалента нет»; профиль bare G64 — этап 8);
//!     blend(t) → bare `G64` + warning при t (P-квантификатора в словаре
//!     Fanuc нет — t теряется).
//!  3. frame_preset (EQUIVALENCE §1.4): выборка preset.wcs, затем
//!     `G92 X_ Y_ Z_` — до любого motion-опа; запекание в G10 запрещено.
//!     G92-hygiene (`G92.1` первым блоком) — флаг профиля, НЕ дефолт
//!     (меняет глобальное состояние станка — операторский контракт
//!     EQUIVALENCE §1.2) — этап 8.
//!  4. опы по порядку: перед каждым — дельты его снапшота (фиксированный
//!     порядок WCS → шпиндель → СОЖ, затем строки хода).
//!  5. движение G0/G1/G2/G3: все три оси конца на каждом ходе; числа —
//!     4 dp С ДЕСЯТИЧНОЙ ТОЧКОЙ ВСЕГДА (DPI-ловушка §1.2 анализа: `X10.0000`
//!     не `X10` — bare integer при DPI=0 читается ×0.001); IS-B mm 3dp-
//!     значимы — трейлинг-нули Fanuc терпит (notice-политика НЕ нужна);
//!     frame=machine → префикс `G53 G0|G1` (одномодальный, явный G0/G1 в
//!     кадре — требование Fanuc manual).
//!  6. дуги: плоскость из канонической нормали (+Z→G17, +Y→G18, +X→G19);
//!     смена плоскости — отдельной строкой; центр — ТОЛЬКО in-plane пара
//!     I/J/K (G17→I+J, G18→I+K, G19→J+K; I/J/K literal X/Y/Z), инкремент
//!     от старта (G91.1 в заголовке); R НЕ эмитится при дефолтных опциях
//!     (EQUIVALENCE §4); этап 8: profile-флаг allow_r_arcs (= emit-r-arcs)
//!     включает R-форму + notice на каждой R-дуге (COVERAGE §5.1), полные
//!     круги — всегда IJK (R при start==end неоднозначен, SPEC §6);
//!     **полный круг (sweep=360°, pitch=0)
//!     БЕЗ end-слов** (`G2 I_ J_` — легальный Fanuc-формат; слова end==start
//!     при перепарсе квантуются в zero-length drop); винтовой полный оборот
//!     (pitch≠0) — с end-словом вне-плоскостной оси; частичная дуга — все
//!     три оси.
//!  7. dwell: **P = целые мс** (decimal point запрещён: `G4 P500` = 0.5 s);
//!     правило COVERAGE §5.1: seconds·1000 целое (в допуске) → P,
//!     иначе sub-ms остаток → **X-форма** (`G4 X0.0005` — Fanuc X принимает
//!     float-секунды).
//!  8. tool_change → `T## M06` в одном блоке (Fanuc-идиома); STOP → M0/M1;
//!     PEND → `M30` (дефолт, Fanuc-конвенция rewind; этап 8: профиль
//!     program_end_style = "M02" → `M02`, COVERAGE §5.1);
//!     WCS_DEFINE → `G10 L2 P<n> X_ Y_ Z_`; **WCS_SET_HERE → отказ
//!     E_UNSUPPORTED_SEMANTICS** (Fanuc L20 = запись G54.1 Pn — другое;
//!     честный отказ, COVERAGE §5.1 ловушка №3). Этап 8: расширенные слоты
//!     G59.x при profile wcs_extended_map → `G54.1 Pn` + notice «map применён,
//!     таблица подменена» (НЕ дефолт, аналогия golden_preset_no_bake);
//!     без мапы — прежний отказ E_UNSUPPORTED_SYNTAX; неполная мапа → отказ
//!     с перечнем незмапленных слотов.
//!
//! Хвост M5/M9 перед program_end НЕ эмитится (ruling COVERAGE §5: AST не
//! хранит пост-конечное состояние; Fanuc M30 сам глушит шпиндель/СОЖ).
//! Развёрнутые циклы §7.7 SPEC эмитятся как ходы self-собой (verbose —
//! документированное следствие, SPEC §1.2; обратное свёртывание запрещено).
//!
//! RAW: payload-hex декодируется и выводится дословно (same-dialect
//! fidelity, grammar D9 — включая терминатор строки). Кросс-диалектный
//! запрет `E_RAW_UNTRANSLATABLE` — НЕ здесь: это правило translate-конвейера
//! (SPEC §8.2), same-dialect эмиттер обязан воспроизводить байты.
//!
//! Warnings (не отказы) — в буфере адаптера (`FanucAdapter::take_warnings`):
//! потери path_mode exact_path/blend(t), не-эмитируемая normal-компонента
//! центра дуги, несвязные снапшоты state.tool/state.feed.
//!
//! Валидация: `eid_core::parser::validate` (канон-форма §3) → magic-проверка
//! → легальность эмиссии против [`FanucEmissionLegality`] (ConversionTarget-
//! паттерн MLIR: коды отказа берутся ТОЛЬКО из таблицы — единый путь
//! отказа, COVERAGE §5.1).

use eid_core::ast::{Coolant, Dir, Frame, Num, Op, Program, Spindle, StartPoint, Units, Wcs};
use eid_core::diag::{Diagnostic, ErrorCode, Stage};
use eid_core::emitter::fmt_num;
use eid_core::parser;

use crate::state::{plane_axes, GCode};

/// Результат эмиссии: текст + warnings (не отказы; забрать через
/// [`crate::FanucAdapter::take_warnings`]).
pub struct EmitOut {
    pub text: String,
    pub warnings: Vec<String>,
}

// ===========================================================================
// Профильные эмиссионные опции (этап 8; COVERAGE §5.1 «дефолт / выбор
// профиля»). Trait Adapter v0.2 ЗАМОРОЖЕН — emit(&self, program) профиль не
// принимает; опции входят через обёртку registry
// (eid-integration FanucNamedAdapter хранит Option<ProfileEmissionOpts>,
// конструктор with_emission_opts) и вызывают [`emit_with_profile`].
// Default = поведение до этапа 8 (байт-в-байт: золотовые тексты целы).
// ===========================================================================

/// Стиль program_end (COVERAGE §5.1: «M30 дефолт, Fanuc-конвенция» vs M02 —
/// выбор профиля). Дефолт — [`FanucProgramEnd::M30`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FanucProgramEnd {
    /// `M30` — rewind + reset-семантика (глушит шпиндель/СОЖ).
    #[default]
    M30,
    /// `M02` — исторический конец программы (легален в словаре Fanuc).
    M02,
}

impl FanucProgramEnd {
    /// Слово эмиссии.
    pub fn word(self) -> &'static str {
        match self {
            FanucProgramEnd::M30 => "M30",
            FanucProgramEnd::M02 => "M02",
        }
    }
}

/// Таблица маппинга расширенных WCS (этап 8, COVERAGE §5.1 profile-опция
/// «map G59.x → G54.1 Pn» с notice — НЕ дефолт, аналогия golden_preset_no_bake:
/// G54.1 — другая таблица с чужим происхождением origin). None (дефолт) =
/// текущее поведение: расширенные слоты (G59.1/.2/.3) → отказ
/// E_UNSUPPORTED_SYNTAX.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FanucG59Map {
    /// `"G59.1" = "G54.1 P<n>"` → Some(n); None = слот не замаплен.
    pub g59_1: Option<u32>,
    /// `"G59.2" = "G54.1 P<n>"` → Some(n); None = слот не замаплен.
    pub g59_2: Option<u32>,
    /// `"G59.3" = "G54.1 P<n>"` → Some(n); None = слот не замаплен.
    pub g59_3: Option<u32>,
}

impl FanucG59Map {
    /// Мапа пуста (ни один слот не замаплен)?
    pub fn is_empty(&self) -> bool {
        self.g59_1.is_none() && self.g59_2.is_none() && self.g59_3.is_none()
    }

    /// Pn слота; None = слот не замаплен (неполная мапа).
    fn slot(&self, w: Wcs) -> Option<u32> {
        match w {
            Wcs::G59_1 => self.g59_1,
            Wcs::G59_2 => self.g59_2,
            Wcs::G59_3 => self.g59_3,
            _ => None,
        }
    }

    /// Имя слота в синтаксисе источника (для диагностики).
    fn slot_name(w: Wcs) -> Option<&'static str> {
        match w {
            Wcs::G59_1 => Some("G59.1"),
            Wcs::G59_2 => Some("G59.2"),
            Wcs::G59_3 => Some("G59.3"),
            _ => None,
        }
    }
}

/// Профильные эмиссионные опции Fanuc (этап 8). [`FanucEmissionOpts::default()`]
/// эквивалентен поведению до этапа 8; библиотечный [`emit`] и
/// `FanucAdapter::emit` используют именно его.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FanucEmissionOpts {
    /// COVERAGE §5.1 program_end: M30 (дефолт) | M02 — выбор профиля.
    pub program_end: FanucProgramEnd,
    /// true → дуги эмитятся R-формой + notice на каждой R-дуге (COVERAGE §5.1
    /// «R — profile-флаг + notice», EQUIVALENCE §4); false (дефолт) = всегда
    /// IJK. Полные круги (sweep=360°) — ВСЕГДА IJK даже при true: R при
    /// совпадающих in-plane start/end неоднозначен (SPEC §6; R-полный круг —
    /// alarm контроллера).
    pub allow_r_arcs: bool,
    /// None = мапа выключена (текущее поведение); Some = таблица применена
    /// (эмиссия G54.1 Pn + notice «map применён, таблица подменена»).
    pub g59_map: Option<FanucG59Map>,
}

impl FanucEmissionOpts {
    /// Мапа применена (Some и непуста)?
    fn map_on(&self) -> bool {
        self.g59_map.as_ref().is_some_and(|m| !m.is_empty())
    }
}

// ===========================================================================
// FanucEmissionLegality — ConversionTarget-паттерн (MLIR), зеркально
// eid-grbl/eid-linuxcnc (COVERAGE §5.1): легальность эмиссии в Fanuc —
// ДАННЫЕ, не размазанные проверки; валидатор берёт ErrorCode ТОЛЬКО из
// таблицы (единый путь отказа). E_RAW_UNTRANSLATABLE в таблицу НЕ входит —
// это translate-путь (SPEC §8.2), не same-dialect эмиссия.
//
// Состав (10 видов опов; 9 легальны — wcs_set_here запрещён ловушкой №3):
//   rapid/linear/arc — G0/G1/G2-G3 (IJK всегда); dwell — G4 P-мс/X-сек;
//   tool_change — T## M06; program_stop — M0/M1; program_end — M30;
//   wcs_define — G10 L2 P1..6; raw — same-dialect verbatim;
//   frame_preset — G92 первым блоком (EQUIVALENCE §1.4).
//   wcs_set_here — НЕ представимо: Fanuc L20 = запись origin G54.1 Pn,
//   НЕ «set here» (COVERAGE §5.1); «set here» через G92 требует машинной
//   позиции — не статично → честный отказ SEMANTICS.
// ===========================================================================

/// Условие отказа легальности эмиссии в Fanuc mill (типизированное, v0.2
/// волна 4 — INTERFACE-V0.2 §2.5 В1: enum вместо &'static str; код —
/// ТОТАЛЬНАЯ функция `code()`, паник-точек «таблица и эмиттер разошлись»
/// на emit-пути нет; согласие таблицы — тест `legality_table_complete`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanucEmissionCondition {
    /// `wcs_set_here` — эмиссия запрещена как L20 (ловушка №3).
    WcsSetHere,
    /// `wcs_outside_g54_g59` — G59.1/.2/.3 вне словаря Fanuc (правило §5).
    WcsOutsideG54G59,
    /// `wcs_define_p_outside_g54_g59` — P вне домена 1..6.
    WcsDefinePOutsideG54G59,
    /// `machine_frame_arc` — дуга в G53 не эмитится (ходы прямые).
    MachineFrameArc,
    /// `motion_feed_none` — канон-форма §3/§4.
    MotionFeedNone,
    /// `arc_unknown_start` — IJK требует числового старта.
    ArcUnknownStart,
    /// `arc_normal_non_canonical` — нормаль не единичная +ось (§6).
    ArcNormalNonCanonical,
    /// `raw_payload_not_hex` — payload не чётный hex (grammar D9 v0.3).
    RawPayloadNotHex,
    /// `program_end_missing` / `_duplicate` / `_not_last` — структура §3
    /// (с Fanuc-спецификой хвоста: Raw-subs после M30 легальны, ruling 1).
    ProgramEndMissing,
    ProgramEndDuplicate,
    ProgramEndNotLast,
}

impl FanucEmissionCondition {
    /// Каноническое имя условия (сохранено 1:1 со строковой таблицы —
    /// его читают тесты и COVERAGE §5.1; manifest r4_expect хранит КОДЫ).
    pub fn as_str(self) -> &'static str {
        match self {
            FanucEmissionCondition::WcsSetHere => "wcs_set_here",
            FanucEmissionCondition::WcsOutsideG54G59 => "wcs_outside_g54_g59",
            FanucEmissionCondition::WcsDefinePOutsideG54G59 => "wcs_define_p_outside_g54_g59",
            FanucEmissionCondition::MachineFrameArc => "machine_frame_arc",
            FanucEmissionCondition::MotionFeedNone => "motion_feed_none",
            FanucEmissionCondition::ArcUnknownStart => "arc_unknown_start",
            FanucEmissionCondition::ArcNormalNonCanonical => "arc_normal_non_canonical",
            FanucEmissionCondition::RawPayloadNotHex => "raw_payload_not_hex",
            FanucEmissionCondition::ProgramEndMissing => "program_end_missing",
            FanucEmissionCondition::ProgramEndDuplicate => "program_end_duplicate",
            FanucEmissionCondition::ProgramEndNotLast => "program_end_not_last",
        }
    }

    /// Код класса отказа — ТОТАЛЬНАЯ функция (исчерпывающий match; классы —
    /// правило COVERAGE §5).
    pub fn code(self) -> ErrorCode {
        use eid_core::diag::ErrorCode::{
            InvalidInput as INVAL, UnsupportedSemantics as USEM, UnsupportedSyntax as SYN,
        };
        match self {
            // COVERAGE §5.1: wcs_set_here эмиссия запрещена как L20 —
            // честный отказ.
            FanucEmissionCondition::WcsSetHere => USEM,
            // G59.1/.2/.3 — кодов нет (правило §5: вне словаря → SYNTAX).
            FanucEmissionCondition::WcsOutsideG54G59 => SYN,
            FanucEmissionCondition::WcsDefinePOutsideG54G59 => SYN,
            // G53 ходы прямые (SPEC §5.3) — дуга не представима.
            FanucEmissionCondition::MachineFrameArc => USEM,
            FanucEmissionCondition::MotionFeedNone => INVAL, // канон-форма §3/§4
            FanucEmissionCondition::ArcUnknownStart => INVAL, // IJK требует числового старта
            FanucEmissionCondition::ArcNormalNonCanonical => INVAL, // нормаль не +ось (§6)
            FanucEmissionCondition::RawPayloadNotHex => INVAL, // grammar D9 v0.3
            FanucEmissionCondition::ProgramEndMissing => INVAL, // структура (SPEC §3)
            FanucEmissionCondition::ProgramEndDuplicate => INVAL,
            FanucEmissionCondition::ProgramEndNotLast => INVAL,
        }
    }
}

/// Статическая таблица легальности эмиссии в Fanuc mill.
#[derive(Debug, Clone, PartialEq)]
pub struct FanucEmissionLegality {
    /// Виды опов, представимые ходами Fanuc (имена = op_kind()).
    pub legal: Vec<&'static str>,
    /// Условия отказа: (условие, код класса) — зеркало `cond.code()`.
    /// Классы — правило COVERAGE §5: конструкция вне словаря цели →
    /// E_UNSUPPORTED_SYNTAX; не представима ходами → E_UNSUPPORTED_SEMANTICS;
    /// нарушение канон-формы AST → E_INVALID_INPUT.
    pub illegal: Vec<(FanucEmissionCondition, ErrorCode)>,
    /// Легально-но-потерьные условия: (условие, текст warning). Эмиттер пишет
    /// текст в warnings-буфер адаптера, отказом НЕ является.
    pub warnings: Vec<(&'static str, &'static str)>,
}

impl FanucEmissionLegality {
    /// Текст warning по имени условия; None = не объявлено.
    pub fn warning_of(&self, condition: &str) -> Option<&'static str> {
        self.warnings
            .iter()
            .find(|(c, _)| *c == condition)
            .map(|(_, text)| *text)
    }

    /// Все коды отказа таблицы (без дубликатов).
    pub fn codes(&self) -> Vec<ErrorCode> {
        let mut v: Vec<ErrorCode> = self.illegal.iter().map(|(_, c)| *c).collect();
        v.sort_by_key(|c| c.as_str());
        v.dedup();
        v
    }

    /// Вид опа легален?
    pub fn is_legal_kind(&self, kind: &str) -> bool {
        self.legal.contains(&kind)
    }
}

/// Имя вида опа — ключ таблицы легальности (словарь manifest r4_expect).
pub fn op_kind(op: &Op) -> &'static str {
    match op {
        Op::Rapid { .. } => "rapid",
        Op::Linear { .. } => "linear",
        Op::Arc { .. } => "arc",
        Op::Dwell { .. } => "dwell",
        Op::ToolChange { .. } => "tool_change",
        Op::ProgramStop { .. } => "program_stop",
        Op::ProgramEnd { .. } => "program_end",
        Op::WcsDefine { .. } => "wcs_define",
        Op::WcsSetHere { .. } => "wcs_set_here",
        Op::Raw { .. } => "raw",
    }
}

/// G-слово WCS-выбора; None = вне словаря Fanuc (G59.1/.2/.3 — кодов нет,
/// COVERAGE §5.1: G54.1 P1..P3 — другая таблица с чужим происхождением;
/// подмена без предупреждения запрещена — аналогия golden_preset_no_bake).
fn wcs_word(w: Wcs) -> Option<&'static str> {
    Some(match w {
        Wcs::G54 => "G54",
        Wcs::G55 => "G55",
        Wcs::G56 => "G56",
        Wcs::G57 => "G57",
        Wcs::G58 => "G58",
        Wcs::G59 => "G59",
        _ => return None,
    })
}

/// Плоскость по канонической нормали (SPEC §6: единичная +ось;
/// +Z→G17, +Y→G18, +X→G19). Возвращает (плоскость, axis_0, axis_1, normal-
/// ось). Не-осевой/отрицательный normal → Err(текст) — канон-форма такого не
/// содержит; валидатор оборачивает в код таблицы, писатель — INVALID.
fn plane_of(normal: &[Num; 3]) -> Result<(GCode, usize, usize, usize), String> {
    let c = [normal[0].value(), normal[1].value(), normal[2].value()];
    let (mut best, mut bi) = (0.0f64, 0usize);
    for (i, &x) in c.iter().enumerate() {
        if x.abs() > best {
            best = x.abs();
            bi = i;
        }
    }
    let axial = (best - 1.0).abs() <= 0.001
        && c[(bi + 1) % 3].abs() <= 0.001
        && c[(bi + 2) % 3].abs() <= 0.001;
    if !axial || c[bi] < 0.0 {
        return Err(format!(
            "arc: normal = ({:.4},{:.4},{:.4}) — не единичная +ось (SPEC §6: G17→+Z, G18→+Y, G19→+X)",
            c[0], c[1], c[2]
        ));
    }
    Ok(match bi {
        2 => {
            let (a0, a1, nl) = plane_axes(GCode::PlaneXY);
            (GCode::PlaneXY, a0, a1, nl)
        }
        1 => {
            let (a0, a1, nl) = plane_axes(GCode::PlaneZX);
            (GCode::PlaneZX, a0, a1, nl)
        }
        _ => {
            let (a0, a1, nl) = plane_axes(GCode::PlaneYZ);
            (GCode::PlaneYZ, a0, a1, nl)
        }
    })
}

fn to_f64(p: &eid_core::ast::Point3) -> [f64; 3] {
    [p.x.value(), p.y.value(), p.z.value()]
}

/// Слова ротационных слотов конца хода (v0.4 §1.2, SPEC v0.22): каждый Some —
/// слово A/B/C (канонический порядок X Y Z A B C), число — DPB 4dp
/// (decimal-point-always — домен адресов Fanuc IS-B, зеркало X/Y/Z). Каждый
/// Some пишется ВСЕГДА (рестейт легален, модальность НЕ оптимизируется);
/// стартовые слоты Known-старта воспроизводятся linking'ом (старт = конец
/// предыдущего опа — его слова уже в потоке; первый ход после якоря — None,
/// слова нет). Все None — пустая строка (3-осевой вывод байт-идентичен).
fn rot_words(end: &eid_core::ast::Point3) -> String {
    let mut s = String::new();
    for (letter, slot) in [("A", &end.a), ("B", &end.b), ("C", &end.c)] {
        if let Some(n) = slot {
            s.push(' ');
            s.push_str(letter);
            s.push_str(&fmt_num(n.value()));
        }
    }
    s
}

fn diag(code: ErrorCode, msg: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code,
        stage: Stage::Emit,
        message: msg.into(),
        src: None,
        options: Vec::new(),
        limit: None,
    }
}

fn diag_src(code: ErrorCode, msg: impl Into<String>, src: &eid_core::ast::SrcSpan) -> Diagnostic {
    Diagnostic {
        code,
        stage: Stage::Emit,
        message: msg.into(),
        src: Some(src.clone()),
        options: Vec::new(),
        limit: None,
    }
}

// ===========================================================================
// Точка входа + валидация легальности
// ===========================================================================

/// Эмиссия без warnings-буфера (совместимость; предупреждения теряются).
pub fn emit(program: &Program) -> Result<String, Vec<Diagnostic>> {
    let mut sink = Vec::new();
    emit_with_warnings(program, &mut sink).map(|out| out.text)
}

/// Эмиссия + warnings с ДЕФОЛТНЫМИ опциями (поведение до этапа 8;
/// библиотечный emit и `FanucAdapter::emit` — байт-в-байт прежние).
pub fn emit_with_warnings(
    program: &Program,
    warnings: &mut Vec<String>,
) -> Result<EmitOut, Vec<Diagnostic>> {
    emit_with_profile(program, &FanucEmissionOpts::default(), warnings)
}

/// Эмиссия + warnings + профильные опции (этап 8, COVERAGE §5.1):
/// program_end_style (M30/M02), allow_r_arcs (R-форма + notice), g59_map
/// (G59.x → G54.1 Pn + notice «map применён, таблица подменена»).
/// `opts = Default` ⇒ байт-в-байт поведение до этапа 8.
pub fn emit_with_profile(
    program: &Program,
    opts: &FanucEmissionOpts,
    warnings: &mut Vec<String>,
) -> Result<EmitOut, Vec<Diagnostic>> {
    // Каноническая форма до эмиссии (R1: parse(emit(x)) — тот же AST) —
    // с Fanuc-спецификой хвоста (ruling 1 этапа 2, COVERAGE §5.1
    // «хвост-subs-случай»): блоки после PEND — line-level Raw (subs-тела
    // O#### после M30 — норма Fanuc DNC-файлов) — ЛЕГАЛЬНЫ в same-dialect
    // эмиссии. Core-verify структурно требует PEND последним (не знает о
    // Fanuc-политике хвоста) — её «program_end must be the last op»
    // отфильтровывается при условии «после PEND только Raw»; не-Raw опы
    // после PEND остаются отказом (SPEC §3).
    let tail_raw_only = match program
        .ops
        .iter()
        .rposition(|o| !matches!(o, Op::Raw { .. }))
    {
        Some(i) => matches!(program.ops[i], Op::ProgramEnd { .. }),
        None => false,
    };
    let mut ds: Vec<Diagnostic> = match parser::validate(program) {
        Ok(()) => Vec::new(),
        Err(v) => v,
    };
    if tail_raw_only {
        ds.retain(|d| {
            !(d.code == ErrorCode::InvalidInput
                && d.message.contains("program_end must be the last op"))
        });
    }
    // v0.20: start-проверки для опов после mid-G92-ребейза подавляются
    // (замороженная verify-модель разрыва mid-G92 не знает; зонды 1–3/8) —
    // маркер-механизм, зеркало linuxcnc strip_mid_g92_start_diags.
    ds = strip_mid_g92_start_diags(ds, &program.ops);
    if !ds.is_empty() {
        return Err(ds);
    }
    // Magic — ровно "EID/L1 v0.3" (grammar v0.3; SPEC §10.4: v0.2 невалиден).
    if program.eid_version != "EID/L1 v0.3" {
        return Err(vec![diag(
            ErrorCode::InvalidInput,
            format!(
                "eid_version `{}` не поддерживается (ожидается EID/L1 v0.3)",
                program.eid_version
            ),
        )]);
    }
    // Легальность эмиссии — против таблицы (COVERAGE §5.1), fail-fast;
    // wcs-домен — с учётом g59_map (этап 8: замапленные G59.x легальны).
    let table = crate::FanucAdapter::new().emission_legality();
    let problems = validate_fanuc(&table, program, opts);
    if !problems.is_empty() {
        return Err(problems);
    }
    let mut w = Writer::new(program, opts);
    w.run(&table, warnings)?;
    // Копия warnings в EmitOut — буфер вызывателя НЕ опустошается (и адаптер,
    // и тест читают свой Vec).
    Ok(EmitOut {
        text: w.text,
        warnings: warnings.clone(),
    })
}

/// v0.20: фильтрация start-диагностик core-verify для motion-опов, чей старт
/// получен ребейзом mid-program G92 (носитель — Raw-оп с reason-маркером
/// [`crate::canonicalize::MID_G92_REASON_PREFIX`]). Замороженная модель
/// разрыва фрейма `eid_core::verify` mid-G92 не знает и требует
/// `start = end предыдущего` (EQUIVALENCE §1.3) — после G92 это неверно
/// (зонд 1: start следующего хода = новое чтение). Ребейз последующего
/// движения мог быть zero-length-дропнут — «вооружение» снимается на первом
/// ВЫЖИВШЕМ motion-опе AST (зеркало linuxcnc strip_mid_g92_start_diags).
fn strip_mid_g92_start_diags(diags: Vec<Diagnostic>, ops: &[Op]) -> Vec<Diagnostic> {
    let mut suppress: std::collections::BTreeSet<u32> = Default::default();
    let mut armed = false;
    for op in ops {
        match op {
            // contains (не starts_with): маркер может быть суффиксом при
            // совмещении с другим raw-основанием строки (`G92 X0 G43 H1`).
            Op::Raw { reason, .. }
                if reason.contains(crate::canonicalize::MID_G92_REASON_PREFIX) =>
            {
                armed = true
            }
            Op::Rapid { src, .. } | Op::Linear { src, .. } | Op::Arc { src, .. } if armed => {
                suppress.insert(src.line_start);
                armed = false;
            }
            _ => {}
        }
    }
    if suppress.is_empty() {
        return diags;
    }
    diags
        .into_iter()
        .filter(|d| {
            !(d.code == ErrorCode::InvalidInput
                && d.message.starts_with("start")
                // Program-level диагностики (src: None) не трогаем.
                && d
                    .src
                    .as_ref()
                    .is_some_and(|s| suppress.contains(&s.line_start)))
        })
        .collect()
}

/// Валидация легальности эмиссии против таблицы (fail-fast, COVERAGE §5.1).
/// Коды отказов берутся ИСКЛЮЧИТЕЛЬНО из таблицы — единый путь отказа.
/// Этап 8: расширенные слоты проверяются с учётом g59_map — замапленные
/// легальны (эмиссия G54.1 Pn); при включённой мапе незмапленные слоты,
/// встреченные в программе, дают ОДИН отказ с перечнем незмапленных
/// (неполная мапа); мапа выключена — прежний per-slot отказ SYNTAX.
fn validate_fanuc(
    table: &FanucEmissionLegality,
    program: &Program,
    opts: &FanucEmissionOpts,
) -> Vec<Diagnostic> {
    // v0.2 волна 4 (§2.5 В1): код отказа — тотальный `cond.code()` на enum;
    // таблица — данные для интроспекции/тестов. Паник-точек на emit-пути нет.
    let code = |cond: FanucEmissionCondition| cond.code();
    let mut v = Vec::new();
    // Этап 8: расширенные слоты без маппинга при включённой g59_map —
    // собираются по всей программе, ОДИН отказ с перечнем в конце (агрегация
    // даёт полный список незмапленных сразу, а не по одному за прогон).
    let mut unmapped: Vec<&'static str> = Vec::new();
    // frame_preset.wcs — тоже использование слота (writer эмитит выборку
    // первым блоком; при map-off отказ — прежний writer-путь с его текстом).
    if let Some(pr) = &program.frame_preset {
        if wcs_word(pr.wcs).is_none() && opts.map_on() {
            let mapped = opts.g59_map.as_ref().and_then(|m| m.slot(pr.wcs)).is_some();
            if !mapped {
                if let Some(name) = FanucG59Map::slot_name(pr.wcs) {
                    if !unmapped.contains(&name) {
                        unmapped.push(name);
                    }
                }
            }
        }
    }
    // «Последний не-Raw оп = PEND» — Fanuc-хвост (Raw-subs после M30, ruling
    // 1) не нарушает структуру; не-Raw опыт после PEND — нарушение.
    let mut last_is_end = true; // отсутствует не-Raw оп ⇒ handled end_count==0
    for op in &program.ops {
        if !matches!(op, Op::Raw { .. }) {
            last_is_end = matches!(op, Op::ProgramEnd { .. });
        }
        // Вид опа, объявленный illegal в таблице (wcs_set_here) — отсекается
        // своим условием ниже; generic-ветка — только для НЕОБЪЯВленных видов
        // (защита полноты: новый вид опа ⇒ дополнить таблицу). Код — fallback
        // SYN (`op_kind_not_legal` в таблицу не входит — ветка покрытия).
        let kind = op_kind(op);
        if !table.is_legal_kind(kind) && !table.illegal.iter().any(|(c, _)| c.as_str() == kind) {
            // Защитный код — SYN по правилу COVERAGE §5 (вне словаря цели);
            // ветка покрытия (не в таблице — все 10 видов объявлены).
            v.push(diag_src(
                ErrorCode::UnsupportedSyntax,
                format!("op_kind_not_legal: `{kind}` не легален для цели Fanuc"),
                op_src(op),
            ));
        }
        match op {
            Op::WcsSetHere { src, .. } => v.push(diag_src(
                code(FanucEmissionCondition::WcsSetHere),
                "wcs_set_here: эмиссия в Fanuc запрещена как G10 L20 — Fanuc L20 = запись origin G54.1 Pn, НЕ «set here» (ловушка №3, COVERAGE §5.1); через G92 требует машинной позиции — не статично",
                src,
            )),
            Op::Arc { start, normal, state, src, .. } => {
                if matches!(start, StartPoint::Unknown) {
                    v.push(diag_src(
                        code(FanucEmissionCondition::ArcUnknownStart),
                        "arc with unknown start — IJK offsets require numeric start (SPEC §2 Q11)",
                        src,
                    ));
                }
                let n = [normal.x.clone(), normal.y.clone(), normal.z.clone()];
                if let Err(m) = plane_of(&n) {
                    v.push(diag_src(code(FanucEmissionCondition::ArcNormalNonCanonical), m, src));
                }
                check_motion_state(table, &mut v, state, src, true, opts, &mut unmapped);
                if state.frame == Frame::Machine {
                    v.push(diag_src(
                        code(FanucEmissionCondition::MachineFrameArc),
                        "machine_frame_arc: дуга в G53 не эмитится (machine-frame ходы прямые; SPEC §5.3)",
                        src,
                    ));
                }
            }
            Op::Linear { state, src, .. } => {
                check_motion_state(table, &mut v, state, src, true, opts, &mut unmapped)
            }
            Op::Rapid { state, src, .. } => {
                check_motion_state(table, &mut v, state, src, false, opts, &mut unmapped)
            }
            Op::Dwell { state, src, .. } | Op::ProgramStop { state, src, .. } => {
                check_wcs(table, &mut v, &state.wcs, src, opts, &mut unmapped);
            }
            Op::WcsDefine { p, state, src, .. } => {
                check_wcs(table, &mut v, &state.wcs, src, opts, &mut unmapped);
                if !(1..=6).contains(p) {
                    v.push(diag_src(
                        code(FanucEmissionCondition::WcsDefinePOutsideG54G59),
                        format!("wcs_define P{p} вне Fanuc-домена G54–G59 (P1..P6; COVERAGE §5.1 — кодов нет)"),
                        src,
                    ));
                }
            }
            Op::Raw { payload_hex, src, .. } => {
                if payload_hex.len() % 2 != 0
                    || !payload_hex
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(&c))
                {
                    v.push(diag_src(
                        code(FanucEmissionCondition::RawPayloadNotHex),
                        "raw_payload_not_hex: payload не чётный hex (grammar D9 v0.3)",
                        src,
                    ));
                }
            }
            _ => {}
        }
    }
    // Неполная мапа (этап 8): слоты, встреченные в программе без записи в
    // таблице — один отказ с перечнем незмапленных (класс — SYN, правило §5:
    // целевой код вне словаря Fanuc; wcs_outside_g54_g59-условие таблицы).
    if opts.map_on() && !unmapped.is_empty() {
        unmapped.sort_unstable();
        v.push(diag(
            FanucEmissionCondition::WcsOutsideG54G59.code(),
            format!(
                "wcs_map_incomplete: profile wcs_extended_map включена, но в программе есть слоты без маппинга: {} — эмиссия G54.1 Pn невозможна (дополните таблицу или выключите мапу; COVERAGE §5.1)",
                unmapped.join(", ")
            ),
        ));
    }
    // Инварианты §3 (енды): отсутствует / дубликат / не последний.
    let end_count = program
        .ops
        .iter()
        .filter(|o| matches!(o, Op::ProgramEnd { .. }))
        .count();
    if end_count == 0 {
        v.push(diag(
            code(FanucEmissionCondition::ProgramEndMissing),
            "program_end_missing: PEND обязателен ровно один раз, последним (SPEC §3)",
        ));
    }
    if end_count > 1 {
        v.push(diag(
            code(FanucEmissionCondition::ProgramEndDuplicate),
            format!("program_end_duplicate: {end_count} × PEND (SPEC §3)"),
        ));
    }
    if !last_is_end && end_count > 0 {
        v.push(diag(
            code(FanucEmissionCondition::ProgramEndNotLast),
            "program_end_not_last: опы после PEND запрещены (SPEC §3)",
        ));
    }
    v
}

fn op_src(op: &Op) -> &eid_core::ast::SrcSpan {
    match op {
        Op::Rapid { src, .. }
        | Op::Linear { src, .. }
        | Op::Arc { src, .. }
        | Op::Dwell { src, .. }
        | Op::ToolChange { src, .. }
        | Op::ProgramStop { src, .. }
        | Op::ProgramEnd { src }
        | Op::WcsDefine { src, .. }
        | Op::WcsSetHere { src, .. }
        | Op::Raw { src, .. } => src,
    }
}

fn check_motion_state(
    _table: &FanucEmissionLegality,
    v: &mut Vec<Diagnostic>,
    state: &eid_core::ast::State,
    src: &eid_core::ast::SrcSpan,
    need_feed: bool,
    opts: &FanucEmissionOpts,
    unmapped: &mut Vec<&'static str>,
) {
    if need_feed && state.feed.is_none() {
        v.push(diag_src(
            FanucEmissionCondition::MotionFeedNone.code(),
            "motion_feed_none: linear/arc без модального feed (SPEC §3/§4)",
            src,
        ));
    }
    check_wcs(_table, v, &state.wcs, src, opts, unmapped);
}

/// WCS-домен эмиссии (этап 8): мапа выключена → прежний per-slot отказ
/// SYNTAX; мапа включена → замапленные слоты легальны (writer эмитит
/// G54.1 Pn + notice), незмапленные собираются в `unmapped` для агрегированного
/// отказа с перечнем.
fn check_wcs(
    _table: &FanucEmissionLegality,
    v: &mut Vec<Diagnostic>,
    wcs: &Wcs,
    src: &eid_core::ast::SrcSpan,
    opts: &FanucEmissionOpts,
    unmapped: &mut Vec<&'static str>,
) {
    if wcs_word(*wcs).is_some() {
        return;
    }
    if opts.map_on() {
        let mapped = opts.g59_map.as_ref().and_then(|m| m.slot(*wcs)).is_some();
        if !mapped {
            if let Some(name) = FanucG59Map::slot_name(*wcs) {
                if !unmapped.contains(&name) {
                    unmapped.push(name);
                }
            }
        }
    } else {
        v.push(diag_src(
            FanucEmissionCondition::WcsOutsideG54G59.code(),
            "wcs_outside_g54_g59: state.wcs = G59.1/.2/.3 — в словаре Fanuc кодов нет (COVERAGE §5.1; G54.1 — другая таблица с чужим происхождением; profile-опция wcs_extended_map выключена — подмена таблицы без notice запрещена)",
            src,
        ));
    }
}

// ===========================================================================
// Писатель
// ===========================================================================

struct Writer<'a> {
    program: &'a Program,
    text: String,
    // Профильные эмиссионные опции (этап 8; Default = поведение до этапа 8).
    opts: FanucEmissionOpts,
    // notice «map применён» — один на эмиссию (первое применение таблицы).
    map_notice_done: bool,
    // Модальное состояние вывода (то, что увидит reparse).
    cur_wcs: Wcs,
    cur_plane: GCode,
    cur_spindle: Spindle,
    cur_coolant: Coolant,
    cur_feed: Option<Num>,
    cur_tool: Option<u32>,
}

impl<'a> Writer<'a> {
    fn new(program: &'a Program, opts: &FanucEmissionOpts) -> Self {
        let cur_wcs = program
            .frame_preset
            .as_ref()
            .map(|p| p.wcs)
            .unwrap_or(Wcs::G54);
        Self {
            program,
            text: String::new(),
            opts: opts.clone(),
            map_notice_done: false,
            cur_wcs,
            cur_plane: GCode::PlaneXY,
            cur_spindle: Spindle::Off,
            cur_coolant: Coolant::default(),
            cur_feed: None,
            cur_tool: None,
        }
    }

    fn line(&mut self, s: impl Into<String>) {
        self.text.push_str(&s.into());
        self.text.push('\n');
    }

    /// WCS-слово эмиссии с учётом g59_map (этап 8). Нативные G54–G59 — как
    /// раньше (без notice); расширенный слот при включённой мапе — `G54.1 Pn`
    /// с ОДНИМ notice «map применён, таблица подменена» на эмиссию (аналогия
    /// golden_preset_no_bake: подмена таблицы WCS громкая). Незмапленный —
    /// Err (fail-fast валидатором уже отсечён; здесь защита полноты).
    fn wcs_emit_word(
        &mut self,
        wcs: Wcs,
        warnings: &mut Vec<String>,
    ) -> Result<String, Vec<Diagnostic>> {
        if let Some(word) = wcs_word(wcs) {
            return Ok(word.to_string());
        }
        let map = match self.opts.g59_map.as_ref() {
            Some(m) if !m.is_empty() => m,
            // map-off: прежний текст отказа (валидатор дал per-slot diag с
            // каноническим условием; writer-ветка — защита полноты).
            _ => {
                return Err(vec![diag(
                FanucEmissionCondition::WcsOutsideG54G59.code(),
                "state.wcs вне словаря Fanuc — wcs-слово не эмитится (валидатор отсекает ранее)",
            )])
            }
        };
        let p = map.slot(wcs).ok_or_else(|| {
            vec![diag(
                FanucEmissionCondition::WcsOutsideG54G59.code(),
                format!(
                    "wcs_map_incomplete: слот {} без маппинга в profile wcs_extended_map (валидатор отсекает ранее — перечень незмапленных в его диагностике)",
                    FanucG59Map::slot_name(wcs).unwrap_or("G59.x")
                ),
            )]
        })?;
        if !self.map_notice_done {
            warnings.push(map_notice_text(map));
            self.map_notice_done = true;
        }
        Ok(format!("G54.1 P{p}"))
    }

    fn run(
        &mut self,
        table: &FanucEmissionLegality,
        warnings: &mut Vec<String>,
    ) -> Result<(), Vec<Diagnostic>> {
        // 1. Заголовок — всегда явно (units §2.5; G90/G94 — дефолты; G17;
        //    G91.1 — инкрементальные центры pinned, см. модуль-док).
        let units_word = if self.program.units == Units::In {
            "G20"
        } else {
            "G21"
        };
        self.line(format!("{units_word} G90 G94 G17 G91.1"));
        // 2. path_mode — всегда явно, одна строка (SPEC §3: одно на программу).
        //    Fanuc value shift (COVERAGE §2.2/§5.1): exact_stop → G61 (1:1 по
        //    значению); exact_path → G61 консервативно + warning (профиль
        //    bare G64 — этап 8); blend(t) → bare G64 + warning при t.
        match &self.program.path_mode {
            eid_core::ast::PathMode::ExactStop => self.line("G61"),
            eid_core::ast::PathMode::ExactPath => {
                self.line("G61");
                if let Some(t) = table.warning_of("path_mode_exact_path") {
                    warnings.push(t.to_string());
                }
            }
            eid_core::ast::PathMode::Blend { t } => {
                self.line("G64");
                if t.is_some() {
                    if let Some(w) = table.warning_of("path_mode_blend_t") {
                        warnings.push(w.to_string());
                    }
                }
            }
        }
        // 3. frame_preset: выборка WCS (этап 8: расширенный слот при
        //    включённой g59_map → G54.1 Pn + notice), затем G92 первым
        //    блоком (EQUIVALENCE §1.4); без запекания в G10. G92-hygiene
        //    (G92.1 первым блоком) — флаг профиля, НЕ дефолт — этап 8
        //    (COVERAGE §5.1).
        if let Some(pr) = &self.program.frame_preset {
            let w = self.wcs_emit_word(pr.wcs, warnings).map_err(|mut d| {
                // Сохранение прежнего текста отказа для map-off (письмо
                // диагностик — writer-путь, COVERAGE §5.1).
                if !self.opts.map_on() {
                    d = vec![diag(
                        FanucEmissionCondition::WcsOutsideG54G59.code(),
                        "frame_preset.wcs вне словаря Fanuc (G59.x) — кодов нет (COVERAGE §5.1)",
                    )];
                }
                d
            })?;
            self.line(w);
            self.line(format!(
                "G92 X{} Y{} Z{}",
                fmt_num(pr.values.x.value()),
                fmt_num(pr.values.y.value()),
                fmt_num(pr.values.z.value())
            ));
        }
        // 4. Опы.
        for op in &self.program.ops {
            self.op(op, warnings)?;
        }
        Ok(())
    }

    /// Дельты снапшота состояния перед строкой опа (фиксированный порядок:
    /// WCS → шпиндель → СОЖ; plane — только у дуг).
    fn pre_state(
        &mut self,
        state: &eid_core::ast::State,
        warnings: &mut Vec<String>,
    ) -> Result<(), Vec<Diagnostic>> {
        if state.wcs != self.cur_wcs {
            // Этап 8: расширенный слот при включённой g59_map → G54.1 Pn +
            // notice «map применён» (один на эмиссию).
            let w = self.wcs_emit_word(state.wcs, warnings)?;
            self.line(w);
            self.cur_wcs = state.wcs;
        }
        if !spindle_eq(&state.spindle, &self.cur_spindle) {
            let line = match &state.spindle {
                Spindle::Off => "M5".to_string(),
                Spindle::Cw(rpm) => format!("M3 S{}", fmt_num(rpm.value())),
                Spindle::Ccw(rpm) => format!("M4 S{}", fmt_num(rpm.value())),
            };
            self.line(line);
            self.cur_spindle = state.spindle.clone();
        }
        if state.coolant != self.cur_coolant {
            // Выключения (M9 гасит обе) — до включений; порядок M7, M8.
            let off_needed = (self.cur_coolant.flood && !state.coolant.flood)
                || (self.cur_coolant.mist && !state.coolant.mist);
            if off_needed {
                self.line("M9");
            }
            if state.coolant.mist {
                self.line("M7");
            }
            if state.coolant.flood {
                self.line("M8");
            }
            self.cur_coolant = state.coolant;
        }
        Ok(())
    }

    fn op(&mut self, op: &Op, warnings: &mut Vec<String>) -> Result<(), Vec<Diagnostic>> {
        match op {
            Op::Rapid {
                end, state, sub, ..
            }
            | Op::Linear {
                end, state, sub, ..
            } => {
                self.pre_state(state, warnings)?;
                self.check_tool_jump(state, warnings);
                let f = self.feed_word(state, *sub, warnings)?;
                let g = match (op, state.frame) {
                    (_, Frame::Machine) => {
                        if matches!(op, Op::Rapid { .. }) {
                            "G53 G0"
                        } else {
                            "G53 G1"
                        }
                    }
                    (Op::Rapid { .. }, _) => "G0",
                    (_, _) => "G1",
                };
                // v0.4 §1.2 (SPEC v0.22): resolved rotary-слоты конца — слова
                // A/B/C (канонический порядок, DPB-формат зеркален X/Y/Z);
                // рестейт легален — модальность НЕ оптимизируется. Все None
                // (3-осевой оп) — пусто, байт-идентично прежнему. Дуга не
                // пишет A/B/C никогда (ротационная дуга вне канона §6).
                self.line(format!(
                    "{g} X{} Y{} Z{}{}{f}",
                    fmt_num(end.x.value()),
                    fmt_num(end.y.value()),
                    fmt_num(end.z.value()),
                    rot_words(end)
                ));
                Ok(())
            }
            Op::Arc {
                start: StartPoint::Known(start),
                end,
                center,
                normal,
                dir,
                sweep_deg,
                pitch,
                state,
                sub,
                ..
            } => {
                self.pre_state(state, warnings)?;
                self.check_tool_jump(state, warnings);
                // Плоскость из канонической нормали (валидатор отсёк не-ось).
                let (plane, a0, a1, _naxis) =
                    plane_of(&[normal.x.clone(), normal.y.clone(), normal.z.clone()])
                        .map_err(|m| vec![diag(ErrorCode::InvalidInput, m)])?;
                if plane != self.cur_plane {
                    self.line(plane_word(plane));
                    self.cur_plane = plane;
                }
                // Центр: ТОЛЬКО in-plane пара, инкремент от старта (G91.1 в
                // заголовке); normal-компонента не эмитится (warning при
                // отличии от старта — канон-форма требует ⟂-компоненту = старта).
                let sv = to_f64(start);
                let cv = to_f64(center);
                let mut offs = [0.0f64; 3];
                offs[a0] = cv[a0] - sv[a0];
                offs[a1] = cv[a1] - sv[a1];
                let dn = cv[[0, 1, 2][_naxis]] - sv[[0, 1, 2][_naxis]];
                if dn.abs() > 0.001 {
                    warnings.push(format!(
                        "arc: normal-компонента центра {dn:+.4} не эмитится (каноническая форма требует center ⟂-компоненту = старта); геометрия дуги не меняется"
                    ));
                }
                let f = self.feed_word(state, *sub, warnings)?;
                let g = if *dir == Dir::Cw { "G2" } else { "G3" };
                // Полный круг (sweep=360°, pitch=0) — БЕЗ end-слов (`G2 I_ J_`
                // — легальный Fanuc-формат; слова end==start перепарсер
                // квантует в zero-length drop). Винтовой полный оборот
                // (pitch≠0) — ТОЛЬКО end-слово вне-плоскостной оси (in-plane
                // наследуют старт). Частичная дуга — все три оси.
                let full_turn = (sweep_deg.value() - 360.0).abs() <= 1e-6;
                let helical_turn = full_turn && pitch.value().abs() > 1e-9;
                // Слова дуги (этап 8): при profile-флаге allow_r_arcs
                // (= emit-r-arcs; COVERAGE §5.1 «R — profile-флаг + notice»)
                // дуга эмитится R-формой, иначе — IJK (EQUIVALENCE §4).
                // Полные круги — ВСЕГДА IJK: R при совпадающих in-plane
                // start/end неоднозначен (SPEC §6; R-полный круг — alarm).
                let arc_words = if self.opts.allow_r_arcs && !full_turn {
                    // Радиус = |центр − start| в плоскости дуги; знак R — по
                    // sweep (Fanuc-соглашение: R>0 = ≤180°, R<0 = >180°;
                    // парсер восстанавливает центр и sweep из хорды и |R| —
                    // канонизатор arc_block, формула LinuxCNC/grbl). R — 4dp.
                    let rho = offs[a0].hypot(offs[a1]);
                    let r_signed = if sweep_deg.value() > 180.0 { -rho } else { rho };
                    warnings.push(format!(
                        "arc: R-форма применена по профилю (allow-r-arcs = emit-r-arcs = true): R{} вместо IJK — та же дуга, центр пересчитывается из R (COVERAGE §5.1; EQUIVALENCE §4)",
                        fmt_num(r_signed)
                    ));
                    format!("R{}", fmt_num(r_signed))
                } else {
                    if self.opts.allow_r_arcs && full_turn {
                        warnings.push(
                            "arc: полный круг 360° — R-форма неоднозначна (совпадающие in-plane start/end, SPEC §6); эмитится IJK даже при allow_r_arcs".into(),
                        );
                    }
                    // Слова смещений в порядке осей X→I, Y→J, Z→K (конвенция
                    // grbl/linuxcnc; I/J/K literal X/Y/Z): ровно две in-plane
                    // оси, G18 → I+K, G19 → J+K, G17 → I+J.
                    let letter = ["I", "J", "K"];
                    let mut words = String::new();
                    for ax in 0..3 {
                        if ax == a0 || ax == a1 {
                            words.push_str(&format!("{}{} ", letter[ax], fmt_num(offs[ax])));
                        }
                    }
                    words.trim_end().to_string()
                };
                let axis_letter = ["X", "Y", "Z"][_naxis];
                let axes_part = if full_turn && !helical_turn {
                    String::new()
                } else if helical_turn {
                    let ev = to_f64(end);
                    format!("{}{} ", axis_letter, fmt_num(ev[_naxis]))
                } else {
                    format!(
                        "X{} Y{} Z{} ",
                        fmt_num(end.x.value()),
                        fmt_num(end.y.value()),
                        fmt_num(end.z.value())
                    )
                };
                self.line(format!("{g} {axes_part}{arc_words}{f}"));
                Ok(())
            }
            Op::Arc {
                start: StartPoint::Unknown,
                ..
            } => {
                // validate() отсекает такие программы ранее; защита.
                Err(vec![diag(
                    ErrorCode::RuntimeState,
                    "arc с start=unknown не эмитится (SPEC §2 Q11)",
                )])
            }
            Op::Dwell { seconds, state, .. } => {
                self.pre_state(state, warnings)?;
                self.check_tool_jump(state, warnings);
                if !seconds.value.is_finite() {
                    return Err(vec![diag(
                        ErrorCode::InvalidInput,
                        "dwell seconds: non-finite",
                    )]);
                }
                // Правило COVERAGE §5.1: seconds·1000 целое (в допуске) →
                // P = целые мс (decimal point ЗАПРЕЩЁН — DPI-домен P);
                // иначе sub-ms остаток → X-форма (float-секунды с точкой).
                let ms = seconds.value * 1000.0;
                if (ms - ms.round()).abs() < 1e-6 {
                    self.line(format!("G4 P{}", ms.round() as i64));
                } else {
                    self.line(format!("G4 X{}", fmt_num(seconds.value)));
                }
                Ok(())
            }
            Op::ToolChange { tool, state, .. } => {
                self.pre_state(state, warnings)?;
                // Fanuc-идиома: T## M06 в одном блоке (1:1, COVERAGE §5.1).
                self.line(format!("T{tool} M06"));
                self.cur_tool = Some(*tool);
                Ok(())
            }
            Op::ProgramStop {
                optional, state, ..
            } => {
                self.pre_state(state, warnings)?;
                self.check_tool_jump(state, warnings);
                self.line(if *optional { "M1" } else { "M0" });
                Ok(())
            }
            Op::ProgramEnd { .. } => {
                // Хвост M5/M9 НЕ эмитится (ruling COVERAGE §5: AST не хранит
                // пост-конечное состояние; M30 сам глушит; M02 — профиль
                // этапа 8 без rewind-семантики).
                self.line(self.opts.program_end.word());
                Ok(())
            }
            Op::WcsDefine {
                p, origin, state, ..
            } => {
                self.pre_state(state, warnings)?;
                self.check_tool_jump(state, warnings);
                self.line(format!(
                    "G10 L2 P{p} X{} Y{} Z{}",
                    fmt_num(origin.x.value()),
                    fmt_num(origin.y.value()),
                    fmt_num(origin.z.value())
                ));
                Ok(())
            }
            Op::WcsSetHere { .. } => {
                // Валидатор отсекает ранее (ловушка №3); защита полноты.
                Err(vec![diag(
                    ErrorCode::UnsupportedSemantics,
                    "wcs_set_here не эмитится в Fanuc: L20 = запись G54.1 Pn, НЕ «set here» (COVERAGE §5.1)",
                )])
            }
            Op::Raw { payload_hex, .. } => self.raw(payload_hex),
        }
    }

    /// F-слово при отличии модального feed от текущего выведенного; иначе "".
    /// F на rapid-кадре воспроизводится (меняет модальный feed — traverse
    /// развёртки §7.7-1 несёт feed кадра). Число — 4dp с точкой ВСЕГДА.
    fn feed_word(
        &mut self,
        state: &eid_core::ast::State,
        sub: Option<u32>,
        warnings: &mut Vec<String>,
    ) -> Result<String, Vec<Diagnostic>> {
        match &state.feed {
            Some(f) => {
                if !f.value.is_finite() {
                    return Err(vec![diag(ErrorCode::InvalidInput, "feed: non-finite")]);
                }
                let changed = match &self.cur_feed {
                    None => true,
                    Some(c) => c.value != f.value,
                };
                // F-кадр развёртки (SPEC §7.7-1, зеркало D feed_word): F также
                // на ПЕРВОМ производном опе каждого цикла (sub == Some(1)) —
                // иначе повторный F-кадр с тем же значением (G81 F120; G81
                // F120) теряет маркер кадра при reparse.
                let first_derived = sub == Some(1);
                // v0.2 волна 3 (SPEC §7.4a): restated-F теперь из AST —
                // провенанс «F-слово было в кадре» (точнее sub=1-эвристики).
                let restated = state.feed_restated;
                if changed || first_derived || restated {
                    self.cur_feed = Some(f.clone());
                    Ok(format!(" F{}", fmt_num(f.value)))
                } else {
                    Ok(String::new())
                }
            }
            None => {
                if self.cur_feed.is_some() {
                    // Несвязный снапшот: modal feed не может «исчезнуть».
                    warnings.push(
                        "state.feed = none при известном модальном feed — скачок снапшота не воспроизводим (F остаётся модальным)".into(),
                    );
                }
                Ok(String::new())
            }
        }
    }

    /// state.tool без ToolChange-опа — несвязный снапшот (eid-text AST не
    /// проверяет непрерывность состояния): воспроизвести нечем, warning.
    fn check_tool_jump(&mut self, state: &eid_core::ast::State, warnings: &mut Vec<String>) {
        if state.tool != self.cur_tool {
            warnings.push(format!(
                "state.tool = {:?} без ToolChange-опа — скачок снапшота не воспроизводим (выведенный tool = {:?})",
                state.tool, self.cur_tool
            ));
            // cur_tool не обновляем: в выводе инструмент = последний M06.
        }
    }

    /// RAW same-dialect fidelity: payload-hex → исходные байты строки ВКЛЮЧАЯ
    /// терминатор (grammar D9 v0.3) → дословно; внешний '\n' только если
    /// декод не кончается им.
    fn raw(&mut self, payload_hex: &str) -> Result<(), Vec<Diagnostic>> {
        if payload_hex.len() % 2 != 0 {
            return Err(vec![diag(
                ErrorCode::InvalidInput,
                "raw payload: bad hex length",
            )]);
        }
        let mut bytes = Vec::with_capacity(payload_hex.len() / 2);
        for i in (0..payload_hex.len()).step_by(2) {
            match u8::from_str_radix(&payload_hex[i..i + 2], 16) {
                Ok(b) => bytes.push(b),
                Err(_) => {
                    return Err(vec![diag(
                        ErrorCode::InvalidInput,
                        "raw payload: bad hex pair",
                    )])
                }
            }
        }
        let s = String::from_utf8_lossy(&bytes).into_owned();
        if s.ends_with('\n') {
            self.text.push_str(&s);
        } else {
            self.text.push_str(&s);
            self.text.push('\n');
        }
        Ok(())
    }
}

/// Notice применения g59_map (этап 8, COVERAGE §5.1 — «подмена таблицы без
/// предупреждения запрещена», аналогия golden_preset_no_bake): один на
/// эмиссию, текст перечисляет всю применённую таблицу (детерминированный
/// порядок G59.1/.2/.3).
fn map_notice_text(map: &FanucG59Map) -> String {
    let mut entries = Vec::new();
    for (name, p) in [
        ("G59.1", map.g59_1),
        ("G59.2", map.g59_2),
        ("G59.3", map.g59_3),
    ] {
        if let Some(p) = p {
            entries.push(format!("{name}→G54.1 P{p}"));
        }
    }
    format!(
        "wcs_extended_map: map применён, таблица WCS подменена профилем ({}) — G54.1 Pn имеет ЧУЖОЕ происхождение origin относительно G59.x-таблиц источника, эквивалентность условна по таблицам станка (EQUIVALENCE §1.2; COVERAGE §5.1; НЕ дефолт — аналогия golden_preset_no_bake)",
        entries.join(", ")
    )
}

fn spindle_eq(a: &Spindle, b: &Spindle) -> bool {
    match (a, b) {
        (Spindle::Off, Spindle::Off) => true,
        (Spindle::Cw(x), Spindle::Cw(y)) | (Spindle::Ccw(x), Spindle::Ccw(y)) => {
            x.value() == y.value()
        }
        _ => false,
    }
}

fn plane_word(p: GCode) -> &'static str {
    match p {
        GCode::PlaneXY => "G17",
        GCode::PlaneZX => "G18",
        GCode::PlaneYZ => "G19",
        _ => "G17",
    }
}
