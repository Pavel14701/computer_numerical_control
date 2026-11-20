//! Детерминированный эмиттер канонического текста EID/L1 (grammar.ebnf v0.3):
//!   * один оп = одна физическая строка; строки — LF;
//!   * все поля state в фиксированном порядке feed, spindle, coolant, tool,
//!     frame, wcs (D3); оси X, Y, Z (D2);
//!   * числа — ровно 4 dp, без экспоненты, трейлинг-нули сохранены (D1);
//!     "-0.0000" → "0.0000" (fmt_num);
//!   * верbatim-предпочтение: если Num.text уже каноничен (4 dp), он
//!     воспроизводится байт-в-байт (SPEC §10.1 — точный same-dialect
//!     round-trip); иначе число канонизируется из f64-зеркала;
//!   * emit(emit(x)) по AST — байт-в-байт (двойной emit детерминирован);
//!   * перед эмиссией программа валидируется (SPEC §3): zero-length опы и
//!     прочие нарушения канонической формы — ошибка, не молчаливый пропуск.

use crate::ast::*;
use crate::diag::{Diagnostic, ErrorCode};
use crate::parser;

/// Канонический формат числа: ровно 4 dp, без экспоненты, "-0.0000" → "0.0000".
pub fn fmt_num(v: f64) -> String {
    if v == 0.0 {
        return "0.0000".to_string(); // покрывает и -0.0
    }
    let s = format!("{v:.4}");
    if s == "-0.0000" {
        "0.0000".to_string()
    } else {
        s
    }
}

fn err(code: ErrorCode, msg: impl Into<String>) -> Diagnostic {
    Diagnostic { code, message: msg.into(), src: None, options: Vec::new(), limit: None }
}

/// Число на эмиссию: verbatim-текст, если он уже каноничен (4 dp, не
/// "-0.0000"), иначе канонизация из f64-зеркала.
fn emit_num(n: &Num, errors: &mut Vec<Diagnostic>, what: &str) -> String {
    if !n.value.is_finite() {
        errors.push(err(
            ErrorCode::InvalidInput,
            format!("{what}: non-finite value {}", n.value),
        ));
        return "0.0000".to_string();
    }
    if parser::parse_num(&n.text).is_some() && n.text != "-0.0000" {
        n.text.clone()
    } else {
        let s = fmt_num(n.value);
        if s == "NaN" || s == "inf" {
            errors.push(err(ErrorCode::InvalidInput, format!("{what}: non-finite value")));
            return "0.0000".to_string();
        }
        s
    }
}

fn emit_vec(p: &Point3, errors: &mut Vec<Diagnostic>, what: &str) -> String {
    format!(
        "{},{},{}",
        emit_num(&p.x, errors, what),
        emit_num(&p.y, errors, what),
        emit_num(&p.z, errors, what)
    )
}

fn emit_state(st: &State, errors: &mut Vec<Diagnostic>) -> String {
    // D3: фиксированный порядок feed, spindle, coolant, tool, frame, wcs.
    let feed = match &st.feed {
        None => "none".to_string(),
        Some(n) => emit_num(n, errors, "feed"),
    };
    let spindle = match &st.spindle {
        Spindle::Off => "off".to_string(),
        Spindle::Cw(n) => format!("cw,{}", emit_num(n, errors, "spindle rpm")),
        Spindle::Ccw(n) => format!("ccw,{}", emit_num(n, errors, "spindle rpm")),
    };
    let coolant = match (st.coolant.flood, st.coolant.mist) {
        (false, false) => "none",
        (true, false) => "flood",
        (false, true) => "mist",
        (true, true) => "flood+mist",
    };
    let tool = st.tool.map(|t| t.to_string()).unwrap_or_else(|| "none".into());
    let frame = match st.frame {
        Frame::Wcs => "wcs",
        Frame::Machine => "machine",
    };
    format!("feed={feed} spindle={spindle} coolant={coolant} tool={tool} frame={frame} wcs={}", parser::wcs_to_str(st.wcs))
}

fn emit_src(src: &SrcSpan) -> String {
    format!("{}.{}-{}.{}", src.line_start, src.col_start, src.line_end, src.col_end)
}

fn emit_sub(sub: &Option<u32>) -> String {
    match sub {
        Some(i) => format!(" sub={i}"),
        None => String::new(),
    }
}

