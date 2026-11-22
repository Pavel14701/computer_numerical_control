//! Интеграционные тесты канонизатора GRBL → EID/L1 (этап 2, Агент C).
//! Пилоты и негативы — corpus/tier_a/v0; допуски — EQUIVALENCE §4 (eps 0.001).
//!
//! ЧЕК-ЛИСТ покрытия GRBL-колонки COVERAGE (spec_cov_*; правило → тест):
//! LEXICAL: UnexpectedChar→errors(lexer), unclosed-paren→errors(lexer),
//!   word-without-number→errors(lexer), exponent→spec_cov_lex_exponent,
//!   word>32→errors(lexer), $$/$110=/$garbage→corpus_47+spec_cov_lex_dollar,
//!   N-word→lexer, %→markers, block-delete→lexer+markers.
//! MODAL-CONFLICT (→INVALID): motion→modal_violation_detected,
//!   plane/distance/feed/units/wcs/path→spec_cov_modal_conflicts; solo —
//!   spec_cov_modal_solo.
//! ARC §6: IJK G17/18/19×cw/ccw→spec_cov_arc_planes_ijk, helix±→pilot_46(+)/
//!   spec_cov_arc_planes_ijk(−), R±→pilot_11+spec_cov_arc_r_negative,
//!   R+pitch→spec_cov_arc_r_pitch, radius-miss→n5a, chord>2R→n5b,
//!   R-full-circle→n19, rotation-eq→позитивы всех дуг (проходят §6.3).
//! CYCLES §7 (GRBL: циклы N/A словаря): G81/G83+L→spec_cov_cycles_raw_syntax,
//!   G84 → spec_cov_cycles_raw_syntax (матрица N/A→SYNTAX; SEMANTICS —
//!   LinuxCNC-диспозиция, расхождение с буквой брифа — отчёт), G80-naked→
//!   unit_g80_naked_coords_invalid; разворот/L-лимит — D-зона.
//!   SPEC v0.18 §7.4a: G73/G82/G85/G86/G89 → spec_cov_cycles_raw_classes
//!   (вне словаря → Raw{SYNTAX}, разворачивания нет — клиренс/G73-модель/
//!   G86-P являются константами D-канонизатора циклов, у grbl поверхности
//!   нет; эмиссионное зеркало — F-кадр sub=1 в grbl_emit).
//! FRAME/WCS §5.3: G10L2/L20→g10_l20_and_l2+pilot_36, L20-mid→n15,
//!   double-preset→n17 (v0.20: Ok, per-axis merge), G92-mid→v020_g92_mid_*
//!   (v0.20: Ok + chaining, Raw-носитель), G92-partial→spec_cov_frame_g92_partial,
//!   G92.1→spec_cov_frame_g92_1_raw (v0.20: Raw{TABLE}), G53-conditions→#37+n1+n16,
//!   G53-without-motion→spec_cov_frame_g53_no_motion, wcs-solo→spec_cov_frame_wcs_solo.
//! v0.20 SPEC §5.2 (зонд-сверка target/probe/g92): bare-T ABSORB→
//!   v020_bare_t_absorb_selected_tool, tail-after-PEND→v020_n12_13_*+
//!   v020_tail_* (Raw-хвост + notice диалект-расхождения).
//! STATE-EMIT: coolant-переходы→coolant_29(#29)+grbl_emit spec_cov_emit_coolant,
//!   spindle→grbl_emit spec_cov_emit_spindle, feed-переживает-G0→pilot_03,
//!   partial-end-default-0→unit_partial_end_at_start.
//! EMIT-LEGALITY (8 условий)→grbl_emit spec_cov_emit_legality_*; tool_change→#27.

use eid_core::ast::{Op, Point3, Program, Spindle, StartPoint, Units, Wcs};
use eid_core::diag::ErrorCode;
use eid_grbl::canonicalize::{canonicalize, canonicalize_with_notices};

const EPS: f64 = 0.001;

fn parse(src: &str) -> Result<Program, Vec<Diagnostic>> {
    canonicalize(src, "test.ngc")
}

use eid_core::diag::Diagnostic;

fn err_code(r: Result<Program, Vec<Diagnostic>>) -> ErrorCode {
    let Err(d) = r else {
        panic!("ожидался Err")
    };
    d[0].code
}

fn pt3(p: &Point3) -> [f64; 3] {
    [p.x.value(), p.y.value(), p.z.value()]
}

fn assert_pt(p: &Point3, x: f64, y: f64, z: f64) {
    let v = pt3(p);
    assert!((v[0] - x).abs() <= EPS, "{v:?} vs ({x},{y},{z})");
    assert!((v[1] - y).abs() <= EPS, "{v:?} vs ({x},{y},{z})");
    assert!((v[2] - z).abs() <= EPS, "{v:?} vs ({x},{y},{z})");
}

fn known(op: &Op) -> [f64; 3] {
    match op {
        Op::Rapid {
            start: StartPoint::Known(s),
            ..
        }
        | Op::Linear {
            start: StartPoint::Known(s),
            ..
        }
        | Op::Arc {
            start: StartPoint::Known(s),
            ..
        } => pt3(s),
        other => panic!("start Unknown у {other:?}"),
    }
}

fn feed_of(op: &Op) -> Option<f64> {
    match op {
        Op::Rapid { state, .. } | Op::Linear { state, .. } | Op::Arc { state, .. } => {
            state.feed.as_ref().map(|n| n.value())
        }
        _ => None,
    }
}

fn corpus(rel: &str) -> String {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tier_a/v0");
    let p = base.join(rel);
    if p.is_file() {
        return std::fs::read_to_string(&p).expect("read");
    }
    let mut hit = None;
    for e in std::fs::read_dir(&p).expect(rel) {
        let f = e.expect("entry").path();
        if f.file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with(".grbl.ngc")
        {
            hit = Some(std::fs::read_to_string(&f).expect("read"));
        }
    }
    hit.expect("no .grbl.ngc in dir")
}

// ---------------------------------------------------------------------------
// Пилоты
// ---------------------------------------------------------------------------

#[test]
fn pilot_01_linear_box() {
    let p = parse(&corpus("01_linear_box")).expect("ok");
    // G0 + 4×G1 + ProgramEnd
    assert_eq!(p.ops.len(), 6);
    assert!(matches!(p.ops[0], Op::Rapid { .. }));
    for i in 1..=4 {
        assert!(matches!(p.ops[i], Op::Linear { .. }), "op{i} не Linear");
    }
    assert!(matches!(p.ops[5], Op::ProgramEnd { .. }));
    assert_eq!(p.units, Units::Mm);
    // state-снапшоты: 2-й linear (Y10 F250) несёт F250, coolant flood, шпиндель cw.
    let lin2 = match &p.ops[2] {
        Op::Linear {
            start, end, state, ..
        } => (start, end, state),
        _ => unreachable!(),
    };
    assert!(matches!(lin2.0, StartPoint::Known(_)));
    assert_eq!(feed_of(&p.ops[2]), Some(250.0));
    assert!(lin2.2.coolant.flood);
    assert_eq!(
        lin2.2.spindle,
        Spindle::Cw(eid_core::ast::Num::new("10000".into(), 10000.0))
    );
    assert_eq!(lin2.2.wcs, Wcs::G54);
    assert_eq!(lin2.2.frame, eid_core::ast::Frame::Wcs);
    assert_eq!(lin2.2.tool, None);
    // связывание: start первого linear = end rapid'а (0,0,5)
    assert!(matches!(
        &p.ops[0],
        Op::Rapid {
            start: StartPoint::Unknown,
            ..
        }
    ));
    match &p.ops[0] {
        Op::Rapid { end, .. } => assert_pt(end, 0.0, 0.0, 5.0),
        _ => unreachable!(),
    }
    let _ = known(&p.ops[1]);
    assert_eq!(known(&p.ops[1]), [0.0, 0.0, 5.0]);
}

#[test]
fn pilot_02_rapid_reposition() {
    let p = parse(&corpus("02_rapid_reposition")).expect("ok");
    // 8 ходов + ProgramEnd
    assert_eq!(p.ops.len(), 9);
    let rapids = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Rapid { .. }))
        .count();
    let linears = p
        .ops
        .iter()
        .filter(|o| matches!(o, Op::Linear { .. }))
        .count();
    assert_eq!(rapids, 6);
    assert_eq!(linears, 2);
    assert_eq!(feed_of(&p.ops[2]), Some(80.0)); // G1 X5 F80
}

#[test]
fn pilot_03_feed_survives_rapid() {
    let p = parse(&corpus("03_feed_survives_rapid")).expect("ok");
    assert_eq!(p.ops.len(), 4); // G1, G0, G1, M2
                                // Ключевой вектор: rapid несёт feed=500 (G0 не сбрасывает, SPEC §4).
    assert_eq!(feed_of(&p.ops[1]), Some(500.0));
    assert!(matches!(&p.ops[1], Op::Rapid { .. }));
    assert_eq!(feed_of(&p.ops[2]), Some(500.0));
}

#[test]
fn pilot_36_preset_g54_then_switch() {
    let p = parse(&corpus("36_preset_G54_then_switch")).expect("ok");
    // frame_preset{X0 Y0 Z5, wcs=G54} — узел Program
    let fp = p.frame_preset.as_ref().expect("frame_preset есть");
    assert_pt(&fp.values, 0.0, 0.0, 5.0);
    assert_eq!(fp.wcs, Wcs::G54);
    // op0 G0: start Known(0,0,5) — якорь пресетки; op1 G1 X10: chaining.
    assert_eq!(known(&p.ops[0]), [0.0, 0.0, 5.0]);
    assert_eq!(known(&p.ops[1]), [0.0, 0.0, 0.0]);
    // op2/op4 — смена wcs без in-text origins → точки разрыва (b): Unknown.
    assert!(matches!(
        &p.ops[2],
        Op::Rapid {
            start: StartPoint::Unknown,
            ..
        }
    ));
    assert!(matches!(
        &p.ops[4],
        Op::Rapid {
            start: StartPoint::Unknown,
            ..
        }
    ));
    // op3/op5 — chaining внутри той же WCS.
    assert_eq!(known(&p.ops[3]), [10.0, 0.0, 5.0]);
    assert_eq!(known(&p.ops[5]), [20.0, 0.0, 0.0]);
    // wcs в state: op1 — G54, op3 — G55, op5 — G54.
    let wcs_of = |i: usize| match &p.ops[i] {
        Op::Rapid { state, .. } | Op::Linear { state, .. } => state.wcs,
        _ => unreachable!(),
    };
    assert_eq!(wcs_of(1), Wcs::G54);
    assert_eq!(wcs_of(3), Wcs::G55);
    assert_eq!(wcs_of(5), Wcs::G54);
}

