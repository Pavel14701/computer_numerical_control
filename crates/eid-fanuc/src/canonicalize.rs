//! Канонизатор Fanuc mill → EID/L1 AST (этапы 1–2).
//! Источники: fanuc_dialect_analysis.md v0.0 (контракт; §1.1 группы, §1.2 DPI,
//! §2.1 движение, §2.3 фреймы, §5 циклы, §3 M-коды, §4 Macro B), SPEC-EID-L1
//! v0.16 (§2 точки разрыва, §3 модель, §4 state, §5.3 фрейм-опы, §6 дуги,
//! §7.7 канон-шаблон циклов, §8 RAW), COVERAGE v0.8 Fanuc-колонка, паттерн —
//! eid-grbl/eid-linuxcnc canonicalize/parser.
//!
//! Этап 2 покрывает (диспозиции — COVERAGE §2/§3 Fanuc-колонка, rulings
//! оркестратора 1–8):
//! * фреймы: G54–G59 → state.wcs p=1..6; G10 L2 P1–P6 → wcs_define
//!   (G90/G91 — абсолют/инкремент записи); G92 X/Y/Z → frame_preset
//!   (только пресетка, §3 SPEC); G92 S → Raw{SEMANTICS} (кламп оборотов);
//!   G53 → machine-frame ходы (правило §5.3, точки разрыва (b)–(d));
//!   G52 → **Err**(E_UNSUPPORTED_SEMANTICS) — единственный жёсткий отказ
//!   Fanuc (смена фрейма mid-program, SPEC §8.5); G10 L2 P0 / G10 L20 /
//!   G10 L1/L10/L11/L50+ → Raw{REQUIRES_MACHINE_TABLE} (L20 Fanuc = запись
//!   G54.1 Pn, НЕ wcs_set_here — ловушка №3); G54.1 Pn / G92.1–.3 →
//!   Raw{SEMANTICS}; G11 → семейство G11 (COVERAGE §1.3).
//! * циклы (§5 отчёта — Fanuc-семантика, НЕ LinuxCNC-константы):
//!   развёртка G81/G82/G83/G73/G85/G86/G89 по канон-шаблону §7.7 SPEC
//!   (traverse+approach НЕ сливать; F кадра — на все производные опы);
//!   P dwell = целые мс → секунды (dotted P → секунды verbatim, v0.18);
//!   G73/G83 δ = 0.254mm/0.01in unit-scaled (SPEC §7.4a v0.18, зонд rs274 —
//!   зеркало D; прежде 0.5mm + W_CYCLE_PARAM_ASSUMED — упразднено);
//!   G86 — шпиндель off на дне + рестарт в исходном направлении (Q9);
//!   G84/G74/G76/G87/G88 → Raw{SEMANTICS}; K/L-повторы; K0/L0 в G91 →
//!   Raw{SEMANTICS}; дефолт retract = G99 (§7.5a); G98/цикл требует
//!   known-position (§7.6); модальность группы 09 — X/Y-блоки = повтор.
//! * M-коды (§3): M0/M1 → program_stop; M2/M30 → program_end; M3/M4/M5+S →
//!   state.spindle (голое S — сохранение оборотов); M6+T → tool_change
//!   (одиночное T — предвыбор → Raw{SEMANTICS}); M7/M8/M9 → coolant;
//!   M19/M29 → Raw{SEMANTICS}; M98/M99/M198 → Raw{SYNTAX}; неизвестный M →
//!   Raw{SYNTAX} + диагностика текста (generic-ведро, «не молча»).
//! * Macro B (§4): `#`-слова / `[ ]`-выражения / IF/GOTO/THEN / WHILE/DO/END /
//!   G65/G66/G66.1/G67 / M98/M99/M198 / O-структуры → line-level
//!   Raw{E_UNSUPPORTED_SYNTAX}; хвост-subs после PEND → Raw-хвост
//!   (ruling 1: Fanuc-специфика — тела subs после M30 норма; осознанное
//!   расхождение с grbl n12, COVERAGE §4 «O-слова»); `/` block delete →
//!   Raw{RUNTIME_STATE}.
//! * компенсации (§2.4): G43/G44 (+G43.1) → Raw{REQUIRES_MACHINE_TABLE};
//!   G43.4/.5, G41/G42 (+семья) → Raw{SEMANTICS}; правило семейств COVERAGE
//!   §1.3 с Fanuc-составом: G40 = {G41,.1,.2-.6, G42,.1,.2-.6, G38, G39};
//!   G49 = {G43, G43.1, G43.4, G43.5, G44}; G40/G49 cancel → IGNORED_SAFE*
//!   (или Raw при активном семействе в файле); G80 → IGNORED_SAFE.
//! * G93 ABSORB (ruling 4): inverse-time — feed = len·F (SPEC §4), эмиссия
//!   всегда G94. DPI-F фикс (ruling 2): F-address IS-B инкремент = 1 mm/min
//!   (параметр 1422) — bare-integer F = целое mm/min verbatim (НЕ ×0.001),
//!   notice W_DPI_ASSUMED сохраняется; Q остаётся ×0.001.
//!
//! Известные ограничения (задокументированы честно):
//! * Whole-line Raw для смешанных кадров: `G43 H2 Z25.` → весь блок Raw
//!   (ruling 7 — консистентность с grbl-моделью; Q11 даёт start=unknown на
//!   точке разрыва; R1-text сохраняет байты; split-parse = скрытая
//!   декомпозиция — ЗАПРЕЩЕНА: координата Z25 из Raw-кадра не извлекается).
//! * Raw-кадры АБСОРБИРУЮТ модальное состояние (SPEC §8.6 правило 6, v0.17 —
//!   критический аудит #33): модальные G (plane/distance/arcdist/feedmode/
//!   units/wcs/path/retract, G80), F/S, M3/M4/M5, M7/M8/M9, G92/G10 — до
//!   push_raw; motion-режим на raw-motion-G (G33/G84/G28…) с осями НЕ
//!   продолжается + notice W_MODAL_ON_RAW. Хвост после PEND — исключение
//!   (программа структурно закончена; абсорбция G92 из sub-тела исказила бы
//!   frame_preset).
//! * Хвост после program_end → Raw-хвост (ruling 1) — COVERAGE-комментарий:
//!   Fanuc-специфика (subs-тела после M30 — норма), осознанное расхождение
//!   с grbl n12.
//! * IS-C-inch-5dp — открытый `?`-реестр (COVERAGE §6.26), не реализовано.

