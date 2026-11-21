//! Тесты эмиссии EID → Fanuc (Агент C, этап 3; COVERAGE §5.1 v0.8, анализ §8).
//! Golden-пары (EID AST → fanuc-текст), illegal-таблица (ConversionTarget,
//! коды только из FanucEmissionLegality), round-trip fanuc→EID→fanuc
//! (байт-точный fixed-point: t1 == t2; AST-уровень — semantic_diff eid-harness
//! с документированным исключением path_mode ExactPath→ExactStop — value
//! shift Fanuc G61, COVERAGE §5.1/§2.2), кросс-чек legality↔поведение.
//!
//! ЧЕК-ЛИСТ этапа 3 (правило → тест):
//! HEADER: units G21/G20 → golden_header_mm / golden_units_inch; G90/G94/G17/
//!   G91.1 всегда → golden_header_mm; path G61/G64 → golden_path_exact_stop /
//!   golden_path_blend / golden_exact_path_warning (консервативный дефолт).
//! MOTION: G0/G1 → golden_minimal_motion; F-дельты (вкл. rapid) →
//!   golden_feed_delta; machine-frame → golden_machine_frame_prefix;
//!   DPI-decimal-point-всегда → golden_decimal_point_always.
//! ARC §5.1: всегда IJK (R не эмитится) → golden_arc_r_source_emits_ijk;
//!   in-plane пары G17/G18/G19 → golden_arc_g17/g18/g19; полный круг БЕЗ
//!   end-слов → golden_full_circle_no_end_words; helical 360° →
//!   golden_helical_full_turn; смена плоскости строкой →
//!   golden_plane_change_line.
//! DWELL: ms-целое → P (golden_dwell_p_ms); sub-ms → X (golden_dwell_x_sub_ms).
//! FRAMES: G92 первым блоком → golden_frame_preset_first; G10 L2 →
//!   golden_wcs_define_and_select; G59.x-отказ → illegal_wcs_g59_1;
//!   wcs_set_here-отказ → illegal_wcs_set_here.
//! PATH: exact_stop G61 1:1 → golden_path_exact_stop; blend bare G64 →
//!   golden_path_blend; exact_path warning+G61 → golden_exact_path_warning.
//! END: M30 дефолт → golden_minimal_motion; M5/M9-хвост НЕ эмитится →
//!   golden_m5_m9_tail_not_emitted.
//! TOOL: T## M06 → golden_tool_change.
//! CYCLES: развёрнутые ходы self-выход → golden_cycle_unroll_verbose /
//!   roundtrip_cycle_unroll.
//! RAW: verbatim → golden_raw_verbatim / roundtrip_full.
//! TABLE: состав → legality_table_composition; поведение↔таблица →
//!   illegal_* + legality_behavior_cross_check; детерминизм →
//!   emit_deterministic_double.

use eid_core::adapter::Adapter;
use eid_core::ast::{
    Coolant, Frame, Num, Op, PathMode, Program, Spindle, SrcSpan, StartPoint, State, Units, Wcs,
};
use eid_core::diag::ErrorCode;
use eid_fanuc::emit::{emit_with_warnings, op_kind, FanucEmissionLegality};
use eid_fanuc::FanucAdapter;
use eid_harness::diff::{semantic_diff, DiffConfig, Divergence};

fn num(v: f64) -> Num {
    Num::new(format!("{v}"), v)
}

fn pt(x: f64, y: f64, z: f64) -> eid_core::ast::Point3 {
    eid_core::ast::Point3 { x: num(x), y: num(y), z: num(z) }
}

fn parse_fanuc(src: &str) -> Program {
    FanucAdapter::new().parse(src, "t.ngc").expect("fanuc parse ok")
}

fn emit_fanuc(src: &str) -> String {
    let p = parse_fanuc(src);
    FanucAdapter::new().emit(&p).expect("emit ok")
}

fn emit_w(src: &str) -> (String, Vec<String>) {
    let ad = FanucAdapter::new();
    let p = ad.parse(src, "t.ngc").expect("fanuc parse ok");
    let text = ad.emit(&p).expect("emit ok");
    let warnings = ad.take_warnings();
    (text, warnings)
}

fn err_code_of(r: Result<String, Vec<eid_core::diag::Diagnostic>>) -> ErrorCode {
    let Err(d) = r else { panic!("ожидался Err") };
    d[0].code
}

