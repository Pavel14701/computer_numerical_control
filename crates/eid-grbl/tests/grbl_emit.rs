//! Тесты эмиссии GRBL (Агент C, итерация 3). Round-trip R1: parse → emit →
//! parse — semantic_diff = 0 (eid-harness, допуски EQUIVALENCE §4).
//! Диалектные файлы — рукописные v0: byte-exact НЕ гарантирован, сравнение
//! только AST-уровня (бриф п.9); emit дважды — байт-в-байт.

use eid_core::adapter::{Adapter, EidTextAdapter};
use eid_core::ast::{
    Coolant, Frame, Num, Op, Point3, Program, Spindle, SrcSpan, StartPoint, State, Units, Wcs,
};
use eid_core::diag::ErrorCode;
use eid_grbl::GrblAdapter;
use eid_harness::diff::{semantic_diff, DiffConfig};
use std::path::PathBuf;

fn corpus(rel: &str) -> String {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus/tier_a/v0");
    p.push(rel);
    std::fs::read_to_string(p).expect("corpus file readable")
}

fn parse_grbl(src: &str) -> eid_core::ast::Program {
    GrblAdapter::new()
        .parse(src, "t.ngc")
        .expect("grbl parse ok")
        .program
}

/// R1 round-trip: parse → emit → parse → semantic_diff = 0; emit дважды
/// байт-в-байт (детерминизм, бриф п.8).
fn roundtrip(rel: &str) {
    let src = corpus(rel);
    let a = parse_grbl(&src);
    let ad = GrblAdapter::new();
    let text1 = ad.emit(&a).expect("emit ok").text;
    let text2 = ad.emit(&a).expect("emit ok").text;
    assert_eq!(text1, text2, "{rel}: emit недетерминирован");
    let b = parse_grbl(&text1);
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    assert!(d.is_empty(), "{rel}: round-trip divergences: {d:?}");
}

#[test]
fn roundtrip_pilot_01_linear_box() {
    roundtrip("01_linear_box/linear_box.grbl.ngc");
}

#[test]
fn roundtrip_pilot_02_rapid_reposition() {
    roundtrip("02_rapid_reposition/rapid_reposition.grbl.ngc");
}

#[test]
fn roundtrip_pilot_03_feed_survives_rapid() {
    roundtrip("03_feed_survives_rapid/feed_survives_rapid.grbl.ngc");
}

#[test]
fn roundtrip_pilot_11_arc_r_semicircle() {
    roundtrip("11_arc_R_semicircle/arc_R_semicircle.grbl.ngc");
}

#[test]
fn roundtrip_pilot_36_preset_then_switch() {
    roundtrip("36_preset_G54_then_switch/preset_G54_then_switch.grbl.ngc");
}

#[test]
fn roundtrip_pilot_46_arc_r_helix() {
    roundtrip("46_arc_R_helix/arc_R_helix.grbl.ngc");
}

#[test]
fn roundtrip_08_full_circle() {
    roundtrip("08_arc_full_circle/arc_full_circle.grbl.ngc");
}

#[test]
fn roundtrip_10_helix_inch() {
    roundtrip("10_arc_helix_inch/arc_helix_inch.grbl.ngc");
}

#[test]
fn roundtrip_13_degenerate_zero_sweep() {
    // v0.17 (#15): X0.00004 квантуется к старту при center ≠ start →
    // ПОЛНЫЙ КРУГ 360° на обеих сторонах round-trip (диспозиция drop
    // снята, CORPUS v0.6.7) → diff 0.
    roundtrip("13_arc_degenerate_zero_sweep/arc_degenerate_zero_sweep.grbl.ngc");
}

#[test]
fn regen_v1_13_grbl_from_golden_ast() {
    // Регенерация corpus/tier_a/v1/13_arc_degenerate_zero_sweep/
    // arc_degenerate_zero_sweep.grbl.ngc ПРОГРАММНО из нового AST
    // (golden.eid v0.6.7 — 4-опный полный круг): golden → GrblAdapter.emit →
    // файл; затем R1 байт-в-байт: emit(parse(файл)) == файл и
    // parse(файл) ≡ golden (semantic_diff = 0). НЕ редактируется вручную.
    let golden_text = corpus("13_arc_degenerate_zero_sweep/golden.eid");
    let golden = EidTextAdapter::new()
        .parse(&golden_text, "13.golden.eid")
        .expect("golden parses")
        .program;
    let ad = GrblAdapter::new();
    let text = ad.emit(&golden).expect("emit golden → grbl").text;
    let v1_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/tier_a/v1/13_arc_degenerate_zero_sweep");
    let v1_path = v1_dir.join("arc_degenerate_zero_sweep.grbl.ngc");
    std::fs::create_dir_all(&v1_dir).expect("v1 dir");
    std::fs::write(&v1_path, &text).expect("v1 grbl regen write");
    // R1 байт-в-байт: re-emit от re-parse совпадает с файлом.
    let reparsed_out = ad.parse(&text, "13_regen.ngc").expect("regen parses");
    let reparsed = reparsed_out.program;
    let text2 = ad.emit(&reparsed).expect("re-emit").text;
    assert_eq!(
        text, text2,
        "emit(parse(file)) != file — R1 байт-в-байт сломан"
    );
    // R3-golden: parse(файл) ≡ golden AST (semantic_diff = 0); notice нет.
    let div = semantic_diff(&golden, &reparsed, &DiffConfig::default());
    assert!(div.is_empty(), "regen divergences vs golden: {div:?}");
    assert!(
        reparsed_out.notices.is_empty(),
        "regen-файл не должен давать drop-notice (полный круг — живой оп)"
    );
    // Полный круг в тексте: 4-опная программа — G0 + ARC-360 (без end-слов)
    // + G1 + M2.
    let arc_line = text
        .lines()
        .find(|l| l.starts_with("G2 "))
        .expect("arc line");
    assert!(
        arc_line.contains("I5.0000") && arc_line.contains("J0.0000"),
        "{arc_line}"
    );
    assert!(
        !arc_line.contains('X') && !arc_line.contains('Y') && !arc_line.contains('Z'),
        "полный круг без end-слов: {arc_line}"
    );
}

