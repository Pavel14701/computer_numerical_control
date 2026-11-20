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
            let value: f64 = num_text.parse().map_err(|_| err(word_col, LexErrorKind::BadNumber))?;
            if !value.is_finite() {
                return Err(err(word_col, LexErrorKind::BadNumber));
            }
            // SPEC §10.6 v0.12 — квантование входа: слова диалектного входа
            // → 4dp при построении Num (single-final-rounding и на входах).
            let value = (value * 10_000.0).round() / 10_000.0;
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
}