use crate::block::{parse_file, Block, Marker, ParsedFile};
use crate::lexer::LexErrorKind;
use crate::state::{
    g_raw_disposition, plane_axes, plane_normal, CycleKind, GCode, MCode, ModalState,
};
use eid_core::ast::{
    Coolant, Dir, Frame, FramePreset, Num, Op, PathMode, Point3, Program, Spindle, SrcSpan,
    StartPoint, State, Units, Wcs,
};
use eid_core::diag::{Diagnostic, ErrorCode, LimitInfo};
use std::cell::RefCell;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Notices (реплика паттерна eid-grbl/eid-linuxcnc; SPEC §3/§5.2/§6/§7)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// rapid/linear: start == end (SPEC §3).
    ZeroLengthMotion,
    /// arc: start == end при sweep != 360° и pitch == 0 (SPEC §3/§6).
    ZeroLengthArc,
    /// dwell == 0 (SPEC §5.2: no-op → drop+notice).
    ZeroLengthDwell,
    /// Программа без M2/M30: неявный конец — ProgramEnd добавлен последним
    /// опом (SPEC §3 v0.16; анализ §4.4 — auto-PEND применим к Fanuc DNC).
    MissingProgramEnd,
    /// near-degenerate R-дуга (±0.12° вокруг 180°): амплитификация центра
    /// (EQUIVALENCE §4) — notice при живом опе.
    NearDegenerateRArc,
    /// W_DPI_ASSUMED (анализ §1.2 ловушка №1): bare-integer в слове дробного
    /// домена прочитан по DPI=0 (× least increment); F — verbatim (ruling 2).
    DpiAssumed,
    /// W_MODAL_ON_RAW (SPEC §8.6 правило 6, v0.17 — критический аудит #33):
    /// raw-motion-G (G33/G84/G28…) с осями — модальное состояние абсорбировано,
    /// но motion-режим НЕ продолжается (ход не представим) — notice.
    ModalOnRaw,
}

#[derive(Debug, Clone)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
    /// Индекс опа в списке ops (для drop-notice — индекс, который оп ИМЕЛ БЫ).
    pub op_index: usize,
    pub op: Op,
}

/// Результат: программа + notices (drop/DPI/auto-PEND/cycle-param).
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

/// DPI least increment по активным единицам (IS-B, §1.2): 0.001 mm / 0.0001".
fn least_increment(units: Units) -> f64 {
    match units {
        Units::Mm => 0.001,
        Units::In => 0.0001,
    }
}

/// Клиренс пека (SPEC §7.4a v0.18, зонд-сверка rs274 — зеркало референса D,
/// eid-linuxcnc parser.rs commit 4705480): 0.01in unit-scaled — **0.254 mm**
/// в G21, ровно **0.0100 in** в G20. Прежние 0.5mm/0.05in (v0.14) не
/// подтверждаются интерпретатором (probe `g83_inch` — RETRACT_CLEARANCE
/// ровно 0.0100 in). Константа канонизатора, профилем НЕ переопределяется
/// (v0.14.1 в силе); notice W_CYCLE_PARAM_ASSUMED упразднён вместе с
/// «параметр-пресумпцией» — константа зонда-подтверждена.
fn approach_clearance(units: Units) -> f64 {
    if units == Units::In {
        0.01
    } else {
        0.254
    }
}

/// max_unroll_iterations (COVERAGE §1.2: 10 000 — сумма L/K на блок цикла).
const MAX_UNROLL_ITERATIONS: u64 = 10_000;

fn span(file: &Arc<str>, line: u32, len: usize) -> SrcSpan {
    SrcSpan {
        file: file.clone(),
        line_start: line,
        col_start: 1,
        line_end: line,
        col_end: len as u32 + 1,
    }
}