#[test]
fn roundtrip_47_crlf_dollar_dollar() {
    // $$ + полный CRLF-файл: RAW verbatim С терминатором (D9 v0.3: payload
    // "$$\r\n" → hex 24240D0A), байты байт-в-байт (только RAW-строка проходит
    // через emit дословно; заголовок — свой, по брифу п.1).
    roundtrip("47_raw_crlf_fidelity/raw_crlf_fidelity.grbl.ngc");
    let a = parse_grbl(&corpus("47_raw_crlf_fidelity/raw_crlf_fidelity.grbl.ngc"));
    match &a.ops[0] {
        eid_core::ast::Op::Raw { payload_hex, .. } => {
            assert_eq!(payload_hex, "24240D0A", "payload без \\n (D9 v0.2-хвост)");
        }
        other => panic!("ops[0] не Raw: {other:?}"),
    }
    let text = GrblAdapter::new().emit(&a).expect("emit ok").text;
    assert!(text.contains("$$\r\n"), "CRLF при $$ потерян: {text:?}");
    assert!(!text.contains("$$\r\n\n"), "двойной терминатор после Raw");
}

#[test]
fn emit_never_uses_r_form() {
    // EQUIVALENCE §4 политика: R не эмитится никогда — только IJK-пара.
    let a = parse_grbl(&corpus("11_arc_R_semicircle/arc_R_semicircle.grbl.ngc"));
    let text = GrblAdapter::new().emit(&a).expect("emit ok").text;
    let arc_line = text
        .lines()
        .find(|l| l.starts_with("G2 "))
        .expect("arc line");
    assert!(
        arc_line.contains('I') && arc_line.contains('J'),
        "{arc_line}"
    );
    assert!(!arc_line.contains(" R"), "{arc_line}");
}

#[test]
fn vector_27_tool_change_is_syntax() {
    // eid-core magic v0.3 (commit 655d194) — golden.eid снова через R1-парсер.
    let golden = EidTextAdapter::new()
        .parse(&corpus("27_tool_change/golden.eid"), "27.golden.eid")
        .expect("golden parses")
        .program;
    let Err(diags) = GrblAdapter::new().emit(&golden) else {
        panic!("#27: ожидался Err(E_UNSUPPORTED_SYNTAX)");
    };
    assert!(diags.iter().all(|d| d.code == ErrorCode::UnsupportedSyntax));
}

#[test]
fn vector_36_preset_order_e14() {
    // EQUIVALENCE §1.4: заголовок → выборка preset.wcs → G92 — ДО всякого хода;
    // без «запекания» в G10.
    let a = parse_grbl(&corpus(
        "36_preset_G54_then_switch/preset_G54_then_switch.grbl.ngc",
    ));
    let text = GrblAdapter::new().emit(&a).expect("emit ok").text;
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "G21 G90 G94", "заголовок всегда явно");
    assert_eq!(lines[1], "G54", "выборка preset.wcs — строка 2");
    assert_eq!(
        lines[2], "G92 X0.0000 Y0.0000 Z5.0000",
        "G92 — строка 3, 4dp"
    );
    let first_motion = lines
        .iter()
        .position(|l| l.starts_with("G0 "))
        .expect("есть ход");
    let g92 = lines
        .iter()
        .position(|l| l.starts_with("G92"))
        .expect("G92 есть");
    assert!(g92 < first_motion, "G92 обязан быть до первого хода");
    assert!(
        !text.contains("G10 L2 P1 X0.0000 Y0.0000 Z5.0000"),
        "запрет запекания в G10"
    );
}

#[test]
fn negative_n3_config_verbatim() {
    // RAW: payload hex → исходные байты verbatim (same-dialect fidelity).
    let rel = "n3_raw_vendor_block/raw_vendor_block.grbl.ngc";
    let a = parse_grbl(&corpus(rel));
    let text = GrblAdapter::new().emit(&a).expect("emit ok").text;
    assert!(text.contains("$110=250.000"), "{text}");
    roundtrip(rel);
}

#[test]
fn raw_crlf_preserved_byte_exact() {
    // CRLF сохраняется байт-в-байт (бриф п.7; #47 не в GRBL-наборе — синтетика).
    let src = "$110=250.000\r\nG21 G90 G94 G17\nG0 X1.0 Y1.0 Z1.0\nM2\n";
    let a = parse_grbl(src);
    let text = GrblAdapter::new().emit(&a).expect("emit ok").text;
    assert!(text.contains("$110=250.000\r\n"), "CRLF потерян: {text:?}");
}

#[test]
fn g53_prefix_and_g10_define_roundtrip() {
    // frame=machine → префикс "G53 " на строке хода; WcsDefine → G10 L2 P..
    let src = "G21 G90 G94 G17\nG10 L2 P1 X1.0 Y2.0 Z3.0\nG0 X1.0 Y0 Z0\nG53 G0 X0.0 Y0.0 Z-5.0\nG0 X1.0 Y0 Z0\nM2\n";
    let a = parse_grbl(src);
    let text = GrblAdapter::new().emit(&a).expect("emit ok").text;
    assert!(text.contains("G10 L2 P1 X1.0000 Y2.0000 Z3.0000"), "{text}");
    assert!(text.contains("G53 G0 X0.0000 Y0.0000 Z-5.0000"), "{text}");
    let b = parse_grbl(&text);
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    assert!(d.is_empty(), "machine-frame round-trip: {d:?}");
}

#[test]
fn plane_switch_and_g18_pair() {
    // G18 → пара I+K; смена плоскости эмитится перед дугой (бриф п.1/п.4).
    let src = "G21 G90 G94\nG0 X0 Y0 Z0\nG18\nG3 X0.0 Y5.0 Z0.0 I0.0 K5.0 F100.0\nM2\n";
    let a = parse_grbl(src);
    let text = GrblAdapter::new().emit(&a).expect("emit ok").text;
    let arc_line = text
        .lines()
        .find(|l| l.starts_with("G3"))
        .expect("arc line");
    assert!(
        arc_line.contains("I") && arc_line.contains("K"),
        "{arc_line}"
    );
    assert!(text.contains("G18\n"), "плоскость перед дугой");
    let b = parse_grbl(&text);
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    assert!(d.is_empty(), "G18 round-trip: {d:?}");
}

