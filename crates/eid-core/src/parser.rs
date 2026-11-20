//! EID/L1 — парсер канонического текстового формата (grammar.ebnf v0.3, правила
//! D1–D10) и валидатор программы (SPEC-EID-L1 v0.15: §3 инварианты, §5 опы,
//! §6 каноническая форма дуги, §2 принцип 2 / Q11 — точки разрыва и связывание).
//!
//! Поток разбора: текст → [`parse_text`] (лексика/структура; нарушение —
//! `E_INVALID_INPUT`) → [`canonicalize`] (отбрасывание zero-length с notice +
//! валидация §3/§6) → [`Program`]. Для пути эмиссии — [`validate`]: та же
//! валидация без отбрасывания (в каноническом AST zero-length уже отсутствуют;
//! наличие — ошибка, см. `emitter::emit`).
//!
//! Замечания по интерпретации SPEC (зафиксированы в отчёте итерации 1):
//!  * точка разрыва (b) Q11 реализована через критерий «разность origin'ов
//!    не вычислима из текста» — нужны `wcs_define` ОБЕИХ систем (старой и
//!    новой), т.к. reading_new − reading_prev = O_prev − O_new (EQUIVALENCE
//!    §1.3, Δ константен). Буквальное «новая система без define» покрывает
//!    только половину случаев (старая без define — тоже невычислимо);
//!  * точки разрыва (c)/(d) Q11 реализованы тем же критерием вычислимости:
//!    machine-ходы выразимы при in-text origins и отсутствии позиционно-
//!    относительных пресеток (SPEC §5.3), в этом случае start обязан быть
//!    Known (обязанность связывания, SPEC §2 v0.9). unknown требуется ровно
//!    тогда, когда позиция из текста не вычислима;
//!  * G10 L20 (`wcs_set_here`) задаёт чтение текущей позиции = delta, но
//!    машинное значение origin (O_p = M_here − delta) при M₀-неизвестности
//!    НЕ считается «известным из текста» для кросс-фреймовых преобразований;
//!  * E_RUNTIME_STATE за невыразимость machine-ходов (SPEC §5.3) — диспозиция
//!    канонизатора диалекта-источника; парсер EID-текста не отвергает такие
//!    программы, если их каноническая форма корректна (unknown в точке разрыва).

use crate::ast::*;
use crate::diag::{Diagnostic, ErrorCode};

// ===========================================================================
// Notices (drop-with-notice, SPEC §3 / §5.2 / §6). Notice — не Diagnostic:
// это не код ошибки, а флаг легального отброса (CORPUS §4: notice-файлы —
// только R1-AST).
// ===========================================================================

/// Класс отброшенного опа.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// rapid/linear: start == end (SPEC §3).
    ZeroLengthMotion,
    /// arc: start == end при sweep != 360° и pitch == 0 (SPEC §3/§6; включает
    /// sweep=0 — CORPUS вектор #13).
    ZeroLengthArc,
    /// dwell: seconds == 0 (SPEC §5.2).
    ZeroDwell,
}

/// Отброшенный оп + контекст отброса.
#[derive(Debug, Clone)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
    /// Индекс опа в исходном (до отбрасывания) списке `ops`.
    pub op_index: usize,
    /// Сам отброшенный оп («верни отброшенные»).
    pub op: Op,
}

/// Результат разбора: программа (zero-length отброшены) + notices.
#[derive(Debug, Clone)]
pub struct ParseOutcome {
    pub program: Program,
    pub notices: Vec<Notice>,
}

// ===========================================================================
// Публичные входные точки
// ===========================================================================

/// Текст EID/L1 → Program (notices отбрасываются). Отказ = Vec<Diagnostic>.
pub fn parse(src: &str, file: &str) -> Result<Program, Vec<Diagnostic>> {
    parse_with_notices(src, file).map(|o| o.program)
}

/// Сырой разбор текста БЕЗ канонизации (без отбрасывания zero-length) —
/// для тестов пути эмиссии и диагностики; результат может нарушать SPEC §3.
pub fn parse_raw(src: &str, file: &str) -> Result<Program, Vec<Diagnostic>> {
    parse_text(src, file)
}

/// Текст EID/L1 → Program + notices (отброшенные zero-length опы).
pub fn parse_with_notices(src: &str, file: &str) -> Result<ParseOutcome, Vec<Diagnostic>> {
    let program = parse_text(src, file)?;
    canonicalize(program)
}

/// AST → AST: отбрасывание zero-length (rapid/linear start==end; arc при
/// start==end, sweep≠360°, pitch=0; dwell seconds==0) с notice + валидация
/// SPEC §3/§6 по выжившим опам.
pub fn canonicalize(program: Program) -> Result<ParseOutcome, Vec<Diagnostic>> {
    run(&program, true).map(|(ops, notices)| ParseOutcome {
        program: Program { ops, ..program },
        notices,
    })
}

/// Только валидация (без отбрасывания) — путь эмиссии: zero-length опы в
/// каноническом AST запрещены (их обязана отбросить канонизация, SPEC §3).
pub fn validate(program: &Program) -> Result<(), Vec<Diagnostic>> {
    run(program, false).map(|_| ())
}

// ===========================================================================
// Диагностика
// ===========================================================================

fn diag(code: ErrorCode, msg: impl Into<String>, src: Option<SrcSpan>) -> Diagnostic {
    Diagnostic {
        code,
        message: msg.into(),
        src,
        options: Vec::new(),
        limit: None,
    }
}

fn op_src(op: &Op) -> Option<SrcSpan> {
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
        | Op::Raw { src, .. } => Some(src.clone()),
    }
}

// ===========================================================================
// Лексика (grammar.ebnf, раздел «лексика»; D1, D9, D10)
// ===========================================================================

/// Число D1: `[ "-" ], digits, ".", ровно 4 цифры`; без экспоненты, без "+".
/// Возвращает Num (verbatim-текст + f64-зеркало, SPEC §10.1).
pub fn parse_num(s: &str) -> Option<Num> {
    let b = s.as_bytes();
    let mut i = 0usize;
    if i < b.len() && b[i] == b'-' {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == int_start || i >= b.len() || b[i] != b'.' {
        return None;
    }
    i += 1;
    let frac_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i != b.len() || i - frac_start != 4 {
        return None;
    }
    let value: f64 = s.parse().ok()?;
    Some(Num::new(s.to_string(), value))
}

fn num(s: &str) -> Result<Num, String> {
    parse_num(s).ok_or_else(|| format!("invalid number `{s}` (D1: fixed 4 dp, no exponent)"))
}

/// integer = digits (без знака).
fn integer(s: &str) -> Result<u64, String> {
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
        return Err(format!("invalid integer `{s}`"));
    }
    s.parse::<u64>().map_err(|_| format!("integer `{s}` out of range"))
}