#[test]
fn pilot_46_arc_r_helix() {
    let p = parse(&corpus("46_arc_R_helix")).expect("ok");
    assert_eq!(p.ops.len(), 3); // G0, G2, M2
    match &p.ops[1] {
        Op::Arc {
            start,
            end,
            center,
            normal,
            dir,
            sweep_deg,
            pitch,
            ..
        } => {
            assert_eq!(known(&p.ops[1]), [0.0, 0.0, 0.0]);
            assert!(matches!(start, StartPoint::Known(_)));
            assert_pt(end, -5.0, 5.0, 2.0);
            assert_pt(center, 0.0, 5.0, 0.0); // R-декод (формула grbl/LinuxCNC)
            assert_pt(normal, 0.0, 0.0, 1.0); // G17 → +Z
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
            assert!(
                (sweep_deg.value() - 90.0).abs() <= EPS,
                "sweep {}",
                sweep_deg.value()
            );
            assert!(
                (pitch.value() - 2.0).abs() <= EPS,
                "pitch {}",
                pitch.value()
            );
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn pilot_11_arc_r_semicircle() {
    // SPEC v0.11: golden перегенерирован — центр/sweep ВЫВЕДЕНЫ из 4dp-концов
    // по §6 (single-final-rounding §10.5): center (-0.0351, 0.0001, 0),
    // sweep 179.4975 (sweep R-формы — из хорды и |R|, не от округлённого центра).
    let out = canonicalize_with_notices(&corpus("11_arc_R_semicircle"), "11.ngc").expect("ok");
    let p = out.program;
    assert_eq!(p.ops.len(), 5); // G0, G0, G2, G0, ProgramEnd
    let arc = &p.ops[2];
    match arc {
        Op::Arc {
            center,
            sweep_deg,
            dir,
            pitch,
            normal,
            ..
        } => {
            assert_pt(center, -0.0351, 0.0001, 0.0);
            assert!(
                (sweep_deg.value() - 179.4975).abs() <= 0.001,
                "sweep {}",
                sweep_deg.value()
            );
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
            assert!((pitch.value()).abs() <= EPS);
            assert_pt(normal, 0.0, 0.0, 1.0);
        }
        _ => panic!("op2 не Arc"),
    }
    // Радиус-валидация прошла: |start − center| в плоскости = R = 10.
    let s = known(arc);
    let c = [center_val(arc).0, center_val(arc).1];
    let rho = ((s[0] - c[0]).powi(2) + (s[1] - c[1]).powi(2)).sqrt();
    assert!((rho - 10.0).abs() <= EPS, "ρ_start {rho}");
}

fn center_val(op: &Op) -> (f64, f64) {
    match op {
        Op::Arc { center, .. } => (center.x.value(), center.y.value()),
        _ => unreachable!(),
    }
}

// ---------------------------------------------------------------------------
// Негативы (правило двух классов, CORPUS §1.2 / SPEC §8 v0.10)
// ---------------------------------------------------------------------------

#[test]
fn v020_n2_mid_g92_chained_with_raw_carrier() {
    // SPEC §3 v0.20 (зонды 1–3; коммит c76ed9a): mid-program G92 — валидный
    // frame_preset: ограничение «только startup» снято (бывший Err SEMANTICS
    // N2). AST-носитель — Raw: grammar/AST v0.3 не имеют mid-program
    // frame_preset-опa (eid-core Op заморожен); chaining чтения — в
    // каноническом состоянии. n2-вектор: G0/G1/G92/M2 — Raw-оп после linear.
    let p = parse(&corpus("n2_mid_G92")).expect("n2: Ok (v0.20)");
    assert!(
        p.frame_preset.is_none(),
        "header — только СТАРТОВАЯ пресетка"
    );
    let raw_g92 = p
        .ops
        .iter()
        .find(|o| matches!(o, Op::Raw { .. }))
        .expect("G92-mid несётся Raw-опом (lossless)");
    match raw_g92 {
        Op::Raw { reason, code, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(
                reason.contains("frame_preset"),
                "reason называет конструкцию frame_preset: {reason}"
            );
        }
        _ => unreachable!(),
    }
    // ProgramEnd — последний оп (M2 после G92 не в хвосте).
    assert!(matches!(
        &p.ops.last().expect("non-empty"),
        Op::ProgramEnd { .. }
    ));
}

#[test]
fn negative_n3_config_is_ok_with_raw() {
    let p = parse(&corpus("n3_raw_vendor_block")).expect("n3: Ok + Raw-узел");
    assert!(matches!(
        &p.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    // Остальное — обычная программа.
    assert!(matches!(&p.ops[1], Op::Rapid { .. }));
    assert!(matches!(&p.ops[2], Op::Linear { .. }));
    assert!(matches!(&p.ops[3], Op::ProgramEnd { .. }));
}

#[test]
fn negative_n5_arcs_invalid() {
    assert_eq!(
        err_code(parse(&corpus("n5a_arc_radius_mismatch"))),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse(&corpus("n5b_arc_chord_gt_2R"))),
        ErrorCode::InvalidInput
    );
}

#[test]
fn v021_arc_offsets_missing_true_err_pin() {
    // v0.21 (инвентаризация §5, класс #9): дуга-продолжение БЕЗ offset-слов
    // (ни I/J/K, ни R в блоке) — прежняя подстановка нулевых offsets давала
    // ЛОЖНЫЙ «arc radius mismatch» (ρ_start 0 vs ρ_end). Реальный GRBL =
    // error 2 STATUS_GCODE_ARC_OFFSETS_MISSING (offsets обязательны в
    // КАЖДОМ дуговом блоке — модального «продолжения offset'ов» предыдущей
    // дуги у grbl нет), LinuxCNC: «I/J/K word missing with G2/G3» → оба
    // контроллера отказывают: TRUE-ERR, InvalidInput. Формулировка зеркальна
    // linuxcnc-адаптеру (v021-фикс D) — тест-пин на ТОЧНУЮ подстроку.
    let d = parse("G21 G90 G94 G17\nG0 X0 Y0 Z0\nG2 X5.0 Y5.0 F100.0\nM2\n").unwrap_err();
    assert_eq!(d[0].code, ErrorCode::InvalidInput);
    assert!(
        d[0].message.contains(
            "дуга без offset-слов: в дуговом блоке нет ни I/J/K, ни R — GRBL error 2 (ARC_OFFSETS_MISSING: offsets обязательны в каждом дуговом блоке), LinuxCNC: „I/J/K word missing with G2/G3“"
        ),
        "точная зеркальная формулировка: {}",
        d[0].message
    );
    // Модальное продолжение дуги (G2-кадр без offset-слов после валидной
    // дуги) — тот же TRUE-ERR с тем же текстом.
    let d = parse(
        "G21 G90 G94 G17\nG0 X10.0 Y0.0 Z0.0\nG2 X10.0 Y0.0 I-5.0 J0.0 F100.0\nG2 X0.0 Y0.0 F100.0\nM2\n",
    )
    .unwrap_err();
    assert_eq!(d[0].code, ErrorCode::InvalidInput);
    assert!(d[0].message.contains("ARC_OFFSETS_MISSING"));
    assert!(d[0].message.contains("I/J/K word missing with G2/G3"));
    // Частичная пара (только I; J=0) — семантика §6.11 «отсутствующая ось
    // = 0» СОХРАНЕНА (пин: не расширялся до «все или error»).
    let p = parse("G21 G90 G94 G17\nG0 X10.0 Y0.0 Z0.0\nG2 X10.0 Y0.0 I-5.0 F100.0\nM2\n")
        .expect("частичный I — Ok (§6.11)");
    match &p.ops[1] {
        Op::Arc { sweep_deg, .. } => {
            assert!((sweep_deg.value() - 360.0).abs() <= EPS, "полное кольцо");
        }
        other => panic!("op 1 must be Arc, got {other:?}"),
    }
}

#[test]
fn v021_n7_unit_switch_mid_ok_carrier_chaining() {
    // SPEC §2.5 принцип 5, ревизия v0.21 (инвентаризация Tier C): реальный
    // grbl исполняет G20/G21 в любой позиции (gcode.c case 20/21 —
    // gc_state.units переключается mid-stream, внутренняя позиция в mm,
    // конверсия чтений внутренняя) — прежний Err N7 «одна система на
    // программу» снят (то же недоразумение, что G92-mid/bare-T — директива
    // ревью). Смена системы чтения = line-level Raw-носитель (опа units в
    // AST v0.2 нет, байты lossless same-dialect) + chaining-конверсия 25.4
    // (start следующего хода = прежнее физическое положение в НОВОЙ системе).
    // Вектор: G21-заголовок / G0 (0,0,5) mm / G20 / G0 (10,10,5) inch / M2
    // → [Rapid(unknown), Raw-носитель G20, Rapid(inch), PEND].
    let out = canonicalize_with_notices(&corpus("n7_unit_switch_mid"), "n7.ngc")
        .expect("Ok (v0.21: units-switch легален)");
    let p = &out.program;
    assert_eq!(
        p.ops.len(),
        4,
        "[Rapid, Raw-носитель G20, Rapid(inch), PEND]: {:?}",
        p.ops
    );
    assert_eq!(p.units, Units::Mm, "первое чистое вхождение = G21");
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    match &p.ops[1] {
        Op::Raw { code, reason, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(
                reason.contains("units switch mid-program"),
                "маркер chaining в reason: {reason}"
            );
        }
        other => panic!("op 1 must be Raw-носитель G20, got {other:?}"),
    }
    // Inch-ход: старт = (0,0,5)mm = (0,0,0.1969)in — конверсия 25.4 (4dp).
    match &p.ops[2] {
        Op::Rapid {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_pt(s, 0.0, 0.0, 0.1969);
            assert_pt(end, 10.0, 10.0, 5.0);
            // 4dp-квантование конверсии видно в тексте Num (SPEC §10.2).
            assert_eq!(s.z.text, "0.1969", "4dp-конверсия 5/25.4");
        }
        other => panic!("op 2 must be inch Rapid, got {other:?}"),
    }
    assert!(matches!(&p.ops[3], Op::ProgramEnd { .. }));
    // Ok-путь без notices (носитель — Op::Raw, не notice).
    assert!(out.notices.is_empty(), "{:?}", out.notices);
}

#[test]
fn v021_units_switch_task_pattern_chain_inch() {
    // N7-паттерн брифа: G21 / G0 X0 Y0 Z5 / G20 / G1 X1. F100 → chaining в
    // inch: start следующего хода = прежнее физическое положение в НОВОЙ
    // системе — (0,0,5)mm = (0,0,0.1969)in; неуказанные оси end наследуют
    // чтение в inch (Y=0, Z=0.1969).
    let p = parse("G21\nG0 X0 Y0 Z5\nG20\nG1 X1. F100.0\nM2\n").expect("Ok");
    assert_eq!(p.ops.len(), 4, "[Rapid, Raw G20, Linear(inch), PEND]");
    match &p.ops[2] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            state,
            ..
        } => {
            assert_pt(s, 0.0, 0.0, 0.1969);
            assert_pt(end, 1.0, 0.0, 0.1969);
            assert_eq!(state.feed.as_ref().map(|n| n.value()), Some(100.0));
        }
        other => panic!("op 2 must be inch Linear, got {other:?}"),
    }
    assert_eq!(p.units, Units::Mm, "заголовок = первое чистое G21");
}

#[test]
fn v021_units_switch_reverse_and_physical_roundtrip() {
    // Обратный switch (inch→mm) + round-trip mm→inch→mm: физическая цепочка
    // сохраняется В ПРЕДЕЛАХ ДОПУСКА (SPEC §2.5 v0.21: конверсия «в пределах
    // допуска», не «точна» — 4dp-дрейф ≤2e-5 мм < ε_pos). Физическая
    // последовательность 10 → 12.7 → 10 мм (10mm = 0.3937"; 0.5" = 12.7mm).
    let p = parse(concat!(
        "G21 G90 G94 G17\n",
        "G0 X10\n",        // mm: (0,0,0) → (10,0,0)
        "G20\n",           // носитель; чтение 10mm = 0.3937"
        "G1 X0.5 F10.0\n", // inch: start (0.3937,0,0), end (0.5,0,0)
        "G21\n",           // носитель; чтение 0.5" = 12.7mm
        "G1 X10 F10.0\n",  // mm: start (12.7,0,0), end (10,0,0)
        "M2\n",
    ))
    .expect("Ok");
    // [Rapid, Raw, Linear, Raw, Linear, PEND]
    assert_eq!(p.ops.len(), 6, "{:?}", p.ops);
    match &p.ops[2] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            // 10mm / 25.4 = 0.39370078… → 4dp 0.3937 (дрейф 2e-5 мм < ε_pos)
            assert_eq!(
                (s.x.text.as_str(), s.y.text.as_str(), s.z.text.as_str()),
                ("0.3937", "0", "0")
            );
            assert_pt(s, 0.3937, 0.0, 0.0);
            // end ХРАНИТСЯ в текущей системе чтения: последующий G21-switch
            // конвертирует якорь (convert_units_anchor, бит-пара
            // start_{i+1} == end_i core-verify) — 0.5" → 12.7mm, та же
            // физическая точка.
            assert_pt(end, 12.7, 0.0, 0.0);
        }
        other => panic!("op 2 must be inch Linear, got {other:?}"),
    }
    match &p.ops[4] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            // 0.5" × 25.4 = 12.7mm — обратный switch точен (×25.4 без дрейфа)
            assert_pt(s, 12.7, 0.0, 0.0);
            assert_pt(end, 10.0, 0.0, 0.0);
        }
        other => panic!("op 4 must be mm Linear, got {other:?}"),
    }
    assert_eq!(p.units, Units::Mm);
}

#[test]
fn v021_units_switch_motion_on_switch_line_carried() {
    // Ход на switch-строке: grbl исполняет ВЕСЬ блок (units [2], motion [10])
    // — цепочка чтений продолжается в НОВОЙ системе; оп хода НЕ строится
    // (байты строки в носителе, носитель первым). Зеркало linuxcnc-теста
    // v021_units_switch_mid_motion_on_switch_line. Вектор брифа:
    // G0 X10 mm / G20 G1 X0.5 / G21 G1 X10 / G0 X20 → физическая
    // последовательность 10 → 12.7 → 10 → 20 мм; конечный ход цепляется
    // от 10 (последний ход switch-строки).
    let p = parse("G21 G90 G94 G17\nG0 X10\nG20 G1 X0.5 F10.0\nG21 G1 X10 F10.0\nG0 X20\nM2\n")
        .expect("Ok");
    assert_eq!(p.ops.len(), 5, "{:?}", p.ops);
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    for i in [1usize, 2] {
        match &p.ops[i] {
            Op::Raw { code, reason, .. } => {
                assert_eq!(*code, ErrorCode::UnsupportedSemantics);
                assert!(
                    reason.contains("units switch mid-program"),
                    "op {i}: маркер в reason: {reason}"
                );
            }
            other => panic!("op {i} must be Raw-носитель, got {other:?}"),
        }
    }
    match &p.ops[3] {
        Op::Rapid {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_pt(s, 10.0, 0.0, 0.0);
            assert_pt(end, 20.0, 0.0, 0.0);
        }
        other => panic!("op 3 must be mm Rapid, got {other:?}"),
    }
    assert!(matches!(&p.ops[4], Op::ProgramEnd { .. }));
}

