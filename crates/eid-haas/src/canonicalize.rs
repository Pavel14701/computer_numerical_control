//! Канонизатор Haas mill NGC → EID/L1 AST (этапы 1–2).
//! Источники: haas_dialect_analysis.md v0.0 (контракт; §1.1 группы, §1.2 DPI
//! machinists notation + Setting 77, §2.1 движение + G01 C/R, §2.2 модальные,
//! §2.3 фреймы, §2.5 циклы, §5 циклы-детали, §9.1 v0.1-скоуп, §10 реестр 34
//! `?`), SPEC-EID-L1 v0.18 (§2 точки разрыва, §3 модель + полные круги v0.18,
//! §4 state, §5 опы, §6 дуги, §7.4a клиренс 0.01in unit-scaled + §7.7 шаблон
//! развёртки, §8 RAW + §8.6 абсорбция v0.17), COVERAGE v0.9.2 Haas-колонка
//! (§6.19 δ-расхождение), паттерн — зеркала eid-grbl/eid-linuxcnc/eid-fanuc.
//!
//! Этап 1 (в силе): лексика (EOB `;`, строгий пробел-в-слове `?`), модальности
//! 01–06+15, движение G0–G3 (IJK-инкремент всегда, R-форма, полный круг 360°
//! по геометрии §3 v0.18), DPI machinists notation + F-домен Setting 77,
//! §8.6 модальная абсорбция, auto-PEND, хвост-subs → Raw.
//!
//! Этап 2 добавляет:
//! * **Фреймы** (§2.3 отчёта): G54–G59 → p=1..6; **G110–G112 → p=7..9**
//!   (нативная симметрия EID-слотов #7–#9 = Wcs::G59_1..G59_3; notice
//!   W_WCS_EXTENDED — эмиссия этапа 3 отдаст G110–G112); G154 P1–P3 → 7..9,
//!   P4+/без P → Raw/Err; G113–G129 → Raw{SEM} «слот вне EID p-домена»;
//!   **G52 → Err** (2-й жёсткий отказ, SPEC §8 правило 5); G53 machine-frame
//!   (правило §5.3: точки разрыва (b)/(d)); G10 L2 P1–P6 → wcs_define
//!   (partial → Raw RT); **G10 L2 P0 = запись слота G52 → Raw RT** (≠ Fanuc
//!   external offset); **G10 L20 → Raw RT** (НЕ wcs_set_here, ловушка №2b);
//!   G10 L1/L10–L13 → Raw RT; G92 — только пресетка (S-кламп → Raw SEM);
//!   absorb_frame_structs (G92/G10 на raw-строках).
//! * **Циклы ×Haas-семантика** (§5.4 отчёта × §7.7 SPEC + §7.4a v0.18
//!   клиренс 0.01in unit-scaled): разворачивание G81/G82/G83/G73/G85/G86/G89;
//!   **L-повторы (НЕ K — K = min-пек G83)**; G91-шаги; **G83: Q ИЛИ I/J/K
//!   (d1=I, dk=max(K, d_(k−1)−J), mutually exclusive) — полный отвод**;
//!   **G73 δ = канон-константа 0.01in/0.254mm** (Setting 22 вне текста →
//!   notice W_CYCLE_PARAM_ASSUMED; документированный дефолт Setting 22
//!   0.02"/0.5mm ≠ канон — диалектное расхождение COVERAGE §6.19);
//!   G86 рестарт шпинделя + **P-dot-rule: dotted=сек / bare=мс** (G82/G86/G89
//!   — уникальное Haas-правило; G86 P обязателен — SPEC v0.18 rs274);
//!   дефолт G99 (§7.5a); G98-known-position (§7.6); G74/G76 (I/J-направление
//!   в тексте)/G77/G156/G174/G184/G84 → Raw SEM; L0 `?` → Raw; группа
//!   09-модальность + G80.
//! * **M-словарь**: M0/M1 → program_stop; M6/M16+T (идиома `T01 M06`);
//!   одиночное T → Raw SEM (предвыбор); **M07 = Shower → Raw SEM** (второй
//!   канал СОЖ, НЕ mist!); **M10/M11 тормоз 4-й оси, M12/M13 тормоз 5-й оси →
//!   Raw SEM** (гипотеза «M13=шпиндель+СОЖ» опровергнута); **M29 = output
//!   relay → Raw SEM (≠ Fanuc rigid-tap)**; M19 → Raw; M97/M98/M99/M46/M96 →
//!   Raw SYNTAX; M109 → Raw RUNTIME.
//! * **Компенсации**: G43/G44/G143 → Raw RT; семейства Haas-состава:
//!   G40={G41,G42,G141}, G49={G43,G44,G143} (правило семейств §1.3 COVERAGE).
//! * **Macro B**: bucket → line-level Raw SYNTAX (система-переменные
//!   #500–#699/#109xxx ≠ Fanuc — в диагностике).
//! * G4 dwell (P-dot-rule; X/U-форм НЕТ); G103/G167/G187/G150/трансформации
//!   → raw-таблица §2.6/§2.7.

use crate::block::{parse_file, Block, Marker, ParsedFile};
use crate::lexer::LexErrorKind;
use crate::state::{
    g_raw_disposition, plane_axes, plane_normal, CycleKind, GCode, MCode, ModalState, RAW_MOTION_G,
};
use eid_core::ast::{
    Coolant, Dir, Frame, FramePreset, Num, Op, PathMode, Point3, Program, Spindle, SrcSpan,
    StartPoint, State, Units, Wcs,
};
use eid_core::diag::{Diagnostic, ErrorCode, LimitInfo};
use std::cell::RefCell;

// ---------------------------------------------------------------------------
// Notices (реплика паттерна eid-grbl/eid-linuxcnc/eid-fanuc; SPEC §3/§6)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// rapid/linear: start == end (SPEC §3).
    ZeroLengthMotion,
    /// arc: start == end при sweep != 360° и pitch == 0 (SPEC §3/§6).
    ZeroLengthArc,
    /// dwell == 0 (SPEC §5.2: no-op → drop+notice).
    ZeroLengthDwell,
    /// near-degenerate R-дуга (±0.12° вокруг 180°): амплитификация центра
    /// (EQUIVALENCE §4) — notice при живом опе.
    NearDegenerateRArc,
    /// Программа без M2/M30: неявный конец — ProgramEnd добавлен последним
    /// опом (SPEC §3 v0.16; контракт §4.4 — DNC-файлы Haas без M30).
    MissingProgramEnd,
    /// W_DPI_ASSUMED (анализ §1.2 ловушка №1): bare-integer в слове дробного
    /// домена прочитан по machinists notation (× least increment, Setting 162
    /// OFF); F — × 0.0001 (Setting 77 «Scale Integer F» OFF); **P dwell bare —
    /// миллисекунды** (P-dot rule: точка = сек, без = мс).
    DpiAssumed,
    /// W_MODAL_ON_RAW (SPEC §8.6 правило 6, v0.17 — критический аудит #33):
    /// raw-motion-G (G28/G31/G12/G70/G74/G84…) с осями — модальное состояние
    /// абсорбировано, но motion-режим НЕ продолжается (ход не представим).
    ModalOnRaw,
    /// W_WCS_EXTENDED (этап 2, бриф): G110–G112/G154 P1–P3 — расширенные слоты
    /// #7–#9 → EID p=7..9 (нативная симметрия нумерации; эмиссия этапа 3 —
    /// G110–G112 с notice; COVERAGE Haas-колонка v0.9 — RAW консервативно,
    /// расхождение колонки и контракта задокументировано, не молча).
    WcsExtended,
    /// W_CYCLE_PARAM_ASSUMED (этап 2, COVERAGE §6.19): G73 retract-δ = Setting
    /// 22 вне текста программы; применена канон-константа SPEC §7.4a v0.18
    /// (0.01in unit-scaled) — документированный дефолт Setting 22 0.02"/0.5mm
    /// ≠ канон (диалектное расхождение зафиксировано, профиль константу не
    /// переопределяет).
    CycleParamAssumed,
}

#[derive(Debug, Clone)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
    /// Индекс опа в списке ops (для drop-notice — индекс, который оп ИМЕЛ БЫ).
    pub op_index: usize,
    pub op: Op,
}

/// Результат: программа + notices (drop/DPI/auto-PEND/WCS-extended).
#[derive(Debug, Clone)]
pub struct ParseOut {
    pub program: Program,
    pub notices: Vec<Notice>,
}

// ---------------------------------------------------------------------------
// Числовые/диагностические хелперы
// ---------------------------------------------------------------------------

/// Квантование входа к 4dp (SPEC §10.6 — при построении Num).
fn quant(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// Вычисленное число → Num: 4 dp, трейлинг-нули нормализованы (SPEC §10.2).
pub(crate) fn num_f64(v: f64) -> Num {
    let mut s = format!("{:.4}", quant(v));
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".into();
    }
    Num::new(s, quant(v))
}

/// Слово с точкой → verbatim Num (SPEC §10.1), квантование значения §10.6.
fn verbatim(w: &crate::lexer::RawNum) -> Num {
    Num::new(w.text.clone(), quant(w.value))
}

/// DPI least increment по активным единицам (§1.2: least input increment
/// 0.001 mm / 0.0001" — совпадает с Fanuc IS-B; таблицы Setting 77).
fn least_increment(units: Units) -> f64 {
    match units {
        Units::Mm => 0.001,
        Units::In => 0.0001,
    }
}

/// Клиренс пека (SPEC §7.4a v0.18 — зеркало eid-fanuc/eid-linuxcnc): 0.01in
/// unit-scaled — **0.254 mm** в G21, ровно **0.0100 in** в G20. Для Haas G73
/// это канон-константа при Setting-22-природе δ (вне текста → notice
/// W_CYCLE_PARAM_ASSUMED; документированный дефолт Setting 22 0.02"/0.5mm ≠
/// канон — COVERAGE §6.19); профиль константу НЕ переопределяет.
fn approach_clearance(units: Units) -> f64 {
    if units == Units::In {
        0.01
    } else {
        0.254
    }
}

/// max_unroll_iterations (COVERAGE §1.2: 10 000 — сумма L на блок цикла).
const MAX_UNROLL_ITERATIONS: u64 = 10_000;

fn span(file: &str, line: u32, len: usize) -> SrcSpan {
    SrcSpan {
        file: file.to_string(),
        line_start: line,
        col_start: 1,
        line_end: line,
        col_end: len as u32 + 1,
    }
}

/// Диагностика с явным этапом: Parse — лексические пути, Verify —
/// канонизационные инварианты (interface v0.1.2, паттерн eid-grbl/fanuc).
fn err_staged(
    stage: eid_core::diag::Stage,
    code: ErrorCode,
    msg: impl Into<String>,
    file: &str,
    line: u32,
    len: usize,
) -> Diagnostic {
    Diagnostic {
        code,
        message: msg.into(),
        src: Some(span(file, line, len)),
        options: Vec::new(),
        limit: None,
        stage,
    }
}

fn err(code: ErrorCode, msg: impl Into<String>, file: &str, line: u32, len: usize) -> Diagnostic {
    err_staged(eid_core::diag::Stage::Verify, code, msg, file, line, len)
}

fn limit_err(
    threshold: &'static str,
    value: f64,
    actual: f64,
    msg: String,
    file: &str,
    line: u32,
) -> Diagnostic {
    Diagnostic {
        code: ErrorCode::LimitExceeded,
        message: msg,
        src: Some(span(file, line, 0)),
        options: Vec::new(),
        limit: Some(LimitInfo {
            threshold,
            threshold_value: value,
            actual,
        }),
        stage: eid_core::diag::Stage::Verify,
    }
}

fn e3(p: &Point3) -> [f64; 3] {
    [p.x.value(), p.y.value(), p.z.value()]
}

fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3 {
        x: num_f64(x),
        y: num_f64(y),
        z: num_f64(z),
    }
}

fn p3(v: [f64; 3]) -> Point3 {
    pt(v[0], v[1], v[2])
}

