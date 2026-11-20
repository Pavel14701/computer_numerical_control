//! Интеграционные тесты translate_checked (контрольная точка 2–3, задача 1):
//! правило RAW-отказа на реальных адаптерах (eid-grbl, eid-linuxcnc) и
//! корпусных файлах: N3 (два направления кросс + same-dialect verbatim),
//! #47 golden (любая цель отказывает), пилоты без RAW → Ok.

use eid_core::adapter::{Adapter, EidTextAdapter};
use eid_core::ast::Op;
use eid_core::diag::ErrorCode;
use eid_grbl::GrblAdapter;
use eid_harness::translate::{translate_checked, translate_checked_golden};
use eid_linuxcnc::LinuxCncAdapter;
use std::path::{Path, PathBuf};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(corpus().join(rel)).expect("corpus file readable")
}

fn codes(err: &[ErrorCode]) -> Vec<ErrorCode> {
    err.to_vec()
}

#[test]
fn n3_grbl_src_to_linuxcnc_refuses_with_all_spans() {
    let text = read("tier_a/v0/n3_raw_vendor_block/raw_vendor_block.grbl.ngc");
    let err = translate_checked(&GrblAdapter::new(), &LinuxCncAdapter::new(), &text, "n3.grbl")
        .expect_err("cross-dialect with RAW must refuse");
    assert_eq!(codes(&err.iter().map(|d| d.code).collect::<Vec<_>>()), [ErrorCode::RawUntranslatable]);
    assert_eq!(err.len(), 1, "ровно одна диагностика: {err:?}");
    assert!(
        !err[0].options.is_empty(),
        "все Raw-спаны перечислены в options: {err:?}"
    );
    assert!(err[0].message.contains("RAW op(s)"), "{err:?}");
}

#[test]
fn n3_linuxcnc_src_to_grbl_refuses() {
    let text = read("tier_a/v0/n3_raw_vendor_block/raw_vendor_block.linuxcnc.ngc");
    let err =
        translate_checked(&LinuxCncAdapter::new(), &GrblAdapter::new(), &text, "n3.linuxcnc")
            .expect_err("cross-dialect with RAW must refuse");
    assert_eq!(err[0].code, ErrorCode::RawUntranslatable);
    assert_eq!(err.len(), 1);
}

#[test]
fn n3_same_dialect_is_verbatim_emit() {
    let text = read("tier_a/v0/n3_raw_vendor_block/raw_vendor_block.grbl.ngc");
    let (p, emitted) = translate_checked(&GrblAdapter::new(), &GrblAdapter::new(), &text, "n3.grbl")
        .expect("same-dialect RAW emission is legal (SPEC §8.1, v0.5.3)");
    assert!(p.ops.iter().any(|o| matches!(o, Op::Raw { .. })), "RAW-оп сохранён");
    assert!(
        emitted.contains("$110=250.000"),
        "verbatim RAW-строка в эмиссии: {emitted:?}"
    );
}

#[test]
fn golden_47_refuses_both_targets() {
    let eid = EidTextAdapter::new();
    let golden = eid.parse(&read("tier_a/v0/47_raw_crlf_fidelity/golden.eid"), "golden47").unwrap();
    for to in [&GrblAdapter::new() as &dyn Adapter, &LinuxCncAdapter::new() as &dyn Adapter] {
        let err = translate_checked_golden(&golden, to)
            .expect_err("golden RAW отказывает любой цели (диалектно-нейтральный источник)");
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].code, ErrorCode::RawUntranslatable);
        assert_eq!(err[0].options.len(), 1, "спан Raw-опа перечислен: {err:?}");
    }
}

#[test]
fn pilots_without_raw_translate_to_both_targets() {
    let eid = EidTextAdapter::new();
    for name in ["01_linear_box", "03_feed_survives_rapid"] {
        let golden = eid.parse(&read(&format!("tier_a/v0/{name}/golden.eid")), name).unwrap();
        for to in [&GrblAdapter::new() as &dyn Adapter, &LinuxCncAdapter::new() as &dyn Adapter] {
            let (_, text) = translate_checked_golden(&golden, to)
                .unwrap_or_else(|e| panic!("{name} -> {}: {e:?}", to.dialect_name()));
            assert!(!text.is_empty());
        }
    }
    // Кросс-пара файлов пилота без RAW: grbl-файл → linuxcnc → Ok.
    let grbl_text = read("tier_a/v0/01_linear_box/linear_box.grbl.ngc");
    let (_, out) = translate_checked(&GrblAdapter::new(), &LinuxCncAdapter::new(), &grbl_text, "01.grbl")
        .expect("пилот без RAW: кросс-трансляция легальна");
    assert!(!out.is_empty());
}