/// vec = number , "," , number , "," , number (X, Y, Z — D2).
fn vec3(s: &str) -> Result<Point3, String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 3 {
        return Err(format!("invalid vec `{s}` (expected X,Y,Z — D2)"));
    }
    Ok(Point3 {
        x: num(parts[0])?,
        y: num(parts[1])?,
        z: num(parts[2])?,
    })
}

/// position = digits "." digits (line.col); span = position "-" position.
/// Плейсхолдер `0.1-0.0` валиден (D10).
fn span(s: &str) -> Result<(u32, u32, u32, u32), String> {
    let halves: Vec<&str> = s.split('-').collect();
    if halves.len() != 2 {
        return Err(format!("invalid span `{s}` (expected line.col-line.col)"));
    }
    let pos = |p: &str| -> Result<(u32, u32), String> {
        let pp: Vec<&str> = p.split('.').collect();
        if pp.len() != 2 || pp.iter().any(|x| x.is_empty() || !x.bytes().all(|c| c.is_ascii_digit())) {
            return Err(format!("invalid position `{p}` (expected digits.digits)"));
        }
        let l: u32 = pp[0].parse().map_err(|_| format!("position `{p}` out of range"))?;
        let c: u32 = pp[1].parse().map_err(|_| format!("position `{p}` out of range"))?;
        Ok((l, c))
    };
    let (ls, cs) = pos(halves[0])?;
    let (le, ce) = pos(halves[1])?;
    Ok((ls, cs, le, ce))
}

/// quoted = '"' { char_without_quote_or_nl } '"' — без escape-механизма.
fn quoted(s: &str) -> Result<String, String> {
    if s.len() < 2 || !s.starts_with('"') || !s.ends_with('"') {
        return Err(format!("invalid quoted string `{s}`"));
    }
    let inner = &s[1..s.len() - 1];
    if inner.contains('"') || inner.contains('\n') || inner.contains('\r') {
        return Err("quoted string must not contain quote/newline (grammar v0.3 has no escapes)".into());
    }
    Ok(inner.to_string())
}

/// hex_digit = 0-9 | A-F; чётная длина (D9); пустой payload грамматичен.
fn hex_payload(s: &str) -> Result<String, String> {
    if !s.bytes().all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(&c)) {
        return Err(format!("invalid hex payload (allowed: 0-9, A-F), got `{s}`"));
    }
    if s.len() % 2 != 0 {
        return Err(format!("hex payload must have even length (D9), got {}", s.len()));
    }
    Ok(s.to_string())
}

fn wcs_name(s: &str) -> Result<Wcs, String> {
    match s {
        "G54" => Ok(Wcs::G54),
        "G55" => Ok(Wcs::G55),
        "G56" => Ok(Wcs::G56),
        "G57" => Ok(Wcs::G57),
        "G58" => Ok(Wcs::G58),
        "G59" => Ok(Wcs::G59),
        "G59.1" => Ok(Wcs::G59_1),
        "G59.2" => Ok(Wcs::G59_2),
        "G59.3" => Ok(Wcs::G59_3),
        _ => Err(format!("invalid wcs name `{s}` (G54..G59.3)")),
    }
}

pub fn wcs_to_str(w: Wcs) -> &'static str {
    match w {
        Wcs::G54 => "G54",
        Wcs::G55 => "G55",
        Wcs::G56 => "G56",
        Wcs::G57 => "G57",
        Wcs::G58 => "G58",
        Wcs::G59 => "G59",
        Wcs::G59_1 => "G59.1",
        Wcs::G59_2 => "G59.2",
        Wcs::G59_3 => "G59.3",
    }
}

/// Номер системы p (1..9) ↔ имя WCS: G54=1 … G59=6, G59.1=7, G59.2=8, G59.3=9.
pub fn wcs_to_p(w: Wcs) -> u8 {
    match w {
        Wcs::G54 => 1,
        Wcs::G55 => 2,
        Wcs::G56 => 3,
        Wcs::G57 => 4,
        Wcs::G58 => 5,
        Wcs::G59 => 6,
        Wcs::G59_1 => 7,
        Wcs::G59_2 => 8,
        Wcs::G59_3 => 9,
    }
}

fn p_to_wcs(p: u8) -> Option<Wcs> {
    match p {
        1 => Some(Wcs::G54),
        2 => Some(Wcs::G55),
        3 => Some(Wcs::G56),
        4 => Some(Wcs::G57),
        5 => Some(Wcs::G58),
        6 => Some(Wcs::G59),
        7 => Some(Wcs::G59_1),
        8 => Some(Wcs::G59_2),
        9 => Some(Wcs::G59_3),
        _ => None,
    }
}

/// Курсор по полям OP-строки: строгий порядок полей по grammar.ebnf.
struct Cur<'a> {
    f: &'a [&'a str],
    i: usize,
}

impl<'a> Cur<'a> {
    fn take(&mut self, prefix: &str) -> Result<&'a str, String> {
        let s = self
            .f
            .get(self.i)
            .copied()
            .ok_or_else(|| format!("missing field `{prefix}` (fixed field order, grammar v0.3)"))?;
        if !s.starts_with(prefix) {
            return Err(format!(
                "expected field `{prefix}` (fixed order), got `{s}`"
            ));
        }
        self.i += 1;
        Ok(&s[prefix.len()..])
    }

    fn take_opt(&mut self, prefix: &str) -> Result<Option<&'a str>, String> {
        if let Some(s) = self.f.get(self.i) {
            if s.starts_with(prefix) {
                self.i += 1;
                return Ok(Some(&s[prefix.len()..]));
            }
        }
        Ok(None)
    }

    fn done(&self) -> Result<(), String> {
        if self.i == self.f.len() {
            Ok(())
        } else {
            Err(format!("unexpected extra field `{}`", self.f[self.i]))
        }
    }
}