/// Диагностика с явным этапом: Parse — лексические пути, Verify —
/// канонизационные инварианты (interface v0.1.2, паттерн eid-grbl).
fn err_staged(
    stage: eid_core::diag::Stage,
    code: ErrorCode,
    msg: impl Into<String>,
    file: &Arc<str>,
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

fn err(
    code: ErrorCode,
    msg: impl Into<String>,
    file: &Arc<str>,
    line: u32,
    len: usize,
) -> Diagnostic {
    err_staged(eid_core::diag::Stage::Verify, code, msg, file, line, len)
}

fn limit_err(
    threshold: &'static str,
    value: f64,
    actual: f64,
    msg: String,
    file: &Arc<str>,
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

/// Активный модальный цикл группы 09 (§5.1: параметры модальны, X/Y-блоки =
/// повтор; Z/R/Q/P-слова повторных блоков обновляют параметры).
#[derive(Debug, Clone)]
struct ActiveCycle {
    kind: CycleKind,
    z: Option<f64>,   // дно отверстия (сырое слово; G91 — от R-плоскости)
    r: Option<f64>,   // R-плоскость (G91 — от initial Z)
    q: Option<f64>,   // пек (G73/G83)
    p_s: Option<f64>, // dwell в секундах (P — целые мс, §1.2)
}

struct Ctx {
    file: Arc<str>,
    modal: ModalState,
    feed: Option<Num>, // модальный F (переживает G0, SPEC §4)
    /// Провенанс: F-слово было в кадре-источнике текущего блока (v0.2
    /// волна 3, §2.4). Сброс — на начале absorb_frame_state; установка —
    /// при абсорбции F-слова; в снапшот — через state()/state_with_frame
    /// (первый производный оп развёртки = кадр, sub=1; производные 2..n —
    /// state_derived).
    feed_restated: bool,
    spindle: Spindle,
    coolant: Coolant,
    tool: Option<u32>,
    last_s: Option<Num>,            // голое S — сохранение оборотов (§3 отчёта)
    preset: Option<FramePreset>,    // G92 (только пресетка, §3 SPEC)
    origins: [Option<[f64; 3]>; 9], // wcs_define in-text (p 1..9 → idx 0..8)
    prev: Option<Reading>,          // end последнего motion-опа
    saw_motion: bool,               // G92-пресетка только до первого движения
    cycle: Option<ActiveCycle>,     // группа 09 — модальный цикл
    ops: Vec<Op>,
    notices: Vec<Notice>,
    /// W_DPI_ASSUMED-сообщения слов, прочитанных до построения опа (напр.
    /// голый F в кадре без движения): прицепляются к следующему опу.
    pending_dpi: RefCell<Vec<String>>,
    end_count: usize,
    end_line: u32,
    /// Правило семейств (COVERAGE §1.3) — пре-скан файла:
    family_g40: bool, // G41/G42-семья (Fanuc-состав) встречена в файле
    family_g49: bool,     // G43/G44-семья (Fanuc-состав) встречена в файле
    family_g10_raw: bool, // RAW-формы G10 (L20/P0/L1+) встречены в файле
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
            feed_restated: self.feed_restated,
        }
    }

    /// Снапшот производного опа развёртки: флаг кадра несёт ТОЛЬКО первый
    /// производный (sub == 1 — оп, соответствующий кадру-источнику с
    /// F-словом); производные 2..n — false (лишний F на каждом сломал бы
    /// счёт SET_FEED_RATE в rs274-каноне).
    fn state_derived(&self, sub: u32) -> State {
        let mut st = self.state();
        if sub != 1 {
            st.feed_restated = false;
        }
        st
    }

    /// Записать W_DPI_ASSUMED-сообщение (прицепится к следующему опу).
    fn note_dpi(&self, msg: String) {
        self.pending_dpi.borrow_mut().push(msg);
    }

    /// Слово дробного домена (X/Y/Z/R/I/J/K/Q) → Num: bare-integer →
    /// × least increment (DPI=0, IS-B) + W_DPI_ASSUMED; с точкой → verbatim
    /// (§1.2).
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
            "W_DPI_ASSUMED: {}{} (bare integer) read as {} ({} × least increment {} = {}; DPI=0 default, IS-B {}; analysis §1.2)",
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

    /// F-слово → Num (ruling 2: F-address IS-B инкремент = 1 mm/min, параметр
    /// 1422 — bare-integer = целое mm/min verbatim, НЕ ×0.001; notice
    /// W_DPI_ASSUMED сохраняется — домен дробный, но инкремент целый).
    fn feed_num(&self, w: &crate::lexer::RawNum) -> Num {
        if w.has_point {
            return verbatim(w);
        }
        self.note_dpi(format!(
            "W_DPI_ASSUMED: F{} (bare integer) read as {} mm/min verbatim — F-address least increment IS-B = 1 mm/min (param 1422), no x0.001 scaling (orchestrator ruling 2; analysis §1.2)",
            w.text, num_f64(w.value).text,
        ));
        num_f64(w.value)
    }

    fn flush_dpi(&mut self, op_index: usize, op: &Op) {
        let pending = std::mem::take(&mut *self.pending_dpi.borrow_mut());
        for msg in pending {
            self.notices.push(Notice {
                kind: NoticeKind::DpiAssumed,
                message: msg,
                op_index,
                op: op.clone(),
            });
        }
    }

    /// push при живом опе (+ flush DPI-notice'ов на этот оп).
    fn push(&mut self, op: Op) {
        self.ops.push(op);
        let idx = self.ops.len() - 1;
        let op_ref = self.ops[idx].clone();
        self.flush_dpi(idx, &op_ref);
    }

    /// drop-with-notice (оп НЕ попадает в ops, SPEC §3) + flush DPI.
    fn push_drop(&mut self, op: Op, kind: NoticeKind, message: String) {
        let idx = self.ops.len();
        self.flush_dpi(idx, &op);
        self.notices.push(Notice {
            kind,
            message,
            op_index: idx,
            op,
        });
    }

    /// notice при ЖИВОМ опе (near-degenerate R-arc) + flush DPI.
    fn push_keep(&mut self, op: Op, kind: NoticeKind, message: String) {
        let idx = self.ops.len();
        self.ops.push(op.clone());
        self.flush_dpi(idx, &op);
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

    fn mark_end(&mut self, line: u32, blen: usize) {
        self.end_count += 1;
        self.end_line = line;
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
                // Точка разрыва (a): якорь — только frame_preset (Fanuc L20
                // wcs_set_here не существует — Raw, ловушка №3).
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
    // v0.2 (#21): один Arc<str> на прогон канонизации; span() клонирует хэндл
    // (refcount) — 0 аллокаций на оп.
    let file: Arc<str> = Arc::from(file);
    // Лимит max_source_lines (COVERAGE §1.2).
    let line_count = src.lines().count() as f64;
    if line_count > 1_000_000.0 {
        return Err(vec![limit_err(
            "max_source_lines",
            1_000_000.0,
            line_count,
            format!("file has {} lines, limit 1000000", src.lines().count()),
            &file,
            1,
        )]);
    }

    let parsed: ParsedFile = parse_file(src);
    // Лексические ошибки: мусор/комментарии → INVALID (Parse); переполнения —
    // LIMIT (§1.2: целая часть >9 цифр max_word_chars не ловит).
    if !parsed.lex_errors.is_empty() {
        return Err(parsed
            .lex_errors
            .iter()
            .map(|e| match &e.kind {
                LexErrorKind::WordTooLong => limit_err(
                    "max_word_chars", 32.0, 32.0,
                    "word value exceeds max_word_chars (32)".into(), &file, e.line,
                ),
                LexErrorKind::IntOverflow { digits } => limit_err(
                    "max_integer_digits", 9.0, *digits as f64,
                    format!("integer part of {digits} digits exceeds max_integer_digits (9) — Fanuc range ±99999.9999 (analysis §1.2)"),
                    &file, e.line,
                ),
                kind => err_staged(
                    eid_core::diag::Stage::Parse,
                    ErrorCode::InvalidInput,
                    format!("lexical error: {:?} at col {}", kind, e.col),
                    &file, e.line, e.col as usize,
                ),
            })
            .collect());
    }

    // Правило семейств (COVERAGE §1.3, Fanuc-составы v0.8): сканер отмечает
    // семейство целиком до диспозиций. Активные коды — вне enum (raw-таблица),
    // поэтому скан по сырым G-значениям unsupported_words.
    let g_family_t = |b: &Block, fam: &[i64]| {
        b.unsupported_words.iter().any(|w| {
            if w.letter != 'G' {
                return false;
            }
            let t = (w.num.value * 10.0).round() as i64;
            (w.num.value * 10.0).round() == w.num.value * 10.0 && fam.contains(&t)
        })
    };
    const G40_FAMILY: [i64; 16] = [
        410, 411, 412, 413, 414, 415, 416, // G41/.1/.2-.6
        420, 421, 422, 423, 424, 425, 426, // G42/.1/.2-.6
        380, 390, // G38/G39
    ];
    const G49_FAMILY: [i64; 5] = [430, 431, 434, 435, 440]; // G43/.1/.4/.5, G44
    let family_g40 = parsed.blocks.iter().any(|b| g_family_t(b, &G40_FAMILY));
    let family_g49 = parsed.blocks.iter().any(|b| g_family_t(b, &G49_FAMILY));
    // Семейство G11 (COVERAGE §1.3): RAW-формы G10 = L20 / P0 / L1/L10/L11/L50+
    // ( Fanuc-семантика; L2 P1–P6 — легальный wcs_define, семью не триггерит).
    let family_g10_raw = parsed
        .blocks
        .iter()
        .any(|b| b.g_codes.contains(&GCode::DataRecord) && g10_form_is_raw(b));

    let mut ctx = Ctx {
        file: file.clone(),
        modal: ModalState::default(),
        feed: None,
        feed_restated: false,
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
        pending_dpi: RefCell::new(Vec::new()),
        end_count: 0,
        end_line: 0,
        family_g40,
        family_g49,
        family_g10_raw,
    };

    for b in parsed.blocks.iter() {
        process_block(&mut ctx, b)?;
    }
    finalize(&mut ctx, line_count as u32)
}

/// G10-форма блока RAW? (Fanuc-семантика, COVERAGE §2.3: L20 = запись G54.1 Pn
/// — НЕ wcs_set_here; P0 = external offset; L1/L10/L11/L50+ = таблицы/параметры;
/// также сломанные формы — без L / L2 без P / P вне 1..6).
fn g10_form_is_raw(b: &Block) -> bool {
    let l = b.word('L').map(|n| (n.value * 10.0).round() as i64 / 10);
    let p = b.word('P').map(|n| (n.value * 10.0).round() as i64 / 10);
    match l {
        None => true, // G10 без L — сломанная форма (Raw SYNTAX → семейство)
        Some(2) => match p {
            None => true,
            Some(pv) => !(1..=6).contains(&pv),
        },
        Some(_) => true, // L20/L1/L10/L11/L50+ — Fanuc RAW-формы
    }
}

/// Структурная проверка program_end (SPEC §3 + анализ §4.4/§7.3):
/// Fanuc-политика хвоста — ВСЁ после первого program_end — line-level
/// Raw-хвост {E_UNSUPPORTED_SYNTAX} (тела subs O#### в том же файле —
/// норма Fanuc; ruling 1 оркестратора, COVERAGE §4 «O-слова»), НЕ жёсткий
/// Err — осознанное расхождение с grbl n12: raw-сохранение хвоста честно,
/// motion-опы после PEND не строятся. Дубль M2/M30 — defensive-guard
/// (недостижим при tail-политике). Отсутствие M2/M30 → auto-PEND + notice.
///
/// Поле ex-`end_is_last` удалено (мутраунд-2, dead-code): Fanuc хвост после
/// PEND — line-level Raw (см. process_block (0)), НЕ Err — флаг
/// «end последний?» нигде не читался (grbl n12 — другой адаптер).
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
    if ctx.end_count == 0 {
        // SPEC §3 v0.16 / анализ §4.4: DNC-файлы Fanuc часто без M30 —
        // авто-вставка ProgramEnd последним опом + notice.
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
        // Fanuc G61 = exact_stop (value shift, COVERAGE §2.2).
        path_mode: ctx.modal.path.clone().unwrap_or(PathMode::ExactPath),
        frame_preset: ctx.preset.clone(), // G92 (ruling брифа 2a)
        ops: std::mem::take(&mut ctx.ops),
    };
    let notices = std::mem::take(&mut ctx.notices);
    Ok(ParseOut { program, notices })
}

// ---------------------------------------------------------------------------
// process_block — диспетчер одного кадра
// ---------------------------------------------------------------------------

/// План кадра (приоритет: one-shot G4 > frame G92/G10 > цикл 09 > движение 01).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Plan {
    Dwell,
    Frame,
    Cycle { repeat: bool }, // repeat = X|Y слова в кадре (триппер сверления)
    Motion,
    Noop,
}

