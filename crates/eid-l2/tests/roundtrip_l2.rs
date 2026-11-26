//! Round-trip DoD фазы 12.4 (агент D3): «Валидный round-trip L2-программ
//! с G31 probing и таблицами маппинга SysVar между Fanuc и Haas».
//!
//! Маршруты:
//! * fanuc-MB ×7 (ценз 12.1): parse_fanuc → emit_haas → parse_haas →
//!   emit_fanuc → parse_fanuc → semantic_diff(P0, P2) == 0 + byte-детерминизм
//!   emit(P0) == emit(P2);
//! * haas-MB ×12: parse_haas → emit_fanuc → parse_fanuc → emit_haas →
//!   parse_haas → semantic_diff(P0, P2) == 0 (кроме файлов с ЧЕСТНЫМ
//!   отказом Fanuc-ноги: M97/G154/bare-IF — задокументированные различия
//!   диалектов);
//! * linuxcnc ×34: same-dialect emit → parse → semantic_diff == 0.
//!
//! Классификация: FULL (обе ноги Ok, ноль семантических notices) / PARTIAL
//! (обе ноги Ok, есть notices — переведено с оговоркой) / REFUSED (хотя бы
//! одна нога отказана — честный отказ, списком причин). Замечание-скэффолд
//! `main-o-number-synthesized` (синтез O1 — пробел каркаса AST 12.1, не
//! семантика конструкции) в классификации не участвует.
//!
//! Отчёт: `cargo test -p eid-l2 --test roundtrip_l2 -- --nocapture`.

use eid_l2::emit_l2::{ast_semantic_eq, emit, TargetDialect};
use eid_l2::parser_fanuc;
use eid_l2::parser_haas;
use eid_l2::parser_linuxcnc;

fn corpus(rel: &str) -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/tier_b/");
    std::fs::read_to_string(format!("{path}{rel}"))
        .unwrap_or_else(|e| panic!("corpus file {rel} not found: {e}"))
}

/// Замечания-скэффолд (пробел каркаса 12.1, не семантика конструкции) —
/// не участвуют в классификации FULL/PARTIAL.
const SCAFFOLD_KINDS: &[&str] = &["main-o-number-synthesized"];

/// Классификация файла round-trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Full,
    Partial,
    Refused,
}

struct Leg {
    ok: bool,
    errors: Vec<String>,
    semantic_notices: usize,
}

fn leg_report(out: &eid_l2::emit_l2::EmitOutcome) -> Leg {
    Leg {
        ok: out.ok(),
        errors: out.errors.iter().map(|e| e.reason.clone()).collect(),
        semantic_notices: out
            .notices
            .iter()
            .filter(|n| !SCAFFOLD_KINDS.contains(&n.kind))
            .count(),
    }
}

fn fmt_leg(name: &str, l: &Leg) -> String {
    format!(
        "    leg {name}: {} ({} errors, {} semantic notices){}",
        if l.ok { "ok" } else { "REFUSED" },
        l.errors.len(),
        l.semantic_notices,
        if l.errors.is_empty() {
            String::new()
        } else {
            format!("\n      reasons: {}", l.errors.join(" | "))
        }
    )
}

/// Один маршрут A→B→A. semantic_diff по AST (до спанов/текста Num) +
/// byte-детерминизм эмиссии.
fn roundtrip<Fp, Fq>(
    src: &str,
    parse_a: Fp,
    to: TargetDialect,
    parse_b: Fq,
    back: TargetDialect,
) -> (Verdict, String, bool, bool)
where
    Fp: Fn(&str) -> Result<eid_l2::ast::L2Program, Vec<eid_l2::eval::L2Error>>,
    Fq: Fn(&str) -> Result<eid_l2::ast::L2Program, Vec<eid_l2::eval::L2Error>>,
{
    let p0 = match parse_a(src) {
        Ok(p) => p,
        Err(e) => {
            return (
                Verdict::Refused,
                format!("    parse source failed: {e:?}"),
                false,
                false,
            )
        }
    };
    let out_b = emit(&p0, to);
    let leg1 = leg_report(&out_b);
    let p1 = match parse_b(&out_b.text) {
        Ok(p) => p,
        Err(e) => {
            return (
                Verdict::Refused,
                format!("{}\n    re-parse leg-1 failed: {e:?}", fmt_leg("to-b", &leg1)),
                false,
                false,
            )
        }
    };
    let out_a = emit(&p1, back);
    let leg2 = leg_report(&out_a);
    let p2 = match parse_a(&out_a.text) {
        Ok(p) => p,
        Err(e) => {
            return (
                Verdict::Refused,
                format!(
                    "{}\n{}\n    re-parse leg-2 (source dialect) failed: {e:?}",
                    fmt_leg("to-b", &leg1),
                    fmt_leg("back-to-a", &leg2)
                ),
                false,
                false,
            )
        }
    };
    // DoD: semantic_diff == 0 (AST-эквивалентность до спанов/текста Num).
    let sem = ast_semantic_eq(&p0, &p2);
    // Byte-детерминизм: эмиссия детерминирована по AST.
    let byte_det = emit(&p2, back).text == out_a.text;

    let verdict = if !leg1.ok || !leg2.ok || !sem || !byte_det {
        Verdict::Refused
    } else if leg1.semantic_notices + leg2.semantic_notices == 0 {
        Verdict::Full
    } else {
        Verdict::Partial
    };
    let report = format!(
        "{}\n{}    semantic_eq={sem} byte_det={byte_det}",
        fmt_leg("to-b", &leg1),
        fmt_leg("back-to-a", &leg2)
    );
    (verdict, report, sem, byte_det)
}

