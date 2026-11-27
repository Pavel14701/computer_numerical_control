//! e2e: L2-маршрут translate (12.4-роут + разморозка kinematics 12.5.4).
//!
//! Контракт: текст с L2-конструкциями диалекта с front-end (fanuc/haas/
//! linuxcnc) роутится в L2-конвейер (front-end parse → Pass-1 lower с
//! кинематикой профиля → сборка L1-Program v0.3 → общий хвост: AST-лимиты →
//! RAW-правило §8.2 → emit цели). Чистые L1-тексты, диалекты без front-end
//! (grbl), NURBS G5.x (SPEC v0.19) и чистые макро-фрагменты (ноль
//! machine-опов) — прежний L1-конвейер / честные отказы. Серия: L2-тесты
//! `cli_dir_*` (verbatim-фрагменты) в eid-cli; порядок носителей/ходов —
//! юнит-тесты eid-l2 (into_ordered_ops); kinematics-проход библиотечного
//! lower() — `kinematics_lowering_e2e.rs`.

use eid_core::ast::{Op, Units};
use eid_core::diag::ErrorCode;
use eid_integration::profile::{MachineProfile, TrtKinematics};
use eid_integration::translate::{translate_text_with_profile, TranslateError};
use eid_l2::kinematics::{xyzac_inverse, Pose6};

/// Профиль fanuc с XYZAC trt-кинематикой (нулевая геометрия, конвенция
/// LinuxCNC «opposite» — con = −1).
fn fanuc_rtcp_profile() -> MachineProfile {
    let mut p = MachineProfile::fanuc_default();
    p.kinematics = eid_integration::profile::MachineKinematics::XyzacTrt(TrtKinematics {
        rot_point: [0.0, 0.0, 0.0],
        y_offset: 0.0,
        x_offset: 0.0,
        z_offset: 0.0,
        z_offset_tool: 0.0,
        conventional_directions: false,
    });
    p.validate().expect("rtcp-профиль валиден");
    p
}

/// Тот же XYZAC, ДРУГАЯ геометрия (пивот −345 — реальный станочный класс):
/// для hash-mismatch-теста (тихая порча закрыта маркером).
fn fanuc_rtcp_profile_other_geometry() -> MachineProfile {
    let mut p = MachineProfile::fanuc_default();
    p.kinematics = eid_integration::profile::MachineKinematics::XyzacTrt(TrtKinematics {
        rot_point: [0.0, 0.0, -345.0],
        y_offset: 0.0,
        x_offset: 0.0,
        z_offset: 25.0,
        z_offset_tool: 110.0,
        conventional_directions: false,
    });
    p.validate().expect("rtcp-профиль валиден");
    p
}

/// Fanuc Macro B: переменная + полный-позный ход с выражением — lowering
/// даёт concrete-опы, эмиссия цели без макро-текста. (Неполный первый кадр
/// без якоря — честный IncompletePose-носитель: иначе-класс, см.
/// off-ветку/якорную семантику L2Config.initial_pos.)
#[test]
fn macro_b_cross_dialect_compiles_to_concrete_ops() {
    let src = concat!("#100 = 25.\n", "G21 G90\n", "G0 X#100 Y0 Z5.\n", "M30\n",);
    let out = translate_text_with_profile("fanuc", "linuxcnc", src, "mb.ngc", None)
        .expect("Macro B кросс-диалект компилируется (l2_available + front-end)");
    assert_eq!(out.from, "fanuc");
    assert_eq!(out.to, "linuxcnc");
    // В program.ops нет Raw: макрос resolved (lowering concrete).
    assert!(
        !out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })),
        "Macro B должен компилироваться без Raw-носителей: {:?}",
        out.program.ops
    );
    // Ход с concrete-координатой (выражение #100 вычислено).
    let rapid = out
        .program
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Rapid { end, .. } => Some(end),
            _ => None,
        })
        .expect("lowered Rapid присутствует");
    assert_eq!(
        rapid.x.value(),
        25.0,
        "#100 = 25 → X25 (выражение resolved)"
    );
    // Эмиссия цели — без макро-текста.
    assert!(
        !out.emitted.contains('#'),
        "эмиссия без #: {:?}",
        out.emitted
    );
    assert!(
        out.emitted.contains("X25"),
        "конкретика в эмиссии: {:?}",
        out.emitted
    );
    assert!(out.l2_lowering_diagnostics.is_empty());
}