#[test]
fn dwell_and_stops_emission() {
    let src = "G21 G90 G94\nG0 X1 Y1 Z1\nG4 P1.5\nM0\nM2\n";
    let a = parse_grbl(src);
    let text = GrblAdapter::new().emit(&a).expect("emit ok").text;
    assert!(text.contains("G4 P1.5000"), "{text}");
    assert!(text.contains("M0"), "{text}");
    assert!(
        text.lines().last().expect("non-empty") == "M2",
        "M2 — последняя строка"
    );
}

// ---------------------------------------------------------------------------
// EmissionLegality — ConversionTarget-таблица (итерация 4)
// ---------------------------------------------------------------------------

// v0.2 волна 4 (§2.5 В1): коды условий — тотальные match-функции enum'а
// (таблица — данные; согласие — legality_table_complete ниже).
use eid_grbl::emit::GrblEmissionCondition as Cond;

fn legality_table() -> eid_grbl::emit::EmissionLegality {
    GrblAdapter::new().emission_legality()
}

/// T9 (v0.2 волна 4): таблица = данные, КОДЫ — тотальные match-функции:
/// каждая пара (условие, код) в таблице обязана совпадать с `cond.code()`;
/// каждое условие объявлено ровно один раз.
#[test]
fn legality_table_complete() {
    let t = legality_table();
    let all = [
        Cond::ToolChange,
        Cond::WcsOutsideG54G59,
        Cond::MotionFeedNone,
        Cond::ArcUnknownStart,
        Cond::ArcNormalNonCanonical,
        Cond::RawPayloadNotHex,
        Cond::ProgramEndMissing,
        Cond::ProgramEndDuplicate,
        Cond::ProgramEndNotLast,
    ];
    for cond in all {
        let n = t.illegal.iter().filter(|(c, _)| *c == cond).count();
        assert_eq!(n, 1, "условие {} в таблице ровно один раз", cond.as_str());
        let (_, code) = t.illegal.iter().find(|(c, _)| *c == cond).unwrap();
        assert_eq!(
            *code,
            cond.code(),
            "код {} = тотальный match",
            cond.as_str()
        );
    }
}

#[test]
fn legality_table_matches_manifest_r4_expect() {
    // Инвариант (не числа): каждый Code из r4_expect["grbl-1.1"] обязан быть
    // объявлен в таблице легальности; исключение — E_RAW_UNTRANSLATABLE
    // (translate-путь, Агент E / SPEC §8.2 — вне same-dialect эмиссии).
    // Обратно: каждый SYNTAX-класс таблицы должен быть покрыт хотя бы одним
    // manifest-вектором; INVALID-класс таблицы — только канон-форма (§3),
    // это инварианты AST→emit, корпусом не ожидается.
    let manifest_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/manifest.toml");
    let manifest = match eid_harness::manifest::load(&manifest_path) {
        Ok(m) => m,
        // Транзит (допустимый): манифест ссылается на ещё не материализованные
        // файлы Tier B (in-flight у B) — инвариант неприменим до стабилизации.
        Err(e) if e.contains("does not exist under") => {
            eprintln!("SKIP: transit — manifest references missing files: {e}");
            return;
        }
        Err(e) => panic!("manifest load failed: {e}"),
    };
    let table = legality_table();

    let mut manifest_codes: Vec<ErrorCode> = Vec::new();
    for v in &manifest.vectors {
        if let Some(eid_harness::manifest::Outcome::Code(code)) = v.r4_expect.get("grbl-1.1") {
            manifest_codes.push(*code);
        }
    }
    assert!(
        !manifest_codes.is_empty(),
        "manifest: нет r4_expect для grbl-1.1"
    );

    let table_codes = table.codes();
    for code in &manifest_codes {
        assert!(
            *code == ErrorCode::RawUntranslatable || table_codes.contains(code),
            "manifest r4_expect {code:?} не объявлен в таблице легальности"
        );
    }
    for (cond, code) in &table.illegal {
        if *code == ErrorCode::UnsupportedSyntax {
            assert!(
                manifest_codes.contains(code),
                "SYNTAX-условие `{}` таблицы не покрыто manifest r4_expect",
                cond.as_str()
            );
        } else {
            assert_eq!(
                *code,
                ErrorCode::InvalidInput,
                "условие `{}`: не-SYNTAX класс таблицы обязан быть канон-формой (INVALID)",
                cond.as_str()
            );
        }
    }
}

#[test]
fn legality_negative_tool_change_via_table() {
    // Отказ #27 идёт ЧЕРЕЗ таблицу: код диагностики == code_of("tool_change").
    let golden = EidTextAdapter::new()
        .parse(&corpus("27_tool_change/golden.eid"), "27.golden.eid")
        .expect("golden parses")
        .program;
    let expected = Cond::ToolChange.code();
    assert_eq!(expected, ErrorCode::UnsupportedSyntax);
    let Err(diags) = GrblAdapter::new().emit(&golden) else {
        panic!("#27: ожидался Err из таблицы легальности");
    };
    assert!(!diags.is_empty());
    for d in &diags {
        assert_eq!(d.code, expected, "код отказа не из таблицы легальности");
    }
}

#[test]
fn legality_table_shape_invariants() {
    let table = legality_table();
    // legal: ровно виды опов, представимые ходами GRBL (COVERAGE §5).
    for kind in [
        "rapid",
        "linear",
        "arc",
        "dwell",
        "wcs_define",
        "wcs_set_here",
        "frame_preset",
        "program_stop",
        "program_end",
        "raw",
    ] {
        assert!(table.is_legal_kind(kind), "`{kind}` отсутствует в legal");
    }
    assert!(!table.is_legal_kind("tool_change"));
    // illegal: условия уникальны.
    let mut conds: Vec<&str> = table.illegal.iter().map(|(c, _)| c.as_str()).collect();
    conds.sort_unstable();
    conds.dedup();
    assert_eq!(conds.len(), table.illegal.len(), "дубли условий в illegal");
}

// ---------------------------------------------------------------------------
// ПОКРЫТИЕ spec_cov_* — state-переходы эмиттера + legality (чек-лист в шапке
// grbl_parse.rs; правило → тест)
// ---------------------------------------------------------------------------

