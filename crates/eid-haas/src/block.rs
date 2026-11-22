//! Блок (кадр) Haas NGC — результат разбора файла. Этап 1: lexing +
//! word-классификация + проверка «один код на модальную группу» + byte-exact
//! payload для RAW-опов, БЕЗ канонизации в EID. Зеркало eid-fanuc block.rs.
//! EOB `;` делит физическую строку на блоки (lexer.rs); пустые блоки —
//! пропуск без диагностики (§1.3). Порядок слов в блоке произвольный
//! (адресный язык, §1.3) — парсер адресных слов, не позиционный.
//!
//! RAW payload (ruling 5 оркестратора, D9-конвенция): payload Raw-фрагмента =
//! точная байтовая вырезка фрагмента (`;` включён, whitespace сохранён);
//! гарантия round-trip — конкатенация фрагментов строки + терминатор строки =
//! исходная строка байт-в-байт. Пустые фрагменты между `;` (`G0;; G1`) —
//! перенос байтов в payload следующего блока той же строки.

use crate::lexer::{self, BlockToks, LexError, RawNum, Tok};
use crate::state::{modal_group, GCode, MCode, ModalGroup, WordKind};

/// Строковые маркеры (диспозиции — канонизатор).
#[derive(Debug, Clone, PartialEq)]
pub enum Marker {
    /// `%` — маркер начала/конца файла, IGNORED_SAFE (§1.3).
    Percent,
    /// `/` block delete → RAW(E_RUNTIME_STATE) (§1.3, как LinuxCNC/Fanuc).
    BlockDelete,
}

/// Слово: буква + сырое число (has_point — DPI-домен решается в canonicalize).
#[derive(Debug, Clone, PartialEq)]
pub struct WordValue {
    pub letter: char,
    pub num: RawNum,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Физическая строка файла (несколько блоков могут делить строку — EOB).
    pub line_no: u32,
    /// Текст фрагмента блока: точная вырезка (+ `;`; последний фрагмент строки
    /// получает терминатор строки `\n`) — RAW payload, ruling 5.
    pub raw: String,
    pub g_codes: Vec<GCode>,
    pub m_codes: Vec<MCode>,
    /// Слова-параметры словаря (F/I/J/K/R/S/X/Y/Z/P/Q/T/H/D/L/O/E/U/W/A/B/C).
    pub words: Vec<WordValue>,
    /// G/M-коды вне словаря этапа и вне raw-таблицы (generic-ведро) — сырьё
    /// для line-level Raw с диагностикой текста (молчаливый пропуск
    /// запрещён, COVERAGE §2.7).
    pub unsupported_words: Vec<WordValue>,
    pub comments: Vec<String>,
    pub marker: Option<Marker>,
    /// Macro B-детект: `#`-слово / `[ ]`-выражение / letter-run — весь блок
    /// line-level Raw{E_UNSUPPORTED_SYNTAX} (§4.3, COVERAGE §4).
    pub macro_b: bool,
    /// Два кода одной модальной группы в блоке (нарушение правила §1.1).
    pub modal_violation: Option<(ModalGroup, GCode)>,
}

impl Block {
    pub fn motion(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::Motion01)
    }
    pub fn plane(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::Plane02)
    }
    pub fn distance(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::Distance03)
    }
    pub fn arc_center(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::ArcCenter04)
    }
    pub fn feed_mode(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::FeedMode05)
    }
    pub fn units(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::Units06)
    }
    pub fn path(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::Path15)
    }

    fn first_modal(&self, group: ModalGroup) -> Option<GCode> {
        self.g_codes
            .iter()
            .copied()
            .find(|g| modal_group(*g) == Some(group))
    }

    /// Сырое число параметра по букве ('X', 'F', …).
    pub fn word(&self, letter: char) -> Option<&RawNum> {
        let up = letter.to_ascii_uppercase();
        self.words.iter().find(|w| w.letter == up).map(|w| &w.num)
    }
}

