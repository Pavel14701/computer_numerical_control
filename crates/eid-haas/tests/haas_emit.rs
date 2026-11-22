//! Интеграционные тесты эмиссии EID/L1 → Haas mill NGC (этап 3, Агент C).
//! Golden-пары (AST → точный haas-текст), Legality-таблица (типизированные
//! условия — INTERFACE-V0.2 §2.5 В1), детерминизм, same-dialect round-trip
//! fixed-point, cross-check развёртки vs golden #19/#20/#21 (semantic_diff).
//! По образцу eid-fanuc tests/fanuc_emit.rs; допуски — EQUIVALENCE §4.
//!
//! ЧЕК-ЛИСТ этапа 3 (правило → тест):
//! ЗАГОЛОВОК: G20|G21 G90 G94 G17 — всегда явно; **G91.1 НЕ эмитится**
//!   (группы 04 в словаре Haas нет) → golden_header_mm / _inch /
//!   no_g91_1_in_header.
//! PATH_MODE: exact_stop → G61 (без warning); exact_path → G61+warning
//!   (эквивалента нет — COVERAGE §5.2, дефолт как Fanuc); blend bare → G64;
//!   blend(t) → G64+warning (t теряется) → golden_path_*.
//! FRAME_PRESET: выборка WCS + G92 первым блоком (EQUIVALENCE §1.4);
//!   G92.1 не эмитится (кода нет, Setting 129 `?`) → golden_preset_g92 /
//!   preset_extended_wcs_warning.
//! WCS: p1..6 → G54–G59; **p7..9 → G110–G112 + warning wcs_extended_slot**
//!   (нативная симметрия слотов #7–#9, зеркало W_WCS_EXTENDED; оговорка
//!   происхождения G59.x-таблиц) → golden_wcs_select_extended /
//!   preset_extended_wcs_warning; WCS_DEFINE P1..P6 → G10 L2; **P7..P9 →
//!   отказ SYNTAX** (L2-домен P0..P6; L20 = profile-опция НЕ дефолт) →
//!   illegal_wcs_define_p_outside.
//! ЧИСЛА: 4dp decimal point ВСЕГДА (DPI-ловушка: bare integer = ×least
//!   increment) → golden_decimal_point_always (bare X5 → 0.0050).
//! DWELL: P-dot-rule ОБРАТНЫЙ — целые мс → bare P500; sub-ms → dotted
//!   P0.0005 сек (X/U-fallback НЕТ) → golden_dwell_p_dot_reverse.
//! ДУГИ: плоскость из нормали (G17/G18/G19-строки при смене), in-plane пара
//!   I/J/K (G18→I+K, G19→J+K), инкремент от старта, R НЕ эмитится, **полный
//!   круг (sweep=360°, pitch=0) БЕЗ end-слов**, helical полный оборот —
//!   end-слово вне-плоскостной оси → golden_arc_* .
//! MACHINE-FRAME: G53 G0|G1 префикс → golden_machine_frame.
//! ОПЫ: T## M06; M0/M1; M30 (хвост M5/M9 не эмитится); RAW verbatim
//!   (+CRLF); F-правило sub=1 (§7.4a v0.18) → golden_tool_stop_end /
//!   golden_raw_verbatim / golden_feed_first_derived_sub1.
//! СОЖ: flood → M8, off → M9 (выключения до включений); **mist → отказ
//!   SEMANTICS** (M07 = Shower — другой канал) → illegal_coolant_mist.
//! LEGALITY: таблица-данные ↔ тотальные match-функции (без паник на
//!   emit-пути) → legality_table_complete / legality_behavior_cross_check /
//!   legality_codes_composition.
//! ДЕТЕРМИНИЗМ + ROUND-TRIP: double-emit байт-в-байт; parse→emit→parse —
//!   semantic_diff 0 (полная программа, циклы, G91, G92+Raw-хвост) →
//!   emit_deterministic_double / roundtrip_*.
//! CROSS-CHECK v0.18: haas-парсер на linuxcnc-исходниках #19/#20/#21 даёт
//!   AST = golden.eid (semantic_diff 0; отличия dwell P-dot учтены dotted-P
//!   в источниках) → mirror_v018_golden_cross_check_19_20_21.

use eid_core::adapter::Adapter;
use eid_core::ast::{
    Coolant, Dir, Frame, Num, Op, PathMode, Program, Spindle, SrcSpan, StartPoint, State, Units,
    Wcs,
};
use eid_core::diag::ErrorCode;
use eid_haas::emit::{op_kind, HaasEmissionCondition, HaasEmissionWarning};
use eid_haas::HaasAdapter;
use eid_harness::diff::{semantic_diff, DiffConfig, Divergence};

fn num(v: f64) -> Num {
    Num::new(format!("{v}"), v)
}

fn pt(x: f64, y: f64, z: f64) -> eid_core::ast::Point3 {
    eid_core::ast::Point3 {
        x: num(x),
        y: num(y),
        z: num(z),
    }
}

fn src_span() -> SrcSpan {
    SrcSpan {
        file: "hand".into(),
        line_start: 1,
        col_start: 1,
        line_end: 1,
        col_end: 1,
    }
}

fn state(wcs: Wcs) -> State {
    State {
        feed: Some(num(100.0)),
        spindle: Spindle::Off,
        coolant: Coolant::default(),
        tool: None,
        frame: Frame::Wcs,
        wcs,
        feed_restated: false,
    }
}

/// Минимальная hand-built программа: ops + PEND, mm, ExactPath, без пресетки.
fn hand_program(ops: Vec<Op>) -> Program {
    let mut ops = ops;
    ops.push(Op::ProgramEnd { src: src_span() });
    Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: PathMode::ExactPath,
        frame_preset: None,
        ops,
    }
}

fn parse_haas(src: &str) -> Program {
    HaasAdapter::new()
        .parse(src, "t.ngc")
        .expect("haas parse ok")
        .program
}

fn emit_haas(src: &str) -> String {
    let p = parse_haas(src);
    HaasAdapter::new().emit(&p).expect("emit ok").text
}