fn state(c: &mut Cur) -> Result<State, String> {
    // D3: полный снапшот, фиксированный порядок feed, spindle, coolant,
    // tool, frame, wcs — каждое поле ровно один раз.
    let feed_v = c.take("feed=")?;
    let feed = if feed_v == "none" {
        None
    } else {
        Some(num(feed_v).map_err(|e| format!("feed: {e}"))?)
    };
    let spindle_v = c.take("spindle=")?;
    let spindle = if spindle_v == "off" {
        Spindle::Off
    } else {
        let (d, r) = spindle_v
            .split_once(',')
            .ok_or_else(|| format!("spindle: expected `off` or `cw|ccw,<rpm>`, got `{spindle_v}`"))?;
        let rpm = num(r).map_err(|e| format!("spindle: {e}"))?;
        match d {
            "cw" => Spindle::Cw(rpm),
            "ccw" => Spindle::Ccw(rpm),
            _ => return Err(format!("spindle: invalid direction `{d}`")),
        }
    };
    let coolant_v = c.take("coolant=")?;
    let coolant = match coolant_v {
        "none" => Coolant { flood: false, mist: false },
        "flood" => Coolant { flood: true, mist: false },
        "mist" => Coolant { flood: false, mist: true },
        "flood+mist" => Coolant { flood: true, mist: true },
        _ => return Err(format!("coolant: invalid value `{coolant_v}` (none|flood|mist|flood+mist)")),
    };
    let tool_v = c.take("tool=")?;
    let tool = if tool_v == "none" {
        None
    } else {
        Some(u32::try_from(integer(tool_v).map_err(|e| format!("tool: {e}"))?)
            .map_err(|_| "tool: out of range".to_string())?)
    };
    let frame_v = c.take("frame=")?;
    let frame = match frame_v {
        "wcs" => Frame::Wcs,
        "machine" => Frame::Machine,
        _ => return Err(format!("frame: invalid value `{frame_v}` (wcs|machine)")),
    };
    let wcs = wcs_name(c.take("wcs=")?)?;
    Ok(State { feed, spindle, coolant, tool, frame, wcs })
}

fn sub_field(c: &mut Cur) -> Result<Option<u32>, String> {
    // D8: sub= допустим только на RAPID/LINEAR/ARC/DWELL (проверяется здесь
    // структурно: у остальных опов поля sub просто нет в грамматике).
    match c.take_opt("sub=")? {
        Some(v) => Ok(Some(
            u32::try_from(integer(v).map_err(|e| format!("sub: {e}"))?)
                .map_err(|_| "sub: out of range".to_string())?,
        )),
        None => Ok(None),
    }
}

fn src_field(c: &mut Cur) -> Result<(u32, u32, u32, u32), String> {
    span(c.take("src=")?)
}

// ===========================================================================
// Разбор текста (лексика/структура, fail-fast)
// ===========================================================================

fn strip_cr(l: &str) -> &str {
    l.strip_suffix('\r').unwrap_or(l)
}

fn parse_text(src: &str, file: &str) -> Result<Program, Vec<Diagnostic>> {
    let fail = |msg: String, line_no: usize, line: &str| -> Vec<Diagnostic> {
        vec![diag(
            ErrorCode::InvalidInput,
            msg,
            Some(SrcSpan {
                file: file.to_string(),
                line_start: line_no as u32,
                col_start: 1,
                line_end: line_no as u32,
                col_end: line.len() as u32,
            }),
        )]
    };

    // NL: LF и CRLF принимаются (токенизатор); ровно один NL после END.
    if !src.ends_with('\n') {
        return Err(fail("missing final newline after END (grammar: \"END\" , NL)".into(), 0, ""));
    }
    let mut lines: Vec<&str> = src.split('\n').map(strip_cr).collect();
    lines.pop(); // хвостовая пустая строка от финального NL
    if lines.is_empty() {
        return Err(fail("empty input".into(), 1, ""));
    }
    for (i, l) in lines.iter().enumerate() {
        if l.is_empty() {
            return Err(fail("empty line (grammar: no blank lines)".into(), i + 1, l));
        }
        if l.contains('\t') {
            return Err(fail("tab character (SP is a single space)".into(), i + 1, l));
        }
    }

    // magic — принимается ровно "EID/L1 v0.3" (magic = версия формата,
    // SPEC §10.4: изменение формата = новая eid_version; v0.1 больше не валиден).
    if lines[0] != "EID/L1 v0.3" {
        return Err(fail(
            format!("invalid magic `{}`; expected `EID/L1 v0.3`", lines[0]),
            1,
            lines[0],
        ));
    }

    // UNITS
    let units = match lines[1].strip_prefix("UNITS ") {
        Some("mm") => Units::Mm,
        Some("in") => Units::In,
        _ => {
            return Err(fail(
                format!("invalid UNITS line `{}` (expected `UNITS mm|in`)", lines[1]),
                2,
                lines[1],
            ))
        }
    };

    // PATH_MODE (grammar v0.3: exact_path | exact_stop | blend [ SP "t=" number ];
    // без t= легален — bare G64; t — G64 P-, число по D1)
    let path_mode = match lines[2].strip_prefix("PATH_MODE ") {
        Some("exact_path") => PathMode::ExactPath,
        Some("exact_stop") => PathMode::ExactStop,
        Some("blend") => PathMode::Blend { t: None },
        Some(rest) => match rest.strip_prefix("blend t=") {
            Some(t) => PathMode::Blend {
                t: Some(num(t).map_err(|e| fail(format!("PATH_MODE blend: {e}"), 3, lines[2]))?),
            },
            None => {
                return Err(fail(
                    format!(
                        "invalid PATH_MODE `{}` (grammar v0.3: exact_path|exact_stop|blend [ t=<number> ])",
                        lines[2]
                    ),
                    3,
                    lines[2],
                ))
            }
        },
        None => {
            return Err(fail(
                format!(
                    "invalid PATH_MODE `{}` (grammar v0.3: exact_path|exact_stop|blend [ t=<number> ])",
                    lines[2]
                ),
                3,
                lines[2],
            ))
        }
    };

    // FRAME_PRESET (опциональная строка; отсутствие = none)
    let mut i = 3usize;
    let mut frame_preset: Option<FramePreset> = None;
    if let Some(rest) = lines[i].strip_prefix("FRAME_PRESET ") {
        if rest == "none" {
            frame_preset = None;
        } else if let Some(g92) = rest.strip_prefix("G92 ") {
            let f: Vec<&str> = g92.split(' ').collect();
            // G92 SP axis_word SP axis_word SP axis_word SP WCS SP wcs_name (D2: X, Y, Z)
            if f.len() != 5 {
                return Err(fail(
                    format!(
                        "invalid G92 block (expected `G92 X.. Y.. Z.. WCS <wcs>`), got `{rest}`"
                    ),
                    i + 1,
                    lines[i],
                ));
            }
            let axis = |w: &str, letter: char| -> Result<Num, String> {
                w.strip_prefix(letter)
                    .and_then(parse_num)
                    .ok_or_else(|| format!("invalid axis word `{w}` (expected {letter}<number>)"))
            };
            let x = axis(f[0], 'X').map_err(|e| format!("{e} (D2: order X, Y, Z)"));
            let y = axis(f[1], 'Y');
            let z = axis(f[2], 'Z');
            let (x, y, z) = match (x, y, z) {
                (Ok(x), Ok(y), Ok(z)) => (x, y, z),
                (e1, e2, e3) => {
                    let msg = [e1.err(), e2.err(), e3.err()]
                        .into_iter()
                        .flatten()
                        .next()
                        .unwrap_or_else(|| "invalid G92 block".into());
                    return Err(fail(msg, i + 1, lines[i]));
                }
            };
            if f[3] != "WCS" {
                return Err(fail(format!("expected `WCS` in G92 block, got `{}`", f[3]), i + 1, lines[i]));
            }
            let w = wcs_name(f[4])
                .map_err(|e| fail(e, i + 1, lines[i]))?;
            frame_preset = Some(FramePreset { values: Point3 { x, y, z }, wcs: w });
        } else {
            return Err(fail(
                format!("invalid FRAME_PRESET `{rest}` (expected `none` or `G92 ...`)"),
                i + 1,
                lines[i],
            ));
        }
        i += 1;
    }

    // OP-строки: один оп = одна физическая строка
    let mut ops: Vec<Op> = Vec::new();
    while i < lines.len() && lines[i].starts_with("OP ") {
        match parse_op_line(lines[i], i + 1, file) {
            Ok(op) => ops.push(op),
            Err(msg) => return Err(fail(msg, i + 1, lines[i])),
        }
        i += 1;
    }

    // END + ничего после
    if i >= lines.len() || lines[i] != "END" {
        return Err(fail(
            format!("expected `END`, got `{}`", lines.get(i).copied().unwrap_or("")),
            i + 1,
            lines.get(i).copied().unwrap_or(""),
        ));
    }
    if i + 1 != lines.len() {
        return Err(fail(
            format!("unexpected content after END: `{}`", lines[i + 1]),
            i + 2,
            lines[i + 1],
        ));
    }

    Ok(Program {
        eid_version: "EID/L1 v0.3".to_string(),
        units,
        path_mode,
        frame_preset,
        ops,
    })
}