fn from_toks(bt: BlockToks, line_no: u32) -> Block {
    let mut b = Block {
        line_no,
        raw: bt.text,
        g_codes: Vec::new(),
        m_codes: Vec::new(),
        words: Vec::new(),
        unsupported_words: Vec::new(),
        comments: Vec::new(),
        marker: None,
        macro_b: false,
        modal_violation: None,
    };
    let mut seen: Vec<(ModalGroup, GCode)> = Vec::new();
    for tok in bt.toks {
        match tok {
            Tok::Word { letter, num } => match letter {
                'G' => match GCode::from_value(num.value) {
                    Some(g) => {
                        if let Some(group) = modal_group(g) {
                            if let Some(&(_, prev)) = seen.iter().find(|(gr, _)| *gr == group) {
                                if b.modal_violation.is_none() {
                                    b.modal_violation = Some((group, prev));
                                }
                            } else {
                                seen.push((group, g));
                            }
                        }
                        b.g_codes.push(g);
                    }
                    None => b.unsupported_words.push(WordValue { letter, num }),
                },
                'M' => match MCode::from_value(num.value) {
                    Some(m) => b.m_codes.push(m),
                    None => b.unsupported_words.push(WordValue { letter, num }),
                },
                _ => {
                    if WordKind::from_letter(letter).is_some() {
                        b.words.push(WordValue { letter, num });
                    } else {
                        b.unsupported_words.push(WordValue { letter, num });
                    }
                }
            },
            Tok::Comment(text) => b.comments.push(text),
            Tok::Percent => b.marker = Some(Marker::Percent),
            Tok::Slash => b.marker = Some(Marker::BlockDelete),
            Tok::MacroB => b.macro_b = true,
        }
    }
    b
}

