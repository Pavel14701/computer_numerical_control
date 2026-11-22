//! Лексер Haas mill NGC — физическая строка → группы токенов блоков,
//! разделённых EOB `;` (§1.3 ловушка №2: `;` — РАЗДЕЛИТЕЛЬ БЛОКА, не
//! комментарий; GRBL/LinuxCNC-лексеры с `;`-комментарием молча съели бы
//! хвосты — запрещено).
//! Прочее (§1.3): `( )`-комментарии (вложенность запрещена → диагностика),
//! `%` — маркер файла IGNORED_SAFE, N-слова — IGNORED_SAFE (drop на лексере),
//! `/` — block delete (маркер, диспозиция RAW(E_RUNTIME_STATE) в канонизаторе),
//! трейлинг-точка `X25.` легальна (и обязательна в эмиссии), переполнение
//! целой части (>9 цифр, §1.2 — диапазон до `?` §10(31), ожидаемо
//! ±99999.9999-класс) → E_LIMIT_EXCEEDED.
//!
//! ПРОБЕЛ ВНУТРИ СЛОВА (`X 25`, `G 1`): Haas-контракт НЕ специфицирует —
//! реализовано СТРОГО (отказ E_INVALID_INPUT, LexErrorKind::SpaceInWord).
//! `?`-реестр (COVERAGE §6, позиция после 68): реальный Haas-лексер не
//! верифицирован; осознанно консервативное чтение — letter-run-детект Macro B
//! (IF/GOTO/WHILE/…) от пробела не зависит. Отличие от eid-fanuc (там пробел
//! в слове легален) — вендорное расхождение, помечено `?` до сверки по manual
//! 96-8210.
//!
//! Лексер отдаёт СЫРЫЕ слова: буква + verbatim-числовая строка + флаг
//! «была ли точка» (RawNum.has_point). DPI-интерпретация (bare-integer →
//! × least increment; F → × least increment по Setting 77) — НЕ здесь:
//! требует модального units-состояния, выполняется в canonicalize.rs
//! (§1.2 ловушка №1 — machinists notation, Setting 162 OFF).