/// Чистый макро-фрагмент (присваивания/циклы, ноль machine-опов) —
/// same-dialect: Ok(None)-fallback → L1-конвейер → verbatim-носители §8.1.
#[test]
fn macro_fragment_same_dialect_falls_back_to_verbatim() {
    let src = concat!(
        "( CLEAR VARIABLES )\n",
        "#1 = 100\n",
        "WHILE[ #1 LE 147] DO1\n",
        "#[#1]=#0\n",
        "#1 = #1 + 1\n",
        "END1\n",
    );
    let out = translate_text_with_profile("fanuc", "fanuc", src, "frag.nc", None)
        .expect("same-dialect макро-фрагмент — verbatim (L1-fallback)");
    assert!(
        out.emitted.contains("WHILE["),
        "фрагмент verbatim: {:?}",
        out.emitted
    );
    assert!(
        out.l2_lowering_diagnostics.is_empty(),
        "L2-маршрут не задействован (fallback): {:?}",
        out.l2_lowering_diagnostics
    );
}

/// Тот же фрагмент (с инкрементом — цикл завершается, lowering даёт ноль
/// machine-опов → Ok(None)-fallback) кросс-диалект — L1-fallback даёт
/// Raw-носители → честный raw_refusal §8.2. (Бесконечный цикл вместо
/// завершающегося был бы ЛЕГАЛЬНО другим исходом: refusal на этапе
/// исполнения — реальный контроллер завис бы так же.)
#[test]
fn macro_fragment_cross_dialect_refuses_raw_rule() {
    let src = "#1 = 100\nWHILE[ #1 LE 147] DO1\n#[#1]=#0\n#1 = #1 + 1\nEND1\n";
    let err = translate_text_with_profile("fanuc", "grbl-1.1", src, "frag.nc", None)
        .expect_err("кросс-диалект фрагмента — отказ");
    match &err {
        TranslateError::Emit(diags) => {
            assert_eq!(diags[0].code, ErrorCode::RawUntranslatable);
        }
        other => panic!("ожидался Emit/RawUntranslatable, фактический: {other:?}"),
    }
}

// (СНЯТО — L1 v0.4, SPEC v0.22) Тест `rotary_carrier_under_off_cross_refuses`
// флипнулся в успех: ротационный кадр под Off понижается pass-through-опом
// со слотами — кросс-диалект легален. Покрытие переехало в
// `rotary_motion_under_off_lowers_passthrough_slots`.

/// ФЛАГМАН разморозки (12.5.4): 3-осевой TCP на 5-осевой машине — Fanuc
/// Macro B + G43.4 + TCP-ход (без ротационных слов) под XYZAC trt-профилем
/// → кросс-диалект grbl: ход resolved в machine-оси (TCP→joints через ядро
/// — оракул-сверка в тесте), G43.4 материализован комментарием шапки
/// (двойное преобразование запрещено), Raw-носителей нет — эмиссия цели
/// легальна.
#[test]
fn rtcp_profile_compiles_tcp_to_joints_cross_dialect() {
    let src = concat!(
        "O0001\n",
        "G21 G90\n",
        "G43.4 H1\n",
        "G0 X10. Y0 Z-10.\n",
        "M30\n",
    );
    let prof = fanuc_rtcp_profile();
    let out = translate_text_with_profile("fanuc", "grbl-1.1", src, "rtcp.nc", Some(&prof))
        .expect("TCP-программа кросс-диалект компилируется (разморозка 12.5.4)");
    // Ход lowered: joints = xyzac_inverse(нулевая геометрия, TCP(10,0,-10), rot 0).
    let eid_l2::interp::RtcpMode::XyzacTrt { params } = prof.kinematics.to_rtcp_mode() else {
        panic!("профиль обязан давать XyzacTrt-режим");
    };
    let pose = Pose6::new(10.0, 0.0, -10.0, 0.0, 0.0, 0.0);
    let want = xyzac_inverse(&params, &pose);
    let linear = out
        .program
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Rapid { end, .. } => Some(end),
            _ => None,
        })
        .expect("lowered Rapid (machine-оси) присутствует");
    assert!(
        (linear.x.value() - want.x).abs() < 1e-9
            && (linear.y.value() - want.y).abs() < 1e-9
            && (linear.z.value() - want.z).abs() < 1e-9,
        "TCP→joints: ожидалось {want:?}, фактическое {:?}",
        (linear.x.value(), linear.y.value(), linear.z.value())
    );
    // G43.4-маркер: НЕ исполняемый носитель, а комментарий шапки.
    assert!(
        !out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })),
        "RTCP-маркер материализован — Raw не остаётся: {:?}",
        out.program.ops
    );
    let marker_at = out
        .emitted
        .find("EID RTCP COMPILED: G43.4 H1")
        .expect("комментарий компиляции RTCP в шапке выхода");
    let move_at = out.emitted.find("G0 X").expect("ход в эмиссии");
    assert!(
        marker_at < move_at,
        "комментарий компиляции — до ходов (machine axes from here): {:?}",
        out.emitted
    );
    // «Не молча»: диагностики материализации + lowering (профиль назван).
    assert!(
        out.l2_lowering_diagnostics
            .iter()
            .any(|d| d.message.contains("RTCP-маркер")),
        "{:?}",
        out.l2_lowering_diagnostics
    );
    assert!(
        out.l2_lowering_diagnostics
            .iter()
            .any(|d| d.message.contains("XyzacTrt") && d.message.contains("machine axes")),
        "per-op диагностика lowering называет профиль: {:?}",
        out.l2_lowering_diagnostics
    );
}

