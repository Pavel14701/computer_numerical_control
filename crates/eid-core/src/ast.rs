//! EID/L1 AST — замороженный интерфейс v0.1.
//! Источники: SPEC-EID-L1 v0.9 (§3 модель, §5 опы, §10 числа), grammar.ebnf v0.1 (D1–D10).

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

#[derive(Debug, Clone, PartialEq)]
pub struct Point3 {
    pub x: Num,
    pub y: Num,
    pub z: Num,
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
}

/// Start motion-опа: известная точка или unknown (Q11 — точки разрыва (a)–(d),
/// SPEC §2 принцип 2; связывание вне точек разрыва — обязанность канонизатора).
#[derive(Debug, Clone, PartialEq)]
pub enum StartPoint {
    Known(Point3),
    Unknown,
}

/// Source span (SPEC §11): file, line.col–line.col. В golden AST — плейсхолдер 0.1-0.0 (D10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrcSpan {
    pub file: String,
    pub line_start: u32,
    pub col_start: u32,
    pub line_end: u32,
    pub col_end: u32,
}

/// 10 типов опов (SPEC §5). RAW и PEND не несут state (D6).
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
    pub eid_version: String, // = версия grammar.ebnf, напр. "EID/L1 v0.1"
    pub units: Units,
    pub path_mode: PathMode,
    pub frame_preset: Option<FramePreset>,
    pub ops: Vec<Op>,
}