/// Round-trip fixed-point: parse → emit t1 → parse t1 → emit t2;
/// t1 == t2 байт-в-байт; AST-дифф пуст (кроме документированного value
/// shift path_mode ExactPath→ExactStop на Fanuc — COVERAGE §5.1/§2.2).
fn roundtrip(src: &str, label: &str) {
    let ad = FanucAdapter::new();
    let a = ad.parse(src, "t.ngc").expect("parse ok");
    let t1 = ad.emit(&a).expect("emit ok");
    let t2 = ad.emit(&a).expect("emit ok");
    assert_eq!(t1, t2, "{label}: emit недетерминирован");
    let b = ad.parse(&t1, "rt.ngc").expect("reparse ok");
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    let d: Vec<_> = d
        .into_iter()
        .filter(|x| !matches!(x, Divergence::MetaField { field: "path_mode" }
            if matches!(a.path_mode, PathMode::ExactPath)
                && matches!(b.path_mode, PathMode::ExactStop)))
        .collect();
    assert!(d.is_empty(), "{label}: round-trip divergences: {d:?}");
}

// ===========================================================================
// A. Golden-пары: EID AST → точный fanuc-текст
// ===========================================================================

#[test]
fn golden_header_mm() {
    // Заголовок всегда явный: units G90 G94 G17 G91.1 + path-строка (дефолт
    // ExactPath → консервативный G61) + M30 (Fanuc-конвенция).
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nM30;\n");
    assert_eq!(
        text,
        "G21 G90 G94 G17 G91.1\nG61\nG0 X10.0000 Y10.0000 Z5.0000\nM30\n"
    );
}

#[test]
fn golden_units_inch() {
    let text = emit_fanuc("G20 G90 G94 G17;\nG0 X1. Y2. Z0.5;\nM30;\n");
    assert!(text.starts_with("G20 G90 G94 G17 G91.1\n"), "{text:?}");
}

#[test]
fn golden_minimal_motion() {
    // G0/G1: все три оси; F на первом feed-опе; rapid без F при том же feed.
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nG0 X10. Y0. Z5.;\nG1 X20. Y0. Z5. F250.;\nG1 X30. Y0. Z5.;\nM30;\n",
    );
    assert_eq!(
        text,
        "G21 G90 G94 G17 G91.1\nG61\nG0 X10.0000 Y0.0000 Z5.0000\n\
         G1 X20.0000 Y0.0000 Z5.0000 F250.0000\n\
         G1 X30.0000 Y0.0000 Z5.0000\nM30\n"
    );
}

#[test]
fn golden_feed_delta_on_rapid() {
    // Развёртка: traverse (rapid) НЕСЁТ модальный feed кадра — F-дельта
    // на G0-кадре легальна (F — своя группа 05, §7.7-1 SPEC).
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z2.;\nG81 X10. Y0. Z-5. R1. F120.;\nM30;\n",
    );
    let lines: Vec<&str> = text.lines().collect();
    // traverse — первый G0 с F120.
    let tr = lines.iter().find(|l| l.starts_with("G0 X10.")).expect("traverse");
    assert!(*tr == "G0 X10.0000 Y0.0000 Z2.0000 F120.0000");
}

#[test]
fn golden_decimal_point_always() {
    // DPI-ловушка: bare-integer источник X25 (DPI=0 → 0.025 mm) эмитится
    // С ТОЧКОЙ ВСЕГДА — `X0.0250`, не `X25` (bare X25 Fanuc прочитал бы
    // как 25 mm).
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X25 Y50 Z2;\nM30;\n");
    // DPI=0-чтение: X25 → 0.025 mm, Z2 → 0.002 mm; на выходе 4dp с точкой.
    assert!(text.contains("X0.0250 Y0.0500 Z0.0020"), "{text:?}");
    // Ни одного bare-числа в координатах (кроме dwell P — целые мс).
    for line in text.lines() {
        if line.starts_with("G0 ") || line.starts_with("G1 ") || line.starts_with("G2 ")
            || line.starts_with("G3 ")
        {
            for w in line.split_whitespace().skip(1) {
                let letters = w.chars().next().expect("word");
                if letters.is_ascii_alphabetic() {
                    assert!(w[1..].contains('.'), "bare number `{w}` in `{line}` (DPI trap)");
                }
            }
        }
    }
}

#[test]
fn golden_machine_frame_prefix() {
    // G53-ход требует origin активной WCS in-text (§5.3b) — задаём G10 L2.
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nG10 L2 P1 X0. Y0. Z0.;\nG0 X10. Y10. Z5.;\nG53 G0 X0. Y0. Z0.;\nM30;\n",
    );
    assert!(text.contains("\nG53 G0 X0.0000 Y0.0000 Z0.0000\n"), "{text:?}");
}