#[allow(clippy::too_many_lines)]
fn process_block(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    // (0) Fanuc-политика хвоста (ruling 1; анализ §4.4/§7.3; COVERAGE §4
    // «O-слова»): блоки после program_end — тела subs (O####) → line-level
    // Raw{SYNTAX}, НЕ Err (осознанное расхождение с grbl n12 — Fanuc-специфика:
    // subs-тела после M30 в одном файле — норма); motion-опы после PEND не
    // строятся (SPEC §3: PEND — последний оп).
    if ctx.end_count > 0 {
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSyntax,
            "block after program end: subprogram tail preserved raw (ruling 1; analysis §4.4/§7.3; COVERAGE §4 O-words)",
        );
        ctx.push(op);
        return Ok(());
    }

    // (1) Строковые маркеры.
    match &b.marker {
        Some(Marker::Percent) => return Ok(()), // IGNORED_SAFE (§1.3)
        Some(Marker::BlockDelete) => {
            // §1.3: исполнение зависит от переключателя станка —
            // RAW(E_RUNTIME_STATE), как LinuxCNC-строка COVERAGE §4.
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

    // (2) Macro B (§4.3, COVERAGE §4): `#`-слова / `[ ]`-выражения / letter-run
    // statement'ы → весь блок line-level Raw{SYNTAX} (параметрика = L2,
    // этап 12; декомпозиция запрещена).
    if b.macro_b {
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSyntax,
            "Macro B construct (#-variable / []-expression / IF-GOTO-WHILE-DO-END) — parametric, line-level Raw (analysis §4.3; COVERAGE §4)",
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
    // G91 → следующий `G0 X10 Y10` молча абсолютный). Для легальных кадров
    // тот же код выполнял шаги (9)–(12) прежней нумерации — порядок эффектов
    // не изменился.
    absorb_frame_state(ctx, b)?;

    // (4) G95 — детект для RAW (§2.2: feed/rev требует rpm, зависящего от
    // override; статическая эквивалентность не гарантируется).
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
    // первым приоритетом, затем generic-ведро Raw{SYNTAX} + диагностика текста.
    // SPEC §8.6 (v0.17): raw-motion-G (G28/G33/G84…) с осями — модальное
    // состояние уже абсорбировано в (3a); motion-режим НЕ продолжается
    // (ход не представим — «продолжение» молча интерпретировало бы следующий
    // X/Y-кадр в старом режиме) + notice W_MODAL_ON_RAW.
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
            format!("G-code G{val}: outside Fanuc mill dictionary — machine option / profile stage 8 (COVERAGE §2 generic bucket; not silent)")
        } else {
            format!("M-code M{val}: builder-specific — outside Fanuc mill dictionary (COVERAGE §3 generic bucket; not silent)")
        };
        let op = ctx.raw_op(b, ErrorCode::UnsupportedSyntax, &reason);
        ctx.push(op);
        return Ok(());
    }

    // (6) M19/M29 → Raw{SEMANTICS}; M98/M99/M198 → Raw{SYNTAX} (§3 отчёта).
    for m in &b.m_codes {
        let (code, reason) = match m {
            MCode::SpindleOrient => (
                ErrorCode::UnsupportedSemantics,
                "M19 spindle orientation — encoder-dependent (COVERAGE §3)",
            ),
            MCode::RigidTapMode => (
                ErrorCode::UnsupportedSemantics,
                "M29 rigid tapping mode — paired with G84/G74 spindle sync (COVERAGE §3)",
            ),
            MCode::SubCall => (
                ErrorCode::UnsupportedSyntax,
                "M98 subprogram call — Macro B / subs mechanics, line-level Raw (analysis §4.2; COVERAGE §3)",
            ),
            MCode::SubReturn => (
                ErrorCode::UnsupportedSyntax,
                "M99 subprogram return / main loop — not static (analysis §4.2; COVERAGE §3)",
            ),
            MCode::SubCallExt => (
                ErrorCode::UnsupportedSyntax,
                "M198 external subprogram call (card/DNC) — outside file, doubly not static (COVERAGE §3)",
            ),
            _ => continue,
        };
        absorb_frame_structs(ctx, b)?;
        let op = ctx.raw_op(b, code, reason);
        ctx.push(op);
        return Ok(());
    }

    // (7) G52 → жёсткий отказ (ERR, SPEC §8.5; COVERAGE §2.3 — единственный
    // ERR Fanuc-матрицы): позиционная смена фрейма mid-program — raw-сохранение
    // исказило бы интерпретацию последующих ходов.
    if b.g_codes.contains(&GCode::LocalCs) {
        return Err(vec![err(
            ErrorCode::UnsupportedSemantics,
            "G52 local coordinate system — mid-program frame shift, hard ERR class (SPEC §8.5; COVERAGE §2.3)",
            &ctx.file, line, blen,
        )]);
    }

    // (8) G92 S → Raw{SEMANTICS} — кламп оборотов: тот же адрес G92, другой
    // word (COVERAGE §2.6: парсер обязан различать, НЕ frame_preset).
    if b.g_codes.contains(&GCode::Preset) && b.word('S').is_some() {
        absorb_frame_structs(ctx, b)?; // без осевых слов — no-op, форма-только
        let op = ctx.raw_op(
            b,
            ErrorCode::UnsupportedSemantics,
            "G92 S spindle speed clamp — same address, different word, not frame_preset (COVERAGE §2.6)",
        );
        ctx.push(op);
        return Ok(());
    }

    // (9)–(12) — бывшие шаги units/ABSORB/path/F-S-M — перенесены в
    // absorb_frame_state (вызов в (3a), ДО raw-возвратов; SPEC §8.6 v0.17).

    // (13) План кадра (приоритет Fanuc-групп: one-shot 00 > frame > 09 > 01).
    let has_axis = b.word('X').is_some() || b.word('Y').is_some() || b.word('Z').is_some();
    let has_arc_words = b.word('I').is_some()
        || b.word('J').is_some()
        || b.word('K').is_some()
        || b.word('R').is_some();
    let explicit_motion = b.motion();
    let explicit_cycle = b.g_codes.iter().copied().find_map(CycleKind::from_gcode);
    // Триппер сверления (анализ §5.1): G90 — X/Y-слово; G91 — X/Y/Z (Z-шаг
    // между отверстиями); блок с только R/Q/P/F — обновление параметров.
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
        // (§2.7 отчёта; COVERAGE §2.1 «голая строка после G80»).
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "naked coordinate words: no active motion mode (analysis §2.7)",
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

    // (14) Слова без потребителя — молчаливый пропуск запрещён.
    // Feature-слова вне скоупа → Raw с per-letter диспозицией COVERAGE;
    // context-слова (P/Q/L/K/R/I/J/K) без контекста — сломанный вход (Err).
    // H00 = cancel TLO (COVERAGE §2.4) — потреблён как cancel-код.
    let h_zero = b.word('H').map(|w| w.value == 0.0).unwrap_or(false);
    if let Some((code, reason)) = unused_word_diagnostic(b, plan, motion_mode, h_zero) {
        if code == ErrorCode::InvalidInput {
            return Err(vec![err(code, reason, &ctx.file, line, blen)]);
        }
        let op = ctx.raw_op(b, code, &reason);
        ctx.push(op);
        return Ok(());
    }

    // (15) Cancel-коды семейств (COVERAGE §1.3, Fanuc-составы): G40/G49/G11
    // (и H00 — cancel TLO, §2.4) — IGNORED_SAFE, если активных кодов семейства
    // в файле нет; иначе Raw (raw-наследование — иначе round-trip оставил бы
    // компенсацию активной навсегда).
    let h_cancel = h_zero
        && !b
            .unsupported_words
            .iter()
            .any(|w| w.letter == 'G' && [430.0, 431.0, 434.0, 435.0, 440.0].contains(&w.num.value));
    let mut family_cancel: Option<(ErrorCode, &'static str)> = None;
    for g in &b.g_codes {
        family_cancel = match g {
            GCode::CompCancel if ctx.family_g40 => Some((
                ErrorCode::UnsupportedSemantics,
                "G40 cancel with active G41/G42 family in file — family rule (COVERAGE §1.3)",
            )),
            GCode::LenCompCancel if ctx.family_g49 => Some((
                ErrorCode::RequiresMachineTable,
                "G49 cancel with active G43/G44 family in file — family rule (COVERAGE §1.3)",
            )),
            GCode::DataCancel if ctx.family_g10_raw => Some((
                ErrorCode::UnsupportedSyntax,
                "G11 cancel with RAW G10-forms (L20/P0/L1+) in file — family G11 rule (COVERAGE §1.3)",
            )),
            _ => None,
        };
        if family_cancel.is_some() {
            break;
        }
    }
    if h_cancel && family_cancel.is_none() && ctx.family_g49 {
        family_cancel = Some((
            ErrorCode::RequiresMachineTable,
            "H00 cancel TLO with active G43/G44 family in file — family rule (COVERAGE §2.4)",
        ));
    }
    if let Some((code, reason)) = family_cancel {
        let op = ctx.raw_op(b, code, reason);
        ctx.push(op);
        return Ok(());
    }

    // (16) G92 → frame_preset: только пресетка до первого motion-опа
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

    // (17) G10: L2 P1–P6 → wcs_define (G90/G91 — абсолют/инкремент записи,
    // §5.3 SPEC); L2 P0 → Raw{REQUIRES}; L20 → Raw{REQUIRES} — Fanuc-семантика
    // «запись G54.1 Pn», НЕ wcs_set_here (ловушка №3); L1/L10/L11/L50+ →
    // Raw{REQUIRES}; сломанные формы (без L / L2 без P) — Err(INVALID).
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
                    "G10 L20 Fanuc semantics = origin write for G54.1 Pn, NOT wcs_set_here (trap #3; COVERAGE §2.3)",
                );
                ctx.push(op);
                return Ok(());
            }
            Some(lv) if lv as i64 != 2 => {
                let op = ctx.raw_op(
                    b,
                    ErrorCode::RequiresMachineTable,
                    "G10 L-tool/parameter table record — machine table data (COVERAGE §2.3)",
                );
                ctx.push(op);
                return Ok(());
            }
            Some(_) => {
                // L2: P обязателен, домен Fanuc 1..6.
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
                if !(1..=6).contains(&pi) || (pv.fract() != 0.0) {
                    let (code, reason) = if pi == 0 {
                        (
                            ErrorCode::RequiresMachineTable,
                            "G10 L2 P0 external/common offset — shift over all WCS, concept outside L1 (COVERAGE §2.3)".to_string(),
                        )
                    } else {
                        (
                            ErrorCode::UnsupportedSyntax,
                            format!(
                                "G10 L2 P{pv} outside Fanuc mill wcs domain 1..6 (COVERAGE §2.3)"
                            ),
                        )
                    };
                    let op = ctx.raw_op(b, code, &reason);
                    ctx.push(op);
                    return Ok(());
                }
                // Partial G10 L2 (не все X/Y/Z) → Raw{REQUIRES}: невыписанные
                // оси Fanuc сохраняет неизменными — значение из таблицы станка,
                // вне текста (ruling брифа этапа 2a «по общему правилу»).
                if b.word('X').is_none() || b.word('Y').is_none() || b.word('Z').is_none() {
                    let op = ctx.raw_op(
                        b,
                        ErrorCode::RequiresMachineTable,
                        "partial G10 L2 — unspecified axes keep their machine-table value, outside text (brief stage 2a; COVERAGE §2.3)",
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

    // (18) G4 dwell (§2.7/§1.2): P — целые миллисекунды (decimal point
    // запрещён Fanuc); X/U — секунды float (U-исключение из общего отказа,
    // §4.1). `G04 P0` → drop+notice (SPEC §5.2 dwell=0).
    if plan == Plan::Dwell {
        return dwell_block(ctx, b);
    }

    // (19) M6 → ToolChange (+T в блоке; Fanuc-идиома `T05 M06`); M6 без T —
    // сломанный вход. Одиночное T отсечено в (14) → Raw{SEMANTICS}.
    if b.m_codes.contains(&MCode::ToolChange) {
        let tw = b.word('T').ok_or_else(|| {
            vec![err(
                ErrorCode::InvalidInput,
                "M6 without T word (COVERAGE §3; Fanuc idiom T..M6)",
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

    // (20) M0/M1 → ProgramStop (optional=false/true; Q8).
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

    // (21) Движение / цикл.
    match plan {
        Plan::Cycle { repeat } => {
            cycle_block(ctx, b, cycle_kind.expect("cycle plan → kind"), repeat)?;
        }
        Plan::Motion => {
            motion_block(ctx, b)?;
        }
        _ => {}
    }

    // (22) M2/M30 → ProgramEnd (после motion — порядок grbl #9; SPEC §5.2).
    if b.m_codes.contains(&MCode::ProgramEnd) {
        ctx.mark_end(line, blen);
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

/// raw-таблица: G-коды, чей блок с осями — ход движения (motion-класс).
/// t = round(value*10). Home G27–G30(.1); G45–G48; probing G31(.8)/G37;
/// threading G33–G36; интерполяции-опции G2.2/.3/.4, G3.2/.3/.4, G6.2;
/// циклы-отказы G74/G76/G84(.2/.3)/G87/G88; RTCP G53.1/G68.2; вендорские
/// motion-режимы G81.1 (chopping). Не-motion (компенсации/TLO/трансформации/
/// Macro B) — НЕ в списке: их raw-кадр не является ходом.
const RAW_MOTION_G: &[i64] = &[
    270, 280, 290, 300, 301, // G27/G28/G29/G30/.1 — home
    450, 460, 470, 480, // G45–G48 — ±H positioning
    310, 318, 370, // G31/G31.8 probing, G37 measurement
    330, 340, 350, 360, // G33–G36 threading
    22, 32, 23, 33, 24, 34, 62, // G2.2/.3/.4, G3.2/.3/.4, G6.2
    740, 760, 840, 842, 843, 870, 880, // G74/G76/G84.2/.3/G87/G88 cycles
    531, 682, // G53.1/G68.2 RTCP
    811, // G81.1 chopping
];

/// Абсорбция frame-level состояния кадра (SPEC §8.6 правило 6, v0.17).
/// Вызывается ДО любых raw-возвратов: строка, уходящая в Raw целиком, обязана
/// абсорбировать модальные группы (plane/distance/arcdist/feedmode/units/wcs/
/// path/retract, G80), F/S, M3/M4/M5, M7/M8/M9, G92 (пресетка), G10 L2
/// (таблица origin) — иначе следующий кадр парсится в потерянной
/// модальности (пробой аудита: `G91 G28 Z0` терял G91 → следующий
/// `G0 X10 Y10` молча абсолютный — точное нарушение правила).
/// Для легальных кадров функция выполняет шаги (9)–(12) прежней нумерации
/// (units-проверка, ABSORB, path, F/S/M-состояние).
///
/// selected_tool (голое T) — вне абсорбции: в State L1 поля нет
/// (только физический tool через M6), абсорбция исказила бы tool-снапшоты;
/// одиночное T остаётся Raw{SEMANTICS} (candidate `selected_tool`, этап 12).
///
/// Хвост после PEND — НЕ абсорбируется (process_block (0)): программа
/// структурно закончена, G92 из sub-тела исказил бы frame_preset.
fn absorb_frame_state(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();

    // Провенанс F-кадра — по кадру (v0.2 волна 3): сброс на начале блока.
    ctx.feed_restated = false;

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
    // группы 01 отменяет активный цикл; G80 — отмена cycle-моды.
    for g in &b.g_codes {
        if modal_group_is_motion(*g) || *g == GCode::CycleCancel {
            ctx.cycle = None;
        }
        ctx.modal.apply(*g);
    }

    // (c) Path-режим (группа 13): глобален (§3); переключение — отказ
    // (и на RAW-кадре — метаданные программы).
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

    // (d) F — модальная подача (ruling 2: bare F = целое mm/min verbatim +
    // notice; notice прицепится к raw-опу через pending_dpi). S — обороты,
    // КРОМЕ G92-кадра (кламп, не команда — шаг (8) даёт raw; абсорбция
    // исказила бы шпиндель).
    if let Some(f) = b.word('F') {
        let num = ctx.feed_num(f);
        ctx.feed = Some(num);
        // F-слово было в кадре (restated-F, SPEC §7.4a): маркер кадра важен
        // независимо от того, изменилось ли ЗНАЧЕНИЕ модального feed.
        ctx.feed_restated = true;
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

    // (e) M3/M4/M5 → шпиндель; M7/M8/M9 → СОЖ.
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
            MCode::CoolantMist => ctx.coolant.mist = true,
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
/// G92/G10 в основных шагах (16)/(17); G10-обновление таблицы idempotent
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
    if b.g_codes.contains(&GCode::DataRecord) && !g10_form_is_raw(b) {
        if let (Some(lv), Some(pv)) = (b.word('L').map(|n| n.value), b.word('P').map(|n| n.value)) {
            let pi = pv as i64;
            if lv == 2.0
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
/// (E_INVALID_INPUT, причина) — сломанный вход (паритет LinuxCNC-адаптера).
fn unused_word_diagnostic(
    b: &Block,
    plan: Plan,
    motion_mode: Option<GCode>,
    h_zero: bool,
) -> Option<(ErrorCode, String)> {
    let mut consumed: Vec<char> = vec!['F', 'S'];
    match plan {
        Plan::Dwell => consumed.extend(['P', 'X', 'U']),
        Plan::Frame => {
            consumed.extend(['X', 'Y', 'Z']);
            if b.g_codes.contains(&GCode::DataRecord) {
                // P/L/R — запись G10 (§6.71 COVERAGE): в официальном Fanuc G10
                // L10 (таблица инструментов) R = geometry-значение (L10 =
                // длина-geometry/H, L12 = диаметр-geometry/D — R-словом).
                // Потребляем все три — строка доходит до шага (17) → Raw{REQUIRES}
                // (паритет eid-haas; до фикса `G10 L10 ... R250.` падал в
                // Err(INVALID) «R word without consumer» на шаге (14)).
                consumed.extend(['P', 'L', 'R']);
            }
        }
        Plan::Cycle { .. } => consumed.extend(['X', 'Y', 'Z', 'R', 'Q', 'P', 'K', 'L']),
        Plan::Motion => {
            consumed.extend(['X', 'Y', 'Z']);
            if matches!(motion_mode, Some(GCode::ArcCw) | Some(GCode::ArcCcw)) {
                consumed.extend(['I', 'J', 'K', 'R']);
            }
        }
        Plan::Noop => {}
    }
    if b.m_codes.contains(&MCode::ToolChange) {
        consumed.push('T');
    }
    // H00 — cancel TLO (обработан в шаге cancel-семейств).
    if h_zero {
        consumed.push('H');
    }
    for letter in [
        'T', 'H', 'D', 'U', 'V', 'W', 'A', 'B', 'C', 'E', 'O', 'P', 'Q', 'R', 'I', 'J', 'K', 'L',
    ] {
        if b.word(letter).is_none() || consumed.contains(&letter) {
            continue;
        }
        return Some(match letter {
            'T' => (
                ErrorCode::UnsupportedSemantics,
                "T word alone — modal next-tool preload, outside L1; candidate `selected_tool` (COVERAGE §3)".to_string(),
            ),
            'H' | 'D' => (
                ErrorCode::RequiresMachineTable,
                format!("{letter} offset number — machine table value (COVERAGE §4 H/D words)"),
            ),
            'U' | 'V' | 'W' => (
                ErrorCode::UnsupportedSemantics,
                format!("{letter} extra linear axis — stage 12 (COVERAGE §4)"),
            ),
            'A' | 'B' | 'C' => (
                ErrorCode::UnsupportedSemantics,
                format!("{letter} rotary axis — stage 12 (COVERAGE §4)"),
            ),
            'E' => (
                ErrorCode::UnsupportedSyntax,
                "E word (G05.1 precision) — vendor mode (COVERAGE §4)".to_string(),
            ),
            'O' => (
                ErrorCode::UnsupportedSyntax,
                "O word — program number / subprogram structure, outside L1 (COVERAGE §4)".to_string(),
            ),
            _ => (
                ErrorCode::InvalidInput,
                format!("{letter} word without consumer in block (arc/cycle/G10/G4 context expected)"),
            ),
        });
    }
    None
}

// ---------------------------------------------------------------------------
// G4 dwell (P — целые мс; X/U — секунды)
// ---------------------------------------------------------------------------

fn dwell_block(ctx: &mut Ctx, b: &Block) -> Result<(), Vec<Diagnostic>> {
    let line = b.line_no;
    let blen = b.raw.len();
    let p = b.word('P');
    let x = b.word('X');
    let u = b.word('U');
    let present = [p.is_some(), x.is_some(), u.is_some()]
        .iter()
        .filter(|&&v| v)
        .count();
    if present == 0 {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "G4 without dwell word P/X/U (analysis §1.2/§2.7)",
            &ctx.file,
            line,
            blen,
        )]);
    }
    if present > 1 {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            "G4 with multiple dwell words P/X/U (analysis §1.2)",
            &ctx.file,
            line,
            blen,
        )]);
    }
    // P — целые мс (integer-домен, DPI не участвует); X/U — секунды float
    // (дробный домен — DPI-политика, bare X5 → 0.005 s).
    let seconds = if let Some(w) = p {
        w.value / 1000.0
    } else if let Some(w) = x {
        ctx.dpi_num('X', w).value()
    } else {
        ctx.dpi_num('U', u.expect("dwell word")).value()
    };
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
// Движение (G0–G3 / модальное продолжение; G53 machine-frame)
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
    // machine-ходы: только полный end (нуль не наследуется, SPEC §2 v0.13).
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
                "G53 requires explicit G0/G1 in the block (Fanuc manual; SPEC §5.3)",
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
            // G93 (ruling 4): feed = len·F (SPEC §4); требует known start.
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

/// Дуга G2/G3 (§2.1 отчёта, SPEC §6): IJK — инкрементальные offsets от start
/// ВСЕГДА (независимо от G90/G91); G90.1 (группа 04, опция) — абсолютный
/// центр как LinuxCNC. R-форма: знак R выбирает ≤180°/>180°; R при полной
/// окружности → E_INVALID_INPUT. Helical — вне-плоскостное осевое слово →
/// pitch; полный круг (center-форма, center ≠ start, start == end) →
/// sweep 360° НЕЗАВИСИМО от наличия явных end-слов (SPEC §3 v0.17);
/// вырожденный центр (center == start) при end == start → drop+notice.
/// Валидация нормали по активной плоскости (SPEC §4/§6): перпендикулярная
/// компонента (center − start) ≈ 0, иначе E_INVALID_INPUT.
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
            // (SPEC §6(2); R-полный круг — alarm обоих контроллеров; §6 отчёта).
            if chord_plane < 1e-9 {
                return Err(vec![inval(
                    "R-arc with coincident in-plane start/end: ambiguous center — full circle only via IJK (SPEC §6; analysis §6)".into(),
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
                // G90.1: абсолютный центр (как LinuxCNC; анализ §2.2 —
                // по умолчанию Fanuc инкрементальный, G90.1 — опция).
                c[a0] = off_vals[a0];
                c[a1] = off_vals[a1];
            } else {
                // G91.1-эквивалент (дефолт Fanuc): инкремент от start ВСЕГДА,
                // независимо от G90/G91 (§2.1 отчёта).
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
        // (полная окружность: нет end-слов + center ≠ start, §6 отчёта).
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
    // start == end = полный круг 360° НЕЗАВИСИМО от формы команды (явные
    // end-слова или наследованные; v0.17 по зонд-доказательству rs274 —
    // реальный контроллер интерпретирует `G2 X10. Y0. I-5. J0` из (10,0)
    // как полный оборот, CAM-посты систематически пишут end-слова на
    // полных кругах). Критерий «форма команды» v0.13 действовал только
    // для R-формы и degenerate-центра.
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
// Циклы группы 09 (§5 отчёта — Fanuc-семантика; канон-шаблон §7.7 SPEC)
// ---------------------------------------------------------------------------

/// Обновление/создание модального цикла из слов кадра (параметры группы 09
/// модальны; повторные блоки обновляют Z/R/Q/P — §5.1).
fn cycle_block(
    ctx: &mut Ctx,
    b: &Block,
    kind: CycleKind,
    repeat: bool,
) -> Result<(), Vec<Diagnostic>> {
    // K0/L0 в G91 → Raw{SEMANTICS} — спец-семантика «движение по одной оси»
    // (§5.2 отчёта; `?` реестр COVERAGE §6.17; разворачивание не делается).
    if !ctx.modal.dist_abs {
        for letter in ['K', 'L'] {
            if let Some(w) = b.word(letter) {
                if w.value == 0.0 {
                    let op = ctx.raw_op(
                        b,
                        ErrorCode::UnsupportedSemantics,
                        "K0/L0 in G91 — special one-axis semantics, not unrolled (analysis §5.2; COVERAGE §2.5)",
                    );
                    ctx.push(op);
                    return Ok(());
                }
            }
        }
    }

    // Обновить/создать модальные параметры цикла.
    let mut cyc = ctx.cycle.take().unwrap_or(ActiveCycle {
        kind,
        z: None,
        r: None,
        q: None,
        p_s: None,
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
        // P dwell цикла: bare P — целые МИЛЛИСЕКУНДЫ (Fanuc-контракт, §1.2:
        // P500 = 0.5 s); dotted P (сломанная для Fanuc форма, alarm на
        // контроллере) — СЕКУНДЫ verbatim — rs274/LinuxCNC-совместимое
        // чтение (SPEC §7.4a v0.18, зеркало D: golden #21 P1.0 → 1.0 s).
        cyc.p_s = Some(if w.has_point {
            w.value
        } else {
            w.value / 1000.0
        });
    }
    let k = repeat_count(ctx, b)?;
    ctx.cycle = Some(cyc.clone());
    if !repeat {
        return Ok(()); // параметр-кадр без X/Y — сверления нет (§5.1)
    }
    unroll_cycle(ctx, b, &cyc, k)
}

/// K/L повтор кадра (0–9999; `?` §10(2) — L на части сериях).
fn repeat_count(ctx: &Ctx, b: &Block) -> Result<u32, Vec<Diagnostic>> {
    let w = match b.word('K').or_else(|| b.word('L')) {
        Some(w) => w,
        None => return Ok(1),
    };
    if w.value.fract() != 0.0 || w.value < 0.0 || w.value > 9999.0 {
        return Err(vec![err(
            ErrorCode::InvalidInput,
            format!(
                "K/L repeat count {} outside integer domain 0..9999 (analysis §1.2)",
                w.text
            ),
            &ctx.file,
            b.line_no,
            b.raw.len(),
        )]);
    }
    Ok(w.value as u32)
}

/// Развёртывание цикла (SPEC §7.7 канон-шаблон; SPEC §7.4a v0.18 — зеркало D).
/// Последовательность на отверстие (L/K → повтор всей последовательности):
/// 1. RAPID traverse к отверстию НА ТЕКУЩЕМ Z (не сливать с подходом);
/// 2. RAPID подход к R-плоскости;
/// 3. LINEAR feed на дно; [DWELL P (G82/G86/G89 — обязателен)];
///    [пеки G83/G73: депеки R−kQ; G83 — отвод на R + подход d+δ;
///    G73 — одиночный rapid d+δ; δ = 0.254mm/0.01in unit-scaled];
///    [G86: DWELL перед отводом, шпиндель off];
/// 4. retract: G99 → RAPID (R); G98 → RAPID (initial Z); G85/G89 — отвод на
///    подаче; G86 — rapid при шпинделе off, рестарт в исходном направлении
///    (Q9) — в state последующих опов.
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
            "cycle as first motion op — position not established by preceding moves (SPEC §7.6)"
                .into(),
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

    // Лимит max_unroll_iterations отдельной проверкой K/L здесь НЕ нужен:
    // repeat_count каппирует домен на 0..=9999 < 10 000 (мутраунд-2: ветка
    // k > MAX была доказуемо мёртвой — 2 эквивалентных мутанта оператора).
    // Живой лимит — пек-оценка est_total ниже (COVERAGE §1.2).

    // R и Z обязательны (Fanuc manual: «It is an error if…» — паритет LinuxCNC).
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
    // дословно; прежнее «опционален `?` §10(6)» — закрыто).
    let dwell = match cyc.kind {
        CycleKind::DrillDwell | CycleKind::BoreDwell => Some(cyc.p_s.ok_or_else(|| {
            fl(
                ErrorCode::InvalidInput,
                "cycle G82/G89 without P word (dwell, integer ms per Fanuc)".into(),
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
    // Q: G73/G83 — обязателен, строго > 0 (иначе бесконечные пеки).
    let q = match cyc.kind {
        CycleKind::Peck | CycleKind::ChipBreak => {
            let q = cyc.q.ok_or_else(|| {
                fl(
                    ErrorCode::InvalidInput,
                    "peck cycle G73/G83 without Q word (peck depth)".into(),
                )
            })?;
            if q <= 0.0 {
                return Err(fl(
                    ErrorCode::InvalidInput,
                    "Q must be > 0 (peck depth)".into(),
                ));
            }
            Some(q)
        }
        _ => None,
    };

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
            state: ctx.state_derived(sub),
            src: src.clone(),
            sub: Some(sub),
        };
        push_cycle_move(ctx, traverse.clone());
        sub += 1;
        // 2. подход к R-плоскости — отдельный оп (§7.7 #2); при cz == r_cur
        // (после отвода предыдущего отверстия) — zero-length → drop+notice.
        let approach = Op::Rapid {
            start: StartPoint::Known(pt(cx, cy, cz)),
            end: pt(cx, cy, r_cur),
            state: ctx.state_derived(sub),
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
                        state: ctx.state_derived(sub),
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
                // §7.4a (v0.18, зонд-сверка rs274 — зеркало D): порядок пиков
                // точный. Депеки ОБА цикла — расписание G83: глубины от
                // R-плоскости вниз шагами Q до z_bottom:
                // d_k = max(z_bottom, r_cur − k·q). Промежуточный отвод:
                // G83 — полный отвод на R, затем подход rapid на d+клиренс;
                // G73 — ОДИН rapid на d+клиренс (модели «de-retract
                // min(R, d+Q) + отдельный подход» в интерпретаторе НЕТ;
                // probe `g73_r2`: депеки R−Q…, отводы d+0.254, финал на R;
                // −2 опа на цикл против прежней модели). Финальный отвод —
                // общий ниже (G99 → R, G98 → initZ).
                let q = q.expect("пекерующий цикл без Q (проверено выше)");
                let t = r_cur - z_bottom;
                // n ≥ 1 во всех ветках: t ≤ 0 (дно на/выше R-плоскости) →
                // один пек на дно; t > 0 → ceil(t/q) ≥ 1. Прежняя избыточная
                // нормализация `if n == 0 { n = 1 }` дублировала else-ветку и
                // делала мутанта `>` → `>=` эквивалентным (мутраунд-2) —
                // дедуплицировано.
                let n = if t > 0.0 { (t / q).ceil() as u64 } else { 1 };
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
                for kk in 1..=n {
                    let d = (r_cur - kk as f64 * q).max(z_bottom);
                    push_cycle_move(
                        ctx,
                        cyc_linear(ctx, [cx, cy, cz], [cx, cy, d], &feed, src.clone(), sub),
                    );
                    sub += 1;
                    cz = d;
                    if kk == n {
                        break; // финальный отвод — общий ниже (§7.1)
                    }
                    // G83: полный отвод на R (SPEC §7.4a). G73 (v0.18):
                    // единственный промежуточный ход — подход ниже;
                    // «de-retract min(R, d+Q)» в интерпретаторе не существует.
                    if cyc.kind == CycleKind::Peck {
                        let op = Op::Rapid {
                            start: StartPoint::Known(pt(cx, cy, cz)),
                            end: pt(cx, cy, r_cur),
                            state: ctx.state_derived(sub),
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
                        state: ctx.state_derived(sub),
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
                    state: ctx.state_derived(sub),
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
                    state: ctx.state_derived(sub),
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
/// отсекает; найдено смоук-этапа 3: подход G82 после отвода G83 на ту же
/// R-плоскость даёт нулевой ход → drop + notice, не оп).
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
    let mut st = ctx.state_derived(sub);
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
