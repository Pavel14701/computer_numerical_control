//! Тесты эмиссии GRBL (Агент C, итерация 3). Round-trip R1: parse → emit →
//! parse — semantic_diff = 0 (eid-harness, допуски EQUIVALENCE §4).
//! Диалектные файлы — рукописные v0: byte-exact НЕ гарантирован, сравнение
//! только AST-уровня (бриф п.9); emit дважды — байт-в-байт.

use eid_core::adapter::{Adapter, EidTextAdapter};
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
    GrblAdapter::new().parse(src, "t.ngc").expect("grbl parse ok")
}

/// R1 round-trip: parse → emit → parse → semantic_diff = 0; emit дважды
/// байт-в-байт (детерминизм, бриф п.8).
fn roundtrip(rel: &str) {
    let src = corpus(rel);
    let a = parse_grbl(&src);
    let ad = GrblAdapter::new();
    let text1 = ad.emit(&a).expect("emit ok");
    let text2 = ad.emit(&a).expect("emit ok");
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
    // Дуга отброшена с notice на обеих сторонах round-trip → diff 0.
    roundtrip("13_arc_degenerate_zero_sweep/arc_degenerate_zero_sweep.grbl.ngc");
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
    let text = GrblAdapter::new().emit(&a).expect("emit ok");
    assert!(text.contains("$$\r\n"), "CRLF при $$ потерян: {text:?}");
    assert!(!text.contains("$$\r\n\n"), "двойной терминатор после Raw");
}

#[test]
fn emit_never_uses_r_form() {
    // EQUIVALENCE §4 политика: R не эмитится никогда — только IJK-пара.
    let a = parse_grbl(&corpus("11_arc_R_semicircle/arc_R_semicircle.grbl.ngc"));
    let text = GrblAdapter::new().emit(&a).expect("emit ok");
    let arc_line = text.lines().find(|l| l.starts_with("G2 ")).expect("arc line");
    assert!(arc_line.contains('I') && arc_line.contains('J'), "{arc_line}");
    assert!(!arc_line.contains(" R"), "{arc_line}");
}

#[test]
fn vector_27_tool_change_is_syntax() {
    // eid-core magic v0.3 (commit 655d194) — golden.eid снова через R1-парсер.
    let golden = EidTextAdapter::new()
        .parse(&corpus("27_tool_change/golden.eid"), "27.golden.eid")
        .expect("golden parses");
    let Err(diags) = GrblAdapter::new().emit(&golden) else {
        panic!("#27: ожидался Err(E_UNSUPPORTED_SYNTAX)");
    };
    assert!(diags.iter().all(|d| d.code == ErrorCode::UnsupportedSyntax));
}

#[test]
fn vector_36_preset_order_e14() {
    // EQUIVALENCE §1.4: заголовок → выборка preset.wcs → G92 — ДО всякого хода;
    // без «запекания» в G10.
    let a = parse_grbl(&corpus("36_preset_G54_then_switch/preset_G54_then_switch.grbl.ngc"));
    let text = GrblAdapter::new().emit(&a).expect("emit ok");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "G21 G90 G94", "заголовок всегда явно");
    assert_eq!(lines[1], "G54", "выборка preset.wcs — строка 2");
    assert_eq!(lines[2], "G92 X0.0000 Y0.0000 Z5.0000", "G92 — строка 3, 4dp");
    let first_motion = lines.iter().position(|l| l.starts_with("G0 ")).expect("есть ход");
    let g92 = lines.iter().position(|l| l.starts_with("G92")).expect("G92 есть");
    assert!(g92 < first_motion, "G92 обязан быть до первого хода");
    assert!(!text.contains("G10 L2 P1 X0.0000 Y0.0000 Z5.0000"), "запрет запекания в G10");
}

#[test]
fn negative_n3_config_verbatim() {
    // RAW: payload hex → исходные байты verbatim (same-dialect fidelity).
    let rel = "n3_raw_vendor_block/raw_vendor_block.grbl.ngc";
    let a = parse_grbl(&corpus(rel));
    let text = GrblAdapter::new().emit(&a).expect("emit ok");
    assert!(text.contains("$110=250.000"), "{text}");
    roundtrip(rel);
}

#[test]
fn raw_crlf_preserved_byte_exact() {
    // CRLF сохраняется байт-в-байт (бриф п.7; #47 не в GRBL-наборе — синтетика).
    let src = "$110=250.000\r\nG21 G90 G94 G17\nG0 X1.0 Y1.0 Z1.0\nM2\n";
    let a = parse_grbl(src);
    let text = GrblAdapter::new().emit(&a).expect("emit ok");
    assert!(text.contains("$110=250.000\r\n"), "CRLF потерян: {text:?}");
}