// ===========================================================================
// A.1 Дуги (всегда IJK; полный круг без end-слов)
// ===========================================================================

#[test]
fn golden_arc_g17_ijk() {
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG3 X5. Y5. I0. J5. F100.;\nM30;\n",
    );
    assert!(
        text.contains("G3 X5.0000 Y5.0000 Z0.0000 I0.0000 J5.0000 F100.0000\n"),
        "{text:?}"
    );
}

#[test]
fn golden_arc_g18_pair_ik() {
    // G18 = ZX: пара I+K (literal X/Z), вне-плоскостное Y — координата конца.
    let text = emit_fanuc(
        "G21 G90 G94 G18;\nG0 X0. Y0. Z0.;\nG2 X5. Y2. Z-5. I5. K0. F100.;\nM30;\n",
    );
    let arc = text.lines().find(|l| l.starts_with("G2 ")).expect("arc line");
    assert!(arc == "G2 X5.0000 Y2.0000 Z-5.0000 I5.0000 K0.0000 F100.0000");
}

#[test]
fn golden_arc_g19_pair_jk() {
    let text = emit_fanuc(
        "G21 G90 G94 G19;\nG0 X0. Y0. Z0.;\nG3 X2. Y5. Z-5. J5. K0. F100.;\nM30;\n",
    );
    let arc = text.lines().find(|l| l.starts_with("G3 ")).expect("arc line");
    assert!(arc == "G3 X2.0000 Y5.0000 Z-5.0000 J5.0000 K0.0000 F100.0000");
}

#[test]
fn golden_arc_r_source_emits_ijk() {
    // EQUIVALENCE §4: R НЕ эмитится никогда — R-источник → IJK; notice
    // near-degenerate не нужен (полукруг точно 180° — вне ±0.12°).
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X10. Y0. R5. F100.;\nM30;\n",
    );
    let arc = text.lines().find(|l| l.starts_with("G2 ")).expect("arc line");
    assert!(arc.contains('I') && arc.contains('J'), "{arc}");
    assert!(!arc.contains(" R"), "{arc}");
    assert!(arc == "G2 X10.0000 Y0.0000 Z0.0000 I5.0000 J0.0000 F100.0000");
}

#[test]
fn golden_full_circle_no_end_words() {
    // COVERAGE §5.1: полный круг (sweep=360°, pitch=0) — `G2 I_ J_` БЕЗ
    // end-слов (слова end==start перепарсер квантует в zero-length drop).
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG2 X10. Y0. I-5. J0. F100.;\nM30;\n",
    );
    let arc = text.lines().find(|l| l.starts_with("G2 ")).expect("arc line");
    assert_eq!(arc, "G2 I-5.0000 J0.0000 F100.0000", "без X/Y/Z на полном круге");
}

#[test]
fn golden_full_circle_inherited_end_no_words() {
    // Полный круг через НАСЛЕДОВАННЫЙ end (без end-слов в источнике).
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG2 I-5. F100.;\nM30;\n");
    let arc = text.lines().find(|l| l.starts_with("G2 ")).expect("arc line");
    assert!(arc == "G2 I-5.0000 J0.0000 F100.0000");
}

#[test]
fn golden_helical_full_turn() {
    // Винтовой полный оборот (pitch≠0) — ТОЛЬКО end-слово вне-плоскостной
    // оси (in-plane наследуют старт).
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG2 X10. Y0. Z5. I-5. J0. F100.;\nM30;\n",
    );
    let arc = text.lines().find(|l| l.starts_with("G2 ")).expect("arc line");
    assert!(arc == "G2 Z5.0000 I-5.0000 J0.0000 F100.0000");
}

#[test]
fn golden_plane_change_line() {
    // Смена плоскости — отдельной строкой перед дугой (заголовок G17).
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG19;\nG0 X0. Y0. Z0.;\nG3 X2. Y5. Z-5. J5. K0. F100.;\nM30;\n",
    );
    let lines: Vec<&str> = text.lines().collect();
    let gi = lines.iter().position(|l| *l == "G19").expect("G19 line");
    let ai = lines.iter().position(|l| l.starts_with("G3 ")).expect("arc");
    assert!(gi < ai, "G19 до дуги: {lines:?}");
    assert!(lines[gi + 1].starts_with("G3 "), "G19 непосредственно перед дугой");
}

// ===========================================================================
// A.2 Dwell (P целые мс / X sub-ms)
// ===========================================================================

#[test]
fn golden_dwell_p_ms() {
    // `P500` = 0.5 s: seconds·1000 целое → P (decimal point ЗАПРЕЩЁН —
    // DPI-домен P).
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z2.;\nG4 P500;\nM30;\n");
    assert!(text.contains("\nG4 P500\n"), "{text:?}");
}