/// Мини-AST: два linear с заданными coolant/spindle + M2 (hand-built — для
/// переходов, невыразимых из GRBL-исходника, и для legality-негативов).
/// Инвариант Q11: первый оп БЕЗ якоря обязан иметь start=unknown.
fn two_move_prog(
    coolant_a: Coolant,
    coolant_b: Coolant,
    spindle: Spindle,
    feed_a: Option<f64>,
) -> Program {
    let n = |v: f64| Num::new(format!("{v}"), v);
    let st = |coolant: Coolant, feed: Option<f64>| State {
        feed: feed.map(|v| Num::new(format!("{v}"), v)),
        spindle: spindle.clone(),
        coolant,
        tool: None,
        frame: Frame::Wcs,
        wcs: Wcs::G54,
        feed_restated: false,
    };
    let lin = |s: StartPoint, e: Point3, coolant: Coolant, feed: Option<f64>| Op::Linear {
        start: s,
        end: e,
        state: st(coolant, feed),
        src: SrcSpan {
            file: "t".into(),
            line_start: 1,
            col_start: 1,
            line_end: 1,
            col_end: 2,
        },
        sub: None,
    };
    Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: eid_core::ast::PathMode::ExactPath,
        frame_preset: None,
        ops: vec![
            lin(
                StartPoint::Unknown,
                Point3 {
                    x: n(1.0),
                    y: n(0.0),
                    z: n(0.0),
                },
                coolant_a,
                feed_a,
            ),
            lin(
                StartPoint::Known(Point3 {
                    x: n(1.0),
                    y: n(0.0),
                    z: n(0.0),
                }),
                Point3 {
                    x: n(2.0),
                    y: n(0.0),
                    z: n(0.0),
                },
                coolant_b,
                Some(100.0),
            ),
            Op::ProgramEnd {
                src: SrcSpan {
                    file: "t".into(),
                    line_start: 9,
                    col_start: 1,
                    line_end: 9,
                    col_end: 2,
                },
            },
        ],
    }
}

fn coolant(state: bool, mist: bool) -> Coolant {
    Coolant { flood: state, mist }
}

#[test]
fn spec_cov_emit_coolant_all_transitions() {
    let ad = GrblAdapter::new();
    // (state A, state B, ожидаемые M-строки: перед оп1, перед оп2)
    let cases: Vec<(Coolant, Coolant, &str, &str)> = vec![
        (coolant(false, false), coolant(true, false), "", "M8\n"), // off→flood
        (coolant(false, false), coolant(false, true), "", "M7\n"), // off→mist
        (coolant(false, false), coolant(true, true), "", "M7\nM8\n"), // off→flood+mist
        (coolant(true, false), coolant(false, false), "M8\n", "M9\n"), // flood→none
        (coolant(false, true), coolant(false, false), "M7\n", "M9\n"), // mist→none (#29-класс)
        (
            coolant(true, true),
            coolant(false, false),
            "M7\nM8\n",
            "M9\n",
        ), // flood+mist→none
        (coolant(true, false), coolant(true, true), "M8\n", "M7\n"), // flood→flood+mist
        (coolant(false, true), coolant(true, true), "M7\n", "M8\n"), // mist→flood+mist
    ];
    for (a, b, pre, mid) in cases {
        let text = ad
            .emit(&two_move_prog(a, b, Spindle::Off, Some(100.0)))
            .expect("emit ok")
            .text;
        let head = format!("G21 G90 G94\nG54\n{pre}G1 X1.0000 Y0.0000 Z0.0000 F100.0000\n{mid}");
        assert!(
            text.starts_with(&head),
            "переход не отражён:\nожидался старт\n{head}\n--- факт ---\n{text}"
        );
    }
}

#[test]
fn spec_cov_emit_coolant_partial_off_reenables() {
    // Частичное выключение при живом втором — только M9 + повтор оставшегося
    // (из GRBL-исходника невыразимо; hand-AST).
    let ad = GrblAdapter::new();
    let flood_to_mist = two_move_prog(
        coolant(true, false),
        coolant(false, true),
        Spindle::Off,
        Some(100.0),
    );
    let text = ad.emit(&flood_to_mist).expect("emit ok").text;
    assert!(text.contains("F100.0000\nM9\nM7\nG1 X2.0000"), "{text}");
    let mist_to_flood = two_move_prog(
        coolant(false, true),
        coolant(true, false),
        Spindle::Off,
        Some(100.0),
    );
    let text = ad.emit(&mist_to_flood).expect("emit ok").text;
    assert!(
        text.contains("M7\nG1 X1.0000 Y0.0000 Z0.0000 F100.0000\nM9\nM8\nG1 X2.0000"),
        "{text}"
    );
}

#[test]
fn spec_cov_emit_spindle_transitions() {
    // off→cw→off→ccw; S-ревизия на работающем шпинделе (cw 10000→12000).
    let src = "G21 G90 G94 G17\nG0 X1 Y1 Z1\nM3 S10000\nG0 X2 Y1 Z1\nM5\nG0 X3 Y1 Z1\nM4 S8000\nG0 X4 Y1 Z1\nS12000\nG0 X5 Y1 Z1\nM2\n";
    let a = parse_grbl(src);
    let text = GrblAdapter::new().emit(&a).expect("emit ok").text;
    let want = "G21 G90 G94\nG54\nG0 X1.0000 Y1.0000 Z1.0000\nM3 S10000.0000\nG0 X2.0000 Y1.0000 Z1.0000\nM5\nG0 X3.0000 Y1.0000 Z1.0000\nM4 S8000.0000\nG0 X4.0000 Y1.0000 Z1.0000\nM4 S12000.0000\nG0 X5.0000 Y1.0000 Z1.0000\nM2\n";
    assert_eq!(text, want, "шпиндельная дельта");
    let b = parse_grbl(&text);
    assert!(
        semantic_diff(&a, &b, &DiffConfig::default()).is_empty(),
        "spindle round-trip"
    );
}