#[test]
fn v021_units_switch_repeated_and_default_mm_header() {
    // Повтор той же системы — легальный no-op БЕЗ носителя (зеркало
    // linuxcnc «st.units_mode == u → continue»); ПЕРВОЕ вхождение после
    // mm-дефолтных ходов — смена чтения, заголовок Program.units остаётся
    // дефолт Mm (linuxcnc-зеркало v021 «заголовок честен»).
    let p = parse("G0 X25.4\nG20\nG1 X0.5 F10.0\nG20\nG1 X1.0 F10.0\nM2\n").expect("Ok");
    assert_eq!(
        p.units,
        Units::Mm,
        "25.4mm-ход до первого G20 — заголовок Mm"
    );
    // [Rapid, Raw(G20), Linear, Linear, PEND] — повтор G20 носителя не строит
    assert_eq!(p.ops.len(), 5, "{:?}", p.ops);
    match &p.ops[1] {
        Op::Raw { reason, .. } => assert!(reason.contains("units switch mid-program")),
        other => panic!("op 1 must be Raw-носитель, got {other:?}"),
    }
    // Чтение после no-op-повтора: 0.5" — без конверсии (та же система).
    match &p.ops[3] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_pt(s, 0.5, 0.0, 0.0);
            assert_pt(end, 1.0, 0.0, 0.0);
        }
        other => panic!("op 3 must be inch Linear, got {other:?}"),
    }
}

#[test]
fn v021_units_switch_parse_emit_roundtrip() {
    // Round-trip same-dialect: носители G20/G21 в потоке (иначе семантика
    // последующих слов потеряна) — emit → reparse: то же число опов и
    // физическая цепочка (дрейф в пределах ε_pos).
    let p1 = parse(concat!(
        "G21 G90 G94 G17\n",
        "G0 X10\n",
        "G20\n",
        "G1 X0.5 F10.0\n",
        "G21\n",
        "G1 X10 F10.0\n",
        "M2\n",
    ))
    .expect("parse 1");
    let text = eid_grbl::emit::emit(&p1).expect("emit");
    assert!(
        text.lines().any(|l| l.trim() == "G20"),
        "G20-носитель в эмиттенном потоке:\n{text}"
    );
    assert!(
        text.lines().any(|l| l.trim() == "G21"),
        "G21-носитель после заголовка:\n{text}"
    );
    let p2 = parse(&text).expect("reparse");
    assert_eq!(p1.ops.len(), p2.ops.len());
    assert_eq!(p1.units, p2.units);
    for (a, b) in p1.ops.iter().zip(&p2.ops) {
        match (a, b) {
            (
                Op::Rapid {
                    start: sa, end: ea, ..
                }
                | Op::Linear {
                    start: sa, end: ea, ..
                },
                Op::Rapid {
                    start: sb, end: eb, ..
                }
                | Op::Linear {
                    start: sb, end: eb, ..
                },
            ) => {
                assert_eq!(std::mem::discriminant(sa), std::mem::discriminant(sb));
                if let (StartPoint::Known(pa), StartPoint::Known(pb)) = (sa, sb) {
                    assert_pt(pa, pb.x.value(), pb.y.value(), pb.z.value());
                }
                assert_pt(ea, eb.x.value(), eb.y.value(), eb.z.value());
            }
            (Op::Raw { .. }, Op::Raw { .. }) => {}
            (Op::ProgramEnd { .. }, Op::ProgramEnd { .. }) => {}
            _ => panic!("вид опа разошёлся при reparse: {a:?} vs {b:?}"),
        }
    }
}

#[test]
fn v021_units_switch_arc_anchor_roundtrip() {
    // Дуга ПЕРЕД switch (каскад якоря): convert_units_anchor скейлит
    // end+center+start дуги (та же геометрия §6, равномерный скейл) и end
    // предыдущего rapid (бит-цепочка start дуги == end rapid); emit-inverse
    // (geo-факторы) восстанавливает литералы — R1 round-trip + связность.
    let p1 = parse(concat!(
        "G21 G90 G94 G17\n",
        "G0 X10.0 Y0.0 Z0.0\n",
        "G2 X4.0 Y0.0 I-3.0 J0.0 F100.0\n",
        "G20\n",
        "G1 X0.5 F10.0\n",
        "M2\n",
    ))
    .expect("parse 1");
    // [Rapid, Arc, Raw(G20), Linear(inch), PEND]
    assert_eq!(p1.ops.len(), 5, "{:?}", p1.ops);
    // Якорь-согласование: end дуги и end rapid — в inch (та же физическая
    // точка): 4mm = 0.1575" (4dp), 10mm = 0.3937".
    match (&p1.ops[0], &p1.ops[1]) {
        (
            Op::Rapid { end: r_end, .. },
            Op::Arc {
                start: StartPoint::Known(s),
                center: c,
                end: a_end,
                ..
            },
        ) => {
            assert_pt(r_end, 0.3937, 0.0, 0.0);
            assert_pt(s, 0.3937, 0.0, 0.0);
            assert_pt(c, 0.2756, 0.0, 0.0); // 7mm = 0.2756"
            assert_pt(a_end, 0.1575, 0.0, 0.0);
        }
        other => panic!("ожидались [Rapid, Arc], got {other:?}"),
    }
    let text = eid_grbl::emit::emit(&p1).expect("emit");
    // Литералы восстановлены: 10/4/I-3 — до-конверсные значения.
    assert!(
        text.contains("G0 X10.0000 Y0.0000 Z0.0000"),
        "rapid-литерал восстановлен:\n{text}"
    );
    assert!(
        text.contains("G2 X4.0000 Y0.0000 Z0.0000 I-3.0000 J0.0000"),
        "дуговые литералы восстановлены:\n{text}"
    );
    assert!(text.lines().any(|l| l.trim() == "G20"), "носитель:\n{text}");
    let p2 = parse(&text).expect("reparse");
    assert_eq!(p1.ops.len(), p2.ops.len());
    for (a, b) in p1.ops.iter().zip(&p2.ops) {
        match (a, b) {
            (
                Op::Rapid {
                    start: sa, end: ea, ..
                }
                | Op::Linear {
                    start: sa, end: ea, ..
                },
                Op::Rapid {
                    start: sb, end: eb, ..
                }
                | Op::Linear {
                    start: sb, end: eb, ..
                },
            ) => {
                assert_eq!(std::mem::discriminant(sa), std::mem::discriminant(sb));
                if let (StartPoint::Known(pa), StartPoint::Known(pb)) = (sa, sb) {
                    assert_pt(pa, pb.x.value(), pb.y.value(), pb.z.value());
                }
                assert_pt(ea, eb.x.value(), eb.y.value(), eb.z.value());
            }
            (
                Op::Arc {
                    start: sa,
                    center: ca,
                    end: ea,
                    sweep_deg: swa,
                    ..
                },
                Op::Arc {
                    start: sb,
                    center: cb,
                    end: eb,
                    sweep_deg: swb,
                    ..
                },
            ) => {
                if let (StartPoint::Known(pa), StartPoint::Known(pb)) = (sa, sb) {
                    assert_pt(pa, pb.x.value(), pb.y.value(), pb.z.value());
                }
                assert_pt(ca, cb.x.value(), cb.y.value(), cb.z.value());
                assert_pt(ea, eb.x.value(), eb.y.value(), eb.z.value());
                assert!((swa.value() - swb.value()).abs() <= EPS);
            }
            (Op::Raw { .. }, Op::Raw { .. }) => {}
            (Op::ProgramEnd { .. }, Op::ProgramEnd { .. }) => {}
            _ => panic!("вид опа разошёлся при reparse: {a:?} vs {b:?}"),
        }
    }
}

