//! Блок (кадр) Fanuc — результат разбора файла. Этап 1: lexing + word-классификация
//! + проверка «один код на модальную группу», БЕЗ канонизации в EID.
//! EOB `;` делит физическую строку на блоки (lexer.rs); пустые блоки — пропуск
//! без диагностики (§1.3). Порядок слов в блоке произвольный (адресный язык,
//! §1.3) — парсер адресных слов, не позиционный.

use crate::lexer::{self, BlockToks, LexError, RawNum, Tok};
use crate::state::{modal_group, GCode, MCode, ModalGroup, WordKind};

/// Строковые маркеры (диспозиции — канонизатор).
#[derive(Debug, Clone, PartialEq)]
pub enum Marker {
    /// `%` — маркер начала/конца файла, IGNORED_SAFE (§1.3).
    Percent,
    /// `/` block delete → RAW(E_RUNTIME_STATE) (§1.3, как LinuxCNC).
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
    /// Текст фрагмента блока (между EOB `;`; для RAW payload — этап 2).
    pub raw: String,
    pub g_codes: Vec<GCode>,
    pub m_codes: Vec<MCode>,
    /// Слова-параметры словаря этапа 1 (F/I/J/K/R/S/X/Y/Z).
    pub words: Vec<WordValue>,
    /// Слова вне словаря этапа 1 (G/M вне словаря, P/Q/T/H/D/L/O/…) — сырьё
    /// для line-level Raw (молчаливый пропуск запрещён).
    pub unsupported_words: Vec<WordValue>,
    pub comments: Vec<String>,
    pub marker: Option<Marker>,
    /// Два кода одной модальной группы в блоке (нарушение Fanuc-правила, §1.1).
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

    fn first_modal(&self, group: ModalGroup) -> Option<GCode> {
        self.g_codes.iter().copied().find(|g| modal_group(*g) == Some(group))
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
        }
    }
    b
}

/// Разбор файла целиком: физические строки → EOB-фрагменты → блоки.
/// Пустые блоки (нет токенов и нет текста) — пропуск без диагностики (§1.3).
/// `%`-маркер — инертный файловый маркер (IGNORED_SAFE, §1.3): блока не
/// создаёт, чтобы не ломать структурные проверки порядка program_end
/// (trailing `%` после M30 — норма DNC-файлов).
/// Raw-байты строки с терминатором — вопрос RAW payload (этап 2); этап 1 хранит
/// фрагмент без `;`.
pub fn parse_file(src: &str) -> ParsedFile {
    let mut out = ParsedFile { blocks: Vec::new(), lex_errors: Vec::new() };
    for (idx, line) in src.split('\n').enumerate() {
        let line_no = (idx + 1) as u32;
        match lexer::lex_line(line, line_no) {
            Err(e) => out.lex_errors.push(e),
            Ok(groups) => {
                for bt in groups {
                    if bt.toks.is_empty() {
                        continue; // пустой блок — легален (§1.3)
                    }
                    // Лексер гарантирует: `%`-строка = ровно один токен Percent.
                    if matches!(bt.toks[0], Tok::Percent) && bt.toks.len() == 1 {
                        continue;
                    }
                    out.blocks.push(from_toks(bt, line_no));
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
        assert_eq!(f.blocks[1].raw, "G1 X5.");
    }

    #[test]
    fn empty_blocks_skipped_percent_ignored() {
        let f = parse_file(";;\nG21;\n%\n;\nG0 X0.;\n");
        assert_eq!(f.blocks.len(), 2); // пустые `;;`/`;` и `%`-маркер пропущены
        assert_eq!(f.blocks[0].g_codes, vec![GCode::UnitsMm]);
        assert_eq!(f.blocks[1].motion(), Some(GCode::Rapid));
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
    fn unsupported_words_out_of_dictionary() {
        // G43 + H — вне словаря этапа 1; G54 — этап 2; T — вне словаря.
        let f = parse_file("G43 H2;\nG54;\nT5;\nM3;\n");
        for b in &f.blocks {
            assert!(!b.unsupported_words.is_empty(), "raw-сырьё: {:?}", b.raw);
        }
    }

    #[test]
    fn modal_violation_detected() {
        let b = &parse_file("G0 G1 X5.;\n").blocks[0];
        assert_eq!(b.modal_violation, Some((ModalGroup::Motion01, GCode::Rapid)));
        // Несколько групп в одном блоке легальны (§1.1).
        let b = &parse_file("G90 G17 G21 G0 X0 Y0;\n").blocks[0];
        assert_eq!(b.modal_violation, None);
    }

    #[test]
    fn m30_and_bare_s() {
        let f = parse_file("M30;\nM2;\nS5000;\n");
        assert_eq!(f.blocks[0].m_codes, vec![MCode::ProgramEnd]);
        assert_eq!(f.blocks[1].m_codes, vec![MCode::ProgramEnd]);
        // Голое S — поглощение (сохранение оборотов; §3 отчёта).
        assert_eq!(f.blocks[2].word('S').map(|n| n.value), Some(5000.0));
        assert!(f.blocks[2].unsupported_words.is_empty());
    }

    #[test]
    fn dpi_flags_in_words() {
        let f = parse_file("G1 X25 Y50. F150;\n");
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
}
