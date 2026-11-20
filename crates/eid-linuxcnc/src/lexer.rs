//! LinuxCNC-лексер (Агент D, итерация 2): исходный текст → токены строк.
//!
//! Правила (COVERAGE §4, SPEC §1.1/§8):
//!  * комментарии `(...)` (без вложенности) и `;`-до-конца-строки — аннотации
//!    (SPEC §11), изымаются до разбора слов;
//!  * `%` — маркер программы, одиночный на строке → IGNORED_SAFE;
//!  * `/` block delete в начале строки → вся строка RAW(E_RUNTIME_STATE)
//!    (COVERAGE §4: исполнение зависит от переключателя станка);
//!  * `#`-параметры/выражения `[...]` → жёсткий отказ E_RUNTIME_STATE;
//!    `if/while/do/...` и O-слова → жёсткий отказ E_UNSUPPORTED_SYNTAX
//!    (SPEC §1.1);
//!  * слово = буква + число (пробелы между словами необязательны, регистр
//!    букв не важен); слово длиннее `max_word_chars` (32) → E_LIMIT_EXCEEDED
//!    (COVERAGE §1.2);
//!  * прочий лексический мусор → E_INVALID_INPUT.

use eid_core::ast::SrcSpan;
use eid_core::diag::{Diagnostic, ErrorCode, LimitInfo};

/// Пороги E_LIMIT_EXCEEDED (COVERAGE §1.2, v0.0 фиксированные).
pub const MAX_WORD_CHARS: usize = 32;
pub const MAX_SOURCE_LINES: usize = 1_000_000;
pub const MAX_OPS_PER_PROGRAM: usize = 200_000;
// max_unroll_iterations в итерации 2 не применяется (развёртывание циклов —
// итерация 3, см. отчёт).

/// Слово: буква + число (verbatim-строка числа).
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub letter: char,
    pub number: String,
    /// Колонка буквы в исходной строке (1-based; для диагностики).
    pub col: usize,
}

/// Класс строки после лексирования.
#[derive(Debug, Clone, PartialEq)]
pub enum LineTok {
    /// Пустая строка или только комментарий.
    Blank,
    /// Одиночный `%` — маркер программы (IGNORED_SAFE).
    Percent,
    /// Block delete `/` в начале — вся строка RAW(E_RUNTIME_STATE).
    Slash,
    /// Слова кадра.
    Words(Vec<Word>),
}

/// Данные одной физической строки.
#[derive(Debug, Clone)]
pub struct LineData {
    pub line_no: usize,
    /// Исходная строка (без CRLF; триммированная) — для payload RAW (D9).
    pub raw: String,
    pub tok: LineTok,
}

/// Управляющие ключевые слова LinuxCNC (SPEC §1.1: поток управления вне L1).
const CONTROL_KEYWORDS: &[&str] = &[
    "if", "else", "elseif", "endif", "while", "do", "endwhile", "repeat", "endrepeat",
    "continue", "break", "return", "sub", "endsub",
];

pub fn limit_diag(
    threshold: &'static str,
    threshold_value: f64,
    actual: f64,
    file: &str,
    line_no: usize,
    col: usize,
) -> Diagnostic {
    Diagnostic {
        code: ErrorCode::LimitExceeded,
        message: format!("limit exceeded: {threshold} = {threshold_value}, actual = {actual}"),
        src: Some(SrcSpan {
            file: file.to_string(),
            line_start: line_no as u32,
            col_start: col as u32,
            line_end: line_no as u32,
            col_end: col as u32,
        }),
        options: Vec::new(),
        limit: Some(LimitInfo { threshold, threshold_value, actual }),
    }
}

/// Весь текст → лексированные строки. Жёсткие лексические отказы —
/// Err (SPEC §8: E_INVALID_INPUT / E_RUNTIME_STATE / E_UNSUPPORTED_SYNTAX /
/// E_LIMIT_EXCEEDED — вход сломан или вне грамматики L1).
pub fn lex_program(src: &str, file: &str) -> Result<Vec<LineData>, Vec<Diagnostic>> {
    let mut out = Vec::new();
    for (i, line_raw) in src.split('\n').enumerate() {
        if i + 1 > MAX_SOURCE_LINES {
            return Err(vec![limit_diag(
                "max_source_lines",
                MAX_SOURCE_LINES as f64,
                (i + 1) as f64,
                file,
                i + 1,
                1,
            )]);
        }
        let line_no = i + 1;
        let raw = line_raw.strip_suffix('\r').unwrap_or(line_raw).trim().to_string();
        let tok = lex_line(&raw, file, line_no)?;
        out.push(LineData { line_no, raw, tok });
    }
    Ok(out)
}