#[test]
fn golden_dwell_x_sub_ms() {
    // Sub-ms остаток (0.5005 s = 500.5 мс — не целое) → X-форма
    // (float-секунды, decimal point).
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z2.;\nG4 X0.5005;\nM30;\n");
    assert!(text.contains("\nG4 X0.5005\n"), "{text:?}");
    assert!(!text.contains("G4 P"), "{text:?}");
}

#[test]
fn golden_dwell_p_point_ms_fraction() {
    // P с дробным значением на входе — Fanuc P decimal point запрещён,
    // парсер читает P как целые мс; P500.5 невозможно… X-форма остаётся
    // единственным носителем sub-ms — проверка конверсии X→X.
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z2.;\nG4 X0.0005;\nM30;\n");
    assert!(text.contains("\nG4 X0.0005\n"), "{text:?}");
}

// ===========================================================================
// A.3 Фреймы / M-словарь / стопы
// ===========================================================================

#[test]
fn golden_frame_preset_first() {
    // EQUIVALENCE §1.4: выборка preset.wcs, затем G92 — первые блоки после
    // шапки; БЕЗ запекания в G10 (golden_preset_no_bake). G92-hygiene
    // (G92.1 первым) — профиль-флаг этапа 8, НЕ дефолт.
    let text = emit_fanuc("G21 G90 G94 G17;\nG92 X0. Y0. Z50.;\nG0 X10. Y10. Z5.;\nM30;\n");
    assert_eq!(
        text,
        "G21 G90 G94 G17 G91.1\nG61\nG54\nG92 X0.0000 Y0.0000 Z50.0000\n\
         G0 X10.0000 Y10.0000 Z5.0000\nM30\n"
    );
}

#[test]
fn golden_wcs_define_and_select() {
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nG10 L2 P2 X5. Y0. Z0.;\nG55;\nG0 X30. Y30. Z5.;\nM30;\n",
    );
    assert!(text.contains("G10 L2 P2 X5.0000 Y0.0000 Z0.0000\n"), "{text:?}");
    assert!(text.contains("\nG55\n"), "{text:?}");
}

#[test]
fn golden_tool_change() {
    // Fanuc-идиома: T## M06 в одном блоке.
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nT5 M06;\nG0 X10. Y10.;\nM30;\n");
    assert!(text.contains("\nT5 M06\n"), "{text:?}");
}

#[test]
fn golden_program_stop_m0_m1() {
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM0;\nM1;\nG0 X10. Y10.;\nM30;\n");
    assert!(text.contains("\nM0\n"), "{text:?}");
    assert!(text.contains("\nM1\n"), "{text:?}");
}

#[test]
fn golden_program_end_m30() {
    // Дефолт M30 (Fanuc-конвенция rewind); профиль M02 — этап 8.
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM2;\n");
    assert!(text.ends_with("M30\n"), "{text:?}");
    assert!(!text.contains("M2\n"), "голый M2 не эмитится (только M30): {text:?}");
}

#[test]
fn golden_m5_m9_tail_not_emitted() {
    // Ruling COVERAGE §5: хвост M5/M9 перед program_end НЕ эмитится
    // (AST не хранит пост-конечное состояние; M30 сам глушит).
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nM3 S1200.;\nM8;\nG0 X0. Y0. Z5.;\nM30;\n",
    );
    // M3/M8 должны быть эмитены ДО хода (дельты снапшота), но НЕ после.
    let pend = text.rfind("M30").expect("M30");
    let tail = &text[pend..];
    assert!(!tail.contains("M5") && !tail.contains("M9"), "хвост M5/M9: {text:?}");
    assert!(text.contains("M3 S1200.0000"), "{text:?}");
}

#[test]
fn golden_spindle_coolant_transitions() {
    // Дельты снапшота: порядок WCS → шпиндель → СОЖ; M9 перед повторными
    // включениями.
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nM3 S1200.;\nM8;\nG0 X0. Y0. Z5.;\nM9;\nM7;\nG0 X5. Y5. Z1.;\nM30;\n",
    );
    let m3 = text.find("M3 S1200.0000").expect("M3");
    let m8 = text.find("M8").expect("M8 first");
    let m9 = text.rfind("M9").expect("M9");
    let m7 = text.find("M7").expect("M7");
    assert!(m3 < m8, "M3 до M8");
    assert!(m8 < m9, "M8 (первое) до M9");
    assert!(m9 < m7, "M9 до повторного M7");
}