/// hex-кодировка байтов строки (grammar D9; payload — точная вырезка +
/// терминатор строки, ruling 5).
fn payload_hex(line: &str) -> String {
    let mut s = String::new();
    for b in line.as_bytes() {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

/// Wcs → индекс таблицы origin (EID enum: G54=0..G59=5, G59_1=6..G59_3=8;
/// слоты p=1..9 — G110–G112 садятся на idx 6..8).
fn wcs_idx(w: Wcs) -> usize {
    w as usize
}

// ---------------------------------------------------------------------------
// Ctx — модальное состояние (state.rs ModalState) + resolved-state + фреймы
// ---------------------------------------------------------------------------

/// Чтение позиции последнего motion-опа: позиция + фрейм записи (machine/
/// wcs) + активная WCS в момент записи — точки разрыва (a)–(d), SPEC §2.
#[derive(Debug, Clone, Copy)]
struct Reading {
    pos: [f64; 3],
    frame: Frame,
    wcs: Wcs,
}

/// Результат связывания старта (SPEC §2 принцип 2).
#[derive(Debug, Clone, Copy)]
enum Linked {
    /// start вычислим (связывание — обязанность канонизатора, v0.9).
    Known([f64; 3]),
    /// Точка разрыва (a): первый motion-оп без якоря — start Unknown, но
    /// отсутствующие оси end наследуют дефолт контроллера 0 (§2 v0.11).
    UnknownAtStart,
    /// Точки разрыва (b)/(d) (и machine-строгость): частичный end наследует
    /// неизвестное → E_RUNTIME_STATE; полный end легален (start Unknown).
    UnknownRequired,
}

/// Активный модальный цикл группы 09 (§5.1 отчёта: параметры модальны,
/// X/Y-блоки = повтор; Z/R/Q/P/L/I/J/K-слова повторных блоков обновляют
/// параметры).
#[derive(Debug, Clone)]
struct ActiveCycle {
    kind: CycleKind,
    z: Option<f64>,   // дно отверстия (сырое слово; G91 — от R-плоскости)
    r: Option<f64>,   // R-плоскость (G91 — от initial Z)
    q: Option<f64>,   // пек (G73/G83 Q-форма)
    p_s: Option<f64>, // dwell в секундах (P-dot rule: точка = сек, без = мс)
    i: Option<f64>,   // G83 переменный пек: первый пек (d1 = I)
    j: Option<f64>,   // G83 переменный пек: уменьшение шага
    k: Option<f64>,   // G83 переменный пек: минимальный пек
}

struct Ctx {
    file: String,
    modal: ModalState,
    feed: Option<Num>, // модальный F (переживает G0, SPEC §4)
    spindle: Spindle,
    coolant: Coolant,
    tool: Option<u32>,  // этап 2 (M6/M16+T)
    last_s: Option<Num>, // голое S — сохранение оборотов (§3 контракта)
    preset: Option<FramePreset>,    // G92 (только пресетка, §3 SPEC)
    origins: [Option<[f64; 3]>; 9], // wcs_define in-text (p 1..9 → idx 0..8)
    prev: Option<Reading>,          // end последнего motion-опа
    saw_motion: bool,               // G92-пресетка только до первого движения
    cycle: Option<ActiveCycle>,     // группа 09 — модальный цикл
    ops: Vec<Op>,
    notices: Vec<Notice>,
    /// Pending-notice'ы слов/кодов, прочитанных до построения опа (DPI,
    /// W_WCS_EXTENDED): прицепляются к следующему опу (в т.ч. к raw-опу —
    /// §8.6).
    pending: RefCell<Vec<(NoticeKind, String)>>,
    end_count: usize,
    end_is_last: bool,
    end_line: u32,
    /// Правило семейств (COVERAGE §1.3, HAAS-СОСТАВЫ) — пре-скан файла:
    family_g40: bool, // G41/G42/G141 встречены в файле
    family_g49: bool, // G43/G44/G143 встречены в файле
}

impl Ctx {
    fn state(&self) -> State {
        self.state_with_frame(Frame::Wcs)
    }

    fn state_with_frame(&self, frame: Frame) -> State {
        State {
            feed: self.feed.clone(),
            spindle: self.spindle.clone(),
            coolant: self.coolant,
            tool: self.tool,
            frame,
            wcs: self.modal.wcs,
        }
    }

    /// Записать pending-notice (прицепится к следующему опу).
    fn note_pending(&self, kind: NoticeKind, msg: String) {
        self.pending.borrow_mut().push((kind, msg));
    }

    /// Записать W_DPI_ASSUMED-сообщение (прицепится к следующему опу).
    fn note_dpi(&self, msg: String) {
        self.note_pending(NoticeKind::DpiAssumed, msg);
    }

    /// Слово дробного домена (X/Y/Z/A/B/C/I/J/K/R/Q/E) → Num: bare-integer →
    /// × least increment (machinists notation, Setting 162 OFF) + W_DPI_ASSUMED;
    /// с точкой → verbatim (§1.2 ловушка №1 — направление обратное Fanuc-DPI-
    /// гипотезе, но политика Fanuc §9.1(1) переиспользуется).
    fn dpi_num(&self, letter: char, w: &crate::lexer::RawNum) -> Num {
        if w.has_point {
            return verbatim(w);
        }
        let inc = least_increment(self.modal.units);
        let v = w.value * inc;
        let (u_str, inc_str) = match self.modal.units {
            Units::Mm => ("mm", "0.001"),
            Units::In => ("inch", "0.0001"),
        };
        self.note_dpi(format!(
            "W_DPI_ASSUMED: {}{} (bare integer) read as {} ({} × least increment {} = {}; machinists notation default, Setting 162 Default To Float OFF, least input increment {}; analysis §1.2)",
            letter,
            w.text,
            num_f64(v).text,
            w.text,
            inc_str,
            num_f64(v).text,
            u_str,
        ));
        num_f64(v)
    }

    /// F-слово → Num — HAAS-ПРАВИЛО (≠ Fanuc ruling 2!): bare-integer →
    /// × 0.0001 (Setting 77 «Scale Integer F» дефолт OFF: `F12` =
    /// 0.0012 units/min — фиксированный F-домен, НЕ unit-scaled ×least-
    /// increment); dotted-F → verbatim. Fanuc-политика «bare F = целое
    /// mm/min verbatim (параметр 1422)» НЕ переносится: F-домен у Haas
    /// другой (§1.2: «Setting 77 — F12 = 0.0012 units/min; ON: F12 = 12.0»;
    /// бриф этапа 1: «Haas F = ×0.0001 при Setting 77 OFF»).
    fn feed_num(&self, w: &crate::lexer::RawNum) -> Num {
        if w.has_point {
            return verbatim(w);
        }
        const F_SCALE: f64 = 0.0001;
        let v = w.value * F_SCALE;
        self.note_dpi(format!(
            "W_DPI_ASSUMED: F{} (bare integer) read as {} units/min ({} × 0.0001 = {}; Setting 77 Scale Integer F default OFF — F12 = 0.0012 units/min; Haas F domain differs from Fanuc integer-mm/min ruling; analysis §1.2)",
            w.text,
            num_f64(v).text,
            w.text,
            num_f64(v).text,
        ));
        num_f64(v)
    }

    /// P dwell (G04 и циклы G82/G86/G89) — P-DOT RULE (уникальное Haas-
    /// правило, §1.2/§2.7): **с точкой = секунды verbatim, без точки =
    /// миллисекунды** (`G04 P10.` = 10 s; `G04 P10` = 0.010 s). Bare-чтение
    /// — с notice W_DPI_ASSUMED (DPI-домен мс-гранулярности).
    fn dwell_seconds(&self, w: &crate::lexer::RawNum) -> f64 {
        if w.has_point {
            return w.value;
        }
        self.note_dpi(format!(
            "W_DPI_ASSUMED: P{} (bare integer) dwell read as {} s (P-dot rule: dotted P = seconds, bare P = milliseconds — {} ms; unique Haas rule, analysis §1.2/§2.7)",
            w.text,
            num_f64(w.value / 1000.0).text,
            num_f64(w.value).text,
        ));
        w.value / 1000.0
    }

    fn flush_pending(&mut self, op_index: usize, op: &Op) {
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        for (kind, message) in pending {
            self.notices.push(Notice {
                kind,
                message,
                op_index,
                op: op.clone(),
            });
        }
    }

    /// push при живом опе (+ flush pending-notice'ов на этот оп).
    fn push(&mut self, op: Op) {
        self.ops.push(op);
        let idx = self.ops.len() - 1;
        let op_ref = self.ops[idx].clone();
        self.flush_pending(idx, &op_ref);
    }

    /// drop-with-notice (оп НЕ попадает в ops, SPEC §3) + flush pending.
    fn push_drop(&mut self, op: Op, kind: NoticeKind, message: String) {
        let idx = self.ops.len();
        self.flush_pending(idx, &op);
        self.notices.push(Notice {
            kind,
            message,
            op_index: idx,
            op,
        });
    }

    /// notice при ЖИВОМ опе (near-degenerate R-arc, auto-PEND) + flush pending.
    fn push_keep(&mut self, op: Op, kind: NoticeKind, message: String) {
        let idx = self.ops.len();
        self.ops.push(op.clone());
        self.flush_pending(idx, &op);
        self.notices.push(Notice {
            kind,
            message,
            op_index: idx,
            op,
        });
    }

    fn raw_op(&self, b: &Block, code: ErrorCode, reason: &str) -> Op {
        Op::Raw {
            payload_hex: payload_hex(&b.raw),
            reason: reason.to_string(),
            code,
            src: span(&self.file, b.line_no, b.raw.len()),
        }
    }

    fn mark_end(&mut self, line: u32, is_last: bool, blen: usize) {
        self.end_count += 1;
        self.end_line = line;
        self.end_is_last = is_last;
        let op = Op::ProgramEnd {
            src: span(&self.file, line, blen),
        };
        self.push(op);
    }

    /// Связывание старта (SPEC §2 принцип 2, точки разрыва (a)–(d)).
    /// `machine` — G53-ход: правило §5.3 (start вычислим ⇔ нет пресетки и
    /// origin активной WCS in-text; иначе E_RUNTIME_STATE — невыразим).
    fn link_start(&self, machine: bool, line: u32, blen: usize) -> Result<Linked, Vec<Diagnostic>> {
        let rt = |msg: String| vec![err(ErrorCode::RuntimeState, msg, &self.file, line, blen)];
        if machine {
            // §5.3(a): machine-ход при активном frame_preset — невыразим.
            if self.preset.is_some() {
                return Err(rt(
                    "machine-frame move with active frame_preset — M-coordinates unknown (SPEC §5.3a)".into(),
                ));
            }
            return Ok(match self.prev {
                Some(Reading {
                    pos,
                    frame: Frame::Machine,
                    ..
                }) => Linked::Known(pos),
                Some(Reading {
                    pos,
                    frame: Frame::Wcs,
                    wcs,
                }) => {
                    // reading_M = W + O_wcs + Δ; Δ = 0 (пресетка была бы отклонена).
                    match self.origins[wcs_idx(wcs)] {
                        Some(o) => Linked::Known([pos[0] + o[0], pos[1] + o[1], pos[2] + o[2]]),
                        None => {
                            return Err(rt(
                                "machine-frame move: origin of active WCS not defined in-text (SPEC §5.3b)".into(),
                            ))
                        }
                    }
                }
                None => {
                    // Первое движение в machine-frame: M₀ внешняя — точка
                    // разрыва (c); полный end обязателен (§2 v0.13).
                    if self.origins[wcs_idx(self.modal.wcs)].is_some() {
                        Linked::UnknownRequired
                    } else {
                        return Err(rt(
                            "machine-frame move: origin of active WCS not defined in-text (SPEC §5.3b)".into(),
                        ));
                    }
                }
            });
        }
        // WCS-движение.
        Ok(match self.prev {
            None => {
                // Точка разрыва (a): якорь — только frame_preset (Haas L20 —
                // запись origin доп. СК, НЕ wcs_set_here — ловушка №2b).
                match self.preset.as_ref() {
                    Some(pr) => {
                        let vals = e3(&pr.values);
                        if pr.wcs == self.modal.wcs {
                            Linked::Known(vals)
                        } else {
                            // reading = preset.values + O_preset − O_current
                            // (точка разрыва (b): обе системы in-text).
                            match (
                                self.origins[wcs_idx(pr.wcs)],
                                self.origins[wcs_idx(self.modal.wcs)],
                            ) {
                                (Some(o0), Some(on)) => Linked::Known([
                                    vals[0] + o0[0] - on[0],
                                    vals[1] + o0[1] - on[1],
                                    vals[2] + o0[2] - on[2],
                                ]),
                                _ => Linked::UnknownRequired,
                            }
                        }
                    }
                    None => Linked::UnknownAtStart,
                }
            }
            Some(Reading {
                pos,
                frame: Frame::Machine,
                ..
            }) => {
                // Точка разрыва (d): reading_wcs = M_after − O_wcs − Δ.
                match self.origins[wcs_idx(self.modal.wcs)] {
                    Some(on) => Linked::Known([pos[0] - on[0], pos[1] - on[1], pos[2] - on[2]]),
                    None => Linked::UnknownRequired,
                }
            }
            Some(Reading {
                pos,
                frame: Frame::Wcs,
                wcs: prev_wcs,
            }) => {
                if prev_wcs == self.modal.wcs {
                    Linked::Known(pos)
                } else {
                    // Точка разрыва (b): критерий РАЗНОСТИ — нужны define
                    // ОБЕИХ систем до точки хода (SPEC §2 v0.10).
                    match (
                        self.origins[wcs_idx(prev_wcs)],
                        self.origins[wcs_idx(self.modal.wcs)],
                    ) {
                        (Some(op_), Some(on)) => Linked::Known([
                            pos[0] + op_[0] - on[0],
                            pos[1] + op_[1] - on[1],
                            pos[2] + op_[2] - on[2],
                        ]),
                        _ => Linked::UnknownRequired,
                    }
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Входные точки
// ---------------------------------------------------------------------------

pub fn canonicalize(src: &str, file: &str) -> Result<Program, Vec<Diagnostic>> {
    canonicalize_with_notices(src, file).map(|o| o.program)
}

pub fn canonicalize_with_notices(src: &str, file: &str) -> Result<ParseOut, Vec<Diagnostic>> {
    // Лимит max_source_lines (COVERAGE §1.2).
    let line_count = src.lines().count() as f64;
    if line_count > 1_000_000.0 {
        return Err(vec![limit_err(
            "max_source_lines",
            1_000_000.0,
            line_count,
            format!("file has {} lines, limit 1000000", src.lines().count()),
            file,
            1,
        )]);
    }

    let parsed: ParsedFile = parse_file(src);
    // Лексические ошибки: мусор/комментарии → INVALID (Parse); переполнения —
    // LIMIT (§1.2: целая часть >9 цифр max_word_chars не ловит); пробел-в-
    // слове — строгий отказ `?` (контракт не специфицирует).
    if !parsed.lex_errors.is_empty() {
        return Err(parsed
            .lex_errors
            .iter()
            .map(|e| match &e.kind {
                LexErrorKind::WordTooLong => limit_err(
                    "max_word_chars", 32.0, 32.0,
                    "word value exceeds max_word_chars (32)".into(), file, e.line,
                ),
                LexErrorKind::IntOverflow { digits } => limit_err(
                    "max_integer_digits", 9.0, *digits as f64,
                    format!("integer part of {digits} digits exceeds max_integer_digits (9) — Haas range ±99999.9999-class `?` (analysis §1.2; registry §10(31))"),
                    file, e.line,
                ),
                LexErrorKind::SpaceInWord { letter } => err_staged(
                    eid_core::diag::Stage::Parse,
                    ErrorCode::InvalidInput,
                    format!(
                        "space between word letter '{letter}' and value — strict reject: Haas contract does not specify (`?` COVERAGE §6 registry; real Haas lexer unverified; analysis §1.3)"
                    ),
                    file, e.line, e.col as usize,
                ),
                kind => err_staged(
                    eid_core::diag::Stage::Parse,
                    ErrorCode::InvalidInput,
                    format!("lexical error: {:?} at col {}", kind, e.col),
                    file, e.line, e.col as usize,
                ),
            })
            .collect());
    }

    // Правило семейств (COVERAGE §1.3, HAAS-СОСТАВЫ — ≠ Fanuc-составы):
    // сканер отмечает семейство целиком до диспозиций. Активные коды — вне
    // enum (raw-таблица), поэтому скан по сырым G-значениям unsupported_words.
    // G40 = {G41, G42, G141}; G49 = {G43, G44, G143}; семейство G11 для Haas
    // НЕ требуется (G10 one-shot, G11 в словаре нет — COVERAGE §1.3).
    let g_family_t = |b: &Block, fam: &[i64]| {
        b.unsupported_words.iter().any(|w| {
            if w.letter != 'G' {
                return false;
            }
            let t = (w.num.value * 10.0).round() as i64;
            (w.num.value * 10.0).round() == w.num.value * 10.0 && fam.contains(&t)
        })
    };
    const G40_FAMILY: [i64; 3] = [410, 420, 1410]; // G41, G42, G141
    const G49_FAMILY: [i64; 3] = [430, 440, 1430]; // G43, G44, G143
    let family_g40 = parsed.blocks.iter().any(|b| g_family_t(b, &G40_FAMILY));
    let family_g49 = parsed.blocks.iter().any(|b| g_family_t(b, &G49_FAMILY));

    let mut ctx = Ctx {
        file: file.to_string(),
        modal: ModalState::default(),
        feed: None,
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        tool: None,
        last_s: None,
        preset: None,
        origins: Default::default(),
        prev: None,
        saw_motion: false,
        cycle: None,
        ops: Vec::new(),
        notices: Vec::new(),
        pending: RefCell::new(Vec::new()),
        end_count: 0,
        end_is_last: true,
        end_line: 0,
        family_g40,
        family_g49,
    };

    let n = parsed.blocks.len();
    for (i, b) in parsed.blocks.iter().enumerate() {
        process_block(&mut ctx, b, i + 1 == n)?;
    }
    finalize(&mut ctx, line_count as u32)
}

/// Структурная проверка program_end (SPEC §3 + контракт §4.4):
/// хвост после первого program_end — line-level Raw-хвост
/// {E_UNSUPPORTED_SYNTAX} (тела subs/main+subs в одном файле — норма Haas,
/// §4.4; осознанное расхождение с grbl n12 — зеркало Fanuc ruling 1),
/// НЕ жёсткий Err. Дубль M2/M30 — defensive-guard. Отсутствие M2/M30 →
/// auto-PEND + notice.
fn finalize(ctx: &mut Ctx, line_count: u32) -> Result<ParseOut, Vec<Diagnostic>> {
    if ctx.end_count > 1 {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "duplicate program end: more than one M2/M30 (SPEC §3; analysis §4.4)",
            &ctx.file,
            ctx.end_line,
            0,
        )]);
    }
    let _ = ctx.end_is_last; // Haas: хвост → Raw (subs-тела §4.4), не Err
    if ctx.end_count == 0 {
        // SPEC §3 v0.16 / контракт §4.4: DNC-файлы часто без M30 — авто-
        // вставка ProgramEnd последним опом + notice.
        let op = Op::ProgramEnd {
            src: SrcSpan {
                file: ctx.file.clone(),
                line_start: line_count.max(1),
                col_start: 1,
                line_end: line_count.max(1),
                col_end: 1,
            },
        };
        ctx.push_keep(
            op,
            NoticeKind::MissingProgramEnd,
            "W_MISSING_PROGRAM_END: no M2/M30 — implicit program end appended (SPEC §3 v0.16; analysis §4.4)".into(),
        );
    }
    // Лимит max_ops_per_program (COVERAGE §1.2) — по итоговому ops+notices.
    let ops_n = (ctx.ops.len() + ctx.notices.len()) as f64;
    if ops_n > 200_000.0 {
        return Err(vec![limit_err(
            "max_ops_per_program",
            200_000.0,
            ops_n,
            format!("program yields {} ops, limit 200000", ops_n as u64),
            &ctx.file,
            1,
        )]);
    }
    let program = Program {
        // grammar v0.3 — magic-строка (SPEC §10.4).
        eid_version: "EID/L1 v0.3".to_string(),
        units: ctx.modal.units,
        // SPEC §3 v0.12: отсутствие G61/G64 → exact_path (дефолт);
        // G61 = exact_stop (value shift, COVERAGE §2.2).
        path_mode: ctx.modal.path.clone().unwrap_or(PathMode::ExactPath),
        frame_preset: ctx.preset.clone(), // G92 (этап 2)
        ops: std::mem::take(&mut ctx.ops),
    };
    let notices = std::mem::take(&mut ctx.notices);
    Ok(ParseOut { program, notices })
}

// ---------------------------------------------------------------------------
// process_block — диспетчер одного кадра
// ---------------------------------------------------------------------------

/// План кадра (приоритет Haas-групп: one-shot G4 > frame G92/G10 > цикл 09 >
/// движение 01).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Plan {
    Dwell,
    Frame,
    Cycle { repeat: bool }, // repeat = X|Y слова в кадре (триппер сверления)
    Motion,
    Noop,
}

#[allow(clippy::too_many_lines)]
fn process_block(ctx: &mut Ctx, b: &Block, is_last: bool) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    // (0) Хвост после program_end (§4.4 контракта): main + subs в одном
    // файле — норма Haas → line-level Raw{SYNTAX}, НЕ Err (зеркало Fanuc
    // ruling 1; осознанное расхождение с grbl n12); motion-опы после PEND
    // не строятся (SPEC §3). Хвост НЕ абсорбируется (программа структурно
    // закончена; G92 из sub-тела исказил бы frame_preset).
    if ctx.end_count > 0 {
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSyntax,
            "block after program end: subprogram tail preserved raw (analysis §4.4; COVERAGE §4 O-words)",
        );
        ctx.push(op);
        return Ok(());
    }

    // (1) Строковые маркеры.
    match &b.marker {
        Some(Marker::Percent) => return Ok(()), // IGNORED_SAFE (§1.3)
        Some(Marker::BlockDelete) => {
            // §1.3: исполнение зависит от переключателя станка —
            // RAW(E_RUNTIME_STATE), как LinuxCNC/Fanuc-строки COVERAGE §4.
            let op = ctx.raw_op(
                b,
                ErrorCode::RuntimeState,
                "block delete '/': execution depends on machine switch (analysis §1.3)",
            );
            ctx.push(op);
            return Ok(());
        }
        None => {}
    }

    // (2) Macro B (§4.3, COVERAGE §4): `#`-слова / `[ ]`-выражения /
    // letter-run statement'ы → весь блок line-level Raw{SYNTAX} (Haas Macro B
    // Fanuc-совместимый; параметрика = L2, этап 12; декомпозиция запрещена).
    // Система-переменные Haas ≠ Fanuc (#500–#699 non-volatile, #109xxx NGC
    // расширения) — в диагностике (бриф этапа 2, не молча).
    if b.macro_b {
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSyntax,
            "Macro B construct (#-variable / []-expression / IF-GOTO-WHILE-DO-END) — parametric, line-level Raw; Haas variable ranges differ from Fanuc (#500–#699 non-volatile vs #500–#999, #1000+ system vars and #109xxx NGC extensions numbered differently — cross-dialect macro transfer impossible without mapping table; analysis §4.3; COVERAGE §4)",
        );
        ctx.push(op);
        return Ok(());
    }

    // (3) Нарушение модальной группы (§1.1: один код на группу на блок) —
    // сломанный вход.
    if let Some((group, g)) = b.modal_violation {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            format!("modal group violation in block: {group:?} after {g:?}"),
            &ctx.file,
            line,
            blen,
        )]);
    }

    // (3a) Модальная абсорбция ДО любых raw-возвратов (SPEC §8.6 правило 6,
    // v0.17 — критический аудит #33): строка, уходящая в Raw целиком, обязана
    // абсорбировать frame-level состояние до push_raw — иначе следующий кадр
    // парсится в потерянной модальности (пробой аудита: `G91 G28 Z0` терял
    // G91 → следующий `G0 X10 Y10` молча абсолютный). Этап 2: группы 09
    // (циклы — Ctx.cycle), 10 (retract), 12 (wcs, включая G110–G112 с
    // notice).
    absorb_frame_state(ctx, b)?;

    // (4) G95 — детект для RAW (§2.2: feed/rev требует rpm, зависящего от
    // override; статическая эквивалентность не гарантируется; кандидат
    // ABSORB при статическом S — ревизия по корпусу).
    if b.g_codes.contains(&GCode::FeedPerRev) {
        absorb_frame_structs(ctx, b)?;
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSemantics,
            "G95 feed-per-rev: resolution needs spindle rpm (override-dependent) — not representable (analysis §2.2; COVERAGE §2.2)",
        );
        ctx.push(op);
        return Ok(());
    }

    // (5) G/M-коды вне enum: raw-таблица (фиксированные COVERAGE-коды §2/§3)
    // первым приоритетом, затем generic-ведро Raw{SYNTAX} + диагностика текста
    // (молчаливый пропуск запрещён, COVERAGE §2.7). SPEC §8.6: raw-motion-G
    // (G28/G31/G12/G70/G74/G84…) с осями — модальное состояние уже
    // абсорбировано в (3a); motion-режим и цикл НЕ продолжаются + notice
    // W_MODAL_ON_RAW.
    for w in &b.unsupported_words {
        if w.letter == 'G' {
            if let Some((code, reason)) = g_raw_disposition(w.num.value) {
                absorb_frame_structs(ctx, b)?;
                let t = (w.num.value * 10.0).round() as i64;
                let has_motion_words = ['X', 'Y', 'Z', 'I', 'J', 'K', 'R']
                    .iter()
                    .any(|l| b.word(*l).is_some());
                if has_motion_words && RAW_MOTION_G.contains(&t) {
                    ctx.modal.motion = None;
                    ctx.cycle = None;
                    let op = ctx.raw_op(b, code, reason);
                    ctx.push_keep(
                        op,
                        NoticeKind::ModalOnRaw,
                        format!(
                            "W_MODAL_ON_RAW: G{} — raw motion block with axis words; modal motion mode/cycle NOT continued (SPEC §8.6 rule 6, audit #33)",
                            w.num.text
                        ),
                    );
                    return Ok(());
                }
                let op = ctx.raw_op(b, code, reason);
                ctx.push(op);
                return Ok(());
            }
        }
    }
    if let Some(w) = b.unsupported_words.first() {
        let val = w.num.text.as_str();
        let reason = if w.letter == 'G' {
            format!(
                "G-code G{val}: outside Haas mill NGC dictionary (COVERAGE §2.7 generic bucket; not silent)"
            )
        } else {
            format!(
                "M-code M{val}: outside Haas mill NGC M dictionary (COVERAGE §3 generic bucket; not silent)"
            )
        };
        let op = ctx.raw_op(b, ErrorCode::UnsupportedSyntax, &reason);
        ctx.push(op);
        return Ok(());
    }

    // (6) Вендорские M-коды (этап 2, COVERAGE §3): документированные Haas —
    // Raw по классу (коллизии vs Fanuc — в диагностике, не молча).
    for m in &b.m_codes {
        let (code, reason) = match m {
            MCode::CoolantShower => (
                ErrorCode::UnsupportedSemantics,
                "M07 shower coolant — second coolant channel, NOT mist (Fanuc-M07 collision; state.coolant L1 = {mist, flood} — third channel not representable; COVERAGE §3)",
            ),
            MCode::Brake4Engage => (
                ErrorCode::UnsupportedSemantics,
                "M10 engage 4th-axis brake — axis mechanics (COVERAGE §3)",
            ),
            MCode::Brake4Release => (
                ErrorCode::UnsupportedSemantics,
                "M11 release 4th-axis brake — axis mechanics (COVERAGE §3)",
            ),
            MCode::Brake5Engage => (
                ErrorCode::UnsupportedSemantics,
                "M12 engage 5th-axis brake — axis mechanics (COVERAGE §3)",
            ),
            MCode::Brake5Release => (
                ErrorCode::UnsupportedSemantics,
                "M13 release 5th-axis brake — NOT spindle+coolant (Fanuc idiom refuted by research; number collision between dialects; COVERAGE §3)",
            ),
            MCode::SpindleOrient => (
                ErrorCode::UnsupportedSemantics,
                "M19 orient spindle — encoder-dependent (COVERAGE §3)",
            ),
            MCode::OutputRelay => (
                ErrorCode::UnsupportedSemantics,
                "M29 set output relay with M-Fin — NOT rigid-tap mode (Fanuc-M29 collision: Haas rigid tap = Setting 57 + G84 internally, no M29; COVERAGE §3)",
            ),
            MCode::SubCallLocal => (
                ErrorCode::UnsupportedSyntax,
                "M97 local subprogram call (P = N-number in same file) — Haas-specific subs mechanics (analysis §4.2; COVERAGE §3)",
            ),
            MCode::SubCall => (
                ErrorCode::UnsupportedSyntax,
                "M98 subprogram call (P = O-number, repeat L) — subs mechanics (analysis §4.2; COVERAGE §3)",
            ),
            MCode::SubReturn => (
                ErrorCode::UnsupportedSyntax,
                "M99 subprogram return / main loop — not static (Setting 118 `?`; analysis §4.2; COVERAGE §3)",
            ),
            MCode::JumpLine => (
                ErrorCode::UnsupportedSyntax,
                "M46 jump to line — control flow (exact semantics `?` registry §10(12); COVERAGE §3)",
            ),
            MCode::JumpIfNoInput => (
                ErrorCode::UnsupportedSyntax,
                "M96 jump if no input (P = line, Q = input) — conditional control flow (COVERAGE §3)",
            ),
            MCode::InteractiveInput => (
                ErrorCode::RuntimeState,
                "M109 interactive user input — waits for operator (runtime state; COVERAGE §3)",
            ),
            _ => continue,
        };
        absorb_frame_structs(ctx, b)?;
        let op = ctx.raw_op(b, code, reason);
        ctx.push(op);
        return Ok(());
    }

    // (7) G52 → жёсткий отказ (ERR, SPEC §8 правило 5; 2-й вендорский ERR
    // после Fanuc — COVERAGE §2.3): позиционная смена фрейма mid-program —
    // raw-сохранение исказило бы интерпретацию последующих ходов. Haas-
    // усложнения (общий регистр с G92 `?`, Setting 129/33 `?`) — консервативно
    // ERR независимо от Setting.
    if b.g_codes.contains(&GCode::LocalCs) {
        return Err(vec![err(
            ErrorCode::UnsupportedSemantics,
            "G52 local coordinate system shift — mid-program frame change, hard ERR class (SPEC §8 rule 5; 2nd vendor hard-reject; shared G92/G52 register `?`, Setting 129/33 `?`; COVERAGE §2.3)",
            &ctx.file, line, blen,
        )]);
    }

    // (8) G92 S → Raw{SEMANTICS} — кламп-формы в словаре Haas mill НЕТ
    // (кламп оборотов — Setting, `?`); S при G92 — не frame_preset (§1.3
    // отчёта; парсер обязан различать, COVERAGE §2.6).
    if b.g_codes.contains(&GCode::Preset) && b.word('S').is_some() {
        absorb_frame_structs(ctx, b)?; // с осевыми — пресетка абсорбирована
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSemantics,
            "G92 S spindle clamp — form does not exist in Haas mill vocabulary (clamp is a Setting, `?`); S word at G92 — not frame_preset (analysis §1.3; COVERAGE §2.6)",
        );
        ctx.push(op);
        return Ok(());
    }

    // (9) G154 Pn: P1–P3 → слоты #7–#9 = p=7..9 (абсорбировано в (3a) с
    // notice W_WCS_EXTENDED); без P — сломанный вход (Err); P вне 1..3 →
    // Raw{SEMANTICS} (слоты #4+… вне EID p-домена).
    if b.g_codes.contains(&GCode::Wcs154) {
        match b.word('P') {
            None => {
                return Err(vec![err(
                    ErrorCode::InvalidInput,
                    "G154 without P word — select work coordinates P1–P99 (extended slots P1–P3 = G110–G112 = EID p7..9; COVERAGE §2.3)",
                    &ctx.file, line, blen,
                )]);
            }
            Some(w) => {
                let pi = w.value as i64;
                if w.value.fract() != 0.0 || !(1..=3).contains(&pi) {
                    absorb_frame_structs(ctx, b)?;
                    let op = ctx.raw_op(
                        b,
                        ErrorCode::UnsupportedSemantics,
                        "G154 Pn outside extended-slot domain 1..3 (P1–P3 = slots #7–#9 = EID p=7..9; P4+ = slots #10+ outside EID p-domain; COVERAGE §2.3)",
                    );
                    ctx.push(op);
                    return Ok(());
                }
            }
        }
    }

    // (10) План кадра (приоритет: one-shot G4 > frame > 09 > 01).
    let has_axis = b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some();
    let has_arc_words = b.word('I').is_some()
        || b.word('J').is_some()
        || b.word('K').is_some()
        || b.word('R').is_some()
        || b.word('C').is_some(); // C в G01 = фаска (Haas-специфика, §2.1)
    let explicit_motion = b.motion();
    let explicit_cycle = b.g_codes.iter().copied().find_map(CycleKind::from_gcode);
    // Триппер сверления (§5.2 отчёта): G90 — X/Y-слово; G91 — X/Y/Z (Z-шаг
    // между отверстиями); блок с только R/Q/P/L/F — обновление параметров.
    let repeat_trigger = b.word('X').is_some()
        || b.word('Y').is_some()
        || (!ctx.modal.dist_abs && b.word('Z').is_some());
    let plan = if b.g_codes.contains(&GCode::Dwell) {
        Plan::Dwell
    } else if b.g_codes.contains(&GCode::Preset) || b.g_codes.contains(&GCode::DataRecord) {
        Plan::Frame
    } else if explicit_cycle.is_some() {
        Plan::Cycle {
            repeat: repeat_trigger,
        }
    } else if ctx.cycle.is_some() && explicit_motion.is_none() {
        // Модальность группы 09: X/Y(Z в G91)-блок без G8x = повтор (§5.1).
        Plan::Cycle {
            repeat: repeat_trigger,
        }
    } else if explicit_motion.is_some()
        || (ctx.modal.motion.is_some() && (has_axis || has_arc_words))
    {
        Plan::Motion
    } else if has_axis || has_arc_words {
        // Голые координаты без активного motion-кода — сломанный вход
        // (§1.3 «легальны — продолжение группы 01 `?`»: продолжение при
        // активном режиме выше; без режима — отказ, паритет Fanuc §2.7).
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "naked coordinate words: no active motion mode (analysis §1.3 `?`)",
            &ctx.file,
            line,
            blen,
        )]);
    } else {
        Plan::Noop
    };
    let cycle_kind = match plan {
        Plan::Cycle { .. } => explicit_cycle.or_else(|| ctx.cycle.as_ref().map(|c| c.kind)),
        _ => None,
    };
    let motion_mode = explicit_motion.or(ctx.modal.motion);

    // (11) Слова без потребителя — молчаливый пропуск запрещён (COVERAGE §4).
    // G01 C/R (фаска/скругление) → Raw{SEMANTICS} (§2.1); context-слова
    // (P/Q/L/I/J/K/R) без контекста — сломанный вход (Err).
    if let Some((code, reason)) = unused_word_diagnostic(b, plan, motion_mode) {
        if code == ErrorCode::InvalidInput {
            return Err(vec![err(code, reason, &ctx.file, line, blen)]);
        }
        let op = ctx.raw_op(b, code, &reason);
        ctx.push(op);
        return Ok(());
    }

    // (12) Cancel-коды семейств (COVERAGE §1.3, HAAS-СОСТАВЫ): G40/G49 —
    // IGNORED_SAFE, если активных кодов семейства в файле нет; иначе Raw
    // (raw-наследование — иначе round-trip оставил бы компенсацию активной
    // навсегда). H00-cancel `?` не реализован (H-слово → таблица, §2.4).
    let mut family_cancel: Option<(ErrorCode, &'static str)> = None;
    for g in &b.g_codes {
        family_cancel = match g {
            GCode::CompCancel if ctx.family_g40 => Some((
                ErrorCode::UnsupportedSemantics,
                "G40 cancel with active G41/G42/G141 family in file — family rule, Haas composition (COVERAGE §1.3)",
            )),
            GCode::LenCompCancel if ctx.family_g49 => Some((
                ErrorCode::RequiresMachineTable,
                "G49 cancel with active G43/G44/G143 family in file — family rule, Haas composition (COVERAGE §1.3)",
            )),
            _ => None,
        };
        if family_cancel.is_some() {
            break;
        }
    }
    if let Some((code, reason)) = family_cancel {
        let op = ctx.raw_op(b, code, reason);
        ctx.push(op);
        return Ok(());
    }

    // (13) G92 → frame_preset: только пресетка до первого motion-опа
    // (позиционное ограничение §3 SPEC); второй G92 — отказ (Q5).
    if b.g_codes.contains(&GCode::Preset) {
        if ctx.saw_motion {
            return Err(vec![err(
                ErrorCode::UnsupportedSemantics,
                "G92 after motion: frame_preset only as startup preset (SPEC §3; Q4/Q5)",
                &ctx.file,
                line,
                blen,
            )]);
        }
        if ctx.preset.is_some() {
            return Err(vec![err(
                ErrorCode::UnsupportedSemantics,
                "second G92 — multiple presets in one program (Q5; SPEC §3)",
                &ctx.file,
                line,
                blen,
            )]);
        }
        if !has_axis {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                "G92 without axis words (SPEC §3)",
                &ctx.file,
                line,
                blen,
            )]);
        }
        // Значения = координаты, присваиваемые текущей позиции (§3); невыписанные
        // оси = дефолт контроллера 0 (точка разрыва (a) — старт программы).
        let mut vals = [0.0f64; 3];
        for (i, letter) in ['X', 'Y', 'Z'].into_iter().enumerate() {
            if let Some(w) = b.word(letter) {
                vals[i] = ctx.dpi_num(letter, w).value();
            }
        }
        ctx.preset = Some(FramePreset {
            values: p3(vals),
            wcs: ctx.modal.wcs,
        });
        return Ok(());
    }

    // (14) G10: L2 P1–P6 → wcs_define (G90/G91 — абсолют/инкремент записи,
    // §5.3 SPEC); **L2 P0 → Raw{RT} — запись слота G52 (≠ Fanuc external
    // offset)**; **L20 → Raw{RT} — запись origin G110–G129/G154, НЕ
    // wcs_set_here (ловушка №2b)**; L1/L10–L13 → Raw{RT} (таблицы
    // инструментов); сломанные формы (без L / L2 без P / P вне домена) —
    // Err(INVALID)/Raw(SYNTAX).
    if b.g_codes.contains(&GCode::DataRecord) {
        let l = b.word('L').map(|n| n.value);
        let p = b.word('P').map(|n| n.value);
        match l {
            None => {
                return Err(vec![err(
                    ErrorCode::InvalidInput,
                    "G10 without L word (COVERAGE §2.3)",
                    &ctx.file,
                    line,
                    blen,
                )]);
            }
            Some(lv) if lv.fract() != 0.0 => {
                let op = ctx.raw_op(
                    b,
                    ErrorCode::UnsupportedSyntax,
                    "G10 L non-integer — broken form (COVERAGE §2.3)",
                );
                ctx.push(op);
                return Ok(());
            }
            Some(lv) if lv as i64 == 20 => {
                let op = ctx.raw_op(
                    b,
                    ErrorCode::RequiresMachineTable,
                    "G10 L20 Haas semantics = origin write for extended WCS G110–G129 (P1–P20) / G154 Pn (P1–P99) from home — NOT wcs_set_here (trap #2b, Fanuc-строка правило применимо дословно; COVERAGE §2.3)",
                );
                ctx.push(op);
                return Ok(());
            }
            Some(lv) if lv as i64 != 2 => {
                let op = ctx.raw_op(
                    b,
                    ErrorCode::RequiresMachineTable,
                    "G10 L tool-offset table record (L1/L11 wear-H, L10 geometry-H, L12 diameter-D, L13 diameter-wear; R = value) — machine table data (COVERAGE §2.3)",
                );
                ctx.push(op);
                return Ok(());
            }
            Some(_) => {
                // L2: P обязателен; домен Haas: P1–P6 = G54–G59, P0 = слот G52.
                let pv = p.ok_or_else(|| {
                    vec![err(
                        ErrorCode::InvalidInput,
                        "G10 L2 without P word (COVERAGE §2.3)",
                        &ctx.file,
                        line,
                        blen,
                    )]
                })?;
                let pi = pv as i64;
                if pv.fract() != 0.0 || !(0..=6).contains(&pi) {
                    let op = ctx.raw_op(
                        b,
                        ErrorCode::UnsupportedSyntax,
                        "G10 L2 P outside Haas wcs domain (P1–P6 = G54–G59, P0 = G52 slot; extended origins = L20; COVERAGE §2.3)",
                    );
                    ctx.push(op);
                    return Ok(());
                }
                if pi == 0 {
                    // ≠ Fanuc P0 (external offset): у Haas L2 P0 = запись
                    // work-coordinate слота G52 — сдвиг поверх активной WCS,
                    // понятие вне L1 (связан с ERR-строкой G52).
                    let op = ctx.raw_op(
                        b,
                        ErrorCode::RequiresMachineTable,
                        "G10 L2 P0 writes the G52 work-coordinate slot (shift over active WCS) — NOT the Fanuc external/common offset (Haas UK: «P0 G52 references work coordinate»; concept outside L1; COVERAGE §2.3)",
                    );
                    ctx.push(op);
                    return Ok(());
                }
                // Partial G10 L2 (не все X/Y/Z) → Raw{REQUIRES}: невыписанные
                // оси Haas сохраняет неизменными — значение из таблицы станка,
                // вне текста (паритет Fanuc-строки).
                if b.word('X').is_none() || b.word('Y').is_none() || b.word('Z').is_none() {
                    let op = ctx.raw_op(
                        b,
                        ErrorCode::RequiresMachineTable,
                        "partial G10 L2 — unspecified axes keep their machine-table value, outside text (COVERAGE §2.3)",
                    );
                    ctx.push(op);
                    return Ok(());
                }
                // Полная форма L2 honors G90/G91; старый origin отсутствует →
                // 0 (инкрементальная запись, §5.3; интерпретация — паритет
                // LinuxCNC-адаптера).
                let idx = (pi - 1) as usize;
                let old = ctx.origins[idx].unwrap_or([0.0; 3]);
                let mut origin = old;
                for (i, letter) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                    if let Some(w) = b.word(letter) {
                        let v = ctx.dpi_num(letter, w).value();
                        origin[i] = if ctx.modal.dist_abs { v } else { old[i] + v };
                    }
                }
                ctx.origins[idx] = Some(origin);
                let op = Op::WcsDefine {
                    p: pi as u8,
                    origin: p3(origin),
                    state: ctx.state(),
                    src: span(&ctx.file, line, blen),
                };
                ctx.push(op);
                return Ok(());
            }
        }
    }

    // (15) G4 dwell (§2.7/§1.2): P-DOT RULE — точка = секунды, без точки =
    // миллисекунды + notice; X/U-форм dwell у Haas НЕТ (отсекается в (11)
    // как слово без потребителя). `G04 P0` → drop+notice (SPEC §5.2).
    if plan == Plan::Dwell {
        return dwell_block(ctx, b);
    }

    // (16) M6/M16 → ToolChange (+T в блоке; идиома `T01 M06`); M6 без T —
    // сломанный вход. Одиночное T отсечено в (11) → Raw{SEMANTICS}.
    if b.m_codes.contains(&MCode::ToolChange) || b.m_codes.contains(&MCode::ToolChangeAlt) {
        let tw = b.word('T').ok_or_else(|| {
            vec![err(
                ErrorCode::InvalidInput,
                "M6/M16 without T word (COVERAGE §3; Haas idiom T..M06)",
                &ctx.file,
                line,
                blen,
            )]
        })?;
        if tw.value.fract() != 0.0 || tw.value < 0.0 || tw.value > 9999.0 {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                format!("T{} outside integer tool-number domain 0..9999", tw.text),
                &ctx.file,
                line,
                blen,
            )]);
        }
        let t = tw.value as u32;
        ctx.tool = Some(t);
        let op = Op::ToolChange {
            tool: t,
            state: ctx.state(),
            src: span(&ctx.file, line, blen),
        };
        ctx.push(op);
    }

    // (17) M0/M1 → ProgramStop (optional=false/true; Q8).
    for m in &b.m_codes {
        if matches!(m, MCode::ProgramStop | MCode::OptionalStop) {
            let op = Op::ProgramStop {
                optional: *m == MCode::OptionalStop,
                state: ctx.state(),
                src: span(&ctx.file, line, blen),
            };
            ctx.push(op);
        }
    }

    // (18) Движение / цикл.
    match plan {
        Plan::Cycle { repeat } => {
            cycle_block(ctx, b, cycle_kind.expect("cycle plan → kind"), repeat)?;
        }
        Plan::Motion => {
            motion_block(ctx, b)?;
        }
        _ => {}
    }

    // (19) M2/M30 → ProgramEnd (после motion — порядок grbl #9; SPEC §5.2).
    if b.m_codes.contains(&MCode::ProgramEnd) {
        ctx.mark_end(line, is_last, blen);
    }
    Ok(())
}

