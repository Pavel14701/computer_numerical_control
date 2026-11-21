//! Лексер GRBL 1.1 — строка → токены: слова (буква+число), комментарии,
//! маркеры. Этап 1 (Агент C). Правила: COVERAGE §4 (N-слова, `%`, `/`),
//! n3 (`$`-config → ConfigLine), комментарии — аннотации (SPEC §11),
//! case-insensitive, whitespace между словами пропускается (grbl gcode.c).

use eid_core::ast::Num;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentStyle {
    Paren,    // ( ... )
    Semicolon, // ; до конца строки
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// Слово: буква (uppercased) + verbatim-число (SPEC §10.1).
    Word { letter: char, value: Num },
    Comment { style: CommentStyle, text: String },
    /// `/` в начале строки (Q3: поддержка в GRBL не сверена; маркер — решение на этапе 2).
    BlockDelete,
    /// Строка `$...=...` — конфигурация станка, RAW(E_UNSUPPORTED_SYNTAX) (COVERAGE §4).
    ConfigLine(String),
    /// `%` — маркер программы, IGNORED_SAFE (COVERAGE §4).
    Percent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexErrorKind {
    /// Лексический мусор — E_INVALID_INPUT (SPEC §8).
    UnexpectedChar(char),
    /// Буква без значения / нечисловое значение.
    BadNumber,
    /// `(` без `)`.
    UnterminatedComment,
    /// Слово длиннее max_word_chars (COVERAGE §1.2: 32 — guard зависания).
    WordTooLong,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub line: u32,
    pub col: u32,
    pub kind: LexErrorKind,
}

/// MAX_WORD_CHARS − 1 (буква); COVERAGE §1.2.
const MAX_NUM_TEXT: usize = 31;

/// Разбор одной строки. `\r` (CRLF) трактуется как whitespace.
pub fn lex_line(line: &str, line_no: u32) -> Result<Vec<Tok>, LexError> {
    let err = |col: u32, kind: LexErrorKind| LexError { line: line_no, col, kind };
    let cs: Vec<char> = line.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0usize;

    // Маркеры начала строки.
    while i < cs.len() && cs[i].is_whitespace() {
        i += 1;
    }
    if i < cs.len() && cs[i] == '$' {
        return Ok(vec![Tok::ConfigLine(line.trim().to_string())]);
    }
    if i < cs.len() && cs[i] == '%' {
        return Ok(vec![Tok::Percent]);
    }
    if i < cs.len() && cs[i] == '/' {
        toks.push(Tok::BlockDelete);
        i += 1;
    }

    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '(' {
            let start = i + 1;
            let mut j = start;
            while j < cs.len() && cs[j] != ')' {
                j += 1;
            }
            if j >= cs.len() {
                return Err(err(i as u32 + 1, LexErrorKind::UnterminatedComment));
            }
            toks.push(Tok::Comment { style: CommentStyle::Paren, text: cs[start..j].iter().collect() });
            i = j + 1;
        } else if c == ';' {
            let text: String = cs[i + 1..].iter().collect();
            toks.push(Tok::Comment { style: CommentStyle::Semicolon, text });
            i = cs.len();
        } else if c.is_ascii_alphabetic() {
            let letter = c.to_ascii_uppercase();
            let word_col = i as u32 + 1;
            i += 1;
            while i < cs.len() && cs[i].is_whitespace() {
                i += 1;
            }
            let (num_text, next) = read_number(&cs, i);
            if num_text.is_empty() {
                return Err(err(word_col, LexErrorKind::BadNumber));
            }
            if num_text.len() > MAX_NUM_TEXT {
                return Err(err(word_col, LexErrorKind::WordTooLong));
            }
            // SPEC §10.6 v0.12 — квантование входа: слова диалектного входа
            // → 4dp при построении Num (single-final-rounding и на входах).
            // #14 (v0.17): округление — целочисленно-десятичное ИЗ ТЕКСТА,
            // half away from zero (зеркало eid-linuxcnc/lexer.rs::word_num).
            let value = word_num(&num_text).ok_or_else(|| err(word_col, LexErrorKind::BadNumber))?;
            if !value.is_finite() {
                return Err(err(word_col, LexErrorKind::BadNumber));
            }
            i = next;
            // N-слова — IGNORED_SAFE (COVERAGE §4): не выходим в токены.
            if letter != 'N' {
                toks.push(Tok::Word { letter, value: Num::new(num_text, value) });
            }
        } else {
            return Err(err(i as u32 + 1, LexErrorKind::UnexpectedChar(c)));
        }
    }
    Ok(toks)
}

/// Число GRBL: `[+-]?` цифры `[.` цифры] (read_float в grbl gcode.c).
/// Возвращает (текст, индекс после числа); пустой текст = числа нет.
fn read_number(cs: &[char], mut i: usize) -> (String, usize) {
    let start = i;
    if i < cs.len() && (cs[i] == '+' || cs[i] == '-') {
        i += 1;
    }
    let mut digits = false;
    while i < cs.len() && cs[i].is_ascii_digit() {
        i += 1;
        digits = true;
    }
    if i < cs.len() && cs[i] == '.' {
        i += 1;
        while i < cs.len() && cs[i].is_ascii_digit() {
            i += 1;
            digits = true;
        }
    }
    if digits {
        (cs[start..i].iter().collect(), i)
    } else {
        (String::new(), start)
    }
}