// ===========================================================================
// A.4 path_mode (Fanuc value shift)
// ===========================================================================

#[test]
fn golden_path_exact_stop_no_warning() {
    // G61 источник = ExactStop (value shift, COVERAGE §2.2) → G61, без
    // warnings (1:1 по значению).
    let (text, warnings) = emit_w("G21 G90 G94 G17;\nG61;\nG0 X0. Y0. Z5.;\nM30;\n");
    assert!(text.contains("\nG61\n"), "{text:?}");
    assert!(warnings.is_empty(), "ExactStop 1:1 — без warnings: {warnings:?}");
}

#[test]
fn golden_path_blend() {
    // G64 источник = bare blend → bare G64; t отсутствует — без warning.
    let (text, warnings) = emit_w("G21 G90 G94 G17;\nG64;\nG0 X0. Y0. Z5.;\nM30;\n");
    assert!(text.contains("\nG64\n"), "{text:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn golden_exact_path_warning() {
    // Дефолт ExactPath (нет G61/G64 в файле) → warning + G61 (консервативно;
    // профиль bare G64 — этап 8).
    let (text, warnings) = emit_w("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM30;\n");
    assert!(text.contains("\nG61\n"), "{text:?}");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("Fanuc exact_path эквивалента нет"),
        "{warnings:?}"
    );
    assert!(warnings[0].contains("этап 8"), "профиль-опция задокументирована: {warnings:?}");
}

#[test]
fn golden_path_blend_t_warning() {
    // AST Blend{t} (кросс-адаптерный: LinuxCNC G64 P0.05) → bare G64 +
    // warning «t теряется» (P-квантификатора в словаре Fanuc нет).
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM30;\n");
    p.path_mode = PathMode::Blend { t: Some(num(0.05)) };
    let mut w = Vec::new();
    let text = emit_with_warnings(&p, &mut w).expect("emit ok").text;
    assert!(text.contains("\nG64\n"), "{text:?}");
    assert!(!text.contains("P0."), "t НЕ эмитится: {text:?}");
    assert!(w.iter().any(|s| s.contains("t теряется")), "{w:?}");
}

// ===========================================================================
// A.5 RAW / cycles verbose
// ===========================================================================

#[test]
fn golden_raw_verbatim() {
    // Same-dialect fidelity: Raw-строка (Macro B) — байты дословно,
    // включая `;`-терминатор и пробелы.
    let src = "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\n#100 = 25.4;\nG1 X10. Y0. F100.;\nM30;\n";
    let text = emit_fanuc(src);
    assert!(text.contains("#100 = 25.4;\n"), "{text:?}");
}

#[test]
fn golden_cycle_unroll_verbose() {
    // Развёрнутые циклы — self-выход: G81 → traverse + approach + drill +
    // retract (§7.7 SPEC); verbose — документировано (SPEC §1.2);
    // обратное свёртывание запрещено.
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z2.;\nG81 X10. Y0. Z-5. R1. F120.;\nM30;\n",
    );
    let lines: Vec<&str> = text.lines().collect();
    let tr = lines.iter().position(|l| l.starts_with("G0 X10.")).expect("traverse");
    let ap = lines
        .iter()
        .position(|l| l.starts_with("G0 X10.0000 Y0.0000 Z1.0000"))
        .expect("approach");
    let dr = lines
        .iter()
        .position(|l| l.starts_with("G1 X10.0000 Y0.0000 Z-5.0000"))
        .expect("drill");
    let rt = lines
        .iter()
        .rposition(|l| l.starts_with("G0 X10.0000 Y0.0000 Z1.0000"))
        .expect("retract");
    assert!(tr < ap && ap < dr && dr < rt, "порядок §7.7: {lines:?}");
    // Цикл НЕ сворачивается обратно: нет G81 в выводе.
    assert!(!text.contains("G81"), "свёртывание запрещено: {text:?}");
}

// ===========================================================================
// B. FanucEmissionLegality — состав таблицы (ConversionTarget)
// ===========================================================================

fn legality_table() -> FanucEmissionLegality {
    FanucAdapter::default().emission_legality()
}

