//! eid-l2 — уровень L2 (параметрический G-код: макро-переменные, ветвления,
//! циклы, подпрограммы, трансформации) → канонический L1 (этап 12).
//!
//! Источник AST/семантики: DESIGN-док `EID-L2-DESIGN.md` v0.1 (§2.1–§2.4,
//! §3 матрица, §4 D0–D5, Приложение — семантика #0). Версии активных
//! спеков: SPEC-EID-L1 v0.20, interface v0.2.0, grammar v0.4.
//! Единственная зависимость — eid-core (path), std-only.
//!
//! Модули (фаза 12.1):
//! * [`ast`] — L2 AST (§2 дизайн-дока);
//! * [`eval`] — семантика #0 (таблицы 2.1–2.4), `L2Env`, `L2Error`;
//! * [`interp`] — интерпретатор D0–D2 (`L2Evaluator`) + минимальный
//!   Pass-1 lowering (`LoweringResult`);
//! * [`parser_fanuc`]/[`parser_haas`]/[`parser_linuxcnc`] — заготовки
//!   контрактов `parse_program` для агентов B/C/D;
//! * [`smt`] — заготовка фазы 12.3 (feature `smt`, кода за ней нет);
//! * [`lowering`] — заготовка фазы 12.2 (полный Pass-1).
//!
//! Маршрутизация T1 (zero-overhead): [`l2_available`] — дешёвый детект
//! L2-конструкций по тексту; false → чистый L1-конвейер без L2-фаз.

pub mod ast;
pub mod eval;
pub mod interp;
pub mod lowering;
pub mod parser_fanuc;
pub mod parser_haas;
pub mod parser_linuxcnc;
// Возвращено агентом B2 (файл завершён, собирается; комментарий агента A от
// временного выключения — см. git-историю: ошибки E0432/E0255/E0599/E0614/
// E0277/E0063 первой редакции устранены).
pub mod parser_macro_common;
pub mod smt;

// ===========================================================================
// l2_available — детект L2-конструкций (T1 zero-overhead маршрутизация)
// ===========================================================================

/// Эвристика «пахнет ли текст L2-конструкциями»:
/// * `#` — макро-переменные;
/// * слова `IF`/`WHILE`/`GOTO`;
/// * G5.x-параметрика (G5/G5.1/G5.2/G5.3 — сплайны) и макро-вызовы
///   G65/G66/G67;
/// * трансформации G68/G68.1/G68.2/G68.3, G51/G51.1/G50.1;
/// * RTCP G43.4/G43.5; skip-проход G31;
/// * ротационные/доп. оси: слова A/B/C/U/V/W с числом/скобкой.
///
/// False-positive допустим (программа уйдёт в L2-конвейер и вернётся в L1);
/// false-negative недопустим. Для чистого L1-текста (типовой golden-corpus)
/// функция — один проход байтов: сначала `#`, потом токены (T1: нулевая
/// цена маршрута). Комментарии (`(...)` и `;…`) вырезаются до анализа.
pub fn l2_available(src: &str) -> bool {
    let stripped = strip_comments(src);
    if stripped.as_bytes().contains(&b'#') {
        return true;
    }
    for tok in stripped.split_whitespace() {
        if l2_token_available(tok) {
            return true;
        }
    }
    false
}

/// Вырезать комментарии: `(...)` — скобочные, `;` — до конца строки.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_paren = false;
    let mut in_line = false;
    for ch in src.chars() {
        match ch {
            ';' if !in_paren => in_line = true,
            '(' if !in_line => in_paren = true,
            ')' => in_paren = false,
            '\n' => {
                in_paren = false; // скобочные комментарии не пересекают кадр
                in_line = false;
                out.push(ch);
            }
            c if !in_paren && !in_line => out.push(c),
            _ => {}
        }
    }
    out
}

/// Токен (кадровое слово) тянет L2?
fn l2_token_available(tok: &str) -> bool {
    let up = tok.to_ascii_uppercase();
    if matches!(
        up.as_str(),
        "IF" | "WHILE"
            | "GOTO"
            | "G65"
            | "G66"
            | "G67"
            | "G5"
            | "G5.1"
            | "G5.2"
            | "G5.3"
            | "G68"
            | "G68.1"
            | "G68.2"
            | "G68.3"
            | "G51"
            | "G51.1"
            | "G50.1"
            | "G43.4"
            | "G43.5"
            | "G31"
    ) {
        return true;
    }
    // Ротационные/доп. оси: A90., B[#..] — буква + число/[/-/+.
    let b = up.as_bytes();
    if b.len() >= 2 {
        let axis_word = matches!(b[0], b'A' | b'B' | b'C' | b'U' | b'V' | b'W');
        let value_start = matches!(b[1], b'0'..=b'9' | b'[' | b'-' | b'+');
        if axis_word && value_start {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::l2_available;

    #[test]
    fn pure_l1_text_is_false() {
        // T1-заготовка: L1-программа без L2-конструкций → прямой конвейер.
        assert!(!l2_available("G90 G54\nG0 X0 Y0\nG1 X10 Y10 F100.\nM30"));
        assert!(!l2_available(
            "(face mill)\nT1 M6\nS1200 M3\nG43 H1 Z5.\nM5 M9"
        ));
        assert!(!l2_available("N10 G1 X-1.5 Y+2 F50\nN20 G4 P1"));
    }

    #[test]
    fn l2_markers_detected() {
        assert!(l2_available("#100=5."));
        assert!(l2_available("G1 X#101 F100"));
        assert!(l2_available("IF [#1 GT 0] GOTO 10"));
        assert!(l2_available("WHILE [#2 LT 10] DO1\nEND1"));
        assert!(l2_available("G65 P1000 A5. B7."));
        assert!(l2_available("G68 X0 Y0 R45."));
        assert!(l2_available("G51 X0 Y0 P1.5"));
        assert!(l2_available("G43.4 H1"));
        assert!(l2_available("G31 X100 F50."));
        assert!(l2_available("G1 A90. B45."));
        assert!(l2_available("if [#1 gt 0] goto 10")); // регистронезависимость
    }

    #[test]
    fn comments_do_not_trigger() {
        // «IF» в комментарии — не L2.
        assert!(!l2_available("(IF NEEDED, RAISE Z)\nG0 Z10"));
        assert!(!l2_available("G0 X0 (axis B CHECK) Y0"));
        assert!(!l2_available("G1 X10 ; see #note in journal\nM30"));
        // Скобочный комментарий с # тоже не триггерит.
        assert!(!l2_available("G0 X0 (see drawing #12) Y0"));
    }

    #[test]
    fn x_axis_word_is_not_l2() {
        // X/Z — линейные оси, НЕ ротационные: не L2.
        assert!(!l2_available("G1 X10 Z-5. F100"));
    }
}