/// ФЛАГМАН L1 v0.4 (SPEC v0.22; реверс ревью-4): ротационные ходы под
/// активной кинематикой — END-TO-END: TCP→joints inverse + resolved слоты
/// (программный A90 = pass-through joint), G43.4 → комментарий шапки,
/// кросс-диалект linuxcnc (ось в словаре). Бывший именованный отказ
/// (гейт «L1-грамматика без слота A/B/C») снят слотами.
#[test]
fn rotary_motion_under_kinematics_end_to_end_v022() {
    let src = concat!(
        "O0001\n",
        "G21 G90\n",
        "G43.4 H1\n",
        "G0 X10. Y0 Z-10. A90.\n",
        "M30\n",
    );
    let prof = fanuc_rtcp_profile();
    let out = translate_text_with_profile("fanuc", "linuxcnc", src, "rtcp5x.nc", Some(&prof))
        .expect("rotary-программа кросс-диалект компилируется (L1 v0.4 слоты)");
    // Ход lowered: xyz joints через inverse (оракул-сверка), A = pass-through.
    let eid_l2::interp::RtcpMode::XyzacTrt { params } = prof.kinematics.to_rtcp_mode() else {
        panic!("профиль обязан давать XyzacTrt-режим");
    };
    let pose = Pose6::new(10.0, 0.0, -10.0, 90.0, 0.0, 0.0);
    let want = xyzac_inverse(&params, &pose);
    let linear = out
        .program
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Rapid { end, .. } => Some(end),
            _ => None,
        })
        .expect("lowered Rapid (machine-оси) присутствует");
    assert!(
        (linear.x.value() - want.x).abs() < 1e-9
            && (linear.y.value() - want.y).abs() < 1e-9
            && (linear.z.value() - want.z).abs() < 1e-9,
        "TCP→joints при A90: ожидалось {want:?}, фактическое {:?}",
        (linear.x.value(), linear.y.value(), linear.z.value())
    );
    // Resolved слот: программный A90 — pass-through joint.
    assert_eq!(linear.a.as_ref().map(|x| x.value()), Some(90.0));
    // G43.4-маркер материализован; Raw не остаётся.
    assert!(
        !out.program.ops.iter().any(|o| matches!(o, Op::Raw { .. })),
        "RTCP-маркер материализован — Raw нет: {:?}",
        out.program.ops
    );
    assert!(out.emitted.contains("EID RTCP COMPILED: G43.4 H1"));
    // Эмиссия цели несёт ротационное слово (linuxcnc — ось в словаре).
    assert!(
        out.emitted.contains("A90"),
        "слот в эмиссии: {:?}",
        out.emitted
    );
}

/// Тот же rotary-источник на цель БЕЗ осей в словаре (grbl-1.1) — честный
/// emit-гейт legality (E_UNSUPPORTED_SYNTAX): цель физически не имеет оси.
/// Гейт — LegalityCondition typed-enum (agent-B, Stage 2).
#[test]
fn rotary_motion_to_grbl_refuses_legality_gate() {
    let src = concat!(
        "O0001\n",
        "G21 G90\n",
        "G43.4 H1\n",
        "G0 X10. Y0 Z-10. A90.\n",
        "M30\n",
    );
    let prof = fanuc_rtcp_profile();
    let err = translate_text_with_profile("fanuc", "grbl-1.1", src, "rtcp_grbl.nc", Some(&prof))
        .expect_err("rotary на grbl — emit-гейт legality");
    match &err {
        TranslateError::Emit(diags) => {
            assert_eq!(diags[0].code, ErrorCode::UnsupportedSyntax);
        }
        other => panic!("ожидался Emit/UnsupportedSyntax, фактический: {other:?}"),
    }
}

