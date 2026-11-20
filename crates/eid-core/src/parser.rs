//! EID/L1 — парсер канонического текстового формата (grammar.ebnf v0.3,
//! правила D1–D10). SPEC-EID-L1 v0.15.
//!
//! Итерация 4 (MLIR-паттерн): валидация канонического AST вынесена в
//! отдельный pass [`crate::verify`] — единственная копия правил; parser.rs
//! и emitter.rs только делегируют. Поток разбора: текст → [`parse_text`]
//! (лексика/структура; нарушение — E_INVALID_INPUT, Stage::Parse) →
//! `verify::validate_with_notices` (отбрасывание zero-length с notice +
//! валидация §3/§6, Stage::Verify) → [`Program`].
//!
//! Пути `eid_core::parser::{Notice, NoticeKind, wcs_to_p, validate}` сохранены
//! (re-export/делегат в verify) — консьюмеры eid-grbl/eid-linuxcnc не затронуты.

use crate::ast::*;
use crate::diag::{Diagnostic, ErrorCode, Stage};

// Типы/утилиты валидации живут в verify; пути parser::* сохранены для
// консьюмеров (interface v0.1.2, итерация 4).
pub use crate::verify::{Notice, NoticeKind, wcs_to_p};

/// Результат разбора: программа (zero-length отброшены) + notices.
#[derive(Debug, Clone)]
pub struct ParseOutcome {
    pub program: Program,
    pub notices: Vec<Notice>,
}

// ===========================================================================
// Публичные входные точки (делегируют лексику → parse_text, валидацию → verify)
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
/// SPEC §3/§6 по выжившим опам. Делегат [`crate::verify::validate_with_notices`].
pub fn canonicalize(program: Program) -> Result<ParseOutcome, Vec<Diagnostic>> {
    let report = crate::verify::validate_with_notices(&program);
    if report.diagnostics.is_empty() {
        Ok(ParseOutcome {
            program: Program { ops: report.kept_ops, ..program },
            notices: report.notices,
        })
    } else {
        Err(report.diagnostics)
    }
}

/// Только валидация (без отбрасывания) — путь эмиссии: zero-length опы в
/// каноническом AST запрещены (их обязана отбросить канонизация, SPEC §3).
/// Делегат [`crate::verify::validate`] (Stage::Verify).
pub fn validate(program: &Program) -> Result<(), Vec<Diagnostic>> {
    let ds = crate::verify::validate(program);
    if ds.is_empty() {
        Ok(())
    } else {
        Err(ds)
    }
}

// ===========================================================================
// Диагностика разбора (Stage::Parse)
// ===========================================================================

fn diag(code: ErrorCode, msg: impl Into<String>, src: Option<SrcSpan>) -> Diagnostic {
    Diagnostic {
        code,
        stage: Stage::Parse,
        message: msg.into(),
        src,
        options: Vec::new(),
        limit: None,
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