#[test]
fn v020_n12_13_tail_after_program_end_raw() {
    // SPEC §5.2 v0.20 (source protocol.c:74-105 — главный цикл БЕЗ проверки
    // program_flow; зонд 5): grbl-стрим ИСПОЛНЯЕТ блоки после M2/M30 —
    // бывшие Err «not the last» (N12) и «duplicate» (N13) сняты; каждый
    // хвостовой блок — lossless Raw-оп, program_end остаётся опом в
    // последовательности (не forced-last); notice диалект-расхождения.
    for rel in ["n12_program_end_not_last", "n13_program_end_duplicate"] {
        let out = canonicalize_with_notices(&corpus(rel), "n12_13.ngc").expect("{rel}: Ok (v0.20)");
        let mut pend_seen = false;
        let mut raw_tail = 0;
        for op in &out.program.ops {
            match op {
                Op::ProgramEnd { .. } => pend_seen = true,
                Op::Raw { .. } if pend_seen => raw_tail += 1,
                _ => {}
            }
        }
        assert!(pend_seen, "{rel}: program_end — оп в последовательности");
        assert!(raw_tail >= 1, "{rel}: хвост после PEND — Raw-опы");
        assert!(
            out.notices.iter().any(|n| {
                n.message.contains("executed in grbl-stream semantics")
                    && n.message.contains("ignored in rs274-file semantics")
            }),
            "{rel}: notice диалект-расхождения: {:?}",
            out.notices.iter().map(|n| &n.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn spec_v016_auto_pend_missing_program_end() {
    // SPEC §3 v0.16 (18/32 Tier B): программа без M2/M30 ВАЛИДА — неявный
    // конец: ProgramEnd добавлен последним опом + notice W_MISSING_PROGRAM_END.
    let out = canonicalize_with_notices(
        &corpus("n14_program_end_missing/program_end_missing.grbl.ngc"),
        "n14.ngc",
    )
    .expect("n14: Ok (auto-PEND, не отказ)");
    // ops: RAPID, LINEAR, ProgramEnd(авто) — последний оп именно ProgramEnd.
    assert!(matches!(&out.program.ops[0], Op::Rapid { .. }));
    assert!(matches!(&out.program.ops[1], Op::Linear { .. }));
    assert!(matches!(
        &out.program.ops.last().expect("non-empty"),
        Op::ProgramEnd { .. }
    ));
    assert_eq!(out.notices.len(), 1);
    assert_eq!(
        out.notices[0].kind,
        eid_grbl::canonicalize::NoticeKind::MissingProgramEnd
    );
    assert!(out.notices[0].message.contains("W_MISSING_PROGRAM_END"));
    // v0.20: дубль M2 больше не Err — второй M2 = Raw-хвост после PEND
    // (grbl-стрим исполняет: protocol.c:74-105; см. v020_n12_13_*).
}

#[test]
fn negative_n15_n16_n17_frames() {
    assert_eq!(
        err_code(parse(&corpus("n15_wcs_set_here_mid"))),
        ErrorCode::UnsupportedSemantics
    );
    assert_eq!(
        err_code(parse(&corpus("n16_g53_after_preset"))),
        ErrorCode::RuntimeState
    );
    // N17 (SPEC §3 v0.20, Q5 ЗАКРЫТ): несколько G92 легальны — до ходов
    // per-axis merge в СТАРТОВУЮ пресетку header-поля (зонд 2: replace-по-
    // пересчёту; зонд 3: неуказанные оси чтение не меняют → merge от
    // (0,0,5): X=10 задан, Y=0 Z=5 сохранены).
    let p = parse(&corpus("n17_multiple_preset_at_start")).expect("n17: Ok (v0.20)");
    let fp = p.frame_preset.as_ref().expect("стартовая пресетка");
    assert_pt(&fp.values, 10.0, 0.0, 5.0);
}

#[test]
fn negative_n19_r_full_circle_invalid() {
    assert_eq!(
        err_code(parse(&corpus("n19_arc_R_full_circle"))),
        ErrorCode::InvalidInput
    );
}

// ---------------------------------------------------------------------------
// SPEC v0.12: дефолт path_mode, установление единиц, квантование входа
// ---------------------------------------------------------------------------

#[test]
fn spec_v012_path_mode_default_exact_path() {
    // SPEC §3 v0.12: отсутствие G61/G61.1/G64 → exact_path.
    let p = parse(&corpus("01_linear_box/linear_box.grbl.ngc")).expect("ok");
    assert_eq!(p.path_mode, eid_core::ast::PathMode::ExactPath);
}

#[test]
fn spec_v012_first_units_word_establishes() {
    // CORPUS #10: G20 первым словом — легальный inch-файл (не смена единиц).
    let p = parse(&corpus("10_arc_helix_inch/arc_helix_inch.grbl.ngc")).expect("ok");
    assert_eq!(p.units, Units::In);
    match &p.ops[1] {
        Op::Arc {
            pitch, sweep_deg, ..
        } => {
            assert!(
                (pitch.value() - 0.1).abs() <= EPS,
                "pitch {}",
                pitch.value()
            );
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
    // n7 v0.21 (ревизия §2.5 принцип 5): switch mid-program легален — Ok,
    // Raw-носитель G20 + chaining-конверсия 25.4 (детально —
    // v021_n7_unit_switch_mid_ok_carrier_chaining).
    parse(&corpus("n7_unit_switch_mid")).expect("n7: Ok (v0.21)");
}

#[test]
fn spec_v0106_quantized_end_with_center_full_circle_360() {
    // CORPUS #13 (SPEC §3/§6 v0.17, аудит #15 — зонд rs274 ARC_FEED
    // rotation=-1): end X0.00004 квантуется к старту (§10.6), но
    // center ≠ start (I5 J0) → ПОЛНЫЙ КРУГ 360° НЕЗАВИСИМО от явных
    // end-слов; дуга сохранена, notice снят (CORPUS v0.6.7 golden regen).
    let out = canonicalize_with_notices(
        &corpus("13_arc_degenerate_zero_sweep/arc_degenerate_zero_sweep.grbl.ngc"),
        "13.ngc",
    )
    .expect("ok");
    assert_eq!(
        out.notices.len(),
        0,
        "диспозиция drop снята: 360°-дуга жива"
    );
    // G0, ARC-360, G1 X10, M2 → 4 опа (golden).
    assert_eq!(out.program.ops.len(), 4);
    match &out.program.ops[1] {
        Op::Arc {
            center,
            sweep_deg,
            pitch,
            dir,
            ..
        } => {
            assert_pt(center, 5.0, 0.0, 0.0);
            assert!(
                (sweep_deg.value() - 360.0).abs() <= EPS,
                "sweep {}",
                sweep_deg.value()
            );
            assert!(pitch.value().abs() <= EPS);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn corpus_47_dollar_dollar_line_is_raw() {
    // CORPUS #47: голое `$$` (dump settings) → ConfigLine → Raw{SYNTAX}.
    let out = canonicalize_with_notices(
        &corpus("47_raw_crlf_fidelity/raw_crlf_fidelity.grbl.ngc"),
        "47.ngc",
    )
    .expect("ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    // Полная CRLF-программа продолжает разбираться: Rapid/Linear/…/M2.
    assert!(matches!(&out.program.ops[1], Op::Rapid { .. }));
}

#[test]
fn corpus_08_full_circle_without_end_words_kept() {
    // CORPUS #08: «G2 I-5.0 J0.0» без end-слов — настоящее полное кольцо 360°.
    let out = canonicalize_with_notices(
        &corpus("08_arc_full_circle/arc_full_circle.grbl.ngc"),
        "08.ngc",
    )
    .expect("ok");
    assert_eq!(out.notices.len(), 0);
    match &out.program.ops[1] {
        Op::Arc { sweep_deg, .. } => {
            assert!(
                (sweep_deg.value() - 360.0).abs() <= EPS,
                "sweep {}",
                sweep_deg.value()
            );
        }
        _ => panic!("op1 не Arc"),
    }
}

// ===========================================================================
// SPEC §3 v0.17 (аудит #15): полный круг / вырожденный центр
// ===========================================================================

#[test]
fn audit15_full_circle_explicit_end_quantized_cw_ccw_and_helix() {
    // Зонд rs274: `G2 X10. Y0. I-5. J0` из (10,0) → полный оборот 360°
    // НЕЗАВИСИМО от явных end-слов (квантовались в старт); G3 — тоже 360°.
    let p = parse("G21 G90 G94 G17\nG0 X10.0 Y0.0 Z0.0\nG2 X10.0 Y0.0 I-5.0 J0.0 F100.0\nM2\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            sweep_deg,
            pitch,
            dir,
            ..
        } => {
            assert_pt(center, 5.0, 0.0, 0.0);
            assert!(
                (sweep_deg.value() - 360.0).abs() <= EPS,
                "cw sweep {}",
                sweep_deg.value()
            );
            assert!(pitch.value().abs() <= EPS);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
        }
        _ => panic!("op1 не Arc"),
    }
    let p = parse("G21 G90 G94 G17\nG0 X10.0 Y0.0 Z0.0\nG3 X10.0 Y0.0 I-5.0 J0.0 F100.0\nM2\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc { sweep_deg, dir, .. } => {
            assert!(
                (sweep_deg.value() - 360.0).abs() <= EPS,
                "ccw sweep {}",
                sweep_deg.value()
            );
            assert_eq!(*dir, eid_core::ast::Dir::Ccw);
        }
        _ => panic!("op1 не Arc"),
    }
    // Частично-явный end: одно Y-слово, квантовавшееся в старт, — тоже 360°
    // (критерий геометрический: center ≠ start, а не «есть ли end-слова»).
    let p =
        parse("G21 G90 G94 G17\nG0 X10.0 Y0.0 Z0.0\nG2 Y0.0 I-5.0 J0.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc { sweep_deg, .. } => {
            assert!(
                (sweep_deg.value() - 360.0).abs() <= EPS,
                "partial-end sweep {}",
                sweep_deg.value()
            );
        }
        _ => panic!("op1 не Arc"),
    }
    // Винтовой полный оборот с явными end-словами: 360° + pitch (Z2) —
    // не drop (pitch ≠ 0), ход сохранён.
    let p =
        parse("G21 G90 G94 G17\nG0 X10.0 Y0.0 Z0.0\nG2 X10.0 Y0.0 Z2.0 I-5.0 J0.0 F100.0\nM2\n")
            .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            sweep_deg, pitch, ..
        } => {
            assert!((sweep_deg.value() - 360.0).abs() <= EPS);
            assert!((pitch.value() - 2.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn audit15_degenerate_center_zero_radius_dropped_both_forms() {
    // Вырожденный центр (I0 J0 → center == start) при end == start (в
    // плоскости) и pitch == 0 → drop+notice ОБЕ формы end-слов (зонд
    // rs274: «Zero-radius arc» — отказ на обеих формах, у нас drop+notice).
    // Форма 1: явные end-слова, совпавшие со стартом.
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG0 X5.0 Y5.0 Z0.0\nG2 X5.0 Y5.0 I0.0 J0.0 F100.0\nM2\n",
        "dg1.ngc",
    )
    .expect("ok");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(
        out.notices[0].kind,
        eid_grbl::canonicalize::NoticeKind::ZeroLengthArc
    );
    assert!(
        out.notices[0].message.contains("zero-radius arc"),
        "{}",
        out.notices[0].message
    );
    // Форма 2: наследованный end (нет end-слов) — тоже drop (обе формы).
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG0 X5.0 Y5.0 Z0.0\nG2 I0.0 J0.0 F100.0\nM2\n",
        "dg2.ngc",
    )
    .expect("ok");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(
        out.notices[0].kind,
        eid_grbl::canonicalize::NoticeKind::ZeroLengthArc
    );
    // Вырожденный + pitch ≠ 0 → 360-винтовая r=0 — остаётся (D/fanuc).
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG0 X5.0 Y5.0 Z0.0\nG2 X5.0 Y5.0 Z2.0 I0.0 J0.0 F100.0\nM2\n",
        "dg3.ngc",
    )
    .expect("ok");
    assert_eq!(out.notices.len(), 0);
    match &out.program.ops[1] {
        Op::Arc {
            center,
            sweep_deg,
            pitch,
            ..
        } => {
            assert_pt(center, 5.0, 5.0, 0.0);
            assert!((sweep_deg.value() - 360.0).abs() <= EPS);
            assert!((pitch.value() - 2.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
    // R-форма при start == end — по-прежнему Err (центр неоднозначен, N19).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17\nG0 X5.0 Y5.0 Z0.0\nG2 X5.0 Y5.0 R5.0 F100.0\nM2\n"
        )),
        ErrorCode::InvalidInput
    );
}

// ===========================================================================
// Аудит #33 (SPEC §8.6 правило 6, v0.17): модальная абсорбция на RAW-строках
// ===========================================================================

#[test]
fn audit33_g91_absorbed_on_raw_line_incremental_continues() {
    // Пробой: `G91 G28 Z0` — G28 ушёл в Raw, G91 терялся → следующий
    // `G0 X10 Y10` молча парсился АБСОЛЮТНЫМ. После фикса G91 абсорбирован:
    // end = стартовая точка + (10,10) инкрементально.
    let src = "G21 G90 G94 G17\nG0 X5.0 Y5.0 Z2.0\nG91 G28 Z0\nG0 X10.0 Y10.0\nM2\n";
    let out = canonicalize_with_notices(src, "a33.ngc").expect("parses Ok (G28 — line-level raw)");
    assert!(
        out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })),
        "строка G28 — RAW-оп"
    );
    assert!(
        out.notices.iter().any(|n| matches!(
            n.kind,
            eid_grbl::canonicalize::NoticeKind::ModalOnRaw
        ) && n.message.contains("W_MODAL_ON_RAW")),
        "ожидался W_MODAL_ON_RAW-notice, получено: {:?}",
        out.notices.iter().map(|n| &n.message).collect::<Vec<_>>()
    );
    let ends: Vec<[f64; 3]> = out
        .program
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Rapid { end, .. } => Some(pt3(end)),
            _ => None,
        })
        .collect();
    assert_eq!(ends.len(), 2);
    for (i, want) in [5.0, 5.0, 2.0].into_iter().enumerate() {
        assert!(
            (ends[0][i] - want).abs() <= EPS,
            "end0[{}] = {} vs {want}",
            i,
            ends[0][i]
        );
    }
    assert_eq!(
        ends[1],
        [15.0, 15.0, 2.0],
        "G91 абсорбирован: end = (5,5,2) + (10,10) инкрементально (было (10,10,0) — тихое искажение)"
    );
}

#[test]
fn audit33_units_absorbed_on_raw_line_g20_g43() {
    // `G20 G43 H2` — G43/H вне словаря GRBL → RAW, но G20 абсорбирован:
    // units = inch, следующая строка читается в inch (до фикса units терялись).
    // (Прежний источник нес `X5.` на raw-строке: с v0.21 chain-break через
    // Raw-носитель с осевыми словами такой ход делает позицию неизвестной и
    // частичный end `G1 X1.0` честно даёт E_RUNTIME_STATE — см.
    // v021_partial_end_after_raw_axis_block_runtime; audit-вектор целиком —
    // units-абсорбция, осевое слово было декорацией и убрано.)
    let out = canonicalize_with_notices("G20 G43 H2\nG1 X1.0 F10.0\nM2\n", "a33u.ngc")
        .expect("parses Ok");
    assert_eq!(
        out.program.units,
        Units::In,
        "G20 абсорбирован с RAW-строки"
    );
    assert!(out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
}

#[test]
fn audit33_feed_spindle_absorbed_on_raw_line() {
    // F/S/M3 на RAW-строке (`G43 H2 F240.0 M3 S6000`) абсорбируются:
    // следующий linear несёт feed=240 и шпиндель Cw(6000) без своих слов.
    let src = "G21 G90 G94 G17\nG43 H2 F240.0 M3 S6000\nG1 X10.0\nM2\n";
    let out = canonicalize_with_notices(src, "a33fs.ngc").expect("parses Ok");
    assert!(out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
    match out
        .program
        .ops
        .iter()
        .find(|o| matches!(o, Op::Linear { .. }))
    {
        Some(Op::Linear { state, .. }) => {
            assert_eq!(
                state.feed.as_ref().map(|f| f.value()),
                Some(240.0),
                "F240 абсорбирован"
            );
            assert!(
                matches!(&state.spindle, Spindle::Cw(n) if n.value() == 6000.0),
                "M3 S6000 абсорбирован: {state:?}"
            );
        }
        other => panic!("ожидался Linear: {other:?}"),
    }
}

#[test]
fn audit33_w_modal_on_raw_notice_on_raw_motion_g() {
    // Raw-motion-G (`G28 X5.0`) — notice W_MODAL_ON_RAW; модальный motion
    // (G1) НЕ продолжается: следующий голый X-кадр → INVALID (нет активного
    // motion-режима), не молчаливое продолжение G1.
    let src = "G21 G90 G94 G17\nG92 X0 Y0 Z0\nG1 X5.0 F100.0\nG28 X5.0\nX8.0\nM2\n";
    assert_eq!(
        err_code(canonicalize(src, "a33w.ngc")),
        ErrorCode::InvalidInput
    );
    // Без голого кадра — Ok + notice:
    let src2 = "G21 G90 G94 G17\nG92 X0 Y0 Z0\nG1 X5.0 F100.0\nG28 X5.0\nM2\n";
    let out2 = canonicalize_with_notices(src2, "a33w2.ngc").expect("Ok");
    assert!(
        out2.notices
            .iter()
            .any(|n| matches!(n.kind, eid_grbl::canonicalize::NoticeKind::ModalOnRaw)),
        "{:?}",
        out2.notices.iter().map(|n| &n.message).collect::<Vec<_>>()
    );
    // G28 без осевых слов — notice НЕ эмитится (нет motion-неоднозначности).
    let src3 = "G21 G90 G94 G17\nG92 X0 Y0 Z0\nG28\nM2\n";
    let out3 = canonicalize_with_notices(src3, "a33w3.ngc").expect("Ok");
    assert!(
        out3.notices
            .iter()
            .all(|n| !matches!(n.kind, eid_grbl::canonicalize::NoticeKind::ModalOnRaw)),
        "{:?}",
        out3.notices.iter().map(|n| &n.message).collect::<Vec<_>>()
    );
}

#[test]
fn audit33_g92_preset_absorbed_on_raw_line() {
    // G92 на RAW-строке абсорбируется: frame_preset задан, следующий ход
    // связывается с якоря пресетки (start known).
    let src = "G21 G90 G94 G17\nG92 X0.0 Y0.0 Z5.0 G28\nG0 X1.0 Y1.0 Z6.0\nM2\n";
    let out = canonicalize_with_notices(src, "a33g92.ngc").expect("parses Ok");
    let fp = out
        .program
        .frame_preset
        .as_ref()
        .expect("G92 абсорбирован: пресетка задана");
    assert_pt(&fp.values, 0.0, 0.0, 5.0);
    assert_eq!(fp.wcs, Wcs::G54);
    assert!(out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })));
    match out
        .program
        .ops
        .iter()
        .find(|o| matches!(o, Op::Rapid { .. }))
        .expect("rapid")
    {
        Op::Rapid {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_pt(s, 0.0, 0.0, 5.0);
            assert_pt(end, 1.0, 1.0, 6.0);
        }
        other => panic!("{other:?}"),
    }
    // v0.20 (Q5 закрыт): вторая G92 в стартовой зоне на raw-кадре — per-axis
    // merge пресетки (зонд 3: заданные оси заменяются, остальные хранятся),
    // не Err. Смешанная L20+G92 остаётся Err (см. spec_cov_frame_l20_*).
    let dup = "G21 G90 G94 G17\nG92 X0.0 Y0.0 Z5.0 G28\nG92 X1.0 Y1.0 Z1.0 G28\nM2\n";
    let dup_out = canonicalize_with_notices(dup, "a33g92b.ngc").expect("n17-класс Ok (v0.20)");
    let fp = dup_out
        .program
        .frame_preset
        .as_ref()
        .expect("merge двух стартовых G92 на raw-кадре");
    assert_pt(&fp.values, 1.0, 1.0, 1.0);
}

#[test]
fn audit33_g10_l2_absorbed_on_raw_line_origin_table_updated() {
    // G10 L2 на RAW-строке: состояние origin абсорбировано (оп НЕ строится —
    // кадр представлен Raw-опом), смена wcs использует абсорбированный
    // origin при связывании следующего хода.
    let src = "G21 G90 G94 G17\nG10 L2 P1 X0.0 Y0.0 Z0.0\nG0 X5.0 Y0.0 Z0.0\nG10 L2 P2 X10.0 Y0.0 Z0.0 G28\nG55\nG0 X5.0 Y0.0 Z0.0\nM2\n";
    let out = canonicalize_with_notices(src, "a33g10.ngc").expect("parses Ok");
    assert!(
        out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })),
        "строка G28 — RAW-оп"
    );
    assert!(
        !out.program
            .ops
            .iter()
            .any(|o| matches!(o, Op::WcsDefine { p: 2, .. })),
        "raw-кадр не эмитит WcsDefine(p=2) (p=2 ушёл в состояние)"
    );
    // Легальная строка P1 — по-прежнему WcsDefine-оп.
    assert!(
        out.program
            .ops
            .iter()
            .any(|o| matches!(o, Op::WcsDefine { p: 1, .. })),
        "легальная G10 L2 P1 строка — WcsDefine-оп"
    );
    // Связывание: start = prev_end(5) + O_P1(0) − O_P2(10) = −5 —
    // origin P2 ДОСТУПЕН (до фикса G10 с raw-строки терялся → разрыв (b)).
    match out
        .program
        .ops
        .iter()
        .find(|o| {
            matches!(
                o,
                Op::Rapid {
                    start: StartPoint::Known(_),
                    ..
                }
            )
        })
        .expect("Known-rapid")
    {
        Op::Rapid {
            start: StartPoint::Known(s),
            ..
        } => assert_pt(s, -5.0, 0.0, 0.0),
        other => panic!("{other:?}"),
    }
}

