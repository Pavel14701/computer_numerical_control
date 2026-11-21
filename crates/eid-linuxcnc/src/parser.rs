//! LinuxCNC-парсер (Агент D, итерация 2): лексированные кадры → EID Program.
//!
//! Слой 1 — lexer.rs (слова/комментарии/жёсткие лексические отказы);
//! слой 2 — этот файл: диспозиции COVERAGE (матрица §2–§4), модальные группы,
//! resolved-state (SPEC §4), связывание стартов Q11 (SPEC §2 принцип 2),
//! структурные инварианты §3 (PEND ровно один раз и последний), лимиты §1.2;
//! слой 3 — canonicalize.rs (геометрия дуг §6).
//!
//! Классы отказов (правило двух классов, CORPUS §1.2 v0.5):
//!  * жёсткий Err — структурные (искажают интерпретацию последующих опов):
//!    units/path-переключения, G92/L20 вне пресетки, machine-frame без
//!    in-text origins (§5.3), сломанные дуги (§6), PEND-инвариант, поток
//!    управления, `#`-параметры, O-code, лимиты;
//!  * Ok + диагностика (line-level RAW) — локальные конструкции: вендорские
//!    M/G (SEMANTICS/SYNTAX), G43 H (REQUIRES), G28/G30 (REQUIRES),
//!    одиночное T (предвыбор, SEMANTICS), block delete (RUNTIME_STATE)…
//!    Диагностики Ok-пути буферизуются: `ParseOut::diagnostics`
//!    (LinuxCncAdapter::take_diagnostics).
//!
//! Интерпретации итерации 2 (зафиксированы в отчёте):
//!  * развёртывание canned cycles (G73/G81/G82/G83/G85/G86/G89) — итерация 3;
//!    встреча цикла сейчас → жёсткий Err E_UNSUPPORTED_SEMANTICS (СТОП-вопрос
//!    отчёта; векторы N4/N6, чьи файлы содержат циклы, отложены);
//!  * refuse-циклы (G74/G84/G87/G88) и вне-цикловые отказные коды (G76/G33/
//!    G38/G5.x/G96/G97/G7/G8/G92.1–.3/M19/…) — line-level RAW по матрице
//!    COVERAGE (N8 проходит без цикл-парсера);
//!  * M98/M99/O-code → жёсткий Err E_UNSUPPORTED_SYNTAX (SPEC §1.1; brief
//!    итерации 2; CORPUS §1.2 перечисляет N11 как Ok-класс — расхождение
//!    заведено в отчёт как СТОП-вопрос);
//!  * G10 L2 honors G90/G91 (старый origin None считается 0); L20 — всегда
//!    относительная запись (delta = слова кадра, отсутствующие оси = 0);
//!  * G92: отсутствующие оси = 0; второй G92 или G92 после motion — Err;
//!  * R-слово дуги имеет приоритет над IJK при одновременном задании.

use crate::canonicalize::{self, num_snap, p3_snap, to_f64, ArcError, Plane};
use crate::lexer::{code_num, lex_program, word_num, LineData, LineTok, Word, MAX_OPS_PER_PROGRAM};
use eid_core::ast::{
    Dir, Frame, Num, Op, PathMode, Program, SrcSpan, StartPoint, State, Units, Wcs,
};
use eid_core::diag::{Diagnostic, ErrorCode, Stage};

/// Локальные kind'и notice'ов канонизатора LinuxCNC (зеркально eid-grbl).
/// MissingProgramEnd — SPEC §3 v0.16: отсутствие M2/M30 = неявный конец
/// (ProgramEnd добавлен последним опом + notice), не отказ; 18+/32 Tier B —
/// реальные посты не пишут program-end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// rapid/linear: start == end (SPEC §3).
    ZeroLengthMotion,
    /// arc: start == end при sweep != 360° и pitch == 0 (SPEC §3/§6; #13).
    ZeroLengthArc,
    /// dwell: seconds == 0 (SPEC §5.2).
    ZeroDwell,
    /// Программа без M2/M30: ProgramEnd добавлен автоматически последним.
    MissingProgramEnd,
}

/// Отброшенный/добавленный оп + контекст notice'а.
#[derive(Debug, Clone)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
    /// Индекс опа в списке ops (для MissingProgramEnd — индекс добавленного).
    pub op_index: usize,
    pub op: Op,
}
use eid_core::ast::{Coolant, Spindle};

/// eid_version = версия grammar.ebnf (SPEC §10.4).
pub const EID_VERSION: &str = "EID/L1 v0.3";  // grammar v0.3 (magic bump)

/// Публичный результат parse: Program + line-level диагностики + notices.
#[derive(Debug, Clone)]
pub struct ParseOut {
    pub program: Program,
    /// Line-level диагностики (Ok-путь, правило двух классов CORPUS §1.2).
    pub diagnostics: Vec<Diagnostic>,
    /// Drop-with-notice (zero-length motion / dwell == 0, SPEC §3/§5.2).
    pub notices: Vec<Notice>,
}

// ===========================================================================
// Модальные состояния парсера
// ===========================================================================

