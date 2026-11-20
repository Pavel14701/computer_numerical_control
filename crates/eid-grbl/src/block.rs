//! Блок (кадр) GRBL — результат разбора строки файла. Этап 1: lexing +
//! word-парсинг + модальные группы, БЕЗ канонизации в EID.
//! Модальные группы — grbl gcode.c (motion 01/02/03, plane, distance, feed,
//! units, wcs, control); G80/G4/G10/G28/G30/G53/G92 — немодальные/отмена,
//! группами не считаются. Нарушение группы записывается в modal_violation
//! (решение — диагностика на этапе 2).

use eid_core::ast::Num;
use crate::gcode::{GCode, MCode, WordKind};
use crate::lexer::{self, CommentStyle, LexError, Tok};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalGroup {
    Motion,     // G0/G1/G2/G3
    Plane,      // G17/G18/G19
    Distance,   // G90/G91
    FeedRate,   // G93/G94
    Units,      // G20/G21
    Wcs,        // G54..G59
    PathControl, // G61
}

/// Строковые маркеры (эп 2 решает диспозицию).
#[derive(Debug, Clone, PartialEq)]
pub enum Marker {
    ConfigLine(String), // $...=... → RAW(E_UNSUPPORTED_SYNTAX)
    BlockDelete,        // '/' в начале строки (Q3)
    Percent,            // '%' → IGNORED_SAFE
}

/// Слово: буква + verbatim-число (SPEC §10.1).
#[derive(Debug, Clone, PartialEq)]
pub struct WordValue {
    pub letter: char,
    pub value: Num,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub line_no: u32,
    /// Исходный текст строки (без \n; для RAW payload_hex, grammar D9).
    pub raw: String,
    pub g_codes: Vec<GCode>,
    pub m_codes: Vec<MCode>,
    /// Параметры кадра (F/I/J/K/L/P/R/S/T/X/Y/Z); для stage 2 — выписка опов.
    pub words: Vec<WordValue>,
    /// G/M вне словаря GRBL и буквы вне словаря (A/B/C/U/V/W/H/…) —
    /// сырьё для RAW-диагностик этапа 2.
    pub unsupported_words: Vec<WordValue>,
    pub comments: Vec<(CommentStyle, String)>,
    pub marker: Option<Marker>,
    /// Два кода одной модальной группы в кадре (нарушение grbl).
    pub modal_violation: Option<(ModalGroup, GCode)>,
}

/// G10 L2 → wcs_define, L20 → wcs_set_here (COVERAGE §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum G10Kind {
    Define,  // L2
    SetHere, // L20
}

impl Block {
    pub fn motion(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::Motion)
    }
    pub fn plane(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::Plane)
    }
    pub fn distance(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::Distance)
    }
    pub fn feed(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::FeedRate)
    }
    pub fn units(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::Units)
    }
    /// G54..G59 → 1..=6.
    pub fn wcs(&self) -> Option<u8> {
        match self.first_modal(ModalGroup::Wcs) {
            Some(GCode::Wcs(p)) => Some(p),
            _ => None,
        }
    }
    pub fn path_control(&self) -> Option<GCode> {
        self.first_modal(ModalGroup::PathControl)
    }

    fn first_modal(&self, group: ModalGroup) -> Option<GCode> {
        self.g_codes.iter().copied().find(|g| modal_group(*g) == Some(group))
    }

    /// Значение параметра по букве ('X', 'F', …).
    pub fn word(&self, letter: char) -> Option<&Num> {
        let up = letter.to_ascii_uppercase();
        self.words.iter().find(|w| w.letter == up).map(|w| &w.value)
    }

    /// G10 с L-словом: L2 → Define, L20 → SetHere; иначе None.
    pub fn g10_kind(&self) -> Option<G10Kind> {
        if !self.g_codes.contains(&GCode::SetCoordSystem) {
            return None;
        }
        match self.word('L').map(|n| n.value() as i64) {
            Some(2) => Some(G10Kind::Define),
            Some(20) => Some(G10Kind::SetHere),
            _ => None,
        }
    }

    fn from_toks(toks: Vec<Tok>, line_no: u32, raw: String) -> Block {
        let mut b = Block {
            line_no,
            raw,
            g_codes: Vec::new(),
            m_codes: Vec::new(),
            words: Vec::new(),
            unsupported_words: Vec::new(),
            comments: Vec::new(),
            marker: None,
            modal_violation: None,
        };
        let mut seen: Vec<(ModalGroup, GCode)> = Vec::new();
        for tok in toks {
            match tok {
                Tok::Word { letter, value } => match letter {
                    'G' => match GCode::from_value(value.value()) {
                        Some(g) => {
                            if let Some(group) = modal_group(g) {
                                if let Some(&(_, prev)) =
                                    seen.iter().find(|(gr, _)| *gr == group)
                                {
                                    if b.modal_violation.is_none() {
                                        b.modal_violation = Some((group, prev));
                                    }
                                } else {
                                    seen.push((group, g));
                                }
                            }
                            b.g_codes.push(g);
                        }
                        None => b.unsupported_words.push(WordValue { letter, value }),
                    },
                    'M' => match MCode::from_value(value.value()) {
                        Some(m) => b.m_codes.push(m),
                        None => b.unsupported_words.push(WordValue { letter, value }),
                    },
                    _ => {
                        if WordKind::from_letter(letter).is_some() {
                            b.words.push(WordValue { letter, value });
                        } else {
                            b.unsupported_words.push(WordValue { letter, value });
                        }
                    }
                },
                Tok::Comment { style, text } => b.comments.push((style, text)),
                Tok::BlockDelete => b.marker = Some(Marker::BlockDelete),
                Tok::ConfigLine(s) => b.marker = Some(Marker::ConfigLine(s)),
                Tok::Percent => b.marker = Some(Marker::Percent),
            }
        }
        b
    }
}