// ===========================================================================
// (перенесено) CORPUS #37
// ===========================================================================

#[test]
fn corpus_37_g53_known_origins_breakpoint_c() {
    // CORPUS #37 (SPEC §2(c) v0.10): первый machine-ход при выполненных §5.3 —
    // точка разрыва: RAPID start=unknown (не отказ); последующие цепляются.
    let out = canonicalize_with_notices(
        &corpus("37_g53_known_origins/g53_known_origins.grbl.ngc"),
        "37.ngc",
    )
    .expect("parse Ok");
    let ops = &out.program.ops;
    // ops: [WcsDefine, Rapid(G53), Linear(G53), ProgramEnd]
    assert_eq!(ops.len(), 4);
    match &ops[1] {
        Op::Rapid {
            start: StartPoint::Unknown,
            end,
            state,
            ..
        } => {
            assert_pt(end, 10.0, 0.0, 0.0);
            assert_eq!(state.frame, eid_core::ast::Frame::Machine);
        }
        _ => panic!("op1 не machine Rapid"),
    }
    // Второй machine-ход цепляется от первого.
    match &ops[2] {
        Op::Linear { state, .. } => {
            assert_eq!(known(&ops[2]), [10.0, 0.0, 0.0]);
            assert_eq!(state.frame, eid_core::ast::Frame::Machine);
            assert_eq!(state.feed.as_ref().map(|n| n.value()), Some(150.0));
        }
        _ => panic!("op2 не machine Linear"),
    }
    // N1 (без origins) остаётся Err RUNTIME — §5.3b-диспозиция диалекта.
    assert_eq!(
        err_code(parse(&corpus("n1_g53_no_origins/g53_no_origins.grbl.ngc"))),
        ErrorCode::RuntimeState
    );
}

// ---------------------------------------------------------------------------
// ПОКРЫТИЕ spec_cov_* — GRBL-колонка COVERAGE (см. чек-лист в шапке)
// ---------------------------------------------------------------------------

#[test]
fn spec_cov_lex_exponent_is_bad_number() {
    // Экспонента: read_number читает "1", 'e' → буква E (вне словаря слов) →
    // whole-line Raw{SYNTAX} (E не в A/B/C/U/V/W — правило unsupported_words).
    let out = canonicalize_with_notices("G21\nG0 X1e5\nM2\n", "e.ngc").expect("Ok + Raw");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
}

#[test]
fn spec_cov_lex_stage_parse_vs_canonical_verify() {
    // interface v0.1.2: lexical-диагностика → Stage::Parse;
    // канонизационный инвариант (§6-геометрия, radius mismatch) →
    // Stage::Verify. (unit-switch §2.5 с v0.21 легален — Stage-пин несёт
    // n5a; Verify-stage n7-носителя не существует: носитель — Ok-оп.)
    use eid_core::diag::Stage;
    let d = parse("G21 G90 G94 G17\nG0 X1 Y1 Z1\nG1 &\nM2\n").unwrap_err();
    assert_eq!(d[0].code, ErrorCode::InvalidInput);
    assert_eq!(d[0].stage, Stage::Parse, "lexical должен быть Parse");
    let d = parse(&corpus("n5a_arc_radius_mismatch")).unwrap_err();
    assert_eq!(d[0].code, ErrorCode::InvalidInput);
    assert_eq!(
        d[0].stage,
        Stage::Verify,
        "канон-инвариант должен быть Verify"
    );
}

#[test]
fn spec_cov_frame_l20_then_g92_double_preset() {
    // Смешанные пресетки: G10 L20 → G92 — второй пресет → Err SEMANTICS
    // (count-гейт ветки G10 L20; CORPUS n17-класс).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17
G10 L20 P1 X1.0 Y2.0 Z3.0
G92 X0.0 Y0.0 Z5.0
G0 X1 Y1 Z1
M2
"
        )),
        ErrorCode::UnsupportedSemantics
    );
}

#[test]
fn spec_cov_lex_dollar_variants() {
    // Любая $-строка — конфигурация станка → Raw{SYNTAX}: $$, $110=, $-мусор.
    for src in ["$$", "$garbage", "$-5=1"] {
        let out = canonicalize_with_notices(
            &format!("{src}\nG21 G90 G94 G17\nG0 X1 Y1 Z1\nM2\n"),
            "d.ngc",
        )
        .expect("Ok + Raw");
        assert!(
            matches!(
                &out.program.ops[0],
                Op::Raw {
                    code: ErrorCode::UnsupportedSyntax,
                    ..
                }
            ),
            "{src}: ops[0] не Raw{{SYNTAX}}"
        );
    }
}

#[test]
fn spec_cov_modal_conflicts() {
    // Конфликт двух кодов одной группы в кадре → Err INVALID (grbl group violation).
    // motion покрыт modal_violation_detected; здесь остальные 6 групп.
    let cases = [
        "G17 G18", // plane
        "G90 G91", // distance
        "G93 G94", // feed
        "G20 G21", // units (G20 устанавливает, G21 в том же кадре — смена)
        "G54 G55", // wcs
        "G61 G61", // path
    ];
    for c in cases {
        assert_eq!(
            parse(&format!("G0 X1 Y1 Z1\n{c}\nM2\n")).unwrap_err()[0].code,
            ErrorCode::InvalidInput,
            "конфликт `{c}` не отловлен"
        );
    }
}

#[test]
fn spec_cov_modal_solo() {
    // Каждый код каждой группы легален solo (семь групп). G20/G21-соло-набор
    // ограничен одной системой единиц (SPEC §2.5) — смена проверена отдельно.
    let src = "G21 G90 G94 G17 G54 G61\nG0 X1 Y1 Z1\nG55\nG0 X2 Y1 Z1\nG91\nG93\nG1 X1.0 Y1.0 Z1.0 F2.0\nG94\nG56\nG18\nG19\nM2\n";
    let p = parse(src).expect("solo-коды легальны");
    assert!(!p.ops.is_empty());
}