/// Сырое число слова: verbatim-текст + f64-зеркало + флаг десятичной точки.
/// Квантование к 4dp (SPEC §10.6) выполняется в canonicalize при построении
/// Num (для bare-integer — ПОСЛЕ DPI-умножения).
#[derive(Debug, Clone, PartialEq)]
pub struct RawNum {
    pub text: String,
    pub value: f64,
    /// Была ли десятичная точка (`X50.` → true; `X25` → false → DPI-домен;
    /// F: bare → × least increment (Setting 77 OFF), dotted → verbatim).
    pub has_point: bool,
    /// Цифр целой части до точки (без знака; §1.2: максимум 9).
    pub int_digits: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// Слово: буква (uppercased) + сырое число.
    Word { letter: char, num: RawNum },
    /// `( ... )` — аннотация (SPEC §11).
    Comment(String),
    /// `/` в начале блока — block delete.
    Slash,
    /// `%` — маркер начала/конца файла (DNC), IGNORED_SAFE (§1.3).
    Percent,
    /// Macro B — детект `#`-слова / `[ ]`-выражения / letter-run ключевого
    /// слова (IF/GOTO/THEN/WHILE/DO/END, операторы GT/LT/…). Haas — живой
    /// диалект Macro B (Fanuc-совместимый, §4.3 контракта). Остаток блока
    /// потребляется как raw-текст (диспозиция — line-level Raw{SYNTAX}).
    MacroB,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LexErrorKind {
    /// Лексический мусор — E_INVALID_INPUT (SPEC §8).
    UnexpectedChar(char),
    /// Буква без значения / нечисловое значение.
    BadNumber,
    /// `(` без `)`.
    UnterminatedComment,
    /// Вложенный `(` внутри комментария — запрещён (§1.3, подтверждено).
    NestedComment,
    /// Пробел между буквой слова и значением (`X 25`) — строгий отказ.
    /// `?`-реестр: Haas-контракт не специфицирует; реальный лексер не
    /// верифицирован (COVERAGE §6).
    SpaceInWord { letter: char },
    /// Слово длиннее max_word_chars (COVERAGE §1.2: 32 — guard зависания).
    WordTooLong,
    /// Целая часть числа > max_integer_digits (§1.2: «до 9 целых + 4 dp»;
    /// диапазон Haas до `?` §10(31) — ±99999.9999-класс).
    /// Порог не ловится max_word_chars — парсер обязан проверять (§1.2).
    IntOverflow { digits: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub line: u32,
    pub col: u32,
    pub kind: LexErrorKind,
}

/// MAX_WORD_CHARS − 1 (буква); COVERAGE §1.2.
const MAX_NUM_TEXT: usize = 31;
/// §1.2: «до 9 целых + 4 dp» (least increment 0.001 mm/0.0001"); 10-я цифра
/// целой части — переполнение → E_LIMIT_EXCEEDED (max_word_chars не ловит).
pub const MAX_INTEGER_DIGITS: u32 = 9;

/// Токены одного блока (фрагмент физической строки до EOB `;`) + его текст.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockToks {
    pub toks: Vec<Tok>,
    /// Точная байтовая вырезка фрагмента из строки: от конца предыдущего `;`
    /// до `;` ВКЛЮЧИТЕЛЬНО (последний фрагмент — до конца строки). Для RAW
    /// payload (ruling 5: конкатенация фрагментов строки + терминатор строки =
    /// исходная строка байт-в-байт). Не тримируется.
    pub text: String,
}

/// Разбор одной физической строки на блоки: `G0 X0; G1 X5;` → 2 блока.
/// `;` внутри `( )`-комментария — текст комментария, НЕ EOB.
/// `\r` (CRLF) трактуется как whitespace.
/// Macro B: `#`, `[`, `]`, letter-run (IF/GOTO/THEN/WHILE/DO/END/…) →
/// `Tok::MacroB`, остаток блока потребляется как raw-текст до `;` — лексика
/// макро-выражений (`=`, `+`, GT/…) не порождает ошибок разбора (§4.3).
pub fn lex_line(line: &str, line_no: u32) -> Result<Vec<BlockToks>, LexError> {
    let err = |col: u32, kind: LexErrorKind| LexError {
        line: line_no,
        col,
        kind,
    };
    let cs: Vec<char> = line.chars().collect();
    let mut i = 0usize;

    // `%` — маркер файла: строка, начинающаяся с `%` (после whitespace) —
    // целиком маркер (аналогия grbl/fanuc; `%` вне начала строки — мусор).
    let mut w = i;
    while w < cs.len() && cs[w].is_whitespace() {
        w += 1;
    }
    if w < cs.len() && cs[w] == '%' {
        return Ok(vec![BlockToks {
            toks: vec![Tok::Percent],
            text: "%".into(),
        }]);
    }

    let mut groups: Vec<BlockToks> = Vec::new();
    let mut cur: Vec<Tok> = Vec::new();
    let mut start = 0usize; // начало текста текущего фрагмента
    let mut raw_mode = false; // Macro B: потребление блока как raw-текста

    while i < cs.len() {
        let c = cs[i];
        if raw_mode {
            // Macro B-остаток: всё до `;` — raw-текст (включая `(`, `=` и пр.).
            if c == ';' {
                groups.push(BlockToks {
                    toks: std::mem::take(&mut cur),
                    text: cs[start..=i].iter().collect::<String>(),
                });
                raw_mode = false;
                i += 1;
                start = i;
            } else {
                i += 1;
            }
        } else if c.is_whitespace() {
            i += 1;
        } else if c == ';' {
            // EOB: закрыть текущий блок (даже пустой — блоки без слов легальны,
            // канонизатор/parse_file их пропустит без диагностики). Вырезка —
            // до `;` включительно (ruling 5, byte-exact реконструкция).
            groups.push(BlockToks {
                toks: std::mem::take(&mut cur),
                text: cs[start..=i].iter().collect::<String>(),
            });
            i += 1;
            start = i;
        } else if c == '#' || c == '[' || c == ']' {
            // Macro B: переменная/выражение — детект, остаток блока — raw.
            cur.push(Tok::MacroB);
            raw_mode = true;
            i += 1;
        } else if c == '(' {
            let open_col = i as u32 + 1;
            let text_start = i + 1;
            let mut j = text_start;
            loop {
                if j >= cs.len() {
                    return Err(err(open_col, LexErrorKind::UnterminatedComment));
                }
                match cs[j] {
                    // Вложенность запрещена (§1.3, подтверждено) —
                    // диагностика, не молчаливое поглощение скобки текстом.
                    '(' => return Err(err((j + 1) as u32, LexErrorKind::NestedComment)),
                    ')' => break,
                    // `;` внутри комментария — текст, НЕ EOB.
                    _ => {}
                }
                j += 1;
            }
            cur.push(Tok::Comment(cs[text_start..j].iter().collect()));
            i = j + 1;
        } else if c == '/' && !cur.iter().any(|t| matches!(t, Tok::Word { .. })) {
            // Block delete: только до первого слова блока (Haas: `/` перед
            // кадром). `Tok::Slash` после слов — мусор.
            cur.push(Tok::Slash);
            i += 1;
        } else if c.is_ascii_alphabetic() {
            let letter = c.to_ascii_uppercase();
            let word_col = i as u32 + 1;
            // СТРОГО (отличие от eid-fanuc): пробел между буквой и значением
            // — отказ. `?`-реестр: контракт не специфицирует, реальный Haas-
            // лексер не верифицирован. Letter-run Macro B (IF/GOTO/…) —
            // смежные буквы, от этого правила не страдает.
            if i + 1 < cs.len() && cs[i + 1].is_whitespace() {
                return Err(err(word_col, LexErrorKind::SpaceInWord { letter }));
            }
            if i + 1 < cs.len()
                && (cs[i + 1].is_ascii_alphabetic() || matches!(cs[i + 1], '#' | '[' | ']'))
            {
                // Letter-run (`IF`, `GOTO`, `WHILE`, `GT`, …) — Macro B
                // конструкция; в статическом подмножестве легальных
                // двухбуквенных слов нет (адресный язык — одна буква + число).
                cur.push(Tok::MacroB);
                raw_mode = true;
                i += 1;
                continue;
            }
            i += 1;
            let Some((num, next)) = read_number(&cs, i) else {
                return Err(err(word_col, LexErrorKind::BadNumber));
            };
            if num.text.len() > MAX_NUM_TEXT {
                return Err(err(word_col, LexErrorKind::WordTooLong));
            }
            if num.int_digits > MAX_INTEGER_DIGITS {
                return Err(err(
                    word_col,
                    LexErrorKind::IntOverflow {
                        digits: num.int_digits,
                    },
                ));
            }
            if !num.value.is_finite() {
                return Err(err(word_col, LexErrorKind::BadNumber));
            }
            i = next;
            // N-слова — IGNORED_SAFE (§1.3): номера кадров (цель M97/GOTO —
            // Macro B-строки и так raw), не выходим в токены.
            if letter != 'N' {
                cur.push(Tok::Word { letter, num });
            }
        } else {
            return Err(err(i as u32 + 1, LexErrorKind::UnexpectedChar(c)));
        }
    }
    // Хвост без завершающего `;` — последний блок (DNC-файлы без trailing
    // EOB). Хвост из одних whitespace тоже отдаётся (byte-exact: `\r`
    // CRLF-терминатора доходит до payload последнего блока строки).
    let tail: String = cs[start..].iter().collect::<String>();
    if !cur.is_empty() || !tail.is_empty() {
        groups.push(BlockToks {
            toks: cur,
            text: tail,
        });
    }
    Ok(groups)
}

/// Число Haas: `[+-]?` (`цифры+` [`.` `цифры*`]? | `.` `цифры+`).
/// Трейлинг-точка легальна (`X25.`, §1.2 — обязательна в эмиссии).
/// Возвращает None = числа нет.
fn read_number(cs: &[char], mut i: usize) -> Option<(RawNum, usize)> {
    let mut text = String::new();
    if i < cs.len() && (cs[i] == '+' || cs[i] == '-') {
        text.push(cs[i]);
        i += 1;
    }
    let mut int_digits: u32 = 0;
    let mut has_point = false;
    let mut frac_digits = false;
    while i < cs.len() && cs[i].is_ascii_digit() {
        int_digits += 1;
        text.push(cs[i]);
        i += 1;
    }
    if i < cs.len() && cs[i] == '.' {
        has_point = true;
        text.push('.');
        i += 1;
        while i < cs.len() && cs[i].is_ascii_digit() {
            frac_digits = true;
            text.push(cs[i]);
            i += 1;
        }
    }
    // Осмысленное число: целая часть ИЛИ дробная (`.5`) присутствуют.
    if int_digits == 0 && !frac_digits {
        return None;
    }
    let value: f64 = text.parse().ok()?;
    Some((
        RawNum {
            text,
            value,
            has_point,
            int_digits,
        },
        i,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(letter: char, text: &str, has_point: bool) -> Tok {
        let num = read_number(&text.chars().collect::<Vec<_>>(), 0).unwrap().0;
        assert_eq!(num.text, text);
        assert_eq!(num.has_point, has_point);
        Tok::Word { letter, num }
    }

    fn lex_one(line: &str) -> Vec<Tok> {
        let mut groups = lex_line(line, 1).unwrap();
        assert_eq!(groups.len(), 1, "ожидался 1 блок: {line}");
        groups.remove(0).toks
    }

    #[test]
    fn eob_splits_blocks() {
        // §1.3 ловушка №2: `;` — разделитель блоков, НЕ комментарий.
        let groups = lex_line("G0 X0; G1 X5;", 1).unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0].toks,
            vec![word('G', "0", false), word('X', "0", false)]
        );
        assert_eq!(
            groups[1].toks,
            vec![word('G', "1", false), word('X', "5", false)]
        );
        // ruling 5: точная вырезка фрагмента, `;` включён, без трима.
        assert_eq!(groups[0].text, "G0 X0;");
        assert_eq!(groups[1].text, " G1 X5;");
    }

    #[test]
    fn trailing_eob_and_empty_blocks() {
        // `G1 X5.;` — хвостовой EOB: пустой фрагмент после `;` не порождает
        // группу (пустые блоки легальны — пропуск без диагностики, §1.3).
        let groups = lex_line("G1 X5.;", 1).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].text, "G1 X5.;");
        // `;;` — только пустые блоки: группы без слов (parse_file пропустит).
        let groups = lex_line(" ;; ", 2).unwrap();
        assert!(groups.iter().all(|g| g.toks.is_empty()));
    }

    #[test]
    fn no_trailing_eob_dnc_tail() {
        // DNC-выгрузка: последний блок без `;`.
        let groups = lex_line("G0 X0; G1 X5.", 1).unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[1].text, " G1 X5.");
    }

    #[test]
    fn comments_paren() {
        // Комментарий вне слов и между словами; `;` внутри комментария —
        // текст комментария, НЕ разделитель блоков.
        let toks = lex_one("(face top) G1 X1 (rough)");
        assert_eq!(toks.len(), 4);
        assert_eq!(toks[0], Tok::Comment("face top".into()));
        assert_eq!(toks[1], word('G', "1", false));
        assert_eq!(toks[2], word('X', "1", false));
        assert_eq!(toks[3], Tok::Comment("rough".into()));
        let groups = lex_line("G0 X0; (drill; then) G1 X5;", 2).unwrap();
        assert_eq!(groups.len(), 2, "`;` внутри ( ) не делит блок");
        assert_eq!(groups[1].toks[0], Tok::Comment("drill; then".into()));
    }

    #[test]
    fn comment_errors_nesting_and_unterminated() {
        let e = lex_line("(nested (bad)) G1 X1", 1).unwrap_err();
        assert_eq!(e.kind, LexErrorKind::NestedComment);
        let e = lex_line("(unclosed G1 X1", 2).unwrap_err();
        assert_eq!(e.kind, LexErrorKind::UnterminatedComment);
    }

    #[test]
    fn space_in_word_strict_reject() {
        // `?`-реестр (контракт не специфицирует): строгий отказ на пробел
        // между буквой и значением; пробелы МЕЖДУ словами легальны.
        let e = lex_line("X 25", 1).unwrap_err();
        assert_eq!(e.kind, LexErrorKind::SpaceInWord { letter: 'X' });
        let e = lex_line("G 1 X5.", 2).unwrap_err();
        assert_eq!(e.kind, LexErrorKind::SpaceInWord { letter: 'G' });
        // Между словами — ок.
        assert_eq!(lex_one("  G1  X5.\tY6  ").len(), 3);
    }

    #[test]
    fn macro_b_hash_expression_detected() {
        // `#`-переменная + арифметика: детект MacroB, остаток блока — raw
        // (`=`, `+` не порождают ошибок лексики, §4.3 — Haas Macro B
        // Fanuc-совместимый).
        let groups = lex_line("#100 = #100 + 1;", 1).unwrap();
        assert_eq!(groups.len(), 1);
        assert!(groups[0].toks.contains(&Tok::MacroB));
        assert_eq!(groups[0].text, "#100 = #100 + 1;");
        // Макро-строка не роняет лексер даже с `(`/`)`-текстом внутри.
        let groups = lex_line("IF [#1 GT 5] GOTO 10;", 2).unwrap();
        assert!(groups[0].toks.contains(&Tok::MacroB));
    }

    #[test]
    fn macro_b_bracket_and_letter_runs() {
        // `[ ]`-выражение.
        assert!(lex_line("X[ #1 + 5 ];", 1).unwrap()[0]
            .toks
            .contains(&Tok::MacroB));
        // Letter-runs: IF/GOTO/WHILE/DO/END/THEN/GT/LT.
        for line in [
            "IF #1 GT 5 GOTO 10;",
            "WHILE #100 LT 10 DO 1;",
            "END 1;",
            "GOTO 5;",
            "X[1] Y2;",
        ] {
            let groups = lex_line(line, 1).unwrap();
            assert!(
                groups[0].toks.contains(&Tok::MacroB),
                "letter-run/bracket → MacroB: {line}"
            );
        }
    }

    #[test]
    fn macro_b_eob_and_neighbors_still_split() {
        // `;` внутри macro-строки — всё ещё EOB: соседние блоки живут.
        let groups = lex_line("#1 = 5; G1 X5. F100.;", 1).unwrap();
        assert_eq!(groups.len(), 2);
        assert!(groups[0].toks.contains(&Tok::MacroB));
        assert_eq!(groups[0].text, "#1 = 5;");
        assert_eq!(groups[1].text, " G1 X5. F100.;");
    }

    #[test]
    fn macro_hash_inside_comment_is_annotation() {
        // `#` внутри ( )-комментария — текст аннотации, НЕ Macro B (SPEC §11).
        let groups = lex_line("(cut #100 depth) G1 X5.;", 1).unwrap();
        assert_eq!(groups.len(), 1);
        assert!(!groups[0].toks.contains(&Tok::MacroB));
        assert_eq!(groups[0].toks[0], Tok::Comment("cut #100 depth".into()));
    }

    #[test]
    fn single_letter_words_not_macro() {
        // Одиночная буква + число (вплотную) — обычные слова.
        let groups = lex_line("G1 X5 D0 H00", 1).unwrap();
        assert!(!groups[0].toks.contains(&Tok::MacroB));
        assert_eq!(groups[0].toks.len(), 4);
    }

    #[test]
    fn trailing_dot_legal_bare_integer_flagged() {
        // Трейлинг-точка легальна (§1.2); bare-integer несёт has_point=false
        // (DPI-домен — machinists notation, интерпретация в canonicalize).
        assert_eq!(lex_one("X50.")[0], word('X', "50.", true));
        assert_eq!(lex_one("X25")[0], word('X', "25", false));
        assert_eq!(lex_one("x-5.25")[0], word('X', "-5.25", true));
        assert_eq!(lex_one("Y+.5")[0], word('Y', "+.5", true));
    }

    #[test]
    fn integer_overflow_limit() {
        // §1.2: >9 цифр целой части → IntOverflow (10 цифр — отказ, 9 — ок).
        let e = lex_line("X1234567890.", 1).unwrap_err();
        assert_eq!(e.kind, LexErrorKind::IntOverflow { digits: 10 });
        assert!(lex_one("X123456789.").len() == 1, "9 цифр легальны");
        assert!(lex_one("X-999999999.9999").len() == 1);
    }

    #[test]
    fn percent_marker_whole_line() {
        let groups = lex_line("%", 1).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].toks, vec![Tok::Percent]);
        assert_eq!(lex_line("  %  ", 2).unwrap()[0].toks, vec![Tok::Percent]);
    }

    #[test]
    fn n_words_dropped() {
        let toks = lex_one("N10 G90 X-1.5");
        assert_eq!(toks, vec![word('G', "90", false), word('X', "-1.5", true)]);
    }

    #[test]
    fn block_delete_slash_at_start() {
        let toks = lex_one("/ G1 X5");
        assert_eq!(toks[0], Tok::Slash);
        assert_eq!(toks[1], word('G', "1", false));
        // `/` после слов — мусор (UnexpectedChar).
        let e = lex_line("G1 / X5", 3).unwrap_err();
        assert_eq!(e.kind, LexErrorKind::UnexpectedChar('/'));
    }

    #[test]
    fn crlf_tail_reaches_payload() {
        assert_eq!(lex_line("G1 X5.5\r", 1).unwrap()[0].text, "G1 X5.5\r");
    }

    #[test]
    fn errors_garbage_and_bad_number() {
        assert_eq!(
            lex_line("G1 &", 1).unwrap_err().kind,
            LexErrorKind::UnexpectedChar('&')
        );
        assert_eq!(
            lex_line("G1 X", 2).unwrap_err().kind,
            LexErrorKind::BadNumber
        );
        let long = format!("X{}", "9".repeat(40));
        assert_eq!(
            lex_line(&long, 3).unwrap_err().kind,
            LexErrorKind::WordTooLong
        );
        assert_eq!(
            lex_line("G1 X5.5.5", 4).unwrap_err().kind,
            LexErrorKind::UnexpectedChar('.')
        );
    }
}