// ===========================================================================
// fanuc-MB ×7 → F→H→F: ALL PASS (semantic_diff == 0)
// ===========================================================================

#[test]
fn roundtrip_fanuc_7_files() {
    let files = [
        "inbox/gce_full_industrial_lathe.g",
        "inbox/mpe_g65_macro_centerline_probe.nc",
        "inbox/mpe_g65_macro_deep_drill.nc",
        "inbox/mpe_g65_macro_material_verify.nc",
        "inbox/mpe_while_clear_variables.nc",
        "local/cncsim_m98_m99_duas_morsas.nc",
        "local/ncm_g434_rtcp_5x_milling.nc",
    ];
    let mut full = 0;
    let mut partial = 0;
    for f in files {
        let src = corpus(f);
        let (verdict, report, sem, byte_det) = roundtrip(
            &src,
            parser_fanuc::parse_program,
            TargetDialect::Haas,
            parser_haas::parse_program,
            TargetDialect::Fanuc,
        );
        println!("F2H2F {f}: {verdict:?}\n{report}");
        // DoD 12.4: все 7 fanuc-MB файлов — валидный round-trip (AST
        // semantic_diff == 0, byte-детерминизм); отказов нет.
        assert!(sem, "{f}: semantic_diff != 0");
        assert!(byte_det, "{f}: emission not deterministic");
        assert_ne!(verdict, Verdict::Refused, "{f}: unexpected refusal");
        match verdict {
            Verdict::Full => full += 1,
            Verdict::Partial => partial += 1,
            Verdict::Refused => unreachable!(),
        }
    }
    println!("FANUC round-trip: full={full} partial={partial} refused=0");
    assert_eq!(full + partial, 7);
}

// ===========================================================================
// haas-MB ×12 → H→F→H: 5 PASS (partial), 7 честных отказов Fanuc-ноги
// (задокументированные различия диалектов)
// ===========================================================================

#[test]
fn roundtrip_haas_12_files() {
    // (файл, ожидание): Ok → semantic_diff обязателен; отказ → ключевое
    // слово причины (честный отказ — задание 12.4).
    let expectations: &[(&str, Result<(), &str>)] = &[
        ("inbox/mpe_toolpath_profiling_m1.nc", Ok(())),
        // M97 ×2 + G154-носители + bare-IF-носители — не существует в Fanuc.
        ("local/cncsim_g154_g43_haas_vf2.nc", Err("M97")),
        ("local/coyo_tram_b_probe_o09817.nc", Ok(())),
        ("local/rensh_g04p_bare_xyz_o09811.nc", Ok(())),
        ("local/rensh_g154p98_tbd_o09006.nc", Err("G154")),
        ("local/rensh_g65_probe_bore_o09351.nc", Ok(())),
        ("local/rensh_settings_check_g103p1_o09725.nc", Ok(())),
        ("local/vultr_m97_l_macro_o9016.nc", Err("M97")),
        // Haas bare-IF без THEN над не-присваиванием (`IF [#4 LT 0] S800 M04`)
        // — носитель; Fanuc bare-IF не принимает (allow_bare_if=false).
        ("local/zapcon_g31_tsc_o9613.nc", Err("bare-IF")),
        ("local/zapcon_g65_ij_g154p11_endmill_ex.nc", Err("G154")),
        ("local/zapcon_g81_p0_g154p10_drill_ex.nc", Err("G154")),
        ("local/zodiac_toolbreak_m59_g154pvar.nc", Err("G154")),
    ];
    let mut partial = 0;
    let mut refused = 0;
    for (f, expect) in expectations {
        let src = corpus(f);
        let (verdict, report, sem, byte_det) = roundtrip(
            &src,
            parser_haas::parse_program,
            TargetDialect::Fanuc,
            parser_fanuc::parse_program,
            TargetDialect::Haas,
        );
        println!("H2F2H {f}: {verdict:?}\n{report}");
        match expect {
            Ok(()) => {
                assert!(sem, "{f}: expected valid round-trip, semantic_diff != 0");
                assert!(byte_det, "{f}: emission not deterministic");
                assert_ne!(verdict, Verdict::Refused, "{f}: unexpected refusal");
                partial += 1;
            }
            Err(keyword) => {
                assert_eq!(verdict, Verdict::Refused, "{f}: expected honest refusal");
                // Причина отказа называет непереводимую конструкцию.
                assert!(
                    report.contains(keyword),
                    "{f}: refusal reason must mention {keyword:?}\n{report}"
                );
                refused += 1;
            }
        }
    }
    println!("HAAS round-trip: full=0 partial={partial} refused={refused}");
    assert_eq!(partial, 5);
    assert_eq!(refused, 7);
}