/// Ротационный ход под Off (Triv) — lowered pass-through со слотами (не
/// носитель): кросс-диалект linuxcnc легален (слот в эмиссии), same-dialect —
/// каноническая эмиссия опа. «Байт-вербатим» — свойство Raw-носителей; слоты
/// — resolved-state (§1.2 v0.22). (Кадр полный-XYZ + A: partial-поза с
/// unknown-стартом — честный IncompletePose-носитель, не этот кейс.)
#[test]
fn rotary_motion_under_off_lowers_passthrough_slots() {
    let src = "G21 G90\nG0 X10. Y0. Z0. A90.\nM30\n";
    let triv = MachineProfile::fanuc_default();
    assert_eq!(
        triv.kinematics,
        eid_integration::profile::MachineKinematics::Triv
    );
    // Кросс-диалект: pass-through слот легален (linuxcnc — ось в словаре).
    let out = translate_text_with_profile("fanuc", "linuxcnc", src, "rot.nc", Some(&triv))
        .expect("rotary pass-through под Off кросс-диалект");
    let rapid = out
        .program
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Rapid { end, .. } => Some(end),
            _ => None,
        })
        .expect("lowered Rapid присутствует");
    assert_eq!(rapid.a.as_ref().map(|x| x.value()), Some(90.0));
    assert!(out.emitted.contains("A90"), "{:?}", out.emitted);
    // Same-dialect: каноническая эмиссия опа (не verbatim-носитель).
    let out_same = translate_text_with_profile("fanuc", "fanuc", src, "rot_same.nc", Some(&triv))
        .expect("same-dialect rotary pass-through");
    assert!(out_same.emitted.contains("A90"), "{:?}", out_same.emitted);
    assert!(
        !out_same
            .program
            .ops
            .iter()
            .any(|o| matches!(o, Op::Raw { .. })),
        "rotary-кадр lowered (не Raw): {:?}",
        out_same.program.ops
    );
}

/// units-switch (G20 и G21 в одной программе) — честный отказ маршрута:
/// L2-lowering не конвертирует координаты (анкер — только L1-адаптер).
#[test]
fn units_mixed_refuses() {
    let src = "G21\n#100 = 5.\nG0 X#100\nG20\nG0 X#100\nM30\n";
    let err = translate_text_with_profile("fanuc", "linuxcnc", src, "units.ngc", None)
        .expect_err("units-switch — отказ маршрута L2");
    match &err {
        TranslateError::Parse(diags) => {
            assert_eq!(diags[0].code, ErrorCode::UnsupportedSemantics);
            assert!(diags[0].message.contains("units-switch"), "{diags:?}");
        }
        other => panic!("ожидался Parse/UnsupportedSemantics, фактический: {other:?}"),
    }
}

/// G20-программа → units Program = In → шапка эмиссии fanuc «G20»
/// (молча-неверная G21-шапка = ошибка ×25.4 — скан обязателен).
#[test]
fn units_g20_emits_inch_header() {
    let src = "G20\n#100 = 5.\nG0 X#100\nM30\n";
    let out = translate_text_with_profile("fanuc", "fanuc", src, "inch.nc", None)
        .expect("G20-программа компилируется");
    assert_eq!(out.program.units, Units::In, "units из скана текста");
    assert!(
        out.emitted.starts_with("G20"),
        "шапка эмиссии G20 (не G21!): {:?}",
        out.emitted
    );
}

/// Ошибка L2-parse — диагностика с префиксом front-end (не паника, не
/// молчаливый fallback).
#[test]
fn l2_parse_error_is_prefixed_diagnostic() {
    let src = "IF [#1 GT\n#100 = 5.\nM30\n";
    let err = translate_text_with_profile("fanuc", "linuxcnc", src, "bad.nc", None)
        .expect_err("битый Macro B — отказ parse");
    match &err {
        TranslateError::Parse(diags) => {
            assert!(
                diags.iter().all(|d| d.message.starts_with("L2 fanuc:")),
                "префикс front-end: {diags:?}"
            );
        }
        other => panic!("ожидался Parse, фактический: {other:?}"),
    }
}

/// Бесконечный WHILE — бюджет T5 lowering → E_LIMIT_EXCEEDED через
/// translate-маршрут (real-контроллер завис бы так же — honest refusal).
/// #1 = 1 фиксирует условие истинным навсегда (пустое тело не считается:
/// Null → GT false → мгновенный выход).
#[test]
fn while_budget_exceeded_refuses() {
    let src = "#1 = 1\nWHILE [#1 GT 0] DO1\nEND1\nM30\n";
    let err = translate_text_with_profile("fanuc", "linuxcnc", src, "loop.ngc", None)
        .expect_err("бесконечный цикл — E_LIMIT_EXCEEDED (T5)");
    match &err {
        TranslateError::Parse(diags) => {
            assert_eq!(diags[0].code, ErrorCode::LimitExceeded);
        }
        other => panic!("ожидался Parse/LimitExceeded, фактический: {other:?}"),
    }
}

