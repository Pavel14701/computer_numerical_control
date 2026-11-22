//! Grammar-conformance test suite (итерация 5, адаптация урока TableGen):
//! парсер рукописный по grammar.ebnf v0.3 — этот файл систематически
//! детектирует дрейф «грамматика ↔ парсер»: КАЖДАЯ продукция покрыта
//! валидным примером (parse Ok + поле/структура верны) и ключевыми
//! невалидными вариантами (parse Err, точный код E_INVALID_INPUT,
//! Stage::Parse — лексика/структура).
//!
//! Точка входа — `parser::parse_raw` (ЧИСТАЯ лексика/структура, без
//! canonicalize/verify): conformance = соответствие grammar.ebnf, не
//! валидатору. Граница «грамматика vs валидатор» отдельно показана
//! boundary-тестами (лексически допустимо — валидатор отвергает).
//!
// ===========================================================================
// ИНВЕНТАРЬ ПРОДУКЦИЙ grammar.ebnf v0.3 → ТЕСТЫ (чек-лист покрытия;
// каждая строка = имя теста). Обычные комментарии (не doc): список-таблица
// декоративная, doc-list-линты не применимы.
// ===========================================================================
//   1. program (magic NL units NL path_mode NL [frame_preset] {op} END NL)
//        → program_shape, frame_preset_line_optional
//   2. magic / version                    → magic_valid, magic_invalid
//   3. units_line                         → units_valid, units_invalid
//   4. path_mode_line                     → path_mode_valid, path_mode_invalid,
//        blend [SP "t=" number]           → path_mode_blend_t_valid, path_mode_blend_t_invalid
//   5. frame_preset_line                  → frame_preset_none_valid, frame_preset_g92_valid,
//                                            frame_preset_invalid
//   6. g92_block / axis_word (D2: X,Y,Z)  → g92_axis_order_invalid
//   7. op_line / op_num (v0.4: 3+ цифры)  → op_num_valid, op_num_invalid
//   8. op_body → rapid_op                 → op_rapid_valid
//   9. op_body → linear_op                → op_linear_valid
//  10. op_body → arc_op (dir/sweep/pitch) → op_arc_valid, dir_invalid,
//                                            arc_sweep_lexical_boundary
//  11. op_body → dwell_op                 → op_dwell_valid, dwell_sign_lexical_boundary
//  12. op_body → tool_change_op           → op_tool_change_valid
//  13. op_body → stop_op                  → op_stop_valid
//  14. op_body → pend_op (без state, D6)  → op_pend_valid
//  15. op_body → wcs_define_op            → op_wcs_define_valid, p_domain_lexical_boundary
//  16. op_body → wcs_set_here_op          → op_wcs_set_here_valid
//  17. op_body → raw_op                   → op_raw_valid, raw_code_all_7_valid,
//                                            raw_code_invalid
//  18. start_f (= vec | "unknown", Q11)   → start_unknown_valid
//  19. vec (D2: XYZ позиционно)           → vec_xyz_mapping_valid, vec_shape_invalid
//  20. dir_f                              → (dir_invalid, п.10)
//  21. src_f / span / position            → src_placeholder_valid, src_full_valid,
//                                            src_invalid
//  22. sub_f (D8: только motion/process)  → sub_valid_on_motion_ops,
//                                            sub_invalid_on_non_motion
//  23. state (D3: полный снапшот, порядок)→ state_order_valid,
//                                            state_permuted_invalid,
//                                            state_missing_invalid,
//                                            state_extra_invalid
//  24. feed_f                             → state_values_valid
//  25. spindle_f                          → state_values_valid (off/cw/ccw)
//  26. coolant_f                          → coolant_all_valid
//  27. tool_f (integer | none)            → state_values_valid
//  28. frame_f                            → state_values_valid (wcs/machine)
//  29. wcs_f / wcs_name                   → wcs_names_valid, wcs_name_invalid
//  30. number (D1: 4dp, знак, без эксп.)  → number_sign_valid, number_invalid_lexemes,
//                                            number_leading_zero_grammar_ok
//  31. integer                            → (tool_change tool=, sub=, п.12/22)
//  32. hex_digit / payload (D9)           → raw_payload_valid, raw_payload_invalid
//  33. quoted (v0.4: экранирование)       → raw_reason_quoted_valid, raw_reason_invalid
// ===========================================================================

use eid_core::ast::*;
use eid_core::diag::{ErrorCode, Stage};
use eid_core::parser;

// ===========================================================================
// Хелперы
// ===========================================================================

const HDR: &str = "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET none\n";
const ST: &str = "feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54";

fn wrap(ops: &str) -> String {
    format!("{HDR}{ops}END\n")
}

/// Чистая лексика/структура (без валидатора).
fn parse_lex(text: &str) -> Result<Program, Vec<eid_core::diag::Diagnostic>> {
    parser::parse_raw(text, "conf")
}

fn ok_lex(text: &str, ctx: &str) -> Program {
    parse_lex(text)
        .unwrap_or_else(|ds| panic!("{ctx}: expected lexical Ok, got {ds:#?}\n---\n{text}"))
}

/// Все диагностики: E_INVALID_INPUT, Stage::Parse (лексика/структура).
fn err_lex(text: &str, ctx: &str) {
    match parse_lex(text) {
        Ok(_) => panic!("{ctx}: expected lexical Err, got Ok\n---\n{text}"),
        Err(ds) => {
            assert!(!ds.is_empty(), "{ctx}: empty diagnostics");
            assert!(
                ds.iter().all(|d| d.code == ErrorCode::InvalidInput),
                "{ctx}: expected all E_INVALID_INPUT, got {:#?}",
                ds.iter().map(|d| d.code).collect::<Vec<_>>()
            );
            assert!(
                ds.iter().all(|d| d.stage == Stage::Parse),
                "{ctx}: expected all Stage::Parse, got {:#?}",
                ds.iter().map(|d| d.stage).collect::<Vec<_>>()
            );
        }
    }
}

fn rapid_line(start: &str, tail: &str) -> String {
    format!("OP 001 RAPID start={start} end=1.0000,0.0000,0.0000 src=0.1-0.0 {tail}\nOP 002 PEND\n")
}

// ===========================================================================
// 1. program (общая форма)
// ===========================================================================

