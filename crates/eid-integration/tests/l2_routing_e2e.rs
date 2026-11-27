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

/// Ротационное слово под Off-профилем (Triv) — носитель lowering →
/// кросс-диалект честно отказывает (§8.2; компиляции не было).
#[test]
fn rotary_carrier_under_off_cross_refuses() {
    let src = "G21 G90\nG0 A90.\nM30\n";
    let err = translate_text_with_profile("fanuc", "linuxcnc", src, "rot.ngc", None)
        .expect_err("ротационный носитель под Off кросс-диалект — отказ");
    match &err {
        TranslateError::Emit(diags) => {
            assert_eq!(diags[0].code, ErrorCode::RawUntranslatable);
            assert!(diags[0].message.contains("RAW op(s)"), "{diags:?}");
        }
        other => panic!("ожидался Emit/RawUntranslatable, фактический: {other:?}"),
    }
}

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

/// ИМЕНОВАННАЯ граница (12.5.4-роут): ротационные ходы под активной
/// кинематикой — честный отказ (L1-грамматика v0.3 без слота A/B/C:
/// эмиссия потеряла бы движение ротационной оси; полный-TCP lowering с
/// круговыми инвариантами доступен библиотечным lower()).
#[test]
fn rotary_motion_under_kinematics_refuses_named_boundary() {
    let src = concat!(
        "O0001\n",
        "G21 G90\n",
        "G43.4 H1\n",
        "G0 X10. Y0 Z-10. A90.\n",
        "M30\n",
    );
    let prof = fanuc_rtcp_profile();
    let err = translate_text_with_profile("fanuc", "grbl-1.1", src, "rtcp5x.nc", Some(&prof))
        .expect_err("ротационные ходы под кинематикой — именованный отказ");
    match &err {
        TranslateError::Parse(diags) => {
            assert_eq!(diags[0].code, ErrorCode::UnsupportedSemantics);
            let m = &diags[0].message;
            assert!(m.contains("ротационными словами"), "{m}");
            assert!(m.contains("L1-грамматика v0.3"), "{m}");
            assert!(m.contains("eid_l2::lowering::lower"), "{m}");
        }
        other => panic!("ожидался Parse/UnsupportedSemantics, фактический: {other:?}"),
    }
}

/// Тот же ротационный ход под Off (Triv) — прежнее поведение: носитель
/// lowering → кросс-диалект §8.2-отказ (покрыто
/// rotary_carrier_under_off_cross_refuses); здесь — same-dialect verbatim
/// остаётся легальным.
#[test]
fn rotary_motion_under_off_same_dialect_still_verbatim() {
    let src = "G21 G90\nG0 A90.\nM30\n";
    // Off-ветка: Triv-профиль (fanuc_default) — гейт ротационных ходов не
    // срабатывает (kins_active = false), кадр идёт носителем lowering.
    let triv = MachineProfile::fanuc_default();
    assert_eq!(
        triv.kinematics,
        eid_integration::profile::MachineKinematics::Triv
    );
    let out = translate_text_with_profile("fanuc", "fanuc", src, "rot.ngc", Some(&triv))
        .expect("same-dialect ротационный кадр под Triv — verbatim");
    assert!(out.emitted.contains("A90"), "{:?}", out.emitted);
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