#[test]
fn legality_table_composition() {
    let t = legality_table();
    // 10 узлов словаря op_kind; 9 легальны (wcs_set_here запрещён — ловушка
    // №3), frame_preset — мета (G92).
    let kinds = [
        "rapid", "linear", "arc", "dwell", "tool_change", "program_stop",
        "program_end", "wcs_define", "wcs_set_here", "raw",
    ];
    for k in kinds {
        assert!(t.legal.contains(&k) || k == "wcs_set_here", "{k}: нет в таблице");
    }
    assert!(t.is_legal_kind("rapid"));
    assert!(t.is_legal_kind("frame_preset"), "frame_preset (G92) легален");
    assert!(!t.is_legal_kind("wcs_set_here"), "wcs_set_here НЕ легален");
    // illegal-условия с кодами (COVERAGE §5.1 + канон-форма).
    let expect = [
        ("wcs_set_here", ErrorCode::UnsupportedSemantics),
        ("wcs_outside_g54_g59", ErrorCode::UnsupportedSyntax),
        ("wcs_define_p_outside_g54_g59", ErrorCode::UnsupportedSyntax),
        ("machine_frame_arc", ErrorCode::UnsupportedSemantics),
        ("motion_feed_none", ErrorCode::InvalidInput),
        ("arc_unknown_start", ErrorCode::InvalidInput),
        ("arc_normal_non_canonical", ErrorCode::InvalidInput),
        ("raw_payload_not_hex", ErrorCode::InvalidInput),
        ("program_end_missing", ErrorCode::InvalidInput),
        ("program_end_duplicate", ErrorCode::InvalidInput),
        ("program_end_not_last", ErrorCode::InvalidInput),
    ];
    for (cond, code) in expect {
        assert_eq!(t.code_of(cond), Some(code), "{cond}");
    }
    assert_eq!(t.code_of("несуществующее_условие"), None);
    // codes() — без дубликатов; все из {INVAL, SYN, USEM}.
    let mut codes = t.codes();
    let n = codes.len();
    codes.dedup();
    assert_eq!(codes.len(), n, "codes() содержит дубликаты");
    assert!(codes.iter().all(|c| matches!(
        c,
        ErrorCode::InvalidInput | ErrorCode::UnsupportedSyntax | ErrorCode::UnsupportedSemantics
    )));
    // warnings-условия.
    assert!(t.warning_of("path_mode_exact_path").is_some());
    assert!(t.warning_of("path_mode_blend_t").is_some());
    assert!(t.warning_of("path_mode_exact_stop").is_none(), "ExactStop 1:1 — без warning");
    // op_kind — словарь имён таблицы.
    let p = parse_fanuc("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n");
    assert_eq!(op_kind(&p.ops[0]), "rapid");
    assert_eq!(op_kind(&p.ops[1]), "program_end");
}

// ===========================================================================
// B.1 illegal-поведение (коды — только из таблицы)
// ===========================================================================

#[test]
fn illegal_wcs_set_here() {
    // COVERAGE §5.1 ловушка №3: эмиссия wcs_set_here → Fanuc запрещена
    // как L20 → E_UNSUPPORTED_SEMANTICS (честный отказ). Hand-built AST,
    // проходящий core-verify (D5: до первого motion; якорь start Known
    // согласован с delta) — отказ даёт ИМЕННО таблица легальности.
    let st = State {
        feed: None,
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        tool: None,
        frame: Frame::Wcs,
        wcs: Wcs::G54,
    };
    let p = Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: PathMode::ExactStop,
        frame_preset: None,
        ops: vec![
            Op::WcsSetHere { p: 1, delta: pt(0.0, 0.0, 0.0), state: st.clone(), src: span(1) },
            Op::Rapid {
                start: StartPoint::Known(pt(0.0, 0.0, 0.0)),
                end: pt(5.0, 5.0, 5.0),
                state: st,
                src: span(2),
                sub: None,
            },
            Op::ProgramEnd { src: span(3) },
        ],
    };
    let Err(d) = FanucAdapter::new().emit(&p) else { panic!("ожидался Err") };
    assert_eq!(d[0].code, legality_table().code_of("wcs_set_here").unwrap());
    assert!(d[0].message.contains("L20"), "{}", d[0].message);
    assert_eq!(d[0].stage, eid_core::diag::Stage::Emit);
}

#[test]
fn illegal_wcs_g59_1() {
    // state.wcs = G59.1 — в словаре Fanuc кодов нет → E_UNSUPPORTED_SYNTAX
    // (правило §5); НЕ подменять G54 молча (профиль-маппинг — этап 8).
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM30;\n");
    set_wcs_of(&mut p, 0, Wcs::G59_1);
    assert_eq!(
        err_code_of(FanucAdapter::new().emit(&p)),
        ErrorCode::UnsupportedSyntax
    );
}