/// Токенизация OP-строки: поля через одиночный SP; символ '"' переключает
/// режим «в кавычках» (reason может содержать пробелы; экранирования в
/// grammar v0.3 нет — `char_without_quote_or_nl`).
fn split_fields(line: &str) -> Result<Vec<&str>, String> {
    let b = line.as_bytes();
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut in_quote = false;
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'"' => {
                in_quote = !in_quote;
                i += 1;
            }
            b' ' if !in_quote => {
                out.push(&line[start..i]);
                i += 1;
                if i == b.len() {
                    return Err("trailing space".into());
                }
                if b[i] == b' ' {
                    return Err("double space (SP is a single space)".into());
                }
                start = i;
            }
            _ => i += 1,
        }
    }
    if in_quote {
        return Err("unterminated quoted string".into());
    }
    out.push(&line[start..i]);
    Ok(out)
}

fn parse_op_line(line: &str, line_no: usize, file: &str) -> Result<Op, String> {
    let rest = &line[3..]; // после "OP "
    let fields: Vec<&str> = split_fields(rest)?;
    if fields.iter().any(|f| f.is_empty()) {
        return Err("empty field (double/leading/trailing space; SP is single)".into());
    }
    if fields.len() < 2 {
        return Err("OP line needs op number and op keyword".into());
    }
    // op_num = ровно 3 цифры (zero-padded: 001…)
    let numf = fields[0];
    if numf.len() != 3 || !numf.bytes().all(|c| c.is_ascii_digit()) {
        return Err(format!("invalid op number `{numf}` (expected zero-padded 3 digits, e.g. 001)"));
    }
    let kw = fields[1];
    let mut c = Cur { f: &fields[2..], i: 0 };
    let src_of = |s: (u32, u32, u32, u32)| SrcSpan {
        file: file.to_string(),
        line_start: s.0,
        col_start: s.1,
        line_end: s.2,
        col_end: s.3,
    };

    let op = match kw {
        "RAPID" => {
            let start = start_point(c.take("start=")?)?;
            let end = vec3(c.take("end=")?)?;
            let s = src_field(&mut c)?;
            let sub = sub_field(&mut c)?;
            let st = state(&mut c)?;
            c.done()?;
            Op::Rapid { start, end, state: st, src: src_of(s), sub }
        }
        "LINEAR" => {
            let start = start_point(c.take("start=")?)?;
            let end = vec3(c.take("end=")?)?;
            let s = src_field(&mut c)?;
            let sub = sub_field(&mut c)?;
            let st = state(&mut c)?;
            c.done()?;
            Op::Linear { start, end, state: st, src: src_of(s), sub }
        }
        "ARC" => {
            let start = start_point(c.take("start=")?)?;
            let end = vec3(c.take("end=")?)?;
            let center = vec3(c.take("center=")?)?;
            let normal = vec3(c.take("normal=")?)?;
            let dir_v = c.take("dir=")?;
            let dir = match dir_v {
                "cw" => Dir::Cw,
                "ccw" => Dir::Ccw,
                _ => return Err(format!("dir: invalid value `{dir_v}` (cw|ccw)")),
            };
            let sweep_deg = num(c.take("sweep=")?)?;
            let pitch = num(c.take("pitch=")?)?;
            let s = src_field(&mut c)?;
            let sub = sub_field(&mut c)?;
            let st = state(&mut c)?;
            c.done()?;
            Op::Arc { start, end, center, normal, dir, sweep_deg, pitch, state: st, src: src_of(s), sub }
        }
        "DWELL" => {
            let seconds = num(c.take("seconds=")?)?;
            let s = src_field(&mut c)?;
            let sub = sub_field(&mut c)?;
            let st = state(&mut c)?;
            c.done()?;
            Op::Dwell { seconds, state: st, src: src_of(s), sub }
        }
        "TOOL_CHANGE" => {
            let tool = u32::try_from(integer(c.take("tool=")?)?)
                .map_err(|_| "tool: out of range".to_string())?;
            let s = src_field(&mut c)?;
            let st = state(&mut c)?;
            c.done()?;
            Op::ToolChange { tool, state: st, src: src_of(s) }
        }
        "STOP" => {
            let optional_v = c.take("optional=")?;
            let optional = match optional_v {
                "yes" => true,
                "no" => false,
                _ => return Err(format!("optional: invalid value `{optional_v}` (yes|no)")),
            };
            let s = src_field(&mut c)?;
            let st = state(&mut c)?;
            c.done()?;
            Op::ProgramStop { optional, state: st, src: src_of(s) }
        }
        "PEND" => {
            c.done()?; // state нет — D6
            // src в тексте не представлен; плейсхолдер 0.1-0.0 (D10)
            Op::ProgramEnd {
                src: SrcSpan { file: file.to_string(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 },
            }
        }
        "WCS_DEFINE" => {
            let p = p_field(&mut c)?;
            let origin = vec3(c.take("origin=")?)?;
            let s = src_field(&mut c)?;
            let st = state(&mut c)?;
            c.done()?;
            Op::WcsDefine { p, origin, state: st, src: src_of(s) }
        }
        "WCS_SET_HERE" => {
            let p = p_field(&mut c)?;
            let delta = vec3(c.take("delta=")?)?;
            let s = src_field(&mut c)?;
            let st = state(&mut c)?;
            c.done()?;
            Op::WcsSetHere { p, delta, state: st, src: src_of(s) }
        }
        "RAW" => {
            // D6: state нет
            let reason = quoted(c.take("reason=")?)?;
            let code_v = c.take("code=")?;
            let code = diag_code(code_v)?;
            let s = src_field(&mut c)?;
            let payload = hex_payload(c.take("payload=")?)?;
            c.done()?;
            Op::Raw { payload_hex: payload, reason, code, src: src_of(s) }
        }
        _ => return Err(format!("unknown op keyword `{kw}` (10 kinds, SPEC §5)")),
    };
    let _ = line_no; // span несёт src= из текста, позиция строки не нужна
    Ok(op)
}

fn p_field(c: &mut Cur) -> Result<u8, String> {
    let v = c.take("p=")?;
    // p = одиночная цифра; домен 1..9 — D7 (валидируется ниже, лексика — здесь)
    if v.len() != 1 || !v.as_bytes()[0].is_ascii_digit() {
        return Err(format!("p: expected single digit, got `{v}`"));
    }
    Ok(v.as_bytes()[0] - b'0')
}

fn diag_code(s: &str) -> Result<ErrorCode, String> {
    match s {
        "E_INVALID_INPUT" => Ok(ErrorCode::InvalidInput),
        "E_UNSUPPORTED_SYNTAX" => Ok(ErrorCode::UnsupportedSyntax),
        "E_UNSUPPORTED_SEMANTICS" => Ok(ErrorCode::UnsupportedSemantics),
        "E_REQUIRES_MACHINE_TABLE" => Ok(ErrorCode::RequiresMachineTable),
        "E_RUNTIME_STATE" => Ok(ErrorCode::RuntimeState),
        "E_LIMIT_EXCEEDED" => Ok(ErrorCode::LimitExceeded),
        "E_RAW_UNTRANSLATABLE" => Ok(ErrorCode::RawUntranslatable),
        _ => Err(format!("invalid diag code `{s}` (7 codes, SPEC §8)")),
    }
}

fn start_point(s: &str) -> Result<StartPoint, String> {
    if s == "unknown" {
        Ok(StartPoint::Unknown) // SPEC §2, Q11
    } else {
        Ok(StartPoint::Known(vec3(s)?))
    }
}

// ===========================================================================
// Валидация (SPEC §3 + D4–D8 + §6) — собирает все ошибки
// ===========================================================================

fn is_motion(op: &Op) -> bool {
    matches!(op, Op::Rapid { .. } | Op::Linear { .. } | Op::Arc { .. })
}

fn nval(n: &Num) -> f64 {
    n.value
}

/// Точное равенство значений (f64-зеркало; verbatim-текст гарантируется
/// эмиттером — SPEC §10.1). -0.0 == 0.0.
fn p3_eq(a: &Point3, b: &Point3) -> bool {
    nval(&a.x) == nval(&b.x) && nval(&a.y) == nval(&b.y) && nval(&a.z) == nval(&b.z)
}

fn motion_parts(op: &Op) -> Option<(&StartPoint, &Point3, &State)> {
    match op {
        Op::Rapid { start, end, state, .. }
        | Op::Linear { start, end, state, .. }
        | Op::Arc { start, end, state, .. } => Some((start, end, state)),
        _ => None,
    }
}

fn run(program: &Program, drop_zero_length: bool) -> Result<(Vec<Op>, Vec<Notice>), Vec<Diagnostic>> {
    let mut errors: Vec<Diagnostic> = Vec::new();
    let ops = &program.ops;

    // --- §3: program_end ровно один раз, последний; в минимальной программе —
    //     единственный; опов после него нет. --------------------------------
    if ops.is_empty() {
        errors.push(diag(
            ErrorCode::InvalidInput,
            "program has no ops: program_end (PEND) required exactly once as the last op (SPEC §3)",
            None,
        ));
    }
    let end_count = ops.iter().filter(|o| matches!(o, Op::ProgramEnd { .. })).count();
    if end_count == 0 && !ops.is_empty() {
        errors.push(diag(
            ErrorCode::InvalidInput,
            "missing program_end (PEND) — required exactly once, last (SPEC §3)",
            None,
        ));
    }
    if end_count > 1 {
        for o in ops.iter().filter(|o| matches!(o, Op::ProgramEnd { .. })) {
            errors.push(diag(
                ErrorCode::InvalidInput,
                "duplicate program_end (SPEC §3: ровно один PEND)",
                op_src(o),
            ));
        }
    }
    if let Some(last) = ops.last() {
        if !matches!(last, Op::ProgramEnd { .. }) {
            errors.push(diag(
                ErrorCode::InvalidInput,
                "program_end must be the last op (SPEC §3)",
                op_src(last),
            ));
        }
    }

    // --- D5: FRAME_PRESET/WCS_SET_HERE — только до первого motion-опа.
    //     FRAME_PRESET — поле заголовка (позиция структурная); WCS_SET_HERE —
    //     оп, проверяем по текстовому порядку. -------------------------------
    let first_motion_raw = ops.iter().position(is_motion);
    for (j, o) in ops.iter().enumerate() {
        if let Op::WcsSetHere { .. } = o {
            if let Some(fm) = first_motion_raw {
                if j > fm {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        "WCS_SET_HERE after first motion op (D5, SPEC §3/§5.3: only a preset before the first motion)",
                        op_src(o),
                    ));
                }
            }
        }
    }

    // --- D4 / D7 (домены, не зависящие от отбрасывания) ---------------------
    for o in ops.iter() {
        match o {
            Op::ToolChange { tool, state, .. } => {
                // D4: значение op-поля tool= равно state-полю tool=
                if state.tool != Some(*tool) {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!("TOOL_CHANGE tool={tool} != state tool={} (D4)", state.tool.map(|t| t.to_string()).unwrap_or_else(|| "none".into())),
                        op_src(o),
                    ));
                }
            }
            Op::WcsDefine { p, .. } | Op::WcsSetHere { p, .. } => {
                if *p < 1 || *p > 9 {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!("p={} out of domain 1..9 (D7: G54..G59.3)", p),
                        op_src(o),
                    ));
                }
            }
            Op::Dwell { seconds, .. } => {
                if nval(seconds) < 0.0 {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!("dwell seconds={} < 0 (D7, SPEC §5.2)", seconds.text),
                        op_src(o),
                    ));
                }
            }
            _ => {}
        }
    }

    // --- Отбрасывание zero-length (Parse: drop+notice; Emit: ошибка) --------
    let mut kept: Vec<Op> = Vec::new();
    let mut notices: Vec<Notice> = Vec::new();
    for (i, o) in ops.iter().enumerate() {
        let zero: Option<(NoticeKind, String)> = match o {
            Op::Rapid { start, end, .. } | Op::Linear { start, end, .. } => {
                if let StartPoint::Known(s) = start {
                    if p3_eq(s, end) {
                        Some((NoticeKind::ZeroLengthMotion, "zero-length motion dropped (start == end, SPEC §3)".into()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Op::Arc { start, end, sweep_deg, pitch, .. } => {
                // start == end при sweep != 360° и pitch == 0 — нулевой ход,
                // не дуга (SPEC §3/§6; sweep=0 сюда же — CORPUS #13).
                if let StartPoint::Known(s) = start {
                    if p3_eq(s, end) && nval(sweep_deg) != 360.0 && nval(pitch) == 0.0 {
                        Some((NoticeKind::ZeroLengthArc, "zero-length arc dropped (start == end, sweep != 360°, pitch == 0; SPEC §3/§6)".into()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Op::Dwell { seconds, .. } => {
                if nval(seconds) == 0.0 {
                    Some((NoticeKind::ZeroDwell, "zero dwell dropped (seconds == 0, SPEC §5.2)".into()))
                } else {
                    None
                }
            }
            _ => None,
        };
        match zero {
            Some((kind, message)) => {
                if drop_zero_length {
                    notices.push(Notice { kind, message, op_index: i, op: o.clone() });
                } else {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!(
                            "{} must be dropped by canonicalization before emission (SPEC §3)",
                            message
                        ),
                        op_src(o),
                    ));
                }
            }
            None => kept.push(o.clone()),
        }
    }

    // --- Проверки по выжившим опам ------------------------------------------
    // D7-домены дуги; ARC с unknown-стартом; feed для linear/arc (SPEC §3:
    // требование относится к опам в итоговом AST, т.е. после отбрасывания).
    for o in kept.iter() {
        match o {
            Op::Arc { start, sweep_deg, normal, state, .. } => {
                if matches!(start, StartPoint::Unknown) {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        "arc with start=unknown is invalid (SPEC §2 Q11: arc requires a numeric start)",
                        op_src(o),
                    ));
                }
                let s = nval(sweep_deg);
                if s <= 0.0 || s > 360.0 {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!("sweep={} out of domain (0°, 360°] (D7, SPEC §6)", sweep_deg.text),
                        op_src(o),
                    ));
                }
                let len = vlen(&vec_vals(normal));
                if (len - 1.0).abs() > 0.001 {
                    // 0.001 — допуск 4dp-квантования компонент единичного вектора (D7)
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        format!("|normal| = {len:.6} != 1 (D7)"),
                        op_src(o),
                    ));
                }
                if state.feed.is_none() {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        "arc with feed=none (SPEC §3: linear/arc in the final AST require modal feed)",
                        op_src(o),
                    ));
                }
            }
            Op::Linear { state, .. } => {
                if state.feed.is_none() {
                    errors.push(diag(
                        ErrorCode::InvalidInput,
                        "linear with feed=none (SPEC §3: linear/arc in the final AST require modal feed)",
                        op_src(o),
                    ));
                }
            }
            _ => {}
        }
    }

    // --- §6: геометрическая валидация выживших дуг --------------------------
    let eps_pos = eps_for(&kept);
    for o in kept.iter() {
        if let Op::Arc { start: StartPoint::Known(s), end, center, normal, dir, sweep_deg, pitch, .. } = o {
            check_arc_geometry(o, s, end, center, normal, *dir, nval(sweep_deg), nval(pitch), eps_pos, &mut errors);
        }
    }

    // --- Связывание start (SPEC §2 принцип 2, Q11; v0.9 обязанность) --------
    check_linking(program, &kept, &mut errors);

    if errors.is_empty() {
        Ok((kept, notices))
    } else {
        Err(errors)
    }
}