#[test]
fn spec_cov_arc_planes_ijk() {
    // IJK: 3 плоскости × cw/ccw × винтовая ±. Центр = старт + пара осей
    // (I/J/K literal X/Y/Z); направление и 90°-сектор согласованы по геометрии.
    // G17 cw (+винт): из (0,0,0) в (−5,5,2), центр (0,5,0).
    let p = parse("G21 G90 G94 G17\nG0 X0 Y0 Z0\nG2 X-5.0 Y5.0 Z2.0 I0.0 J5.0 F100.0\nM2\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            pitch,
            dir,
            sweep_deg,
            normal,
            ..
        } => {
            assert_pt(center, 0.0, 5.0, 0.0);
            assert!((pitch.value() - 2.0).abs() <= EPS);
            assert_eq!(*dir, eid_core::ast::Dir::Cw);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
            assert_pt(normal, 0.0, 0.0, 1.0);
        }
        _ => panic!("op1 не Arc"),
    }
    // G17 ccw (−винт): центр (5,0,0), e=(5,−5,−2).
    let p = parse("G21 G90 G94 G17\nG0 X0 Y0 Z0\nG3 X5.0 Y-5.0 Z-2.0 I5.0 J0.0 F100.0\nM2\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            pitch,
            dir,
            sweep_deg,
            normal,
            ..
        } => {
            assert_pt(center, 5.0, 0.0, 0.0);
            assert!((pitch.value() + 2.0).abs() <= EPS);
            assert_eq!(*dir, eid_core::ast::Dir::Ccw);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
            assert_pt(normal, 0.0, 0.0, 1.0);
        }
        _ => panic!("op1 не Arc"),
    }
    // G18 cw: пара I(X)+K(Z), нормаль +Y, linear=Y (pitch 2); 90° CW сектор.
    let p = parse("G21 G90 G94 G18\nG0 X0 Y0 Z0\nG2 X5.0 Y2.0 Z-5.0 I5.0 K0.0 F100.0\nM2\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            normal,
            sweep_deg,
            pitch,
            ..
        } => {
            assert_pt(center, 5.0, 0.0, 0.0);
            assert_pt(normal, 0.0, 1.0, 0.0);
            assert!(
                (sweep_deg.value() - 90.0).abs() <= EPS,
                "sweep {}",
                sweep_deg.value()
            );
            assert!((pitch.value() - 2.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
    // G19 ccw: пара J(Y)+K(Z), нормаль +X, linear=X; 90° CCW сектор.
    let p = parse("G21 G90 G94 G19\nG0 X0 Y0 Z0\nG3 X2.0 Y5.0 Z-5.0 J5.0 K0.0 F100.0\nM2\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            normal,
            sweep_deg,
            ..
        } => {
            assert_pt(center, 0.0, 5.0, 0.0);
            assert_pt(normal, 1.0, 0.0, 0.0);
            assert!((sweep_deg.value() - 90.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn spec_cov_arc_r_negative_and_pitch() {
    // R<0 → большая дуга (>180°), центр на другой стороне хорды.
    let p = parse("G21 G90 G94 G17\nG0 X0 Y10 Z0\nG2 X10.0 Y0.0 R-10.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center, sweep_deg, ..
        } => {
            assert_pt(center, 10.0, 10.0, 0.0);
            assert!(
                (sweep_deg.value() - 270.0).abs() <= EPS,
                "sweep {}",
                sweep_deg.value()
            );
        }
        _ => panic!("op1 не Arc"),
    }
    // R + pitch: винтовая R-форма (SPEC §6.1) — Z-слово вне плоскости = pitch.
    let p = parse("G21 G90 G94 G17\nG0 X0 Y0 Z0\nG2 X8.0 Y0.0 Z2.0 R5.0 F100.0\nM2\n").expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            pitch,
            sweep_deg,
            ..
        } => {
            assert_pt(center, 4.0, -3.0, 0.0);
            assert!((pitch.value() - 2.0).abs() <= EPS);
            assert!(
                (sweep_deg.value() - 106.26).abs() <= 0.01,
                "sweep {}",
                sweep_deg.value()
            );
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn spec_cov_cycles_raw_classes() {
    // COVERAGE §2.5 + рулинг оркестратора: G81/G83 — N/A словаря GRBL как
    // источника → Raw{SYNTAX}; G74/G84/G87/G88 — refuse-класс конструкции-
    // источника (LinuxCNC-словарь валиден, «не представимо ходами») →
    // Raw{SEMANTICS}. Разворот/L-лимит — D-зона.
    //
    // SPEC v0.18 §7.4a (зеркало D, агент C): канонизатор циклов в eid-grbl
    // ОТСУТСТВУЕТ — GRBL 1.1 не имеет canned cycles в словаре, G73/G82/G83/
    // G85/G86/G89 — вне словаря → Raw{SYNTAX} и БЕЗ разворачивания. Поэтому
    // клиренс пека (0.254/0.01), G73-модель (одиночный rapid d+клиренс) и
    // G86-требование P — D-константы канонизатора циклов; на grbl-стороне
    // поверхности НЕ имеют (пин диспозиции ниже; эмиссионное зеркало v0.18 —
    // F-кадр на sub=1 — в grbl_emit).
    let cases = [
        ("G81 X1.0 Y2.0 Z-3.0 R1.0", ErrorCode::UnsupportedSyntax),
        (
            "G82 X1.0 Y2.0 Z-3.0 R1.0 P0.5",
            ErrorCode::UnsupportedSyntax,
        ),
        ("G83 X1.0 Y2.0 Z-5.0 Q1.0 L3", ErrorCode::UnsupportedSyntax),
        (
            "G73 X1.0 Y2.0 Z-5.0 Q2.0 R2.0",
            ErrorCode::UnsupportedSyntax,
        ),
        ("G85 X1.0 Y2.0 Z-3.0 R1.0", ErrorCode::UnsupportedSyntax),
        (
            "G86 X1.0 Y2.0 Z-3.0 R1.0 P1.0",
            ErrorCode::UnsupportedSyntax,
        ),
        (
            "G89 X1.0 Y2.0 Z-3.0 R1.0 P0.5",
            ErrorCode::UnsupportedSyntax,
        ),
        ("G84 X1.0 Y2.0 Z-5.0", ErrorCode::UnsupportedSemantics),
        ("G74 X1.0 Y2.0 Z-5.0", ErrorCode::UnsupportedSemantics),
        ("G87 X1.0 Y2.0 Z-5.0", ErrorCode::UnsupportedSemantics),
        ("G88 X1.0 Y2.0 Z-5.0", ErrorCode::UnsupportedSemantics),
        ("G76 X1.0 Y2.0 Z-5.0", ErrorCode::UnsupportedSemantics), // токарный цикл резьбы (§1.2/§2.5)
    ];
    for (g, code) in cases {
        let out =
            canonicalize_with_notices(&format!("G21 G90 G94 G17\n{g}\nF100.0\nM2\n"), "c.ngc")
                .expect("Ok + Raw");
        assert!(
            matches!(&out.program.ops[0], Op::Raw { code: expected, .. } if *expected == code),
            "{g}: ожидался {code:?}"
        );
    }
    // G86 БЕЗ P — та же диспозиция SYNTAX (требование P §7.4a — проверка
    // канонизатора циклов LinuxCNC/Fanuc; словарь GRBL отвергает G86 ДО
    // любых проверок слов).
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG86 X1.0 Y2.0 Z-3.0 R1.0\nF100.0\nM2\n",
        "c.ngc",
    )
    .expect("Ok + Raw");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
}

#[test]
fn spec_cov_frame_g92_partial_and_g92_1() {
    // Частичный G92 → RUNTIME (незаданные оси — неизвестное чтение, §5.3).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17\nG92 X0.0 Y0.0\nG0 X1 Y1 Z1\nM2\n")),
        ErrorCode::RuntimeState
    );
    // G92.1 → Raw{E_REQUIRES_MACHINE_TABLE} (SPEC §3 v0.20 / Q4-частично:
    // зонд 4 — reset возвращает чтения в прежнюю СК, требует абсолютной
    // машинной позиции, которой в AST нет; код сменён с SEMANTICS).
    let out = canonicalize_with_notices("G21 G90 G94 G17\nG92.1\nG0 X1 Y1 Z1\nM2\n", "g.ngc")
        .expect("Ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
}

#[test]
fn spec_cov_frame_g53_no_motion() {
    // G53 без G0/G1 в кадре → INVALID (grbl gcode.c).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17\nG53\nG0 X1 Y1 Z1\nM2\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn spec_cov_frame_wcs_solo() {
    // G54..G59 соло — легальный выбор, state.wcs переключается по опам.
    let src = "G21 G90 G94 G17\nG54\nG0 X1 Y1 Z1\nG55\nG0 X2 Y1 Z1\nG56\nG0 X3 Y1 Z1\nG57\nG0 X4 Y1 Z1\nG58\nG0 X5 Y1 Z1\nG59\nG0 X6 Y1 Z1\nM2\n";
    let p = parse(src).expect("ok");
    let wcs_of = |i: usize| match &p.ops[i] {
        Op::Rapid { state, .. } => state.wcs,
        _ => unreachable!(),
    };
    let expected = [Wcs::G54, Wcs::G55, Wcs::G56, Wcs::G57, Wcs::G58, Wcs::G59];
    for (i, w) in expected.iter().enumerate() {
        assert_eq!(wcs_of(i), *w, "оп {i}");
    }
}

#[test]
fn spec_cov_arc_nonzero_start_exact() {
    // Ненулевой старт на любых осях: арифметика центров/ρ/pitch не вырождается
    // (± -мутанты arc_block ловятся точными ассертами).
    // G17 cw 120°: s=(3,7,1), c=(3,2,1), e = c + Rot_cw120(0,5) = (7.33,−0.5,1).
    let p =
        parse("G21 G90 G94 G17\nG0 X3.0 Y7.0 Z1.0\nG2 X7.3300 Y-0.5000 I0.0 J-5.0 F100.0\nM2\n")
            .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            sweep_deg,
            pitch,
            ..
        } => {
            assert_pt(center, 3.0, 2.0, 1.0);
            assert!(
                (sweep_deg.value() - 120.0).abs() <= 0.01,
                "sweep {}",
                sweep_deg.value()
            );
            assert!(pitch.value().abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn spec_cov_arc_r_nonzero_start() {
    // R-форма с ненулевым d0/d1 и pitch: p̂/sgn/h арифметика не вырождается.
    // s=(3,7,1), e=(−1,10,4), R5 cw: центр (3.598, 11.964), sweep 60°, pitch 3.
    let p = parse("G21 G90 G94 G17\nG0 X3.0 Y7.0 Z1.0\nG2 X-1.0 Y10.0 Z4.0 R5.0 F100.0\nM2\n")
        .expect("ok");
    match &p.ops[1] {
        Op::Arc {
            center,
            sweep_deg,
            pitch,
            ..
        } => {
            assert_pt(center, 3.598, 11.964, 1.0);
            assert!(
                (sweep_deg.value() - 60.0).abs() <= 0.01,
                "sweep {}",
                sweep_deg.value()
            );
            assert!((pitch.value() - 3.0).abs() <= EPS);
        }
        _ => panic!("op1 не Arc"),
    }
}

#[test]
fn spec_cov_arc_g93_arc_len_and_linear_diagonal() {
    // G93: arc_len = ρ·sweep (дуга) и len = √(x²+y²+z²) (диагональ) —
    // формулы длины ловятся точными значениями feed.
    let p = parse("G21 G90 G93 G17\nG92 X0 Y0 Z0\nG2 X-5.0 Y5.0 Z0.0 I0.0 J5.0 F2.0\nM2\n")
        .expect("ok");
    match &p.ops[0] {
        Op::Arc { state, .. } => {
            let f = state.feed.as_ref().expect("feed").value();
            // quarter circle ρ=5: len = 5·π/2 ≈ 7.854; feed = 2·len ≈ 15.708.
            assert!((f - 15.708).abs() <= 0.01, "G93 arc feed {f}");
        }
        _ => panic!("op0 не Arc"),
    }
    let p = parse("G21 G90 G93 G17\nG92 X0 Y0 Z0\nG1 X10.0 Y20.0 F2.0\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Linear { state, .. } => {
            let f = state.feed.as_ref().expect("feed").value();
            assert!(
                (f - 2.0 * 500.0f64.sqrt()).abs() <= 0.01,
                "G93 diagonal feed {f}"
            );
        }
        _ => panic!("op0 не Linear"),
    }
}

#[test]
fn spec_cov_frame_g28_g38_raw() {
    // COVERAGE §2.1/§2.3: G28 → Raw{MACHINE_TABLE}; G38.2 → Raw{SEMANTICS}.
    let out =
        canonicalize_with_notices("G21 G90 G94 G17\nG28\nG0 X1 Y1 Z1\nM2\n", "a.ngc").expect("Ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
    let out =
        canonicalize_with_notices("G21 G90 G94 G17\nG38.2 X1.0\nF50.0\nM2\n", "b.ngc").expect("Ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSemantics,
            ..
        }
    ));
}

#[test]
fn spec_cov_frame_g10_p_variants() {
    // P вне 1..=6 / нецелое → Err INVALID (guard process_g10);
    // частичный origin L2 → Raw{MACHINE_TABLE} (таблица хранит старое).
    for p_word in ["P0", "P7", "P2.5", "P-1"] {
        assert_eq!(
            err_code(parse(&format!(
                "G21 G90 G94 G17\nG10 L2 {p_word} X1.0 Y2.0 Z3.0\nG0 X1 Y1 Z1\nM2\n"
            ))),
            ErrorCode::InvalidInput,
            "{p_word}"
        );
    }
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG10 L2 P1 X1.0 Y2.0\nG0 X1 Y1 Z1\nM2\n",
        "p.ngc",
    )
    .expect("Ok + Raw");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::RequiresMachineTable,
            ..
        }
    ));
}

#[test]
fn spec_cov_frame_g91_at_start_and_breakpoint() {
    // G91 на старте: база = дефолт 0 (SPEC §2 v0.11); G91 в точке разрыва (b)
    // → RUNTIME (инкремент от неизвестного чтения).
    let p = parse("G21 G91 G17\nG1 X1.0 Y2.0 F100.0\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Linear { end, .. } => assert_pt(end, 1.0, 2.0, 0.0),
        _ => panic!("op0 не Linear"),
    }
    let src = "G21 G90 G17\nG0 X5.0 Y0 Z0\nG55\nG91\nG1 X1.0 Y1.0 Z1.0\nM2\n";
    assert_eq!(err_code(parse(src)), ErrorCode::RuntimeState);
}

#[test]
fn spec_cov_frame_dist_feed_resets() {
    // DistAbs/FeedPerMin соло-сброс мод: G91→G90 абсолют; G93→G94 — F как есть.
    let src = "G21 G90 G94 G17\nG92 X10 Y10 Z10\nG91\nG1 X1.0 Y0 Z0 F100.0\nG90\nG94\nG1 X20.0 Y10.0 Z10.0 F200.0\nM2\n";
    let p = parse(src).expect("ok");
    match &p.ops[0] {
        Op::Linear { end, .. } => assert_pt(end, 11.0, 10.0, 10.0),
        _ => panic!("op0 не Linear"),
    }
    match &p.ops[1] {
        Op::Linear { state, end, .. } => {
            // G90: абсолют X20 (не 21 инкрементально); G94: F как есть (не len×F).
            assert_pt(end, 20.0, 10.0, 10.0);
            assert_eq!(state.feed.as_ref().map(|n| n.value()), Some(200.0));
        }
        _ => panic!("op1 не Linear"),
    }
}

#[test]
fn spec_cov_coolant_ops_state() {
    // MistOn/CoolantOff соло-руки: M7 → mist в state; M9 → none.
    let p = parse("G21 G90 G94 G17\nM7\nG0 X1 Y1 Z1\nM9\nG0 X2 Y1 Z1\nM2\n").expect("ok");
    let cool_of = |i: usize| match &p.ops[i] {
        Op::Rapid { state, .. } => (state.coolant.flood, state.coolant.mist),
        _ => unreachable!(),
    };
    assert_eq!(cool_of(0), (false, true));
    assert_eq!(cool_of(1), (false, false));
}

#[test]
fn spec_cov_limit_max_source_lines() {
    // COVERAGE §1.2: ровно 1_000_000 строк — Ok; 1_000_001 — Err LIMIT.
    let ok_src = format!("{}M2\n", "G21\n".repeat(999_999));
    assert_eq!(ok_src.lines().count(), 1_000_000);
    parse(&ok_src).expect("1M строк легальны");
    let over = format!("{}M2\n", "G21\n".repeat(1_000_000));
    let d = parse(&over).unwrap_err();
    assert_eq!(d[0].code, ErrorCode::LimitExceeded);
    assert_eq!(
        d[0].limit.as_ref().map(|l| l.threshold),
        Some("max_source_lines")
    );
}

#[test]
fn spec_cov_limit_max_ops_boundary() {
    // COVERAGE §1.2: сумма ops+notices; граница 200_000: снизу Ok, +1 → Err.
    // Ходы различны (нулевые rapid'ы давали бы лишние notices); финальный
    // zero-length linear даёт notice без опа — проверяет ±-мутант суммы.
    let base = "G21 G90 G94 G17\nG92 X0 Y0 Z0\n";
    let ok_moves: String = (0..199_998).map(|i| format!("G0 X{i} Y0 Z0\n")).collect();
    let ok_src = format!("{base}{ok_moves}G1 X199997 Y0 Z0 F100.0\nM2\n");
    // ops: 199_998 rapid + 1 ProgramEnd = 199_999; notices: 1 → сумма 200_000.
    parse(&ok_src).expect("граница снизу легальна");
    let over_moves: String = (0..199_999).map(|i| format!("G0 X{i} Y0 Z0\n")).collect();
    let over_src = format!("{base}{over_moves}G1 X199998 Y0 Z0 F100.0\nM2\n");
    let d = parse(&over_src).unwrap_err();
    assert_eq!(d[0].code, ErrorCode::LimitExceeded);
    assert_eq!(
        d[0].limit.as_ref().map(|l| l.threshold),
        Some("max_ops_per_program")
    );
}

#[test]
fn spec_cov_arc_near_degenerate_r_notice() {
    // EQUIVALENCE §4: |cos(sweep/2)| < 1e-3 (±0.12° около 180°) → notice
    // NearDegenerateRArc, дуга ОСТАЁТСЯ (не отказ, не drop).
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG92 X0 Y10 Z0\nG2 X0.0175 Y-9.99999 R10.0 F100.0\nM2\n",
        "nd.ngc",
    )
    .expect("ok");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(
        out.notices[0].kind,
        eid_grbl::canonicalize::NoticeKind::NearDegenerateRArc
    );
    assert!(matches!(&out.program.ops[0], Op::Arc { .. }));
}