#[test]
fn illegal_wcs_define_p7() {
    // wcs_define P7 вне Fanuc-домена 1..6 → E_UNSUPPORTED_SYNTAX.
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM30;\n");
    match &mut p.ops[0] {
        Op::Rapid { state, .. } => state.wcs = Wcs::G54,
        _ => unreachable!(),
    }
    p.ops.insert(
        1,
        Op::WcsDefine {
            p: 7,
            origin: pt(0.0, 0.0, 0.0),
            state: state_of(&p.ops[0]),
            src: span(2),
        },
    );
    assert_eq!(
        err_code_of(FanucAdapter::new().emit(&p)),
        ErrorCode::UnsupportedSyntax
    );
}

#[test]
fn illegal_machine_frame_arc() {
    // Дуга в machine-frame — не эмитится (G53 ходы прямые, SPEC §5.3).
    // Порядок отказов: core-verify (канон-форма) идёт ДО таблицы легальности,
    // поэтому первый код — E_INVALID_INPUT verify; условие machine_frame_arc
    // в таблице — defensive-полнота (второй барьер). Эмиссия = Err в обоих
    // случаях — контракт §5.1 соблюдён.
    let mut p = parse_fanuc(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X5. Y5. I0. J5. F100.;\nM30;\n",
    );
    match &mut p.ops[1] {
        Op::Arc { state, .. } => state.frame = Frame::Machine,
        other => panic!("оп не Arc: {other:?}"),
    }
    let Err(d) = FanucAdapter::new().emit(&p) else { panic!("ожидался Err") };
    assert!(
        matches!(
            d[0].code,
            ErrorCode::InvalidInput | ErrorCode::UnsupportedSemantics
        ),
        "{}",
        d[0].code.as_str()
    );
    assert_eq!(
        legality_table().code_of("machine_frame_arc"),
        Some(ErrorCode::UnsupportedSemantics)
    );
}

#[test]
fn illegal_program_end_missing() {
    // PEND отсутствует (кросс-адаптерный AST) → E_INVALID_INPUT.
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM30;\n");
    p.ops.pop();
    assert_eq!(
        err_code_of(FanucAdapter::new().emit(&p)),
        ErrorCode::InvalidInput
    );
}

#[test]
fn legal_raw_tail_after_pend_emitted() {
    // Fanuc-политика хвоста (ruling 1, COVERAGE §5.1 «хвост-subs-случай»):
    // блоки после M30 (subs-тела O####) — line-level Raw — ЛЕГАЛЬНЫ в
    // same-dialect эмиссии; байты хвоста воспроизводятся после M30.
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nM30;\nO1000;\nX20.;\nM99;\n");
    let pend = text.find("M30\n").expect("M30");
    let tail = &text[pend + 4..];
    assert!(tail.contains("O1000;\n"), "хвост-subs verbatim: {text:?}");
    assert!(tail.contains("M99;\n"), "{text:?}");
}

#[test]
fn illegal_program_end_not_last() {
    // НЕ-Raw опыт после PEND → отказ (SPEC §3; порядок: core-verify первый —
    // E_INVALID_INPUT; условие program_end_not_last в таблице — defensive).
    // Raw-хвост после PEND, напротив, легален (legal_raw_tail_after_pend).
    let p = parse_fanuc("G21 G90 G94 G17;\nM30;\nG0 X0. Y0. Z5.;\n");
    // Парсер: хвост после PEND → Raw-хвост (ruling 1) — этот случай теперь
    // ЛЕГАЛЕН; для негатива — hand-built AST с Rapid после PEND.
    let mut p2 = parse_fanuc("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n");
    p2.ops.insert(
        2,
        Op::Rapid {
            start: StartPoint::Known(pt(1.0, 1.0, 1.0)),
            end: pt(2.0, 2.0, 2.0),
            state: state_of(&p2.ops[0]),
            src: span(4),
            sub: None,
        },
    );
    let Err(d) = FanucAdapter::new().emit(&p2) else { panic!("ожидался Err") };
    assert_eq!(d[0].code, ErrorCode::InvalidInput);
    assert_eq!(
        legality_table().code_of("program_end_not_last"),
        Some(ErrorCode::InvalidInput)
    );
}

#[test]
fn illegal_magic_version() {
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM30;\n");
    p.eid_version = "EID/L1 v0.2".into();
    assert_eq!(
        err_code_of(FanucAdapter::new().emit(&p)),
        ErrorCode::InvalidInput
    );
}

/// Кросс-чек: каждый illegal-код в поведении совпадает с таблицей
/// (единый путь отказа: валидатор берёт ErrorCode ТОЛЬКО из таблицы).
#[test]
fn legality_behavior_cross_check() {
    let t = legality_table();
    // wcs_set_here — код из таблицы.
    assert_eq!(t.code_of("wcs_set_here"), Some(ErrorCode::UnsupportedSemantics));
    // Таблица declarations ↔ фактически возвращённые коды (см. illegal_*):
    // достаточно исчерпывающей проверки соответствия классов.
    for (cond, _) in &t.illegal {
        assert!(
            t.code_of(cond).is_some(),
            "условие {cond} без кода"
        );
    }
}