fn modal_group_is_motion(g: GCode) -> bool {
    matches!(
        g,
        GCode::Rapid | GCode::Linear | GCode::ArcCw | GCode::ArcCcw
    )
}

// ---------------------------------------------------------------------------
// Модальная абсорбция на RAW-строках (SPEC §8.6 правило 6, v0.17 — аудит #33)
// ---------------------------------------------------------------------------

/// Абсорбция frame-level состояния кадра (SPEC §8.6 правило 6). Вызывается ДО
/// любых raw-возвратов: строка, уходящая в Raw целиком, обязана абсорбировать
/// модальные группы (plane/distance/arcdist/feedmode/units/wcs/retract/path),
/// F/S (spindle, last_s), M3/M4/M5, M8/M9 — иначе следующий кадр парсится в
/// потерянной модальности (пробой аудита: `G91 G28 Z0` терял G91 → следующий
/// `G0 X10 Y10` молча абсолютный — точное нарушение правила).
/// Этап 2: явный код группы 01 отменяет активный цикл; G80 — отмена cycle-
/// моды; G98/G99 — retract-режим; WCS-выбор (включая G110–G112 с notice
/// W_WCS_EXTENDED и G154 P1–P3).
///
/// selected_tool (голое T) — вне абсорбции: в State L1 поля нет (только
/// физический tool через M6), абсорбция исказила бы tool-снапшоты; одиночное
/// T остаётся Raw{SEMANTICS} (candidate `selected_tool`, этап 12).
///
/// Хвост после PEND — НЕ абсорбируется (process_block (0)): программа
/// структурно закончена, G92 из sub-тела исказил бы frame_preset.
fn absorb_frame_state(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    // (a) Единицы: ровно одна система на программу (SPEC §2.5) — и на
    // RAW-кадре (§8.5: смена units — структурный класс, жёсткий отказ).
    for g in &b.g_codes {
        if let Some(u) = g.units() {
            if u != ctx.modal.units {
                if ctx.modal.units_established {
                    return Err(vec![err(
                        ErrorCode::UnsupportedSemantics,
                        "unit switch mid-program: one unit system per program (SPEC §2.5)",
                        &ctx.file,
                        line,
                        blen,
                    )]);
                }
                ctx.modal.units = u;
            }
            ctx.modal.units_established = true;
        }
    }

    // (b) ABSORB модальных групп (01–06, 10, 12; units выше). Явный код
    // группы 01 отменяет активный цикл (группа 09); G80 — отмена cycle-моды.
    for g in &b.g_codes {
        if modal_group_is_motion(*g) || *g == GCode::CycleCancel {
            ctx.cycle = None;
        }
        ctx.modal.apply(*g);
    }

    // (b2) Расширенные WCS-слоты (этап 2, бриф): G110–G112 → p=7..9 с notice
    // W_WCS_EXTENDED (нативная симметрия слотов #7–#9; эмиссия этапа 3 —
    // G110–G112 с notice); G154 P1–P3 → p=7..9 с тем же notice (P1 = слот #7).
    // COVERAGE Haas-колонка v0.9 фиксирует RAW консервативно — расхождение
    // колонки и брифа этапа 2 документировано notice'ом, не молча.
    for g in &b.g_codes {
        let (code_text, wcs) = match g {
            GCode::Wcs110 => ("G110", Wcs::G59_1),
            GCode::Wcs111 => ("G111", Wcs::G59_2),
            GCode::Wcs112 => ("G112", Wcs::G59_3),
            GCode::Wcs154 => {
                // Контекстен по P: P1–P3 = алиасы G110–G112.
                let p_ok = b
                    .word('P')
                    .map(|w| w.value.fract() == 0.0 && (1..=3).contains(&(w.value as i64)))
                    .unwrap_or(false);
                if !p_ok {
                    continue; // P вне 1..3 — Raw/Err путь в (9)
                }
                let pi = b.word('P').expect("p_ok проверено").value as usize;
                (
                    "G154",
                    [Wcs::G59_1, Wcs::G59_2, Wcs::G59_3][pi - 1],
                )
            }
            _ => continue,
        };
        let slot = wcs_idx(wcs) + 1; // p-номер слота EID (7..9)
        ctx.modal.wcs = wcs;
        ctx.note_pending(
            NoticeKind::WcsExtended,
            format!(
                "W_WCS_EXTENDED: {code_text} selects Haas extended slot #{slot} → EID wcs p{slot} ({wcs:?}) — native slot-numbering symmetry; stage-3 emission will emit {code_text} with this notice (COVERAGE Haas column v0.9 records G110–G129/G154 RAW conservatively; stage-2 brief maps slots #7–#9 onto p=7..9)"
            ),
        );
    }

    // (c) Path-режим (группа 15): глобален (§3); переключение — отказ (и на
    // RAW-кадре — метаданные программы).
    for g in &b.g_codes {
        if let Some(pm) = g.path_mode() {
            match &ctx.modal.path {
                None => ctx.modal.path = Some(pm),
                Some(old) if *old != pm => {
                    return Err(vec![err(
                        ErrorCode::UnsupportedSemantics,
                        "path mode switch mid-program — one mode per program (SPEC §3)",
                        &ctx.file,
                        line,
                        blen,
                    )]);
                }
                _ => {} // повтор того же режима — легальный no-op
            }
        }
    }

    // (d) F — модальная подача (HAAS-правило: bare F → × 0.0001 Setting 77 +
    // notice; notice прицепится к raw-опу через pending). S — обороты,
    // КРОМЕ G92-кадра (S-кламп — другой адрес-смысл, §1.3: шаг (8) даёт raw;
    // абсорбция исказила бы шпиндель).
    if let Some(f) = b.word('F') {
        let num = ctx.feed_num(f);
        ctx.feed = Some(num);
    }
    if !b.g_codes.contains(&GCode::Preset) {
        if let Some(s) = b.word('S') {
            let num = verbatim(s);
            match ctx.spindle.clone() {
                Spindle::Off => ctx.last_s = Some(num),
                Spindle::Cw(_) => ctx.spindle = Spindle::Cw(num),
                Spindle::Ccw(_) => ctx.spindle = Spindle::Ccw(num),
            }
        }
    }

    // (e) M3/M4/M5 → шпиндель; M8/M9 → СОЖ (M07 Shower — Raw-путь в (6),
    // НЕ mist — второй канал; венторские M10–M13/M19/M29 — тоже (6)).
    for m in &b.m_codes {
        match m {
            MCode::SpindleCw | MCode::SpindleCcw => {
                let n = match b.word('S') {
                    Some(s) => verbatim(s),
                    None => ctx.last_s.clone().unwrap_or_else(|| num_f64(0.0)),
                };
                ctx.spindle = if *m == MCode::SpindleCw {
                    Spindle::Cw(n)
                } else {
                    Spindle::Ccw(n)
                };
            }
            MCode::SpindleOff => ctx.spindle = Spindle::Off,
            MCode::CoolantFlood => ctx.coolant.flood = true,
            MCode::CoolantOff => ctx.coolant = Coolant::default(),
            _ => {}
        }
    }

    // (f)/(g) — G92/G10 — в absorb_frame_structs: вызывается ТОЛЬКО на
    // raw-возвратах (двойная обработка структурного пути недопустима:
    // пресетка «second G92» и задвоение wcs_define).
    Ok(())
}