#[test]
fn spec_cov_emit_legality_conditions_via_table() {
    // 8 illegal-условий таблицы × AST → Err с ТОЧНЫМ кодом из таблицы.
    // stage показывает гейт: Emit = наша таблица, Verify = eid-core-валидатор
    // (защита раньше по потоку; код отказа тот же).
    use eid_core::diag::Stage;
    let ad = GrblAdapter::new();
    let n = |v: f64| Num::new(format!("{v}"), v);
    let span0 = || SrcSpan {
        file: "t".into(),
        line_start: 1,
        col_start: 1,
        line_end: 1,
        col_end: 2,
    };
    let st = |feed: Option<f64>, wcs: Wcs| State {
        feed: feed.map(|v| Num::new(format!("{v}"), v)),
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        tool: None,
        frame: Frame::Wcs,
        wcs,
        feed_restated: false,
    };
    let lin = |unknown_start: bool, feed: Option<f64>, wcs: Wcs| Op::Linear {
        start: if unknown_start {
            StartPoint::Unknown
        } else {
            StartPoint::Known(Point3 {
                x: n(0.0),
                y: n(0.0),
                z: n(0.0),
            })
        },
        end: Point3 {
            x: n(1.0),
            y: n(0.0),
            z: n(0.0),
        },
        state: st(feed, wcs),
        src: span0(),
        sub: None,
    };
    let arc = |unknown_start: bool, normal: [f64; 3]| Op::Arc {
        start: if unknown_start {
            StartPoint::Unknown
        } else {
            StartPoint::Known(Point3 {
                x: n(0.0),
                y: n(0.0),
                z: n(0.0),
            })
        },
        end: Point3 {
            x: n(1.0),
            y: n(0.0),
            z: n(0.0),
        },
        center: Point3 {
            x: n(0.0),
            y: n(5.0),
            z: n(0.0),
        },
        normal: Point3 {
            x: n(normal[0]),
            y: n(normal[1]),
            z: n(normal[2]),
        },
        dir: eid_core::ast::Dir::Cw,
        sweep_deg: n(90.0),
        pitch: n(0.0),
        state: st(Some(100.0), Wcs::G54),
        src: span0(),
        sub: None,
    };
    let prog = |ops: Vec<Op>| Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: eid_core::ast::PathMode::ExactPath,
        // Якорь: Known-start первого опа легален только при frame_preset (Q11).
        frame_preset: Some(eid_core::ast::FramePreset {
            values: Point3 {
                x: n(0.0),
                y: n(0.0),
                z: n(0.0),
            },
            wcs: Wcs::G54,
        }),
        ops,
    };
    let end = || Op::ProgramEnd { src: span0() };

    // 1. motion_feed_none — код из таблицы; гейт: eid-core добавил §3-проверку
    // (stage Verify) — Emit-путь таблицы остаётся защитой в глубину.
    let Err(d) = ad.emit(&prog(vec![lin(false, None, Wcs::G54), end()])) else {
        panic!("feed_none: ожидался Err")
    };
    assert!(
        d.iter().all(|x| x.code == Cond::MotionFeedNone.code()),
        "motion_feed_none: фактические диагностики: {d:?}"
    );
    // 2. wcs_outside_g54_g59 — гейт Emit или Verify (eid-core дублирует).
    let Err(d) = ad.emit(&prog(vec![lin(true, Some(100.0), Wcs::G59_1), end()])) else {
        panic!("wcs: ожидался Err")
    };
    assert!(
        d.iter().all(|x| x.code == Cond::WcsOutsideG54G59.code()),
        "wcs: {d:?}"
    );
    // 3. raw_payload_not_hex — гейт Emit.
    let Err(d) = ad.emit(&prog(vec![
        Op::Raw {
            payload_hex: "ZZ".into(),
            reason: "r".into(),
            code: ErrorCode::UnsupportedSyntax,
            src: span0(),
        },
        end(),
    ])) else {
        panic!("raw_hex: ожидался Err")
    };
    assert!(d
        .iter()
        .all(|x| x.code == Cond::RawPayloadNotHex.code() && x.stage == Stage::Emit));
    // 4–5. arc_unknown_start / arc_normal_non_canonical — Err INVALID
    // (код из таблицы; eid-core может сработать тем же классом раньше).
    for (arc_op, cond) in [
        (arc(true, [0.0, 0.0, 1.0]), Cond::ArcUnknownStart),
        (arc(false, [0.6, 0.8, 0.0]), Cond::ArcNormalNonCanonical),
    ] {
        let Err(d) = ad.emit(&prog(vec![arc_op, end()])) else {
            panic!("{}: ожидался Err", cond.as_str())
        };
        assert!(
            d.iter().all(|x| x.code == cond.code()),
            "{}: код не из таблицы",
            cond.as_str()
        );
    }
    // 6–8. program_end missing/duplicate/not_last — INVALID; структура
    // дублируется eid-core-валидатором → гейт Verify допустим (код тот же).
    for (ops, cond) in [
        (
            vec![lin(false, Some(100.0), Wcs::G54)],
            Cond::ProgramEndMissing,
        ),
        (vec![end(), end()], Cond::ProgramEndDuplicate),
        (
            vec![end(), lin(false, Some(100.0), Wcs::G54)],
            Cond::ProgramEndNotLast,
        ),
    ] {
        let Err(d) = ad.emit(&prog(ops)) else {
            panic!("{}: ожидался Err", cond.as_str())
        };
        assert!(
            d.iter().all(|x| x.code == cond.code()),
            "{}: код не из таблицы",
            cond.as_str()
        );
    }

    // 9-10. wcs_outside через RAPID и DWELL (другие arms валидатора);
    // arc_normal_non_canonical на ВАЛИДНОЙ геометрии (ρ=0.5, 180°) —
    // отклонить может только проверка нормали.
    let Err(d) = ad.emit(&prog(vec![
        // start Unknown: wcs G59_1 не совпадает с якорем G54 → чтение неизвестно (Q11).
        Op::Rapid {
            start: StartPoint::Unknown,
            end: Point3 {
                x: n(1.0),
                y: n(0.0),
                z: n(0.0),
            },
            state: st(Some(100.0), Wcs::G59_1),
            src: span0(),
            sub: None,
        },
        end(),
    ])) else {
        panic!("wcs-rapid: ожидался Err")
    };
    assert!(
        d.iter().all(|x| x.code == Cond::WcsOutsideG54G59.code()),
        "wcs-rapid: {d:?}"
    );
    let Err(d) = ad.emit(&prog(vec![
        Op::Dwell {
            seconds: n(1.0),
            state: st(Some(100.0), Wcs::G59_1),
            src: span0(),
            sub: None,
        },
        end(),
    ])) else {
        panic!("wcs-dwell: ожидался Err")
    };
    assert!(
        d.iter().all(|x| x.code == Cond::WcsOutsideG54G59.code()),
        "wcs-dwell: {d:?}"
    );
    let Err(d) = ad.emit(&prog(vec![
        Op::Arc {
            start: StartPoint::Known(Point3 {
                x: n(0.0),
                y: n(0.0),
                z: n(0.0),
            }),
            end: Point3 {
                x: n(1.0),
                y: n(0.0),
                z: n(0.0),
            },
            center: Point3 {
                x: n(0.5),
                y: n(0.0),
                z: n(0.0),
            },
            normal: Point3 {
                x: n(0.6),
                y: n(0.8),
                z: n(0.0),
            },
            dir: eid_core::ast::Dir::Cw,
            sweep_deg: n(180.0),
            pitch: n(0.0),
            state: st(Some(100.0), Wcs::G54),
            src: span0(),
            sub: None,
        },
        end(),
    ])) else {
        panic!("normal: ожидался Err")
    };
    assert!(
        d.iter()
            .all(|x| x.code == Cond::ArcNormalNonCanonical.code()),
        "normal: {d:?}"
    );
}