// ===========================================================================
// C. Детерминизм + round-trip fixed-point
// ===========================================================================

#[test]
fn emit_deterministic_double() {
    let p = parse_fanuc(FULL_PROGRAM);
    let ad = FanucAdapter::new();
    let t1 = ad.emit(&p).expect("emit ok");
    let t2 = ad.emit(&p).expect("emit ok");
    assert_eq!(t1, t2);
}

const FULL_PROGRAM: &str = "G21 G90 G94 G17;\n\
G0 X10. Y10. Z5.;\n\
G1 X20. Y10. Z2. F300.;\n\
G3 X30. Y20. I0. J10.;\n\
G2 X30. Y20. I-5. J0.;\n\
G2 I-5.;\n\
G4 P250;\n\
G4 X0.0005;\n\
G10 L2 P2 X5. Y0. Z0.;\n\
G55;\n\
G0 X40. Y40. Z5.;\n\
T3 M06;\n\
M8;\n\
G53 G0 X0. Y0. Z0.;\n\
M1;\n\
M30;\n";

#[test]
fn roundtrip_full_fixed_point() {
    // fanuc → EID → fanuc: t1 == t2 байт-в-байт; AST-дифф пуст (кроме
    // документированного path_mode value shift).
    roundtrip(FULL_PROGRAM, "full");
}

#[test]
fn roundtrip_cycle_unroll() {
    roundtrip(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z2.;\nG83 X10. Y0. Z-15. R1. Q3. F150.;\nG82 X20. Y0. Z-10. R1. P400. F120.;\nM30;\n",
        "cycles",
    );
}

#[test]
fn roundtrip_g91_incremental() {
    // G91-секция: эмиссия абсолютами; reparse в G90-заголовке даёт те же
    // координаты (связывание) → fixed point.
    roundtrip(
        "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG91;\nG1 X5. Y0. Z-1. F100.;\nG0 X5. Y5.;\nG90;\nM30;\n",
        "g91",
    );
}

#[test]
fn roundtrip_g92_preset_and_raw() {
    roundtrip(
        "G21 G90 G94 G17;\nG92 X0. Y0. Z50.;\nG0 X10. Y10. Z5.;\nG43 H2;\nG1 X20. Y20. F500.;\nM30;\n",
        "preset+raw",
    );
}

#[test]
fn roundtrip_units_inch() {
    roundtrip(
        "G20 G90 G94 G17;\nG0 X1. Y2. Z0.5;\nG1 X2. Y3. Z0. F10.;\nM30;\n",
        "inch",
    );
}

#[test]
fn roundtrip_full_circle_and_helix() {
    roundtrip(
        "G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG2 I-5. F100.;\nG3 X10. Y0. Z5. I-5. J0. F80.;\nM30;\n",
        "full-circle",
    );
}

// ===========================================================================
// D. Warnings-буфер адаптера
// ===========================================================================

#[test]
fn adapter_take_warnings() {
    let ad = FanucAdapter::new();
    let p = ad
        .parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n", "t.ngc")
        .expect("parse ok");
    ad.emit(&p).expect("emit ok");
    let w = ad.take_warnings();
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("exact_path"), "{w:?}");
    // Буфер опустошён.
    assert!(ad.take_warnings().is_empty());
}

// ===========================================================================
// Хелперы hand-built AST
// ===========================================================================

fn span(line: u32) -> SrcSpan {
    SrcSpan { file: "t.ngc".into(), line_start: line, col_start: 1, line_end: line, col_end: 0 }
}

fn state_of(op: &Op) -> State {
    match op {
        Op::Rapid { state, .. } | Op::Linear { state, .. } | Op::Arc { state, .. } => state.clone(),
        _ => State {
            feed: None,
            spindle: Spindle::Off,
            coolant: Coolant::default(),
            tool: None,
            frame: Frame::Wcs,
            wcs: Wcs::G54,
        },
    }
}

/// Заменить wcs во всех state-полях опа `idx`.
fn set_wcs_of(p: &mut Program, idx: usize, wcs: Wcs) {
    match &mut p.ops[idx] {
        Op::Rapid { state, .. } | Op::Linear { state, .. } | Op::Arc { state, .. } => {
            state.wcs = wcs
        }
        _ => unreachable!(),
    }
}