/// Структурная часть абсорбции (SPEC §8.6: G92/G10 в списке абсорбции) —
/// вызывается ТОЛЬКО перед raw-возвратами. Легальный путь обрабатывает
/// G92/G10 в основных шагах (13)/(14); G10-обновление таблицы idempotent
/// (те же слова, та же модальность → то же значение), G92 — нет, поэтому
/// его абсорбция отделена от общего пути.
fn absorb_frame_structs(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    // (f) G92 → пресетка (до первого motion-опа, одна пресетка — инварианты
    // основного пути; на raw-кадре осевые слова абсорбируются в frame_preset,
    // S-кламп отсечён условием «есть осевые»).
    if b.g_codes.contains(&GCode::Preset)
        && (b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some())
    {
        if ctx.saw_motion {
            return Err(vec![err(
                ErrorCode::UnsupportedSemantics,
                "G92 after motion: frame_preset only as startup preset (SPEC §3; Q4/Q5)",
                &ctx.file,
                line,
                blen,
            )]);
        }
        if ctx.preset.is_some() {
            return Err(vec![err(
                ErrorCode::UnsupportedSemantics,
                "second G92 — multiple presets in one program (Q5; SPEC §3)",
                &ctx.file,
                line,
                blen,
            )]);
        }
        let mut vals = [0.0f64; 3];
        for (i, letter) in ['X', 'Y', 'Z'].into_iter().enumerate() {
            if let Some(w) = b.word(letter) {
                vals[i] = ctx.dpi_num(letter, w).value();
            }
        }
        ctx.preset = Some(FramePreset {
            values: p3(vals),
            wcs: ctx.modal.wcs,
        });
    }

    // (g) G10 L2 P1–6 ПОЛНАЯ форма → обновление таблицы origin (§8.6: G10 в
    // списке абсорбции). RAW-формы (L20/L1/P0/partial/сломанные) таблицу не
    // трогают — значение из машины, вне текста. WcsDefine-оп на raw-кадре НЕ
    // строится (кадр представлен Raw-опом; обновление — внутреннее состояние
    // связывания последующих ходов).
    if b.g_codes.contains(&GCode::DataRecord) {
        if let (Some(lv), Some(pv)) = (b.word('L').map(|n| n.value), b.word('P').map(|n| n.value)) {
            let pi = pv as i64;
            if lv == 2.0
                && lv.fract() == 0.0
                && pv.fract() == 0.0
                && (1..=6).contains(&pi)
                && b.word('X').is_some()
                && b.word('Y').is_some()
                && b.word('Z').is_some()
            {
                let idx = (pi - 1) as usize;
                let old = ctx.origins[idx].unwrap_or([0.0; 3]);
                let mut origin = old;
                for (i, letter) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                    if let Some(w) = b.word(letter) {
                        let v = ctx.dpi_num(letter, w).value();
                        origin[i] = if ctx.modal.dist_abs { v } else { old[i] + v };
                    }
                }
                ctx.origins[idx] = Some(origin);
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Слова без потребителя (молчаливый пропуск запрещён, COVERAGE §4)
// ---------------------------------------------------------------------------

/// Feature-слова вне скоупа → (RAW-код, причина); context-слова без контекста →
/// (E_INVALID_INPUT, причина) — сломанный вход (паритет Fanuc/LinuxCNC).
/// HAAS-СПЕЦИФИКА (§2.1): C/R в G01 = фаска/скругление угла (lookahead-
/// декомпозиция, этап 12) → Raw{SEMANTICS}, НЕ InvalidInput.
/// Dwell-план потребляет ТОЛЬКО P (X/U-форм dwell у Haas НЕТ — §1.2).
/// K — НЕ повтор цикла: потребляется только пек-планом (G83 min-пек) или
/// дугой (K = компонент центра); в прочих планах — слово без потребителя.
fn unused_word_diagnostic(
    b: &Block,
    plan: Plan,
    motion_mode: Option<GCode>,
) -> Option<(ErrorCode, String)> {
    let mut consumed: Vec<char> = vec!['F', 'S'];
    match plan {
        // X/U тоже «потреблены» планом — специализированный отказ в
        // dwell_block (формы не существуют, Err InvalidInput, §1.2).
        Plan::Dwell => consumed.extend(['P', 'X', 'U']),
        Plan::Frame => {
            consumed.extend(['X', 'Y', 'Z']);
            if b.g_codes.contains(&GCode::DataRecord) {
                // P = слот/номер, L = форма записи, R = значение tool-offset
                // форм (G10 L10 G90 P5 R250. — официальный пример, COVERAGE
                // §2.3).
                consumed.extend(['P', 'L', 'R']);
            }
            if b.g_codes.contains(&GCode::Wcs154) {
                consumed.push('P');
            }
        }
        Plan::Cycle { .. } => consumed.extend(['X', 'Y', 'Z', 'R', 'Q', 'P', 'I', 'J', 'K', 'L']),
        Plan::Motion => {
            consumed.extend(['X', 'Y', 'Z']);
            if matches!(motion_mode, Some(GCode::ArcCw) | Some(GCode::ArcCcw)) {
                consumed.extend(['I', 'J', 'K', 'R']);
            }
        }
        Plan::Noop => {}
    }
    if b.g_codes.contains(&GCode::Wcs154) {
        consumed.push('P');
    }
    if b.m_codes.contains(&MCode::ToolChange) || b.m_codes.contains(&MCode::ToolChangeAlt) {
        consumed.push('T');
    }
    for letter in ['T', 'H', 'D', 'U', 'W', 'A', 'B', 'C', 'E', 'O', 'P', 'Q', 'R', 'I', 'J', 'K', 'L']
    {
        if b.word(letter).is_none() || consumed.contains(&letter) {
            continue;
        }
        // G01 C/R — фаска/скругление угла (Haas-специфика §2.1): R в G01 —
        // радиус скругления (контекстное слово), C — фаска; lookahead-деко-
        // мпозиция следующего хода — этап 12 → Raw{SEMANTICS}.
        if matches!(plan, Plan::Motion)
            && motion_mode == Some(GCode::Linear)
            && (letter == 'C' || letter == 'R')
        {
            return Some((
                ErrorCode::UnsupportedSemantics,
                format!(
                    "{letter} word in G01 — corner chamfer/rounding between this and next move, needs lookahead decomposition (analysis §2.1; stage-12 candidate)"
                ),
            ));
        }
        return Some(match letter {
            'T' => (
                ErrorCode::UnsupportedSemantics,
                "T word alone — modal next-tool preload (SMTC), outside L1; candidate `selected_tool` (COVERAGE §3)".to_string(),
            ),
            'H' | 'D' => (
                ErrorCode::RequiresMachineTable,
                format!("{letter} offset number — machine table value (COVERAGE §4 H/D words)"),
            ),
            'U' | 'W' => (
                ErrorCode::UnsupportedSemantics,
                format!("{letter} word — Setting 162 mentions U/W (lathe legacy `?`), mill vocabulary stage 12 (COVERAGE §4; registry §10(22))"),
            ),
            'A' | 'B' => (
                ErrorCode::UnsupportedSemantics,
                format!("{letter} rotary axis — stage 12 (COVERAGE §4)"),
            ),
            // C — ротационная ось ИЛИ фаска G01 (обработано выше); вне G01 —
            // ротационная ось.
            'C' => (
                ErrorCode::UnsupportedSemantics,
                "C rotary axis — stage 12 (COVERAGE §4; C in G01 context = chamfer, handled above)".to_string(),
            ),
            'E' => (
                ErrorCode::UnsupportedSyntax,
                "E word (G187 accuracy) — vendor smoothness mode, stage-12 candidate (COVERAGE §2.7)".to_string(),
            ),
            'O' => (
                ErrorCode::UnsupportedSyntax,
                "O word — program number / subprogram structure, outside L1 (COVERAGE §4)".to_string(),
            ),
            _ => (
                ErrorCode::InvalidInput,
                format!("{letter} word without consumer in block (arc/cycle/G10/G4-P context expected; K is NOT a cycle repeat on Haas — L is)"),
            ),
        });
    }
    None
}

// ---------------------------------------------------------------------------
// G4 dwell (P-DOT RULE: точка = сек / без = мс; X/U-форм НЕТ)
// ---------------------------------------------------------------------------

fn dwell_block(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let p = b.word('P');
    if p.is_none() {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "G4 without dwell word P (Haas P-dot dwell; X/U dwell forms do not exist — analysis §1.2)",
            &ctx.file,
            line,
            blen,
        )]);
    }
    // X/U-форм dwell у Haas НЕТ (§1.2: fallback Fanuc-G04-X недоступен) —
    // X/U в G4-кадре не является dwell-словом → молчаливый пропуск
    // запрещён (COVERAGE §4).
    if b.word('X').is_some() || b.word('U').is_some() {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "G4 with X/U word — X/U dwell forms do not exist in Haas vocabulary (P word only, P-dot rule; analysis §1.2)",
            &ctx.file,
            line,
            blen,
        )]);
    }
    // P-dot rule: dotted P = секунды verbatim; bare P = миллисекунды
    // (+ notice W_DPI_ASSUMED — DPI-домен мс-гранулярности).
    let seconds = ctx.dwell_seconds(p.expect("P проверено выше"));
    if seconds < 0.0 {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "negative dwell (SPEC §5.2)",
            &ctx.file,
            line,
            blen,
        )]);
    }
    if seconds == 0.0 {
        let op = Op::Dwell {
            seconds: num_f64(0.0),
            state: ctx.state(),
            src: span(&ctx.file, line, blen),
            sub: None,
        };
        ctx.push_drop(
            op,
            NoticeKind::ZeroLengthDwell,
            "zero dwell dropped (seconds == 0, SPEC §5.2)".into(),
        );
        return Ok(());
    }
    let op = Op::Dwell {
        seconds: num_f64(seconds),
        state: ctx.state(),
        src: span(&ctx.file, line, blen),
        sub: None,
    };
    ctx.push(op);
    Ok(())
}