/// Циклы LinuxCNC, разворачиваемые в явные ходы (SPEC §7; COVERAGE §2.5).
/// Refuse-класс (G74/G84/G87/G88 → Raw SEMANTICS) и G76 (токарная резьба)
/// обрабатываются в raw-скане.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CycleKind {
    /// G81 — сверление: rapid XY → rapid R → feed Z → retract.
    Drill,
    /// G82 — сверление + dwell P на дне.
    DrillDwell,
    /// G83 — пекерование с полным отводом на R после каждого пика.
    Peck,
    /// G73 — chip-break: отвод на Q над последней глубиной.
    ChipBreak,
    /// G85 — расточка: feed-out (отвод на подаче).
    BoreFeedOut,
    /// G86 — расточка: dwell P?, шпиндель off, rapid out, рестарт шпинделя.
    BoreStop,
    /// G89 — расточка + dwell P: feed-out.
    BoreDwell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MotionKind {
    G0,
    G1,
    G2,
    G3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathKind {
    ExactPath,
    ExactStop,
    Blend,
}

/// Предыдущий motion — для связывания стартов (EQUIVALENCE §1.3).
#[derive(Debug, Clone, Copy)]
struct LastMotion {
    end: [f64; 3],
    frame: Frame,
    wcs: Wcs,
}

struct St {
    file: String,
    units: Option<Units>,
    path: Option<PathKind>,
    path_t: Option<Num>,
    distance_abs: bool,   // G90 (true) / G91
    arc_center_abs: bool, // G90.1 / G91.1 (дефолт LinuxCNC — G91.1)
    plane: Plane,
    feed_inverse: bool, // G93 / G94
    wcs: Wcs,
    feed: Option<Num>,
    last_s: Option<Num>,
    spindle: Spindle,
    coolant: Coolant,
    tool: Option<u32>,
    selected_tool: Option<u32>,
    motion: Option<MotionKind>,
    retract_g98: bool, // ABSORB циклов (правило отвода — итерация 3)
    origin: [Option<[f64; 3]>; 10], // G10 L2 (known_origin; L20 НЕ считается)
    l20: [Option<[f64; 3]>; 10],    // G10 L20 delta (якорь пресетки)
    preset: Option<eid_core::ast::FramePreset>,
    saw_motion: bool,
    last_motion: Option<LastMotion>,
    ops: Vec<Op>,
    diags: Vec<Diagnostic>,
    /// Drop-with-notice, собранные на ходе разбора (zero-sweep дуги, §10.6).
    notices: Vec<Notice>,
}

impl St {
    fn snapshot(&self, frame: Frame) -> State {
        State {
            feed: self.feed.clone(),
            spindle: self.spindle.clone(),
            coolant: self.coolant,
            tool: self.tool,
            frame,
            wcs: self.wcs,
        }
    }

    fn origin_of(&self, w: Wcs) -> Option<[f64; 3]> {
        self.origin[eid_core::parser::wcs_to_p(w) as usize]
    }

    fn l20_of(&self, w: Wcs) -> Option<[f64; 3]> {
        self.l20[eid_core::parser::wcs_to_p(w) as usize]
    }

    fn src(&self, line_no: usize, line: &str) -> SrcSpan {
        SrcSpan {
            file: self.file.clone(),
            line_start: line_no as u32,
            col_start: 1,
            line_end: line_no as u32,
            col_end: line.len().max(1) as u32,
        }
    }

    fn push_diag(&mut self, code: ErrorCode, msg: impl Into<String>, line_no: usize, line: &str) {
        self.diags.push(Diagnostic {
            stage: Stage::Parse,
            code,
            message: msg.into(),
            src: Some(self.src(line_no, line)),
            options: Vec::new(),
            limit: None,
        });
    }

    fn push_raw(
        &mut self,
        line_no: usize,
        line: &str,
        term: &str,
        reason: impl Into<String> + Clone,
        code: ErrorCode,
    ) {
        let reason = reason.into();
        // D9 v0.15: payload — байты строки ВКЛЮЧАЯ line terminator.
        let mut bytes = String::with_capacity(line.len() + term.len());
        bytes.push_str(line);
        bytes.push_str(term);
        let mut payload_hex = String::with_capacity(bytes.len() * 2);
        for b in bytes.as_bytes() {
            payload_hex.push_str(&format!("{b:02X}"));
        }
        self.ops.push(Op::Raw {
            payload_hex,
            reason: reason.clone(),
            code,
            src: self.src(line_no, line),
        });
        // Диагностика Ok-пути (CORPUS §1.2 v0.5: parse = Ok, diagnostics ⊇ {код}).
        self.push_diag(code, reason, line_no, line);
    }
}

fn fatal(file: &str, code: ErrorCode, msg: impl Into<String>, line_no: usize, line: &str) -> Vec<Diagnostic> {
    vec![Diagnostic {
        stage: Stage::Parse,
        code,
        message: msg.into(),
        src: Some(SrcSpan {
            file: file.to_string(),
            line_start: line_no as u32,
            col_start: 1,
            line_end: line_no as u32,
            col_end: line.len().max(1) as u32,
        }),
        options: Vec::new(),
        limit: None,
    }]
}

// ===========================================================================
// Точка входа
// ===========================================================================

pub fn parse_program(src: &str, file: &str) -> Result<ParseOut, Vec<Diagnostic>> {
    let lines = lex_program(src, file)?;

    // Правило семейств (COVERAGE §1.3): сканер отмечает семейство целиком
    // до диспозиций. G40 = {G41,G41.1,G42,G42.1}; G49 = {G43,G43.1,G43.2}.
    let mut family_g40 = false;
    let mut family_g49 = false;
    for ld in &lines {
        if let LineTok::Words(words) = &ld.tok {
            for w in words {
                if w.letter == 'G' {
                    if let Some((maj, min)) = code_num(&w.number) {
                        if matches!((maj, min), (41, _) | (42, _)) {
                            family_g40 = true;
                        }
                        if matches!((maj, min), (43, _)) {
                            family_g49 = true;
                        }
                    }
                }
            }
        }
    }

    let mut st = St {
        file: file.to_string(),
        units: None,
        path: None,
        path_t: None,
        distance_abs: true,    // G90 — дефолт контроллера (COVERAGE §0.3)
        arc_center_abs: false, // G91.1 — дефолт LinuxCNC
        plane: Plane::G17,
        feed_inverse: false,
        wcs: Wcs::G54,
        feed: None,
        last_s: None,
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        tool: None,
        selected_tool: None,
        motion: None,
        retract_g98: false,
        origin: Default::default(),
        l20: Default::default(),
        preset: None,
        saw_motion: false,
        last_motion: None,
        ops: Vec::new(),
        diags: Vec::new(),
        notices: Vec::new(),
    };

    for ld in &lines {
        match &ld.tok {
            LineTok::Blank | LineTok::Percent => {}
            LineTok::Unknown => {
                // Неизвестная/чужая строка (напр. `$$`-конфиг) — line-level
                // RAW{E_UNSUPPORTED_SYNTAX} (CORPUS #47; правило двух классов).
                st.push_raw(
                    ld.line_no,
                    &ld.raw,
                    ld.term,
                    "неизвестная строка — вне словаря LinuxCNC (line-local)",
                    ErrorCode::UnsupportedSyntax,
                );
            }
            LineTok::Slash => {
                // Block delete: исполнение зависит от переключателя станка —
                // не статично (COVERAGE §4) → line-level RAW.
                st.push_raw(
                    ld.line_no,
                    &ld.raw,
                    ld.term,
                    "block delete `/` — исполнение зависит от переключателя станка",
                    ErrorCode::RuntimeState,
                );
            }
            LineTok::Words(words) => parse_block(&mut st, ld, words, family_g40, family_g49)?,
        }
    }

    finish(st)
}

// ===========================================================================
// Разбор кадра
// ===========================================================================

#[allow(clippy::too_many_lines)]
fn parse_block(
    st: &mut St,
    ld: &LineData,
    words: &[Word],
    family_g40: bool,
    family_g49: bool,
) -> Result<(), Vec<Diagnostic>> {
    let (line_no, line) = (ld.line_no, ld.raw.as_str());
    macro_rules! fl {
        ($code:expr, $msg:expr $(,)?) => {
            fatal(&st.file, $code, $msg, line_no, line)
        };
    }

    // --- сбор слов кадра ----------------------------------------------------
    let mut gs: Vec<(i64, u32)> = Vec::new();
    let mut ms: Vec<(i64, u32)> = Vec::new();
    let mut f_word: Option<Num> = None;
    let mut s_word: Option<Num> = None;
    let mut t_word: Option<u32> = None;
    let mut p_word: Option<Num> = None;
    let mut l_word: Option<u32> = None;
    let mut q_word: Option<f64> = None;
    let mut r_word: Option<f64> = None;
    let mut ijk: [Option<f64>; 3] = [None; 3];
    let mut axes: [Option<(f64, String)>; 3] = [const { None }; 3]; // (f64-зеркало, verbatim)
    let mut extra_axes: Vec<char> = Vec::new(); // A/B/C/U/V/W
    let mut has_t = false;
    let mut has_m6 = false;

    // Модальные группы: конфликт (два разных major одной группы) → InvalidInput.
    let mut seen: Vec<(&'static str, i64)> = Vec::new();
    let group_of = |maj: i64, min: u32| -> Option<(&'static str, i64)> {
        let minor = if min == 0 { 0 } else { min };
        match (maj, minor) {
            (m, 0) if matches!(m, 0 | 1 | 2 | 3 | 73 | 74 | 76 | 81..=89) => Some(("motion", m)),
            (17, 0) | (18, 0) | (19, 0) => Some(("plane", maj)),
            (90, 0) | (91, 0) => Some(("distance", maj)),
            (90, 1) | (91, 1) => Some(("arcdist", maj)),
            (93, 0) | (94, 0) => Some(("feedmode", maj)),
            (20, 0) | (21, 0) => Some(("units", maj)),
            (54, 0) | (55, 0) | (56, 0) | (57, 0) | (58, 0) | (59, 0) | (59, 1) | (59, 2)
            | (59, 3) => Some(("wcs", maj)),
            (61, 0) | (61, 1) | (64, 0) => Some(("path", maj)),
            (98, 0) | (99, 0) => Some(("retract", maj)),
            _ => None,
        }
    };

    for w in words {
        match w.letter {
            'G' => {
                let (maj, min) = code_num(&w.number)
                    .ok_or_else(|| fl!(ErrorCode::InvalidInput, format!("bad G-code `G{}`", w.number)))?;
                if let Some((name, m)) = group_of(maj, min) {
                    if let Some(&(_, prev)) = seen.iter().find(|(n, _)| *n == name) {
                        if prev != m {
                            return Err(fl!(
                                ErrorCode::InvalidInput,
                                format!("конфликт модальной группы `{name}` в кадре: G{} и G{}", prev, m),
                            ));
                        }
                    } else {
                        seen.push((name, m));
                    }
                }
                let minor = if min == 0 { 0 } else { min };
                gs.push((maj, minor));
            }
            'M' => {
                let (maj, min) = code_num(&w.number)
                    .ok_or_else(|| fl!(ErrorCode::InvalidInput, format!("bad M-code `M{}`", w.number)))?;
                let minor = if min == 0 { 0 } else { min };
                ms.push((maj, minor));
                if maj == 6 {
                    has_m6 = true;
                }
            }
            'F' => {
                let v = word_num(&w.number)
                    .ok_or_else(|| fl!(ErrorCode::InvalidInput, format!("bad F value `{}`", w.number)))?;
                f_word = Some(Num::new(w.number.clone(), v));
            }
            'S' => {
                let v = word_num(&w.number)
                    .ok_or_else(|| fl!(ErrorCode::InvalidInput, format!("bad S value `{}`", w.number)))?;
                s_word = Some(Num::new(w.number.clone(), v));
            }
            'T' => {
                let (maj, min) = code_num(&w.number)
                    .ok_or_else(|| fl!(ErrorCode::InvalidInput, format!("bad T value `{}`", w.number)))?;
                if min != 0 || maj < 0 {
                    return Err(fl!(
                        ErrorCode::InvalidInput,
                        format!("T должен быть целым неотрицательным, получено `T{}`", w.number),
                    ));
                }
                t_word = Some(maj as u32);
                has_t = true;
            }
            'P' => {
                let v = word_num(&w.number)
                    .ok_or_else(|| fl!(ErrorCode::InvalidInput, format!("bad P value `{}`", w.number)))?;
                p_word = Some(Num::new(w.number.clone(), v));
            }
            'L' => {
                let (maj, min) = code_num(&w.number)
                    .ok_or_else(|| fl!(ErrorCode::InvalidInput, format!("bad L value `{}`", w.number)))?;
                if min != 0 || maj < 0 {
                    return Err(fl!(
                        ErrorCode::InvalidInput,
                        format!("L должен быть целым неотрицательным, получено `L{}`", w.number),
                    ));
                }
                l_word = Some(maj as u32);
            }
            'Q' => {
                q_word = Some(word_num(&w.number)
                    .ok_or_else(|| fl!(ErrorCode::InvalidInput, format!("bad Q value `{}`", w.number)))?);
            }
            'R' => {
                r_word = Some(word_num(&w.number)
                    .ok_or_else(|| fl!(ErrorCode::InvalidInput, format!("bad R value `{}`", w.number)))?);
            }
            'I' | 'J' | 'K' => {
                let idx = match w.letter {
                    'I' => 0,
                    'J' => 1,
                    _ => 2,
                };
                ijk[idx] = Some(word_num(&w.number).ok_or_else(|| {
                    fl!(ErrorCode::InvalidInput, format!("bad {} value `{}`", w.letter, w.number))
                })?);
            }
            'X' | 'Y' | 'Z' => {
                let idx = match w.letter {
                    'X' => 0,
                    'Y' => 1,
                    _ => 2,
                };
                let v = word_num(&w.number)
                    .ok_or_else(|| fl!(ErrorCode::InvalidInput, format!("bad {} value `{}`", w.letter, w.number)))?;
                axes[idx] = Some((v, w.number.clone()));
            }
            'A' | 'B' | 'C' | 'U' | 'V' | 'W' => extra_axes.push(w.letter),
            // H/D — параметры G43*/G41* (кадр уходит в line-level raw целиком);
            // значения сохраняются для полноты лексики, диспозицией несут кадры.
            'H' | 'D' => {}
            'N' => {} // IGNORED_SAFE (COVERAGE §4)
            other => {
                return Err(fl!(
                    ErrorCode::InvalidInput,
                    format!("необработанная буква слова `{other}`"),
                ))
            }
        }
    }

    // --- 1. Жёсткие структурные коды (до line-level raw) --------------------
    // M98/M99 (ruling итерации 6, Phase 1.2): line-local → line-level Raw
    // (см. raw-скан ниже), НЕ структурный Err.
    // Развёртываемые циклы (SPEC §7) — диспетчеризация в цикловом блоке ниже
    // (после сбора слов и модальных обновлений); refuse-циклы (G74/G84/G87/
    // G88) и G76 остаются line-level raw (COVERAGE §2.5).

    // --- 2. Line-level raw (весь кадр → один RAW-узел, payload = строка) ----
    // Локальные конструкции: не искажают интерпретацию последующих опов
    // (правило двух классов, CORPUS §1.2 v0.5; SPEC §8.5).
    let mut raw_reason: Option<(String, ErrorCode)> = None;
    let gname = |maj: i64, min: u32| -> String {
        if min == 0 { format!("G{maj}") } else { format!("G{maj}.{min}") }
    };
    for &(maj, min) in &gs {
        let found = match (maj, min) {
            (5, _) | (33, _) => Some((ErrorCode::UnsupportedSemantics, "семантика не представима ходами (COVERAGE §2.1)")),
            (7, 0) | (8, 0) => Some((ErrorCode::UnsupportedSemantics, "диаметральный режим — токарное, вне скоупа (COVERAGE §2.6)")),
            (28, _) | (30, _) => Some((ErrorCode::RequiresMachineTable, "home-позиция из таблицы станка (COVERAGE §2.3, Q1)")),
            (38, 2) | (38, 3) | (38, 4) | (38, 5) => Some((ErrorCode::UnsupportedSemantics, "пробинг — этап 12 (COVERAGE §2.1)")),
            (40, 0) if family_g40 => Some((ErrorCode::UnsupportedSemantics, "G40 при активном семействе компенсаций — правило семейств (COVERAGE §1.3)")),
            (41, _) | (42, _) => Some((ErrorCode::UnsupportedSemantics, "компенсация геометрии — не-цель §1.2 (COVERAGE §2.4)")),
            (43, 0) | (43, 2) => Some((ErrorCode::RequiresMachineTable, "TLO из таблицы станка (COVERAGE §2.4, N10)")),
            (43, 1) => Some((ErrorCode::UnsupportedSemantics, "динамический TLO (COVERAGE §2.4)")),
            (49, 0) if family_g49 => Some((ErrorCode::RequiresMachineTable, "G49 при активном семействе TLO — правило семейств (COVERAGE §1.3)")),
            (74, 0) | (84, 0) | (87, 0) | (88, 0) => Some((ErrorCode::UnsupportedSemantics, "refuse-цикл: tapping / undefined behavior (COVERAGE §2.5)")),
            (76, 0) => Some((ErrorCode::UnsupportedSemantics, "G76 — токарный цикл резьбы, вне скоупа §1.2 (COVERAGE §2.5)")),
            (92, 1) | (92, 2) | (92, 3) => Some((ErrorCode::UnsupportedSemantics, "сброс/приостановка G92-сдвигов (Q4, COVERAGE §2.3)")),
            (96, _) | (97, _) => Some((ErrorCode::UnsupportedSemantics, "CSS — токарное, вне скоупа §1.2 (COVERAGE §2.6, N18)")),
            _ => None,
        };
        // G40/G49 без активного семейства — IGNORED_SAFE (молча, SPEC §8).
        if let Some((c, r)) = found {
            raw_reason = Some((format!("{}: {r}", gname(maj, min)), c));
            break;
        }
    }
    // G10 с L∉{2,20} → REQUIRES (COVERAGE §2.3). G10 без L — E_INVALID_INPUT
    // (см. поток G10 ниже).
    if raw_reason.is_none()
        && gs.iter().any(|&(maj, min)| (maj, min) == (10, 0))
        && matches!(l_word, Some(l) if l != 2 && l != 20)
    {
        raw_reason = Some((
            "G10 L1/…: запись таблиц инструментов (COVERAGE §2.3)".into(),
            ErrorCode::RequiresMachineTable,
        ));
    }
    if raw_reason.is_none() {
        for &(maj, min) in &ms {
            let found = match (maj, min) {
                (19, 0) => Some((ErrorCode::UnsupportedSemantics, "ориентация шпинделя (COVERAGE §3)")),
                (48, 0) | (49, 0) | (50, 0) | (51, 0) => Some((ErrorCode::UnsupportedSemantics, "override — process intent (COVERAGE §3, ревью §7.4)")),
                (52, 0) | (53, 0) => Some((ErrorCode::RuntimeState, "adaptive feed / feed-hold — вход HAL в рантайме (COVERAGE §3)")),
                (60, 0) => Some((ErrorCode::UnsupportedSemantics, "pallet change (COVERAGE §3)")),
                (61, 0) => Some((ErrorCode::UnsupportedSemantics, "номер инструмента без физической смены (COVERAGE §3, ревью §7.5)")),
                (62, 0) | (63, 0) | (64, 0) | (65, 0) => Some((ErrorCode::UnsupportedSemantics, "цифровые выходы (COVERAGE §3)")),
                (66, 0) => Some((ErrorCode::RuntimeState, "ожидание входа (COVERAGE §3)")),
                (67, 0) | (68, 0) => Some((ErrorCode::UnsupportedSemantics, "аналоговые выходы (COVERAGE §3)")),
                (70, 0) | (71, 0) | (72, 0) | (73, 0) => Some((ErrorCode::UnsupportedSemantics, "стек модального состояния (COVERAGE §3)")),
                (m, 0) if (100..=199).contains(&m) => Some((ErrorCode::UnsupportedSemantics, "user-defined M100–M199 (COVERAGE §3, N3)")),
                _ => None,
            };
            if let Some((c, r)) = found {
                raw_reason = Some((format!("M{maj}: {r}"), c));
                break;
            }
        }
    }
    if raw_reason.is_none() {
        // Коды вне словаря → SYNTAX (COVERAGE §1.3: RAW(E_UNSUPPORTED_SYNTAX)).
        for &(maj, min) in &gs {
            if !is_known_g(maj, min) {
                raw_reason = Some((format!("{}: вне словаря L1 (COVERAGE §2)", gname(maj, min)), ErrorCode::UnsupportedSyntax));
                break;
            }
        }
    }
    if raw_reason.is_none() {
        for &(maj, _min) in &ms {
            if !is_known_m(maj) {
                raw_reason = Some((format!("M{maj}: вне словаря (COVERAGE §3)"), ErrorCode::UnsupportedSyntax));
                break;
            }
        }
    }
    if raw_reason.is_none() && !extra_axes.is_empty() {
        raw_reason = Some((
            format!(
                "вне-скоуп оси {} — этап 12 (COVERAGE §4)",
                extra_axes.iter().collect::<String>()
            ),
            ErrorCode::UnsupportedSemantics,
        ));
    }
    if raw_reason.is_none() && has_t && !has_m6 {
        // Одиночное T — легальный предвыбор в LinuxCNC, вне L1 (COVERAGE §3,
        // SPEC §4; исправлено ревью — не E_INVALID_INPUT). N9.
        raw_reason = Some((
            "одиночное T — предвыбор инструмента, вне L1 (COVERAGE §3)".into(),
            ErrorCode::UnsupportedSemantics,
        ));
    }
    if let Some((reason, code)) = raw_reason {
        if let Some(t) = t_word {
            st.selected_tool = Some(t); // предвыбор сохраняется для последующего M6
        }
        st.push_raw(line_no, line, ld.term, reason, code);
        return Ok(());
    }

    // --- 3. Состояние кадра (SPEC §4, resolved-state) -----------------------
    if let Some(f) = f_word.clone() {
        st.feed = Some(f);
    }
    if let Some(s) = s_word.clone() {
        match st.spindle {
            Spindle::Off => st.last_s = Some(s),
            Spindle::Cw(_) => st.spindle = Spindle::Cw(s),
            Spindle::Ccw(_) => st.spindle = Spindle::Ccw(s),
        }
    }
    for &(maj, _min) in &ms {
        match maj {
            3 | 4 => {
                let n = match &s_word {
                    Some(n) => n.clone(),
                    None => st.last_s.clone().unwrap_or_else(|| num_snap(0.0)),
                };
                st.spindle = if maj == 3 { Spindle::Cw(n) } else { Spindle::Ccw(n) };
            }
            5 => st.spindle = Spindle::Off,
            _ => {}
        }
    }
    for &(maj, _min) in &ms {
        match maj {
            7 => st.coolant.mist = true,
            8 => st.coolant.flood = true,
            9 => st.coolant = Coolant::default(),
            _ => {}
        }
    }
    for &(maj, min) in &gs {
        if min == 0 {
            match maj {
                17 => st.plane = Plane::G17,
                18 => st.plane = Plane::G18,
                19 => st.plane = Plane::G19,
                90 => st.distance_abs = true,
                91 => st.distance_abs = false,
                93 => st.feed_inverse = true,
                94 => st.feed_inverse = false,
                98 => st.retract_g98 = true,
                99 => st.retract_g98 = false,
                80 => st.motion = None, // G80: голые координаты после —
                // E_INVALID_INPUT (COVERAGE §2.1, N6)
                _ => {}
            }
        } else {
            match (maj, min) {
                (90, 1) => st.arc_center_abs = true,  // ABSORB (COVERAGE §2.2)
                (91, 1) => st.arc_center_abs = false, // оба режима, канон §6
                _ => {}
            }
        }
    }

    // Units (G20/G21): одна система на программу (SPEC §2.5) — N7.
    for &(maj, min) in &gs {
        if min == 0 && (maj == 20 || maj == 21) {
            let u = if maj == 20 { Units::In } else { Units::Mm };
            match st.units {
                None => st.units = Some(u),
                Some(old) if old != u => {
                    return Err(fl!(
                        ErrorCode::UnsupportedSemantics,
                        "переключение единиц в середине программы — одна система единиц (SPEC §2.5/§3, N7)",
                    ));
                }
                _ => {} // повтор той же системы — легальный no-op
            }
        }
    }

    // Path mode (G61/G61.1/G64): один на программу (SPEC §3) — mid-switch Err.
    for &(maj, min) in &gs {
        let kind = match (maj, min) {
            (61, 0) => Some(PathKind::ExactPath),
            (61, 1) => Some(PathKind::ExactStop),
            (64, 0) => Some(PathKind::Blend),
            _ => None,
        };
        if let Some(k) = kind {
            match st.path {
                None => {
                    st.path = Some(k);
                    if k == PathKind::Blend {
                        st.path_t = p_word.clone();
                        if let Some(q) = q_word {
                            // Q = Naive CAM Detector — теряется с warning
                            // (COVERAGE §2.2); warning — line-level диагностика.
                            st.push_diag(
                                ErrorCode::UnsupportedSemantics,
                                format!("G64 Q{q} (Naive CAM Detector) потерян — warning (COVERAGE §2.2)"),
                                line_no,
                                line,
                            );
                        }
                    }
                }
                Some(old) if old != k => {
                    return Err(fl!(
                        ErrorCode::UnsupportedSemantics,
                        "переключение path mode в середине программы — один режим (SPEC §3)",
                    ));
                }
                _ => {}
            }
        }
    }

    // WCS выбор (G54–G59.3): поле состояния (SPEC §5.3 — выбор, не запись).
    for &(maj, min) in &gs {
        if let Some(w) = g_to_wcs(maj, min) {
            st.wcs = w;
        }
    }

    // --- 3.5 Циклы LinuxCNC (SPEC §7): разворачивание в явные ходы ----------
    let mut pending: Vec<Op> = Vec::new();
    let cycle_kind = gs.iter().find_map(|&(maj, min)| match (maj, min) {
        (81, 0) => Some(CycleKind::Drill),
        (82, 0) => Some(CycleKind::DrillDwell),
        (83, 0) => Some(CycleKind::Peck),
        (73, 0) => Some(CycleKind::ChipBreak),
        (85, 0) => Some(CycleKind::BoreFeedOut),
        (86, 0) => Some(CycleKind::BoreStop),
        (89, 0) => Some(CycleKind::BoreDwell),
        _ => None,
    });
    if let Some(kind) = cycle_kind {
        expand_cycle(
            st,
            kind,
            &axes,
            r_word,
            p_word.clone(),
            l_word,
            q_word,
            line_no,
            line,
            &mut pending,
        )?;
    }

    // --- 4. Опы кадра (порядок LinuxCNC: T/M6 → dwell → stops → frame →
    //        motion → end) ------------------------------------------------------

    // T..M6 → ToolChange (SPEC §5.2).
    if has_m6 {
        let tool = match (t_word, st.selected_tool) {
            (Some(t), _) => t,
            (None, Some(t)) => t,
            (None, None) => {
                return Err(fl!(
                    ErrorCode::InvalidInput,
                    "M6 без T-слова (ни в кадре, ни предвыбором)",
                ));
            }
        };
        st.selected_tool = Some(tool);
        st.tool = Some(tool);
        let state = st.snapshot(Frame::Wcs);
        let src = st.src(line_no, line);
        pending.push(Op::ToolChange { tool, state, src });
    }

    // G4 → Dwell (P секунды; SPEC §5.2).
    if gs.iter().any(|&(maj, min)| (maj, min) == (4, 0)) {
        let p = p_word
            .clone()
            .ok_or_else(|| fl!(ErrorCode::InvalidInput, "G4 без P-слова (секунды)"))?;
        if p.value < 0.0 {
            return Err(fl!(
                ErrorCode::InvalidInput,
                format!("G4 P{} < 0 (SPEC §5.2)", p.text),
            ));
        }
        let state = st.snapshot(Frame::Wcs);
        let src = st.src(line_no, line);
        pending.push(Op::Dwell { seconds: p, state, src, sub: None });
    }

    // M0/M1 → ProgramStop.
    for &(maj, _min) in &ms {
        if maj == 0 || maj == 1 {
            let state = st.snapshot(Frame::Wcs);
            let src = st.src(line_no, line);
            pending.push(Op::ProgramStop { optional: maj == 1, state, src });
        }
    }

    // G92 / G10 L2 / G10 L20 (SPEC §3/§5.3).
    for &(maj, min) in &gs {
        if (maj, min) == (92, 0) {
            if st.preset.is_some() {
                return Err(fl!(
                    ErrorCode::UnsupportedSemantics,
                    "второй G92 — множественные пресетки (Q5, SPEC v0.7; N17)",
                ));
            }
            if st.saw_motion {
                return Err(fl!(
                    ErrorCode::UnsupportedSemantics,
                    "G92 после motion-опа — пресетка только до первого движения (SPEC §3, Q4/Q5; N2)",
                ));
            }
            let values = axes_from_words(&axes, &[0.0; 3]);
            st.preset = Some(eid_core::ast::FramePreset { values: p3_snap(values), wcs: st.wcs });
        }
        if (maj, min) == (10, 0) {
            let l = l_word.ok_or_else(|| fl!(ErrorCode::InvalidInput, "G10 без L-слова"))?;
            let p = p_word.as_ref().map(|n| n.value as u32);
            match l {
                2 => {
                    let p = p.ok_or_else(|| fl!(ErrorCode::InvalidInput, "G10 L2 без P-слова (1..9)"))?;
                    if !(1..=9).contains(&p) {
                        return Err(fl!(
                            ErrorCode::InvalidInput,
                            format!("G10 L2 P{p} вне домена 1..9 (D7)"),
                        ));
                    }
                    // L2 honors G90/G91; старый origin отсутствует → 0
                    // (интерпретация, отчёт итерации 2).
                    let old = st.origin[p as usize].unwrap_or([0.0; 3]);
                    let origin = if st.distance_abs {
                        axes_from_words(&axes, &old)
                    } else {
                        add_axes(&old, &axes)
                    };
                    st.origin[p as usize] = Some(origin);
                    let state = st.snapshot(Frame::Wcs);
                    let src = st.src(line_no, line);
                    pending.push(Op::WcsDefine { p: p as u8, origin: p3_snap(origin), state, src });
                }
                20 => {
                    let p = p.ok_or_else(|| fl!(ErrorCode::InvalidInput, "G10 L20 без P-слова (1..9)"))?;
                    if !(1..=9).contains(&p) {
                        return Err(fl!(
                            ErrorCode::InvalidInput,
                            format!("G10 L20 P{p} вне домена 1..9 (D7)"),
                        ));
                    }
                    if st.saw_motion {
                        return Err(fl!(
                            ErrorCode::UnsupportedSemantics,
                            "G10 L20 после motion-опа — позиционно-зависим, только пресетка (SPEC §5.3; N15)",
                        ));
                    }
                    let delta = axes_from_words(&axes, &[0.0; 3]);
                    st.l20[p as usize] = Some(delta);
                    let state = st.snapshot(Frame::Wcs);
                    let src = st.src(line_no, line);
                    pending.push(Op::WcsSetHere { p: p as u8, delta: p3_snap(delta), state, src });
                }
                _ => unreachable!("G10 L not in {{2,20}} ушёл в line-level raw"),
            }
        }
    }

    // Motion.
    let g53 = gs.iter().any(|&(maj, min)| (maj, min) == (53, 0));
    let mut explicit_motion: Option<MotionKind> = None;
    for &(maj, min) in &gs {
        if min == 0 {
            match maj {
                0 => explicit_motion = Some(MotionKind::G0),
                1 => explicit_motion = Some(MotionKind::G1),
                2 => explicit_motion = Some(MotionKind::G2),
                3 => explicit_motion = Some(MotionKind::G3),
                _ => {}
            }
        }
    }
    if g53 && !matches!(explicit_motion, Some(MotionKind::G0) | Some(MotionKind::G1)) {
        return Err(fl!(
            ErrorCode::InvalidInput,
            "G53 требует явного G0/G1 в кадре (machine-frame ходы — прямые)",
        ));
    }
    if let Some(k) = explicit_motion {
        st.motion = Some(k);
    }

    let has_axis_words = axes.iter().any(|a| a.is_some());
    // Оси, поглощённые G92/G10/циклом — не «голые координаты».
    let frame_took_axes = gs
        .iter()
        .any(|&(maj, min)| {
            (maj, min) == (92, 0)
                || (maj, min) == (10, 0)
                || matches!(
                    (maj, min),
                    (73, 0) | (81, 0) | (82, 0) | (83, 0) | (85, 0) | (86, 0) | (89, 0)
                )
        });
    // Полный круг center-формой (SPEC §6 v0.12, CORPUS #08): явный G2/G3 с
    // I/J/K или R БЕЗ слов осей — end наследует старт → sweep = 360°.
    let explicit_arc_with_data = matches!(explicit_motion, Some(MotionKind::G2) | Some(MotionKind::G3))
        && (ijk.iter().any(|o| o.is_some()) || r_word.is_some());
    let motion_block = (has_axis_words || explicit_arc_with_data) && !frame_took_axes;
    if motion_block {
        match st.motion {
            Some(kind) => match build_motion(st, kind, g53, &axes, ijk, r_word, line_no, line)? {
                MotionBuilt::Op(op) => pending.push(op),
                MotionBuilt::Dropped(op, kind_, msg) => {
                    // Zero-sweep дуга (§10.6 v0.12, CORPUS #13): op не попадает
                    // в список; notice с записью отброшенного.
                    let idx = (st.ops.len() + pending.len()) as u32;
                    st.notices.push(Notice {
                        kind: kind_,
                        message: msg,
                        op_index: idx as usize,
                        op,
                    });
                }
            },
            None => {
                return Err(fl!(
                    ErrorCode::InvalidInput,
                    "голые координаты без motion-кода (модальная группа очищена G80 или не задана — COVERAGE §2.1, N6)"
                ));
            }
        }
    }

    // M2/M30 → ProgramEnd (SPEC §5.2; инвариант «ровно один раз и последний»
    // проверяется в finish()).
    if ms.iter().any(|&(maj, _)| maj == 2 || maj == 30) {
        let src = st.src(line_no, line);
        pending.push(Op::ProgramEnd { src });
    }

    // --- 5. Неиспользованные слова → сломанный вход -------------------------
    let has_g = |maj: i64, min: u32| gs.iter().any(|&(m, mi)| (m, mi) == (maj, min));
    let is_arc = matches!(st.motion, Some(MotionKind::G2) | Some(MotionKind::G3))
        && (has_axis_words || explicit_arc_with_data)
        && !frame_took_axes;
    // Циклы потребляют R/Z/L/P(dwell)/Q(пеки).
    let cycle_takes_p = matches!(cycle_kind, Some(CycleKind::DrillDwell | CycleKind::BoreStop | CycleKind::BoreDwell));
    let cycle_takes_q = matches!(cycle_kind, Some(CycleKind::Peck | CycleKind::ChipBreak));
    let is_cycle = cycle_kind.is_some();
    if r_word.is_some() && !is_arc && !is_cycle {
        return Err(fl!(ErrorCode::InvalidInput, "R-слово без дуги G2/G3 или цикла"));
    }
    if ijk.iter().any(|o| o.is_some()) && !is_arc {
        return Err(fl!(ErrorCode::InvalidInput, "I/J/K без дуги G2/G3"));
    }
    if p_word.is_some() && !(has_g(4, 0) || has_g(10, 0) || has_g(64, 0) || cycle_takes_p) {
        return Err(fl!(ErrorCode::InvalidInput, "P-слово без потребителя в кадре (G4/G10/G64/цикл с dwell)"));
    }
    if q_word.is_some() && !cycle_takes_q {
        return Err(fl!(ErrorCode::InvalidInput, "Q-слово без пекерующего цикла (G73/G83)"));
    }
    if l_word.is_some() && !(has_g(10, 0) || is_cycle) {
        return Err(fl!(ErrorCode::InvalidInput, "L-слово без G10 или цикла"));
    }

    st.ops.append(&mut pending);
    if st.ops.len() > MAX_OPS_PER_PROGRAM {
        return Err(vec![crate::lexer::limit_diag(
            "max_ops_per_program",
            MAX_OPS_PER_PROGRAM as f64,
            st.ops.len() as f64,
            &st.file,
            line_no,
            1,
        )]);
    }
    Ok(())
}

fn axes_from_words(axes: &[Option<(f64, String)>; 3], base: &[f64; 3]) -> [f64; 3] {
    let mut out = *base;
    for i in 0..3 {
        if let Some((v, _)) = axes[i] {
            out[i] = v;
        }
    }
    out
}

fn add_axes(base: &[f64; 3], axes: &[Option<(f64, String)>; 3]) -> [f64; 3] {
    let mut out = *base;
    for i in 0..3 {
        if let Some((v, _)) = axes[i] {
            out[i] += v;
        }
    }
    out
}

fn g_to_wcs(maj: i64, min: u32) -> Option<Wcs> {
    match (maj, min) {
        (54, 0) => Some(Wcs::G54),
        (55, 0) => Some(Wcs::G55),
        (56, 0) => Some(Wcs::G56),
        (57, 0) => Some(Wcs::G57),
        (58, 0) => Some(Wcs::G58),
        (59, 0) => Some(Wcs::G59),
        (59, 1) => Some(Wcs::G59_1),
        (59, 2) => Some(Wcs::G59_2),
        (59, 3) => Some(Wcs::G59_3),
        _ => None,
    }
}

fn is_known_g(maj: i64, min: u32) -> bool {
    matches!(
        (maj, min),
        (0, 0) | (1, 0) | (2, 0) | (3, 0)
            | (4, 0)
            | (5, 0) | (5, 1) | (5, 2) | (5, 3)
            | (7, 0) | (8, 0)
            | (10, 0)
            | (17, 0) | (18, 0) | (19, 0)
            | (20, 0) | (21, 0)
            | (28, 0) | (28, 1) | (30, 0) | (30, 1)
            | (33, 0) | (33, 1)
            | (38, 2) | (38, 3) | (38, 4) | (38, 5)
            | (40, 0) | (41, 0) | (41, 1) | (42, 0) | (42, 1)
            | (43, 0) | (43, 1) | (43, 2) | (49, 0)
            | (53, 0)
            | (54, 0) | (55, 0) | (56, 0) | (57, 0) | (58, 0) | (59, 0) | (59, 1) | (59, 2)
            | (59, 3)
            | (61, 0) | (61, 1) | (64, 0)
            | (73, 0) | (74, 0) | (76, 0)
            | (80, 0)
            | (81, 0) | (82, 0) | (83, 0) | (84, 0) | (85, 0) | (86, 0) | (87, 0) | (88, 0)
            | (89, 0)
            | (90, 0) | (90, 1) | (91, 0) | (91, 1)
            | (92, 0) | (92, 1) | (92, 2) | (92, 3)
            | (93, 0) | (94, 0)
            | (96, 0) | (97, 0)
            | (98, 0) | (99, 0)
    )
}

fn is_known_m(maj: i64) -> bool {
    (0..=9).contains(&maj) || maj == 19 || (48..=73).contains(&maj) || (100..=199).contains(&maj)
}

// ===========================================================================
// Motion: связывание стартов (Q11, SPEC §2 принцип 2 v0.10) + геометрия §6
// ===========================================================================

/// Результат связывания: Known(точка) | UnknownRequired (точка разрыва).
enum Linked {
    Known([f64; 3]),
    UnknownRequired,
}

fn link_start(
    st: &St,
    machine: bool,
    line_no: usize,
    line: &str,
) -> Result<Linked, Vec<Diagnostic>> {
    macro_rules! fl {
        ($code:expr, $msg:expr $(,)?) => {
            fatal(&st.file, $code, $msg, line_no, line)
        };
    }

    // Machine-frame (SPEC §5.3): (a) нет позиционно-относительных пресеток;
    // (b) origin активной WCS определён in-text (L20-происхождение не
    // считается — M-производное).
    if machine {
        if st.preset.is_some() {
            return Err(fl!(
                ErrorCode::RuntimeState,
                "machine-frame ход при активном frame_preset — SPEC §5.3(a) (N16)",
            ));
        }
        if st.l20.iter().any(|o| o.is_some()) {
            return Err(fl!(
                ErrorCode::RuntimeState,
                "machine-frame ход при активном wcs_set_here — SPEC §5.3(a)",
            ));
        }
        return Ok(match st.last_motion {
            Some(LastMotion { end, frame: Frame::Machine, .. }) => Linked::Known(end),
            Some(LastMotion { end, frame: Frame::Wcs, wcs }) => match st.origin_of(wcs) {
                Some(o) => Linked::Known([end[0] + o[0], end[1] + o[1], end[2] + o[2]]),
                None => {
                    return Err(fl!(
                        ErrorCode::RuntimeState,
                        "machine-frame ход: origin активной WCS не определён in-text (SPEC §5.3(b); N1)",
                    ));
                }
            },
            None => match st.origin_of(st.wcs) {
                // Первое движение в machine-frame: M₀ внешняя — точка разрыва
                // (c), SPEC §2 v0.10: start unknown легален (RAPID/LINEAR,
                // полный end); без origin — N1 («при выборе G54 без define»).
                Some(_) => Linked::UnknownRequired,
                None => {
                    return Err(fl!(
                        ErrorCode::RuntimeState,
                        "machine-frame ход: origin активной WCS не определён in-text (SPEC §5.3(b); N1)",
                    ));
                }
            },
        });
    }

    // WCS-движение.
    Ok(match st.last_motion {
        None => {
            // Точка разрыва (a): первый motion-оп — якоря пресетки.
            let sh = st.l20_of(st.wcs);
            match (sh, st.preset.as_ref()) {
                (Some(_), Some(_)) => Linked::UnknownRequired,
                (Some(delta), None) => Linked::Known(delta),
                (None, Some(pr)) => {
                    let pr_vals = to_f64(&pr.values);
                    if pr.wcs == st.wcs {
                        Linked::Known(pr_vals)
                    } else {
                        match (st.origin_of(pr.wcs), st.origin_of(st.wcs)) {
                            (Some(o0), Some(on)) => Linked::Known([
                                pr_vals[0] + o0[0] - on[0],
                                pr_vals[1] + o0[1] - on[1],
                                pr_vals[2] + o0[2] - on[2],
                            ]),
                            _ => Linked::UnknownRequired, // (b): разность не вычислима
                        }
                    }
                }
                (None, None) => Linked::UnknownRequired, // (a): без якоря
            }
        }
        Some(LastMotion { end, frame: Frame::Machine, .. }) => {
            // Точка разрыва (d): reading = M − O_wcs (Δ = 0 — пресетка была бы
            // отклонена на machine-ходе, §5.3(a)).
            match st.origin_of(st.wcs) {
                Some(on) => Linked::Known([end[0] - on[0], end[1] - on[1], end[2] - on[2]]),
                None => Linked::UnknownRequired,
            }
        }
        Some(LastMotion { end, frame: Frame::Wcs, wcs: prev_wcs }) => {
            if prev_wcs == st.wcs {
                Linked::Known(end)
            } else {
                // Точка разрыва (b): критерий разности origin'ов (SPEC §2
                // v0.10, вопрос A2) — нужны define ОБЕИХ систем.
                match (st.origin_of(prev_wcs), st.origin_of(st.wcs)) {
                    (Some(op_), Some(on)) => Linked::Known([
                        end[0] + op_[0] - on[0],
                        end[1] + op_[1] - on[1],
                        end[2] + op_[2] - on[2],
                    ]),
                    _ => Linked::UnknownRequired,
                }
            }
        }
    })
}

/// Построение motion-опа (вызывается только при наличии координатных слов).
#[allow(clippy::too_many_arguments)]
/// Итог построения motion-опа: оп либо zero-sweep drop (SPEC §10.6 v0.12).
enum MotionBuilt {
    Op(Op),
    Dropped(Op, NoticeKind, String),
}

/// Построение motion-опа (вызывается только при наличии координатных слов
/// либо явного G2/G3 с I/J/K/R — полный круг, SPEC §6 v0.12).
#[allow(clippy::too_many_arguments)]
fn build_motion(
    st: &mut St,
    kind: MotionKind,
    g53: bool,
    axes: &[Option<(f64, String)>; 3],
    ijk: [Option<f64>; 3],
    r_word: Option<f64>,
    line_no: usize,
    line: &str,
) -> Result<MotionBuilt, Vec<Diagnostic>> {
    let machine = g53;
    let frame = if machine { Frame::Machine } else { Frame::Wcs };
    macro_rules! fl {
        ($code:expr, $msg:expr $(,)?) => {
            fatal(&st.file, $code, $msg, line_no, line)
        };
    }

    // Старт = текущее чтение (связывание — обязанность канонизатора, SPEC §2
    // v0.9); для machine-ходов — текущая машинная позиция. Он же — база
    // наследования end (G90: отсутствующие оси; G91: все оси).
    let start = link_start(st, machine, line_no, line)?;
    let base = match &start {
        Linked::Known(b) => Some(*b),
        Linked::UnknownRequired => {
            // Первый motion-оп без якоря (точка разрыва (a)): start остаётся
            // Unknown (SPEC §2), но база наследования end — дефолт контроллера
            // (0,0,0) (golden #03: `G1 X10` при неизвестном старте даёт
            // end = (10,0,0); COVERAGE §0.3 — модальные значения на входе =
            // дефолты). Machine-frame наследовать не из чего — строго.
            if !machine && st.last_motion.is_none() {
                Some([0.0; 3])
            } else {
                None
            }
        }
    };
    let end: [f64; 3] = if st.distance_abs {
        match base {
            Some(b) => axes_from_words(axes, &b),
            None => {
                if axes.iter().all(|a| a.is_some()) {
                    axes_from_words(axes, &[0.0; 3])
                } else {
                    return Err(fl!(
                        ErrorCode::RuntimeState,
                        "частичный end наследует неизвестное (точка разрыва фрейма) — E_RUNTIME_STATE (SPEC §2)",
                    ));
                }
            }
        }
    } else {
        match base {
            Some(b) => add_axes(&b, axes),
            None => {
                return Err(fl!(
                    ErrorCode::RuntimeState,
                    "G91 требует известной текущей позиции (точка разрыва фрейма) — E_RUNTIME_STATE (SPEC §2)",
                ));
            }
        }
    };

    let start_point = match start {
        Linked::Known(v) => StartPoint::Known(p3_snap(v)),
        Linked::UnknownRequired => StartPoint::Unknown,
    };

    let op = match kind {
        MotionKind::G0 => {
            let state = st.snapshot(frame);
            let src = st.src(line_no, line);
            Op::Rapid { start: start_point, end: p3_snap(end), state, src, sub: None }
        }
        MotionKind::G1 => {
            let mut state = st.snapshot(frame);
            if st.feed_inverse {
                // G93: feed = len × F (SPEC §4); требует known start.
                let s = match (&start_point, base) {
                    (StartPoint::Known(_), Some(b)) => b,
                    _ => {
                        return Err(fl!(
                            ErrorCode::RuntimeState,
                            "G93-разрешение feed требует известного старта (SPEC §4)",
                        ));
                    }
                };
                let f = st
                    .feed
                    .as_ref()
                    .ok_or_else(|| fl!(ErrorCode::InvalidInput, "G93-linear без F"))?;
                let len = ((end[0] - s[0]).powi(2)
                    + (end[1] - s[1]).powi(2)
                    + (end[2] - s[2]).powi(2))
                .sqrt();
                state.feed = Some(num_snap(len * f.value));
            }
            let src = st.src(line_no, line);
            Op::Linear { start: start_point, end: p3_snap(end), state, src, sub: None }
        }
        MotionKind::G2 | MotionKind::G3 => {
            let start_v = match base {
                Some(b) => b,
                None => {
                    return Err(fl!(
                        ErrorCode::RuntimeState,
                        "дуга с start=unknown — геометрия требует числового старта (SPEC §2, Q11)",
                    ));
                }
            };
            if ijk.iter().all(|o| o.is_none()) && r_word.is_none() {
                return Err(fl!(
                    ErrorCode::InvalidInput,
                    "дуга без центра: ни I/J/K, ни R (LinuxCNC: no offsets in arc block)",
                ));
            }
            let dir = if kind == MotionKind::G2 { Dir::Cw } else { Dir::Ccw };
            let explicit_end = axes.iter().any(|a| a.is_some());
            let resolved = canonicalize::resolve_arc(
                &start_v,
                &end,
                st.plane,
                dir,
                st.arc_center_abs,
                ijk,
                r_word,
                eps_pos(&start_v, &end),
                explicit_end,
            )
            .map_err(|e| match e {
                ArcError::RFullCircle => fl!(
                    ErrorCode::InvalidInput,
                    "R-форма при start == end — центр неоднозначен (SPEC §6 v0.7; N19)",
                ),
                ArcError::ChordTooLong => fl!(
                    ErrorCode::InvalidInput,
                    "невозможная дуга: планиметрическая хорда > 2R (SPEC §6; N5b)",
                ),
                ArcError::RadiusMismatch => fl!(
                    ErrorCode::InvalidInput,
                    "несоответствие радиусов дуги: |start−center| ≠ |end−center| (SPEC §6.1; N5a)",
                ),
            })?;
            let resolved = match resolved {
                canonicalize::ArcOutcome::ZeroSweep => {
                    // §10.6 v0.12 (CORPUS #13): явный end, квантованный к
                    // старту, pitch == 0 — нулевая дуга → drop+notice.
                    let mut state = st.snapshot(frame);
                    if let Some(f) = &st.feed {
                        state.feed = Some(f.clone());
                    }
                    let (n_axis, _, _) = canonicalize::plane_def(st.plane);
                    let mut normal = [0.0; 3];
                    normal[n_axis] = 1.0;
                    let op = Op::Arc {
                        start: StartPoint::Known(p3_snap(start_v)),
                        end: p3_snap(end),
                        center: p3_snap(start_v), // центр вырожден — запись
                        normal: p3_snap(normal),
                        dir,
                        sweep_deg: num_snap(360.0),
                        pitch: num_snap(0.0),
                        state,
                        src: st.src(line_no, line),
                        sub: None,
                    };
                    let msg = "zero-length arc dropped (end quantizes to start, sweep != 360°, pitch == 0; SPEC §3/§6; CORPUS #13)".to_string();
                    st.last_motion = Some(LastMotion { end, frame, wcs: st.wcs });
                    st.saw_motion = true;
                    return Ok(MotionBuilt::Dropped(op, NoticeKind::ZeroLengthArc, msg));
                }
                canonicalize::ArcOutcome::Resolved(r) => r,
            };

            // G93: feed = len(траектории) × F; длина винтовой дуги —
            // sqrt((sweep·ρ)² + pitch²) (SPEC §4, §6).
            let mut state = st.snapshot(frame);
            if st.feed_inverse {
                let f = st
                    .feed
                    .as_ref()
                    .ok_or_else(|| fl!(ErrorCode::InvalidInput, "G93-дуга без F"))?;
                let sweep_rad = resolved.sweep_deg.to_radians();
                let len = ((sweep_rad * resolved.radius).powi(2) + resolved.pitch.powi(2)).sqrt();
                state.feed = Some(num_snap(len * f.value));
            }

            let (n_axis, _, _) = canonicalize::plane_def(st.plane);
            let mut normal = [0.0; 3];
            normal[n_axis] = 1.0;
            let src = st.src(line_no, line);
            Op::Arc {
                start: StartPoint::Known(p3_snap(start_v)),
                end: p3_snap(end),
                center: p3_snap(resolved.center),
                normal: p3_snap(normal),
                dir,
                sweep_deg: num_snap(resolved.sweep_deg),
                pitch: num_snap(resolved.pitch),
                state,
                src,
                sub: None,
            }
        }
    };

    // last_motion: end нового опа (связывание следующего, SPEC §2 v0.9).
    st.last_motion = Some(LastMotion { end, frame, wcs: st.wcs });
    st.saw_motion = true;
    Ok(MotionBuilt::Op(op))
}

/// ε_pos = max(0.001, 1e-7·L) (EQUIVALENCE §4); локальный масштаб по ходу.
fn eps_pos(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    let l = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    (0.001_f64).max(1e-7 * l)
}

// ===========================================================================
// Циклы LinuxCNC — разворачивание в явные ходы (SPEC §7, правила 1–6)
// ===========================================================================

/// Производный ход развёртки (SPEC §11): src исходного блока + sub=<i>.
/// §7.7: каждый ход несёт полный resolved-state (feed — модальный).
fn m_rapid(st: &St, from: [f64; 3], to: [f64; 3], src: SrcSpan, sub: u32) -> Op {
    Op::Rapid {
        start: StartPoint::Known(p3_snap(from)),
        end: p3_snap(to),
        state: st.snapshot(Frame::Wcs),
        src,
        sub: Some(sub),
    }
}

fn m_linear(st: &St, from: [f64; 3], to: [f64; 3], src: SrcSpan, sub: u32) -> Op {
    let mut state = st.snapshot(Frame::Wcs);
    if st.feed_inverse {
        // G93: feed = len(траектории) × F (SPEC §4).
        if let Some(f) = &st.feed {
            let len = ((to[0] - from[0]).powi(2)
                + (to[1] - from[1]).powi(2)
                + (to[2] - from[2]).powi(2))
            .sqrt();
            state.feed = Some(num_snap(len * f.value));
        }
    }
    Op::Linear {
        start: StartPoint::Known(p3_snap(from)),
        end: p3_snap(to),
        state,
        src,
        sub: Some(sub),
    }
}

fn m_dwell(st: &St, seconds: &Num, src: SrcSpan, sub: u32) -> Op {
    Op::Dwell {
        seconds: seconds.clone(),
        state: st.snapshot(Frame::Wcs),
        src,
        sub: Some(sub),
    }
}

/// Промежуточная высота подхода после полного отвода пека (LinuxCNC
/// gcode.html G83/G73: «rapid to a point 0.5 mm (0.05 inch) above the last
/// depth») — в единицах программы. Интерпретация отчёта итерации 3-4:
/// правило ОБЯЗАНО быть единым с eid-grbl (R3); константу сверить с
/// source LinuxCNC в COVERAGE-проходе.
fn approach_clearance(units: Units) -> f64 {
    if units == Units::In { 0.05 } else { 0.5 }
}

/// Разворачивание цикла (SPEC §7.1–7.6).
///
/// Последовательность на отверстие: rapid XY (на текущем z) → rapid на
/// R-плоскость → [тело цикла: feed на дно, dwell/пеки/шпиндель] → отвод
/// (§7.1: G99 → R; G98 → initial Z из предшествующих опов). Повторы L —
/// полная последовательность с абсолютными координатами (§7.2–7.3: G91 —
/// шаг X/Y/R/Z на повтор, LinuxCNC); порядок пиков точный (§7.4).
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn expand_cycle(
    st: &mut St,
    kind: CycleKind,
    axes: &[Option<(f64, String)>; 3],
    r_word: Option<f64>,
    p_word: Option<Num>,
    l_word: Option<u32>,
    q_word: Option<f64>,
    line_no: usize,
    line: &str,
    pending: &mut Vec<Op>,
) -> Result<(), Vec<Diagnostic>> {
    macro_rules! fl {
        ($code:expr, $msg:expr $(,)?) => {
            fatal(&st.file, $code, $msg, line_no, line)
        };
    }

    // §7.6: позиция до цикла должна быть определена предшествующими
    // motion-опами (пресетки-якоря не считаются — strict reading).
    if st.last_motion.is_none() {
        return Err(fl!(
            ErrorCode::RuntimeState,
            "цикл как первый motion-оп — позиция не определена предшествующими ходами (SPEC §7.6)"
        ));
    }
    let base = match link_start(st, false, line_no, line)? {
        Linked::Known(b) => b,
        Linked::UnknownRequired => {
            return Err(fl!(
                ErrorCode::RuntimeState,
                "цикл после позиционно-неопределённого участка — E_RUNTIME_STATE (SPEC §7.6)"
            ));
        }
    };

    // Лимит развёртки (COVERAGE §1.2) — раньше прочих проверок параметров:
    // guard транслятора имеет приоритет над диагностикой сломанных слов
    // (N4: L99999 → E_LIMIT_EXCEEDED).
    let l = l_word.unwrap_or(1);
    if l as f64 > crate::lexer::MAX_UNROLL_ITERATIONS as f64 {
        return Err(vec![crate::lexer::limit_diag(
            "max_unroll_iterations",
            crate::lexer::MAX_UNROLL_ITERATIONS as f64,
            l as f64,
            &st.file,
            line_no,
            1,
        )]);
    }

    // R и Z обязательны (LinuxCNC: «It is an error if: R/Z is not specified»).
    let r_word = r_word.ok_or_else(|| fl!(ErrorCode::InvalidInput, "цикл без R-слова (плоскость отвода)"))?;
    let z_word = axes[2]
        .as_ref()
        .ok_or_else(|| fl!(ErrorCode::InvalidInput, "цикл без Z-слова (дно отверстия)"))?
        .0;
    // Модальный feed обязателен (feed-ходы цикла — SPEC §4).
    let feed = st
        .feed
        .clone()
        .ok_or_else(|| fl!(ErrorCode::InvalidInput, "цикл без F — feed-ходы требуют модального feed (SPEC §4)"))?;
    let _ = &feed;

    // Dwell: G82/G89 — P обязателен (LinuxCNC), G86 — опционален.
    let dwell = match kind {
        CycleKind::DrillDwell | CycleKind::BoreDwell => Some(
            p_word
                .clone()
                .ok_or_else(|| fl!(ErrorCode::InvalidInput, "цикл G82/G89 без P-слова (dwell, секунды)"))?,
        ),
        CycleKind::BoreStop => p_word.clone(),
        _ => None,
    };
    // Q: G83/G73 — обязателен, строго > 0 (иначе бесконечные пеки).
    let q = match kind {
        CycleKind::Peck | CycleKind::ChipBreak => {
            let q = q_word
                .ok_or_else(|| fl!(ErrorCode::InvalidInput, "пекерующий цикл G73/G83 без Q-слова (глубина пека)"))?;
            if q <= 0.0 {
                return Err(fl!(ErrorCode::InvalidInput, "Q должен быть > 0 (глубина пека)"));
            }
            Some(q)
        }
        _ => None,
    };
    let _ = q;

    // Повторы L (default 1; лимит проверен выше, до диагностики слов).
    let l = l_word.unwrap_or(1);

    let src = st.src(line_no, line);
    let (mut cx, mut cy, mut cz) = (base[0], base[1], base[2]);
    let initial_z = base[2];
    let mut r_cur = if st.distance_abs { r_word } else { initial_z };
    let mut sub = 1u32;
    let spindle_before = st.spindle.clone(); // рестарт G86 (Q9)
    let units = st.units.unwrap_or(Units::Mm);
    let mut est_total = 0usize; // guard blowup развёртки (COVERAGE §1.2)

    for _ in 0..l {
        let (fx, fy) = (cx, cy); // позиция ДО шага — оттуда traverse
        // Позиции итерации (§7.3): G90 — константны; G91 — шаг X/Y/R на
        // повтор (LinuxCNC), Z = инкремент от R-плоскости.
        if st.distance_abs {
            cx = axes[0].as_ref().map(|v| v.0).unwrap_or(cx);
            cy = axes[1].as_ref().map(|v| v.0).unwrap_or(cy);
            // r_cur const = r_word
        } else {
            r_cur += r_word;
            cx += axes[0].as_ref().map(|v| v.0).unwrap_or(0.0);
            cy += axes[1].as_ref().map(|v| v.0).unwrap_or(0.0);
        }
        let z_bottom = if st.distance_abs { z_word } else { r_cur + z_word };

        // 1. traverse к отверстию НА ТЕКУЩЕМ Z (один оп; §7.7 #1 — merge
        //    с подходом запрещён).
        pending.push(m_rapid(st, [fx, fy, cz], [cx, cy, cz], src.clone(), sub));
        sub += 1;
        // 2. подход к R-плоскости — отдельный оп (§7.7 #2).
        pending.push(m_rapid(st, [cx, cy, cz], [cx, cy, r_cur], src.clone(), sub));
        sub += 1;
        cz = r_cur;

        // 3. Тело цикла.
        match kind {
            CycleKind::Drill | CycleKind::DrillDwell | CycleKind::BoreFeedOut
            | CycleKind::BoreDwell | CycleKind::BoreStop => {
                pending.push(m_linear(st, [cx, cy, cz], [cx, cy, z_bottom], src.clone(), sub));
                sub += 1;
                cz = z_bottom;
                if let Some(p) = &dwell {
                    pending.push(m_dwell(st, p, src.clone(), sub));
                    sub += 1;
                }
            }
            CycleKind::Peck | CycleKind::ChipBreak => {
                // §7.4: порядок пиков точный. Глубины пеков — от R-плоскости
                // вниз шагами Q (LinuxCNC «move down by Q»): d_k =
                // max(z_bottom, r_cur − k·q); G83 — полный отвод на R после
                // каждого пека; G73 — отвод на Q над последней глубиной;
                // подход — rapid на clr над последней глубиной (≤ R).
                let q = q.expect("пекерующий цикл без Q (проверено выше)");
                let t = r_cur - z_bottom;
                let mut n = if t > 0.0 { (t / q).ceil() as u64 } else { 0 };
                if n == 0 {
                    n = 1;
                }
                let est = 2 + 3 * n as usize + 1;
                est_total += est;
                if est_total > crate::lexer::MAX_UNROLL_ITERATIONS {
                    return Err(vec![crate::lexer::limit_diag(
                        "max_unroll_iterations",
                        crate::lexer::MAX_UNROLL_ITERATIONS as f64,
                        est_total as f64,
                        &st.file,
                        line_no,
                        1,
                    )]);
                }
                let clr = approach_clearance(units);
                for k in 1..=n {
                    let d = (r_cur - k as f64 * q).max(z_bottom);
                    pending.push(m_linear(st, [cx, cy, cz], [cx, cy, d], src.clone(), sub));
                    sub += 1;
                    cz = d;
                    if k == n {
                        break; // финальный отвод — общий ниже (§7.1)
                    }
                    match kind {
                        CycleKind::Peck => {
                            // полный отвод на R (SPEC §7 таблица)
                            pending.push(m_rapid(st, [cx, cy, cz], [cx, cy, r_cur], src.clone(), sub));
                            sub += 1;
                            cz = r_cur;
                        }
                        _ => {
                            // G73: отвод на Q над последней глубиной (≤ R)
                            let up = (d + q).min(r_cur);
                            pending.push(m_rapid(st, [cx, cy, cz], [cx, cy, up], src.clone(), sub));
                            sub += 1;
                            cz = up;
                        }
                    }
                    // подход: rapid на clr над последней глубиной (≤ R)
                    let appr = (d + clr).min(r_cur);
                    pending.push(m_rapid(st, [cx, cy, cz], [cx, cy, appr], src.clone(), sub));
                    sub += 1;
                    cz = appr;
                }
            }
        }

        // 4/5. Отвод (§7.1): G99 → R, G98 → initial Z. Тип отвода: G85/G89 —
        // feed-out (bore); G86 — rapid при выключенном шпинделе (Q9: рестарт
        // в прежнем направлении после отвода); остальные — rapid.
        let retract = if st.retract_g98 { initial_z } else { r_cur };
        match kind {
            CycleKind::BoreFeedOut | CycleKind::BoreDwell => {
                pending.push(m_linear(st, [cx, cy, cz], [cx, cy, retract], src.clone(), sub));
                sub += 1;
            }
            CycleKind::BoreStop => {
                st.spindle = Spindle::Off;
                pending.push(m_rapid(st, [cx, cy, cz], [cx, cy, retract], src.clone(), sub));
                sub += 1;
                st.spindle = spindle_before.clone(); // рестарт — в state последующих опов
            }
            _ => {
                pending.push(m_rapid(st, [cx, cy, cz], [cx, cy, retract], src.clone(), sub));
                sub += 1;
            }
        }
        cz = retract;
    }

    // last_motion: конечная позиция развёртки (связывание следующего опа).
    st.last_motion = Some(LastMotion { end: [cx, cy, cz], frame: Frame::Wcs, wcs: st.wcs });
    st.saw_motion = true;
    Ok(())
}

// ===========================================================================
// Финализация: PEND-инвариант, drop+notice, feed-требование
// ===========================================================================

fn finish(mut st: St) -> Result<ParseOut, Vec<Diagnostic>> {
    let mut errors: Vec<Diagnostic> = Vec::new();

    // SPEC §3 v0.16: program_end ровно один раз, последний; ОТСУТСТВИЕ
    // M2/M30 = неявный конец → авто-PEND + notice (не отказ; реальные
    // посты часто не пишут program-end — 18+/32 Tier B).
    let end_count = st.ops.iter().filter(|o| matches!(o, Op::ProgramEnd { .. })).count();
    let mut notices: Vec<Notice> = st.notices;
    if end_count == 0 {
        let auto = Op::ProgramEnd {
            src: SrcSpan {
                file: st.file.clone(),
                line_start: 0,
                col_start: 1,
                line_end: 0,
                col_end: 0,
            },
        };
        notices.push(Notice {
            kind: NoticeKind::MissingProgramEnd,
            message: "W_MISSING_PROGRAM_END: program_end отсутствует — добавлен автоматически (SPEC §3 v0.16)".into(),
            op_index: st.ops.len(),
            op: auto.clone(),
        });
        st.ops.push(auto);
    }
    if end_count > 1 {
        errors.push(Diagnostic {
            stage: Stage::Verify,
            code: ErrorCode::InvalidInput,
            message: format!("дубликат program_end: {end_count} × M2/M30 (SPEC §3; N13)"),
            src: None,
            options: Vec::new(),
            limit: None,
        });
    }
    match st.ops.last() {
        Some(last) if !matches!(last, Op::ProgramEnd { .. }) => {
            let src = op_src(last);
            errors.push(Diagnostic {
                stage: Stage::Verify,
                code: ErrorCode::InvalidInput,
                message: "program_end не последний (опы после M2/M30 запрещены, SPEC §3; N12)".into(),
                src: Some(src),
                options: Vec::new(),
                limit: None,
            });
        }
        _ => {}
    }

    // Drop-with-notice (SPEC §3/§5.2/§10.6): ход-разбора (zero-sweep дуги)
    // + пост-проход (zero-length rapid/linear, dwell 0).
    let mut kept: Vec<Op> = Vec::new();
    for (i, o) in st.ops.iter().enumerate() {
        let zero: Option<(NoticeKind, String)> = match o {
            Op::Rapid { start, end, .. } | Op::Linear { start, end, .. } => match start {
                StartPoint::Known(s) => {
                    let sv = to_f64(s);
                    let ev = to_f64(end);
                    if sv == ev {
                        Some((
                            NoticeKind::ZeroLengthMotion,
                            "zero-length motion dropped (start == end, SPEC §3)".into(),
                        ))
                    } else {
                        None
                    }
                }
                StartPoint::Unknown => None,
            },
            Op::Dwell { seconds, .. } => {
                if seconds.value == 0.0 {
                    Some((
                        NoticeKind::ZeroDwell,
                        "zero dwell dropped (seconds == 0, SPEC §5.2)".into(),
                    ))
                } else {
                    None
                }
            }
            _ => None,
        };
        match zero {
            Some((kind, message)) => {
                notices.push(Notice { kind, message, op_index: i, op: o.clone() })
            }
            None => kept.push(o.clone()),
        }
    }

    // linear/arc в итоговом AST требуют модального feed (SPEC §3);
    // ARC с start=unknown не доживает до сюда (Err на построении) — защита.
    for o in &kept {
        match o {
            Op::Arc { start: StartPoint::Unknown, src, .. } => {
                errors.push(Diagnostic {
                    stage: Stage::Verify,
                    code: ErrorCode::RuntimeState,
                    message: "arc с start=unknown (защита, SPEC §2 Q11)".into(),
                    src: Some(src.clone()),
                    options: Vec::new(),
                    limit: None,
                });
            }
            Op::Linear { state, src, .. } | Op::Arc { state, src, .. } => {
                if state.feed.is_none() {
                    errors.push(Diagnostic {
                        stage: Stage::Verify,
                        code: ErrorCode::InvalidInput,
                        message: "linear/arc без F до первого содержательного хода — E_INVALID_INPUT (SPEC §3)".into(),
                        src: Some(src.clone()),
                        options: Vec::new(),
                        limit: None,
                    });
                }
            }
            _ => {}
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let program = Program {
        eid_version: EID_VERSION.to_string(),
        units: st.units.unwrap_or(Units::Mm),
        path_mode: match st.path {
            None => PathMode::ExactPath,
            Some(PathKind::ExactPath) => PathMode::ExactPath,
            Some(PathKind::ExactStop) => PathMode::ExactStop,
            Some(PathKind::Blend) => PathMode::Blend { t: st.path_t },
        },
        frame_preset: st.preset,
        ops: kept,
    };
    Ok(ParseOut { program, diagnostics: st.diags, notices })
}

fn op_src(op: &Op) -> SrcSpan {
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
        | Op::Raw { src, .. } => src.clone(),
    }
}