#[test]
fn spec_cov_lex_whitespace_and_word_boundary() {
    // Ведущие/множественные/хвостовые пробелы и табы; граница max_word_chars
    // (31 цифра Ok / 32 WordTooLong); col в ошибке (мутанты col-арифметики).
    let toks = eid_grbl::lexer::lex_line("  G1  X5\tY6  ", 1).expect("ok");
    assert_eq!(toks.len(), 3);
    let toks = eid_grbl::lexer::lex_line("G 1 X 5", 2).expect("ok");
    assert_eq!(toks.len(), 2); // G1 + X5
    let ok =
        eid_grbl::lexer::lex_line(&format!("X{}", "1".repeat(31)), 3).expect("31 цифра легальна");
    assert_eq!(ok.len(), 1);
    let e = eid_grbl::lexer::lex_line(&format!("X{}", "1".repeat(32)), 4).unwrap_err();
    assert_eq!(e.kind, eid_grbl::lexer::LexErrorKind::WordTooLong);
    assert_eq!(e.line, 4);
    let e = eid_grbl::lexer::lex_line("G1 &", 5).unwrap_err();
    assert_eq!(e.col, 4, "col ошибки: позиция &");
}

// ---------------------------------------------------------------------------

#[test]
fn unit_g93_inverse_time_resolution() {
    // G93: feed = len(траектория) × F (SPEC §4): len 10 × F2 = 20.
    let p = parse("G21 G90 G93 G17\nG92 X0 Y0 Z0\nG1 X10.0 F2.0\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Linear { state, .. } => {
            let f = state.feed.as_ref().expect("feed есть").value();
            assert!((f - 20.0).abs() <= EPS, "G93 feed {f} != 20");
        }
        _ => panic!("op0 не Linear"),
    }
}

#[test]
fn unit_g91_incremental_resolution() {
    let p = parse("G21 G91 G17\nG92 X1 Y2 Z3\nG1 X1.0 Y1.0 Z1.0 F100.0\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Linear { start, end, .. } => {
            assert_eq!(known(&p.ops[0]), [1.0, 2.0, 3.0]); // якорь G92
            assert_pt(end, 2.0, 3.0, 4.0); // инкремент разрешён в абсолют
            let _ = start;
        }
        _ => panic!("op0 не Linear"),
    }
}

#[test]
fn unit_zero_length_linear_dropped_with_notice() {
    let out = canonicalize_with_notices(
        "G21 G90 G94 G17\nG92 X0 Y0 Z0\nG1 X0.0 Y0.0 Z0.0 F100.0\nM2\n",
        "z.ngc",
    )
    .expect("ok");
    // Linear отброшен; остаются только ProgramEnd.
    assert!(out
        .program
        .ops
        .iter()
        .all(|o| matches!(o, Op::ProgramEnd { .. })));
    assert_eq!(out.notices.len(), 1);
    assert_eq!(
        out.notices[0].kind,
        eid_grbl::canonicalize::NoticeKind::ZeroLengthMotion
    );
}

#[test]
fn unit_cross_wcs_linking_arithmetic() {
    // (b): обе системы in-text → start = prev_end + O_prev − O_new.
    let src = "G21 G90 G94 G17\nG10 L2 P1 X0 Y0 Z0\nG10 L2 P2 X10 Y0 Z0\nG54\nG0 X5.0 Y0 Z0\nG55\nG0 X5.0 Y0 Z0\nM2\n";
    let p = parse(src).expect("ok");
    // ops: [WcsDefine, WcsDefine, Rapid, Rapid, ProgramEnd]
    assert!(matches!(
        &p.ops[2],
        Op::Rapid {
            start: StartPoint::Unknown,
            ..
        }
    )); // (a): без якоря
    assert_eq!(known(&p.ops[3]), [-5.0, 0.0, 0.0]); // 5 + O1(0) − O2(10)
}

#[test]
fn unit_partial_end_at_start_inherits_zero() {
    // SPEC §2 v0.11: старт программы — отсутствующие оси = дефолт 0.
    let p = parse("G21 G90 G94 G17\nG0 X10.0\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Rapid { end, .. } => assert_pt(end, 10.0, 0.0, 0.0),
        _ => panic!("op0 не Rapid"),
    }
}

#[test]
fn unit_partial_end_after_unknown_breakpoint_is_runtime() {
    // (b) без in-text origins → reading Unknown; частичный end → RUNTIME.
    let src = "G21 G90 G94 G17\nG0 X5.0 Y0 Z0\nG55\nG0 X6.0\nM2\n";
    assert_eq!(err_code(parse(src)), ErrorCode::RuntimeState);
}

#[test]
fn unit_g80_naked_coords_invalid() {
    // Голые координаты после G80 без motion-кода → INVALID (COVERAGE §2.1).
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17\nG0 X1 Y1 Z1\nG80\nX2.0 Y2.0 Z2.0\nM2\n"
        )),
        ErrorCode::InvalidInput
    );
}

#[test]
fn unit_modal_motion_continuation() {
    // Без G80 голые координаты наследуют модальный motion (grbl).
    let p = parse("G21 G90 G94 G17\nG92 X0 Y0 Z0\nG1 X5.0 F100.0\nX10.0\nM2\n").expect("ok");
    assert!(matches!(&p.ops[1], Op::Linear { .. }));
    match &p.ops[1] {
        Op::Linear { end, .. } => assert_pt(end, 10.0, 0.0, 0.0),
        _ => unreachable!(),
    }
}