/// Чистый L1-файл (grbl) — маршрут не задействован вовсе (нет front-end):
/// прежний конвейер, l2-диагностики пусты.
#[test]
fn plain_l1_grbl_file_unaffected() {
    let src = "G21 G90\nG0 X0 Y0\nG1 X10 Y10 F100\nM30\n";
    let out = translate_text_with_profile("grbl-1.1", "linuxcnc", src, "box.ngc", None)
        .expect("чистый L1 — прежний конвейер");
    assert_eq!(out.from, "grbl-1.1");
    assert!(out.l2_lowering_diagnostics.is_empty());
    assert!(out.emitted.contains("G1 X10"), "{:?}", out.emitted);
}

// ===========================================================================
// ЗАЩИТА ГЕОМЕТРИИ (ревью-6, kinematics_hash): joint-значения специфичны
// для геометрии профиля; маркер `(EID KINEMATICS: … hash=…)` делает её
// явной для следующего хопа EID-пайплайна. Чек-лист «правило → тест»:
//   штамп: kins_active lowering → выход несёт hash-комментарий →
//       kinematics_hash_stamp_roundtrip_v022
//   совпадение hash (реингест того же профиля) → pass-through (повторный
//       inverse = двойное преобразование — форс Off) →
//       kinematics_hash_match_forces_passthrough_v022
//   расхождение hash (та же топология, ДРУГАЯ геометрия) → честный отказ →
//       kinematics_hash_mismatch_refuses_v022
//   детерминизм/чувствительность FNV-1a hash →
//       kinematics_hash_deterministic_and_sensitive (eid-l2, interp tests)
// ===========================================================================

/// Штамп: fanuc rotary-программа + XYZAC-профиль → linuxcnc выход содержит
/// hash-комментарий; реингест выхода (A-слово в тексте → l2_available →
/// L2-маршрут, где геометрия применяется/проверяется) тем же профилем →
/// Ok (hash совпал).
#[test]
fn kinematics_hash_stamp_roundtrip_v022() {
    let src = "O0001\nG21 G90\nG43.4 H1\nG0 X10. Y0 Z-10. A90.\nM30\n";
    let prof = fanuc_rtcp_profile();
    let out = translate_text_with_profile("fanuc", "linuxcnc", src, "rtcp.nc", Some(&prof))
        .expect("rotary-программа компилируется (L1 v0.4)");
    let stamp_line = out
        .emitted
        .lines()
        .find(|l| l.contains("(EID KINEMATICS:"))
        .expect("штамп геометрии в шапке")
        .to_string();
    assert!(stamp_line.contains("hash="), "{stamp_line}");
    assert!(stamp_line.contains("xyzac-trt"), "{stamp_line}");
    assert!(
        stamp_line.contains("rot-point=0,0,0"),
        "параметры геометрии в штампе: {stamp_line}"
    );
    // Реингест того же файла тем же профилем — hash совпал → Ok.
    let back = translate_text_with_profile(
        "linuxcnc",
        "haas",
        &out.emitted,
        "rtcp_back.nc",
        Some(&prof),
    )
    .expect("hash совпал — реингест легален (pass-through)");
    assert!(
        back.emitted.contains("(EID KINEMATICS:"),
        "штамп живёт дальше"
    );
    assert!(
        back.emitted.contains("A90"),
        "слот живёт: {:?}",
        back.emitted
    );
}

/// Совпадение hash → значения уже joints этой геометрии: повторный inverse
/// запрещён (двойное преобразование) — lowering форсируется в pass-through:
/// joint-значения в выходе = joint-значения входа (не inverse∘inverse).
#[test]
fn kinematics_hash_match_forces_passthrough_v022() {
    let src = "O0001\nG21 G90\nG43.4 H1\nG0 X10. Y0 Z-10. A90.\nM30\n";
    let prof = fanuc_rtcp_profile();
    let first = translate_text_with_profile("fanuc", "linuxcnc", src, "a.nc", Some(&prof))
        .expect("первая компиляция");
    // Из первого выхода берём lowered joint-значения.
    let j1 = first
        .program
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Rapid { end, .. } => Some((end.x.value(), end.y.value(), end.z.value())),
            _ => None,
        })
        .expect("lowered op");
    // Реингест тем же профилем: inverse НЕ применяется второй раз —
    // joint-значения проходят 1:1 (были бы inverse(inverse(TCP)) ≠ TCP).
    let second =
        translate_text_with_profile("linuxcnc", "haas", &first.emitted, "b.nc", Some(&prof))
            .expect("hash-совпадение → pass-through");
    let j2 = second
        .program
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Rapid { end, .. } => Some((end.x.value(), end.y.value(), end.z.value())),
            _ => None,
        })
        .expect("reingested op");
    assert!(
        (j1.0 - j2.0).abs() < 1e-9 && (j1.1 - j2.1).abs() < 1e-9 && (j1.2 - j2.2).abs() < 1e-9,
        "pass-through: joints {j1:?} не пере-inverse'ятся: {j2:?}"
    );
    let a2 = second.program.ops.iter().find_map(|o| match o {
        Op::Rapid { end, .. } => end.a.as_ref().map(|n| n.value()),
        _ => None,
    });
    assert_eq!(a2, Some(90.0), "слот pass-through сохранён");
}

