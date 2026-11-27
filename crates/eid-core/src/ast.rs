//! EID/L1 AST — интерфейс v0.2 (разморозка #21: SrcSpan.file → Arc<str>;
//! INTERFACE-V0.2-DESIGN §2.1). Источники: SPEC-EID-L1 v0.10 (§3 модель,
//! §5 опы, §10 числа), grammar.ebnf v0.2 (D1–D10).

use std::sync::Arc;

/// Число: verbatim-десятичная строка + f64-зеркало (SPEC §10.1).
/// Сравнение — на f64 (EQUIVALENCE §4); текст — для детерминированной эмиссии (4 dp).
#[derive(Debug, Clone, PartialEq)]
pub struct Num {
    pub text: String,
    pub value: f64,
}

impl Num {
    pub fn new(text: String, value: f64) -> Self {
        Self { text, value }
    }
    pub fn value(&self) -> f64 {
        self.value
    }
}

/// L1 v0.4-амендрмент (§1.2 ревизия, SPEC v0.22): ротационные слоты позы —
/// RESOLVED joint-углы: Some = слово кадра ИЛИ унаследованная модальность
/// (канон resolved-state: canonicalize/lowering заполняют наследование);
/// None = угол не установлен в программе (станок в home/дефолте). Слоты НЕ
/// несут кинематики — 6 чисел, трансмиссия результата inverse (§15.4
/// REPORT); применение inverse — только L2-маршрут с профилем.
/// Инварианты: (1) эмиссия Some → слово, None → молчание — 3-осевой текст
/// байт-идентичен v0.3; (2) center/normal/frame_preset/anchor-Point3 —
/// всегда None (конструктор [`Point3::xyz`]); (3) ARC-конец — всегда None
/// (ротационная дуга вне канона v0.4 — носитель, SPEC §6); (4) zero-length
/// и linking сравнивают слоты (чисто-rotary ход ≠ zero-length, §3).
#[derive(Debug, Clone, PartialEq)]
pub struct Point3 {
    pub x: Num,
    pub y: Num,
    pub z: Num,
    /// A — joint-угол (градусы, resolved). None = не установлен.
    pub a: Option<Num>,
    /// B — joint-угол (градусы, resolved).
    pub b: Option<Num>,
    /// C — joint-угол (градусы, resolved).
    pub c: Option<Num>,
}

impl Point3 {
    /// 3-осевая точка (center/normal/frame_preset/anchor/legacy-сайты):
    /// слоты None — инвариант не-позовых применений (§1.2 v0.4).
    pub fn xyz(x: Num, y: Num, z: Num) -> Self {
        Self {
            x,
            y,
            z,
            a: None,
            b: None,
            c: None,
        }
    }

    /// Конец хода с resolved-слотами (Rapid/Linear; ARC запрещён — §6).
    pub fn pose(x: Num, y: Num, z: Num, a: Option<Num>, b: Option<Num>, c: Option<Num>) -> Self {
        Self { x, y, z, a, b, c }
    }

    /// Слоты сравнены resolved-точно (None==None, Some==Some по значению —
    /// EQUIVALENCE §4; текст может отличаться — verbatim-предпочтение
    /// эмиссии канонизирует).
    pub fn rotary_eq(&self, other: &Self) -> bool {
        let slot_eq = |s: &Option<Num>, o: &Option<Num>| match (s, o) {
            (None, None) => true,
            (Some(s), Some(o)) => s.value == o.value,
            _ => false,
        };
        slot_eq(&self.a, &other.a) && slot_eq(&self.b, &other.b) && slot_eq(&self.c, &other.c)
    }

    /// Есть ли хоть один установленный слот (3-осевой ли это конец).
    pub fn has_rotary(&self) -> bool {
        self.a.is_some() || self.b.is_some() || self.c.is_some()
    }
}

pub type Vec3 = Point3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Units {
    Mm,
    In,
}

/// PATH_MODE — метаданные программы, одно значение (SPEC §3).
/// q (Naive CAM Detector) в AST не входит — теряется с warning (COVERAGE §2.2).
#[derive(Debug, Clone, PartialEq)]
pub enum PathMode {
    ExactPath,                // G61
    ExactStop,                // G61.1
    Blend { t: Option<Num> }, // G64 [P-]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wcs {
    G54,
    G55,
    G56,
    G57,
    G58,
    G59,
    G59_1,
    G59_2,
    G59_3,
}