// ===========================================================================
// §6 — геометрия дуги (допуски — EQUIVALENCE §4)
// ===========================================================================

type V3 = (f64, f64, f64);

fn vec_vals(p: &Point3) -> V3 {
    (nval(&p.x), nval(&p.y), nval(&p.z))
}

fn p3_vals(p: &Point3) -> V3 {
    (nval(&p.x), nval(&p.y), nval(&p.z))
}

fn vlen(v: &V3) -> f64 {
    v.0.hypot(v.1).hypot(v.2)
}

fn vsub(a: &V3, b: &V3) -> V3 {
    (a.0 - b.0, a.1 - b.1, a.2 - b.2)
}

fn vadd(a: &V3, b: &V3) -> V3 {
    (a.0 + b.0, a.1 + b.1, a.2 + b.2)
}

fn vscale(a: &V3, k: f64) -> V3 {
    (a.0 * k, a.1 * k, a.2 * k)
}

fn vdot(a: &V3, b: &V3) -> f64 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

fn vcross(a: &V3, b: &V3) -> V3 {
    (a.1 * b.2 - a.2 * b.1, a.2 * b.0 - a.0 * b.2, a.0 * b.1 - a.1 * b.0)
}

fn vdist(a: &V3, b: &V3) -> f64 {
    vlen(&vsub(a, b))
}