// ===========================================================================
// LinuxCNC ×34: same-dialect emit → parse → semantic_diff == 0
// (GOTO-golden — unit-тесты emit_linuxcnc; здесь источники linuxcnc —
// реструктуризация не порождается, но все конструкции проходят)
// ===========================================================================

#[test]
fn roundtrip_linuxcnc_same_dialect() {
    let files = [
        "local/lcnc_configs_sim_gmoccapy_macros_change.ngc",
        "local/lcnc_configs_sim_gmoccapy_macros_change_g43.ngc",
        "local/lcnc_configs_sim_gmoccapy_macros_go_to_position.ngc",
        "local/lcnc_g33_threading.ngc",
        "local/lcnc_m61_output_demo.ngc",
        "local/lcnc_m70_m72_demo.ngc",
        "local/lcnc_m73_demo.ngc",
        "local/lcnc_nc_files_3D_Chips.ngc",
        "local/lcnc_nc_files_cone.ngc",
        "local/lcnc_nc_files_daisy.ngc",
        "local/lcnc_nc_files_factorial.ngc",
        "local/lcnc_nc_files_flowsnake.ngc",
        "local/lcnc_nc_files_g20sub.ngc",
        "local/lcnc_nc_files_g76.ngc",
        "local/lcnc_nc_files_g881.ngc",
        "local/lcnc_nc_files_g881min.ngc",
        "local/lcnc_nc_files_g882.ngc",
        "local/lcnc_nc_files_gridprobe.ngc",
        "local/lcnc_nc_files_increment.ngc",
        "local/lcnc_nc_files_ngcgui_lib_arc1.ngc",
        "local/lcnc_nc_files_ngcgui_lib_arc2.ngc",
        "local/lcnc_nc_files_ngcgui_lib_backlash.ngc",
        "local/lcnc_nc_files_ngcgui_lib_db25.ngc",
        "local/lcnc_nc_files_ngcgui_lib_gosper.ngc",
        "local/lcnc_nc_files_ngcgui_lib_lathe_g76base.ngc",
        "local/lcnc_nc_files_ngcgui_lib_lathe_g76diam.ngc",
        "local/lcnc_nc_files_probe_basic_probe_macros_probe_back_edge_angle.ngc",
        "local/lcnc_nc_files_probe_basic_probe_macros_probe_back_left_inside.ngc",
        "local/lcnc_nc_files_probe_basic_probe_macros_probe_back_left_outside.ngc",
        "local/lcnc_nc_files_remap-subroutines_change.ngc",
        "local/lcnc_nc_files_remap-subroutines_on_abort.ngc",
        "local/lcnc_polar_at_angle.ngc",
        "local/lcnc_tests_interp_good_good-arc.big.imperial.center-format.ngc",
        "local/lcnc_tests_interp_good_good-arc.big.metric.center-format.ngc",
    ];
    let mut ok = 0;
    for f in files {
        let src = corpus(f);
        let p0 = match parser_linuxcnc::parse_program(&src) {
            Ok(p) => p,
            Err(e) => panic!("{f}: source must parse (ценз 12.1): {:?}", e.len()),
        };
        let out = emit(&p0, TargetDialect::LinuxCnc);
        assert!(
            out.ok(),
            "{f}: same-dialect emit must not refuse: {:?}",
            out.errors.first()
        );
        match parser_linuxcnc::parse_program(&out.text) {
            Ok(p2) => {
                let sem = ast_semantic_eq(&p0, &p2);
                let byte_det = emit(&p2, TargetDialect::LinuxCnc).text == out.text;
                assert!(sem, "{f}: semantic_diff != 0");
                assert!(byte_det, "{f}: emission not deterministic");
                ok += 1;
            }
            Err(e) => panic!("{f}: re-parse failed: {e:?}"),
        }
    }
    println!("LINUXCNC same-dialect: ok={ok} (34 expected)");
    assert_eq!(ok, 34, "all 34 linuxcnc corpus files must round-trip");
}