#[test]
fn unit_t_word_and_modal_violation_invalid() {
    // SPEC §5.2 v0.20 (зонд 6b + source gcode.c:315-316/:936): bare-T →
    // ABSORB в state.selected_tool, НЕ Err (бывший Err «bare T word» снят);
    // в AST tool_change НЕ строится — реальный grbl смену не исполняет.
    let p = parse("G21 G90 G17\nT1\nM2\n").expect("bare T absorbed, no Err");
    assert!(
        p.ops.iter().all(|o| !matches!(o, Op::ToolChange { .. })),
        "T без M6 не строит ToolChange (gcode.c:939 «[6. Change tool]: NOT SUPPORTED»)"
    );
    // Модальный конфликт остаётся Err (структура кадра, не T-слово).
    assert_eq!(
        err_code(parse("G21 G90 G17\nG0 G1 X5.0\nM2\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn unit_dwell_and_stops() {
    let p = parse("G21 G90 G17\nG4 P1.5\nM1\nM2\n").expect("ok");
    match &p.ops[0] {
        Op::Dwell { seconds, .. } => assert!((seconds.value() - 1.5).abs() <= EPS),
        _ => panic!("op0 не Dwell"),
    }
    match &p.ops[1] {
        Op::ProgramStop { optional, .. } => assert!(optional),
        _ => panic!("op1 не ProgramStop"),
    }
    // G4 P0 → drop+notice; G4 P<0 → INVALID.
    let out = canonicalize_with_notices("G21 G90 G17\nG4 P0\nM2\n", "d.ngc").expect("ok");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(
        err_code(parse("G21 G90 G17\nG4 P-1.0\nM2\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn unit_g53_machine_frame_and_limits() {
    // (c): machine-ход вычислим только с in-text origin (§5.3) → N1-класс.
    assert_eq!(
        err_code(parse(
            "G21 G90 G94 G17\nG0 X1 Y1 Z1\nG53 G0 X0.0 Y0.0 Z-5.0\nM2\n"
        )),
        ErrorCode::RuntimeState
    );
    // С in-text origin: machine-ход ок; (d) первый WCS-ход после — Known.
    let src = "G21 G90 G94 G17\nG10 L2 P1 X0 Y0 Z0\nG0 X1.0 Y0 Z0\nG53 G0 X0.0 Y0.0 Z-5.0\nG0 X1.0 Y0 Z0\nM2\n";
    let p = parse(src).expect("ok");
    // ops: [WcsDefine, Rapid(wcs), Rapid(machine), Rapid(wcs), ProgramEnd]
    match &p.ops[2] {
        Op::Rapid { state, end, .. } => {
            assert_eq!(state.frame, eid_core::ast::Frame::Machine);
            assert_pt(end, 0.0, 0.0, -5.0);
        }
        _ => panic!("op2 не machine Rapid"),
    }
    // (d): start = M_after − O_wcs = (0,0,−5) − (0,0,0).
    assert_eq!(known(&p.ops[3]), [0.0, 0.0, -5.0]);
}

// ---------------------------------------------------------------------------
// SPEC v0.20 (коммит c76ed9a; ground truth target/probe/g92/PROBE_REPORT.md):
// bare-T ABSORB / G92-mid chaining / Raw-хвост после program_end
// ---------------------------------------------------------------------------

#[test]
fn v020_bare_t_absorb_selected_tool() {
    // Пункт 1 (зонд 6b + source gcode.c:315-316/:936 «NOT SUPPORTED, only
    // tracks tool value»): `T4` без M6 → НЕ Err, ABSORB в state
    // (selected_tool); в AST tool_change НЕ строится (реальный grbl смену
    // не исполняет; M6 вне словаря GRBL — gcode.c:939).
    let p = parse("G21 G90 G94 G17\nT4\nG0 X1.0\nM2\n").expect("bare T → Ok (нет Err «bare T»)");
    assert!(p.ops.iter().all(|o| !matches!(o, Op::ToolChange { .. })));
    // T-строка опа не строит (аналог F/S-абсорбции): только ход + PEND.
    assert_eq!(p.ops.len(), 2, "ops: Rapid, ProgramEnd");
    // Повторное T обновляет state (`gc_state.tool = t` на каждом блоке,
    // gcode.c:936) — по-прежнему Ok, без опов и без отказов.
    parse("G21 G90 G94 G17\nT4\nT7\nG0 X1.0\nM2\n").expect("повторное T → Ok");
    // Регресс «T с M6»: M6 вне словаря GRBL — строка Raw{SYNTAX} как прежде,
    // ToolChange не строится ни при какой форме.
    let out =
        canonicalize_with_notices("G21 G90 G94 G17\nT4 M6\nG0 X1.0\nM2\n", "t6.ngc").expect("Ok");
    assert!(matches!(
        &out.program.ops[0],
        Op::Raw {
            code: ErrorCode::UnsupportedSyntax,
            ..
        }
    ));
    assert!(out
        .program
        .ops
        .iter()
        .all(|o| !matches!(o, Op::ToolChange { .. })));
    // Диапазон-чеки зеркальны источнику: только non-negative (gcode.c:315)
    // и max (gcode.c:316, MAX_TOOL_NUMBER 255).
    assert_eq!(
        err_code(parse("G21 G90 G94 G17\nT256\nM2\n")),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        err_code(parse("G21 G90 G94 G17\nT-1\nM2\n")),
        ErrorCode::InvalidInput
    );
}

#[test]
fn v020_g92_mid_golden_chain_new_reading() {
    // Пункт 2, golden из рулинга: `G0 X10 Y10 / G92 X0 Y0 / G1 X5` →
    // frame_preset-семантика mid-program (зонд 1: сдвиг последующих
    // координат) + linear с НОВЫМ чтением (start = (0,0,0) — позиция
    // перечитана в системе G92). AST-носитель блока — Raw (см. process_g92):
    // grammar v0.3 не имеет mid-program frame_preset-опa, chaining — в
    // каноническом состоянии (v0.20 §3).
    let p = parse("G21 G90 G94 G17\nG0 X10.0 Y10.0\nG92 X0 Y0\nG1 X5.0 F100.0\nM2\n")
        .expect("G92-mid → Ok (v0.20)");
    assert!(
        p.frame_preset.is_none(),
        "header — только стартовая пресетка"
    );
    assert!(matches!(&p.ops[0], Op::Rapid { .. }));
    match &p.ops[1] {
        Op::Raw { reason, code, .. } => {
            assert_eq!(*code, ErrorCode::UnsupportedSemantics);
            assert!(reason.contains("frame_preset"), "{reason}");
        }
        other => panic!("op1 не Raw-носитель G92: {other:?}"),
    }
    match &p.ops[2] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_pt(s, 0.0, 0.0, 0.0); // новое чтение после G92 X0 Y0
            assert_pt(end, 5.0, 0.0, 0.0);
        }
        other => panic!("op2 не Linear: {other:?}"),
    }
}

#[test]
fn v020_g92_mid_twice_replace_by_recompute() {
    // Зонд 2: повторная G92 ЗАМЕНЯЕТ регистр по пересчёту от текущей
    // машины (не аккумуляция). Без промежуточного хода: machine (10,0,0);
    // G92 X0 → чтение 0; G92 X5 → чтение 5 (replace, не 0−5); G1 X8 —
    // start = 5 (новое чтение той же точки).
    let p = parse("G21 G90 G94 G17\nG0 X10.0\nG92 X0\nG92 X5\nG1 X8.0 F100.0\nM2\n")
        .expect("два G92 подряд → Ok (Q5 закрыт)");
    let raws = p.ops.iter().filter(|o| matches!(o, Op::Raw { .. })).count();
    assert_eq!(raws, 2, "несколько G92 = несколько опов-носителей");
    match &p.ops[3] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_pt(s, 5.0, 0.0, 0.0);
            assert_pt(end, 8.0, 0.0, 0.0);
        }
        other => panic!("op3 не Linear: {other:?}"),
    }
}

#[test]
fn v020_g92_mid_per_axis_y_reading_unchanged() {
    // Зонд 3: per-axis assignment — `G92 X0` меняет чтение ТОЛЬКО X;
    // Y-чтение не меняется (старт linear.Y = 10 — прежняя система).
    let p = parse("G21 G90 G94 G17\nG0 X10.0 Y10.0\nG92 X0\nG1 X5.0 Y5.0 F100.0\nM2\n")
        .expect("per-axis G92 → Ok");
    match &p.ops[2] {
        Op::Linear {
            start: StartPoint::Known(s),
            end,
            ..
        } => {
            assert_pt(s, 0.0, 10.0, 0.0);
            assert_pt(end, 5.0, 5.0, 0.0);
        }
        other => panic!("op2 не Linear: {other:?}"),
    }
}

#[test]
fn v020_tail_after_program_end_golden() {
    // Пункт 3 golden: `G0 X10 / M2 / G0 X20` → Ok; хвост Raw (lossless);
    // program_end остаётся опом в последовательности (НЕ forced-last —
    // последний оп Raw); notice диалект-расхождения (ровно один).
    let out = canonicalize_with_notices("G21 G90 G94 G17\nG0 X10.0\nM2\nG0 X20.0\n", "tail.ngc")
        .expect("tail → Ok (v0.20; бывший Err N12 снят)");
    let kinds: Vec<&str> = out
        .program
        .ops
        .iter()
        .map(|o| match o {
            Op::Rapid { .. } => "rapid",
            Op::ProgramEnd { .. } => "pend",
            Op::Raw { .. } => "raw",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["rapid", "pend", "raw"], "{kinds:?}");
    assert_eq!(out.notices.len(), 1, "один notice на программу");
    let msg = &out.notices[0].message;
    assert!(
        msg.contains("tail after program end")
            && msg.contains("executed in grbl-stream semantics")
            && msg.contains("ignored in rs274-file semantics")
            && msg.contains("dialect-divergent"),
        "текст расхождения: {msg}"
    );
    // Хвост из НЕСКОЛЬКИХ блоков: notice один, все блоки Raw.
    let out2 = canonicalize_with_notices(
        "G21 G90 G94 G17\nG0 X10.0\nM2\nG0 X20.0\n%\nG4 P1.0\n",
        "t2.ngc",
    )
    .expect("Ok");
    assert!(matches!(&out2.program.ops[2], Op::Raw { .. }));
    assert!(matches!(&out2.program.ops[3], Op::Raw { .. }));
    assert!(matches!(&out2.program.ops[4], Op::Raw { .. }));
    assert_eq!(out2.notices.len(), 1, "notice диалект-расхождения — один");
    // Хвост после M30 (BewareOfDog-форма) — та же диспозиция.
    let out3 = canonicalize_with_notices("G21 G90 G94 G17\nG0 X10.0\nM30\nG0 X20.0\n", "t3.ngc")
        .expect("M30 + хвост → Ok");
    assert!(matches!(&out3.program.ops[2], Op::Raw { .. }));
}

#[test]
fn v020_tail_emit_same_dialect_honest_bytes() {
    // SPEC §5.2 v0.20: «same-dialect хранит байты честно» — эмиссия
    // программы с Raw-хвостом легальна (ex-ProgramEndNotLast n12-гейт:
    // отказ только на НЕ-Raw оп после PEND), tail-байт verbatim.
    let out = canonicalize_with_notices("G21 G90 G94 G17\nG0 X10.0\nM2\nG0 X20.0\n", "te.ngc")
        .expect("parse Ok");
    let ad = eid_grbl::GrblAdapter::new();
    let text = eid_core::adapter::Adapter::emit(&ad, &out.program)
        .expect("emit с Raw-хвостом легален (SPEC §5.2 v0.20)")
        .text;
    assert!(text.contains("G0 X20.0"), "хвост verbatim: {text:?}");
    assert!(text.contains("M2"));
    // НЕ-Raw оп после PEND — по-прежнему нарушение канон-формы (emit-гейт).
    let mut bad = out.program.clone();
    bad.ops.insert(
        2,
        Op::Dwell {
            seconds: eid_core::ast::Num::new("1.0".into(), 1.0),
            state: eid_core::ast::State {
                feed: None,
                spindle: eid_core::ast::Spindle::Off,
                coolant: eid_core::ast::Coolant::default(),
                tool: None,
                frame: eid_core::ast::Frame::Wcs,
                wcs: Wcs::G54,
                feed_restated: false,
            },
            src: eid_core::ast::SrcSpan {
                file: std::sync::Arc::from("x"),
                line_start: 1,
                col_start: 1,
                line_end: 1,
                col_end: 1,
            },
            sub: None,
        },
    );
    assert!(
        eid_core::adapter::Adapter::emit(&ad, &bad).is_err(),
        "не-Raw оп после PEND → отказ n12"
    );
}

// ---------------------------------------------------------------------------
// v0.21-кандидат (инвентаризация Tier A#3): chain-continuity через Raw-блоки.
// Raw-носитель с осевыми словами представляет РЕАЛЬНОЕ движение контроллера
// (`G41 G1 Y4` ведёт Y в 4; rotary-only `G0 A0.` при RTCP двигает TCP), ход
// непредставим в AST L1 (split-parse запрещён) → позиционная цепочка
// разрывается (Ctx.pos_unknown). Следующий ход — от start=unknown (Q11,
// SPEC §2 принцип 2): ARC → честный E_RUNTIME_STATE вместо ложной
// «невозможной дуги / radius mismatch» от СТАРОЙ позиции (зонд: с G41-блоком
// отказ, без — OK); RAPID/LINEAR с полным end — Ok от unknown.
// ---------------------------------------------------------------------------

#[test]
fn v021_raw_axis_block_arc_unknown_start_runtime() {
    // comp311-мимикрия: `G41 G1 Y4` — G41 вне словаря GRBL → Raw-носитель
    // с осевым словом Y; дуга ПОСЛЕ него парсится от unknown → честный
    // E_RUNTIME_STATE «arc with unknown start», НЕ ложный radius-mismatch
    // (без break: start (1,5,−1), центр (2,5,−1): ρ_start 1 vs ρ_end 2 →
    // INVALID n5a на живом файле).
    let r = parse("G21 G90\nG0 X1 Y5\nG1 Z-1 F60\nG41 G1 Y4\nG3 X2 Y3 I1\n");
    let Err(d) = r else {
        panic!("дуга от разорванной цепочки — E_RUNTIME_STATE")
    };
    assert_eq!(
        d[0].code,
        ErrorCode::RuntimeState,
        "честный код вместо INVALID: {:?}",
        d[0].message
    );
    assert!(
        d[0].message.contains("unknown start"),
        "диагностика указывает unknown-старт: {}",
        d[0].message
    );
}

#[test]
fn v021_raw_axis_block_linear_full_end_from_unknown() {
    // RAPID/LINEAR с ПОЛНЫМ end — Ok от unknown (Q11-механизм); Raw-носитель
    // остаётся lossless-опом в последовательности.
    let p = parse("G21 G90\nG0 X1 Y5\nG41 G1 Y4\nG1 X10 Y20 Z5 F60\nM2\n")
        .expect("linear с полным end — Ok от unknown");
    // ops: rapid, raw(G41-строка), linear, pend
    assert!(matches!(p.ops[0], Op::Rapid { .. }));
    assert!(matches!(p.ops[1], Op::Raw { .. }), "G41-блок — Raw");
    match &p.ops[2] {
        Op::Linear {
            start: StartPoint::Unknown,
            end,
            ..
        } => assert_pt(end, 10.0, 20.0, 5.0),
        other => panic!("linear start=Unknown, получен {other:?}"),
    }
}

#[test]
fn v021_rotary_only_raw_breaks_chain() {
    // rotary-only `G0 A0.` (ncm_g434_rtcp/ncm_g43_g69): A/C — unsupported →
    // Raw-носитель; RTCP-движение TCP непредставимо → цепочка разорвана,
    // следующий linear с полным end — от start=Unknown.
    let p = parse("G21 G90\nG0 A0.\nG1 X10 Y10 Z10 F60\nM2\n")
        .expect("rotary-only Raw + linear от unknown — Ok");
    assert!(matches!(p.ops[0], Op::Raw { .. }));
    match &p.ops[1] {
        Op::Linear {
            start: StartPoint::Unknown,
            ..
        } => {}
        other => panic!("linear start=Unknown, получен {other:?}"),
    }
}

#[test]
fn v021_partial_end_after_raw_axis_block_runtime() {
    // ncm_g43_g69-механизм: `G43 Z162. H1` — Raw-носитель глотает Z162 →
    // частичный end (`Z154.`) наследует неизвестное → честный
    // E_RUNTIME_STATE (SPEC §2 v0.11: дефолт-0 — только на старте,
    // pos_unknown разрывом старта НЕ является).
    let r = parse("G21 G90\nG0 X1 Y5\nG43 Z162. H1\nZ154.\n");
    let Err(d) = r else {
        panic!("частичный end после разрыва — E_RUNTIME_STATE")
    };
    assert_eq!(d[0].code, ErrorCode::RuntimeState);
    assert!(
        d[0].message.contains("partial end"),
        "диагностика: {}",
        d[0].message
    );
}

#[test]
fn v021_comment_axis_letters_do_not_break_chain() {
    // Буквы осей в ТЕКСТЕ комментария («(A : 0 ; C : 0 )» — ncm-шапки)
    // движением не являются: разрыв по словам, не по байтам raw.
    let p = parse("G21 G90\nG0 X1 Y5\n(A : 0 ; C : 0)\nG1 X10 Y20 Z5 F60\nM2\n")
        .expect("комментарий не рвёт цепочку");
    match &p.ops[1] {
        Op::Linear {
            start: StartPoint::Known(s),
            ..
        } => assert_pt(s, 1.0, 5.0, 0.0),
        other => panic!("chaining жив — start Known(1,5,0), получен {other:?}"),
    }
}

#[test]
fn v021_chain_restores_after_full_end_move() {
    // Разрыв одноразовый: ход с явным полным end восстанавливает цепочку —
    // следующий ход снова Known (update_pos сбрасывает pos_unknown).
    let p =
        parse("G21 G90\nG0 X1 Y5\nG41 X2 D1\nG1 X10 Y20 Z5 F60\nG1 X30 Y40 Z5\nM2\n").expect("Ok");
    match &p.ops[2] {
        Op::Linear {
            start: StartPoint::Unknown,
            ..
        } => {}
        other => panic!("первый ход после разрыва — Unknown, получен {other:?}"),
    }
    match &p.ops[3] {
        Op::Linear {
            start: StartPoint::Known(s),
            ..
        } => assert_pt(s, 10.0, 20.0, 5.0),
        other => panic!("цепочка восстановлена, получен {other:?}"),
    }
}