/// Масштаб L (диагональ bbox всех endpoint'ов motion-опов, EQUIVALENCE §4) →
/// ε_pos = max(0.001, 1e-7·L).
fn eps_for(ops: &[Op]) -> f64 {
    let mut min = (f64::MAX, f64::MAX, f64::MAX);
    let mut max = (f64::MIN, f64::MIN, f64::MIN);
    let mut any = false;
    let mut take = |v: V3| {
        any = true;
        min.0 = min.0.min(v.0); min.1 = min.1.min(v.1); min.2 = min.2.min(v.2);
        max.0 = max.0.max(v.0); max.1 = max.1.max(v.1); max.2 = max.2.max(v.2);
    };
    for o in ops {
        if let Some((start, end, _)) = motion_parts(o) {
            if let StartPoint::Known(k) = start {
                take(p3_vals(k));
            }
            take(p3_vals(end));
        }
    }
    let l = if any {
        vlen(&(max.0 - min.0, max.1 - min.1, max.2 - min.2))
    } else {
        0.0
    };
    (0.001_f64).max(1e-7 * l)
}

/// Rodrigues: поворот v вокруг единичной оси n на угол θ (ccw — положительный).
fn rotate(v: &V3, n: &V3, theta: f64) -> V3 {
    let (s, c) = theta.sin_cos();
    let nxv = vcross(n, v);
    let nv = vdot(n, v);
    vadd(
        &vadd(
            &vscale(v, c),
            &vscale(&nxv, s),
        ),
        &vscale(n, nv * (1.0 - c)),
    )
}