pub fn modal_group(g: GCode) -> Option<ModalGroup> {
    Some(match g {
        GCode::Rapid | GCode::Linear | GCode::ArcCw | GCode::ArcCcw => ModalGroup::Motion,
        GCode::PlaneXY | GCode::PlaneZX | GCode::PlaneYZ => ModalGroup::Plane,
        GCode::DistAbs | GCode::DistInc => ModalGroup::Distance,
        GCode::FeedInverse | GCode::FeedPerMin => ModalGroup::FeedRate,
        GCode::UnitsIn | GCode::UnitsMm => ModalGroup::Units,
        GCode::Wcs(_) => ModalGroup::Wcs,
        GCode::PathExact => ModalGroup::PathControl,
        _ => return None,
    })
}

/// Разбор файла целиком: блоки + лексические ошибки (порядок строк сохранён).
pub fn parse_file(src: &str) -> ParsedFile {
    let mut out = ParsedFile { blocks: Vec::new(), lex_errors: Vec::new() };
    for (idx, line) in src.split('\n').enumerate() {
        let line_no = (idx + 1) as u32;
        match lexer::lex_line(line, line_no) {
            Err(e) => out.lex_errors.push(e),
            Ok(toks) if !toks.is_empty() => {
                out.blocks.push(Block::from_toks(toks, line_no, line.trim_end_matches('\r').to_string()))
            }
            Ok(_) => {}
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
    fn pilot_header_modal_state() {
        let b = &parse_file("G21 G90 G94 G17\n").blocks[0];
        assert_eq!(b.units(), Some(GCode::UnitsMm));
        assert_eq!(b.distance(), Some(GCode::DistAbs));
        assert_eq!(b.feed(), Some(GCode::FeedPerMin));
        assert_eq!(b.plane(), Some(GCode::PlaneXY));
        assert_eq!(b.modal_violation, None);
    }

    #[test]
    fn pilot_01_file_structure() {
        let f = parse_file("G21 G90 G94 G17\nM3 S10000\nM8\nG0 X0.0000 Y0.0000 Z5.0000\nG1 X10.0000 F150.0\nM5\nM9\nM2\n");
        assert_eq!(f.lex_errors, vec![]);
        assert_eq!(f.blocks.len(), 8);
        let m3 = &f.blocks[1];
        assert_eq!(m3.m_codes, vec![MCode::SpindleCw]);
        assert_eq!(m3.word('S').map(|n| n.value()), Some(10000.0));
        assert_eq!(f.blocks[3].motion(), Some(GCode::Rapid));
        assert_eq!(f.blocks[4].motion(), Some(GCode::Linear));
        assert_eq!(f.blocks[4].word('F').map(|n| n.value()), Some(150.0));
        assert_eq!(f.blocks[7].m_codes, vec![MCode::ProgramEnd]);
    }

    #[test]
    fn g10_l20_and_l2() {
        let f = parse_file("G10 L20 P1 X0.0000 Y0.0000 Z0.0000\nG10 L2 P2 X1.0 Y2.0 Z3.0\n");
        assert_eq!(f.blocks[0].g10_kind(), Some(G10Kind::SetHere));
        assert_eq!(f.blocks[0].word('P').map(|n| n.value()), Some(1.0));
        assert_eq!(f.blocks[1].g10_kind(), Some(G10Kind::Define));
        assert_eq!(f.blocks[1].word('P').map(|n| n.value()), Some(2.0));
    }

    #[test]
    fn arc_r_words() {
        let b = &parse_file("G2 X0.0175 Y-9.9998 R10.0 F100.0\n").blocks[0];
        assert_eq!(b.motion(), Some(GCode::ArcCw));
        assert_eq!(b.word('R').map(|n| n.value()), Some(10.0));
        assert_eq!(b.unsupported_words, vec![]);
    }

    #[test]
    fn modal_violation_detected() {
        let b = &parse_file("G0 G1 X5\n").blocks[0];
        assert_eq!(b.modal_violation, Some((ModalGroup::Motion, GCode::Rapid)));
    }

    #[test]
    fn markers_and_out_of_dictionary() {
        let f = parse_file("$110=250.000\nG64 P0.01\nG43 H2\n%\n(hi) M2 ; tail\n");
        assert_eq!(f.blocks[0].marker, Some(Marker::ConfigLine("$110=250.000".into())));
        assert_eq!(f.blocks[1].unsupported_words, vec![WordValue { letter: 'G', value: Num::new("64".into(), 64.0) }]);
        assert_eq!(f.blocks[1].word('P').map(|n| n.value()), Some(0.01));
        assert_eq!(f.blocks[2].unsupported_words.len(), 2); // G43 + H
        assert_eq!(f.blocks[3].marker, Some(Marker::Percent));
        assert_eq!(f.blocks[4].comments.len(), 2);
        assert_eq!(f.blocks[4].m_codes, vec![MCode::ProgramEnd]);
    }

    #[test]
    fn pilot_46_and_negative_19_parse() {
        let f46 = parse_file("G21 G90 G94 G17 G91.1\nM3 S12000\nM8\nG0 X0.0000 Y0.0000 Z0.0000\nG2 X-5.0000 Y5.0000 Z2.0000 R5.0 F100.0\nM5\nM9\nM2\n");
        assert_eq!(f46.lex_errors, vec![]);
        assert_eq!(f46.blocks.len(), 8);
        assert!(f46.blocks[0].g_codes.contains(&GCode::ArcDistInc));
        // n19 (R-полный круг) на этапе 1 парсится; отказ — канонизатор (этап 2).
        let n19 = parse_file("G2 X0.0000 Y0.0000 R5.0 F100.0\n");
        assert_eq!(n19.blocks[0].motion(), Some(GCode::ArcCw));
    }

    /// Smoke по реальному корпусу: все GRBL-файлы пилотов и негативов лексически
    /// чисты (CRLF, G91.1, $-config n3, G10 L20 n15). Семантика — этап 2.
    #[test]
    fn corpus_grbl_files_lex_clean() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tier_a/v0");
        let mut checked = 0;
        let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&base)
            .expect("corpus dir")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        dirs.sort();
        for dir in dirs {
            for f in std::fs::read_dir(&dir).expect("dir").filter_map(|e| e.ok().map(|e| e.path())) {
                let name = f.file_name().unwrap().to_string_lossy().to_string();
                if !name.ends_with(".grbl.ngc") {
                    continue;
                }
                let src = std::fs::read_to_string(&f).expect("read");
                let parsed = parse_file(&src);
                assert!(parsed.lex_errors.is_empty(), "{name}: {:?}", parsed.lex_errors);
                assert!(!parsed.blocks.is_empty(), "{name}: нет блоков");
                checked += 1;
            }
        }
        assert!(checked >= 10, "слишком мало GRBL-файлов найдено: {checked}");
    }
}
