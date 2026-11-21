//! Верификация v1-файлов #08/#47 (итерация 3-4: регенерация emit(golden),
//! read-only — файлы в corpus/tier_a/v1/, приёмка — оркестратор).
use eid_core::adapter::{Adapter, EidTextAdapter};
use eid_harness::diff::{semantic_diff, DiffConfig};
use eid_linuxcnc::LinuxCncAdapter;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tier_a")
}

#[test]
fn v1_08_full_circle_diff_zero() {
    let v1 =
        std::fs::read_to_string(root().join("v1/08_arc_full_circle/arc_full_circle.linuxcnc.ngc"))
            .expect("v1 #08 существует");
    // Эмиттер full-circle: дуга БЕЗ end-слов (`G2 I.. J..`, COVERAGE §5).
    let arc = v1.lines().find(|l| l.starts_with("G2 ")).expect("G2 line");
    assert_eq!(
        arc, "G2 I-5.0000 J0.0000 F120.0000",
        "full-circle без end-слов:\n{arc}"
    );
    let golden = EidTextAdapter::new()
        .parse(
            &std::fs::read_to_string(root().join("v0/08_arc_full_circle/golden.eid")).unwrap(),
            "g.eid",
        )
        .unwrap();
    let p = LinuxCncAdapter::new()
        .parse(&v1, "v1.ngc")
        .expect("parse v1");
    let d = semantic_diff(&golden, &p, &DiffConfig::default());
    assert!(d.is_empty(), "#08 v1: divergences: {d:?}");
}

#[test]
fn v1_47_raw_r1_text_payload() {
    // R1-text-класс: payload v1-файла байт-в-байт равен payload golden
    // (24240D0A = `$$\r\n` — D9 v0.15: терминатор в payload; эмиссия не
    // нормализует line endings).
    let v1 = std::fs::read_to_string(
        root().join("v1/47_raw_crlf_fidelity/raw_crlf_fidelity.linuxcnc.ngc"),
    )
    .expect("v1 #47 существует");
    assert!(
        v1.contains("$$\r\n"),
        "CRLF-терминатор декодирован дословно"
    );
    let golden_text =
        std::fs::read_to_string(root().join("v0/47_raw_crlf_fidelity/golden.eid")).unwrap();
    let golden_payload = golden_text
        .lines()
        .find(|l| l.contains("payload="))
        .and_then(|l| l.split("payload=").nth(1))
        .expect("golden payload")
        .to_string();
    let p = LinuxCncAdapter::new()
        .parse(&v1, "v1.ngc")
        .expect("parse v1");
    let v1_payload = match &p.ops[0] {
        eid_core::ast::Op::Raw {
            payload_hex, code, ..
        } => {
            assert_eq!(*code, eid_core::diag::ErrorCode::UnsupportedSyntax);
            payload_hex.clone()
        }
        other => panic!("op0: {other:?}"),
    };
    assert_eq!(v1_payload, golden_payload, "R1-text: payload байт-в-байт");
    // AST-level: дифф против golden тоже нулевой (raw-оп + ходы).
    let golden = EidTextAdapter::new().parse(&golden_text, "g.eid").unwrap();
    let d = semantic_diff(&golden, &p, &DiffConfig::default());
    assert!(d.is_empty(), "#47 v1: divergences: {d:?}");
}