/// Эмиссия + warnings (v0.2 #22: owning-результат emit — буфера нет).
fn emit_w(src: &str) -> (String, Vec<String>) {
    let ad = HaasAdapter::new();
    let p = ad.parse(src, "t.ngc").expect("haas parse ok").program;
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
/// t1 == t2 байт-в-байт; AST-дифф пуст.
fn roundtrip(src: &str, label: &str) {
    let ad = HaasAdapter::new();
    let a = ad.parse(src, "t.ngc").expect("parse ok").program;
    let t1 = ad.emit(&a).expect("emit ok").text;
    let t2 = ad.emit(&a).expect("emit ok").text;
    assert_eq!(t1, t2, "{label}: emit недетерминирован");
    let b = ad.parse(&t1, "rt.ngc").expect("reparse ok").program;
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    // Документированный value shift (COVERAGE §5.2/§2.2): дефолт ExactPath →
    // эмиссия G61 → reparse ExactStop (G61 = exact stop по значению).
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
// A. Golden-пары: EID AST → точный haas-текст
// ===========================================================================

#[test]
fn golden_header_mm() {
    // Заголовок всегда явный, НО БЕЗ G91.1 (режима центров в словаре Haas
    // нет — группы 04 не существует; I/J/K инкрементальные — дефолт).
    // path-строка: дефолт ExactPath → консервативный G61 (+warning).
    let (text, w) = emit_w("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nM30;\n");
    assert_eq!(
        text,
        "G21 G90 G94 G17\nG61\nG0 X10.0000 Y10.0000 Z5.0000\nM30\n"
    );
    assert_eq!(w.len(), 1, "exact_path warning: {w:?}");
    assert!(w[0].contains("exact_path эквивалента нет"), "{w:?}");
}

#[test]
fn golden_header_inch() {
    let text = emit_haas("G20 G90 G94 G17;\nG0 X1. Y1. Z0.5;\nM30;\n");
    assert!(text.starts_with("G20 G90 G94 G17\n"), "{text:?}");
}

#[test]
fn no_g91_1_in_header() {
    // G91.1 в словаре Haas НЕТ — эмиссия G91.1 сделала бы программу
    // непарсируемой (generic-ведро) и исказила бы контракт инкрементальных
    // центров (I/J/K от start — дефолт контроллера, канон §6).
    let text = emit_haas("G21 G90 G94 G17;\nG0 X1. Y1. Z1.;\nM30;\n");
    assert!(!text.contains("G91.1"), "{text:?}");
    assert!(!text.contains("G90.1"), "{text:?}");
}

#[test]
fn golden_path_exact_stop_no_warning() {
    // Haas G61 = exact_stop по ЗНАЧЕНИЮ (совпадает с Fanuc) — 1:1 без потерь.
    let (text, w) = emit_w("G21 G90 G94 G17;\nG61;\nG0 X1. Y1. Z1.;\nM30;\n");
    assert!(text.contains("\nG61\n"), "{text:?}");
    assert!(
        w.iter().all(|x| !x.contains("exact_path")),
        "exact_stop — без warning: {w:?}"
    );
}

#[test]
fn golden_path_blend_bare_and_t_warning() {
    // G64 = cancel G61 (bare blend — значение Haas); blend(t) hand-built:
    // t теряется с warning (квантификатора в словаре нет, COVERAGE §5.2).
    let (text, w) = emit_w("G21 G90 G94 G17;\nG64;\nG0 X1. Y1. Z1.;\nM30;\n");
    assert!(text.contains("\nG64\n"), "{text:?}");
    assert!(w.is_empty(), "bare blend — без потерь: {w:?}");
    // blend(t) — hand-built AST.
    let p = Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: PathMode::Blend { t: Some(num(0.5)) },
        frame_preset: None,
        ops: vec![
            Op::Rapid {
                start: StartPoint::Unknown,
                end: pt(1.0, 1.0, 1.0),
                state: state(Wcs::G54),
                src: src_span(),
                sub: None,
            },
            Op::ProgramEnd { src: src_span() },
        ],
    };
    let ad = HaasAdapter::new();
    // v0.2 #22: warnings — в owning-результате emit.
    let out = ad.emit(&p).expect("emit ok");
    let text = out.text;
    assert!(text.contains("\nG64\n"), "{text:?}");
    let w = out.warnings;
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("t теряется"), "{w:?}");
}

#[test]
fn restated_f_reemitted_from_ast_flag() {
    // T8 (v0.2 волна 3): restated-F из AST — F на каждом кадре с F-словом
    // (dotted-формат числа; Haas DPI: bare F → × least increment, поэтому
    // F с точкой — verbatim).
    let text = emit_haas("G21 G90 G94 G17;\nG0 X10.;\nG1 X20. F120.;\nG1 X30. F120.;\nM30;\n");
    let f_count = text.matches(" F120.0000").count();
    assert_eq!(f_count, 2, "restated-F переизлучён:\n{text}");
}

#[test]
fn inherited_f_not_reemitted() {
    // T8 (б): наследованный feed → F не переизлучается.
    let text = emit_haas("G21 G90 G94 G17;\nG0 X10.;\nG1 X20. F120.;\nG1 X30.;\nM30;\n");
    let f_count = text.matches(" F120.0000").count();
    assert_eq!(f_count, 1, "наследованный кадр без F:\n{text}");
}

#[test]
fn golden_preset_g92_first_block() {
    // frame_preset: выборка WCS (явная G54-строка) + G92 ДО любого motion-опа
    // (EQUIVALENCE §1.4); значения — координаты, присваиваемые позиции.
    let text = emit_haas("G21 G90 G94;\nG92 X1. Y2. Z3.;\nG0 X5. Y5. Z5.;\nM30;\n");
    assert_eq!(
        text,
        "G21 G90 G94 G17\nG61\nG54\nG92 X1.0000 Y2.0000 Z3.0000\nG0 X5.0000 Y5.0000 Z5.0000\nM30\n"
    );
    // G92.1 НЕ эмитится — кода сброса в словаре Haas нет; фиктивная очистка
    // запрещена (COVERAGE §5.2; Setting 129 `?` — doc-комментарий emit.rs).
    assert!(!text.contains("G92.1"), "{text:?}");
}