/// Two-kins K1 (same-topology different-geometry): XYZAC геометрия A
/// (нулевая) → XYZAC геометрия B (пивот −345, z-offset 25+110) — TRANSFER:
/// forward_A(joints_A) → TCP → inverse_B → joints_B; физическая
/// инвариантность — forward_B(joints_B) ≡ forward_A(joints_A) (ядро как
/// оракул в тесте); штамп переписан на геометрию цели.
#[test]
fn kinematics_hash_mismatch_transfers_k1_v022() {
    use eid_l2::kinematics::{xyzac_forward, Joints5};
    let src = "O0001\nG21 G90\nG43.4 H1\nG0 X10. Y0 Z-10. A90.\nM30\n";
    let prof_a = fanuc_rtcp_profile();
    let first = translate_text_with_profile("fanuc", "linuxcnc", src, "a.nc", Some(&prof_a))
        .expect("компиляция с геометрией A");
    let j_a = first
        .program
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Rapid { end, .. } => Some(end.clone()),
            _ => None,
        })
        .expect("lowered op");
    let other = fanuc_rtcp_profile_other_geometry();
    let second =
        translate_text_with_profile("linuxcnc", "haas", &first.emitted, "b.nc", Some(&other))
            .expect("same-topology different-geometry — K1 transfer");
    let j_b = second
        .program
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Rapid { end, .. } => Some(end.clone()),
            _ => None,
        })
        .expect("transferred op");
    // joints изменились (другая геометрия).
    assert!(
        j_a.x.value() != j_b.x.value() || j_a.z.value() != j_b.z.value(),
        "transfer обязан переписать joints: {j_a:?} vs {j_b:?}"
    );
    // Слоты: A90 сохранилась (same-topology — семантика углов та же).
    assert_eq!(j_b.a.as_ref().map(|n| n.value()), Some(90.0));
    // ФИЗИЧЕСКАЯ ИНВАРИАНТНОСТЬ через ядро: forward_B(joints_B) ≡ forward_A(joints_A).
    let eid_l2::interp::RtcpMode::XyzacTrt { params: pa } = prof_a.kinematics.to_rtcp_mode() else {
        panic!()
    };
    let eid_l2::interp::RtcpMode::XyzacTrt { params: pb } = other.kinematics.to_rtcp_mode() else {
        panic!()
    };
    let pose_a = xyzac_forward(
        &pa,
        &Joints5::new(j_a.x.value(), j_a.y.value(), j_a.z.value(), 90.0, 0.0),
    );
    let pose_b = xyzac_forward(
        &pb,
        &Joints5::new(j_b.x.value(), j_b.y.value(), j_b.z.value(), 90.0, 0.0),
    );
    assert!(
        (pose_a.x - pose_b.x).abs() < 1e-9
            && (pose_a.y - pose_b.y).abs() < 1e-9
            && (pose_a.z - pose_b.z).abs() < 1e-9,
        "мир-инвариантность: forward_A(jA)={pose_a:?} vs forward_B(jB)={pose_b:?}"
    );
    // Штамп переписан на геометрию цели (с её параметрами).
    let stamp = second
        .emitted
        .lines()
        .find(|l| l.contains("(EID KINEMATICS:"))
        .expect("штамп");
    assert!(
        stamp.contains("rot-point=0,0,-345"),
        "геометрия цели: {stamp}"
    );
    assert!(stamp.contains("transferred from xyzac-trt"), "{stamp}");
}

/// Tamper-детекция: подмена параметра в штампе (z-offset) — integrity-пин
/// ловит (параметры обязаны пересчитываться в hash).
#[test]
fn kinematics_stamp_tamper_refuses_v022() {
    let src = "O0001\nG21 G90\nG43.4 H1\nG0 X10. Y0 Z-10. A90.\nM30\n";
    let prof = fanuc_rtcp_profile();
    let out = translate_text_with_profile("fanuc", "linuxcnc", src, "a.nc", Some(&prof))
        .expect("компиляция");
    // Подмена: z-offset 0 → 999 (hash не тронут).
    let tampered = out.emitted.replace("z-offset=0", "z-offset=999");
    assert!(tampered != out.emitted, "подмена применена");
    let err = translate_text_with_profile("linuxcnc", "haas", &tampered, "b.nc", Some(&prof))
        .expect_err("tamper — integrity-отказ");
    match &err {
        TranslateError::Emit(diags) => {
            let m = &diags[0].message;
            assert!(m.contains("stamp tampered"), "{m}");
        }
        other => panic!("ожидался Emit, фактический: {other:?}"),
    }
}