// ---------------------------------------------------------------------------
// Движение (G0–G3 / модальное продолжение; G53 machine-frame §5.3)
// ---------------------------------------------------------------------------

/// Целевая точка хода: слова (G90 abs / G91 inc) + база связывания.
/// SPEC §2 v0.11: в точке разрыва (a) отсутствующие оси end наследуют дефолт
/// контроллера (0); точки разрыва (b)/(d) и machine-ходы — частичный end
/// наследует неизвестное → E_RUNTIME_STATE; machine-ходы — только полные end
/// (v0.13: нуль в машинных координатах не наследуется).
fn resolve_end(
    ctx: &Ctx,
    b: &Block,
    base: Option<[f64; 3]>,
    machine: bool,
) -> Result<Point3, Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    if machine && (b.word('X').is_none() || b.word('Y').is_none() || b.word('Z').is_none()) {
        return Err(vec![err(
            ErrorCode::RuntimeState,
            "machine-frame move requires full end — zero in machine coordinates is not inherited (SPEC §2 v0.13)",
            &ctx.file, line, blen,
        )]);
    }
    let mut v: [Option<Num>; 3] = std::array::from_fn(|_| None);
    for (i, w) in [(0usize, 'X'), (1, 'Y'), (2, 'Z')] {
        v[i] = match b.word(w) {
            Some(n) => {
                let num = ctx.dpi_num(w, n);
                if ctx.modal.dist_abs {
                    Some(num)
                } else {
                    let base = base.ok_or_else(|| {
                        vec![err(
                            ErrorCode::RuntimeState,
                            "incremental move requires known current position (SPEC §2)",
                            &ctx.file,
                            line,
                            blen,
                        )]
                    })?;
                    Some(num_f64(base[i] + num.value()))
                }
            }
            None => match base {
                Some(bb) => Some(num_f64(bb[i])),
                None => {
                    return Err(vec![err(
                        ErrorCode::RuntimeState,
                        "partial end inherits unknown at frame break point (SPEC §2)",
                        &ctx.file,
                        line,
                        blen,
                    )]);
                }
            },
        };
    }
    match (v[0].take(), v[1].take(), v[2].take()) {
        (Some(x), Some(y), Some(z)) => Ok(Point3 { x, y, z }),
        _ => unreachable!("end fully resolved: base known or all axes explicit"),
    }
}