#[test]
fn spec_cov_emit_planes_exact_text() {
    // Эмиссия дуг в G19/G18 — точные строки смены плоскости и пары осей
    // (I+K для G18; J+K для G19); non-unit normal → Err INVALID из plane_of.
    let src = "G21 G90 G94 G18\nG0 X0 Y0 Z0\nG2 X5.0 Y2.0 Z-5.0 I5.0 K0.0 F100.0\nG19\nG3 X5.0 Y7.0 Z-10.0 J5.0 K0.0 F100.0\nM2\n";
    let a = parse_grbl(src);
    let text = GrblAdapter::new().emit(&a).expect("emit ok").text;
    assert!(text.contains("G18\n"), "{text}");
    assert!(text.contains("G19\n"), "{text}");
    assert!(
        text.contains("G2 X5.0000 Y2.0000 Z-5.0000 I5.0000 K0.0000"),
        "{text}"
    );
    assert!(
        text.contains("G3 X5.0000 Y7.0000 Z-10.0000 J5.0000 K0.0000"),
        "{text}"
    );
    let b = parse_grbl(&text);
    assert!(semantic_diff(&a, &b, &DiffConfig::default()).is_empty());
}

#[test]
fn spec_cov_emit_wcs_cycle_exact_text() {
    // Смена всех шести WCS — строки G54..G59 в emitted-тексте (arms wcs_code).
    let mut src = String::from("G21 G90 G94 G17\n");
    for (i, g) in ["G54", "G55", "G56", "G57", "G58", "G59"]
        .iter()
        .enumerate()
    {
        src.push_str(&format!("{g}\nG0 X{}.0000 Y0.0000 Z1.0000\n", i + 1));
    }
    src.push_str("M2\n");
    let a = parse_grbl(&src);
    let text = GrblAdapter::new().emit(&a).expect("emit ok").text;
    for g in ["G54\n", "G55\n", "G56\n", "G57\n", "G58\n", "G59\n"] {
        assert!(text.contains(g), "{g} отсутствует: {text}");
    }
    let b = parse_grbl(&text);
    assert!(semantic_diff(&a, &b, &DiffConfig::default()).is_empty());
}

#[test]
fn spec_cov_emit_spindle_same_value_not_reemitted() {
    // Соседние ходы с тем же шпинделем — БЕЗ повторной M3/M4-эмиссии
    // (дельта по spindle_eq; ловит re-emit мутантов).
    let src = "G21 G90 G94 G17\nM3 S10000\nG0 X1 Y1 Z1\nG0 X2 Y1 Z1\nG0 X3 Y1 Z1\nM2\n";
    let a = parse_grbl(src);
    let text = GrblAdapter::new().emit(&a).expect("emit ok").text;
    assert_eq!(text.matches("M3").count(), 1, "повторная M3: {text}");
}

#[test]
fn spec_cov_emit_op_kind_in_legality() {
    // op_kind всех опов пилота #01 ∈ legal-таблицы (связка таблица↔op_kind).
    let a = parse_grbl(&corpus("01_linear_box/linear_box.grbl.ngc"));
    let table = legality_table();
    for op in &a.ops {
        let kind = eid_grbl::emit::op_kind(op);
        assert!(table.is_legal_kind(kind), "{kind} не в legal");
    }
}

#[test]
fn spec_cov_emit_zero_length_rapid_notice_kind() {
    // Drop zero-length RAPID: notice несёт именно Rapid-оп.
    let out = eid_grbl::canonicalize::canonicalize_with_notices(
        "G21 G90 G94 G17\nG92 X0 Y0 Z0\nG0 X0 Y0 Z0\nM2\n",
        "zr.ngc",
    )
    .expect("ok");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(
        out.notices[0].kind,
        eid_grbl::canonicalize::NoticeKind::ZeroLengthMotion
    );
    assert!(matches!(out.notices[0].op, Op::Rapid { .. }));
}