#[test]
fn preset_extended_wcs_warning() {
    // Пресетка в расширенном слоте p7 (#7): выборка G110 + warning
    // (зеркало W_WCS_EXTENDED; оговорка происхождения G59.x-таблиц).
    // Пресетка в p7 (#7): якорь первого motion-опа = значения пресетки
    // (EQUIVALENCE §1.4 — та же СК), start Known (вне точек разрыва).
    let mut p = hand_program(vec![Op::Rapid {
        start: StartPoint::Known(pt(1.0, 2.0, 3.0)),
        end: pt(5.0, 5.0, 5.0),
        state: state(Wcs::G59_1),
        src: src_span(),
        sub: None,
    }]);
    p.frame_preset = Some(eid_core::ast::FramePreset {
        values: pt(1.0, 2.0, 3.0),
        wcs: Wcs::G59_1,
    });
    let ad = HaasAdapter::new();
    // v0.2 #22: warnings — в owning-результате emit.
    let out = ad.emit(&p).expect("emit ok");
    let text = out.text;
    assert!(text.contains("\nG110\n"), "{text:?}");
    assert!(text.contains("G92 X1.0000"), "{text:?}");
    let w = out.warnings;
    assert!(
        w.iter().any(|x| x.contains("G110") && x.contains("G59.1")),
        "wcs_extended_slot warning с оговоркой G59.x: {w:?}"
    );
}

#[test]
fn golden_wcs_select_extended_p7_p9() {
    // Парс-сторона: G110/G111/G112 → p7..9 (этап 2); эмиссия: обратно
    // G110/G111/G112 + warning на каждую выборку (симметрия).
    // G61 в источнике — ExactStop (нет value shift ExactPath→G61→ExactStop
    // в reparse-диффе).
    let src = "G21 G90 G94 G17;\nG61;\nG0 X0. Y0. Z5.;\nG110;\nG0 X1. Y1. Z1.;\nG112;\nG0 X2. Y2. Z2.;\nM30;\n";
    let (text, w) = emit_w(src);
    assert!(text.contains("\nG110\n"), "{text:?}");
    assert!(text.contains("\nG112\n"), "{text:?}");
    assert!(
        !text.contains("G59.1"),
        "G59.1-кода в словаре Haas нет: {text:?}"
    );
    let ext = w.iter().filter(|x| x.contains("wcs_extended_slot")).count();
    assert_eq!(ext, 2, "warning на G110 и G112: {w:?}");
    // reparse ≡ источник (расширенный слот туда-обратно).
    let ad = HaasAdapter::new();
    let a = ad.parse(src, "t.ngc").expect("parse ok").program;
    let b = ad.parse(&text, "rt.ngc").expect("reparse ok").program;
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    assert!(d.is_empty(), "{d:?}");
}

#[test]
fn golden_wcs_define_l2_domain() {
    // WCS_DEFINE P1..P6 → G10 L2 (1:1); полная форма абсолютная.
    let text = emit_haas("G21 G90;\nG10 L2 P3 G90 X1. Y2. Z3.;\nG0 X1. Y1. Z1.;\nM30;\n");
    assert!(
        text.contains("G10 L2 P3 X1.0000 Y2.0000 Z3.0000"),
        "{text:?}"
    );
}

#[test]
fn golden_decimal_point_always() {
    // DPI-ловушка эмиссии: bare integer в Haas = ×least increment (machinists
    // notation) — числа координат ВСЕГДА с точкой (4dp). Bare-вход X5 Y10 Z2
    // парсится как 0.005/0.010/0.002 и ЭМИТИТСЯ dotted → reparse verbatim.
    let text = emit_haas("G21 G90 G94 G17;\nG0 X5 Y10 Z2;\nM30;\n");
    assert!(text.contains("G0 X0.0050 Y0.0100 Z0.0020"), "{text:?}");
    // Ни одного bare-axis-слова в выходе (буква+целое без точки).
    for word in [" X5 ", " Y10 ", " Z2 ", " X10 ", " F100 "] {
        assert!(!text.contains(word), "bare `{word}` в выходе: {text:?}");
    }
}

#[test]
fn golden_dwell_p_dot_reverse() {
    // P-DOT-RULE ОБРАТНЫЙ (бриф этапа 3): seconds·1000 целое → bare P (мс!);
    // sub-ms остаток → dotted P (сек). X/U-fallback НЕТ (форм в словаре нет).
    // 2.5 s → P2500; 0.5 s → P500; 0.0005 s → P0.0005 (dotted).
    let text = emit_haas("G21 G90 G94;\nG4 P2.5;\nM30;\n");
    assert!(text.contains("G4 P2500"), "{text:?}");
    let text = emit_haas("G21 G90 G94;\nG4 P500;\nM30;\n");
    assert!(text.contains("G4 P500"), "{text:?}");
    let text = emit_haas("G21 G90 G94;\nG4 P0.0005;\nM30;\n");
    assert!(text.contains("G4 P0.0005"), "{text:?}");
    assert!(!text.contains("G4 X"), "X-форм dwell у Haas нет: {text:?}");
}

#[test]
fn golden_arc_plane_switch_and_ij_pairs() {
    // Плоскость из нормали: смена — отдельной строкой; центр — in-plane пара
    // (G18→I+K, G19→J+K); инкремент от старта; R не эмитится никогда.
    // Четверти-круги с согласованными радиусами:
    // G18 (ZX, нормаль +Y): старт (0,·,5), центр (0,·,0) → I0 K-5, конец
    //   (5,·,0), CCW 90° (углы 0°→90° в паре (Z,X)).
    // G19 (YZ, нормаль +X): старт (5,·,0) — конец G18-дуги, центр
    //   (5,·,−5) → J0 K-5, конец (5,·,−5), CW 90° (углы 90°→0° в паре
    //   (Y,Z)).
    let text = emit_haas(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG18 G3 X5. Y0. Z0. I0. K-5. F100.;\nG19 G2 X5. Y5. Z-5. J0. K-5. F100.;\nM30;\n",
    );
    assert!(text.contains("\nG18\n"), "{text:?}");
    assert!(text.contains("\nG19\n"), "{text:?}");
    assert!(!text.contains(" R"), "R не эмитится: {text:?}");
    // G18-дуга: пара I+K (Y — нормаль), инкремент от старта (0,0,5).
    let line18 = text
        .lines()
        .find(|l| l.starts_with("G3 "))
        .expect("G3 line");
    assert_eq!(
        line18, "G3 X5.0000 Y0.0000 Z0.0000 I0.0000 K-5.0000 F100.0000",
        "{line18}"
    );
    // G19-дуга: пара J+K (X — нормаль), инкремент от старта (5,0,0).
    // v0.2 волна 3 (SPEC §7.4a): кадр-источник несёт F-слово (restated —
    // то же значение) → эмиттер ПЕРЕИЗЛУЧАЕТ F (rs274 ассертит SET_FEED_
    // RATE на каждом F-кадре; прежнее «F модален — без повтора» теряло
    // маркер кадра — закрытый класс дыры e2e48 #21).
    let line19 = text
        .lines()
        .find(|l| l.starts_with("G2 "))
        .expect("G2 line");
    assert_eq!(
        line19, "G2 X5.0000 Y5.0000 Z-5.0000 J0.0000 K-5.0000 F100.0000",
        "{line19}"
    );
}