/// Cross-topology (XYZAC → Maxkins) — named отказ (K2: семантика углов
/// между семьями различна — part-frame tool-axis solver per семью).
#[test]
fn kinematics_cross_topology_refuses_v022() {
    let src = "O0001\nG21 G90\nG43.4 H1\nG0 X10. Y0 Z-10. A90.\nM30\n";
    let prof_a = fanuc_rtcp_profile();
    let out = translate_text_with_profile("fanuc", "linuxcnc", src, "a.nc", Some(&prof_a))
        .expect("компиляция");
    let mut mk = MachineProfile::fanuc_default();
    mk.kinematics = eid_integration::profile::MachineKinematics::Maxkins(
        eid_integration::profile::MaxkinsKinematics::default(),
    );
    mk.validate().unwrap();
    let err = translate_text_with_profile("linuxcnc", "haas", &out.emitted, "b.nc", Some(&mk))
        .expect_err("cross-topology — отказ (K2)");
    match &err {
        TranslateError::Emit(diags) => {
            let m = &diags[0].message;
            assert!(m.contains("cross-topology transfer not supported"), "{m}");
            assert!(m.contains("two-kins K2"), "{m}");
        }
        other => panic!("ожидался Emit, фактический: {other:?}"),
    }
}

// ===========================================================================
// Two-kins K2a: кросс-топология XYZAC ↔ XYZBC (trt-семья, перпендикулярные
// плоскости наклона → re-fixture φ=±90°). Чек-лист «правило → тест»:
//   наклон-мэппинг b_B = a_A через формулы осей (XYZAC part-frame
//       (0,−sin a,−cos a) = Rz(−φ)·(sin b,0,−cos b) при φ=+90°) →
//       cross_topology_trt_transfer_v022
//   part-frame позиция: v_B = Rz(φ)·v_A (центры вращения из геометрий) →
//       тот же тест
//   round-trip XYZAC→XYZBC→XYZAC восстанавливает joints →
//       cross_topology_trt_roundtrip_v022
// ===========================================================================

/// Центр вращения стола (зеркало trt_joint_center для теста).
fn test_trt_center(p: &eid_l2::kinematics::TrtParams, xyzac: bool) -> [f64; 3] {
    if xyzac {
        [
            p.rot_point[0],
            p.y_offset + p.rot_point[1],
            p.z_offset + p.rot_point[2],
        ]
    } else {
        [
            p.x_offset + p.rot_point[0],
            p.rot_point[1],
            p.z_offset + p.rot_point[2],
        ]
    }
}