#[test]
fn spec_cov_emit_f_bootstrap_on_rapid() {
    // §7.7-1 развёртки: traverse/approach rapid несут modal feed кадра —
    // F-дельта включает rapid ("G0 F.." легален, устанавливает модальный F;
    // без него parse-back даёт feed=none ≠ golden — корень #17–#26).
    let n = |v: f64| Num::new(format!("{v}"), v);
    let st150 = State {
        feed: Some(n(150.0)),
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        tool: None,
        frame: Frame::Wcs,
        wcs: Wcs::G54,
        feed_restated: false,
    };
    let span0 = || SrcSpan {
        file: "t".into(),
        line_start: 1,
        col_start: 1,
        line_end: 1,
        col_end: 2,
    };
    let prog = Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: eid_core::ast::PathMode::ExactPath,
        frame_preset: None,
        ops: vec![
            Op::Rapid {
                start: StartPoint::Unknown,
                end: Point3 {
                    x: n(1.0),
                    y: n(0.0),
                    z: n(0.0),
                },
                state: st150.clone(),
                src: span0(),
                sub: None,
            },
            Op::Linear {
                start: StartPoint::Known(Point3 {
                    x: n(1.0),
                    y: n(0.0),
                    z: n(0.0),
                }),
                end: Point3 {
                    x: n(2.0),
                    y: n(0.0),
                    z: n(0.0),
                },
                state: st150,
                src: span0(),
                sub: None,
            },
            Op::ProgramEnd { src: span0() },
        ],
    };
    let text = GrblAdapter::new().emit(&prog).expect("emit ok").text;
    assert!(
        text.contains("G0 X1.0000 Y0.0000 Z0.0000 F150.0000"),
        "F не забутстрапился на rapid: {text}"
    );
    assert!(
        text.contains("G1 X2.0000 Y0.0000 Z0.0000\n"),
        "линейный F-повтор не нужен: {text}"
    );
    // Standalone rapid ПОСЛЕ F сохраняет feed (не сбрасывает).
    let b = parse_grbl(&text);
    assert!(
        semantic_diff(&prog, &b, &DiffConfig::default()).is_empty(),
        "round-trip F-bootstrap"
    );
}

#[test]
fn spec_cov_emit_path_mode_warnings() {
    // Q3: PATH_MODE не выражается в GRBL-шапке → emit Ok + warning-текст
    // из EmissionLegality.warnings (manifest r4_expect: grbl = warning).
    let ad = GrblAdapter::new();
    let cases = [
        (
            "40_path_exact/golden.eid",
            "junction blending behaviour lost on GRBL (COVERAGE §6.1)",
        ),
        (
            "41_path_blend/golden.eid",
            "target G64 P- tolerance not representable",
        ),
        (
            "45_path_exact_stop/golden.eid",
            "exact stop not representable on GRBL (no G61.1; COVERAGE §5)",
        ),
    ];
    for (rel, want) in cases {
        let g = EidTextAdapter::new()
            .parse(&corpus(rel), rel)
            .expect("golden parses")
            .program;
        let out = ad.emit(&g).expect("emit Ok (warning, не отказ)");
        let text = out.text;
        let warnings = out.warnings;
        assert_eq!(
            warnings.len(),
            1,
            "{rel}: ровно одно предупреждение, факт {warnings:?}"
        );
        assert_eq!(warnings[0], want, "{rel}: текст warning");
        assert!(text.contains("G21 G90 G94"), "{rel}: заголовок на месте");
    }
}

#[test]
fn cycle_f_word_reemitted_on_each_f_frame() {
    // SPEC §7.4a v0.18, эмиссионное следствие (зеркало linuxcnc feed_word,
    // агент C): rs274 ассертит SET_FEED_RATE на каждом кадре с F-словом —
    // эмиттер переизлучает F на ПЕРВОМ производном опе (sub=1) каждого
    // F-кадра, даже при неизменном значении. Тест 1:1 с linuxcnc_emit
    // cycle_f_word_reemitted_on_each_f_frame: 2 подряд G81 F120 → ОБА
    // traverse-блока с F. АСТ строится напрямую: словарь GRBL canned cycles
    // не содержит (см. spec_cov_cycles_raw_classes в grbl_parse.rs) —
    // cycle-производные опы появляются в grbl-эмиссии только кросс-
    // диалектно (golden/перевод, §7.7: traverse sub=1, подход sub=2,
    // feed sub=3, отвод sub=4 — все несут модальный feed кадра).
    let n = |v: f64| Num::new(format!("{v}"), v);
    let span0 = || SrcSpan {
        file: "t.ngc".into(),
        line_start: 1,
        col_start: 1,
        line_end: 1,
        col_end: 1,
    };
    let st120 = State {
        feed: Some(Num::new("120.0".into(), 120.0)),
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        tool: None,
        frame: Frame::Wcs,
        wcs: Wcs::G54,
        feed_restated: false,
    };
    let st_none = State {
        feed: None,
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        tool: None,
        frame: Frame::Wcs,
        wcs: Wcs::G54,
        feed_restated: false,
    };
    let p3 = |x: f64, y: f64, z: f64| Point3 {
        x: n(x),
        y: n(y),
        z: n(z),
    };
    let rapid = |start: Option<(f64, f64, f64)>,
                 end: (f64, f64, f64),
                 sub: Option<u32>,
                 feed120: bool| Op::Rapid {
        start: match start {
            Some((x, y, z)) => StartPoint::Known(p3(x, y, z)),
            None => StartPoint::Unknown,
        },
        end: p3(end.0, end.1, end.2),
        state: if feed120 {
            st120.clone()
        } else {
            st_none.clone()
        },
        src: span0(),
        sub,
    };
    let linear = |start: (f64, f64, f64), end: (f64, f64, f64), sub: Option<u32>| Op::Linear {
        start: StartPoint::Known(p3(start.0, start.1, start.2)),
        end: p3(end.0, end.1, end.2),
        state: st120.clone(),
        src: span0(),
        sub,
    };
    let prog = Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: eid_core::ast::PathMode::ExactPath,
        frame_preset: None,
        ops: vec![
            // G0 X0 Y0 Z5 (до циклов; feed ещё не установлен).
            rapid(None, (0.0, 0.0, 5.0), None, false),
            // Кадр цикла 1: traverse sub=1 → подход sub=2 → feed sub=3 →
            // отвод sub=4 (канон-шаблон §7.7, все несут feed кадра).
            rapid(Some((0.0, 0.0, 5.0)), (10.0, 0.0, 5.0), Some(1), true),
            rapid(Some((10.0, 0.0, 5.0)), (10.0, 0.0, 2.0), Some(2), true),
            linear((10.0, 0.0, 2.0), (10.0, 0.0, -3.0), Some(3)),
            rapid(Some((10.0, 0.0, -3.0)), (10.0, 0.0, 2.0), Some(4), true),
            // Кадр цикла 2: feed модальный НЕ изменился (120.0) — traverse
            // sub=1 всё равно обязан переизлучить F (суть v0.18).
            rapid(Some((10.0, 0.0, 2.0)), (20.0, 0.0, 2.0), Some(1), true),
            rapid(Some((20.0, 0.0, 2.0)), (20.0, 0.0, 3.0), Some(2), true),
            linear((20.0, 0.0, 3.0), (20.0, 0.0, -3.0), Some(3)),
            rapid(Some((20.0, 0.0, -3.0)), (20.0, 0.0, 3.0), Some(4), true),
            Op::ProgramEnd { src: span0() },
        ],
    };
    let ad = GrblAdapter::new();
    let text = ad.emit(&prog).expect("emit ok").text;
    // Второй блок: traverse с F (sub=1 переизлучение при неизменном значении),
    // подход/feed/отвод без F — только traverse несёт F120.
    let second_block: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("G0 X20.0000"))
        .collect();
    assert_eq!(
        second_block.first(),
        Some(&"G0 X20.0000 Y0.0000 Z2.0000 F120.0000"),
        "первый производный оп кадра с F:\n{text}"
    );
    assert_eq!(
        second_block.len(),
        3,
        "traverse + подход + отвод (только traverse с F):\n{text}"
    );
    let f_count = text.lines().filter(|l| l.contains(" F120.0000")).count();
    assert_eq!(f_count, 2, "оба блока с F:\n{text}");
    // Детерминизм: emit дважды байт-в-байт.
    let text2 = ad.emit(&prog).expect("emit ok 2").text;
    assert_eq!(text, text2, "emit недетерминирован");
    // R1-семантика: F-переизлучение на sub=1 не меняет AST при парс-бэке
    // (feed модален) — semantic_diff = 0 против исходного АСТа.
    let reparsed = GrblAdapter::new()
        .parse(&text, "t_reparse.ngc")
        .expect("reparse")
        .program;
    let d = semantic_diff(&prog, &reparsed, &DiffConfig::default());
    assert!(d.is_empty(), "round-trip divergences: {d:?}");
}