fn lex_line(raw: &str, file: &str, line_no: usize) -> Result<LineTok, Vec<Diagnostic>> {
    let fail = |code: ErrorCode, msg: String, col: usize| -> Vec<Diagnostic> {
        vec![Diagnostic {
            code,
            message: msg,
            src: Some(SrcSpan {
                file: file.to_string(),
                line_start: line_no as u32,
                col_start: col as u32,
                line_end: line_no as u32,
                col_end: raw.len().max(1) as u32,
            }),
            options: Vec::new(),
            limit: None,
        }]
    };

    // Комментарии: (...) — без вложенности (до первой ')'), ; — до конца.
    let mut code = String::with_capacity(raw.len());
    let mut in_paren = false;
    let mut col_of: Vec<usize> = Vec::with_capacity(raw.len()); // колонки code-символов
    for (ci, ch) in raw.char_indices() {
        let col = ci + 1;
        if in_paren {
            if ch == ')' {
                in_paren = false;
            }
            continue;
        }
        match ch {
            '(' => in_paren = true,
            ';' => break,
            _ => {
                code.push(ch);
                col_of.push(col);
            }
        }
    }
    // Незакрытая '(' — остаток строки комментируется (протокол; см. отчёт).

    let trimmed = code.trim();
    if trimmed.is_empty() {
        return Ok(LineTok::Blank);
    }
    if trimmed == "%" {
        return Ok(LineTok::Percent);
    }

    // Block delete — '/' первым символом кадра (после пробела).
    // Собственная колонка первого символа trimmed в code:
    let first_col = code.len() - code.trim_start().len() + 1;
    let _ = first_col;
    if trimmed.starts_with('/') {
        return Ok(LineTok::Slash);
    }

    // Сканер слов. Пробелы/табы — разделители; буква начинает слово.
    let bytes: Vec<char> = trimmed.chars().collect();
    // колонки trimmed-символов в исходной строке:
    let mut cols: Vec<usize> = Vec::new();
    {
        let mut seen_start = false;
        for (ci, ch) in code.char_indices() {
            if !seen_start && !ch.is_whitespace() {
                seen_start = true;
            }
            if seen_start {
                cols.push(ci + 1);
            }
        }
    }
    let col_at = |k: usize| -> usize { cols.get(k).copied().unwrap_or(1) };

    let mut words: Vec<Word> = Vec::new();
    let mut k = 0usize;
    while k < bytes.len() {
        let ch = bytes[k];
        if ch.is_whitespace() {
            k += 1;
            continue;
        }
        if ch.is_ascii_alphabetic() {
            // Буквенная последовательность: 1 буква = слово; 2+ — ключевое слово
            // (поток управления, SPEC §1.1) или мусор.
            let mut j = k;
            while j < bytes.len() && bytes[j].is_ascii_alphabetic() {
                j += 1;
            }
            let alpha: String = bytes[k..j].iter().collect();
            if j - k > 1 {
                let lower = alpha.to_ascii_lowercase();
                if CONTROL_KEYWORDS.contains(&lower.as_str()) {
                    return Err(fail(
                        ErrorCode::UnsupportedSyntax,
                        format!("control flow keyword `{alpha}` — вне грамматики L1 (SPEC §1.1)"),
                        col_at(k),
                    ));
                }
                return Err(fail(
                    ErrorCode::InvalidInput,
                    format!("lexical garbage `{alpha}` (слово = одна буква + число)"),
                    col_at(k),
                ));
            }
            let letter = alpha.chars().next().unwrap().to_ascii_uppercase();
            if letter == 'O' {
                return Err(fail(
                    ErrorCode::UnsupportedSyntax,
                    "O-code (подпрограммы/макро) — вне грамматики L1 (SPEC §1.1, Q10)".into(),
                    col_at(k),
                ));
            }
            // Число: [sign] digits [. digits]
            let num_start = j;
            let mut m = j;
            if m < bytes.len() && (bytes[m] == '+' || bytes[m] == '-') {
                m += 1;
            }
            let int_start = m;
            while m < bytes.len() && bytes[m].is_ascii_digit() {
                m += 1;
            }
            if m == int_start {
                // Допускаем ".5"-форму? Число обязано начинаться с цифры или '.'
                if m < bytes.len() && bytes[m] == '.' {
                    // форма ".digits"
                    m += 1;
                    let f0 = m;
                    while m < bytes.len() && bytes[m].is_ascii_digit() {
                        m += 1;
                    }
                    if m == f0 {
                        return Err(fail(
                            ErrorCode::InvalidInput,
                            format!("word `{letter}`: число без цифр"),
                            col_at(k),
                        ));
                    }
                } else {
                    return Err(fail(
                        ErrorCode::InvalidInput,
                        format!("word `{letter}` без числа"),
                        col_at(k),
                    ));
                }
            } else if m < bytes.len() && bytes[m] == '.' {
                m += 1;
                let f0 = m;
                while m < bytes.len() && bytes[m].is_ascii_digit() {
                    m += 1;
                }
                if m == f0 {
                    // "10." — трейлинг-точка без дробной части: допустим
                    // (число целиком — digits), дробной части нет.
                }
            }
            let number: String = bytes[num_start..m].iter().collect();
            if number.len() > MAX_WORD_CHARS {
                return Err(vec![limit_diag(
                    "max_word_chars",
                    MAX_WORD_CHARS as f64,
                    number.len() as f64,
                    file,
                    line_no,
                    col_at(k),
                )]);
            }
            words.push(Word { letter, number, col: col_at(k) });
            k = m;
            continue;
        }
        match ch {
            '#' => {
                return Err(fail(
                    ErrorCode::RuntimeState,
                    "`#`-параметр/переменная — значение зависит от состояния исполнения (SPEC §1.1)".into(),
                    col_at(k),
                ))
            }
            '[' => {
                return Err(fail(
                    ErrorCode::RuntimeState,
                    "выражение `[...]` — вычислимо только при исполнении (SPEC §1.1)".into(),
                    col_at(k),
                ))
            }
            ']' => {
                return Err(fail(
                    ErrorCode::InvalidInput,
                    "закрывающая `]` без открывающей `[`".into(),
                    col_at(k),
                ))
            }
            '%' => {
                return Err(fail(
                    ErrorCode::InvalidInput,
                    "`%` вне начала кадра".into(),
                    col_at(k),
                ))
            }
            '/' => {
                return Err(fail(
                    ErrorCode::InvalidInput,
                    "`/` внутри кадра (block delete допустим только в начале)".into(),
                    col_at(k),
                ))
            }
            _ => {
                return Err(fail(
                    ErrorCode::InvalidInput,
                    format!("lexical garbage `{ch}`"),
                    col_at(k),
                ))
            }
        }
    }
    if words.is_empty() {
        return Ok(LineTok::Blank);
    }
    Ok(LineTok::Words(words))
}