fn motion_block(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let machine = b.g_codes.contains(&GCode::MachineCs);

    let has_axis = b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some();
    let has_arc_words = b.word('I').is_some()
        || b.word('J').is_some()
        || b.word('K').is_some()
        || b.word('R').is_some();
    if !has_axis && !has_arc_words {
        return Ok(()); // кадр без целевых слов — движения нет (`G53;` — no-op)
    }

    // G53: явный G0/G1 обязателен в кадре; дуги в machine-frame невыразимы.
    let explicit = b.motion();
    if machine {
        if matches!(explicit, Some(GCode::ArcCw) | Some(GCode::ArcCcw)) {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                "G53 machine-frame arc — G53 is rapid/linear only (SPEC §5.3)",
                &ctx.file,
                line,
                blen,
            )]);
        }
        if !matches!(explicit, Some(GCode::Rapid) | Some(GCode::Linear)) {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                "G53 requires explicit G0/G1 in the block (analysis §2.3; SPEC §5.3)",
                &ctx.file,
                line,
                blen,
            )]);
        }
    }

    let linked = ctx.link_start(machine, line, blen)?;
    let base = match linked {
        Linked::Known(v) => Some(v),
        Linked::UnknownAtStart => {
            if machine {
                None
            } else {
                Some([0.0; 3]) // дефолт контроллера (§2 v0.11)
            }
        }
        Linked::UnknownRequired => None,
    };
    let start_point = match linked {
        Linked::Known(v) => StartPoint::Known(p3(v)),
        _ => StartPoint::Unknown,
    };

    let mode = explicit.or(ctx.modal.motion).expect("plan Motion ⇒ mode");

    if matches!(mode, GCode::ArcCw | GCode::ArcCcw) {
        return arc_block(ctx, b, mode, base);
    }

    let end = resolve_end(ctx, b, base, machine)?;
    let frame = if machine { Frame::Machine } else { Frame::Wcs };

    // zero-length (start == end) → drop + notice (SPEC §3).
    if let StartPoint::Known(s) = &start_point {
        if len3(&sub3(&end, s)) < 1e-9 {
            let op = if mode == GCode::Rapid {
                Op::Rapid {
                    start: start_point.clone(),
                    end: end.clone(),
                    state: ctx.state_with_frame(frame),
                    src: span(&ctx.file, line, blen),
                    sub: None,
                }
            } else {
                Op::Linear {
                    start: start_point.clone(),
                    end: end.clone(),
                    state: ctx.state_with_frame(frame),
                    src: span(&ctx.file, line, blen),
                    sub: None,
                }
            };
            ctx.push_drop(
                op,
                NoticeKind::ZeroLengthMotion,
                "zero-length motion dropped (start == end, SPEC §3)".into(),
            );
            ctx.prev = Some(Reading {
                pos: e3(&end),
                frame,
                wcs: ctx.modal.wcs,
            });
            ctx.saw_motion = true;
            return Ok(());
        }
    }

    // feed: G0 не требует; linear без F → INVALID (SPEC §3/§4).
    let op = if mode == GCode::Rapid {
        Op::Rapid {
            start: start_point.clone(),
            end: end.clone(),
            state: ctx.state_with_frame(frame),
            src: span(&ctx.file, line, blen),
            sub: None,
        }
    } else {
        if ctx.feed.is_none() {
            return Err(vec![err(
                ErrorCode::InvalidInput,
                "linear move before any F word (SPEC §3/§4)",
                &ctx.file,
                line,
                blen,
            )]);
        }
        let mut st = ctx.state_with_frame(frame);
        if ctx.modal.feed_inverse {
            // G93 (ABSORB, §2.2): feed = len·F (SPEC §4); требует known start.
            let s = match &start_point {
                StartPoint::Known(s) => e3(s),
                StartPoint::Unknown => {
                    return Err(vec![err(
                        ErrorCode::RuntimeState,
                        "G93 feed resolution requires known start (SPEC §4)",
                        &ctx.file,
                        line,
                        blen,
                    )]);
                }
            };
            let f = ctx.feed.clone().expect("feed checked above");
            let len = {
                let d = [
                    end.x.value() - s[0],
                    end.y.value() - s[1],
                    end.z.value() - s[2],
                ];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
            };
            st.feed = Some(num_f64(len * f.value()));
        }
        Op::Linear {
            start: start_point,
            end: end.clone(),
            state: st,
            src: span(&ctx.file, line, blen),
            sub: None,
        }
    };
    ctx.push(op);
    ctx.prev = Some(Reading {
        pos: e3(&end),
        frame,
        wcs: ctx.modal.wcs,
    });
    ctx.saw_motion = true;
    Ok(())
}

/// Дуга G2/G3 (§2.1 контракта, SPEC §6): IJK — инкрементальные offsets от
/// start ВСЕГДА (группы 04 в словаре Haas НЕТ — официальный ман: «I J K
/// values define the relative X Y Z distances from the starting point»);
/// G90.1 `?` (ABSORB-как-LinuxCNC ruling этапа 1) — абсолютный центр.
/// R-форма: знак R выбирает ≤180°/>180°; R при полной окружности →
/// E_INVALID_INPUT (§2.1: «R при полной окружности невалидна»); R+вне-
/// плоскостное слово (helical R-форма) — допустимо, `?` §10(24) (SPEC §6
/// v0.7 правило 1: нормаль известна из плоскости). Helical — вне-плоскостное
/// осевое слово → pitch. **Полное кольцо = IJK без end-слов** (официальный
/// ман: «Defining only the I J K values… will cut a circle»); ПО ГЕОМЕТРИИ:
/// center-форма end==start при center≠start → sweep 360° НЕЗАВИСИМО от
/// наличия явных end-слов (SPEC §3 v0.18); вырожденный центр (center ==
/// start) при end == start → drop+notice. Валидация нормали по активной
/// плоскости (SPEC §4/§6): перпендикулярная компонента (center − start) ≈ 0,
/// иначе E_INVALID_INPUT.
#[allow(clippy::too_many_lines)]
fn arc_block(
    ctx: &mut Ctx,
    b: &Block,
    mode: GCode,
    base: Option<[f64; 3]>,
) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let inval = |msg: String| err(ErrorCode::InvalidInput, msg, &ctx.file, line, blen);
    // ARC с start=unknown — E_RUNTIME_STATE (геометрия требует числового
    // старта, SPEC §2).
    let start = base.ok_or_else(|| {
        vec![err(
            ErrorCode::RuntimeState,
            "arc with unknown start (SPEC §2)",
            &ctx.file,
            line,
            blen,
        )]
    })?;
    let end = resolve_end(ctx, b, Some(start), false)?;

    let (a0, a1, al) = plane_axes(ctx.modal.plane);
    let s = start;
    let e = e3(&end);
    let pitch = e[al] - s[al]; // вне-плоскостная компонента хода (винт)
    let dir = if mode == GCode::ArcCw {
        Dir::Cw
    } else {
        Dir::Ccw
    };
    let normal = plane_normal(ctx.modal.plane);

    // Центр: in-plane пара; линейная компонента = start (круг в плоскости старта).
    let mut c = s;
    let r_val = b.word('R').map(|n| ctx.dpi_num('R', n)).map(|n| n.value());
    let d0 = e[a0] - s[a0];
    let d1 = e[a1] - s[a1];
    let chord_plane = (d0 * d0 + d1 * d1).sqrt();
    match r_val {
        Some(r) => {
            // R при совпадающих in-plane start/end — центр неоднозначен
            // (SPEC §6(2); контракт §2.1: «R при полной окружности невалидна»;
            // полный круг — только IJK-формой).
            if chord_plane < 1e-9 {
                return Err(vec![inval(
                    "R-arc with coincident in-plane start/end: ambiguous center — full circle only via IJK (SPEC §6; analysis §2.1)".into(),
                )]);
            }
            // Хорда > 2R — невозможная дуга (SPEC §6).
            let tol_r = 0.001f64.max(1e-6 * r.abs());
            let h2 = 4.0 * r * r - d0 * d0 - d1 * d1;
            if h2 < -tol_r * 4.0 * r.abs().max(1.0) {
                return Err(vec![inval(format!(
                    "arc chord {chord_plane:.4} > 2R {}: impossible arc (SPEC §6)",
                    r.abs()
                ))]);
            }
            // Формула LinuxCNC/grbl: центр = середина хорды ± h·p̂; знак
            // направления + знак R (≤180° / >180°).
            let mut sgn = if mode == GCode::ArcCw { -1.0 } else { 1.0 };
            if r < 0.0 {
                sgn = -sgn;
            }
            let h = h2.max(0.0).sqrt() / 2.0;
            let (px, py) = (-d1 / chord_plane, d0 / chord_plane);
            c[a0] = (e[a0] + s[a0]) / 2.0 + sgn * h * px;
            c[a1] = (e[a1] + s[a1]) / 2.0 + sgn * h * py;
        }
        None => {
            // IJK: пара осей плоскости; буквенные соответствия I↔X, J↔Y, K↔Z.
            // Инкрементальные от start ВСЕГДА (группы 04 нет; §1.1/§2.1);
            // G90.1 `?` — ABSORB-как-LinuxCNC (ruling этапа 1).
            let mut off_vals = [0.0f64; 3];
            for (i, letter) in ['I', 'J', 'K'].into_iter().enumerate() {
                if let Some(n) = b.word(letter) {
                    off_vals[i] = ctx.dpi_num(letter, n).value();
                }
            }
            // Валидация нормали (SPEC §4/§6): перпендикулярная компонента
            // (center − start) ≈ 0 → вне-плоскостное слово обязано быть ~0.
            if off_vals[al].abs() > 1e-9 {
                return Err(vec![inval(format!(
                    "perpendicular arc-center component {}{} in plane {}: center must lie in the start plane (SPEC §4/§6 plane validation)",
                    ['I', 'J', 'K'][al], num_f64(off_vals[al]).text, ['X', 'Y', 'Z'][al],
                ))]);
            }
            if ctx.modal.arc_center_abs {
                // G90.1 `?` — вне словаря Haas; ABSORB-как-LinuxCNC ruling.
                c[a0] = off_vals[a0];
                c[a1] = off_vals[a1];
            } else {
                // Дефолт Haas: инкремент от start ВСЕГДА, независимо от
                // G90/G91 (§2.1 контракта).
                c[a0] = s[a0] + off_vals[a0];
                c[a1] = s[a1] + off_vals[a1];
            }
        }
    }

    // Sweep: угол от старта к цели в направлении дуги (SPEC §6, (0°, 360°]).
    // SPEC §10.5 — single-final-rounding: производные в f64 от сырых входов,
    // округление к 4dp однократно (num_f64).
    let two_pi = 2.0 * std::f64::consts::PI;
    let sweep = match r_val {
        // R-форма: из хорды и |R| (закон хорд): 2·asin(c/2R) — ≤180° при R>0,
        // дополнение до 360° при R<0.
        Some(r) => {
            let sw = 2.0 * (chord_plane / (2.0 * r.abs())).clamp(0.0, 1.0).asin();
            if r < 0.0 {
                two_pi - sw
            } else {
                sw
            }
        }
        // IJK: полярные углы от центра; совпадение в плоскости = 360°
        // (**полный круг ПО ГЕОМЕТРИИ**: center ≠ start + start == end —
        // 360° НЕЗАВИСИМО от наличия явных end-слов, SPEC §3 v0.18; контракт
        // §2.1 «полное кольцо = IJK без end-слов» — частный случай).
        None => {
            let a_s = (s[a1] - c[a1]).atan2(s[a0] - c[a0]);
            let a_e = (e[a1] - c[a1]).atan2(e[a0] - c[a0]);
            let mut sweep_ccw = (a_e - a_s).rem_euclid(two_pi);
            if sweep_ccw < 1e-9 {
                sweep_ccw = two_pi;
            }
            let sw = if mode == GCode::ArcCw {
                two_pi - sweep_ccw
            } else {
                sweep_ccw
            };
            if sw < 1e-9 {
                two_pi
            } else {
                sw
            }
        }
    };

    // zero-length arc (SPEC §3 v0.17): нулевой ход = end-слова, квантованные
    // до стартовой точки (§10.6), при center == start (вырожденное кольцо
    // радиуса 0) → drop+notice. Center-форма с center ≠ start при
    // start == end = полный круг 360° НЕЗАВИСИМО от формы команды (v0.18;
    // CAM-посты систематически пишут end-слова на полных кругах).
    let rho_s = ((s[a0] - c[a0]).powi(2) + (s[a1] - c[a1]).powi(2)).sqrt();
    let rho_e = ((e[a0] - c[a0]).powi(2) + (e[a1] - c[a1]).powi(2)).sqrt();
    let explicit_end = b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some();
    if explicit_end && chord_plane < 1e-9 && pitch.abs() < 1e-9 && rho_s < 1e-9 {
        let mut st = ctx.state();
        st.feed = ctx.feed.clone();
        let op = Op::Arc {
            start: StartPoint::Known(pt(s[0], s[1], s[2])),
            end: end.clone(),
            center: pt(c[0], c[1], c[2]),
            normal: pt(normal[0], normal[1], normal[2]),
            dir,
            sweep_deg: num_f64(sweep.to_degrees()),
            pitch: num_f64(pitch),
            state: st,
            src: span(&ctx.file, line, blen),
            sub: None,
        };
        ctx.push_drop(
            op,
            NoticeKind::ZeroLengthArc,
            "zero-length arc dropped (end quantizes to start, sweep != 360°, pitch == 0; SPEC §3/§6)".into(),
        );
        ctx.prev = Some(Reading {
            pos: e,
            frame: Frame::Wcs,
            wcs: ctx.modal.wcs,
        });
        ctx.saw_motion = true;
        return Ok(());
    }
    let mut near_degenerate = false;
    if b.word('R').is_some() && (180.0 - sweep.to_degrees()).abs() < 0.114 {
        near_degenerate = true; // |cos(sweep/2)| < 1e-3 (EQUIVALENCE §4)
    }

    // Валидация §6: планиметрический радиус ρ_start ≈ ρ_end.
    let tol_r = 0.001f64.max(1e-6 * rho_s.max(rho_e));
    if (rho_s - rho_e).abs() > tol_r {
        return Err(vec![inval(format!(
            "arc radius mismatch: ρ_start {rho_s:.4} vs ρ_end {rho_e:.4} (SPEC §6)"
        ))]);
    }
    // Уравнение позиции: end ≈ c + Rot(n, dir, sweep)·(start − c) + pitch·n.
    let v = {
        let th = if mode == GCode::ArcCw { -sweep } else { sweep };
        let rv = [s[0] - c[0], s[1] - c[1], s[2] - c[2]];
        let cos = th.cos();
        let sin = th.sin();
        let nv = [rv[0] * normal[0] + rv[1] * normal[1] + rv[2] * normal[2]];
        let cr = [
            normal[1] * rv[2] - normal[2] * rv[1],
            normal[2] * rv[0] - normal[0] * rv[2],
            normal[0] * rv[1] - normal[1] * rv[0],
        ];
        [
            rv[0] * cos + cr[0] * sin + normal[0] * nv[0] * (1.0 - cos),
            rv[1] * cos + cr[1] * sin + normal[1] * nv[0] * (1.0 - cos),
            rv[2] * cos + cr[2] * sin + normal[2] * nv[0] * (1.0 - cos),
        ]
    };
    let eps_pos = 0.001f64.max(1e-7 * len3(&sub3(&pt(e[0], e[1], e[2]), &pt(s[0], s[1], s[2]))));
    let mut resid2 = 0.0;
    for i in 0..3 {
        let pred = c[i] + v[i] + pitch * normal[i];
        resid2 += (e[i] - pred).powi(2);
    }
    if resid2.sqrt() > eps_pos {
        return Err(vec![inval(format!(
            "arc rotation-equation residual {:.4} > {eps_pos:.4} (SPEC §6.3)",
            resid2.sqrt()
        ))]);
    }

    if ctx.feed.is_none() {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "arc move before any F word (SPEC §3/§4)",
            &ctx.file,
            line,
            blen,
        )]);
    }
    let mut st = ctx.state();
    if ctx.modal.feed_inverse {
        // G93: feed = len(траектории)·F; длина винтовой дуги —
        // sqrt((sweep·ρ)² + pitch²) (SPEC §4, §6).
        let f = ctx.feed.clone().expect("feed checked above");
        let sweep_rad = sweep;
        let len = ((sweep_rad * rho_s).powi(2) + pitch.powi(2)).sqrt();
        st.feed = Some(num_f64(len * f.value()));
    } else {
        st.feed = ctx.feed.clone();
    }
    let op = Op::Arc {
        start: StartPoint::Known(pt(s[0], s[1], s[2])),
        end: end.clone(),
        center: pt(c[0], c[1], c[2]),
        normal: pt(normal[0], normal[1], normal[2]),
        dir,
        sweep_deg: num_f64(sweep.to_degrees()),
        pitch: num_f64(pitch),
        state: st,
        src: span(&ctx.file, line, blen),
        sub: None,
    };
    if near_degenerate {
        ctx.push_keep(
            op,
            NoticeKind::NearDegenerateRArc,
            "near-degenerate R-arc (±0.12° around 180°): center amplification (EQUIVALENCE §4)"
                .into(),
        );
    } else {
        ctx.push(op);
    }
    ctx.prev = Some(Reading {
        pos: e,
        frame: Frame::Wcs,
        wcs: ctx.modal.wcs,
    });
    ctx.saw_motion = true;
    Ok(())
}