/// Значение слова → f64, квантованное к 4dp (SPEC §10.6 v0.12).
/// #14 (v0.17): округление — целочисленно-десятичное ИЗ ТЕКСТА, half away
/// from zero. Прежняя формула (v·10⁴).round()/10⁴ округляла ДВАЖДЫ
/// (десятичная строка → f64 → ×10⁴ → round) — на .5-границах направление
/// зависело от шума f64-парса («1.23455» давало 1.2345 вместо 1.2346).
/// Зеркало eid-linuxcnc/lexer.rs::word_num (R3-консистентность адаптеров).
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

#[cfg(test)]
mod tests {
    use super::*;

    fn word(letter: char, text: &str) -> Tok {
        Tok::Word { letter, value: Num::new(text.to_string(), text.parse().unwrap()) }
    }

    #[test]
    fn pilot_header_words() {
        let toks = lex_line("G21 G90 G94 G17", 1).unwrap();
        assert_eq!(toks, vec![word('G', "21"), word('G', "90"), word('G', "94"), word('G', "17")]);
    }

    #[test]
    fn pilot_lowercase_and_signed() {
        let toks = lex_line("g2 x-5.0000 y5.0", 2).unwrap();
        assert_eq!(toks, vec![word('G', "2"), word('X', "-5.0000"), word('Y', "5.0")]);
    }

    #[test]
    fn comments_paren_and_semicolon() {
        let toks = lex_line("(face top) G1 X1 ; feed pass", 3).unwrap();
        assert_eq!(
            toks,
            vec![
                Tok::Comment { style: CommentStyle::Paren, text: "face top".into() },
                word('G', "1"),
                word('X', "1"),
                Tok::Comment { style: CommentStyle::Semicolon, text: " feed pass".into() },
            ]
        );
    }

    #[test]
    fn markers_config_percent_block_delete() {
        assert_eq!(lex_line("$110=250.000", 1).unwrap(), vec![Tok::ConfigLine("$110=250.000".into())]);
        assert_eq!(lex_line("%", 2).unwrap(), vec![Tok::Percent]);
        let bd = lex_line("/ G1 X5", 3).unwrap();
        assert_eq!(bd[0], Tok::BlockDelete);
        assert_eq!(bd[1], word('G', "1"));
    }

    #[test]
    fn n_word_skipped_decimal_code() {
        let toks = lex_line("N10 G91.1", 4).unwrap();
        assert_eq!(toks, vec![word('G', "91.1")]);
    }

    #[test]
    fn errors() {
        assert_eq!(lex_line("G1 &", 1).unwrap_err().kind, LexErrorKind::UnexpectedChar('&'));
        assert_eq!(lex_line("G1 X", 2).unwrap_err().kind, LexErrorKind::BadNumber);
        assert_eq!(lex_line("(unclosed", 3).unwrap_err().kind, LexErrorKind::UnterminatedComment);
        let long = format!("X{}", "9".repeat(40));
        assert_eq!(lex_line(&long, 4).unwrap_err().kind, LexErrorKind::WordTooLong);
    }

    // ===========================================================================
    // Аудит #14 (SPEC §10.6 v0.17): word_num — один финальный раунд ИЗ ТЕКСТА
    // ===========================================================================

    #[test]
    fn audit14_word_num_single_final_rounding_from_text() {
        // .5-границы: 5-й знак ≥ 5 → вверх по ТЕКСТУ, без f64-шума парса.
        assert_eq!(word_num("1.23455"), Some(1.2346), "старая формула давала 1.2345 (двойное округление)");
        assert_eq!(word_num("0.00005"), Some(0.0001));
        assert_eq!(word_num("1.99995"), Some(2.0), "перенос в целую часть");
        assert_eq!(word_num("-1.23455"), Some(-1.2346), "half AWAY from zero");
        // 5-й знак < 5 → усечение (без округления вверх).
        assert_eq!(word_num("1.23454"), Some(1.2345));
        assert_eq!(word_num("0.00004"), Some(0.0), "CORPUS #13 — квантование к старту");
        // ≤ 4 дробных знаков — значение текста без изменений (формат grbl).
        assert_eq!(word_num("10."), Some(10.0));
        assert_eq!(word_num("+5"), Some(5.0));
        assert_eq!(word_num("0"), Some(0.0));
        assert_eq!(word_num("10.0000"), Some(10.0));
        // Мусор — None.
        assert_eq!(word_num(""), None);
        assert_eq!(word_num("-"), None);
        assert_eq!(word_num("1.2.3"), None);
        assert_eq!(word_num("abc"), None);
    }

    #[test]
    fn audit14_word_num_fmt_num_roundtrip_invariant() {
        // Инвариант (аудит #14): word_num(s) — неподвижная точка fmt_num:
        // word_num(s) == parse(fmt_num(word_num(s))) на .5-границах.
        for s in ["1.23455", "0.00005", "2.00005", "-1.23455", "9.99995", "1.23454", "123.45675", "0.00015"] {
            let v = word_num(s).expect("parses");
            let rt: f64 = eid_core::emitter::fmt_num(v).parse().expect("fmt_num parses");
            assert_eq!(v, rt, "инвариант сломан на {s}: {v} vs fmt_num {}", eid_core::emitter::fmt_num(v));
        }
    }

    #[test]
    fn audit14_lex_line_quantizes_from_text() {
        // Значение слова строится из ТЕКСТА: X1.23455 → 1.2346 (было 1.2345 —
        // двойное округление через f64-шум).
        let toks = lex_line("X1.23455", 1).unwrap();
        assert_eq!(toks, vec![Tok::Word { letter: 'X', value: Num::new("1.23455".into(), 1.2346) }]);
    }
}
