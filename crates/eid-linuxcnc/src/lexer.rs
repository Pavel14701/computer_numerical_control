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
//!    букв не важен; пробел/таб между буквой и числом слова допустим —
//!    зонд rs274 #18: `X 10.5` принимается контроллером); слово длиннее
//!    `max_word_chars` (32) → E_LIMIT_EXCEEDED (COVERAGE §1.2);
//!  * неизвестная/чужая строка (напр. `$`-конфиг GRBL) → line-level
//!    RAW{E_UNSUPPORTED_SYNTAX} (правило двух классов, CORPUS #47;
//!    зеркально eid-grbl); D9 v0.15: payload RAW включает терминатор строки.

use eid_core::ast::SrcSpan;
use eid_core::diag::{Diagnostic, ErrorCode, LimitInfo, Stage};

/// Пороги E_LIMIT_EXCEEDED (COVERAGE §1.2, v0.0 фиксированные).
pub const MAX_WORD_CHARS: usize = 32;
pub const MAX_SOURCE_LINES: usize = 1_000_000;
pub const MAX_OPS_PER_PROGRAM: usize = 200_000;
/// Сумма L/K на блок цикла + guard на число производных опов развёртки.
pub const MAX_UNROLL_ITERATIONS: usize = 10_000;
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
    /// Неизвестная/чужая строка (напр. `$`-конфиг GRBL вне словаря LinuxCNC)
    /// → line-level RAW{E_UNSUPPORTED_SYNTAX} (правило двух классов:
    /// line-local, CORPUS #47; зеркально eid-grbl).
    Unknown,
    /// Слова кадра.
    Words(Vec<Word>),
}

/// Данные одной физической строки.
#[derive(Debug, Clone)]
pub struct LineData {
    pub line_no: usize,
    /// Исходная строка (без терминатора; триммированная) — для payload RAW.
    pub raw: String,
    /// D9 v0.15: терминатор строки (`\r\n` | `\n` | "" у EOF без NL) —
    /// payload RAW включает его.
    pub term: &'static str,
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
        stage: Stage::Parse,
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
    let chunks: Vec<&str> = src.split('\n').collect();
    let chunks_total = chunks.len();
    let mut out = Vec::new();
    for (i, line_raw) in chunks.iter().enumerate() {
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
        let is_last = i + 1 == chunks_total;
        // D9 v0.15: терминатор строки ("\r\n" | "\n" | "" у EOF без NL).
        let (term, content) = if let Some(stripped) = line_raw.strip_suffix('\r') {
            ("\r\n", stripped)
        } else if !is_last {
            ("\n", *line_raw)
        } else {
            ("", *line_raw)
        };
        let raw = content.trim().to_string();
        let tok = lex_line(&raw, file, line_no)?;
        out.push(LineData { line_no, raw, term, tok });
    }
    Ok(out)
}

fn lex_line(raw: &str, file: &str, line_no: usize) -> Result<LineTok, Vec<Diagnostic>> {
    // #17: `(` внутри комментария (вложенные скобки) — комментарий закрыт
    // первой `)`, остаток строки — мусор; реальный rs274 отказывает с
    // «Nested comment found». Наша строгость честна (поведение не меняется,
    // аудит #17) — текст диагностики дополняется причиной.
    let nested_paren = std::cell::Cell::new(false);
    let fail = |code: ErrorCode, msg: String, col: usize| -> Vec<Diagnostic> {
        let msg = if nested_paren.get() {
            format!(
                "{msg} — вложенные скобки в комментарии: комментарий закрыт первой `)`, остаток строки — мусор (реальный rs274: „Nested comment found“)"
            )
        } else {
            msg
        };
        vec![Diagnostic {
            code,
            stage: Stage::Parse,
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
            } else if ch == '(' {
                nested_paren.set(true);
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
                // Ruling итерации 6 (Phase 1.2): O-code строки — line-local →
                // вся строка RAW{E_UNSUPPORTED_SYNTAX} (CORPUS #47-класс;
                // зеркально `$`-строкам), разбор продолжается.
                return Ok(LineTok::Unknown);
            }
            // #18 (зонд rs274): горизонтальные пробелы/табы между буквой и
            // числом слова ДОПУСТИМЫ — `G0 X 10.5 Y20` / `G1 X5. Y 8. F100`
            // rs274 принимает (ошибки лексики нет). Пропуск только здесь, в
            // слово-сканере (комментарии изъяты выше, до разбора слов).
            let mut num_pos = j;
            while num_pos < bytes.len() && (bytes[num_pos] == ' ' || bytes[num_pos] == '\t') {
                num_pos += 1;
            }
            // Число: [sign] digits [. digits]
            let num_start = num_pos;
            let mut m = num_pos;
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
                // Правило двух классов (CORPUS #47): неизвестная строка
                // (напр. `$`-конфиг GRBL вне словаря LinuxCNC) — line-local →
                // вся строка RAW{E_UNSUPPORTED_SYNTAX}, разбор продолжается.
                return Ok(LineTok::Unknown);
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
/// #14 (v0.17): округление — целочисленно-десятичное ИЗ ТЕКСТА, half away
/// from zero. Прежняя формула (v·10⁴).round()/10⁴ округляла ДВАЖДЫ
/// (десятичная строка → f64 → ×10⁴ → round) — на .5-границах направление
/// зависело от шума f64-парса («1.23455» давало 1.2345 вместо 1.2346).
pub fn word_num(s: &str) -> Option<f64> {
    let (neg, body0) = if let Some(rest) = s.strip_prefix('+') {
        (false, rest)
    } else if let Some(rest) = s.strip_prefix('-') {
        (true, rest)
    } else {
        (false, s)
    };
    let body = if body0.starts_with('.') {
        format!("0{body0}")
    } else {
        body0.to_string()
    };
    let (int_part, frac_part) = match body.split_once('.') {
        Some((a, b)) => (a, b),
        None => (body.as_str(), ""),
    };
    if int_part.is_empty() || !int_part.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if !frac_part.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let fb = frac_part.as_bytes();
    // 5-й дробный знак решает: ≥5 → вверх (half away from zero: ровно .5 —
    // тоже вверх), <5 → усечение до 4 знаков.
    let round_up = fb.len() > 4 && fb[4] >= b'5';
    let mut frac_out = [0u8; 4];
    for i in 0..4.min(fb.len()) {
        frac_out[i] = fb[i] - b'0';
    }
    let mut int_out = int_part.to_string();
    if round_up {
        // +1 ulp (0.0001) к модулю: перенос по 4-значной дроби, затем в
        // целую часть (9.99995 → 10.0000).
        let mut carry = 1u8;
        for i in (0..4).rev() {
            let d = frac_out[i] + carry;
            frac_out[i] = d % 10;
            carry = d / 10;
        }
        if carry == 1 {
            let mut arr: Vec<u8> = int_out.into_bytes();
            let mut i = arr.len();
            loop {
                if i == 0 {
                    arr.insert(0, b'1');
                    break;
                }
                i -= 1;
                if arr[i] == b'9' {
                    arr[i] = b'0';
                } else {
                    arr[i] += 1;
                    break;
                }
            }
            int_out = String::from_utf8(arr).expect("ascii digits");
        }
    }
    let frac_str: String = frac_out.iter().map(|&d| (b'0' + d) as char).collect();
    let sign = if neg { "-" } else { "" };
    let text = format!("{sign}{int_out}.{frac_str}");
    text.parse().ok()
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