// ---------------------------------------------------------------------------
// Циклы группы 09 (§5 отчёта × Haas-семантика §5.4; канон-шаблон §7.7 SPEC)
// ---------------------------------------------------------------------------

/// Обновление/создание модального цикла из слов кадра (параметры группы 09
/// модальны; повторные блоки обновляют Z/R/Q/P/L/I/J/K — §5.1).
#[allow(clippy::too_many_lines)]
fn cycle_block(
    ctx: &mut Ctx,
    b: &Block,
    kind: CycleKind,
    repeat: bool,
) -> Result<(), Vec<Diagnostic>> {
    // L0 → Raw{SEMANTICS} (`?` — «запись данных без исполнения», COVERAGE
    // §2.5: до сверки не разворачивать; инверсия имён vs Fanuc — K-правила
    // Fanuc НЕ переносятся).
    if let Some(w) = b.word('L') {
        if w.value == 0.0 {
            let op = ctx.raw_op(
                b,
                ErrorCode::UnsupportedSemantics,
                "L0 in cycle — data-record without execution `?`, not unrolled until verified (COVERAGE §2.5; Haas L = repeat, K is NOT)",
            );
            ctx.push(op);
            return Ok(());
        }
    }

    // G73 с I/J/K — переменный пекинг на G73 не сверен (`?` COVERAGE §6,
    // реестр §10(4)): консервативно Raw{SEMANTICS}, не разворачивается.
    if kind == CycleKind::ChipBreak
        && (b.word('I').is_some() || b.word('J').is_some() || b.word('K').is_some())
    {
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSemantics,
            "G73 with I/J-K variable pecking — unverified on Haas mill (`?` COVERAGE §6, registry §10(4)); not unrolled",
        );
        ctx.push(op);
        return Ok(());
    }

    // Обновить/создать модальные параметры цикла.
    let mut cyc = ctx.cycle.take().unwrap_or(ActiveCycle {
        kind,
        z: None,
        r: None,
        q: None,
        p_s: None,
        i: None,
        j: None,
        k: None,
    });
    cyc.kind = kind;
    if let Some(w) = b.word('Z') {
        cyc.z = Some(ctx.dpi_num('Z', w).value());
    }
    if let Some(w) = b.word('R') {
        cyc.r = Some(ctx.dpi_num('R', w).value());
    }
    if let Some(w) = b.word('Q') {
        cyc.q = Some(ctx.dpi_num('Q', w).value());
    }
    if let Some(w) = b.word('P') {
        // P dwell цикла — P-DOT RULE (уникальное Haas-правило, §1.2/§5.1):
        // dotted P = секунды verbatim; bare P = МИЛЛИСЕКУНДЫ + notice.
        cyc.p_s = Some(ctx.dwell_seconds(w));
    }
    if let Some(w) = b.word('I') {
        cyc.i = Some(ctx.dpi_num('I', w).value());
    }
    if let Some(w) = b.word('J') {
        cyc.j = Some(ctx.dpi_num('J', w).value());
    }
    if let Some(w) = b.word('K') {
        cyc.k = Some(ctx.dpi_num('K', w).value());
    }
    let rep = repeat_count_l(ctx, b)?;
    ctx.cycle = Some(cyc.clone());
    if !repeat {
        return Ok(()); // параметр-кадр без X/Y — сверления нет (§5.1)
    }
    unroll_cycle(ctx, b, &cyc, rep)
}

/// L-повтор кадра (0–9999). **ТОЛЬКО L** — K на Haas НЕ повтор (min-пек
/// G83 / угловой шаг G12; §1.2/§5.2 отчёта — Fanuc-K-семантика запрещена).
fn repeat_count_l(ctx: &Ctx, b: &Block) -> Result<u32, Vec<Diagnostic>> {
    let w = match b.word('L') {
        Some(w) => w,
        None => return Ok(1),
    };
    if w.value.fract() != 0.0 || w.value < 0.0 || w.value > 9999.0 {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            format!(
                "L repeat count {} outside integer domain 0..9999 (analysis §1.2; L = repeat, K is NOT a repeat on Haas)",
                w.text
            ),
            &ctx.file,
            b.line_no,
            b.raw.len(),
        )]);
    }
    Ok(w.value as u32)
}

/// Расписание глубин пеков (bottoms, абсолютные Z, порядок сохранён — §7.4).
/// Q-форма (§7.4a): d_k = max(z_bottom, r_cur − k·q).
/// I/J/K-форма (переменный пекинг G83, §5.3 отчёта): первый пек d1 = I
/// (инкремент от R вниз), последующие dk = max(K, d_(k−1) − J); дно клампится
/// на z_bottom.
fn peck_bottoms(
    ctx: &Ctx,
    b: &Block,
    r_cur: f64,
    z_bottom: f64,
    cyc: &ActiveCycle,
) -> Result<Vec<f64>, Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let inval =
        |msg: String| vec![err(ErrorCode::InvalidInput, msg, &ctx.file, line, blen)];
    match (cyc.q, cyc.i) {
        (Some(q), _) => {
            // Q-форма: строго > 0 (иначе бесконечные пеки).
            if q <= 0.0 {
                return Err(inval("Q must be > 0 (peck depth)".into()));
            }
            let t = r_cur - z_bottom;
            let mut n = if t > 0.0 { (t / q).ceil() as u64 } else { 0 };
            if n == 0 {
                n = 1;
            }
            if n > MAX_UNROLL_ITERATIONS {
                return Err(vec![limit_err(
                    "max_unroll_iterations",
                    MAX_UNROLL_ITERATIONS as f64,
                    n as f64,
                    format!("peck count {n} exceeds max_unroll_iterations ({MAX_UNROLL_ITERATIONS})"),
                    &ctx.file,
                    line,
                )]);
            }
            Ok((1..=n)
                .map(|kk| (r_cur - kk as f64 * q).max(z_bottom))
                .collect())
        }
        (None, Some(first)) => {
            // I/J/K-форма: mutually exclusive с Q (проверено выше).
            if first <= 0.0 {
                return Err(inval("I (first peck) must be > 0 (variable pecking)".into()));
            }
            let j = cyc.j.unwrap_or(0.0);
            let k_min = cyc.k.unwrap_or(0.0);
            let mut bottoms = Vec::new();
            let mut d = first; // текущий инкремент пека
            let mut bottom = r_cur;
            loop {
                let target = (bottom - d).max(z_bottom);
                bottoms.push(target);
                if target <= z_bottom + 1e-9 {
                    break;
                }
                bottom = target;
                let nd = (d - j).max(k_min);
                if nd <= 0.0 {
                    return Err(inval(format!(
                        "variable peck schedule degenerates (d = max(K, d-J) = {nd:.4} ≤ 0) — broken I/J/K form (analysis §5.3)"
                    )));
                }
                d = nd;
                if bottoms.len() as u64 > MAX_UNROLL_ITERATIONS {
                    return Err(vec![limit_err(
                        "max_unroll_iterations",
                        MAX_UNROLL_ITERATIONS as f64,
                        bottoms.len() as f64,
                        format!("variable peck count {} exceeds max_unroll_iterations ({MAX_UNROLL_ITERATIONS})", bottoms.len()),
                        &ctx.file,
                        line,
                    )]);
                }
            }
            Ok(bottoms)
        }
        (None, None) => Err(inval(
            "peck cycle G83 without Q word or I/J-K variable pecking (mutually exclusive forms; analysis §5.3)".into(),
        )),
    }
}

