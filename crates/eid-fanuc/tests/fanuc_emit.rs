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
use eid_fanuc::emit::FanucEmissionCondition as Cond;
use eid_fanuc::emit::{emit_with_warnings, op_kind, FanucEmissionLegality};
use eid_fanuc::FanucAdapter;
use eid_harness::diff::{semantic_diff, DiffConfig, Divergence};

fn num(v: f64) -> Num {
    Num::new(format!("{v}"), v)
}

fn pt(x: f64, y: f64, z: f64) -> eid_core::ast::Point3 {
    eid_core::ast::Point3::xyz(num(x), num(y), num(z))
}

fn parse_fanuc(src: &str) -> Program {
    FanucAdapter::new()
        .parse(src, "t.ngc")
        .expect("fanuc parse ok")
        .program
}

fn emit_fanuc(src: &str) -> String {
    let p = parse_fanuc(src);
    FanucAdapter::new().emit(&p).expect("emit ok").text
}

fn emit_w(src: &str) -> (String, Vec<String>) {
    let ad = FanucAdapter::new();
    let p = ad.parse(src, "t.ngc").expect("fanuc parse ok").program;
    // v0.2 #22: warnings — в owning-результате emit (буфера адаптера нет).
    let out = ad.emit(&p).expect("emit ok");
    (out.text, out.warnings)
}

fn err_code_of<T>(r: Result<T, Vec<eid_core::diag::Diagnostic>>) -> ErrorCode {
    let Err(d) = r else {
        panic!("ожидался Err")
    };
    d[0].code
}

/// Round-trip fixed-point: parse → emit t1 → parse t1 → emit t2;
/// t1 == t2 байт-в-байт; AST-дифф пуст (кроме документированного value
/// shift path_mode ExactPath→ExactStop на Fanuc — COVERAGE §5.1/§2.2).
fn roundtrip(src: &str, label: &str) {
    let ad = FanucAdapter::new();
    let a = ad.parse(src, "t.ngc").expect("parse ok").program;
    let t1 = ad.emit(&a).expect("emit ok").text;
    let t2 = ad.emit(&a).expect("emit ok").text;
    assert_eq!(t1, t2, "{label}: emit недетерминирован");
    let b = ad.parse(&t1, "rt.ngc").expect("reparse ok").program;
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    let d: Vec<_> = d
        .into_iter()
        .filter(|x| {
            !matches!(x, Divergence::MetaField { field: "path_mode" }
            if matches!(a.path_mode, PathMode::ExactPath)
                && matches!(b.path_mode, PathMode::ExactStop))
        })
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
    let text =
        emit_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z2.;\nG81 X10. Y0. Z-5. R1. F120.;\nM30;\n");
    let lines: Vec<&str> = text.lines().collect();
    // traverse — первый G0 с F120.
    let tr = lines
        .iter()
        .find(|l| l.starts_with("G0 X10."))
        .expect("traverse");
    assert!(*tr == "G0 X10.0000 Y0.0000 Z2.0000 F120.0000");
}