#[test]
fn program_shape() {
    let p = ok_lex(&wrap("OP 001 PEND\n"), "program shape");
    assert_eq!(p.eid_version, "EID/L1 v0.3");
    assert_eq!(p.units, Units::Mm);
    assert_eq!(p.path_mode, PathMode::ExactPath);
    assert!(p.frame_preset.is_none());
    assert_eq!(p.ops.len(), 1);
    // последняя строка без NL / мусор после END / пустая строка — нарушение формы
    err_lex(&format!("{HDR}OP 001 PEND\nEND"), "no final NL");
    err_lex(
        &format!("{HDR}OP 001 PEND\nEND\nOP 002 PEND\n"),
        "content after END",
    );
    err_lex(&format!("{HDR}\nOP 001 PEND\nEND\n"), "blank line");
    err_lex(
        &format!("{HDR}OP 001 PEND\n\nEND\n"),
        "blank line before END",
    );
}

#[test]
fn frame_preset_line_optional() {
    // grammar: [ frame_preset_line , NL ] — строка опциональна
    let text = "EID/L1 v0.3\nUNITS mm\nPATH_MODE blend\nOP 001 PEND\nEND\n";
    let p = ok_lex(text, "no frame_preset line");
    assert!(p.frame_preset.is_none());
}

// ===========================================================================
// 2. magic / version
// ===========================================================================

#[test]
fn magic_valid() {
    // ровно "EID/L1 v0.3": magic несёт версию ФОРМАТА (SPEC §10.4)
    let p = ok_lex(&wrap("OP 001 PEND\n"), "magic");
    assert_eq!(p.eid_version, "EID/L1 v0.3");
}

#[test]
fn magic_invalid() {
    for bad in [
        "EID/L1 v0.2\n",  // прежняя версия формата
        "EID/L1 v0.4\n",  // будущая
        "EID/L1 v1.0\n",  // version-продукция допускает, но формат зафиксирован
        "EID/L1 v03\n",   // нет точки
        "EID/L1\n",       // нет версии
        "EID/L1  v0.3\n", // двойной SP
        "eid/L1 v0.3\n",  // регистр
        "EID/L2 v0.3\n",
        "",
    ] {
        let text =
            format!("{bad}UNITS mm\nPATH_MODE exact_path\nFRAME_PRESET none\nOP 001 PEND\nEND\n");
        err_lex(&text, "bad magic");
    }
}

// ===========================================================================
// 3. units_line
// ===========================================================================

#[test]
fn units_valid() {
    for (u, want) in [("UNITS mm", Units::Mm), ("UNITS in", Units::In)] {
        let text = format!(
            "EID/L1 v0.3\n{u}\nPATH_MODE exact_path\nFRAME_PRESET none\nOP 001 PEND\nEND\n"
        );
        let p = ok_lex(&text, "units");
        assert_eq!(p.units, want, "{u}");
    }
}

#[test]
fn units_invalid() {
    for bad in [
        "UNITS MM",
        "UNITS cm",
        "UNITSmm",
        "UNITS mm ",
        "UNITS",
        "units mm",
    ] {
        let text = format!(
            "EID/L1 v0.3\n{bad}\nPATH_MODE exact_path\nFRAME_PRESET none\nOP 001 PEND\nEND\n"
        );
        err_lex(&text, "bad units");
    }
}

// ===========================================================================
// 4. path_mode_line (+ blend t=)
// ===========================================================================

#[test]
fn path_mode_valid() {
    for (pm, want) in [
        ("exact_path", PathMode::ExactPath),
        ("exact_stop", PathMode::ExactStop),
        ("blend", PathMode::Blend { t: None }),
    ] {
        let text =
            format!("EID/L1 v0.3\nUNITS mm\nPATH_MODE {pm}\nFRAME_PRESET none\nOP 001 PEND\nEND\n");
        let p = ok_lex(&text, "path_mode");
        assert_eq!(p.path_mode, want, "{pm}");
    }
}

#[test]
fn path_mode_blend_t_valid() {
    for (t, want) in [("t=0.0100", 0.01), ("t=-0.5000", -0.5)] {
        let text = format!(
            "EID/L1 v0.3\nUNITS mm\nPATH_MODE blend {t}\nFRAME_PRESET none\nOP 001 PEND\nEND\n"
        );
        let p = ok_lex(&text, "blend t=");
        match p.path_mode {
            PathMode::Blend { t: Some(n) } => assert_eq!(n.value, want, "{t}"),
            other => panic!("blend {t}: expected Blend{{Some}}, got {other:?}"),
        }
    }
}

#[test]
fn path_mode_blend_t_invalid() {
    for bad in [
        "blend t=0.500",        // 3 dp (D1)
        "blend t=0.50000",      // 5 dp
        "blend t=",             // пустое значение
        "blend t=1.0000 extra", // мусор после числа
        "blendt=0.5000",        // нет SP
        "blend  t=0.5000",      // двойной SP
        "blend T=0.5000",       // регистр
        "blend 0.5000",         // нет t=
    ] {
        let text = format!(
            "EID/L1 v0.3\nUNITS mm\nPATH_MODE {bad}\nFRAME_PRESET none\nOP 001 PEND\nEND\n"
        );
        err_lex(&text, "bad blend t=");
    }
}

#[test]
fn path_mode_invalid() {
    for bad in ["exact_path extra", "exact", "Exact_Path", "G61", ""] {
        let text = format!(
            "EID/L1 v0.3\nUNITS mm\nPATH_MODE {bad}\nFRAME_PRESET none\nOP 001 PEND\nEND\n"
        );
        err_lex(&text, "bad path_mode");
    }
}

// ===========================================================================
// 5-6. frame_preset_line / g92_block / axis_word (D2)
// ===========================================================================

#[test]
fn frame_preset_none_valid() {
    let p = ok_lex(&wrap("OP 001 PEND\n"), "frame_preset none");
    assert!(p.frame_preset.is_none());
}

#[test]
fn frame_preset_g92_valid() {
    let text = "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET G92 X1.0000 Y-2.5000 Z5.0000 WCS G59.3\nOP 001 PEND\nEND\n";
    let p = ok_lex(text, "G92 block");
    let pr = p.frame_preset.expect("preset present");
    assert_eq!(pr.values.x.value, 1.0); // D2: X, Y, Z — позиционно
    assert_eq!(pr.values.y.value, -2.5);
    assert_eq!(pr.values.z.value, 5.0);
    assert_eq!(pr.wcs, Wcs::G59_3);
    // знак числа в axis_word
    let text = "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET G92 X-1.0000 Y0.0000 Z0.0000 WCS G54\nOP 001 PEND\nEND\n";
    assert!(parse_lex(text).is_ok());
}