/// Число слова → f64. Принимает "+5", "-.5", "10.", "10.0000".
/// SPEC §10.6 v0.12 — квантование входа: слова диалектного входа → 4dp
/// при построении значения (single-final-rounding применяется и ко входам);
/// zero-length проверки выполняются уже по квантованным значениям.
/// Формула зеркальна eid-grbl (R3-симметрия): (v·10⁴).round()/10⁴.
pub fn word_num(s: &str) -> Option<f64> {
    let norm = if let Some(rest) = s.strip_prefix('+') {
        if rest.starts_with('.') { format!("0{rest}") } else { rest.to_string() }
    } else if s.starts_with("-.") {
        format!("-0{}", &s[1..])
    } else if s.starts_with('.') {
        format!("0{s}")
    } else {
        s.to_string()
    };
    let v: f64 = norm.parse().ok()?;
    Some((v * 10_000.0).round() / 10_000.0)
}

/// G/M-номер → (major, minor): "91.1" → (91, 1), "0" → (0, 0).
pub fn code_num(s: &str) -> Option<(i64, u32)> {
    let (maj, min) = match s.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (s, None),
    };
    if maj.is_empty() || !maj.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let major: i64 = maj.parse().ok()?;
    let minor = match min {
        None => 0,
        Some(b) => {
            if b.is_empty() || !b.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            b.parse::<u32>().ok()?
        }
    };
    Some((major, minor))
}