/// G92-пресетка: значения, присваиваемые текущей позиции + WCS, активная в момент
/// пресетки (SPEC v0.5; EQUIVALENCE §1.3: Δ = M₀ − O₀ − frame_preset).
#[derive(Debug, Clone, PartialEq)]
pub struct FramePreset {
    pub values: Point3,
    pub wcs: Wcs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Cw,
    Ccw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame {
    Wcs,
    Machine,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Spindle {
    Off,
    Cw(Num),
    Ccw(Num),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Coolant {
    pub flood: bool,
    pub mist: bool,
}

/// Полный снапшот состояния на каждом не-RAW/не-PEND опе (EQUIVALENCE §2).
/// Порядок эмиссии полей: feed, spindle, coolant, tool, frame, wcs (grammar D3).
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub feed: Option<Num>, // none только до первого F; G0 не сбрасывает (SPEC §4)
    pub spindle: Spindle,
    pub coolant: Coolant,
    pub tool: Option<u32>, // физически установленный инструмент (SPEC §4 v0.3)
    pub frame: Frame,
    pub wcs: Wcs,
    /// Провенанс «F-слово было в кадре-источнике этого опа» (v0.2 волна 3,
    /// INTERFACE-V0.2-DESIGN §2.4: restated-F, хвост v0.18/SPEC §7.4a).
    /// НЕ сериализуется в EID-текст (emit_state грамматику не меняет —
    /// goldens байт-в-байт целы, eid_version без bump), semantic_diff
    /// исключает (провенанс-класс src/sub, EQUIVALENCE §5). Эмиттеры
    /// воспроизводят F-кадр по флагу: `changed ∨ first_derived ∨ restated`.
    /// Развёртка циклов: флаг несёт ТОЛЬКО первый производный оп (sub=1,
    /// = кадру-источнику); производные 2..n — false.
    pub feed_restated: bool,
}

/// Start motion-опа: известная точка или unknown (Q11 — точки разрыва (a)–(d),
/// SPEC §2 принцип 2; связывание вне точек разрыва — обязанность канонизатора).
#[derive(Debug, Clone, PartialEq)]
pub enum StartPoint {
    Known(Point3),
    Unknown,
}

/// Source span (SPEC §11): file, line.col–line.col. В golden AST — плейсхолдер 0.1-0.0 (D10).
///
/// v0.2 (#21, INTERFACE-V0.2-DESIGN §2.1): `file` — `Arc<str>` вместо `String`.
/// Адаптер держит один `Arc<str>` на прогон parse (Ctx.file), `span()` клонирует
/// хэндл (refcount) — 0 аллокаций на оп (−150k аллокаций на 150k-оповую
/// программу: сборка 1.91M→1.76M, kept-pass-клон 1.41M→1.26M, замер §0
/// дизайн-дока). Клон SrcSpan-поля (op_src в диагностике/verify kept-pass) —
/// refcount, 0 аллокаций.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrcSpan {
    pub file: Arc<str>,
    pub line_start: u32,
    pub col_start: u32,
    pub line_end: u32,
    pub col_end: u32,
}

/// 10 типов опов (SPEC §5). RAW и PEND не несут state (D6).
// Точечный allow с обоснованием (clippy-гейт CI, волна аудита A): finding
// PRE-EXISTING (не из правок волны — файл не менялся с freeze). Code-fix по
// подсказке clippy = Box<State>/Box<SrcSpan> в 7 вариантах — изменение
// ЗАМОРОЖЕННОГО интерфейса v0.1 (lib.rs: «изменение сигнатур — только PR к
// оркестратору») и compile-break eid-grbl / eid-linuxcnc / eid-fanuc (агент C
// в полёте) / eid-harness generator.rs (зона B). Кандидат на разморозку API —
// оркестратор; до тех пор гейт держится на этом атрибуте, а не на
// crate-wide allow (удалён, аудит #24).
// Измерение (INTERFACE-V0.2-DESIGN §0/§2.3, prob-крейт с counting-аллокатором,
// 150k опов 85/10/5): Box-PR (Box<State>+Box<SrcSpan> в 7 вариантах) —
// +300k аллокаций на сборке (1.91M→2.21M) и на kept-pass-клоне
// (1.41M→1.71M, +21% на самой горячей фазе), −19% только на размере Vec;
// линт НЕ гаснет (ARC-вариант: 584−320 = 264 > 200 — доминируют 3 лишних
// Point3). Решение §2.3: отклонить Box-PR, allow с этими цифрами остаётся
// (post-v0.2: Op 584 B, Rapid/LINEAR 320, ARC 584 — зазор прежний 264).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Rapid {
        start: StartPoint,
        end: Point3,
        state: State,
        src: SrcSpan,
        sub: Option<u32>, // развёрнутые циклы (SPEC §11)
    },
    Linear {
        start: StartPoint,
        end: Point3,
        state: State,
        src: SrcSpan,
        sub: Option<u32>,
    },
    Arc {
        start: StartPoint,
        end: Point3,
        center: Point3,
        normal: Vec3,
        dir: Dir,
        sweep_deg: Num,
        pitch: Num,
        state: State,
        src: SrcSpan,
        sub: Option<u32>,
    },
    Dwell {
        seconds: Num,
        state: State,
        src: SrcSpan,
        sub: Option<u32>,
    },
    ToolChange {
        tool: u32,
        state: State,
        src: SrcSpan,
    },
    ProgramStop {
        optional: bool, // M0 = false, M1 = true (Q8)
        state: State,
        src: SrcSpan,
    },
    ProgramEnd {
        src: SrcSpan,
    },
    WcsDefine {
        p: u8, // 1..9
        origin: Point3,
        state: State,
        src: SrcSpan,
    }, // G10 L2
    WcsSetHere {
        p: u8, // 1..9
        delta: Vec3,
        state: State,
        src: SrcSpan,
    }, // G10 L20
    Raw {
        payload_hex: String, // hex исходных байтов, lossless (grammar D9)
        reason: String,
        code: crate::diag::ErrorCode,
        src: SrcSpan,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub eid_version: String, // = версия grammar.ebnf, напр. "EID/L1 v0.2"
    pub units: Units,
    pub path_mode: PathMode,
    pub frame_preset: Option<FramePreset>,
    pub ops: Vec<Op>,
}