#[test]
fn g92_axis_order_invalid() {
    // D2: порядок X, Y, Z строгий
    for bad in [
        "G92 Y0.0000 X1.0000 Z3.0000 WCS G54", // перестановка
        "G92 X1.0000 Z3.0000 Y2.0000 WCS G54",
        "G92 X1.0000 Y2.0000 WCS G54",                 // только 2 оси
        "G92 X1.0000 Y2.0000 Z3.0000 W4.0000 WCS G54", // лишняя ось
        "G92 X1.0000 Y2.0000 Z3.0000 G54",             // нет WCS
        "G92 X1.0000 Y2.0000 Z3.0000 WCS G99",         // плохое имя
        "g92 X1.0000 Y2.0000 Z3.0000 WCS G54",         // регистр
        "G92 1.0000 2.0000 3.0000 WCS G54",            // нет букв осей
    ] {
        let text = format!(
            "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET {bad}\nOP 001 PEND\nEND\n"
        );
        err_lex(&text, "bad G92 block");
    }
}

#[test]
fn frame_preset_invalid() {
    for bad in [
        "none extra",
        "None",
        "G92",
        "g92 X0.0000 Y0.0000 Z0.0000 WCS G54",
    ] {
        let text = format!(
            "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET {bad}\nOP 001 PEND\nEND\n"
        );
        err_lex(&text, "bad frame_preset");
    }
}

// ===========================================================================
// 7. op_line / op_num
// ===========================================================================

#[test]
fn op_num_valid() {
    // zero-padded: 001…, минимум 3 цифры; 1000+ легальны (grammar v0.4 — аудит #3)
    for n in [1, 42, 999, 1000, 1234] {
        let text = wrap(&format!("OP {n:03} PEND\n"));
        let p = ok_lex(&text, "op_num");
        assert_eq!(p.ops.len(), 1);
    }
    // zero-pad min 3 сохраняется эмиттером ({n:03})
    let text = wrap("OP 007 PEND\n");
    ok_lex(&text, "op_num zero-pad");
}

#[test]
fn op_num_invalid() {
    // меньше 3 цифр / нецифры
    for bad in ["1", "01", "12a", "abc", "-01", "  1"] {
        let text = wrap(&format!("OP {bad} PEND\n"));
        err_lex(&text, "bad op_num");
    }
    // нет "OP " префикса
    err_lex(&wrap("001 PEND\n"), "no OP prefix");
    err_lex(&wrap("PEND\n"), "bare PEND");
}

// ===========================================================================
// 8-17. op_body: все 10 видов
// ===========================================================================

#[test]
fn op_rapid_valid() {
    let p = ok_lex(&wrap(&rapid_line("unknown", ST)), "rapid");
    assert_eq!(p.ops.len(), 2);
    match &p.ops[0] {
        Op::Rapid {
            start,
            end,
            state,
            src,
            sub,
        } => {
            assert!(matches!(start, StartPoint::Unknown));
            assert_eq!(end.x.value, 1.0);
            assert!(state.feed.is_none());
            assert_eq!(state.wcs, Wcs::G54);
            assert_eq!(
                (src.line_start, src.col_start, src.line_end, src.col_end),
                (0, 1, 0, 0)
            );
            assert!(sub.is_none());
        }
        other => panic!("expected Rapid, got {other:?}"),
    }
}

#[test]
fn op_linear_valid() {
    let text = wrap(
        "OP 001 LINEAR start=0.0000,0.0000,5.0000 end=10.0000,0.0000,5.0000 src=3.1-3.44 feed=150.0000 spindle=cw,10000.0000 coolant=flood tool=none frame=wcs wcs=G54\nOP 002 PEND\n",
    );
    let p = ok_lex(&text, "linear");
    match &p.ops[0] {
        Op::Linear {
            start,
            end,
            state,
            sub,
            ..
        } => {
            assert!(matches!(start, StartPoint::Known(s) if s.x.value == 0.0));
            assert_eq!(end.x.value, 10.0);
            assert_eq!(state.feed.as_ref().unwrap().value, 150.0);
            assert_eq!(
                state.spindle,
                Spindle::Cw(Num::new("10000.0000".into(), 10000.0))
            );
            assert!(state.coolant.flood && !state.coolant.mist);
            assert!(sub.is_none());
        }
        other => panic!("expected Linear, got {other:?}"),
    }
}

#[test]
fn op_arc_valid() {
    let text = wrap(
        "OP 001 ARC start=0.0000,0.0000,0.0000 end=-5.0000,5.0000,2.0000 center=0.0000,5.0000,0.0000 normal=0.0000,0.0000,1.0000 dir=ccw sweep=179.9000 pitch=-2.0000 src=0.1-0.0 sub=3 feed=100.0000 spindle=ccw,1.0000 coolant=flood+mist tool=none frame=machine wcs=G59\nOP 002 PEND\n",
    );
    let p = ok_lex(&text, "arc");
    match &p.ops[0] {
        Op::Arc {
            start,
            end,
            center,
            normal,
            dir,
            sweep_deg,
            pitch,
            state,
            sub,
            ..
        } => {
            assert!(matches!(start, StartPoint::Known(_)));
            assert_eq!(end.x.value, -5.0);
            assert_eq!(end.z.value, 2.0);
            assert_eq!(center.y.value, 5.0);
            assert_eq!(normal.z.value, 1.0);
            assert_eq!(*dir, Dir::Ccw);
            assert_eq!(sweep_deg.value, 179.9);
            assert_eq!(pitch.value, -2.0); // знак pitch по D1
            assert_eq!(*sub, Some(3));
            assert!(matches!(state.spindle, Spindle::Ccw(_)));
            assert!(state.coolant.flood && state.coolant.mist);
            assert_eq!(state.frame, Frame::Machine);
            assert_eq!(state.wcs, Wcs::G59);
        }
        other => panic!("expected Arc, got {other:?}"),
    }
}

#[test]
fn dir_invalid() {
    let mk = |d: &str| {
        wrap(&format!(
            "OP 001 ARC start=0.0000,0.0000,0.0000 end=-5.0000,5.0000,2.0000 center=0.0000,5.0000,0.0000 normal=0.0000,0.0000,1.0000 dir={d} sweep=90.0000 pitch=0.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
        ))
    };
    for bad in ["CW", "ccw ", "clockwise", ""] {
        // dir= пустое / с мусором —Lexical err; собираем полную строку с dir={bad}
        let text = mk(bad);
        err_lex(&text, "bad dir");
    }
}