#[test]
fn golden_full_circle_no_end_words() {
    // Полный круг (sweep=360°, pitch=0) — БЕЗ end-слов (`G2 I_ J_` — легальный
    // Haas-формат, официальный ман); слова end==start при reparse квантуются
    // в zero-length drop.
    let src = "G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG2 I-5. J0. F100.;\nM30;\n";
    let text = emit_haas(src);
    let line = text
        .lines()
        .find(|l| l.starts_with("G2 "))
        .expect("G2 line");
    assert_eq!(line, "G2 I-5.0000 J0.0000 F100.0000", "без X/Y/Z end-слов");
    // reparse: end-слова отсутствуют → полный круг по геометрии (§3 v0.18).
    let ad = HaasAdapter::new();
    let a = ad.parse(src, "t.ngc").expect("parse ok").program;
    let b = ad.parse(&text, "rt.ngc").expect("reparse ok").program;
    let d = semantic_diff(&a, &b, &DiffConfig::default());
    // value shift ExactPath→ExactStop (эмиссия G61 — COVERAGE §5.2).
    let d: Vec<_> = d
        .into_iter()
        .filter(|x| {
            !matches!(x, Divergence::MetaField { field: "path_mode" }
            if matches!(a.path_mode, PathMode::ExactPath)
                && matches!(b.path_mode, PathMode::ExactStop))
        })
        .collect();
    assert!(d.is_empty(), "полный круг fixed-point: {d:?}");
}

#[test]
fn golden_helical_full_turn_end_word() {
    // Винтовой полный оборот (sweep=360°, pitch≠0) — ТОЛЬКО end-слово
    // вне-плоскостной оси (in-plane наследуют старт).
    let text =
        emit_haas("G21 G90 G94 G17;\nG0 X10. Y10. Z5.;\nG2 X10. Y10. Z6. I-5. J0. F100.;\nM30;\n");
    let line = text
        .lines()
        .find(|l| l.starts_with("G2 "))
        .expect("G2 line");
    assert_eq!(line, "G2 Z6.0000 I-5.0000 J0.0000 F100.0000", "{line}");
}

#[test]
fn golden_machine_frame() {
    // frame=machine → префикс `G53 G0|G1` (одномодальный, явный G0/G1).
    let text = emit_haas(
        "G21 G90;\nG10 L2 P1 G90 X1. Y2. Z3.;\nG0 X5. Y5. Z5.;\nG53 G0 X0 Y0 Z0;\nM30;\n",
    );
    assert!(text.contains("G53 G0 X0.0000 Y0.0000 Z0.0000"), "{text:?}");
}

#[test]
fn golden_tool_stop_end() {
    // T## M06 в одном блоке; M0/M1; M30; хвост M5/M9 НЕ эмитится (Haas M30
    // сам глушит — ruling COVERAGE §5).
    let text = emit_haas("G21 G90;\nT3 M06;\nM8;\nG0 X1. Y1. Z1.;\nM1;\nM0;\nM30;\n");
    assert!(text.contains("T3 M06"), "{text:?}");
    assert!(text.contains("\nM8\n"), "{text:?}");
    assert!(text.contains("\nM1\n"), "{text:?}");
    assert!(text.contains("\nM0\n"), "{text:?}");
    assert!(text.ends_with("M30\n"), "{text:?}");
}

#[test]
fn golden_spindle_coolant_deltas() {
    // Порядок дельт снапшота: WCS → шпиндель (M3 S) → СОЖ (M8); M9 гасит
    // до включений В ОДНОЙ дельте; финальные M5/M9 БЕЗ последующего опа
    // не воспроизводятся (deltas only; Haas M30 сам глушит — ruling §5).
    let text = emit_haas(
        "G21 G90;\nS5000;\nM3;\nG0 X1. Y1. Z1.;\nM8;\nG1 X2. Y1. Z1. F100.;\nM5;\nM9;\nM30;\n",
    );
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.contains(&"M3 S5000.0000"), "{lines:?}");
    assert!(lines.contains(&"M8"), "{lines:?}");
    let i_m3 = lines
        .iter()
        .position(|l| *l == "M3 S5000.0000")
        .expect("M3 line");
    let i_m8 = lines.iter().position(|l| *l == "M8").expect("M8");
    assert!(i_m3 < i_m8, "шпиндель до СОЖ: {lines:?}");
    // M5/M9 без последующего опа не эмитятся.
    assert!(!lines.contains(&"M5"), "{lines:?}");
    assert!(!lines.contains(&"M9"), "{lines:?}");
    assert_eq!(lines.last(), Some(&"M30"));
}

#[test]
fn golden_raw_verbatim_crlf() {
    // RAW same-dialect fidelity: payload дословно, включая \r (CRLF-хвост).
    let src = "G21 G90;\nG43 H2;\r\nG0 X1. Y1. Z1.;\nM30;\n";
    let text = emit_haas(src);
    assert!(text.contains("G43 H2;\r\n"), "{text:?}");
}

#[test]
fn golden_feed_first_derived_sub1() {
    // F-правило sub=1 (§7.4a v0.18): F на первом производном опе каждого
    // F-кадра (traverse, sub=1); повторный F-кадр с тем же значением — на
    // следующем traverse (sub=5) НЕ дублируется (F модален).
    let text = emit_haas(
        "G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nG99 G81 X10. Z-5. R2. F100. L2;\nG80;\nM30;\n",
    );
    let f_count = text.lines().filter(|l| l.contains("F100.0000")).count();
    assert_eq!(f_count, 1, "F один раз (sub=1), modal далее: {text:?}");
    // Развёртка как ходы: 4 производных опа первой итерации (traverse нулевой
    // на повторе — drop) + feed/retract второго.
    let sub_lines = text
        .lines()
        .filter(|l| l.starts_with("G0 ") || l.starts_with("G1 "))
        .count();
    // позиционирующий + (traverse+approach+G1+retract) + повтор (G1+retract,
    // traverse/approach нулевые — drop) = 7.
    assert_eq!(sub_lines, 7, "{text:?}");
}