/// Разбор файла целиком: физические строки → EOB-фрагменты → блоки.
/// Пустые блоки (нет токенов) — пропуск; их байты переносятся в payload
/// следующего блока той же строки (byte-exact reconstruction, ruling 5).
/// `%`-маркер — инертный файловый маркер (IGNORED_SAFE, §1.3): блока не
/// создаёт, чтобы не ломать структурные проверки порядка program_end
/// (trailing `%` после M30 — норма DNC-файлов).
/// Последний блок строки получает терминатор `\n` в payload (payload
/// конкатенации фрагментов = исходная строка включая терминатор).
pub fn parse_file(src: &str) -> ParsedFile {
    let mut out = ParsedFile {
        blocks: Vec::new(),
        lex_errors: Vec::new(),
    };
    let lines: Vec<&str> = src.split('\n').collect();
    for (idx, line) in lines.iter().enumerate() {
        let line_no = (idx + 1) as u32;
        // Есть ли `\n` после этой строки (split даёт хвостовой кусок после
        // завершающего \n — тогда Terminated = true для предыдущих строк).
        let terminated = idx + 1 < lines.len();
        match lexer::lex_line(line, line_no) {
            Err(e) => out.lex_errors.push(e),
            Ok(groups) => {
                let mut carry = String::new();
                let mut last_block: Option<usize> = None;
                for bt in groups {
                    if bt.toks.is_empty() {
                        carry.push_str(&bt.text); // пустой фрагмент — байты к следующему
                        continue;
                    }
                    // Лексер гарантирует: `%`-строка = ровно один токен Percent.
                    if matches!(bt.toks[0], Tok::Percent) && bt.toks.len() == 1 {
                        continue;
                    }
                    let mut raw = std::mem::take(&mut carry);
                    raw.push_str(&bt.text);
                    let bi = out.blocks.len();
                    out.blocks.push(from_toks(
                        BlockToks {
                            toks: bt.toks,
                            text: raw,
                        },
                        line_no,
                    ));
                    last_block = Some(bi);
                }
                // Byte-exact (ruling 5): байты после последнего блока строки
                // (напр. `\r` CRLF-терминатора, whitespace до `\n`) — к payload
                // последнего блока строки; терминатор `\n` — в конец.
                if let Some(bi) = last_block {
                    if !carry.is_empty() {
                        out.blocks[bi].raw.push_str(&carry);
                    }
                    if terminated {
                        out.blocks[bi].raw.push('\n');
                    }
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFile {
    pub blocks: Vec<Block>,
    pub lex_errors: Vec<LexError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_modal_state() {
        let b = &parse_file("G21 G90 G94 G17;\n").blocks[0];
        assert_eq!(b.units(), Some(GCode::UnitsMm));
        assert_eq!(b.distance(), Some(GCode::DistAbs));
        assert_eq!(b.feed_mode(), Some(GCode::FeedPerMin));
        assert_eq!(b.plane(), Some(GCode::PlaneXY));
        assert_eq!(b.modal_violation, None);
    }

    #[test]
    fn eob_two_blocks_one_line() {
        let f = parse_file("G0 X0; G1 X5.;\n");
        assert_eq!(f.lex_errors, vec![]);
        assert_eq!(f.blocks.len(), 2);
        assert_eq!(f.blocks[0].line_no, 1);
        assert_eq!(f.blocks[1].line_no, 1);
        assert_eq!(f.blocks[0].motion(), Some(GCode::Rapid));
        assert_eq!(f.blocks[1].motion(), Some(GCode::Linear));
        // ruling 5: payload = точная вырезка + `\n` на последнем фрагменте.
        assert_eq!(f.blocks[0].raw, "G0 X0;");
        assert_eq!(f.blocks[1].raw, " G1 X5.;\n");
    }

    #[test]
    fn empty_blocks_skipped_percent_ignored() {
        let f = parse_file(";;\nG21;\n%\n;\nG0 X0.;\n");
        assert_eq!(f.blocks.len(), 2); // пустые `;;`/`;` и `%`-маркер пропущены
        assert_eq!(f.blocks[0].g_codes, vec![GCode::UnitsMm]);
        assert_eq!(f.blocks[1].motion(), Some(GCode::Rapid));
    }

    #[test]
    fn empty_fragment_bytes_carry_to_next_block() {
        // `G0;; G1 X5` — пустой фрагмент `;` переносится в payload следующего.
        let f = parse_file("G0;; G1 X5.\n");
        assert_eq!(f.blocks.len(), 2);
        assert_eq!(f.blocks[0].raw, "G0;");
        assert_eq!(f.blocks[1].raw, "; G1 X5.\n");
        // Конкатенация payload'ов = исходная строка (ruling 5).
        assert_eq!(
            format!("{}{}", f.blocks[0].raw, f.blocks[1].raw),
            "G0;; G1 X5.\n"
        );
    }

    #[test]
    fn arc_words_and_comments() {
        let f = parse_file("(contour) N10 G2 X-5. Y5. I0. J5. F100. (cw);\n");
        assert_eq!(f.blocks.len(), 1);
        let b = &f.blocks[0];
        assert_eq!(b.motion(), Some(GCode::ArcCw));
        assert_eq!(b.comments.len(), 2);
        assert_eq!(b.word('I').map(|n| n.value), Some(0.0));
        assert_eq!(b.word('J').map(|n| n.value), Some(5.0));
        assert_eq!(b.word('F').map(|n| n.value), Some(100.0));
        assert_eq!(b.unsupported_words, vec![]);
    }

    #[test]
    fn stage1_words_and_gcodes() {
        // G70 — вне enum (raw-таблица → unsupported); G61/G64 — enum
        // (группа 15); T/P/Q/L — contextual-слова (не unsupported).
        // V — вне словаря Haas → unsupported.
        let f = parse_file("G70;\nG61;\nT5 P500 Q2 L3;\nM3;\nV9;\n");
        assert_eq!(f.blocks[0].unsupported_words.len(), 1);
        assert_eq!(f.blocks[0].unsupported_words[0].letter, 'G');
        assert_eq!(f.blocks[1].path(), Some(GCode::PathExactStop));
        assert_eq!(f.blocks[2].word('T').map(|n| n.value), Some(5.0));
        assert_eq!(f.blocks[2].word('P').map(|n| n.value), Some(500.0));
        assert_eq!(f.blocks[2].word('Q').map(|n| n.value), Some(2.0));
        assert_eq!(f.blocks[2].word('L').map(|n| n.value), Some(3.0));
        assert_eq!(f.blocks[3].m_codes, vec![MCode::SpindleCw]);
        // V — вне словаря Haas (не в Setting 162-домене) → unsupported.
        assert_eq!(f.blocks[4].unsupported_words[0].letter, 'V');
    }

    #[test]
    fn macro_b_flagged() {
        let f = parse_file("#100 = 5;\nIF [1 GT 2] GOTO 9;\nG1 X5. F100.;\n");
        assert!(f.blocks[0].macro_b);
        assert!(f.blocks[1].macro_b);
        assert!(!f.blocks[2].macro_b);
    }

    #[test]
    fn modal_violation_detected() {
        let b = &parse_file("G0 G1 X5.;\n").blocks[0];
        assert_eq!(
            b.modal_violation,
            Some((ModalGroup::Motion01, GCode::Rapid))
        );
        // Несколько групп в одном блоке легальны (§1.1).
        let b = &parse_file("G90 G17 G21 G94 G0 X0 Y0;\n").blocks[0];
        assert_eq!(b.modal_violation, None);
        // Два кода группы 15 — нарушение.
        let b = &parse_file("G61 G64;\n").blocks[0];
        assert_eq!(
            b.modal_violation,
            Some((ModalGroup::Path15, GCode::PathExactStop))
        );
    }

    #[test]
    fn m_codes_stage2_matrix() {
        let f = parse_file("M3;\nM30;\nM8;\nM9;\nM0;\nM1;\nM6;\nM16;\n");
        assert_eq!(f.blocks[0].m_codes, vec![MCode::SpindleCw]);
        assert_eq!(f.blocks[1].m_codes, vec![MCode::ProgramEnd]);
        assert_eq!(f.blocks[2].m_codes, vec![MCode::CoolantFlood]);
        assert_eq!(f.blocks[3].m_codes, vec![MCode::CoolantOff]);
        // Этап 2: M0/M1/M6/M16 — в словаре (диспозиции — канонизатор).
        assert_eq!(f.blocks[4].m_codes, vec![MCode::ProgramStop]);
        assert_eq!(f.blocks[5].m_codes, vec![MCode::OptionalStop]);
        assert_eq!(f.blocks[6].m_codes, vec![MCode::ToolChange]);
        assert_eq!(f.blocks[7].m_codes, vec![MCode::ToolChangeAlt]);
        // Вендорские M (этап 2) — тоже в словаре: M7-Shower (НЕ mist),
        // M13-тормоз 5-й оси (≠ Fanuc шпиндель+СОЖ), M29-relay (≠ rigid-tap).
        let f = parse_file("M7;\nM13;\nM29;\nM98;\n");
        assert_eq!(f.blocks[0].m_codes, vec![MCode::CoolantShower]);
        assert_eq!(f.blocks[1].m_codes, vec![MCode::Brake5Release]);
        assert_eq!(f.blocks[2].m_codes, vec![MCode::OutputRelay]);
        assert_eq!(f.blocks[3].m_codes, vec![MCode::SubCall]);
        // M64 — вне словаря Haas → generic-ведро (unsupported).
        let f = parse_file("M64;\n");
        assert!(f.blocks[0].m_codes.is_empty());
        assert_eq!(f.blocks[0].unsupported_words[0].letter, 'M');
    }

    #[test]
    fn bare_s_stored() {
        let f = parse_file("S5000;\n");
        // Голое S — поглощение (сохранение оборотов; §3 контракта).
        assert_eq!(f.blocks[0].word('S').map(|n| n.value), Some(5000.0));
        assert!(f.blocks[0].unsupported_words.is_empty());
    }

    #[test]
    fn dpi_flags_in_words() {
        // §1.2 ловушка №1: bare-integer (machinists notation) несёт
        // has_point=false; трейлинг-точка — true.
        let f = parse_file("G1 X25 Y50. F12;\n");
        let b = &f.blocks[0];
        assert_eq!(b.word('X').map(|n| n.has_point), Some(false));
        assert_eq!(b.word('Y').map(|n| n.has_point), Some(true));
        assert_eq!(b.word('F').map(|n| n.has_point), Some(false));
    }

    #[test]
    fn block_delete_marker() {
        let f = parse_file("/ G1 X5.;\nG0 X0. Y0.;\n");
        assert_eq!(f.blocks[0].marker, Some(Marker::BlockDelete));
        assert_eq!(f.blocks[1].marker, None);
    }

    #[test]
    fn payload_round_trip_single_line() {
        // ruling 5 golden (лексер-уровень): 2 фрагмента одной строки —
        // конкатенация payload + терминатор = исходная строка байт-в-байт.
        let src = "G43 H2; T5;\n";
        let f = parse_file(src);
        assert_eq!(f.blocks.len(), 2);
        let recon: String = f.blocks.iter().map(|b| b.raw.as_str()).collect();
        assert_eq!(recon, src);
    }

    #[test]
    fn space_in_word_lex_error() {
        // `?`-реестр: строгий отказ — ошибка доходит до ParsedFile.
        let f = parse_file("X 25;\n");
        assert_eq!(f.lex_errors.len(), 1);
        assert!(matches!(
            f.lex_errors[0].kind,
            crate::lexer::LexErrorKind::SpaceInWord { letter: 'X' }
        ));
    }
}