/// K2a transfer: XYZAC (нулевая геометрия) → XYZBC (пивот −345, офсеты):
/// наклон переезжает a→b, part-frame позиция повёрнута fixture'ом φ=+90°,
/// mounting-инструкция в штампе.
#[test]
fn cross_topology_trt_transfer_v022() {
    let src = "O0001\nG21 G90\nG43.4 H1\nG0 X10. Y0 Z-10. A90.\nM30\n";
    let prof_a = fanuc_rtcp_profile();
    let out = translate_text_with_profile("fanuc", "linuxcnc", src, "a.nc", Some(&prof_a))
        .expect("компиляция XYZAC");
    let j_a = out
        .program
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Rapid { end, .. } => Some(end.clone()),
            _ => None,
        })
        .expect("lowered op");

    let mut prof_b = MachineProfile::linuxcnc_default();
    prof_b.kinematics = eid_integration::profile::MachineKinematics::XyzbcTrt(TrtKinematics {
        rot_point: [5.0, 0.0, -345.0],
        y_offset: 0.0,
        x_offset: 7.0,
        z_offset: 25.0,
        z_offset_tool: 110.0,
        conventional_directions: false,
    });
    prof_b.validate().unwrap();
    let second =
        translate_text_with_profile("linuxcnc", "haas", &out.emitted, "b.nc", Some(&prof_b))
            .expect("XYZAC→XYZBC — K2a transfer");
    let j_b = second
        .program
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Rapid { end, .. } => Some(end.clone()),
            _ => None,
        })
        .expect("transferred op");

    // Наклон-мэппинг: b_B = a_A (плоскости выровнены fixture'ом).
    assert_eq!(j_b.b.as_ref().map(|n| n.value()), Some(90.0), "a→b");
    assert!(j_b.a.is_none(), "XYZBC не имеет A-слота");
    // C в источнике не задан (нет C-слова) — None → None (home, resolved).
    assert!(j_b.c.is_none(), "C-home переезжает как None");

    // Part-frame позиция: v_B = Rz(90°)·v_A (центры из геометрий).
    let eid_l2::interp::RtcpMode::XyzacTrt { params: pa } = prof_a.kinematics.to_rtcp_mode() else {
        panic!()
    };
    let eid_l2::interp::RtcpMode::XyzbcTrt { params: pb } = prof_b.kinematics.to_rtcp_mode() else {
        panic!()
    };
    let ca = test_trt_center(&pa, true);
    let cb = test_trt_center(&pb, false);
    let v_a = [
        j_a.x.value() - ca[0],
        j_a.y.value() - ca[1],
        j_a.z.value() - ca[2],
    ];
    // Rz(+90°): (x,y,z) → (−y, x, z)
    let v_b_expected = [-v_a[1], v_a[0], v_a[2]];
    assert!(
        (j_b.x.value() - (v_b_expected[0] + cb[0])).abs() < 1e-9
            && (j_b.y.value() - (v_b_expected[1] + cb[1])).abs() < 1e-9
            && (j_b.z.value() - (v_b_expected[2] + cb[2])).abs() < 1e-9,
        "part-frame позиция через fixture: {j_b:?}"
    );

    // ОСЬ-инвариантность через формулы: part-frame tool-axis A
    // (0,−sin a,−cos a) == Rz(−φ)·(B-table axis (sin b,0,−cos b)).
    let d_a = [
        0.0,
        -(90.0f64).to_radians().sin(),
        -(90.0f64).to_radians().cos(),
    ];
    let d_b_table = [
        (90.0f64).to_radians().sin(),
        0.0,
        -(90.0f64).to_radians().cos(),
    ];
    // Rz(−90°): (x,y,z) → (y, −x, z)
    let d_b_part = [d_b_table[1], -d_b_table[0], d_b_table[2]];
    for i in 0..3 {
        assert!(
            (d_a[i] - d_b_part[i]).abs() < 1e-9,
            "part-frame tool-axis инвариантен: {d_a:?} vs {d_b_part:?}"
        );
    }
    // Mounting-инструкция в штампе.
    let stamp = second
        .emitted
        .lines()
        .find(|l| l.contains("(EID KINEMATICS:"))
        .expect("штамп");
    assert!(stamp.contains("MOUNT PART ROTATED +90 DEG"), "{stamp}");
    assert!(
        stamp.contains("cross-topology transferred from xyzac-trt"),
        "{stamp}"
    );
}

/// Round-trip XYZAC→XYZBC→XYZAC восстанавливает исходные joints (fixture
/// φ=−90° обратный, слоты b→a).
#[test]
fn cross_topology_trt_roundtrip_v022() {
    let src = "O0001\nG21 G90\nG43.4 H1\nG0 X10. Y0 Z-10. A90.\nM30\n";
    let prof_a = fanuc_rtcp_profile();
    let first = translate_text_with_profile("fanuc", "linuxcnc", src, "a.nc", Some(&prof_a))
        .expect("компиляция XYZAC");
    let j_a = first
        .program
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Rapid { end, .. } => Some(end.clone()),
            _ => None,
        })
        .expect("lowered op");

    let mut prof_b = MachineProfile::linuxcnc_default();
    prof_b.kinematics = eid_integration::profile::MachineKinematics::XyzbcTrt(TrtKinematics {
        rot_point: [5.0, 0.0, -345.0],
        y_offset: 0.0,
        x_offset: 7.0,
        z_offset: 25.0,
        z_offset_tool: 110.0,
        conventional_directions: false,
    });
    prof_b.validate().unwrap();
    let second =
        translate_text_with_profile("linuxcnc", "haas", &first.emitted, "b.nc", Some(&prof_b))
            .expect("K2a → XYZBC");

    let back =
        translate_text_with_profile("haas", "linuxcnc", &second.emitted, "c.nc", Some(&prof_a))
            .expect("K2a → обратно XYZAC");
    let j_back = back
        .program
        .ops
        .iter()
        .find_map(|o| match o {
            Op::Rapid { end, .. } => Some(end.clone()),
            _ => None,
        })
        .expect("round-trip op");
    assert_eq!(
        j_back.a.as_ref().map(|n| n.value()),
        Some(90.0),
        "a восстановлен"
    );
    assert_eq!(
        (j_back.x.value() - j_a.x.value()).abs(),
        0.0,
        "xyz round-trip"
    );
    assert_eq!((j_back.y.value() - j_a.y.value()).abs(), 0.0);
    assert_eq!((j_back.z.value() - j_a.z.value()).abs(), 0.0);
}