// ===========================================================================
// Мутационный раунд v0.18 (агент D): free-fn-обёртка emit, отклонение
// перевёрнутой нормали, F-слово не повторяется при том же feed, дубликат PEND.
// ===========================================================================

#[test]
fn mround_grbl_free_fn_emit_wrapper() {
    // Свободная функция eid_grbl::emit::emit = обёртка emit_with_warnings
    // (ни один тест её не вызывал — FnValue-мутанты выживали).
    let a = parse_grbl(&corpus(
        "03_feed_survives_rapid/feed_survives_rapid.grbl.ngc",
    ));
    let text = eid_grbl::emit::emit(&a).expect("free emit ok");
    assert!(text.contains("G21"), "{text}");
    assert!(text.contains("G0") || text.contains("G1"), "{text}");
}

#[test]
fn mround_grbl_flipped_normal_rejected() {
    // plane_of: отрицательная ось (−1,0,0) — не канон (+X/+Y/+Z) → Err
    // (guard-мутанты `(v−1).abs() < 1e-9 → true` принимали любую (v,0,0)).
    let mut p = parse_grbl(&corpus("11_arc_R_semicircle/arc_R_semicircle.grbl.ngc"));
    for op in &mut p.ops {
        if let Op::Arc { normal, .. } = op {
            normal.x = eid_core::ast::Num::new("-1".into(), -1.0);
        }
    }
    let Err(d) = GrblAdapter::new().emit(&p) else {
        panic!("перевёрнутая нормаль обязана быть отклонена")
    };
    assert!(d.iter().any(|x| x.code == ErrorCode::InvalidInput), "{d:?}");
    assert!(d.iter().any(|x| x.message.contains("+axis")), "{d:?}");
}

#[test]
fn restated_f_reemitted_from_ast_flag() {
    // T8 (v0.2 волна 3): restated-F из AST — F на каждом кадре с F-словом,
    // включая тот же значение (GRBL излучает F даже на rapid-кадре).
    let ad = GrblAdapter::new();
    let p = ad
        .parse("G21\nG90\nG1 X10. F100.\nG1 X20. F100.\nM2\n", "t.ngc")
        .expect("parse ok")
        .program;
    let text = ad.emit(&p).expect("emit ok").text;
    let f_count = text.matches("F100").count();
    assert_eq!(f_count, 2, "restated-F переизлучён:\n{text}");
}

#[test]
fn inherited_f_not_reemitted() {
    // T8 (б): наследованный feed (нет F-слова) → F ровно одно.
    let ad = GrblAdapter::new();
    let p = ad
        .parse("G21\nG90\nG1 X10. F100.\nG1 X20.\nM2\n", "t.ngc")
        .expect("parse ok")
        .program;
    let text = ad.emit(&p).expect("emit ok").text;
    let f_count = text.matches("F100").count();
    assert_eq!(f_count, 1, "наследованный кадр без F:\n{text}");
}

#[test]
fn mround_grbl_feed_word_not_repeated() {
    // F-правило v0.18: F эмиссится при СМЕНЕ значения или на sub=1; два хода
    // с тем же feed → F-слово РОВНО одно (`==`-мутант инвертировал условие).
    let ad = GrblAdapter::new();
    let p = ad
        .parse("G21\nG90\nG1 X10. F100.\nG1 X20.\nG1 X30.\nM2\n", "t.ngc")
        .expect("parse ok")
        .program;
    let text = ad.emit(&p).expect("emit ok").text;
    let f_count = text.matches("F100").count();
    assert_eq!(f_count, 1, "F при том же feed не повторяется:\n{text}");
}

#[test]
fn mround_grbl_duplicate_program_end_rejected() {
    // Два PEND → program_end_duplicate на эмиссии (parse свой guard уже
    // отработал — здесь hand-built AST, defense-in-depth валидатора;
    // `> with <`-мутант менял условие).
    let ad = GrblAdapter::new();
    let mut p = ad
        .parse("G21\nG1 X10. F100.\nM2\n", "t.ngc")
        .expect("parse ok")
        .program;
    p.ops.push(Op::ProgramEnd {
        src: eid_core::ast::SrcSpan {
            file: "t.ngc".into(),
            line_start: 99,
            col_start: 1,
            line_end: 99,
            col_end: 1,
        },
    });
    let Err(d) = ad.emit(&p) else {
        panic!("дубликат program_end обязан быть отклонён")
    };
    assert!(d.iter().any(|x| x.message.contains("duplicate")), "{d:?}");
}
