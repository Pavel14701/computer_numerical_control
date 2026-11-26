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
// Фаза 12.4 (агент D3): L2-to-L2 диалектная эмиссия — ядро + Macro-B
// (Fanuc ↔ Haas) в `emit_l2`, O-word LinuxCNC (реструктуризация GOTO →
// o-if/do-while) в `emit_linuxcnc`. Точка входа: [`emit_l2::emit`].
pub mod emit_l2;
pub mod emit_linuxcnc;
pub mod eval;
pub mod interp;
pub mod kinematics;
// Фаза 12.5.4 (агент A): оставшиеся топологии кинематики (research
// `kinematics_research.md` §1 — все «РЕАЛИЗУЕМО СЕЙЧАС», оракулы LinuxCNC).
// Формулы — математические факты (источники GPL — цитируются в doc):
// * [`kinematics_tdr`] — table-table TT-2 «cradle/trunnion» XYZAB
//   (`xyzab_tdr_kins.comp`; квирк forward≠dx воспроизведён 1:1);
// * [`kinematics_trsrn`] — head-head универсальный (ν=90°) + nutating
//   (ν 0–90°) (`xyzacb_trsrn.comp` + независимый python-оракул kins-twp);
// * [`kinematics_maxkins`] — head-table intersecting, DMU 50-класс
//   (`maxkins.c`);
// * [`kinematics_5axis`] — gantry/bridge mill XYZBCW (`5axiskins.c`).
// Интеграция enum топологий — `eid-integration::profile::MachineKinematics`
// (12.5.4); lowering-проводка новых топологий (RtcpMode) — заморожена
// (СТОП-вопрос оркестратору), см. doc профиля.
pub mod kinematics_5axis;
pub mod kinematics_maxkins;
pub mod kinematics_tdr;
pub mod kinematics_trsrn;
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
/// * O-word-конструкции БЕЗ `#`-переменных (находка D2, 12.2): `o<имя>`
///   / `o<N>` + O-control keywords (`sub`/`endsub`/`while`/`endwhile`/
///   `if`/`else`/`endif`/`do`/`call`/`break`/`continue`) — иначе файлы
///   вида `O<sub> sub` / `o<showstate> call` (corpus m70_m72/m73/g20sub)
///   уходили в чистый L1-конвейер (нарушение T1-маршрутизации L2-файлов);
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
    // Пара «O-word + O-control keyword» (`o123 sub`, `O100 while ...`):
    // bare-ключевое слово без O-word впереди НЕ триггерит (SUB/CALL как
    // случайные слова чистого L1 — не L2).
    let mut prev_o_word = false;
    for tok in stripped.split_whitespace() {
        if l2_token_available(tok) {
            return true;
        }
        if prev_o_word && is_o_control_keyword(tok) {
            return true;
        }
        prev_o_word = is_o_word_token(tok);
    }
    false
}

/// O-word: `o<имя>` (LinuxCNC named) или `o123` (номерная форма).
fn is_o_word_token(tok: &str) -> bool {
    let b = tok.as_bytes();
    if b.is_empty() || (b[0] != b'o' && b[0] != b'O') {
        return false;
    }
    if b.len() >= 2 && b[1] == b'<' {
        return true; // o<name>
    }
    b.len() >= 2 && b[1..].iter().all(|c| c.is_ascii_digit()) // o123
}

/// Ключевые слова O-control (LinuxCNC o-codes); значимы только сразу
/// после O-word (см. [`l2_available`]).
fn is_o_control_keyword(tok: &str) -> bool {
    let up = tok.to_ascii_uppercase();
    matches!(
        up.as_str(),
        "SUB"
            | "ENDSUB"
            | "WHILE"
            | "ENDWHILE"
            | "IF"
            | "ELSE"
            | "ENDIF"
            | "DO"
            | "CALL"
            | "BREAK"
            | "CONTINUE"
    )
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
    // O-word с именем: `o<sub>` (LinuxCNC) — сама по себе L2-конструкция
    // (найдка D2: `O<showstate> sub` / `o<showstate> call` без `#`).
    if b.len() >= 2 && b[0] == b'O' && b[1] == b'<' {
        return true;
    }
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

    #[test]
    fn pin_o_word_constructs_without_hash_detected() {
        // Находка D2 (12.2): O-word-файлы без `#`-переменных (corpus
        // lcnc_m70_m72_demo.ngc / lcnc_m73_demo.ngc / lcnc_nc_files_g20sub.ngc)
        // обязаны идти в L2-конвейер.
        // m70_m72-паттерн (реальный корпус, с комментариями):
        assert!(l2_available(
            "; Demo M70 save / M72 restore around a subroutine call\n\
             O<showstate> sub\n\
             (DEBUG, imperial=#<_imperial>)\n\
             O<showstate> endsub\n\
             g21\n\
             o<showstate> call\n\
             M72\n\
             M30"
        ));
        // g20sub-паттерн (M70/M71 внутри O-подпрограммы):
        assert!(l2_available(
            "O<g20sub> sub\nM70 (save state)\ng20\nG0 X1.\nM71\nO<g20sub> endsub"
        ));
        // Номерная форма O-word + keyword (пара):
        assert!(l2_available("o123 sub\nM99\no123 endsub"));
        assert!(l2_available("o45 if [#1 lt 3] endif"));
        assert!(l2_available("O7 while [#100 EQ 1] endwhile"));
        // Именованный o<...> — сам по себе L2 (даже без keyword рядом).
        assert!(l2_available("o<repeat> call"));
        // Регистронезависимость: нижний регистр o<...> и keyword.
        assert!(l2_available("o<showstate> endwhile"));
    }

    #[test]
    fn o_word_regressions_stay_false() {
        // Голый O-номер программы (Fanuc) без control-keyword — НЕ L2.
        assert!(!l2_available("O1234\nG0 X0 Y0\nM30"));
        // Bare-keyword без O-word впереди — не триггер (пара обязательна).
        assert!(!l2_available("(call the sub when ready)\nG1 X10 F50\nM30"));
        assert!(!l2_available("G1 X10\nM30\n; sub while if endif call"));
        // N-слово с номером — не O-word (N100 sub не пары).
        assert!(!l2_available("N100 G1 X5. F100"));
    }
}