#[test]
fn arc_sweep_lexical_boundary() {
    // sweep=400.0000 ЛЕКСИЧЕСКИ валиден (number) — домен (0°, 360°] это
    // D7/валидатор, не grammar: parse_raw Ok, verify — Err.
    let text = wrap(
        "OP 001 ARC start=0.0000,0.0000,0.0000 end=0.0000,0.0000,0.0000 center=5.0000,0.0000,0.0000 normal=0.0000,0.0000,1.0000 dir=cw sweep=400.0000 pitch=0.0000 src=0.1-0.0 feed=1.0000 spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 PEND\n",
    );
    assert!(
        parse_lex(&text).is_ok(),
        "sweep=400 must be lexically valid"
    );
}

#[test]
fn op_dwell_valid() {
    let text = wrap(
        "OP 001 DWELL seconds=0.5000 src=4.1-4.30 sub=1 feed=120.0000 spindle=off coolant=mist tool=7 frame=wcs wcs=G55\nOP 002 PEND\n",
    );
    let p = ok_lex(&text, "dwell");
    match &p.ops[0] {
        Op::Dwell {
            seconds,
            state,
            sub,
            ..
        } => {
            assert_eq!(seconds.value, 0.5);
            assert_eq!(seconds.text, "0.5000");
            assert!(state.coolant.mist);
            assert_eq!(state.tool, Some(7));
            assert_eq!(state.wcs, Wcs::G55);
            assert_eq!(*sub, Some(1));
        }
        other => panic!("expected Dwell, got {other:?}"),
    }
}