#[test]
fn g53_prefix_and_g10_define_roundtrip() {
    // frame=machine → префикс "G53 " на строке хода; WcsDefine → G10 L2 P..
    let src = "G21 G90 G94 G17\nG10 L2 P1 X1.0 Y2.0 Z3.0\nG0 X1.0 Y0 Z0\nG53 G0 X0.0 Y0.0 Z-5.0\nG0 X1.0 Y0 Z0\nM2\n";
    let a = parse_grbl(src);
    let text = GrblAdapter::new().emit(&a).expect("emit ok");
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
    let text = GrblAdapter::new().emit(&a).expect("emit ok");
    let arc_line = text.lines().find(|l| l.starts_with("G3")).expect("arc line");
    assert!(arc_line.contains("I") && arc_line.contains("K"), "{arc_line}");
    assert!(text.contains("G18\n"), "плоскость перед дугой");
    let b = parse_grbl(&text);
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    assert!(d.is_empty(), "G18 round-trip: {d:?}");
}

#[test]
fn dwell_and_stops_emission() {
    let src = "G21 G90 G94\nG0 X1 Y1 Z1\nG4 P1.5\nM0\nM2\n";
    let a = parse_grbl(src);
    let text = GrblAdapter::new().emit(&a).expect("emit ok");
    assert!(text.contains("G4 P1.5000"), "{text}");
    assert!(text.contains("M0"), "{text}");
    assert!(text.lines().last().expect("non-empty") == "M2", "M2 — последняя строка");
}

// ---------------------------------------------------------------------------
// EmissionLegality — ConversionTarget-таблица (итерация 4)
// ---------------------------------------------------------------------------

fn legality_table() -> eid_grbl::emit::EmissionLegality {
    GrblAdapter::new().emission_legality()
}

#[test]
fn legality_table_matches_manifest_r4_expect() {
    // Инвариант (не числа): каждый Code из r4_expect["grbl-1.1"] обязан быть
    // объявлен в таблице легальности; исключение — E_RAW_UNTRANSLATABLE
    // (translate-путь, Агент E / SPEC §8.2 — вне same-dialect эмиссии).
    // Обратно: каждый SYNTAX-класс таблицы должен быть покрыт хотя бы одним
    // manifest-вектором; INVALID-класс таблицы — только канон-форма (§3),
    // это инварианты AST→emit, корпусом не ожидается.
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/manifest.toml");
    let manifest = eid_harness::manifest::load(&manifest_path).expect("manifest loads");
    let table = legality_table();

    let mut manifest_codes: Vec<ErrorCode> = Vec::new();
    for v in &manifest.vectors {
        if let Some(outcome) = v.r4_expect.get("grbl-1.1") {
            if let eid_harness::manifest::Outcome::Code(code) = outcome {
                manifest_codes.push(*code);
            }
        }
    }
    assert!(!manifest_codes.is_empty(), "manifest: нет r4_expect для grbl-1.1");

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
                "SYNTAX-условие `{cond}` таблицы не покрыто manifest r4_expect"
            );
        } else {
            assert_eq!(
                *code, ErrorCode::InvalidInput,
                "условие `{cond}`: не-SYNTAX класс таблицы обязан быть канон-формой (INVALID)"
            );
        }
    }
}

#[test]
fn legality_negative_tool_change_via_table() {
    // Отказ #27 идёт ЧЕРЕЗ таблицу: код диагностики == code_of("tool_change").
    let golden = EidTextAdapter::new()
        .parse(&corpus("27_tool_change/golden.eid"), "27.golden.eid")
        .expect("golden parses");
    let table = legality_table();
    let expected = table.code_of("tool_change").expect("tool_change в таблице");
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
        "rapid", "linear", "arc", "dwell", "wcs_define", "wcs_set_here",
        "frame_preset", "program_stop", "program_end", "raw",
    ] {
        assert!(table.is_legal_kind(kind), "`{kind}` отсутствует в legal");
    }
    assert!(!table.is_legal_kind("tool_change"));
    // illegal: условия уникальны.
    let mut conds: Vec<&str> = table.illegal.iter().map(|(c, _)| *c).collect();
    conds.sort_unstable();
    conds.dedup();
    assert_eq!(conds.len(), table.illegal.len(), "дубли условий в illegal");
}

#[test]
fn coolant_29_mist_flood_none() {
    // CORPUS #29 (Adapter-wrong, дельта coolant): переход flood+mist → none
    // обязан дать M9 перед 4-м ходом (StateField{coolant}@3 — красный B);
    // все переходы к none → M9, включения → M7/M8, частичные выключения →
    // M9 + повтор оставшегося.
    roundtrip("29_coolant_mist_flood/coolant_mist_flood.grbl.ngc");
    let golden = EidTextAdapter::new()
        .parse(&corpus("29_coolant_mist_flood/golden.eid"), "29.golden.eid")
        .expect("golden parses");
    let text = GrblAdapter::new().emit(&golden).expect("emit ok");
    assert!(text.contains("M9\nG1 X30.0000 Y0.0000 Z5.0000"), "{text}");
    // v1-пара — байт-в-байт emit(golden) (регенерация подтверждена).
    let v1_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/tier_a/v1/29_coolant_mist_flood/coolant_mist_flood.grbl.ngc");
    let v1 = std::fs::read_to_string(&v1_path).expect("v1 pair exists");
    assert_eq!(v1, text, "v1-пара #29 не равна emit(golden)");
}