#[allow(clippy::too_many_arguments)]
fn check_arc_geometry(
    op: &Op,
    start: &Point3,
    end: &Point3,
    center: &Point3,
    normal: &Point3,
    dir: Dir,
    sweep: f64,
    pitch: f64,
    eps_pos: f64,
    errors: &mut Vec<Diagnostic>,
) {
    let src = op_src(op);
    let s = p3_vals(start);
    let e = p3_vals(end);
    let c = p3_vals(center);
    let n_raw = vec_vals(normal);
    let nlen = vlen(&n_raw);
    let n = vscale(&n_raw, 1.0 / nlen); // |n|≈1 проверен доменом (D7)

    // 1. Планиметрический радиус: ρ_start ≈ ρ_end (проекция ⟂ normal).
    let planar = |d: &V3| -> V3 { vsub(d, &vscale(&n, vdot(&n, d))) };
    let rho_s = vlen(&planar(&vsub(&s, &c)));
    let rho_e = vlen(&planar(&vsub(&e, &c)));
    let tol_r = eps_pos.max(1e-6 * rho_s);
    if (rho_s - rho_e).abs() > tol_r {
        errors.push(diag(
            ErrorCode::InvalidInput,
            format!(
                "arc radius mismatch: |start| = {rho_s:.6}, |end| = {rho_e:.6}, tol = {tol_r:.6} (SPEC §6.1)"
            ),
            src.clone(),
        ));
    }

    // 2. Нормальная компонента хорды ≈ pitch.
    let chord_n = vdot(&vsub(&e, &s), &n);
    if (chord_n - pitch).abs() > eps_pos {
        errors.push(diag(
            ErrorCode::InvalidInput,
            format!(
                "arc chord·normal = {chord_n:.6} != pitch = {pitch:.6} (tol {eps_pos:.6}, SPEC §6.2)"
            ),
            src.clone(),
        ));
    }

    // 3. Полное уравнение позиции: end ≈ center + Rot(n, dir, sweep)·(start−center) + pitch·n.
    let theta = sweep.to_radians() * if dir == Dir::Cw { -1.0 } else { 1.0 };
    let rotated = rotate(&vsub(&s, &c), &n, theta);
    let expected = vadd(&vadd(&c, &rotated), &vscale(&n, pitch));
    let d = vdist(&expected, &e);
    if d > eps_pos {
        errors.push(diag(
            ErrorCode::InvalidInput,
            format!(
                "arc endpoint inconsistent with (center, normal, dir, sweep, pitch): |Δ| = {d:.6} > {eps_pos:.6} (SPEC §6.3)"
            ),
            src,
        ));
    }
}

// ===========================================================================
// Связывание start — SPEC §2 принцип 2 / Q11, v0.9 (точки разрыва (a)–(d))
// ===========================================================================

/// Ожидаемый start: точное значение (verbatim-цепочка), значение с допуском
/// (вычислено через origin-арифметику) или обязательный unknown.
#[derive(Debug, Clone)]
enum StartExpect {
    Exact(Point3),
    Tol(Point3),
    UnknownRequired,
}

struct PrevMotion {
    end: Point3,
    frame: Frame,
    wcs: Wcs,
}

fn p3_build(v: V3) -> Point3 {
    let mk = |x: f64| Num::new(crate::emitter::fmt_num(x), x);
    Point3 { x: mk(v.0), y: mk(v.1), z: mk(v.2) }
}