// ===========================================================================
// B. Legality — типизированные условия (INTERFACE-V0.2 §2.5 В1)
// ===========================================================================

fn legality_table() -> eid_haas::emit::HaasEmissionLegality {
    HaasAdapter::new().emission_legality()
}

/// Таблица = данные, КОДЫ — тотальные match-функции: каждая пара
/// (условие, код) в таблице обязана совпадать с `cond.code()`; каждое
/// условие объявлено ровно один раз; все warning-условия с текстом.
/// Compile-time полноту гарантирует сам enum (match без Option) — таблица
/// не может «разойтись» с эмиттером молча (паник-точек на emit-пути НЕТ).
#[test]
fn legality_table_complete() {
    let t = legality_table();
    let all = [
        HaasEmissionCondition::WcsSetHere,
        HaasEmissionCondition::WcsDefinePOutsideL2Domain,
        // Этап 8: неполная profile wcs_extended_map (незмапленный слот).
        HaasEmissionCondition::WcsMapIncomplete,
        HaasEmissionCondition::CoolantMistUnrepresentable,
        HaasEmissionCondition::MachineFrameArc,
        HaasEmissionCondition::MotionFeedNone,
        HaasEmissionCondition::ArcUnknownStart,
        HaasEmissionCondition::ArcNormalNonCanonical,
        HaasEmissionCondition::RawPayloadNotHex,
        HaasEmissionCondition::ProgramEndMissing,
        HaasEmissionCondition::ProgramEndDuplicate,
        HaasEmissionCondition::ProgramEndNotLast,
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
    // Тексты warning нетривиальны и соответствуют match-функции.
    for w in [
        HaasEmissionWarning::PathModeExactPath,
        HaasEmissionWarning::PathModeBlendT,
        HaasEmissionWarning::WcsExtendedSlot,
    ] {
        let (_, text) = t
            .warnings
            .iter()
            .find(|(c, _)| *c == w)
            .unwrap_or_else(|| panic!("warning {} не объявлен", w.as_str()));
        assert_eq!(*text, w.text(), "текст {} = тотальный match", w.as_str());
        assert!(text.len() > 20, "текст нетривиален: {text}");
    }
    // Легальные виды: 9 опов + frame_preset; op_kind покрывает все 10 видов.
    assert_eq!(t.legal.len(), 10);
    for kind in t.legal.clone() {
        assert!(
            [
                "rapid",
                "linear",
                "arc",
                "dwell",
                "tool_change",
                "wcs_define",
                "program_stop",
                "program_end",
                "raw",
                "frame_preset",
            ]
            .contains(&kind),
            "{kind}"
        );
    }
    let _ = op_kind; // используется в golden-тестах через программные AST
}

/// Кросс-чек поведения: каждый illegal-код, фактически возвращённый
/// эмиттером, совпадает с `cond.code()` (единый путь отказа).
#[test]
fn legality_behavior_cross_check() {
    // wcs_set_here → SEMANTICS (ловушка №2b).
    let p = hand_program(vec![Op::WcsSetHere {
        p: 1,
        delta: pt(1.0, 1.0, 1.0),
        state: state(Wcs::G54),
        src: src_span(),
    }]);
    assert_eq!(
        err_code_of(HaasAdapter::new().emit(&p)),
        HaasEmissionCondition::WcsSetHere.code()
    );
    // wcs_define P7 → SYNTAX (L2-домен P0..6; L20 = profile-опция).
    let p = hand_program(vec![Op::WcsDefine {
        p: 7,
        origin: pt(1.0, 1.0, 1.0),
        state: state(Wcs::G54),
        src: src_span(),
    }]);
    assert_eq!(
        err_code_of(HaasAdapter::new().emit(&p)),
        HaasEmissionCondition::WcsDefinePOutsideL2Domain.code()
    );
    // coolant.mist → SEMANTICS (M07 = Shower — другой канал).
    let mut st = state(Wcs::G54);
    st.coolant.mist = true;
    let p = hand_program(vec![Op::Rapid {
        start: StartPoint::Unknown,
        end: pt(1.0, 1.0, 1.0),
        state: st,
        src: src_span(),
        sub: None,
    }]);
    assert_eq!(
        err_code_of(HaasAdapter::new().emit(&p)),
        HaasEmissionCondition::CoolantMistUnrepresentable.code()
    );
    // Дуга в machine-frame → SEMANTICS (геометрия — согласованная четверть
    // G18-круга в machine-координатах; anchor через WcsDefine origin:
    // reading_M = W + O = (5,5,5) + (1,2,3) = (6,7,8); центр (6,7,3),
    // конец = центр + Rot(+Y, ccw 90°)·(0,0,5) = (11,7,3), CCW 90°).
    let p = hand_program(vec![
        Op::WcsDefine {
            p: 1,
            origin: pt(1.0, 2.0, 3.0),
            state: state(Wcs::G54),
            src: src_span(),
        },
        Op::Rapid {
            start: StartPoint::Unknown, // точка разрыва (a) — первый оп
            end: pt(5.0, 5.0, 5.0),
            state: state(Wcs::G54),
            src: src_span(),
            sub: None,
        },
        Op::Arc {
            start: StartPoint::Known(pt(6.0, 7.0, 8.0)),
            end: pt(11.0, 7.0, 3.0),
            center: pt(6.0, 7.0, 3.0),
            normal: pt(0.0, 1.0, 0.0),
            dir: Dir::Ccw,
            sweep_deg: num(90.0),
            pitch: num(0.0),
            state: State {
                feed: Some(num(100.0)),
                spindle: Spindle::Off,
                coolant: Coolant::default(),
                tool: None,
                frame: Frame::Machine,
                wcs: Wcs::G54,
                feed_restated: false,
            },
            src: src_span(),
            sub: None,
        },
    ]);
    assert_eq!(
        err_code_of(HaasAdapter::new().emit(&p)),
        HaasEmissionCondition::MachineFrameArc.code()
    );
}

// ===========================================================================
// C. Детерминизм + round-trip fixed-point
// ===========================================================================

const FULL_PROGRAM: &str = "G21 G90 G94 G17;\n\
G0 X10. Y10. Z5.;\n\
G1 X20. Y10. Z2. F300.;\n\
G3 X30. Y20. I0. J10.;\n\
G2 X30. Y20. I-5. J0.;\n\
G2 I-5.;\n\
G4 P250;\n\
G4 P0.0005;\n\
G10 L2 P2 X5. Y0. Z0.;\n\
G55;\n\
G0 X40. Y40. Z5.;\n\
T3 M06;\n\
M8;\n\
G53 G0 X0. Y0. Z0.;\n\
M1;\n\
M30;\n";

#[test]
fn emit_deterministic_double() {
    let p = parse_haas(FULL_PROGRAM);
    let ad = HaasAdapter::new();
    let t1 = ad.emit(&p).expect("emit ok").text;
    let t2 = ad.emit(&p).expect("emit ok").text;
    assert_eq!(t1, t2);
}

#[test]
fn roundtrip_full_fixed_point() {
    // haas → EID → haas: t1 == t2 байт-в-байт; AST-дифф пуст.
    roundtrip(FULL_PROGRAM, "full");
}

#[test]
fn roundtrip_cycle_unroll() {
    // Развёртка §7.7 (Haas-семантика: L-повторы, P-dot dwell) эмитится как
    // ходы self-собой; reparse ≡ (traverse/approach нулевые — drop).
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
fn roundtrip_g92_preset_and_raw_tail() {
    // G92-пресетка + Raw-хвост после PEND (subs-тела — норма Haas §4.4):
    // tail-Raw эмитится verbatim; O-код в хвосте — тоже Raw.
    roundtrip(
        "G21 G90;\nG92 X1. Y1. Z1.;\nG0 X1. Y1. Z1.;\nM30;\nO0010;\nG0 X5.;\n",
        "g92+raw-tail",
    );
}

#[test]
fn roundtrip_extended_wcs() {
    // Расширенные слоты p7..9 → G110–G112 → reparse p7..9 (симметрия).
    roundtrip(
        "G21 G90 G94 G17;\nG110;\nG0 X1. Y1. Z1. F50.;\nG111;\nG1 X2. Y2. Z2. F50.;\nM30;\n",
        "ext-wcs",
    );
}

// ===========================================================================
// D. Cross-check v0.18 — 3-я поверхность развёртки
// ===========================================================================

#[test]
fn mirror_v018_golden_cross_check_19_20_21() {
    // haas-парсер обязан давать на v0-файлах #19/#20/#21 те же AST, что
    // golden.eid (regen от D, rs274-конформный, commit 4705480) —
    // semantic_diff = 0. Источники — linuxcnc-диалект (значения dotted →
    // Haas-вербатим; dotted P → секунды — P-dot-rule обоих направлений);
    // канон-шаблон §7.7 + клиренс §7.4a v0.18 — общий с fanuc/linuxcnc
    // (Haas-отличия: L-повторы — в этих векторах L нет; Q-формы 1:1).
    use eid_core::adapter::EidTextAdapter;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tier_a/v0");
    for (dir, src_name) in [
        ("19_peck_G83_retract", "peck_G83_retract.linuxcnc.ngc"),
        ("20_chipbreak_G73", "chipbreak_G73.linuxcnc.ngc"),
        ("21_bore_G86_restart", "bore_G86_restart.linuxcnc.ngc"),
    ] {
        let source =
            std::fs::read_to_string(root.join(dir).join(src_name)).expect("v0 source readable");
        let golden =
            std::fs::read_to_string(root.join(dir).join("golden.eid")).expect("golden readable");
        let a = HaasAdapter::new()
            .parse(&source, src_name)
            .expect("haas parse ok")
            .program;
        let b = EidTextAdapter::new()
            .parse(&golden, "golden.eid")
            .expect("golden parses")
            .program;
        let d = semantic_diff(&a, &b, &DiffConfig::default());
        assert!(d.is_empty(), "{dir}: divergences: {d:?}");
    }
}

// ===========================================================================
// E. Регрессии пути нелегального входа в emit
// ===========================================================================

#[test]
fn illegal_feed_none_and_end_invariants() {
    // motion_feed_none → INVAL.
    let p = hand_program(vec![Op::Linear {
        start: StartPoint::Known(pt(0.0, 0.0, 0.0)),
        end: pt(1.0, 1.0, 1.0),
        state: State {
            feed: None,
            spindle: Spindle::Off,
            coolant: Coolant::default(),
            tool: None,
            frame: Frame::Wcs,
            wcs: Wcs::G54,
            feed_restated: false,
        },
        src: src_span(),
        sub: None,
    }]);
    assert_eq!(
        err_code_of(HaasAdapter::new().emit(&p)),
        HaasEmissionCondition::MotionFeedNone.code()
    );
    // program_end_missing → INVAL.
    let p = Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: PathMode::ExactPath,
        frame_preset: None,
        ops: vec![Op::Rapid {
            start: StartPoint::Unknown,
            end: pt(1.0, 1.0, 1.0),
            state: state(Wcs::G54),
            src: src_span(),
            sub: None,
        }],
    };
    assert_eq!(
        err_code_of(HaasAdapter::new().emit(&p)),
        HaasEmissionCondition::ProgramEndMissing.code()
    );
    // program_end_duplicate → INVAL.
    let mut p = hand_program(vec![]);
    p.ops.push(Op::ProgramEnd { src: src_span() });
    assert_eq!(
        err_code_of(HaasAdapter::new().emit(&p)),
        HaasEmissionCondition::ProgramEndDuplicate.code()
    );
    // program_end_not_last → INVAL (не-Raw оп после PEND).
    let p = hand_program(vec![
        Op::ProgramEnd { src: src_span() },
        Op::Rapid {
            start: StartPoint::Unknown,
            end: pt(1.0, 1.0, 1.0),
            state: state(Wcs::G54),
            src: src_span(),
            sub: None,
        },
    ]);
    assert_eq!(
        err_code_of(HaasAdapter::new().emit(&p)),
        HaasEmissionCondition::ProgramEndNotLast.code()
    );
    // raw_payload_not_hex → INVAL.
    let p = hand_program(vec![Op::Raw {
        payload_hex: "ZZ".into(),
        reason: "test".into(),
        code: ErrorCode::UnsupportedSyntax,
        src: src_span(),
    }]);
    assert_eq!(
        err_code_of(HaasAdapter::new().emit(&p)),
        HaasEmissionCondition::RawPayloadNotHex.code()
    );
    // arc_unknown_start + arc_normal_non_canonical → INVAL.
    let p = hand_program(vec![Op::Arc {
        start: StartPoint::Unknown,
        end: pt(5.0, 0.0, 0.0),
        center: pt(2.5, 0.0, 0.0),
        normal: pt(0.0, 1.0, 0.0),
        dir: Dir::Cw,
        sweep_deg: num(180.0),
        pitch: num(0.0),
        state: state(Wcs::G54),
        src: src_span(),
        sub: None,
    }]);
    let Err(d) = HaasAdapter::new().emit(&p) else {
        panic!("ожидался Err")
    };
    assert!(d
        .iter()
        .any(|x| x.code == HaasEmissionCondition::ArcUnknownStart.code()));
    let p = hand_program(vec![Op::Arc {
        start: StartPoint::Known(pt(0.0, 0.0, 0.0)),
        end: pt(5.0, 0.0, 0.0),
        center: pt(2.5, 0.0, 0.0),
        normal: pt(0.7, 0.7, 0.1),
        dir: Dir::Cw,
        sweep_deg: num(180.0),
        pitch: num(0.0),
        state: state(Wcs::G54),
        src: src_span(),
        sub: None,
    }]);
    let Err(d) = HaasAdapter::new().emit(&p) else {
        panic!("ожидался Err")
    };
    assert!(d
        .iter()
        .any(|x| x.code == HaasEmissionCondition::ArcNormalNonCanonical.code()));
}

// ===========================================================================
// Этап 8 (agent C, emission-профили): профильные опции эмиссии Haas —
// program_end M30/M02 (COVERAGE §5.2 «выбор профиля»), allow_r_arcs
// (R-форма + notice на каждой R-дуге; дефолт false = всегда IJK),
// g59_map (G59.x → G154 Pn + notice «map применён, таблица подменена»;
// неполная мапа → отказ wcs_map_incomplete с перечнем незмапленных).
// Опции входят через HaasAdapter::emit_with_opts (trait v0.2 заморожен);
// Default = прежняя эмиссия байт-в-байт (мапа off: p7..9 → G110–G112 +
// warning wcs_extended_slot — приёмка этапа 3 сохранена).
// ===========================================================================

// StartPoint уже в use-блоке выше (eid_core::ast).
use eid_haas::emit::{emit_with_profile, HaasEmissionOpts, HaasG59Map, HaasProgramEnd};

/// Эмиссия hand-программы с опциями (text + warnings).
fn emit_with_opts_haas(p: &Program, opts: &HaasEmissionOpts) -> (String, Vec<String>) {
    let mut w = Vec::new();
    let out = emit_with_profile(p, opts, &mut w).expect("emit_with_profile ok");
    (out.text, out.warnings)
}

/// CCW 90° (центр (10,0), старт (0,0), end (10,-10), R = +10) и CCW 270°
/// (старт (10,-10), тот же центр (I0 J10 — инкремент от старта), end (0,0),
/// R = −10 — знак по sweep > 180°). G61 явно — без path-mode value shift.
fn two_arcs_program_haas() -> Program {
    parse_haas(
        "G21 G90 G94 G17;\nF100.0;\nG61;\nG0 X0. Y0. Z5.;\nG3 X10. Y-10. I10. J0.;\nG3 X0. Y0. I0. J10.;\nM30;\n",
    )
}

#[test]
fn stage8_default_opts_byte_identical_to_plain_emit() {
    let p = two_arcs_program_haas();
    let plain = HaasAdapter::new().emit(&p).expect("emit ok").text;
    let (with_default, _) = emit_with_opts_haas(&p, &HaasEmissionOpts::default());
    assert_eq!(
        plain, with_default,
        "Default-опции = прежняя эмиссия байт-в-байт"
    );
}

#[test]
fn stage8_program_end_m02() {
    // COVERAGE §5.2: M30 дефолт (Haas-конвенция, reset-семантика) vs M02 —
    // выбор профиля.
    let p = parse_haas("G21 G90 G94 G17;\nG0 X0. Y0. Z5.;\nM30;\n");
    let (m30, _) = emit_with_opts_haas(&p, &HaasEmissionOpts::default());
    assert!(m30.trim_end().ends_with("M30"), "{m30}");
    let opts = HaasEmissionOpts {
        program_end: HaasProgramEnd::M02,
        ..HaasEmissionOpts::default()
    };
    let (m02, _) = emit_with_opts_haas(&p, &opts);
    assert!(
        m02.trim_end().ends_with("M02") && !m02.contains("M30"),
        "{m02}"
    );
    let (m02_again, _) = emit_with_opts_haas(&p, &opts);
    assert_eq!(m02, m02_again, "детерминизм");
}

#[test]
fn stage8_r_arcs_false_ijk_true_r_with_notice() {
    let p = two_arcs_program_haas();
    // false (дефолт) = всегда IJK.
    let (ijk, _) = emit_with_opts_haas(&p, &HaasEmissionOpts::default());
    assert!(ijk.contains("I10.0000") && ijk.contains("J0.0000"), "{ijk}");
    assert!(!ijk.contains(" R"), "{ijk}");
    // true = R-форма + notice на КАЖДОЙ R-дуге (COVERAGE §5.2).
    let opts = HaasEmissionOpts {
        allow_r_arcs: true,
        ..HaasEmissionOpts::default()
    };
    let (r_text, w) = emit_with_opts_haas(&p, &opts);
    assert!(r_text.contains("R10.0000"), "{r_text}");
    assert!(!r_text.contains("I10.0000"), "{r_text}");
    let notices = w.iter().filter(|s| s.contains("R-форма применена")).count();
    assert_eq!(notices, 2, "notice на каждой R-дуге: {w:?}");
    assert!(r_text.contains(" R10.0000"), "90° → R>0: {r_text}");
    assert!(r_text.contains(" R-10.0000"), "270° → R<0: {r_text}");
}

#[test]
fn stage8_r_arcs_full_circle_stays_ijk() {
    // Полный круг — ВСЕГДА IJK даже при allow_r_arcs (R при start==end
    // неоднозначен, SPEC §6) + notice про fallback.
    let p = parse_haas("G21 G90 G94 G17;\nF100.0;\nG0 X5. Y0. Z5.;\nG2 I-5. J0. F100.;\nM30;\n");
    let opts = HaasEmissionOpts {
        allow_r_arcs: true,
        ..HaasEmissionOpts::default()
    };
    let (text, w) = emit_with_opts_haas(&p, &opts);
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
    let ad = HaasAdapter::new();
    let ast_ijk = two_arcs_program_haas();
    let opts = HaasEmissionOpts {
        allow_r_arcs: true,
        ..HaasEmissionOpts::default()
    };
    let (r_text, _) = emit_with_opts_haas(&ast_ijk, &opts);
    let ast_r = ad
        .parse(&r_text, "rt.ngc")
        .expect("parse R-текста ok")
        .program;
    let (ijk_text, _) = emit_with_opts_haas(&ast_ijk, &HaasEmissionOpts::default());
    let ast_ijk_re = ad
        .parse(&ijk_text, "rt.ngc")
        .expect("parse IJK-текста ok")
        .program;
    let d = semantic_diff(&ast_r, &ast_ijk_re, &DiffConfig::default());
    assert!(d.is_empty(), "R ≡ IJK (та же дуга): {d:?}");
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
    let d = semantic_diff(&ast_ijk, &ast_r, &DiffConfig::default());
    assert!(d.is_empty(), "parse(R) ≡ исходный AST: {d:?}");
}

#[test]
fn stage8_wcs_map_off_default_unchanged_g110() {
    // Мапа выключена (дефолт) = приёмка этапа 3: p7 → G110 + warning
    // wcs_extended_slot; байт-равно plain emit.
    let p = hand_program(vec![Op::Rapid {
        start: StartPoint::Unknown,
        end: pt(1.0, 1.0, 1.0),
        state: state(Wcs::G59_1),
        src: src_span(),
        sub: None,
    }]);
    let (text, w) = emit_with_opts_haas(&p, &HaasEmissionOpts::default());
    assert!(text.contains("G110"), "{text}");
    assert!(w.iter().any(|s| s.contains("wcs_extended_slot")), "{w:?}");
    let plain = HaasAdapter::new().emit(&p).expect("emit ok");
    assert_eq!(plain.text, text);
    assert_eq!(plain.warnings, w);
}

#[test]
fn stage8_wcs_map_on_g154_pn_notice_once() {
    // Мапа включена → G154 Pn + notice «map применён, таблица подменена»
    // РОВНО ОДИН раз на эмиссию (НЕ дефолт — аналогия golden_preset_no_bake).
    let p = hand_program(vec![
        Op::Rapid {
            start: StartPoint::Unknown,
            end: pt(1.0, 1.0, 1.0),
            state: state(Wcs::G59_1),
            src: src_span(),
            sub: None,
        },
        Op::Rapid {
            start: StartPoint::Unknown,
            end: pt(2.0, 2.0, 2.0),
            state: state(Wcs::G59_2),
            src: src_span(),
            sub: None,
        },
    ]);
    let opts = HaasEmissionOpts {
        g59_map: Some(HaasG59Map {
            g59_1: Some(1),
            g59_2: Some(2),
            g59_3: None,
        }),
        ..HaasEmissionOpts::default()
    };
    let (text, w) = emit_with_opts_haas(&p, &opts);
    assert!(text.contains("G154 P1"), "{text}");
    assert!(text.contains("G154 P2"), "{text}");
    assert!(!text.contains("G110") && !text.contains("G111"), "{text}");
    let notices: Vec<&String> = w.iter().filter(|s| s.contains("map применён")).collect();
    assert_eq!(notices.len(), 1, "notice один на эмиссию: {w:?}");
    assert!(
        notices[0].contains("G59.1→G154 P1") && notices[0].contains("G59.2→G154 P2"),
        "notice перечисляет таблицу: {}",
        notices[0]
    );
}

#[test]
fn stage8_wcs_map_incomplete_refusal_lists_unmapped() {
    // Неполная мапа: G59.1 замаплен, G59.2 нет, в программе есть оба →
    // ОДИН отказ wcs_map_incomplete (SYN) с перечнем незмапленных.
    let p = hand_program(vec![
        Op::Rapid {
            start: StartPoint::Unknown,
            end: pt(1.0, 1.0, 1.0),
            state: state(Wcs::G59_1),
            src: src_span(),
            sub: None,
        },
        Op::Rapid {
            start: StartPoint::Unknown,
            end: pt(2.0, 2.0, 2.0),
            state: state(Wcs::G59_2),
            src: src_span(),
            sub: None,
        },
    ]);
    let opts = HaasEmissionOpts {
        g59_map: Some(HaasG59Map {
            g59_1: Some(1),
            g59_2: None,
            g59_3: None,
        }),
        ..HaasEmissionOpts::default()
    };
    let err = HaasAdapter::new().emit_with_opts(&p, &opts);
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
    // frame_preset.wcs = расширенный слот: мапа включена → G154 P1 первым
    // блоком + notice (вместо G110 + wcs_extended_slot).
    let mut p = hand_program(vec![Op::Rapid {
        start: StartPoint::Unknown,
        end: pt(1.0, 1.0, 1.0),
        state: state(Wcs::G54),
        src: src_span(),
        sub: None,
    }]);
    p.frame_preset = Some(eid_core::ast::FramePreset {
        wcs: Wcs::G59_1,
        values: pt(1.0, 2.0, 3.0),
    });
    let opts = HaasEmissionOpts {
        g59_map: Some(HaasG59Map {
            g59_1: Some(1),
            g59_2: None,
            g59_3: None,
        }),
        ..HaasEmissionOpts::default()
    };
    let (text, w) = emit_with_opts_haas(&p, &opts);
    assert!(text.contains("G154 P1\nG92 X"), "{text}");
    assert!(w.iter().any(|s| s.contains("map применён")), "{w:?}");
    assert!(
        !w.iter().any(|s| s.contains("wcs_extended_slot")),
        "мапленный слот не дублирует старый warning: {w:?}"
    );
}