#[test]
fn dwell_sign_lexical_boundary() {
    // seconds=-1.0000 — number со знаком лексически валиден; dwell < 0 —
    // D7/валидатор (SPEC §5.2), не grammar.
    let text = wrap(&format!(
        "OP 001 DWELL seconds=-1.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    assert!(
        parse_lex(&text).is_ok(),
        "negative seconds must be lexically valid"
    );
    // а "0.5" (3dp / 2dp) — лексическая ошибка в любом случае
    let bad = wrap(&format!(
        "OP 001 DWELL seconds=0.5 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    err_lex(&bad, "dwell 2dp");
}

#[test]
fn op_tool_change_valid() {
    let text = wrap(
        "OP 001 TOOL_CHANGE tool=42 src=2.1-2.24 feed=none spindle=off coolant=none tool=42 frame=wcs wcs=G54\nOP 002 PEND\n",
    );
    let p = ok_lex(&text, "tool_change");
    match &p.ops[0] {
        Op::ToolChange { tool, state, .. } => {
            assert_eq!(*tool, 42);
            assert_eq!(state.tool, Some(42));
        }
        other => panic!("expected ToolChange, got {other:?}"),
    }
}

#[test]
fn op_stop_valid() {
    for (opt, want) in [("yes", true), ("no", false)] {
        let text = wrap(&format!(
            "OP 001 STOP optional={opt} src=0.1-0.0 {ST}\nOP 002 PEND\n"
        ));
        let p = ok_lex(&text, "stop");
        match &p.ops[0] {
            Op::ProgramStop { optional, .. } => assert_eq!(*optional, want, "{opt}"),
            other => panic!("expected ProgramStop, got {other:?}"),
        }
    }
    // невалидное значение optional
    let bad = wrap(&format!(
        "OP 001 STOP optional=maybe src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    err_lex(&bad, "bad optional");
    let bad2 = wrap(&format!("OP 001 STOP src=0.1-0.0 {ST}\nOP 002 PEND\n"));
    err_lex(&bad2, "missing optional");
}

#[test]
fn op_pend_valid() {
    // pend_op = "PEND" — ровно, без state и без полей (D6)
    let p = ok_lex(&wrap("OP 001 PEND\n"), "pend");
    assert!(matches!(p.ops[0], Op::ProgramEnd { .. }));
    let bad = wrap("OP 001 PEND src=0.1-0.0\n");
    err_lex(&bad, "pend with extra field");
    let bad2 = wrap(&format!("OP 001 PEND {ST}\n"));
    err_lex(&bad2, "pend with state");
}

#[test]
fn op_wcs_define_valid() {
    let text = wrap(&format!(
        "OP 001 WCS_DEFINE p=9 origin=-10.0000,0.0000,3.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    let p = ok_lex(&text, "wcs_define");
    match &p.ops[0] {
        Op::WcsDefine { p, origin, .. } => {
            assert_eq!(*p, 9);
            assert_eq!(origin.x.value, -10.0);
            assert_eq!(origin.z.value, 3.0);
        }
        other => panic!("expected WcsDefine, got {other:?}"),
    }
    // p=1 (нижняя граница домена)
    let text = wrap(&format!(
        "OP 001 WCS_DEFINE p=1 origin=0.0000,0.0000,0.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    assert!(parse_lex(&text).is_ok());
}

#[test]
fn p_domain_lexical_boundary() {
    // grammar: p_f = "p=" digit — ЛЕКСИЧЕСКИ одна цифра; домен 1..9 — D7/
    // валидатор. p=0 и p=10 лексически: p=0 валиден (цифра), p=10 — две
    // цифры → лексическая ошибка.
    let p0 = wrap(&format!(
        "OP 001 WCS_DEFINE p=0 origin=0.0000,0.0000,0.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    assert!(
        parse_lex(&p0).is_ok(),
        "p=0 must be lexically valid (digit)"
    );
    let p10 = wrap(&format!(
        "OP 001 WCS_DEFINE p=10 origin=0.0000,0.0000,0.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    err_lex(&p10, "p=10 is two digits");
    // нет p=
    let nop = wrap(&format!(
        "OP 001 WCS_DEFINE 1 origin=0.0000,0.0000,0.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    err_lex(&nop, "missing p=");
}

#[test]
fn op_wcs_set_here_valid() {
    let text = wrap(&format!(
        "OP 001 WCS_SET_HERE p=2 delta=1.0000,0.0000,0.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    let p = ok_lex(&text, "wcs_set_here");
    match &p.ops[0] {
        Op::WcsSetHere { p, delta, .. } => {
            assert_eq!(*p, 2);
            assert_eq!(delta.x.value, 1.0);
        }
        other => panic!("expected WcsSetHere, got {other:?}"),
    }
    // delta= (не origin=) — поля не взаимозаменяемы
    let bad = wrap(&format!(
        "OP 001 WCS_SET_HERE p=2 origin=1.0000,0.0000,0.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    err_lex(&bad, "set_here with origin=");
}

// ===========================================================================
// 17. raw_op: reason_f / code_f / payload_f (D9)
// ===========================================================================

#[test]
fn op_raw_valid() {
    // D9 v0.3: payload — hex исходных байтов ВКЛЮЧАЯ line terminator
    // (0D0A = CRLF-терминатор в payload — валидно)
    let text = wrap("OP 001 RAW reason=\"vendor macro block\" code=E_UNSUPPORTED_SEMANTICS src=47.1-49.0 payload=0D0A24240D0A\nOP 002 PEND\n");
    let p = ok_lex(&text, "raw");
    match &p.ops[0] {
        Op::Raw {
            payload_hex,
            reason,
            code,
            src,
        } => {
            assert_eq!(payload_hex, "0D0A24240D0A");
            assert_eq!(reason, "vendor macro block");
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert_eq!(
                (src.line_start, src.col_start, src.line_end, src.col_end),
                (47, 1, 49, 0)
            );
        }
        other => panic!("expected Raw, got {other:?}"),
    }
}

#[test]
fn raw_code_all_7_valid() {
    for (code, want) in [
        ("E_INVALID_INPUT", ErrorCode::InvalidInput),
        ("E_UNSUPPORTED_SYNTAX", ErrorCode::UnsupportedSyntax),
        ("E_UNSUPPORTED_SEMANTICS", ErrorCode::UnsupportedSemantics),
        ("E_REQUIRES_MACHINE_TABLE", ErrorCode::RequiresMachineTable),
        ("E_RUNTIME_STATE", ErrorCode::RuntimeState),
        ("E_LIMIT_EXCEEDED", ErrorCode::LimitExceeded),
        ("E_RAW_UNTRANSLATABLE", ErrorCode::RawUntranslatable),
    ] {
        let text = wrap(&format!(
            "OP 001 RAW reason=\"x\" code={code} src=0.1-0.0 payload=00\nOP 002 PEND\n"
        ));
        let p = ok_lex(&text, "raw code");
        match &p.ops[0] {
            Op::Raw { code: got, .. } => assert_eq!(*got, want, "{code}"),
            other => panic!("expected Raw, got {other:?}"),
        }
    }
}

#[test]
fn raw_code_invalid() {
    for bad in [
        "E_NOPE",
        "e_invalid_input",
        "E_INVALID",
        "INVALID_INPUT",
        "",
    ] {
        let text = wrap(&format!(
            "OP 001 RAW reason=\"x\" code={bad} src=0.1-0.0 payload=00\nOP 002 PEND\n"
        ));
        err_lex(&text, "bad raw code");
    }
}

#[test]
fn raw_payload_valid() {
    // { hex_digit } — включая пустой payload (нуль повторов грамматичен)
    for pl in ["00", "0D0A", "DEADBEEF", ""] {
        let text = wrap(&format!(
            "OP 001 RAW reason=\"x\" code=E_INVALID_INPUT src=0.1-0.0 payload={pl}\nOP 002 PEND\n"
        ));
        let p = ok_lex(&text, "raw payload");
        match &p.ops[0] {
            Op::Raw { payload_hex, .. } => assert_eq!(payload_hex, pl, "payload {pl:?}"),
            other => panic!("expected Raw, got {other:?}"),
        }
    }
}

#[test]
fn raw_payload_invalid() {
    // чётная длина (D9); hex_digit = 0-9 | A-F (uppercase only)
    for bad in ["0", "0D0A1", "0d0a", "0g", "ZZ", "0x0D", " 0D"] {
        let text = wrap(&format!(
            "OP 001 RAW reason=\"x\" code=E_INVALID_INPUT src=0.1-0.0 payload={bad}\nOP 002 PEND\n"
        ));
        err_lex(&text, "bad raw payload");
    }
    // нет payload= вовсе
    let miss = wrap("OP 001 RAW reason=\"x\" code=E_INVALID_INPUT src=0.1-0.0\nOP 002 PEND\n");
    err_lex(&miss, "missing payload");
}

#[test]
fn raw_reason_quoted_valid() {
    // quoted = '"' , { escaped | char_without_quote_or_nl } '"' — пробелы
    // внутри легальны; строки без обратного слэша декодируются как в v0.3
    for r in ["vendor macro", "", "G65 call #1 (see COVERAGE)", "0D0A"] {
        let text = wrap(&format!(
            "OP 001 RAW reason=\"{r}\" code=E_INVALID_INPUT src=0.1-0.0 payload=00\nOP 002 PEND\n"
        ));
        let p = ok_lex(&text, "raw reason");
        match &p.ops[0] {
            Op::Raw { reason, .. } => assert_eq!(reason, r, "reason {r:?}"),
            other => panic!("expected Raw, got {other:?}"),
        }
    }
    // grammar v0.4 (аудит #12): экранирование \" → '"' и \\ → '\'
    let cases = [
        ("say \\\"hi\\\"", "say \"hi\""),
        ("C:\\\\path", "C:\\path"),
        ("end\\\\", "end\\"),
        ("\\\"\\\"", "\"\""),
    ];
    for (src, decoded) in cases {
        let text = wrap(&format!("OP 001 RAW reason=\"{src}\" code=E_INVALID_INPUT src=0.1-0.0 payload=00\nOP 002 PEND\n"));
        let p = ok_lex(&text, "escaped reason");
        match &p.ops[0] {
            Op::Raw { reason, .. } => assert_eq!(reason, decoded, "escape {src:?}"),
            other => panic!("expected Raw, got {other:?}"),
        }
    }
}

#[test]
fn raw_reason_invalid() {
    // непарная кавычка-текст: `"` внутри; незакрытая; NL внутри невозможен
    // одной строкой, но кавычка-чек проверяем
    for r in ["bad\"quote", "unterminated", "\""] {
        let text = wrap(&format!(
            "OP 001 RAW reason={r} code=E_INVALID_INPUT src=0.1-0.0 payload=00\nOP 002 PEND\n"
        ));
        err_lex(&text, "bad raw reason");
    }
    // grammar v0.4: `\` не перед `"`/`\` — невалидный escape (каноничность:
    // у одного AST одна текстовая форма)
    for r in ["a\\zb", "a\\", "\\ x", "x\\"] {
        let text = wrap(&format!(
            "OP 001 RAW reason=\"{r}\" code=E_INVALID_INPUT src=0.1-0.0 payload=00\nOP 002 PEND\n"
        ));
        err_lex(&text, "invalid escape");
    }
    // нет reason=
    let miss = wrap("OP 001 RAW code=E_INVALID_INPUT src=0.1-0.0 payload=00\nOP 002 PEND\n");
    err_lex(&miss, "missing reason");
    // double SP внутри quoted — легален (это содержимое строки)
    let inner = wrap(
        "OP 001 RAW reason=\"a  b\" code=E_INVALID_INPUT src=0.1-0.0 payload=00\nOP 002 PEND\n",
    );
    assert!(
        parse_lex(&inner).is_ok(),
        "spaces inside quoted are content"
    );
}

// ===========================================================================
// 18-19. start_f / vec (D2)
// ===========================================================================

#[test]
fn start_unknown_valid() {
    // start_f = "start=" , ( vec | "unknown" ) — unknown (SPEC §2, Q11)
    let p = ok_lex(&wrap(&rapid_line("unknown", ST)), "start unknown");
    match &p.ops[0] {
        Op::Rapid { start, .. } => assert!(matches!(start, StartPoint::Unknown)),
        _ => unreachable!(),
    }
    // known-форма тоже валидна
    let p = ok_lex(
        &wrap(&rapid_line("-1.5000,2.0000,3.0000", ST)),
        "start known",
    );
    match &p.ops[0] {
        Op::Rapid { start, .. } => match start {
            StartPoint::Known(k) => assert_eq!(k.x.value, -1.5),
            _ => panic!("expected Known"),
        },
        _ => unreachable!(),
    }
}

#[test]
fn vec_xyz_mapping_valid() {
    // D2: XYZ — позиционное отображение vec на x, y, z AST
    let text = wrap(&format!(
        "OP 001 RAPID start=10.0000,20.0000,-30.0000 end=1.0000,2.0000,3.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    let p = ok_lex(&text, "vec mapping");
    match &p.ops[0] {
        Op::Rapid { start, end, .. } => {
            match start {
                StartPoint::Known(s) => {
                    assert_eq!((s.x.value, s.y.value, s.z.value), (10.0, 20.0, -30.0))
                }
                StartPoint::Unknown => panic!("expected Known start"),
            }
            assert_eq!((end.x.value, end.y.value, end.z.value), (1.0, 2.0, 3.0));
        }
        _ => unreachable!(),
    }
}

#[test]
fn vec_shape_invalid() {
    for bad in [
        "1.0000,2.0000",               // 2 компоненты
        "1.0000,2.0000,3.0000,4.0000", // 4
        "1.0000,,3.0000",              // пустая
        "1.0000,2.0000,",              // trailing comma
        "1.0000 2.0000 3.0000",        // не запятые
        "1.0000;,2.0000,3.0000",       // мусор
    ] {
        let text = wrap(&format!(
            "OP 001 RAPID start=unknown end={bad} src=0.1-0.0 {ST}\nOP 002 PEND\n"
        ));
        err_lex(&text, "bad vec");
    }
}

// ===========================================================================
// 21. src_f / span / position
// ===========================================================================

#[test]
fn src_placeholder_valid() {
    // D10: плейсхолдер 0.1-0.0 (golden AST)
    let p = ok_lex(&wrap(&rapid_line("unknown", ST)), "src placeholder");
    match &p.ops[0] {
        Op::Rapid { src, .. } => {
            assert_eq!(
                (src.line_start, src.col_start, src.line_end, src.col_end),
                (0, 1, 0, 0)
            );
            assert_eq!(&*src.file, "conf");
        }
        _ => unreachable!(),
    }
}

#[test]
fn src_full_valid() {
    let text = wrap(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=12.34-56.78 {ST}\nOP 002 PEND\n"
    ));
    let p = ok_lex(&text, "src full");
    match &p.ops[0] {
        Op::Rapid { src, .. } => {
            assert_eq!(
                (src.line_start, src.col_start, src.line_end, src.col_end),
                (12, 34, 56, 78)
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn src_invalid() {
    for bad in [
        "0.1",           // нет "-" половины
        "0.1-0.1-0.1",   // три половины
        ".1-0.1",        // нет digits до точки
        "0.-0.1",        // нет digits после точки
        "01-0.1",        // нет точки в первой позиции
        "0.1-",          // нет второй позиции
        "-0.1",          // нет первой
        "12.34 - 56.78", // SP внутри span
    ] {
        let text = wrap(&format!(
            "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src={bad} {ST}\nOP 002 PEND\n"
        ));
        err_lex(&text, "bad src");
    }
}

// ===========================================================================
// 22. sub_f (D8)
// ===========================================================================

#[test]
fn sub_valid_on_motion_ops() {
    // D8: sub= допустим только на RAPID/LINEAR/ARC/DWELL
    let cases = [
        format!("OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 sub=2 {ST}"),
        "OP 001 LINEAR start=0.0000,0.0000,0.0000 end=1.0000,0.0000,0.0000 src=0.1-0.0 sub=2 feed=1.0000 spindle=off coolant=none tool=none frame=wcs wcs=G54".to_string(),
        "OP 001 ARC start=0.0000,0.0000,0.0000 end=-5.0000,5.0000,2.0000 center=0.0000,5.0000,0.0000 normal=0.0000,0.0000,1.0000 dir=cw sweep=90.0000 pitch=2.0000 src=0.1-0.0 sub=2 feed=1.0000 spindle=off coolant=none tool=none frame=wcs wcs=G54".to_string(),
        "OP 001 DWELL seconds=1.0000 src=0.1-0.0 sub=2 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54".to_string(),
    ];
    for c in &cases {
        let text = wrap(&format!("{c}\nOP 002 PEND\n"));
        let p = ok_lex(&text, "sub on motion");
        match p.ops.first() {
            Some(
                Op::Rapid { sub, .. }
                | Op::Linear { sub, .. }
                | Op::Arc { sub, .. }
                | Op::Dwell { sub, .. },
            ) => {
                assert_eq!(*sub, Some(2), "{c}");
            }
            other => panic!("expected motion op, got {other:?}"),
        }
    }
    // sub без значения / мусор
    let bad = wrap(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 sub= {ST}\nOP 002 PEND\n"
    ));
    err_lex(&bad, "sub empty");
    let bad2 = wrap(&format!("OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 sub=-1 {ST}\nOP 002 PEND\n"));
    err_lex(&bad2, "sub negative (integer = digits, без знака)");
}

#[test]
fn sub_invalid_on_non_motion() {
    // D8: на остальных 6 видах sub= — вне продукции op_body
    let bad_stop = wrap(&format!(
        "OP 001 STOP optional=no src=0.1-0.0 sub=1 {ST}\nOP 002 PEND\n"
    ));
    err_lex(&bad_stop, "sub on STOP");
    let bad_tc = wrap("OP 001 TOOL_CHANGE tool=3 src=0.1-0.0 sub=1 feed=none spindle=off coolant=none tool=3 frame=wcs wcs=G54\nOP 002 PEND\n");
    err_lex(&bad_tc, "sub on TOOL_CHANGE");
    let bad_raw = wrap(
        "OP 001 RAW reason=\"x\" code=E_INVALID_INPUT src=0.1-0.0 sub=1 payload=00\nOP 002 PEND\n",
    );
    err_lex(&bad_raw, "sub on RAW");
    let bad_wd = wrap(&format!(
        "OP 001 WCS_DEFINE p=1 origin=0.0000,0.0000,0.0000 src=0.1-0.0 sub=1 {ST}\nOP 002 PEND\n"
    ));
    err_lex(&bad_wd, "sub on WCS_DEFINE");
    let bad_pend = wrap("OP 001 PEND sub=1\nOP 002 PEND\n");
    err_lex(&bad_pend, "sub on PEND");
}

// ===========================================================================
// 23. state (D3: полный снапшот, фиксированный порядок) — комбинаторные
// ===========================================================================

#[test]
fn state_order_valid() {
    // канонический порядок: feed, spindle, coolant, tool, frame, wcs
    let p = ok_lex(&wrap(&rapid_line("unknown", ST)), "state canonical");
    assert_eq!(p.ops.len(), 2);
}

#[test]
fn state_permuted_invalid() {
    // перестановка полей — вне продукции state (фиксированный порядок D3)
    let cases = [
        // spindle перед feed
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 spindle=off feed=none coolant=none tool=none frame=wcs wcs=G54",
        // wcs перед frame
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none wcs=G54 frame=wcs",
        // coolant перед spindle
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none coolant=none spindle=off tool=none frame=wcs wcs=G54",
        // tool перед coolant
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off tool=none coolant=none frame=wcs wcs=G54",
        // полная инверсия
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 wcs=G54 frame=wcs tool=none coolant=none spindle=off feed=none",
    ];
    for c in &cases {
        err_lex(&wrap(&format!("{c}\nOP 002 PEND\n")), "permuted state");
    }
}

#[test]
fn state_missing_invalid() {
    // «оп без state» и с каждым пропущенным полем — вне продукции state
    let cases = [
        // state нет вовсе
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0",
        // каждое поле по одному пропущено (6 вариантов)
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 spindle=off coolant=none tool=none frame=wcs wcs=G54",                              // нет feed
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 feed=none coolant=none tool=none frame=wcs wcs=G54",                                  // нет spindle
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 feed=none spindle=off tool=none frame=wcs wcs=G54",                                    // нет coolant
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 feed=none spindle=off coolant=none frame=wcs wcs=G54",                                 // нет tool
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 feed=none spindle=off coolant=none tool=none wcs=G54",                                 // нет frame
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 feed=none spindle=off coolant=none tool=none frame=wcs",                               // нет wcs
    ];
    for c in &cases {
        err_lex(&wrap(&format!("{c}\nOP 002 PEND\n")), "missing state field");
    }
}

#[test]
fn state_extra_invalid() {
    // лишнее поле после полного снапшота
    let cases = [
        format!("OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST} wcs=G54"), // wcs дважды
        format!("OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST} feed=none"), // feed дважды
        format!("OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST} tool=5"), // tool дважды
        format!("OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST} extra=1"), // неизвестное поле
    ];
    for c in &cases {
        err_lex(&wrap(&format!("{c}\nOP 002 PEND\n")), "extra state field");
    }
}

// ===========================================================================
// 24-29. значения полей state
// ===========================================================================

#[test]
fn state_values_valid() {
    // feed: none | number
    // spindle: off | cw,rpm | ccw,rpm
    // tool: none | integer; frame: wcs | machine
    let cases = [
        "feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54",
        "feed=250.0000 spindle=cw,12000.0000 coolant=none tool=none frame=wcs wcs=G54",
        "feed=0.0000 spindle=ccw,0.5000 coolant=none tool=none frame=wcs wcs=G54",
        "feed=none spindle=off coolant=none tool=0 frame=wcs wcs=G54",
        "feed=none spindle=off coolant=none tool=999999 frame=wcs wcs=G54",
        "feed=none spindle=off coolant=none tool=none frame=machine wcs=G54",
    ];
    for c in &cases {
        let text = wrap(&format!(
            "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {c}\nOP 002 PEND\n"
        ));
        let p = ok_lex(&text, "state values");
        match &p.ops[0] {
            Op::Rapid { state, .. } => {
                if c.contains("tool=0") {
                    assert_eq!(state.tool, Some(0), "tool=0 is a valid integer");
                }
                if c.contains("tool=999999") {
                    assert_eq!(state.tool, Some(999999));
                }
            }
            _ => unreachable!(),
        }
    }
    // невалидные значения значимых полей
    let bads = [
        "feed=NONE", // регистр
        "spindle=On",
        "spindle=cw12000.0000",  // нет запятой
        "spindle=cw,",           // пустой rpm
        "spindle=up,12000.0000", // нет направления
        "tool=3.0000",           // integer, не number
        "tool=-3",               // integer = digits, без знака
        "frame=WCS",
    ];
    for b in &bads {
        let text = wrap(&format!(
            "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {b} coolant=none tool=none frame=wcs wcs=G54\nOP 002 PEND\n"
        ));
        err_lex(&text, "bad state value");
    }
}

#[test]
fn coolant_all_valid() {
    for (c, flood, mist) in [
        ("none", false, false),
        ("flood", true, false),
        ("mist", false, true),
        ("flood+mist", true, true),
    ] {
        let text = wrap(&format!(
            "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant={c} tool=none frame=wcs wcs=G54\nOP 002 PEND\n"
        ));
        let p = ok_lex(&text, "coolant");
        match &p.ops[0] {
            Op::Rapid { state, .. } => {
                assert_eq!(
                    (state.coolant.flood, state.coolant.mist),
                    (flood, mist),
                    "{c}"
                );
            }
            _ => unreachable!(),
        }
    }
    for bad in ["Flood", "flood+matist", "mist+flood", "coolant", "flood+"] {
        let text = wrap(&format!(
            "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant={bad} tool=none frame=wcs wcs=G54\nOP 002 PEND\n"
        ));
        err_lex(&text, "bad coolant");
    }
}

#[test]
fn wcs_names_valid() {
    // все 9 wcs_name; числовой маппинг p: G54=1 … G59.3=9
    let names = [
        ("G54", Wcs::G54),
        ("G55", Wcs::G55),
        ("G56", Wcs::G56),
        ("G57", Wcs::G57),
        ("G58", Wcs::G58),
        ("G59", Wcs::G59),
        ("G59.1", Wcs::G59_1),
        ("G59.2", Wcs::G59_2),
        ("G59.3", Wcs::G59_3),
    ];
    for (i, (w, want)) in names.iter().enumerate() {
        let text = wrap(&format!(
            "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs={w}\nOP 002 PEND\n"
        ));
        let p = ok_lex(&text, "wcs name");
        match &p.ops[0] {
            Op::Rapid { state, .. } => {
                assert_eq!(state.wcs, *want, "{w}");
                // маппинг имя → p: G54=1 … G59.3=9 (parser::wcs_to_p, пере-экспорт verify)
                assert_eq!(
                    eid_core::parser::wcs_to_p(state.wcs),
                    (i + 1) as u8,
                    "{w} → p={}",
                    i + 1
                );
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn wcs_name_invalid() {
    for bad in [
        "G53", "G54.1", "G59.4", "G59.", "g54", "G5", "G60", "WCS", "",
    ] {
        let text = wrap(&format!(
            "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs={bad}\nOP 002 PEND\n"
        ));
        err_lex(&text, "bad wcs name");
    }
}

// ===========================================================================
// 30. number (D1)
// ===========================================================================

#[test]
fn number_sign_valid() {
    // число со знаком в любой числовой позиции (координаты, pitch, feed)
    let text = wrap(
        "OP 001 LINEAR start=-1.0000,-2.0000,-3.0000 end=1.0000,2.0000,3.0000 src=0.1-0.0 feed=-5.0000 spindle=ccw,-0.5000 coolant=none tool=none frame=wcs wcs=G54\nOP 002 PEND\n",
    );
    let p = ok_lex(&text, "signed numbers");
    match &p.ops[0] {
        Op::Linear { start, state, .. } => {
            match start {
                StartPoint::Known(s) => {
                    assert_eq!(s.x.value, -1.0);
                    assert_eq!(s.z.value, -3.0);
                }
                StartPoint::Unknown => panic!("expected Known start"),
            }
            assert_eq!(state.feed.as_ref().unwrap().value, -5.0);
            if let Spindle::Ccw(r) = &state.spindle {
                assert_eq!(r.value, -0.5);
            } else {
                panic!("expected Ccw");
            }
        }
        other => panic!("expected Linear, got {other:?}"),
    }
    // "-0.0000" грамматичен (знак + digits + .0000)
    let neg0 = wrap(&format!(
        "OP 001 DWELL seconds=-0.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    assert!(parse_lex(&neg0).is_ok(), "-0.0000 is grammatical");
}

#[test]
fn number_invalid_lexemes() {
    // D1: [ "-" ] digits "." ровно 4 цифры; без экспоненты; без "+"
    let bads = [
        "1.000",   // 3 dp
        "1.00000", // 5 dp
        "1e2",     // экспонента
        "1E2", "1.0e2", "+1.0000", // плюс не разрешён
        ".5000",   // нет digits до точки
        "-.5000",  // знак без digits
        "1.",      // нет 4 цифр
        "1",       // нет дробной части
        "1.0000x", // мусор
        "1 .0000", // SP внутри
    ];
    for b in &bads {
        let text = wrap(&format!(
            "OP 001 DWELL seconds={b} src=0.1-0.0 {ST}\nOP 002 PEND\n"
        ));
        err_lex(&text, "bad number");
    }
}

#[test]
fn number_leading_zero_grammar_ok() {
    // digits = digit {digit} — ведущие нули ЛЕКСИЧЕСКИ валидны (каноническая
    // форма эмиссии их не порождает, но grammar допускает: conformance
    // проверяет грамматику, не стиль эмиттера)
    let text = wrap(&format!(
        "OP 001 DWELL seconds=01.5000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    let p = ok_lex(&text, "leading zero");
    match &p.ops[0] {
        Op::Dwell { seconds, .. } => {
            assert_eq!(seconds.text, "01.5000"); // verbatim сохранён
            assert_eq!(seconds.value, 1.5);
        }
        other => panic!("expected Dwell, got {other:?}"),
    }
    // "00.0000" тоже
    let text2 = wrap(&format!(
        "OP 001 DWELL seconds=00.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    assert!(parse_lex(&text2).is_ok());
}

// ===========================================================================
// Токенизация строк (SP-дисциплина) — сквозная для op_line
// ===========================================================================

#[test]
fn sp_discipline() {
    // двойной SP / ведущий / замыкающий — вне grammar (SP одиночный)
    let dbl = wrap(&format!(
        "OP 001 RAPID  start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    err_lex(&dbl, "double SP");
    let trail = wrap(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST} \nOP 002 PEND\n"
    ));
    err_lex(&trail, "trailing SP");
    let lead = wrap(&format!(
        " OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    err_lex(&lead, "leading SP on op line");
    // tab вместо SP
    let tab = wrap(&format!(
        "OP\t001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    err_lex(&tab, "tab");
    // CRLF терминаторы: токенизатор принимает оба (LF vs CRLF не важен)
    let lf = wrap(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST}\nOP 002 PEND\n"
    ));
    let crlf = lf.replace('\n', "\r\n");
    assert!(parse_lex(&crlf).is_ok(), "CRLF accepted");
    // CR-бэги без LF (одиночный CR) — не NL по grammar
    let cr_only = wrap("OP 001 PEND\rEND\n");
    err_lex(&cr_only, "lone CR is not NL");
}

// ===========================================================================
// Нумерация строк ошибок: span диагностики указывает на строку нарушения
// ===========================================================================

#[test]
fn error_span_points_at_line() {
    let text = wrap(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST}\nOP 002 DWELL seconds=1.0 src=0.1-0.0 {ST}\nOP 003 PEND\n"
    ));
    let ds = parse_lex(&text).unwrap_err();
    assert_eq!(ds.len(), 1);
    assert_eq!(
        ds[0].src.as_ref().unwrap().line_start,
        6,
        "OP 002 is file line 6"
    );
}