/// Развёртывание цикла (SPEC §7.7 канон-шаблон; SPEC §7.4a v0.18 — клиренс
/// 0.01in unit-scaled; Haas-семантика §5.4 отчёта). Последовательность на
/// отверстие (L → повтор всей последовательности):
/// 1. RAPID traverse к отверстию НА ТЕКУЩЕМ Z (не сливать с подходом);
/// 2. RAPID подход к R-плоскости;
/// 3. LINEAR feed на дно; [DWELL P (G82/G86/G89 — P-dot rule; обязателен)];
///    [пеки G83/G73: депеки по расписанию; G83 — отвод на R + подход d+δ;
///    G73 — одиночный rapid d+δ; δ = 0.254mm/0.0100in unit-scaled §7.4a;
///    G73 δ = Setting 22 вне текста → notice W_CYCLE_PARAM_ASSUMED §6.19];
///    [G86: DWELL перед отводом, шпиндель off];
/// 4. retract: G99 → RAPID (R); G98 → RAPID (initial Z); G85/G89 — отвод на
///    подаче; G86 — rapid при шпинделе off, рестарт в исходном направлении
///    (Q9) — в state последующих опов.
#[allow(clippy::too_many_lines)]
fn unroll_cycle(
    ctx: &mut Ctx,
    b: &Block,
    cyc: &ActiveCycle,
    k: u32,
) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let fl = |code: ErrorCode, msg: String| vec![err(code, msg, &ctx.file, line, blen)];

    // §7.6: позиция до цикла должна быть определена предшествующими
    // motion-опами (пресетки-якоря не считаются — strict reading).
    if ctx.prev.is_none() {
        return Err(fl(
            ErrorCode::RuntimeState,
            "cycle as first motion op — position not established by preceding moves (SPEC §7.6)".into(),
        ));
    }
    let base = match ctx.link_start(false, line, blen)? {
        Linked::Known(v) => v,
        _ => {
            return Err(fl(
                ErrorCode::RuntimeState,
                "cycle after positionally-unknown section — E_RUNTIME_STATE (SPEC §7.6)".into(),
            ))
        }
    };

    // Лимит развёртки (COVERAGE §1.2) — раньше прочих проверок параметров.
    if k as u64 > MAX_UNROLL_ITERATIONS {
        return Err(vec![limit_err(
            "max_unroll_iterations",
            MAX_UNROLL_ITERATIONS as f64,
            k as f64,
            format!("cycle repeat L {k} exceeds max_unroll_iterations ({MAX_UNROLL_ITERATIONS})"),
            &ctx.file,
            line,
        )]);
    }

    // R и Z обязательны (паритет Fanuc/LinuxCNC «It is an error if…»).
    let r_word = cyc.r.ok_or_else(|| {
        fl(
            ErrorCode::InvalidInput,
            "cycle without R word (retract plane)".into(),
        )
    })?;
    let z_word = cyc.z.ok_or_else(|| {
        fl(
            ErrorCode::InvalidInput,
            "cycle without Z word (hole bottom)".into(),
        )
    })?;
    // Модальный feed обязателен (feed-ходы цикла — SPEC §4).
    let feed = ctx.feed.clone().ok_or_else(|| {
        fl(
            ErrorCode::InvalidInput,
            "cycle without F — feed moves require modal feed (SPEC §4)".into(),
        )
    })?;

    // Dwell: G82/G89 — P обязателен; G86 — P обязателен (SPEC §7.4a v0.18:
    // rs274 отказывает на G86 без P — exit 1, сообщение зеркалируется
    // дословно). P-dot rule уже применён в cycle_block.
    let dwell = match cyc.kind {
        CycleKind::DrillDwell | CycleKind::BoreDwell => Some(cyc.p_s.ok_or_else(|| {
            fl(
                ErrorCode::InvalidInput,
                "cycle G82/G89 without P word (dwell — P-dot rule: dotted P = seconds, bare P = milliseconds)".into(),
            )
        })?),
        CycleKind::BoreStop => Some(cyc.p_s.ok_or_else(|| {
            fl(
                ErrorCode::InvalidInput,
                "Dwell time p word missing with g86".into(),
            )
        })?),
        _ => None,
    };

    // G83: Q ИЛИ I/J/K — mutually exclusive («Do not use a Q value when
    // programming with I, J, and K», §5.3 отчёта). G73 — только Q (`?`
    // I/J-K отсечён в cycle_block).
    if cyc.kind == CycleKind::Peck && cyc.q.is_some() && cyc.i.is_some() {
        return Err(fl(
            ErrorCode::InvalidInput,
            "G83 Q and I/J-K are mutually exclusive — «Do not use a Q value when programming with I, J, and K» (analysis §5.3)".into(),
        ));
    }
    if cyc.kind == CycleKind::Peck && cyc.q.is_none() && cyc.i.is_none() {
        return Err(fl(
            ErrorCode::InvalidInput,
            "G83 without Q word or I/J-K variable pecking (mutually exclusive forms; analysis §5.3)".into(),
        ));
    }
    if cyc.kind == CycleKind::ChipBreak {
        let q = cyc.q.ok_or_else(|| {
            fl(
                ErrorCode::InvalidInput,
                "peck cycle G73 without Q word (chip-break peck depth)".into(),
            )
        })?;
        if q <= 0.0 {
            return Err(fl(ErrorCode::InvalidInput, "Q must be > 0 (peck depth)".into()));
        }
    }

    let src = span(&ctx.file, line, blen);
    let (mut cx, mut cy, mut cz) = (base[0], base[1], base[2]);
    let initial_z = base[2];
    let mut r_cur = if ctx.modal.dist_abs {
        r_word
    } else {
        initial_z
    };
    let mut sub = 1u32;
    let spindle_before = ctx.spindle.clone(); // рестарт G86 (Q9)
    let units = ctx.modal.units;
    let mut est_total = 0usize; // guard blowup развёртки (COVERAGE §1.2)

    // G73: δ = Setting 22 (вне текста) → канон-константа + notice
    // W_CYCLE_PARAM_ASSUMED (COVERAGE §6.19: документированный дефолт
    // Setting 22 0.02"/0.5mm ≠ канон — диалектное расхождение зафиксировано).
    if cyc.kind == CycleKind::ChipBreak {
        ctx.note_pending(
            NoticeKind::CycleParamAssumed,
            format!(
                "W_CYCLE_PARAM_ASSUMED: G73 chip-break retract δ = Setting 22 (Canned Cycle Delta Z) outside program text — canon constant {} applied (SPEC §7.4a v0.18 unit-scaled; documented Setting 22 default 0.02\"/0.5mm differs — dialect divergence COVERAGE §6.19; profile does not override)",
                num_f64(approach_clearance(units)).text
            ),
        );
    }

    for _ in 0..k {
        let (fx, fy) = (cx, cy); // позиция ДО шага — оттуда traverse
                                 // Позиции итерации (§5.2/§7.3): G90 — константны; G91 — шаг X/Y/R
                                 // на повтор, Z = инкремент от R-плоскости.
        if ctx.modal.dist_abs {
            if let Some(w) = b.word('X') {
                cx = ctx.dpi_num('X', w).value();
            }
            if let Some(w) = b.word('Y') {
                cy = ctx.dpi_num('Y', w).value();
            }
        } else {
            r_cur += r_word;
            if let Some(w) = b.word('X') {
                cx += ctx.dpi_num('X', w).value();
            }
            if let Some(w) = b.word('Y') {
                cy += ctx.dpi_num('Y', w).value();
            }
        }
        let z_bottom = if ctx.modal.dist_abs {
            z_word
        } else {
            r_cur + z_word
        };

        // 1. traverse к отверстию НА ТЕКУЩЕМ Z (один оп; §7.7 #1 — merge
        //    с подходом запрещён); нулевой (повтор той же X/Y) → drop.
        let traverse = Op::Rapid {
            start: StartPoint::Known(pt(fx, fy, cz)),
            end: pt(cx, cy, cz),
            state: ctx.state(),
            src: src.clone(),
            sub: Some(sub),
        };
        push_cycle_move(ctx, traverse);
        sub += 1;
        // 2. подход к R-плоскости — отдельный оп (§7.7 #2); при cz == r_cur
        //    (после отвода предыдущего отверстия) — zero-length → drop+notice.
        let approach = Op::Rapid {
            start: StartPoint::Known(pt(cx, cy, cz)),
            end: pt(cx, cy, r_cur),
            state: ctx.state(),
            src: src.clone(),
            sub: Some(sub),
        };
        push_cycle_move(ctx, approach);
        sub += 1;
        cz = r_cur;

        // 3. Тело цикла.
        match cyc.kind {
            CycleKind::Drill
            | CycleKind::DrillDwell
            | CycleKind::BoreFeedOut
            | CycleKind::BoreDwell
            | CycleKind::BoreStop => {
                push_cycle_move(
                    ctx,
                    cyc_linear(
                        ctx,
                        [cx, cy, cz],
                        [cx, cy, z_bottom],
                        &feed,
                        src.clone(),
                        sub,
                    ),
                );
                sub += 1;
                cz = z_bottom;
                if let Some(p) = dwell {
                    let op = Op::Dwell {
                        seconds: num_f64(p),
                        state: ctx.state(),
                        src: src.clone(),
                        sub: Some(sub),
                    };
                    // SPEC §5.2: dwell == 0 → drop + notice (P0 в цикле).
                    if p == 0.0 {
                        ctx.push_drop(
                            op,
                            NoticeKind::ZeroLengthDwell,
                            "zero dwell dropped (seconds == 0, SPEC §5.2)".into(),
                        );
                    } else {
                        ctx.push(op);
                    }
                    sub += 1;
                }
            }
            CycleKind::Peck | CycleKind::ChipBreak => {
                // §7.4a (v0.18, зонд-сверка rs274): порядок пиков точный.
                // G83 (Haas §5.3): Q-расписание d_k = max(z_bottom, r−k·q)
                // ИЛИ переменный пекинг I/J/K (d1 = I, dk = max(K, d_(k−1)−J)).
                // Промежуточный отвод: G83 — полный отвод на R, затем подход
                // rapid на d+δ; G73 — ОДИН rapid на d+δ (δ = Setting 22 →
                // канон 0.254mm/0.0100in + notice W_CYCLE_PARAM_ASSUMED;
                // финальный отвод — общий ниже).
                let bottoms = peck_bottoms(ctx, b, r_cur, z_bottom, cyc)?;
                let n = bottoms.len() as u64;
                // est: traverse+подход (2) + на пек: feed + отводы (G83 —
                // retract R и подход: 3; G73 — одиночный rapid: 2) + финал (1).
                let ops_per_peck = if cyc.kind == CycleKind::Peck { 3 } else { 2 };
                let est = 2 + ops_per_peck * n as usize + 1;
                est_total += est;
                if est_total as u64 > MAX_UNROLL_ITERATIONS {
                    return Err(vec![limit_err(
                        "max_unroll_iterations",
                        MAX_UNROLL_ITERATIONS as f64,
                        est_total as f64,
                        format!("peck unroll estimate {est_total} exceeds max_unroll_iterations ({MAX_UNROLL_ITERATIONS})"),
                        &ctx.file, line,
                    )]);
                }
                let clr = approach_clearance(units);
                for (kk, d) in bottoms.iter().enumerate() {
                    let d = *d;
                    push_cycle_move(
                        ctx,
                        cyc_linear(ctx, [cx, cy, cz], [cx, cy, d], &feed, src.clone(), sub),
                    );
                    sub += 1;
                    cz = d;
                    if kk + 1 == bottoms.len() {
                        break; // финальный отвод — общий ниже (§7.1)
                    }
                    // G83: полный отвод на R (SPEC §7.4a). G73 (v0.18):
                    // единственный промежуточный ход — подход ниже;
                    // «de-retract min(R, d+Q)» в интерпретаторе не существует.
                    if cyc.kind == CycleKind::Peck {
                        let op = Op::Rapid {
                            start: StartPoint::Known(pt(cx, cy, cz)),
                            end: pt(cx, cy, r_cur),
                            state: ctx.state(),
                            src: src.clone(),
                            sub: Some(sub),
                        };
                        push_cycle_move(ctx, op);
                        sub += 1;
                        cz = r_cur;
                    }
                    // подход: одиночный rapid на клиренс над последней
                    // глубиной (≤ R); для G73 — единственный промежуточный
                    // отвод; для G83 — второй ход после отвода на R.
                    let appr = (d + clr).min(r_cur);
                    let op = Op::Rapid {
                        start: StartPoint::Known(pt(cx, cy, cz)),
                        end: pt(cx, cy, appr),
                        state: ctx.state(),
                        src: src.clone(),
                        sub: Some(sub),
                    };
                    push_cycle_move(ctx, op);
                    sub += 1;
                    cz = appr;
                }
            }
        }

        // 4/5. Отвод (§7.1/§7.5a): G99 → R (дефолт), G98 → initial Z. Тип
        // отвода: G85/G89 — feed-out (bore); G86 — rapid при выключенном
        // шпинделе (Q9: рестарт в прежнем направлении после отвода);
        // остальные — rapid.
        let retract = if ctx.modal.retract_g98 {
            initial_z
        } else {
            r_cur
        };
        match cyc.kind {
            CycleKind::BoreFeedOut | CycleKind::BoreDwell => {
                push_cycle_move(
                    ctx,
                    cyc_linear(
                        ctx,
                        [cx, cy, cz],
                        [cx, cy, retract],
                        &feed,
                        src.clone(),
                        sub,
                    ),
                );
                sub += 1;
            }
            CycleKind::BoreStop => {
                ctx.spindle = Spindle::Off;
                let op = Op::Rapid {
                    start: StartPoint::Known(pt(cx, cy, cz)),
                    end: pt(cx, cy, retract),
                    state: ctx.state(),
                    src: src.clone(),
                    sub: Some(sub),
                };
                push_cycle_move(ctx, op);
                sub += 1;
                ctx.spindle = spindle_before.clone(); // рестарт — в state последующих опов
            }
            _ => {
                let op = Op::Rapid {
                    start: StartPoint::Known(pt(cx, cy, cz)),
                    end: pt(cx, cy, retract),
                    state: ctx.state(),
                    src: src.clone(),
                    sub: Some(sub),
                };
                push_cycle_move(ctx, op);
                sub += 1;
            }
        }
        cz = retract;
    }

    // last: конечная позиция развёртки (связывание следующего опа).
    ctx.prev = Some(Reading {
        pos: [cx, cy, cz],
        frame: Frame::Wcs,
        wcs: ctx.modal.wcs,
    });
    ctx.saw_motion = true;
    Ok(())
}

/// Push производного хода развёртки с guard'ом нулевой длины (SPEC §3:
/// каноническая форма не содержит zero-length опов — verify эмиссии
/// отсекает; найдено смоук-этапа 3 eid-fanuc: подход G82 после отвода G83
/// на ту же R-плоскость даёт нулевой ход → drop + notice, не оп).
fn push_cycle_move(ctx: &mut Ctx, op: Op) {
    let zero = match &op {
        Op::Rapid {
            start: StartPoint::Known(s),
            end,
            ..
        }
        | Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            let sv = e3(s);
            let ev = e3(end);
            sv.iter().zip(ev.iter()).all(|(a, b)| (a - b).abs() < 1e-9)
        }
        _ => false,
    };
    if zero {
        ctx.push_drop(
            op,
            NoticeKind::ZeroLengthMotion,
            "zero-length motion dropped (start == end, SPEC §3)".into(),
        );
    } else {
        ctx.push(op);
    }
}

/// LINEAR-оп цикла: feed кадра цикла активен для ВСЕХ производных опов
/// (§7.7 v0.15.1); G93 — разрешение feed = len·F (вертикальный ход).
fn cyc_linear(ctx: &Ctx, from: [f64; 3], to: [f64; 3], feed: &Num, src: SrcSpan, sub: u32) -> Op {
    let mut st = ctx.state();
    if ctx.modal.feed_inverse {
        let len = (to[2] - from[2]).abs();
        st.feed = Some(num_f64(len * feed.value()));
    } else {
        st.feed = Some(feed.clone());
    }
    Op::Linear {
        start: StartPoint::Known(pt(from[0], from[1], from[2])),
        end: pt(to[0], to[1], to[2]),
        state: st,
        src,
        sub: Some(sub),
    }
}

fn sub3(a: &Point3, b: &Point3) -> Point3 {
    let [x, y, z] = e3(a);
    let [u, v, w] = e3(b);
    pt(x - u, y - v, z - w)
}

fn len3(d: &Point3) -> f64 {
    let [x, y, z] = e3(d);
    (x * x + y * y + z * z).sqrt()
}