#[allow(clippy::too_many_arguments)]
fn expect_start(
    is_first: bool,
    frame: Frame,
    wcs: Wcs,
    preset: Option<&FramePreset>,
    set_here: &[Option<Point3>; 10],
    known_origin: &[Option<Point3>; 10],
    prev: Option<&PrevMotion>,
) -> StartExpect {
    let op_o = known_origin[wcs_to_p(wcs) as usize].clone();
    if !is_first {
        let prev = prev.expect("prev exists when not first");
        let prev_o = known_origin[wcs_to_p(prev.wcs) as usize].clone();
        return match (prev.frame, frame) {
            (Frame::Machine, Frame::Machine) => StartExpect::Exact(prev.end.clone()),
            (Frame::Wcs, Frame::Wcs) if prev.wcs == wcs => StartExpect::Exact(prev.end.clone()),
            (Frame::Wcs, Frame::Wcs) => match (&prev_o, &op_o) {
                // start = prev_end + O_prev − O_new (Δ константен, EQUIVALENCE §1.3)
                (Some(o_prev), Some(o_new)) => StartExpect::Tol(p3_build(vadd(
                    &p3_vals(&prev.end),
                    &vsub(&p3_vals(o_prev), &p3_vals(o_new)),
                ))),
                _ => StartExpect::UnknownRequired, // (b): разность origin'ов не вычислима
            },
            (Frame::Wcs, Frame::Machine) => {
                // M = reading + O_wcs + S; S = 0 только без frame_preset (SPEC §5.3)
                if preset.is_none() {
                    if let Some(op_) = &prev_o {
                        StartExpect::Tol(p3_build(vadd(&p3_vals(&prev.end), &p3_vals(op_))))
                    } else {
                        StartExpect::UnknownRequired
                    }
                } else {
                    StartExpect::UnknownRequired // (c): M₀ внешний
                }
            }
            (Frame::Machine, Frame::Wcs) => {
                // reading = M − O_wcs − S
                if preset.is_none() {
                    if let Some(on) = &op_o {
                        StartExpect::Tol(p3_build(vsub(&p3_vals(&prev.end), &p3_vals(on))))
                    } else {
                        StartExpect::UnknownRequired // (d)
                    }
                } else {
                    StartExpect::UnknownRequired // (d): Δ содержит M₀
                }
            }
        };
    }

    // Первый (выживший) motion-оп программы.
    if frame == Frame::Machine {
        // (c): стартовая позиция станка M₀ внешняя
        return StartExpect::UnknownRequired;
    }
    let sh = set_here[wcs_to_p(wcs) as usize].clone();
    match (sh, preset) {
        // G92 задаёт Δ = M₀ − O₀ − preset ≠ 0 → L20-чтение = delta − Δ содержит M₀
        (Some(_), Some(_)) => StartExpect::UnknownRequired,
        // (a)-якорь L20 (без пресетки): текущая позиция читается как delta
        (Some(delta), None) => StartExpect::Exact(delta),
        (None, Some(pr)) => {
            if pr.wcs == wcs {
                // (a)-якорь G92 в той же системе: reading = preset (golden 36)
                StartExpect::Exact(pr.values.clone())
            } else if known_origin[wcs_to_p(pr.wcs) as usize].is_some() && op_o.is_some() {
                // reading = preset + O₀ − O_wcs (обе системы определены в тексте)
                let o0 = known_origin[wcs_to_p(pr.wcs) as usize].clone().unwrap();
                StartExpect::Tol(p3_build(vadd(&p3_vals(&pr.values), &vsub(&p3_vals(&o0), &p3_vals(&op_o.unwrap())))))
            } else {
                // (b): смена на систему, разность origin'ов которой не вычислима
                StartExpect::UnknownRequired
            }
        }
        (None, None) => StartExpect::UnknownRequired, // (a): без якоря
    }
}

fn check_linking(program: &Program, kept: &[Op], errors: &mut Vec<Diagnostic>) {
    let mut known_origin: [Option<Point3>; 10] = Default::default();
    let mut set_here: [Option<Point3>; 10] = Default::default();
    let mut prev: Option<PrevMotion> = None;
    let mut saw_motion = false;

    for o in kept.iter() {
        match o {
            Op::WcsDefine { p, origin, .. } => {
                if (*p as usize) < known_origin.len() {
                    known_origin[*p as usize] = Some(origin.clone());
                }
            }
            Op::WcsSetHere { p, delta, .. } => {
                if (*p as usize) < set_here.len() {
                    set_here[*p as usize] = Some(delta.clone());
                }
            }
            _ if is_motion(o) => {
                let (start, end, state) = motion_parts(o).expect("motion op");
                let expect = expect_start(
                    !saw_motion,
                    state.frame,
                    state.wcs,
                    program.frame_preset.as_ref(),
                    &set_here,
                    &known_origin,
                    prev.as_ref(),
                );
                match (start, expect) {
                    (StartPoint::Unknown, StartExpect::UnknownRequired) => {}
                    (StartPoint::Unknown, StartExpect::Exact(_) | StartExpect::Tol(_)) => {
                        errors.push(diag(
                            ErrorCode::InvalidInput,
                            "start=unknown outside frame-break points (SPEC §2 v0.9: канонизатор обязан вычислить start = end предыдущего motion-опа)",
                            op_src(o),
                        ));
                    }
                    (StartPoint::Known(_v), StartExpect::UnknownRequired) => {
                        errors.push(diag(
                            ErrorCode::InvalidInput,
                            "start must be unknown here: позиция не вычислима из текста (точка разрыва фрейма, SPEC §2 Q11)",
                            op_src(o),
                        ));
                    }
                    (StartPoint::Known(v), StartExpect::Exact(exp)) => {
                        if !p3_eq(v, &exp) {
                            errors.push(diag(
                                ErrorCode::InvalidInput,
                                format!(
                                    "start {} != expected {} (точное равенство: start = end предыдущего motion-опа / якорь, SPEC §2 v0.9)",
                                    fmt_point(v), fmt_point(&exp)
                                ),
                                op_src(o),
                            ));
                        }
                    }
                    (StartPoint::Known(v), StartExpect::Tol(exp)) => {
                        let d = vdist(&p3_vals(v), &p3_vals(&exp));
                        if d > 0.001 {
                            // ε_abs из EQUIVALENCE §4 (floor 4dp-эмиссии с запасом ×10)
                            errors.push(diag(
                                ErrorCode::InvalidInput,
                                format!(
                                    "start {} != frame-adjusted previous end {} (|Δ| = {d:.6} > 0.001, SPEC §2 v0.9)",
                                    fmt_point(v), fmt_point(&exp)
                                ),
                                op_src(o),
                            ));
                        }
                    }
                }
                saw_motion = true;
                prev = Some(PrevMotion { end: end.clone(), frame: state.frame, wcs: state.wcs });
            }
            _ => {}
        }
    }
}

fn fmt_point(p: &Point3) -> String {
    format!("({} {} {})", p.x.text, p.y.text, p.z.text)
}