/// EID Program → канонический текст. Отказы: нарушение канонической формы
/// (SPEC §3), непредставимость (RAW reason с кавычкой — без экранирования),
/// лимит нумерации опов (3 цифры).
pub fn emit(program: &Program) -> Result<String, Vec<Diagnostic>> {
    let mut errors: Vec<Diagnostic> = Vec::new();

    // Каноническая форма обязана быть валидной до эмиссии (R1: parse(emit(x))
    // обязан вернуть тот же AST — эмиссия невалидного AST дала бы текст,
    // который сам парсер отверг бы).
    parser::validate(program)?;

    if program.eid_version != "EID/L1 v0.3" {
        errors.push(err(
            ErrorCode::InvalidInput,
            format!(
                "eid_version `{}` не поддерживается эмиттером L1 v0.3 (ожидается `EID/L1 v0.3`)",
                program.eid_version
            ),
        ));
    }

    let mut out = String::new();
    out.push_str(&program.eid_version);
    out.push('\n');

    out.push_str(match program.units {
        Units::Mm => "UNITS mm\n",
        Units::In => "UNITS in\n",
    });

    match &program.path_mode {
        PathMode::ExactPath => out.push_str("PATH_MODE exact_path\n"),
        PathMode::ExactStop => out.push_str("PATH_MODE exact_stop\n"),
        // grammar v0.3: blend [ SP "t=" number ] — bare G64 легален (A1)
        PathMode::Blend { t: None } => out.push_str("PATH_MODE blend\n"),
        PathMode::Blend { t: Some(n) } => {
            out.push_str(&format!("PATH_MODE blend t={}\n", emit_num(n, &mut errors, "path_mode.t")))
        }
    }

    match &program.frame_preset {
        None => out.push_str("FRAME_PRESET none\n"),
        Some(pr) => {
            out.push_str(&format!(
                "FRAME_PRESET G92 X{} Y{} Z{} WCS {}\n",
                emit_num(&pr.values.x, &mut errors, "frame_preset.x"),
                emit_num(&pr.values.y, &mut errors, "frame_preset.y"),
                emit_num(&pr.values.z, &mut errors, "frame_preset.z"),
                parser::wcs_to_str(pr.wcs)
            ));
        }
    }

    for (i, op) in program.ops.iter().enumerate() {
        let n = i + 1;
        if n > 999 {
            // op_num = ровно 3 цифры (grammar); охрана эмиссии — E_LIMIT_EXCEEDED.
            errors.push(Diagnostic {
                code: ErrorCode::LimitExceeded,
                message: format!("op number {n} exceeds 3-digit numbering (grammar v0.3: 001..999)"),
                src: None,
                options: Vec::new(),
                limit: Some(crate::diag::LimitInfo {
                    threshold: "op_number_max",
                    threshold_value: 999.0,
                    actual: n as f64,
                }),
            });
            break;
        }
        out.push_str(&format!("OP {n:03} "));
        emit_op(op, &mut out, &mut errors);
        out.push('\n');
    }

    out.push_str("END\n");

    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

fn emit_op(op: &Op, out: &mut String, errors: &mut Vec<Diagnostic>) {
    match op {
        Op::Rapid { start, end, state, src, sub } => {
            out.push_str("RAPID start=");
            emit_start(start, out, errors);
            out.push_str(&format!(
                " end={} src={}{} ",
                emit_vec(end, errors, "end"),
                emit_src(src),
                emit_sub(sub)
            ));
            out.push_str(&emit_state(state, errors));
        }
        Op::Linear { start, end, state, src, sub } => {
            out.push_str("LINEAR start=");
            emit_start(start, out, errors);
            out.push_str(&format!(
                " end={} src={}{} ",
                emit_vec(end, errors, "end"),
                emit_src(src),
                emit_sub(sub)
            ));
            out.push_str(&emit_state(state, errors));
        }
        Op::Arc { start, end, center, normal, dir, sweep_deg, pitch, state, src, sub } => {
            out.push_str("ARC start=");
            emit_start(start, out, errors);
            out.push_str(&format!(
                " end={} center={} normal={} dir={} sweep={} pitch={} src={}{} ",
                emit_vec(end, errors, "arc.end"),
                emit_vec(center, errors, "arc.center"),
                emit_vec(normal, errors, "arc.normal"),
                match dir { Dir::Cw => "cw", Dir::Ccw => "ccw" },
                emit_num(sweep_deg, errors, "arc.sweep"),
                emit_num(pitch, errors, "arc.pitch"),
                emit_src(src),
                emit_sub(sub)
            ));
            out.push_str(&emit_state(state, errors));
        }
        Op::Dwell { seconds, state, src, sub } => {
            out.push_str(&format!(
                "DWELL seconds={} src={}{} ",
                emit_num(seconds, errors, "dwell.seconds"),
                emit_src(src),
                emit_sub(sub)
            ));
            out.push_str(&emit_state(state, errors));
        }
        Op::ToolChange { tool, state, src } => {
            // D4: op-поле tool= == state-полю tool= (валидатор проверил).
            out.push_str(&format!("TOOL_CHANGE tool={tool} src={} ", emit_src(src)));
            out.push_str(&emit_state(state, errors));
        }
        Op::ProgramStop { optional, state, src } => {
            out.push_str(&format!(
                "STOP optional={} src={} ",
                if *optional { "yes" } else { "no" },
                emit_src(src)
            ));
            out.push_str(&emit_state(state, errors));
        }
        Op::ProgramEnd { .. } => {
            out.push_str("PEND"); // state нет — D6
        }
        Op::WcsDefine { p, origin, state, src } => {
            out.push_str(&format!(
                "WCS_DEFINE p={p} origin={} src={} ",
                emit_vec(origin, errors, "wcs_define.origin"),
                emit_src(src)
            ));
            out.push_str(&emit_state(state, errors));
        }
        Op::WcsSetHere { p, delta, state, src } => {
            out.push_str(&format!(
                "WCS_SET_HERE p={p} delta={} src={} ",
                emit_vec(delta, errors, "wcs_set_here.delta"),
                emit_src(src)
            ));
            out.push_str(&emit_state(state, errors));
        }
        Op::Raw { payload_hex, reason, code, src } => {
            // D6: state нет. D9: payload hex, lossless, чётная длина.
            if !reason.contains('"') && !reason.contains('\n') && !reason.contains('\r') {
                out.push_str(&format!(
                    "RAW reason=\"{reason}\" code={} src={} payload={payload_hex}",
                    code.as_str(),
                    emit_src(src)
                ));
            } else {
                errors.push(err(
                    ErrorCode::InvalidInput,
                    "RAW reason содержит кавычку/перевод строки — grammar v0.3 не определяет экранирование",
                ));
            }
        }
    }
}

fn emit_start(start: &StartPoint, out: &mut String, errors: &mut Vec<Diagnostic>) {
    match start {
        StartPoint::Unknown => out.push_str("unknown"), // SPEC §2, Q11
        StartPoint::Known(p) => out.push_str(&emit_vec(p, errors, "start")),
    }
}