#[test]
fn golden_feed_frame_each_cycle_first_op() {
    // SPEC §7.7-1 (v0.18, зеркало D feed_word `changed || sub==Some(1)`):
    // F эмитится на ПЕРВОМ производном опе КАЖДОГО цикла — повторный F-кадр
    // с тем же значением (G81 F120; G81 F120) не теряет маркер кадра при
    // reparse (оба traverse с F, не только первый).
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z2.;\nG81 X10. Y0. Z-5. R1. F120.;\nG81 X20. Y0. Z-5. R1. F120.;\nM30;\n",
    );
    // traverse второго цикла — на ТЕКУЩЕМ Z (1, не 2; §7.7); маркер F-кадра:
    // только traverse-строки несут F (отводы/подходы — не first-derived).
    let traverses: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("G0 X10.") || l.starts_with("G0 X20."))
        .filter(|l| l.ends_with("F120.0000"))
        .collect();
    assert_eq!(traverses.len(), 2, "оба traverse: {text:?}");
    for t in &traverses {
        assert!(t.ends_with("F120.0000"), "{t}");
    }
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
        if line.starts_with("G0 ")
            || line.starts_with("G1 ")
            || line.starts_with("G2 ")
            || line.starts_with("G3 ")
        {
            for w in line.split_whitespace().skip(1) {
                let letters = w.chars().next().expect("word");
                if letters.is_ascii_alphabetic() {
                    assert!(
                        w[1..].contains('.'),
                        "bare number `{w}` in `{line}` (DPI trap)"
                    );
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
    assert!(
        text.contains("\nG53 G0 X0.0000 Y0.0000 Z0.0000\n"),
        "{text:?}"
    );
}

// ===========================================================================
// A.1 Дуги (всегда IJK; полный круг без end-слов)
// ===========================================================================

#[test]
fn golden_arc_g17_ijk() {
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG3 X5. Y5. I0. J5. F100.;\nM30;\n");
    assert!(
        text.contains("G3 X5.0000 Y5.0000 Z0.0000 I0.0000 J5.0000 F100.0000\n"),
        "{text:?}"
    );
}

#[test]
fn golden_arc_g18_pair_ik() {
    // G18 = ZX: пара I+K (literal X/Z), вне-плоскостное Y — координата конца.
    let text =
        emit_fanuc("G21 G90 G94 G18;\nG0 X0. Y0. Z0.;\nG2 X5. Y2. Z-5. I5. K0. F100.;\nM30;\n");
    let arc = text
        .lines()
        .find(|l| l.starts_with("G2 "))
        .expect("arc line");
    assert!(arc == "G2 X5.0000 Y2.0000 Z-5.0000 I5.0000 K0.0000 F100.0000");
}

#[test]
fn golden_arc_g19_pair_jk() {
    let text =
        emit_fanuc("G21 G90 G94 G19;\nG0 X0. Y0. Z0.;\nG3 X2. Y5. Z-5. J5. K0. F100.;\nM30;\n");
    let arc = text
        .lines()
        .find(|l| l.starts_with("G3 "))
        .expect("arc line");
    assert!(arc == "G3 X2.0000 Y5.0000 Z-5.0000 J5.0000 K0.0000 F100.0000");
}

#[test]
fn golden_arc_r_source_emits_ijk() {
    // EQUIVALENCE §4: R НЕ эмитится никогда — R-источник → IJK; notice
    // near-degenerate не нужен (полукруг точно 180° — вне ±0.12°).
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X10. Y0. R5. F100.;\nM30;\n");
    let arc = text
        .lines()
        .find(|l| l.starts_with("G2 "))
        .expect("arc line");
    assert!(arc.contains('I') && arc.contains('J'), "{arc}");
    assert!(!arc.contains(" R"), "{arc}");
    assert!(arc == "G2 X10.0000 Y0.0000 Z0.0000 I5.0000 J0.0000 F100.0000");
}

#[test]
fn golden_full_circle_no_end_words() {
    // COVERAGE §5.1: полный круг (sweep=360°, pitch=0) — `G2 I_ J_` БЕЗ
    // end-слов (слова end==start перепарсер квантует в zero-length drop).
    let text =
        emit_fanuc("G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG2 X10. Y0. I-5. J0. F100.;\nM30;\n");
    let arc = text
        .lines()
        .find(|l| l.starts_with("G2 "))
        .expect("arc line");
    assert_eq!(
        arc, "G2 I-5.0000 J0.0000 F100.0000",
        "без X/Y/Z на полном круге"
    );
}

#[test]
fn golden_full_circle_inherited_end_no_words() {
    // Полный круг через НАСЛЕДОВАННЫЙ end (без end-слов в источнике).
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG2 I-5. F100.;\nM30;\n");
    let arc = text
        .lines()
        .find(|l| l.starts_with("G2 "))
        .expect("arc line");
    assert!(arc == "G2 I-5.0000 J0.0000 F100.0000");
}

#[test]
fn golden_helical_full_turn() {
    // Винтовой полный оборот (pitch≠0) — ТОЛЬКО end-слово вне-плоскостной
    // оси (in-plane наследуют старт).
    let text =
        emit_fanuc("G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG2 X10. Y0. Z5. I-5. J0. F100.;\nM30;\n");
    let arc = text
        .lines()
        .find(|l| l.starts_with("G2 "))
        .expect("arc line");
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
    let ai = lines
        .iter()
        .position(|l| l.starts_with("G3 "))
        .expect("arc");
    assert!(gi < ai, "G19 до дуги: {lines:?}");
    assert!(
        lines[gi + 1].starts_with("G3 "),
        "G19 непосредственно перед дугой"
    );
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
    let text =
        emit_fanuc("G21 G90 G94 G17;\nG10 L2 P2 X5. Y0. Z0.;\nG55;\nG0 X30. Y30. Z5.;\nM30;\n");
    assert!(
        text.contains("G10 L2 P2 X5.0000 Y0.0000 Z0.0000\n"),
        "{text:?}"
    );
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
    assert!(
        !text.contains("M2\n"),
        "голый M2 не эмитится (только M30): {text:?}"
    );
}

#[test]
fn golden_m5_m9_tail_not_emitted() {
    // Ruling COVERAGE §5: хвост M5/M9 перед program_end НЕ эмитится
    // (AST не хранит пост-конечное состояние; M30 сам глушит).
    let text = emit_fanuc("G21 G90 G94 G17;\nM3 S1200.;\nM8;\nG0 X0. Y0. Z5.;\nM30;\n");
    // M3/M8 должны быть эмитены ДО хода (дельты снапшота), но НЕ после.
    let pend = text.rfind("M30").expect("M30");
    let tail = &text[pend..];
    assert!(
        !tail.contains("M5") && !tail.contains("M9"),
        "хвост M5/M9: {text:?}"
    );
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
    assert!(
        warnings.is_empty(),
        "ExactStop 1:1 — без warnings: {warnings:?}"
    );
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
    assert!(
        warnings[0].contains("этап 8"),
        "профиль-опция задокументирована: {warnings:?}"
    );
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
    let text =
        emit_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z2.;\nG81 X10. Y0. Z-5. R1. F120.;\nM30;\n");
    let lines: Vec<&str> = text.lines().collect();
    let tr = lines
        .iter()
        .position(|l| l.starts_with("G0 X10."))
        .expect("traverse");
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
    FanucAdapter::new().emission_legality()
}

#[test]
fn legality_table_composition() {
    let t = legality_table();
    // 10 узлов словаря op_kind; 9 легальны (wcs_set_here запрещён — ловушка
    // №3), frame_preset — мета (G92).
    let kinds = [
        "rapid",
        "linear",
        "arc",
        "dwell",
        "tool_change",
        "program_stop",
        "program_end",
        "wcs_define",
        "wcs_set_here",
        "raw",
    ];
    for k in kinds {
        assert!(
            t.legal.contains(&k) || k == "wcs_set_here",
            "{k}: нет в таблице"
        );
    }
    assert!(t.is_legal_kind("rapid"));
    assert!(
        t.is_legal_kind("frame_preset"),
        "frame_preset (G92) легален"
    );
    assert!(!t.is_legal_kind("wcs_set_here"), "wcs_set_here НЕ легален");
    // illegal-условия с кодами (COVERAGE §5.1 + канон-форма). v0.2 волна 4:
    // коды — ТОТАЛЬНЫЕ match-функции enum'а; таблица — зеркало `cond.code()`.
    let expect = [
        (Cond::WcsSetHere, ErrorCode::UnsupportedSemantics),
        (Cond::WcsOutsideG54G59, ErrorCode::UnsupportedSyntax),
        (Cond::WcsDefinePOutsideG54G59, ErrorCode::UnsupportedSyntax),
        (Cond::MachineFrameArc, ErrorCode::UnsupportedSemantics),
        (Cond::MotionFeedNone, ErrorCode::InvalidInput),
        (Cond::ArcUnknownStart, ErrorCode::InvalidInput),
        (Cond::ArcNormalNonCanonical, ErrorCode::InvalidInput),
        (Cond::RawPayloadNotHex, ErrorCode::InvalidInput),
        (Cond::ProgramEndMissing, ErrorCode::InvalidInput),
        (Cond::ProgramEndDuplicate, ErrorCode::InvalidInput),
        (Cond::ProgramEndNotLast, ErrorCode::InvalidInput),
    ];
    for (cond, code) in expect {
        assert_eq!(cond.code(), code, "{}", cond.as_str());
        // Таблица = данные: пара (условие, код) обязана совпадать с match.
        let n = t.illegal.iter().filter(|(c, _)| *c == cond).count();
        assert_eq!(n, 1, "условие {} в таблице ровно один раз", cond.as_str());
        let (_, table_code) = t.illegal.iter().find(|(c, _)| *c == cond).unwrap();
        assert_eq!(
            *table_code,
            code,
            "{}: таблица = тотальный match",
            cond.as_str()
        );
    }
    // codes() — без дубликатов; все из {INVAL, SYN, USEM}.
    let mut codes = t.codes();
    #[allow(unused_variables)]
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
    assert!(
        t.warning_of("path_mode_exact_stop").is_none(),
        "ExactStop 1:1 — без warning"
    );
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
        feed_restated: false,
    };
    let p = Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: PathMode::ExactStop,
        frame_preset: None,
        ops: vec![
            Op::WcsSetHere {
                p: 1,
                delta: pt(0.0, 0.0, 0.0),
                state: st.clone(),
                src: span(1),
            },
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
    let Err(d) = FanucAdapter::new().emit(&p) else {
        panic!("ожидался Err")
    };
    assert_eq!(d[0].code, Cond::WcsSetHere.code());
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
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG2 X5. Y5. I0. J5. F100.;\nM30;\n");
    match &mut p.ops[1] {
        Op::Arc { state, .. } => state.frame = Frame::Machine,
        other => panic!("оп не Arc: {other:?}"),
    }
    let Err(d) = FanucAdapter::new().emit(&p) else {
        panic!("ожидался Err")
    };
    assert!(
        matches!(
            d[0].code,
            ErrorCode::InvalidInput | ErrorCode::UnsupportedSemantics
        ),
        "{}",
        d[0].code.as_str()
    );
    assert_eq!(
        Cond::MachineFrameArc.code(),
        ErrorCode::UnsupportedSemantics
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
    let Err(d) = FanucAdapter::new().emit(&p2) else {
        panic!("ожидался Err")
    };
    assert_eq!(d[0].code, ErrorCode::InvalidInput);
    assert_eq!(Cond::ProgramEndNotLast.code(), ErrorCode::InvalidInput);
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
    // wcs_set_here — код из таблицы (тотальный match enum'а, волна 4).
    assert_eq!(Cond::WcsSetHere.code(), ErrorCode::UnsupportedSemantics);
    // Таблица declarations ↔ тотальные match-функции (волна 4): код пары
    // обязан совпадать с `cond.code()` — расхождение невозможно молча.
    for (cond, code) in &t.illegal {
        assert_eq!(
            *code,
            cond.code(),
            "код {} = тотальный match",
            cond.as_str()
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
    let t1 = ad.emit(&p).expect("emit ok").text;
    let t2 = ad.emit(&p).expect("emit ok").text;
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
        "G21 G90 G94 G17;\nG0 X0. Y0. Z2.;\nG83 X10. Y0. Z-15. R1. Q3. F150.;\nG82 X20. Y0. Z-10. R1. P400 F120.;\nM30;\n",
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
// T8 (v0.2 волна 3, SPEC §7.4a + INTERFACE-V0.2-DESIGN §2.4): restated-F
// провенанс в AST — эмиттер переизлучает F по флагу кадра.
// ===========================================================================

#[test]
fn restated_f_reemitted_from_ast_flag() {
    // Два кадра с F-словом ОДНОГО значения → F переизлучён на обоих
    // (rs274 ассертит SET_FEED_RATE на каждом кадре с F-словом; прежде
    // второй кадр терял маркер — дыра класса e2e48 #21, закрыта флагом
    // feed_restated в AST).
    let text =
        emit_fanuc("G21 G90 G94 G17;\nG0 X10. Y0. Z5.;\nG1 X20. F120.;\nG1 X30. F120.;\nM30;\n");
    let f_count = text.matches(" F120.0000").count();
    assert_eq!(
        f_count, 2,
        "restated-F переизлучён на обоих F-кадрах:\n{text}"
    );
}

#[test]
fn inherited_f_not_reemitted() {
    // Второй кадр БЕЗ F-слова (наследованный модальный feed) → F НЕ
    // переизлучается (flag=false, changed=false, sub=None).
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X10. Y0. Z5.;\nG1 X20. F120.;\nG1 X30.;\nM30;\n");
    let f_count = text.matches(" F120.0000").count();
    assert_eq!(f_count, 1, "наследованный кадр не переизлучает F:\n{text}");
}

#[test]
fn g93_resolved_feed_is_emitted() {
    // (в) T8: G93-разрешённый feed (len·F) — F излучён (провенанс кадра
    // важен независимо от преобразования значения).
    let text = emit_fanuc("G21 G90 G93 G17;\nG0 X0. Y0. Z1.;\nG1 X10. F1000.;\nM30;\n");
    // Z не меняется (G1 X10. из (0,0,1)) — len = 10; feed = len·F = 10000.
    assert!(
        text.contains("F10000.0000"),
        "G93-resolved feed излучён:\n{text}"
    );
}

// ===========================================================================
// D. Warnings-буфер адаптера
// ===========================================================================

#[test]
fn adapter_take_warnings() {
    // v0.2 #22: owning-семантика — каждый emit возвращает СВОИ warnings
    // ( прежний «буфер с затиранием» заменён результатом; повторный emit
    // не опустошает предыдущий, а даёт тот же детерминированный набор).
    let ad = FanucAdapter::new();
    let p = ad
        .parse("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n", "t.ngc")
        .expect("parse ok")
        .program;
    let w = ad.emit(&p).expect("emit ok").warnings;
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("exact_path"), "{w:?}");
    // Owning-повтор: тот же детерминированный набор, не пусто.
    let w2 = ad.emit(&p).expect("emit ok 2").warnings;
    assert_eq!(w, w2, "warnings детерминированы (owning)");
}

// ===========================================================================
// Хелперы hand-built AST
// ===========================================================================

fn span(line: u32) -> SrcSpan {
    SrcSpan {
        file: "t.ngc".into(),
        line_start: line,
        col_start: 1,
        line_end: line,
        col_end: 0,
    }
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
            feed_restated: false,
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

// ===========================================================================
// Мутационный раунд v0.18 (агент D): wcs-словарь G56–G58, plane-строка G18,
// free-fn-обёртка emit, отклонение не-осевой нормали plane_of.
// ===========================================================================

#[test]
fn mround_wcs_word_g56_g57_g58() {
    // Полный словарь wcs_word G54–G59: G56/G57/G58 эмитятся дословно
    // (удаление match-армов mолча роняло код в catch-all).
    for (wcs_word_src, expect) in [("G56;\n", "G56"), ("G57;\n", "G57"), ("G58;\n", "G58")] {
        let text = emit_fanuc(&format!(
            "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\n{wcs_word_src}G0 X1. Y1. Z1.;\nM30;\n"
        ));
        assert!(
            text.lines().any(|l| l == expect),
            "wcs-строка {expect} потеряна:\n{text}"
        );
    }
}

#[test]
fn mround_plane_change_line_g18() {
    // Смена плоскости на G18 — отдельной строкой дословно (golden_plane_change_line
    // покрывал только G19; arm PlaneZX в plane_word молча падал в catch-all "G17").
    let text = emit_fanuc(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z0.;\nG18;\nG0 X0. Y0. Z0.;\nG2 X5. Y2. Z-5. I5. K0. F100.;\nM30;\n",
    );
    let lines: Vec<&str> = text.lines().collect();
    let gi = lines.iter().position(|l| *l == "G18").expect("G18 line");
    let ai = lines
        .iter()
        .position(|l| l.starts_with("G2 "))
        .expect("arc");
    assert!(gi < ai, "G18 до дуги: {lines:?}");
}

#[test]
fn mround_free_fn_emit_wrapper() {
    // Свободная функция eid_fanuc::emit::emit = обёртка emit_with_warnings
    // (текст с заголовком; обёртка не покрывалась ни одним тестом).
    let p = parse_fanuc("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n");
    let text = eid_fanuc::emit::emit(&p).expect("free emit ok");
    assert!(text.contains("G21 G90 G94"), "{text}");
    assert!(text.contains("G0"));
}

// 0.7071 — намеренное 4dp-приближение нормали (не FRAC_1_SQRT_2).
#[allow(clippy::approx_constant)]
#[test]
fn mround_emit_normal_non_axis_rejected() {
    // plane_of: не-осевая нормаль (0,1,0.5) — не канон → Err INVALID
    // (axial-проверка: best≈1 И остальные ≈0 И знак +).
    let mut p =
        parse_fanuc("G21 G90 G94 G17;\nG0 X10. Y0. Z0.;\nG3 X0. Y10. I-10. J0. F100.;\nM30;\n");
    for op in &mut p.ops {
        if let Op::Arc { normal, .. } = op {
            // единичная, но не осевая — D7-длину проходит, axial-проверку нет
            *normal = pt(0.0, 0.7071, 0.7071);
        }
    }
    let Err(d) = FanucAdapter::new().emit(&p) else {
        panic!("не-осевая нормаль обязана быть отклонена")
    };
    assert!(d.iter().any(|x| x.code == ErrorCode::InvalidInput), "{d:?}");
    assert!(
        d.iter().any(|x| {
            x.message.contains("единичная +ось") || x.message.contains("+axis")
        }),
        "{d:?}"
    );
}

// ===========================================================================
// Этап 8 (agent C, emission-профили): профильные опции эмиссии —
// program_end M30/M02 (COVERAGE §5.1 «выбор профиля»), allow_r_arcs
// (R-форма + notice на каждой R-дуге; дефолт false = всегда IJK),
// g59_map (G59.x → G54.1 Pn + notice «map применён, таблица подменена»;
// неполная мапа → отказ с перечнем незмапленных). Опции входят через
// FanucAdapter::emit_with_opts (trait v0.2 заморожен); Default = прежняя
// эмиссия байт-в-байт.
// ===========================================================================

use eid_fanuc::emit::{emit_with_profile, FanucEmissionOpts, FanucG59Map, FanucProgramEnd};

/// Эмиссия hand-программы с опциями (text + warnings).
fn emit_with_opts_fanuc(p: &Program, opts: &FanucEmissionOpts) -> (String, Vec<String>) {
    let mut w = Vec::new();
    let out = emit_with_profile(p, opts, &mut w).expect("emit_with_profile ok");
    (out.text, out.warnings)
}

/// CCW 90° (центр (10,0), старт (0,0), end (10,-10), R = +10) и CCW 270°
/// (старт (10,-10), тот же центр (I0 J10 — инкремент от старта), end (0,0),
/// R = −10 — знак по sweep > 180°). G61 явно — без path-mode value shift
/// (ExactPath→ExactStop — документированное исключение semantic_diff).
fn two_arcs_program() -> Program {
    parse_fanuc(
        "G21 G90 G94 G17 G91.1;\nF100.0;\nG61;\nG0 X0. Y0. Z5.;\nG3 X10. Y-10. I10. J0.;\nG3 X0. Y0. I0. J10.;\nM30;\n",
    )
}

/// wcs + start=Unknown (смена wcs после первого хода — точка разрыва фрейма,
/// SPEC §2 Q11: старт в новой СК не вычислим из текста; rapid с Unknown
/// стартом эмитится, проверка wcs не зависит от старта).
fn set_wcs_unknown(p: &mut Program, idx: usize, wcs: Wcs) {
    match &mut p.ops[idx] {
        Op::Rapid { state, start, .. } => {
            state.wcs = wcs;
            *start = StartPoint::Unknown;
        }
        _ => unreachable!(),
    }
}

#[test]
fn stage8_default_opts_byte_identical_to_plain_emit() {
    let p = two_arcs_program();
    let plain = FanucAdapter::new().emit(&p).expect("emit ok").text;
    let (with_default, _) = emit_with_opts_fanuc(&p, &FanucEmissionOpts::default());
    assert_eq!(
        plain, with_default,
        "Default-опции = прежняя эмиссия байт-в-байт"
    );
}

#[test]
fn stage8_program_end_m02() {
    // COVERAGE §5.1: M30 дефолт (Fanuc-конвенция rewind) vs M02 — выбор
    // профиля.
    let p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM30;\n");
    let (m30, _) = emit_with_opts_fanuc(&p, &FanucEmissionOpts::default());
    assert!(m30.trim_end().ends_with("M30"), "{m30}");
    let opts = FanucEmissionOpts {
        program_end: FanucProgramEnd::M02,
        ..FanucEmissionOpts::default()
    };
    let (m02, _) = emit_with_opts_fanuc(&p, &opts);
    assert!(
        m02.trim_end().ends_with("M02") && !m02.contains("M30"),
        "{m02}"
    );
    // Оба стиля детерминированы (двойной emit — те же байты).
    let (m02_again, _) = emit_with_opts_fanuc(&p, &opts);
    assert_eq!(m02, m02_again);
}

#[test]
fn stage8_r_arcs_false_ijk_true_r_with_notice() {
    let p = two_arcs_program();
    // false (дефолт) = всегда IJK: слова I/J, R-слов нет.
    let (ijk, _) = emit_with_opts_fanuc(&p, &FanucEmissionOpts::default());
    assert!(ijk.contains("I10.0000") && ijk.contains("J0.0000"), "{ijk}");
    assert!(!ijk.contains(" R"), "{ijk}");
    // true = R-форма + notice на КАЖДОЙ R-дуге (COVERAGE §5.1).
    let opts = FanucEmissionOpts {
        allow_r_arcs: true,
        ..FanucEmissionOpts::default()
    };
    let (r_text, w) = emit_with_opts_fanuc(&p, &opts);
    assert!(r_text.contains("R10.0000"), "{r_text}");
    assert!(!r_text.contains("I10.0000"), "{r_text}");
    let notices = w.iter().filter(|s| s.contains("R-форма применена")).count();
    assert_eq!(notices, 2, "notice на каждой R-дуге: {w:?}");
    // Знак R: sweep ≤ 180° → положительный; sweep > 180° → отрицательный
    // (Fanuc-соглашение ≤180°/>180°; парсер восстанавливает по знаку).
    assert!(r_text.contains(" R10.0000"), "90° → R>0: {r_text}");
    assert!(r_text.contains(" R-10.0000"), "270° → R<0: {r_text}");
}

#[test]
fn stage8_r_arcs_full_circle_stays_ijk() {
    // Полный круг (sweep=360°) — ВСЕГДА IJK даже при allow_r_arcs: R при
    // совпадающих in-plane start/end неоднозначен (SPEC §6; alarm) + notice.
    let p = parse_fanuc("G21 G90 G94 G17 G91.1;\nF100.0;\nG0 X5. Y0. Z5.;\nG2 I-5. J0.;\nM30;\n");
    let opts = FanucEmissionOpts {
        allow_r_arcs: true,
        ..FanucEmissionOpts::default()
    };
    let (text, w) = emit_with_opts_fanuc(&p, &opts);
    assert!(text.contains("I-5.0000"), "полный круг → IJK: {text}");
    assert!(!text.contains(" R"), "{text}");
    assert!(
        w.iter().any(|s| s.contains("полный круг 360°")),
        "notice про IJK-fallback: {w:?}"
    );
}

#[test]
fn stage8_r_roundtrip_ast_and_semantic_diff() {
    // Требование этапа 8: parse(R-файл) ≡ AST (R → центр-пересчёт) —
    // semantic_diff с IJK-вариантом = 0 (та же дуга).
    let ad = FanucAdapter::new();
    let ast_ijk = two_arcs_program();
    let opts = FanucEmissionOpts {
        allow_r_arcs: true,
        ..FanucEmissionOpts::default()
    };
    let (r_text, _) = emit_with_opts_fanuc(&ast_ijk, &opts);
    let ast_r = ad
        .parse(&r_text, "rt.ngc")
        .expect("parse R-текста ok")
        .program;
    // IJK-вариант того же AST (дефолт) и его reparse.
    let (ijk_text, _) = emit_with_opts_fanuc(&ast_ijk, &FanucEmissionOpts::default());
    let ast_ijk_re = ad
        .parse(&ijk_text, "rt.ngc")
        .expect("parse IJK-текста ok")
        .program;
    // semantic_diff(R-вариант, IJK-вариант) = 0.
    let d = semantic_diff(&ast_r, &ast_ijk_re, &DiffConfig::default());
    assert!(d.is_empty(), "R ≡ IJK (та же дуга): {d:?}");
    // AST-равенство дуг (центр пересчитан из R): 4dp-квант парсера делает
    // значения точно равными на выбранной геометрии.
    let arcs = |p: &Program| -> Vec<(f64, f64, f64, f64)> {
        p.ops
            .iter()
            .filter_map(|op| match op {
                Op::Arc {
                    center, sweep_deg, ..
                } => Some((
                    center.x.value(),
                    center.y.value(),
                    center.z.value(),
                    sweep_deg.value(),
                )),
                _ => None,
            })
            .collect()
    };
    let a = arcs(&ast_ijk);
    let b = arcs(&ast_r);
    assert_eq!(a.len(), 2);
    for (l, r) in a.iter().zip(b.iter()) {
        for (lv, rv, name) in [
            (l.0, r.0, "center.x"),
            (l.1, r.1, "center.y"),
            (l.2, r.2, "center.z"),
            (l.3, r.3, "sweep"),
        ] {
            assert!(
                (lv - rv).abs() < 1e-9,
                "{name}: IJK {lv} vs R-пересчёт {rv}"
            );
        }
    }
    // И вся программа эквивалентна исходному AST.
    let d = semantic_diff(&ast_ijk, &ast_r, &DiffConfig::default());
    assert!(d.is_empty(), "parse(R) ≡ исходный AST: {d:?}");
}

#[test]
fn stage8_wcs_map_off_refusal_unchanged() {
    // Без мапы (дефолт) — прежний отказ SYNTAX (wcs_outside_g54_g59).
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM30;\n");
    set_wcs_of(&mut p, 0, Wcs::G59_1);
    let err = FanucAdapter::new().emit_with_opts(&p, &FanucEmissionOpts::default());
    let Err(d) = err else {
        panic!("ожидался отказ")
    };
    assert_eq!(d[0].code, ErrorCode::UnsupportedSyntax);
    assert!(d[0].message.contains("wcs_outside_g54_g59"), "{d:?}");
}

#[test]
fn stage8_wcs_map_on_g541_pn_with_notice_once() {
    // Мапа включена → эмиссия целевого кода G54.1 Pn + notice «map применён,
    // таблица подменена» РОВНО ОДИН раз на эмиссию (аналогия
    // golden_preset_no_bake — НЕ дефолт).
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG0 X1. Y1. Z1.;\nG4 P500;\nM30;\n");
    for i in 0..3 {
        match &mut p.ops[i] {
            Op::Rapid { state, .. } | Op::Dwell { state, .. } => state.wcs = Wcs::G59_1,
            _ => unreachable!(),
        }
    }
    let opts = FanucEmissionOpts {
        g59_map: Some(FanucG59Map {
            g59_1: Some(1),
            g59_2: Some(2),
            g59_3: None,
        }),
        ..FanucEmissionOpts::default()
    };
    let (text, w) = emit_with_opts_fanuc(&p, &opts);
    assert!(text.contains("G54.1 P1"), "{text}");
    assert!(!text.contains("G59"), "{text}");
    let notices: Vec<&String> = w.iter().filter(|s| s.contains("map применён")).collect();
    assert_eq!(notices.len(), 1, "notice один на эмиссию: {w:?}");
    assert!(
        notices[0].contains("G59.1→G54.1 P1") && notices[0].contains("G59.2→G54.1 P2"),
        "notice перечисляет таблицу: {}",
        notices[0]
    );
}

#[test]
fn stage8_wcs_map_incomplete_refusal_lists_unmapped() {
    // Неполная мапа: G59.1 замаплен, G59.2 нет, в программе есть оба →
    // ОДИН отказ с перечнем незмапленных (SYN).
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG0 X1. Y1. Z1.;\nM30;\n");
    set_wcs_unknown(&mut p, 0, Wcs::G59_1);
    set_wcs_unknown(&mut p, 1, Wcs::G59_2);
    let opts = FanucEmissionOpts {
        g59_map: Some(FanucG59Map {
            g59_1: Some(1),
            g59_2: None,
            g59_3: None,
        }),
        ..FanucEmissionOpts::default()
    };
    let err = FanucAdapter::new().emit_with_opts(&p, &opts);
    let Err(d) = err else {
        panic!("ожидался отказ")
    };
    assert_eq!(d[0].code, ErrorCode::UnsupportedSyntax);
    assert!(
        d[0].message.contains("wcs_map_incomplete") && d[0].message.contains("G59.2"),
        "перечень незмапленных: {d:?}"
    );
}

#[test]
fn stage8_wcs_map_frame_preset_mapped() {
    // frame_preset.wcs = расширенный слот: без мапы → прежний отказ;
    // мапа включена → G54.1 Pn первым блоком + notice. Пресетка задаётся
    // hand-AST (G92-текст делает старты after-preset UnknownRequired —
    // точка разрыва фрейма, SPEC §2 Q11).
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM30;\n");
    p.frame_preset = Some(eid_core::ast::FramePreset {
        wcs: Wcs::G59_1,
        values: pt(1.0, 2.0, 3.0),
    });
    let err = FanucAdapter::new().emit_with_opts(&p, &FanucEmissionOpts::default());
    assert!(err.is_err(), "map-off: прежний отказ");
    let opts = FanucEmissionOpts {
        g59_map: Some(FanucG59Map {
            g59_1: Some(7),
            g59_2: None,
            g59_3: None,
        }),
        ..FanucEmissionOpts::default()
    };
    let (text, w) = emit_with_opts_fanuc(&p, &opts);
    assert!(text.contains("G54.1 P7\nG92 X"), "{text}");
    assert!(w.iter().any(|s| s.contains("map применён")), "{w:?}");
}

// ===========================================================================
// МУТ-РАУНД 2 (fanuc, agent D): writer-границы и validate-arms по сообщениям.
// Коды отказов дублируются защитными путями (writer-protection / core-validate),
// поэтому различение — по ТЕКСТУ сообщения валидатора таблицы.
// ===========================================================================

/// Диагностики эмиссии (текст + коды).
fn emit_diags(p: &Program) -> Vec<eid_core::diag::Diagnostic> {
    let Err(d) = FanucAdapter::new().emit(p) else {
        panic!("ожидался Err")
    };
    d
}

#[test]
fn mround2_writer_wcs_g59_line() {
    // wcs-выборка G59 — wcs_word-плечо G59 (строка «G59» перед ходом).
    let text = emit_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG59;\nG0 X1. Y1. Z1.;\nM30;\n");
    assert!(text.lines().any(|l| l == "G59"), "{text}");
}

#[test]
fn mround2_writer_spindle_transitions_exact() {
    // Шпиндель: M3/M4/M5-строки с оборотами; повторный bare S с тем же
    // значением НЕ переэмитит M3 (spindle_eq Cw==Cw).
    let text = emit_fanuc(
        "S1200 M3;\nG0 X0. Y0. Z5.;\nG1 X1. F100.;\nS1200;\nG1 X2.;\nM4 S900;\nG1 X3.;\nM5;\nG1 X4.;\nM30;\n",
    );
    let lines: Vec<String> = text.lines().map(String::from).collect();
    let m3 = lines.iter().filter(|l| l.starts_with("M3 ")).count();
    let m4 = lines.iter().filter(|l| l.starts_with("M4 ")).count();
    let (m3_line, m4_line, m5_line) = ("M3 S1200.0000", "M4 S900.0000", "M5");
    let m5 = lines.iter().filter(|l| l.as_str() == m5_line).count();
    assert_eq!(m3, 1, "M3 S1200 ровно один раз: {text}");
    assert_eq!(m4, 1, "M4 S900 ровно один раз: {text}");
    assert_eq!(m5, 1, "M5 ровно один раз: {text}");
    assert!(lines.iter().any(|l| l == m3_line), "{text}");
    assert!(lines.iter().any(|l| l == m4_line), "{text}");
}

#[test]
fn mround2_writer_coolant_m9_m7_m8_order() {
    // СОЖ: flood-включение → M8; добавка mist поверх flood → M7 (+ре-эмиссия
    // M8 при компонентном diff); hand-built сброс flood при живом mist →
    // M9 ДО включений (off_needed-плечо), затем M7.
    let text =
        emit_fanuc("M8;\nG0 X0. Y0. Z5.;\nM7;\nG0 X1. Y0. Z5.;\nM9;\nG0 X2. Y0. Z5.;\nM30;\n");
    let lines: Vec<String> = text.lines().map(String::from).collect();
    let m8 = lines.iter().position(|l| l == "M8").expect("M8");
    let m7 = lines.iter().position(|l| l == "M7").expect("M7");
    let m9 = lines.iter().position(|l| l == "M9").expect("M9");
    assert!(m8 < m7, "M8 первым: {lines:?}");
    assert!(m7 < m9, "M9 (полное выключение) последним: {lines:?}");
    // off_needed-плечо: cur {flood, mist} → state {mist} — M9 перед M7.
    let mut p = parse_fanuc("G0 X0. Y0. Z5.;\nG0 X1. Y0. Z5.;\nM30;\n");
    match &mut p.ops[0] {
        Op::Rapid { state, .. } => {
            state.coolant = Coolant {
                mist: true,
                flood: true,
            };
        }
        other => panic!("ops[0] не Rapid: {other:?}"),
    }
    match &mut p.ops[1] {
        Op::Rapid { state, .. } => {
            state.coolant = Coolant {
                mist: true,
                flood: false,
            };
        }
        other => panic!("ops[1] не Rapid: {other:?}"),
    }
    let (_, w) = emit_w_prog(&p);
    let text2 = FanucAdapter::new().emit(&p).expect("emit ok").text;
    let l2: Vec<String> = text2.lines().map(String::from).collect();
    let m9 = l2.iter().position(|l| l == "M9").expect("M9");
    let m7_last = l2.iter().rposition(|l| l == "M7").expect("M7");
    assert!(m9 < m7_last, "M9 строго до повторных включений: {l2:?}");
    let _ = w;
}

#[test]
fn mround2_codes_table_nonempty() {
    // codes() — непустой, отсортированный, без дубликатов; содержит все три
    // класса (INTROSPECTION-контракт таблицы).
    let t = FanucAdapter::new().emission_legality();
    let codes = t.codes();
    assert!(!codes.is_empty(), "codes() пуст");
    let mut sorted = codes.clone();
    sorted.sort_by_key(|c| c.as_str());
    assert_eq!(codes, sorted, "codes() отсортирован");
    #[allow(unused_variables)]
    let n = codes.len();
    assert_eq!(
        codes.len(),
        codes
            .iter()
            .map(|c| c.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        "без дубликатов"
    );
    for want in [
        ErrorCode::InvalidInput,
        ErrorCode::UnsupportedSemantics,
        ErrorCode::UnsupportedSyntax,
    ] {
        assert!(codes.contains(&want), "{want:?} отсутствует в {codes:?}");
    }
}

#[test]
fn mround2_wcs_set_here_validator_message() {
    // wcs_set_here: отказ именно от ВАЛИДАТОРА таблицы (текст про ловушку №3),
    // не от writer-protection (защитный путь даёт тот же код). Оп вставляется
    // ДО первого motion (core: preset-класс легален только там) и до PEND.
    let mut p = parse_fanuc("G21 G90 G94 G17;\nM30;\n");
    p.ops.insert(
        0,
        Op::WcsSetHere {
            p: 1,
            delta: eid_core::ast::Vec3::xyz(num(0.0), num(0.0), num(0.0)),
            state: State {
                feed: None,
                spindle: Spindle::Off,
                coolant: Coolant::default(),
                tool: None,
                frame: Frame::Wcs,
                wcs: Wcs::G54,
                feed_restated: false,
            },
            src: span(1),
        },
    );
    let d = emit_diags(&p);
    assert!(
        d.iter().any(
            |x| x.message.contains("запрещена как G10 L20") && x.message.contains("ловушка №3")
        ),
        "{d:?}"
    );
}

#[test]
fn mround2_arc_hand_built_diags() {
    // Arc-arms валидатора, достижимые поверх core-verify: machine-frame
    // дуга и не-каноническая нормаль — каждый со СВОИМ текстом.
    // (a) machine-frame дуга:
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG10 L2 P1 X0. Y0. Z0.;\nF100.;\nG0 X10. Y0. Z0.;\nG3 X0. Y10. I-10. J0.;\nM30;\n");
    for op in &mut p.ops {
        if let Op::Arc { state, .. } = op {
            state.frame = Frame::Machine;
        }
    }
    let d = emit_diags(&p);
    assert!(
        d.iter().any(|x| x.message.contains("machine_frame_arc")),
        "{d:?}"
    );
}

#[test]
fn mround2_plane_of_degenerate_normals_rejected() {
    // plane_of: смесь осей (1,0.5,0) и нулевая нормаль — отказ, не паника.
    for nn in [(1.0f64, 0.5f64, 0.0f64), (0.0, 0.0, 0.0), (0.6, 0.8, 0.0)] {
        let n = nn;
        let mut p = parse_fanuc(
            "G21 G90 G94 G17;\nF100.;\nG0 X10. Y0. Z0.;\nG3 X0. Y10. I-10. J0.;\nM30;\n",
        );
        for op in &mut p.ops {
            if let Op::Arc { normal, .. } = op {
                *normal = pt(n.0, n.1, n.2);
            }
        }
        let d = emit_diags(&p);
        assert!(
            d.iter().any(|x| x.code == ErrorCode::InvalidInput),
            "нормаль {n:?} обязана быть отклонена: {d:?}"
        );
    }
}

#[test]
fn mround2_motion_dwell_wcs_outside_message() {
    // G59.1 при map-off: отказ — ВАЛИДATOR (текст wcs_outside_g54_g59),
    // покрыт rapid, linear и dwell-плечами (каждое — свой match-arm).
    // Смена wcs делает связывание невычислимым → start=Unknown
    // (это легально для rapid/linear в core).
    fn set_wcs_unknown(p: &mut Program, idx: usize, wcs: Wcs) {
        match &mut p.ops[idx] {
            Op::Rapid { state, start, .. } | Op::Linear { state, start, .. } => {
                state.wcs = wcs;
                *start = StartPoint::Unknown;
            }
            _ => unreachable!(),
        }
    }
    // (a) linear:
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG1 X1. F100.;\nM30;\n");
    set_wcs_unknown(&mut p, 1, Wcs::G59_1);
    let d = emit_diags(&p);
    assert!(
        d.iter().any(|x| x.message.contains("wcs_outside_g54_g59")),
        "{d:?}"
    );
    // (b) rapid:
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM30;\n");
    set_wcs_unknown(&mut p, 0, Wcs::G59_1);
    let d = emit_diags(&p);
    assert!(
        d.iter().any(|x| x.message.contains("wcs_outside_g54_g59")),
        "{d:?}"
    );
    // (c) dwell:
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG4 P100;\nM30;\n");
    match &mut p.ops[0] {
        Op::Dwell { state, .. } => state.wcs = Wcs::G59_1,
        other => panic!("ops[0] не Dwell: {other:?}"),
    }
    let d = emit_diags(&p);
    assert!(
        d.iter().any(|x| x.message.contains("wcs_outside_g54_g59")),
        "{d:?}"
    );
}

#[test]
fn mround2_raw_bad_hex_message() {
    // Raw с некорректным payload: отказ валидатора (raw_payload_not_hex),
    // не writer-decode (оба INVAL — различаем по тексту).
    // (a) нечётная длина из hex-символов (плечо длины):
    let mut p = parse_fanuc("G65;\nM30;\n");
    match &mut p.ops[0] {
        Op::Raw { payload_hex, .. } => *payload_hex = "41424".into(),
        other => panic!("ops[0] не Raw: {other:?}"),
    }
    let d = emit_diags(&p);
    assert!(
        d.iter().any(|x| x.message.contains("raw_payload_not_hex")),
        "{d:?}"
    );
    // (b) чётная длина, не-hex символы (плечо алфавита):
    let mut p = parse_fanuc("G65;\nM30;\n");
    match &mut p.ops[0] {
        Op::Raw { payload_hex, .. } => *payload_hex = "ZZ".into(),
        other => panic!("ops[0] не Raw: {other:?}"),
    }
    let d = emit_diags(&p);
    assert!(
        d.iter().any(|x| x.message.contains("raw_payload_not_hex")),
        "{d:?}"
    );
}

#[test]
fn mround2_emit_tail_raw_after_pend_ok() {
    // Fanuc-хвост: Raw-сабы после M30 ЛЕГАЛЬНЫ в same-dialect эмиссии
    // (ruling 1) — retain-фильтр core-ошибки «program_end must be the last
    // op» обязан оставить программу эмитибельной.
    let p = parse_fanuc("G0 X1. Y1. Z1.;\nM30;\nO100;\nX5.;\n");
    let text = FanucAdapter::new()
        .emit(&p)
        .expect("tail-subs emit ok")
        .text;
    assert!(text.contains("M30"), "{text}");
    assert!(text.contains("X5."), "raw-хвост дословно: {text}");
}

#[test]
fn mround2_writer_dn_normal_component_warning() {
    // normal-компонента центра: warning при |dn| > 0.001; при 0.0005 — нет.
    let mut p =
        parse_fanuc("G21 G90 G94 G17;\nF100.;\nG0 X10. Y0. Z0.;\nG3 X0. Y10. I-10. J0.;\nM30;\n");
    for op in &mut p.ops {
        if let Op::Arc { center, .. } = op {
            center.z = num(0.5); // normal-компонента (плоскость XY)
        }
    }
    let (_, w) = emit_w_prog(&p);
    assert!(w.iter().any(|s| s.contains("не эмитится")), "{w:?}");
    // sub-порог:
    let mut p =
        parse_fanuc("G21 G90 G94 G17;\nF100.;\nG0 X10. Y0. Z0.;\nG3 X0. Y10. I-10. J0.;\nM30;\n");
    for op in &mut p.ops {
        if let Op::Arc { center, .. } = op {
            center.z = num(0.0005);
        }
    }
    let (_, w) = emit_w_prog(&p);
    assert!(w.iter().all(|s| !s.contains("не эмитится")), "{w:?}");
}

/// emit_with_warnings для hand-программы (warnings без адаптера).
fn emit_w_prog(p: &Program) -> (String, Vec<String>) {
    let mut w = Vec::new();
    let out = emit_with_warnings(p, &mut w).expect("emit ok");
    (out.text, w)
}

#[test]
fn mround2_writer_feed_changed_and_none_warning() {
    // feed_word: смена feed → F-слово в кадре; feed=None при известном
    // модальном — warning «скачок снапшота».
    // (a) два linear с РАЗНЫМ feed — оба кадра несут F:
    let text = emit_fanuc("G1 X1. F100.;\nG1 X2. F200.;\nM30;\n");
    let f_count = text.lines().filter(|l| l.contains("F200")).count();
    assert!(f_count >= 1, "F200 потерян: {text}");
    // (b) feed=None на rapid при активном cur_feed → warning (core не
    // требует feed на rapid, поэтому fanuc-путь достижим).
    let mut p = parse_fanuc("G1 X1. F100.;\nG0 X2. Y0. Z0.;\nM30;\n");
    match &mut p.ops[1] {
        Op::Rapid { state, .. } => state.feed = None,
        other => panic!("ops[1] не Rapid: {other:?}"),
    }
    let (_, w) = emit_w_prog(&p);
    assert!(w.iter().any(|s| s.contains("скачок снапшота")), "{w:?}");
}

#[test]
fn mround2_writer_tool_jump_warning() {
    // state.tool без ToolChange-опа — несвязный снапшот → warning.
    let mut p = parse_fanuc("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n");
    match &mut p.ops[0] {
        Op::Rapid { state, .. } => state.tool = Some(7),
        other => panic!("ops[0] не Rapid: {other:?}"),
    }
    let (_, w) = emit_w_prog(&p);
    assert!(
        w.iter()
            .any(|s| s.contains("state.tool = Some(7)") && s.contains("скачок")),
        "{w:?}"
    );
}

#[test]
fn mround2_writer_m30_and_stop_lines() {
    // M0/M1/M30 — точные строки; хвост M5/M9 НЕ эмитится.
    let text = emit_fanuc("M0;\nM1;\nG0 X1. Y1. Z1.;\nM30;\n");
    let lines: Vec<String> = text.lines().map(String::from).collect();
    assert!(lines.iter().any(|l| l == "M0"), "{text}");
    assert!(lines.iter().any(|l| l == "M1"), "{text}");
    assert_eq!(
        lines.last().map(String::as_str),
        Some("M30"),
        "M30 последний: {text}"
    );
    assert!(
        lines.iter().all(|l| l.as_str() != "M5"),
        "хвост M5 не эмитится: {text}"
    );
}
