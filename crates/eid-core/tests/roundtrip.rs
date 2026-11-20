//! Итерация 1 — Агент A (EID Core): round-trip на пилотных golden + негативные
//! и позитивные unit-тесты парсера/эмиттера/валидации.
//! Итерация 4 — verify-pass (MLIR-паттерн): validate на любом этапе пайплайна;
//! Diagnostic.stage (interface v0.1.2).
//!
//! Критерии: parse(golden) → Ok; emit(parsed) == golden байт-в-байт (golden
//! написаны по grammar.ebnf v0.3); parse(emit(parsed)) ≡ первый AST (сравнение
//! на f64-зеркале, EQUIVALENCE §3/§4); emit(parsed) дважды — байт-в-байт;
//! LF/CRLF не важен для parse; validate(golden) = пусто (все 48).

use eid_core::adapter::{Adapter, EidTextAdapter};
use eid_core::ast::*;
use eid_core::diag::{Diagnostic, ErrorCode, Stage};
use eid_core::emitter;
use eid_core::parser::{self, NoticeKind};
use eid_core::verify;
use std::fs;
use std::path::{Path, PathBuf};

// ===========================================================================
// Инфраструктура
// ===========================================================================

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tier_a/v0")
}

fn golden_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(corpus_dir()).expect("corpus/tier_a/v0 must exist") {
        let entry = entry.expect("read_dir entry");
        let g = entry.path().join("golden.eid");
        if g.is_file() {
            out.push(g);
        }
    }
    out.sort();
    assert!(
        !out.is_empty(),
        "no golden.eid found under {}",
        corpus_dir().display()
    );
    out
}

// --- AST-сравнение на f64-зеркале (EQUIVALENCE §3: значения, не текст) ---

fn nq(a: &Num, b: &Num) -> bool {
    a.value == b.value
}

fn pq(a: &Point3, b: &Point3) -> bool {
    nq(&a.x, &b.x) && nq(&a.y, &b.y) && nq(&a.z, &b.z)
}

fn spq(a: &StartPoint, b: &StartPoint) -> bool {
    match (a, b) {
        (StartPoint::Unknown, StartPoint::Unknown) => true,
        (StartPoint::Known(x), StartPoint::Known(y)) => pq(x, y),
        _ => false,
    }
}

fn spindleq(a: &Spindle, b: &Spindle) -> bool {
    match (a, b) {
        (Spindle::Off, Spindle::Off) => true,
        (Spindle::Cw(x), Spindle::Cw(y)) | (Spindle::Ccw(x), Spindle::Ccw(y)) => nq(x, y),
        _ => false,
    }
}

fn stateq(a: &State, b: &State) -> bool {
    let feed = match (&a.feed, &b.feed) {
        (None, None) => true,
        (Some(x), Some(y)) => nq(x, y),
        _ => false,
    };
    feed
        && spindleq(&a.spindle, &b.spindle)
        && a.coolant == b.coolant
        && a.tool == b.tool
        && a.frame == b.frame
        && a.wcs == b.wcs
}

fn opq(a: &Op, b: &Op) -> bool {
    match (a, b) {
        (Op::Rapid { .. }, Op::Rapid { .. })
        | (Op::Linear { .. }, Op::Linear { .. }) => {
            let (sa, ea, ta, ra, ua) = motion_fields(a);
            let (sb, eb, tb, rb, ub) = motion_fields(b);
            spq(sa, sb) && pq(ea, eb) && stateq(ta, tb) && ra == rb && ua == ub
        }
        (Op::Arc { .. }, Op::Arc { .. }) => {
            let (sa, ea, ca, na, da, wa, pa, ta, ra, ua) = arc_fields(a);
            let (sb, eb, cb, nb, db, wb, pb, tb, rb, ub) = arc_fields(b);
            spq(sa, sb)
                && pq(ea, eb)
                && pq(ca, cb)
                && pq(na, nb)
                && da == db
                && nq(wa, wb)
                && nq(pa, pb)
                && stateq(ta, tb)
                && ra == rb
                && ua == ub
        }
        (Op::Dwell { .. }, Op::Dwell { .. }) => {
            let (sa, ta, ra, ua) = dwell_fields(a);
            let (sb, tb, rb, ub) = dwell_fields(b);
            nq(sa, sb) && stateq(ta, tb) && ra == rb && ua == ub
        }
        (Op::ToolChange { tool: a, state: ta, src: ra }, Op::ToolChange { tool: b, state: tb, src: rb }) => {
            a == b && stateq(ta, tb) && ra == rb
        }
        (Op::ProgramStop { optional: a, state: ta, src: ra }, Op::ProgramStop { optional: b, state: tb, src: rb }) => {
            a == b && stateq(ta, tb) && ra == rb
        }
        (Op::ProgramEnd { src: a }, Op::ProgramEnd { src: b }) => a == b,
        (Op::WcsDefine { p: a, origin: oa, state: ta, src: ra }, Op::WcsDefine { p: b, origin: ob, state: tb, src: rb }) => {
            a == b && pq(oa, ob) && stateq(ta, tb) && ra == rb
        }
        (Op::WcsSetHere { p: a, delta: oa, state: ta, src: ra }, Op::WcsSetHere { p: b, delta: ob, state: tb, src: rb }) => {
            a == b && pq(oa, ob) && stateq(ta, tb) && ra == rb
        }
        (Op::Raw { payload_hex: a, reason: ra, code: ca, src: sa }, Op::Raw { payload_hex: b, reason: rb, code: cb, src: sb }) => {
            a == b && ra == rb && ca == cb && sa == sb
        }
        _ => false,
    }
}

#[allow(clippy::type_complexity)]
fn motion_fields(
    op: &Op,
) -> (&StartPoint, &Point3, &State, &SrcSpan, &Option<u32>) {
    match op {
        Op::Rapid { start, end, state, src, sub } | Op::Linear { start, end, state, src, sub } => {
            (start, end, state, src, sub)
        }
        _ => unreachable!(),
    }
}

#[allow(clippy::type_complexity)]
fn arc_fields(
    op: &Op,
) -> (
    &StartPoint,
    &Point3,
    &Point3,
    &Point3,
    &Dir,
    &Num,
    &Num,
    &State,
    &SrcSpan,
    &Option<u32>,
) {
    match op {
        Op::Arc { start, end, center, normal, dir, sweep_deg, pitch, state, src, sub } => {
            (start, end, center, normal, dir, sweep_deg, pitch, state, src, sub)
        }
        _ => unreachable!(),
    }
}

fn dwell_fields(op: &Op) -> (&Num, &State, &SrcSpan, &Option<u32>) {
    match op {
        Op::Dwell { seconds, state, src, sub } => (seconds, state, src, sub),
        _ => unreachable!(),
    }
}

fn path_modeq(a: &PathMode, b: &PathMode) -> bool {
    match (a, b) {
        (PathMode::ExactPath, PathMode::ExactPath) => true,
        (PathMode::ExactStop, PathMode::ExactStop) => true,
        (PathMode::Blend { t: x }, PathMode::Blend { t: y }) => match (x, y) {
            (None, None) => true,
            (Some(p), Some(q)) => nq(p, q),
            _ => false,
        },
        _ => false,
    }
}

fn programq(a: &Program, b: &Program) -> bool {
    a.eid_version == b.eid_version
        && a.units == b.units
        && path_modeq(&a.path_mode, &b.path_mode)
        && match (&a.frame_preset, &b.frame_preset) {
            (None, None) => true,
            (Some(x), Some(y)) => pq(&x.values, &y.values) && x.wcs == y.wcs,
            _ => false,
        }
        && a.ops.len() == b.ops.len()
        && a.ops.iter().zip(b.ops.iter()).all(|(x, y)| opq(x, y))
}

// --- Шаблоны текстов -------------------------------------------------------

const HDR: &str = "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET none\n";

fn prog(ops: &str) -> String {
    format!("{HDR}{ops}END\n")
}

/// Состояние по умолчанию для тестов (wcs настраивается заменой "G54").
fn st(wcs: &str, frame: &str, feed: &str) -> String {
    format!("feed={feed} spindle=off coolant=none tool=none frame={frame} wcs={wcs}")
}

fn expect_err(text: &str, want: Option<ErrorCode>, ctx: &str) -> Vec<eid_core::diag::Diagnostic> {
    match parser::parse(text, "test") {
        Ok(p) => panic!(
            "{ctx}: expected parse failure, got Ok ({:#?})\n--- text ---\n{text}",
            p.ops
        ),
        Err(ds) => {
            assert!(!ds.is_empty(), "{ctx}: empty diagnostics");
            if let Some(code) = want {
                assert!(
                    ds.iter().all(|d| d.code == code),
                    "{ctx}: expected all diagnostics {code:?}, got {:#?}",
                    ds.iter().map(|d| d.code).collect::<Vec<_>>()
                );
            }
            ds
        }
    }
}

// ===========================================================================
// R1 round-trip на пилотных golden (чтение директории в рантайме)
// ===========================================================================

#[test]
fn golden_roundtrip_all() {
    let files = golden_files();
    let mut failures: Vec<String> = Vec::new();
    for f in &files {
        let text = fs::read_to_string(f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let name = f.display().to_string();

        // parse(golden) → Ok; в пилоте notices нет (zero-length — вектор #43)
        let outcome = match parser::parse_with_notices(&text, &name) {
            Ok(o) => o,
            Err(ds) => {
                failures.push(format!(
                    "{name}: parse failed: {}",
                    ds.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("; ")
                ));
                continue;
            }
        };
        if !outcome.notices.is_empty() {
            failures.push(format!(
                "{name}: unexpected notices: {}",
                outcome.notices.iter().map(|n| n.message.clone()).collect::<Vec<_>>().join("; ")
            ));
            continue;
        }
        let p1 = outcome.program;

        // emit(parsed) == golden байт-в-байт
        let t1 = match emitter::emit(&p1) {
            Ok(t) => t,
            Err(ds) => {
                failures.push(format!(
                    "{name}: emit failed: {}",
                    ds.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("; ")
                ));
                continue;
            }
        };
        if t1 != text {
            failures.push(format!("{name}: emit(parsed) != golden text (byte-exact)"));
            continue;
        }

        // emit(parsed) дважды — байт-в-байт
        let t1b = emitter::emit(&p1).unwrap();
        assert_eq!(t1, t1b, "{name}: emit not deterministic");

        // parse(emit(parsed)) → AST равен первому (на f64-зеркале)
        let p2 = parser::parse(&t1, &name)
            .unwrap_or_else(|ds| panic!("{name}: re-parse failed: {ds:#?}"));
        assert!(
            programq(&p1, &p2),
            "{name}: AST mismatch after parse→emit→parse\nfirst: {p1:#?}\nsecond: {p2:#?}"
        );
        let t2 = emitter::emit(&p2).unwrap();
        assert_eq!(t1, t2, "{name}: second emit differs");
    }
    assert!(failures.is_empty(), "golden round-trip failures ({}):\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn adapter_is_r1_machine() {
    let adapter = EidTextAdapter::new();
    assert_eq!(adapter.dialect_name(), "eid-l1");
    let f = &golden_files()[0];
    let text = fs::read_to_string(f).unwrap();
    let p = adapter.parse(&text, "golden").expect("adapter parse");
    let out = emitter::emit(&p).expect("adapter emit");
    assert_eq!(out, text);
    assert!(adapter.take_notices().is_empty());
    // Повторный вызов после опустошения — пусто
    assert!(adapter.take_notices().is_empty());
}

// ===========================================================================
// Негативные unit-тесты (задание итерации 1 + ближайшие края)
// ===========================================================================

#[test]
fn neg_bad_magic() {
    for bad in [
        "EID/L1 v0.1\n", // прежние версии формата не валидны:
        "EID/L1 v0.2\n", // изменение формата = новая magic (SPEC §10.4)
        "EID/L1 v0.4\n",
        "EID/L1\n",
        "EID/L1  v0.3\n",
        "eid/L1 v0.3\n",
        "EID/L2 v0.3\n",
        "",
    ] {
        let text = format!("{bad}UNITS mm\nPATH_MODE exact_path\nFRAME_PRESET none\nOP 001 PEND\nEND\n");
        expect_err(&text, Some(ErrorCode::InvalidInput), "bad magic");
    }
}

#[test]
fn neg_missing_state_field() {
    // пропущено tool= (D3: полный снапшот, каждое поле ровно один раз)
    let text = prog(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none frame=wcs wcs=G54\nOP 002 PEND\n"
    ));
    expect_err(&text, Some(ErrorCode::InvalidInput), "missing state field");
    // лишнее поле state (wcs дважды)
    let text2 = prog(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {} wcs=G54\nOP 002 PEND\n",
        st("G54", "wcs", "none")
    ));
    expect_err(&text2, Some(ErrorCode::InvalidInput), "extra state field");
    // нарушен порядок полей state (D3)
    let text3 = prog(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 spindle=off feed=none coolant=none tool=none frame=wcs wcs=G54\nOP 002 PEND\n"
    ));
    expect_err(&text3, Some(ErrorCode::InvalidInput), "state field order");
}

#[test]
fn neg_number_3dp() {
    let text = prog(&format!(
        "OP 001 RAPID start=unknown end=1.000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 PEND\n",
        st("G54", "wcs", "none")
    ));
    expect_err(&text, Some(ErrorCode::InvalidInput), "3 dp number");
    for bad in ["1.00000", "+1.0000", "1e2", ".5000", "1.", "-.5000", "1"] {
        let text = prog(&format!(
            "OP 001 RAPID start=unknown end={bad},0.0000,0.0000 src=0.1-0.0 {}\nOP 002 PEND\n",
            st("G54", "wcs", "none")
        ));
        expect_err(&text, Some(ErrorCode::InvalidInput), "bad number lexeme");
    }
}

#[test]
fn neg_pend_not_last() {
    let text = prog(&format!(
        "OP 001 PEND\nOP 002 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\n",
        st("G54", "wcs", "none")
    ));
    expect_err(&text, Some(ErrorCode::InvalidInput), "PEND not last");
}

#[test]
fn neg_duplicate_program_end() {
    let text = prog(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 PEND\nOP 003 PEND\n",
        st("G54", "wcs", "none")
    ));
    expect_err(&text, Some(ErrorCode::InvalidInput), "duplicate PEND");
    // и программа вовсе без PEND
    let text2 = prog(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\n",
        st("G54", "wcs", "none")
    ));
    expect_err(&text2, Some(ErrorCode::InvalidInput), "no PEND");
    // и пустая программа (только заголовок + END)
    expect_err(&prog(""), Some(ErrorCode::InvalidInput), "empty program");
}

#[test]
fn neg_arc_unknown_start() {
    let text = prog(&format!(
        "OP 001 ARC start=unknown end=-5.0000,5.0000,2.0000 center=0.0000,5.0000,0.0000 normal=0.0000,0.0000,1.0000 dir=cw sweep=90.0000 pitch=2.0000 src=0.1-0.0 {}\nOP 002 PEND\n",
        st("G54", "wcs", "100.0000")
    ));
    expect_err(&text, Some(ErrorCode::InvalidInput), "ARC start=unknown");
}

#[test]
fn neg_ops_after_end_and_structure() {
    // контент после END
    let text = format!("{}OP 001 PEND\nEND\nOP 002 PEND\n", HDR);
    expect_err(&text, Some(ErrorCode::InvalidInput), "content after END");
    // пустая строка внутри
    let text2 = format!("{HDR}\nOP 001 PEND\nEND\n");
    expect_err(&text2, Some(ErrorCode::InvalidInput), "blank line");
    // отсутствует финальный NL
    let text3 = format!("{}OP 001 PEND\nEND", HDR);
    expect_err(&text3, Some(ErrorCode::InvalidInput), "missing final NL");
    // неизвестный оп-ключ
    let text4 = prog("OP 001 JUMP src=0.1-0.0\n");
    expect_err(&text4, Some(ErrorCode::InvalidInput), "unknown op keyword");
    // номер опа не 3 цифры
    let text5 = prog(&format!("OP 1 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 PEND\n", st("G54", "wcs", "none")));
    expect_err(&text5, Some(ErrorCode::InvalidInput), "op number not 3 digits");
}

#[test]
fn crlf_accepted() {
    // LF vs CRLF не важен для parse; AST совпадает с LF-вариантом
    let f = &golden_files()[0];
    let lf = fs::read_to_string(f).unwrap();
    let crlf = lf.replace('\n', "\r\n");
    let a = parser::parse(&lf, "x").expect("lf parse");
    let b = parser::parse(&crlf, "x").expect("crlf parse");
    assert!(programq(&a, &b), "CRLF parse AST differs from LF");
    // CRLF-файл эмитится в канонический LF-текст байт-в-байт
    assert_eq!(emitter::emit(&b).unwrap(), lf);
}

// ===========================================================================
// SPEC §3: zero-length → drop+notice; feed; D4–D8
// ===========================================================================

#[test]
fn zero_length_rapid_dropped_with_notice() {
    let text = prog(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 RAPID start=1.0000,0.0000,0.0000 end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 003 RAPID start=1.0000,0.0000,0.0000 end=2.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 004 PEND\n",
        st("G54", "wcs", "none"), st("G54", "wcs", "none"), st("G54", "wcs", "none")
    ));
    let out = parser::parse_with_notices(&text, "t").expect("parse ok");
    assert_eq!(out.program.ops.len(), 3, "zero-length op removed");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(out.notices[0].kind, NoticeKind::ZeroLengthMotion);
    assert_eq!(out.notices[0].op_index, 1);
    // R1-AST: parse(emit(parse(x))) ≡ parse(x) — отбрасывание стабильно
    let t1 = emitter::emit(&out.program).unwrap();
    let out2 = parser::parse_with_notices(&t1, "t").expect("re-parse");
    assert!(out2.notices.is_empty());
    assert!(programq(&out.program, &out2.program));
    // emit отклоняет невалидный AST с zero-length (путь эмиссии)
    let bad = prog(&format!(
        "OP 001 RAPID start=1.0000,0.0000,0.0000 end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 PEND\n",
        st("G54", "wcs", "none")
    ));
    let raw = parser::parse_raw(&bad, "t").expect("raw parse (без канонизации)");
    let ds = emitter::emit(&raw).unwrap_err();
    assert!(ds.iter().all(|d| d.code == ErrorCode::InvalidInput), "emit must reject zero-length op: {ds:?}");
}

#[test]
fn zero_length_linear_feed_none_dropped_not_error() {
    // отбрасывание раньше проверки feed (SPEC §3: «в итоговом AST»)
    let text = prog(&format!(
        "OP 001 LINEAR start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 LINEAR start=1.0000,0.0000,0.0000 end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 003 PEND\n",
        st("G54", "wcs", "500.0000"), st("G54", "wcs", "none")
    ));
    let out = parser::parse_with_notices(&text, "t").expect("zero-length linear with feed=none is dropped");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(out.notices[0].kind, NoticeKind::ZeroLengthMotion);
}

#[test]
fn linear_feed_none_rejected() {
    let text = prog(&format!(
        "OP 001 LINEAR start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 PEND\n",
        st("G54", "wcs", "none")
    ));
    expect_err(&text, Some(ErrorCode::InvalidInput), "linear feed=none");
}

#[test]
fn arc_sweep_zero_dropped_not_error() {
    // CORPUS #13: sweep=0 (нулевой ход) → drop+notice, не ошибка
    let text = prog(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 ARC start=1.0000,0.0000,0.0000 end=1.0000,0.0000,0.0000 center=2.0000,0.0000,0.0000 normal=0.0000,0.0000,1.0000 dir=cw sweep=0.0000 pitch=0.0000 src=0.1-0.0 {}\nOP 003 PEND\n",
        st("G54", "wcs", "none"), st("G54", "wcs", "100.0000")
    ));
    let out = parser::parse_with_notices(&text, "t").expect("sweep=0 with start==end is dropped");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(out.notices[0].kind, NoticeKind::ZeroLengthArc);
    // sweep=0 при start != end — невозможная дуга → ошибка
    let bad = prog(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 ARC start=1.0000,0.0000,0.0000 end=3.0000,0.0000,0.0000 center=2.0000,0.0000,0.0000 normal=0.0000,0.0000,1.0000 dir=cw sweep=0.0000 pitch=0.0000 src=0.1-0.0 {}\nOP 003 PEND\n",
        st("G54", "wcs", "none"), st("G54", "wcs", "100.0000")
    ));
    expect_err(&bad, Some(ErrorCode::InvalidInput), "sweep=0 with start!=end");
}

#[test]
fn arc_domains_and_geometry() {
    let base = |end: &str, sweep: &str, normal: &str| {
        prog(&format!(
            "OP 001 RAPID start=unknown end=0.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 ARC start=0.0000,0.0000,0.0000 end={end} center=0.0000,5.0000,0.0000 normal={normal} dir=cw sweep={sweep} pitch=2.0000 src=0.1-0.0 {}\nOP 003 PEND\n",
            st("G54", "wcs", "none"), st("G54", "wcs", "100.0000")
        ))
    };
    // валидная дуга (golden #46-образная)
    assert!(parser::parse(&base("-5.0000,5.0000,2.0000", "90.0000", "0.0000,0.0000,1.0000"), "t").is_ok());
    // sweep=400 → D7
    expect_err(&base("-5.0000,5.0000,2.0000", "400.0000", "0.0000,0.0000,1.0000"), Some(ErrorCode::InvalidInput), "sweep=400");
    // |normal| != 1 → D7
    expect_err(&base("-5.0000,5.0000,2.0000", "90.0000", "0.0000,0.0000,2.0000"), Some(ErrorCode::InvalidInput), "normal len 2");
    // точка end не согласована с sweep (уравнение §6.3)
    expect_err(&base("1.0000,5.0000,2.0000", "90.0000", "0.0000,0.0000,1.0000"), Some(ErrorCode::InvalidInput), "endpoint mismatch");
    // радиус несогласован (§6.1)
    expect_err(&base("-6.0000,5.0000,2.0000", "90.0000", "0.0000,0.0000,1.0000"), Some(ErrorCode::InvalidInput), "radius mismatch");
    // хорда·normal != pitch (§6.2)
    expect_err(&base("-5.0000,5.0000,3.0000", "90.0000", "0.0000,0.0000,1.0000"), Some(ErrorCode::InvalidInput), "pitch mismatch");
}

#[test]
fn dwell_zero_dropped_negative_rejected() {
    let ok = prog(&format!(
        "OP 001 DWELL seconds=0.0000 src=0.1-0.0 {}\nOP 002 PEND\n",
        st("G54", "wcs", "none")
    ));
    let out = parser::parse_with_notices(&ok, "t").expect("zero dwell dropped");
    assert_eq!(out.notices.len(), 1);
    assert_eq!(out.notices[0].kind, NoticeKind::ZeroDwell);
    assert_eq!(out.program.ops.len(), 1);

    let neg = prog(&format!(
        "OP 001 DWELL seconds=-1.0000 src=0.1-0.0 {}\nOP 002 PEND\n",
        st("G54", "wcs", "none")
    ));
    expect_err(&neg, Some(ErrorCode::InvalidInput), "negative dwell");
}

#[test]
fn d4_tool_change_mismatch() {
    let text = prog(&format!(
        "OP 001 TOOL_CHANGE tool=3 src=0.1-0.0 {}\nOP 002 PEND\n",
        st("G54", "wcs", "none") // state tool=none != op tool=3
    ));
    expect_err(&text, Some(ErrorCode::InvalidInput), "D4 mismatch");
    // согласованный вариант валиден
    let ok = prog(&format!(
        "OP 001 TOOL_CHANGE tool=3 src=0.1-0.0 feed=none spindle=off coolant=none tool=3 frame=wcs wcs=G54\nOP 002 PEND\n"
    ));
    assert!(parser::parse(&ok, "t").is_ok());
}

#[test]
fn d5_set_here_after_motion() {
    let text = prog(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 WCS_SET_HERE p=2 delta=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 003 PEND\n",
        st("G54", "wcs", "none"), st("G55", "wcs", "none")
    ));
    expect_err(&text, Some(ErrorCode::InvalidInput), "D5 set_here after motion");
}

#[test]
fn d8_sub_rules() {
    // sub= на RAPID — грамматичен
    let ok = prog(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 sub=2 {}\nOP 002 PEND\n",
        st("G54", "wcs", "none")
    ));
    assert!(parser::parse(&ok, "t").is_ok());
    // sub= на STOP — вне грамматики (D8)
    let bad = prog(&format!(
        "OP 001 STOP optional=no src=0.1-0.0 sub=1 {}\nOP 002 PEND\n",
        st("G54", "wcs", "none")
    ));
    expect_err(&bad, Some(ErrorCode::InvalidInput), "sub on STOP");
    // sub= на TOOL_CHANGE — вне грамматики
    let bad2 = prog(&format!(
        "OP 001 TOOL_CHANGE tool=3 src=0.1-0.0 sub=1 feed=none spindle=off coolant=none tool=3 frame=wcs wcs=G54\nOP 002 PEND\n"
    ));
    expect_err(&bad2, Some(ErrorCode::InvalidInput), "sub on TOOL_CHANGE");
}

#[test]
fn d9_raw_payload() {
    let ok = prog("OP 001 RAW reason=\"vendor macro\" code=E_UNSUPPORTED_SYNTAX src=47.1-49.0 payload=0D0A25\nOP 002 PEND\n");
    let p = parser::parse(&ok, "t").expect("raw parse");
    // байт-в-байт same-dialect round-trip (R1-text)
    assert_eq!(emitter::emit(&p).unwrap(), ok);
    // пустой payload грамматичен
    let empty = prog("OP 001 RAW reason=\"\" code=E_LIMIT_EXCEEDED src=0.1-0.0 payload=\nOP 002 PEND\n");
    assert!(parser::parse(&empty, "t").is_ok());
    // нечётная длина — D9
    let odd = prog("OP 001 RAW reason=\"x\" code=E_INVALID_INPUT src=0.1-0.0 payload=0D0A1\nOP 002 PEND\n");
    expect_err(&odd, Some(ErrorCode::InvalidInput), "odd hex length");
    // lowercase вне грамматики
    let lower = prog("OP 001 RAW reason=\"x\" code=E_INVALID_INPUT src=0.1-0.0 payload=0d0a\nOP 002 PEND\n");
    expect_err(&lower, Some(ErrorCode::InvalidInput), "lowercase hex");
    // недиагностический код
    let badcode = prog("OP 001 RAW reason=\"x\" code=E_NOPE src=0.1-0.0 payload=00\nOP 002 PEND\n");
    expect_err(&badcode, Some(ErrorCode::InvalidInput), "bad diag code");
}

#[test]
fn path_mode_blend_t() {
    // grammar v0.2 (A1): blend [ SP "t=" number ]; bare blend легален (G64 без P).
    // bare blend → t: None
    let bare = "EID/L1 v0.3\nUNITS mm\nPATH_MODE blend\nFRAME_PRESET none\nOP 001 PEND\nEND\n";
    let p = parser::parse(bare, "t").expect("bare blend");
    assert!(matches!(p.path_mode, PathMode::Blend { t: None }));
    // blend t= → t: Some (число по D1)
    let with_t = "EID/L1 v0.3\nUNITS mm\nPATH_MODE blend t=0.0100\nFRAME_PRESET none\nOP 001 PEND\nEND\n";
    let p = parser::parse(with_t, "t").expect("blend t=");
    match &p.path_mode {
        PathMode::Blend { t: Some(n) } => assert_eq!(n.value, 0.01),
        other => panic!("expected Blend{{Some}}, got {other:?}"),
    }
    // байт-в-байт round-trip + AST-стабильность
    let t1 = emitter::emit(&p).unwrap();
    assert_eq!(t1, with_t, "emit(blend t=) byte-exact");
    let p2 = parser::parse(&t1, "t").unwrap();
    assert!(programq(&p, &p2));
    // отрицательный t грамматичен (число по D1 допускает знак)
    let neg = "EID/L1 v0.3\nUNITS mm\nPATH_MODE blend t=-0.5000\nFRAME_PRESET none\nOP 001 PEND\nEND\n";
    assert!(parser::parse(neg, "t").is_ok());
    // все три значения PATH_MODE парсятся
    for pm in ["exact_path", "exact_stop", "blend"] {
        let text = format!("EID/L1 v0.3\nUNITS mm\nPATH_MODE {pm}\nFRAME_PRESET none\nOP 001 PEND\nEND\n");
        assert!(parser::parse(&text, "t").is_ok(), "PATH_MODE {pm}");
    }
    // негативы лексики t= (D1 и структура)
    for bad in [
        "blend t=0.500",         // 3 dp
        "blend t=",              // пустое значение
        "blend t=1.0000 extra",  // мусор после числа
        "blendt=0.5000",         // нет SP
        "blend  t=0.5000",       // двойной SP
        "blend T=0.5000",        // регистр
    ] {
        let text = format!("EID/L1 v0.3\nUNITS mm\nPATH_MODE {bad}\nFRAME_PRESET none\nOP 001 PEND\nEND\n");
        expect_err(&text, Some(ErrorCode::InvalidInput), "bad blend t= lexeme");
    }
    // FRAME_PRESET-строка опциональна по grammar
    let text2 = "EID/L1 v0.3\nUNITS mm\nPATH_MODE blend\nOP 001 PEND\nEND\n";
    let p = parser::parse(text2, "t").expect("no frame_preset line");
    assert!(p.frame_preset.is_none());
}

// ===========================================================================
// Q11: точки разрыва и связывание (SPEC §2 принцип 2, v0.9)
// ===========================================================================

#[test]
fn unknown_outside_break_rejected() {
    // неизвестный старт вне точки разрыва (не первый motion, та же wcs)
    let text = prog(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 RAPID start=unknown end=2.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 003 PEND\n",
        st("G54", "wcs", "none"), st("G54", "wcs", "none")
    ));
    expect_err(&text, Some(ErrorCode::InvalidInput), "unknown outside break");
}

#[test]
fn known_at_uncomputable_rejected() {
    // смена wcs на систему без in-text wcs_define → start обязан быть unknown
    let text = prog(&format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 RAPID start=1.0000,0.0000,0.0000 end=2.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 003 PEND\n",
        st("G54", "wcs", "none"), st("G55", "wcs", "none")
    ));
    expect_err(&text, Some(ErrorCode::InvalidInput), "known at (b)-break");
}

#[test]
fn cross_wcs_linking_with_defines() {
    // обе системы определены: start = prev_end + O_prev − O_new — вычислим
    let ok = prog(&format!(
        "OP 001 WCS_DEFINE p=1 origin=10.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 WCS_DEFINE p=2 origin=3.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 003 RAPID start=unknown end=5.0000,0.0000,5.0000 src=0.1-0.0 {}\nOP 004 RAPID start=12.0000,0.0000,5.0000 end=7.0000,0.0000,5.0000 src=0.1-0.0 {}\nOP 005 PEND\n",
        st("G54", "wcs", "none"), st("G54", "wcs", "none"), st("G54", "wcs", "none"), st("G55", "wcs", "none")
    ));
    assert!(parser::parse(&ok, "t").is_ok(), "cross-wcs with in-text origins must link");
    // неверное значение start → ошибка
    let bad = ok.replace("start=12.0000,0.0000,5.0000", "start=5.0000,0.0000,5.0000");
    expect_err(&bad, Some(ErrorCode::InvalidInput), "cross-wcs wrong start");
}

#[test]
fn machine_frame_linking() {
    // machine-ходы выразимы при in-text origins и отсутствии пресеток (§5.3)
    let ok = prog(&format!(
        "OP 001 WCS_DEFINE p=1 origin=10.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 RAPID start=unknown end=5.0000,0.0000,5.0000 src=0.1-0.0 {}\nOP 003 RAPID start=15.0000,0.0000,5.0000 end=20.0000,0.0000,6.0000 src=0.1-0.0 {}\nOP 004 RAPID start=20.0000,0.0000,6.0000 end=25.0000,0.0000,6.0000 src=0.1-0.0 {}\nOP 005 RAPID start=15.0000,0.0000,6.0000 end=14.0000,0.0000,6.0000 src=0.1-0.0 {}\nOP 006 PEND\n",
        st("G54", "wcs", "none"),
        st("G54", "wcs", "none"),
        st("G54", "machine", "none"), // (c)-переход вычислим: M = reading + O
        st("G54", "machine", "none"), // machine→machine: цепочка
        st("G54", "wcs", "none"),     // (d)-переход вычислим: reading = M − O
    ));
    // (end последнего опа сделан не-нулевым, чтобы ход не был отброшен
    // как zero-length и переход machine→wcs реально проверился)
    assert!(parser::parse(&ok, "t").is_ok(), "machine-frame linking with in-text origins");

    // G53 без origins (вектор N1-образный) — известный start при невычислимой
    // позиции → ошибка
    let bad = prog(&format!(
        "OP 001 RAPID start=unknown end=5.0000,0.0000,5.0000 src=0.1-0.0 {}\nOP 002 RAPID start=15.0000,0.0000,5.0000 end=20.0000,0.0000,6.0000 src=0.1-0.0 {}\nOP 003 PEND\n",
        st("G54", "wcs", "none"), st("G54", "machine", "none")
    ));
    expect_err(&bad, Some(ErrorCode::InvalidInput), "G53 without in-text origins");

    // а unknown в точке разрыва (c) — валиден
    let ok2 = prog(&format!(
        "OP 001 RAPID start=unknown end=5.0000,0.0000,5.0000 src=0.1-0.0 {}\nOP 002 RAPID start=unknown end=20.0000,0.0000,6.0000 src=0.1-0.0 {}\nOP 003 PEND\n",
        st("G54", "wcs", "none"), st("G54", "machine", "none")
    ));
    assert!(parser::parse(&ok2, "t").is_ok(), "unknown at (c) break");
}

#[test]
fn preset_anchor_rules() {
    // G92-якорь в той же системе: start == preset.values (golden #36)
    let ok = prog(&format!(
        "OP 001 RAPID start=0.0000,0.0000,5.0000 end=0.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 PEND\n",
        st("G54", "wcs", "none")
    ));
    let text = format!(
        "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET G92 X0.0000 Y0.0000 Z5.0000 WCS G54\n{ops}",
        ops = ok.strip_prefix(HDR).unwrap()
    );
    assert!(parser::parse(&text, "t").is_ok());
    // известный старт, не равный пресетке → ошибка
    let bad = text.replace("start=0.0000,0.0000,5.0000", "start=1.0000,0.0000,5.0000");
    expect_err(&bad, Some(ErrorCode::InvalidInput), "start != preset");

    // пресетка G54, первый ход в G55 без define → (b): unknown обязан
    let both_undef = format!(
        "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET G92 X0.0000 Y0.0000 Z5.0000 WCS G54\nOP 001 RAPID start=unknown end=1.0000,0.0000,5.0000 src=0.1-0.0 {}\nOP 002 PEND\nEND\n",
        st("G55", "wcs", "none")
    );
    assert!(parser::parse(&both_undef, "t").is_ok(), "(b) after preset: unknown ok");
    let known = both_undef.replace("start=unknown", "start=0.0000,0.0000,5.0000");
    expect_err(&known, Some(ErrorCode::InvalidInput), "(b) after preset: known rejected");

    // пресетка G54, первый ход в G55, обе системы определены → вычислимо:
    // start = preset + O54 − O55 = (0,0,5) + (10,0,0) − (3,0,0) = (7,0,5)
    let defined = format!(
        "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET G92 X0.0000 Y0.0000 Z5.0000 WCS G54\nOP 001 WCS_DEFINE p=1 origin=10.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 WCS_DEFINE p=2 origin=3.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 003 RAPID start=7.0000,0.0000,5.0000 end=8.0000,0.0000,5.0000 src=0.1-0.0 {}\nOP 004 PEND\nEND\n",
        st("G54", "wcs", "none"), st("G54", "wcs", "none"), st("G55", "wcs", "none")
    );
    assert!(parser::parse(&defined, "t").is_ok(), "preset cross-wcs with both origins defined");
    let wrong = defined.replace("start=7.0000,0.0000,5.0000", "start=0.0000,0.0000,5.0000");
    expect_err(&wrong, Some(ErrorCode::InvalidInput), "preset cross-wcs wrong start");
}

#[test]
fn set_here_anchor_rules() {
    // L20-якорь: текущая позиция читается как delta
    let ok = prog(&format!(
        "OP 001 WCS_SET_HERE p=1 delta=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 RAPID start=1.0000,0.0000,0.0000 end=2.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 003 PEND\n",
        st("G54", "wcs", "none"), st("G54", "wcs", "none")
    ));
    assert!(parser::parse(&ok, "t").is_ok(), "set_here anchor");
    let bad = ok.replace("start=1.0000,0.0000,0.0000", "start=5.0000,0.0000,0.0000");
    expect_err(&bad, Some(ErrorCode::InvalidInput), "set_here anchor wrong start");

    // G92 + L20 вместе: Δ != 0 → чтение после L20 не вычислимо → unknown
    let both = format!(
        "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET G92 X0.0000 Y0.0000 Z5.0000 WCS G54\nOP 001 WCS_SET_HERE p=1 delta=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 RAPID start=unknown end=2.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 003 PEND\nEND\n",
        st("G54", "wcs", "none"), st("G54", "wcs", "none")
    );
    assert!(parser::parse(&both, "t").is_ok(), "preset+set_here: unknown ok");
    let known = both.replace("start=unknown", "start=1.0000,0.0000,0.0000");
    expect_err(&known, Some(ErrorCode::InvalidInput), "preset+set_here: known rejected");
}

#[test]
fn tool_change_before_first_motion_not_anchor() {
    // (a): не-motion опы перед первым motion-опом роли не играют (golden #27)
    let ok = prog(&format!(
        "OP 001 TOOL_CHANGE tool=3 src=0.1-0.0 feed=none spindle=off coolant=none tool=3 frame=wcs wcs=G54\nOP 002 STOP optional=no src=0.1-0.0 {}\nOP 003 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 004 PEND\n",
        st("G54", "wcs", "none"), st("G54", "wcs", "none")
    ));
    assert!(parser::parse(&ok, "t").is_ok(), "non-motion ops are not an anchor");
}

#[test]
fn minimal_program_valid() {
    // CORPUS #42: заголовок + program_end — валидна
    let p = parser::parse(&prog("OP 001 PEND\n"), "t").expect("minimal program");
    assert_eq!(p.ops.len(), 1);
}

// ===========================================================================
// Эмиттер: детерминизм, канонизация чисел, отказы
// ===========================================================================

#[test]
fn fmt_num_rules() {
    assert_eq!(emitter::fmt_num(0.0), "0.0000");
    assert_eq!(emitter::fmt_num(-0.0), "0.0000");
    assert_eq!(emitter::fmt_num(-1e-9), "0.0000");
    assert_eq!(emitter::fmt_num(10.0), "10.0000");
    assert_eq!(emitter::fmt_num(-5.5), "-5.5000");
    assert_eq!(emitter::fmt_num(179.9), "179.9000");
    assert_eq!(emitter::fmt_num(0.0175), "0.0175");
    assert_eq!(emitter::fmt_num(12000.0), "12000.0000");
    assert_eq!(emitter::fmt_num(-9.99985), "-9.9999");
}

#[test]
fn emit_rejects_invalid_canonical_forms() {
    // zero-length оп в AST → ошибка эмиссии (канонизация обязана отбросить)
    let zero = prog(&format!(
        "OP 001 RAPID start=1.0000,0.0000,0.0000 end=1.0000,0.0000,0.0000 src=0.1-0.0 {}\nOP 002 PEND\n",
        st("G54", "wcs", "none")
    ));
    let raw = parser::parse_raw(&zero, "t").expect("raw parse");
    let ds = emitter::emit(&raw).unwrap_err();
    assert!(ds.iter().all(|d| d.code == ErrorCode::InvalidInput));

    // eid_version != "EID/L1 v0.3" (в т.ч. прежние v0.1/v0.2 — формат версионирован)
    let mut p = parser::parse(&prog("OP 001 PEND\n"), "t").unwrap();
    p.eid_version = "EID/L1 v0.1".into();
    assert!(emitter::emit(&p).is_err());
    p.eid_version = "EID/L1 v0.2".into();
    assert!(emitter::emit(&p).is_err());
    p.eid_version = "EID/L1 v0.9".into();
    assert!(emitter::emit(&p).is_err());

    // PATH_MODE Blend с t= — представим в grammar v0.2 (A1); эмиссия канонична
    let mut p = parser::parse(&prog("OP 001 PEND\n"), "t").unwrap();
    p.path_mode = PathMode::Blend { t: Some(Num::new("0.5000".into(), 0.5)) };
    let t = emitter::emit(&p).unwrap();
    assert!(t.contains("PATH_MODE blend t=0.5000\n"), "blend t= emitted: {t}");
    assert!(parser::parse(&t, "t").is_ok());
    // неканоничный verbatim-текст числа канонизируется из f64-зеркала
    p.path_mode = PathMode::Blend { t: Some(Num::new("0.5".into(), 0.5)) };
    let t = emitter::emit(&p).unwrap();
    assert!(t.contains("PATH_MODE blend t=0.5000\n"), "canonicalized: {t}");
    // bare blend
    p.path_mode = PathMode::Blend { t: None };
    let t = emitter::emit(&p).unwrap();
    assert!(t.contains("PATH_MODE blend\n"));

    // RAW reason с кавычкой — не представим (нет экранирования)
    let mut p = parser::parse(&prog("OP 001 RAW reason=\"x\" code=E_INVALID_INPUT src=0.1-0.0 payload=00\nOP 002 PEND\n"), "t").unwrap();
    for op in p.ops.iter_mut() {
        if let Op::Raw { reason, .. } = op {
            *reason = "bad\"quote".into();
        }
    }
    assert!(emitter::emit(&p).is_err());
}

#[test]
fn emit_op_number_limit() {
    // op_num = ровно 3 цифры: AST из 999 опов эмитится, 1000-й номер —
    // E_LIMIT_EXCEEDED (охрана эмиссии). В тексте 4-значный номер невозможен
    // лексически (парсер отвергает).
    let mk = |v: f64| Num::new(emitter::fmt_num(v), v);
    let p3 = |x: f64| Point3 { x: mk(x), y: mk(0.0), z: mk(0.0) };
    let state = State {
        feed: None,
        spindle: Spindle::Off,
        coolant: Coolant { flood: false, mist: false },
        tool: None,
        frame: Frame::Wcs,
        wcs: Wcs::G54,
    };
    let src = SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 };
    let build = |n_motion: usize| -> Program {
        let mut ops = Vec::new();
        for i in 0..n_motion {
            let start = if i == 0 {
                StartPoint::Unknown
            } else {
                StartPoint::Known(p3(i as f64))
            };
            ops.push(Op::Rapid {
                start,
                end: p3((i + 1) as f64),
                state: state.clone(),
                src: src.clone(),
                sub: None,
            });
        }
        ops.push(Op::ProgramEnd { src: src.clone() });
        Program {
            eid_version: "EID/L1 v0.3".into(),
            units: Units::Mm,
            path_mode: PathMode::ExactPath,
            frame_preset: None,
            ops,
        }
    };
    // 998 rapid + PEND = 999 опов — номер последнего 999, всё представимо
    let ok = build(998);
    let t = emitter::emit(&ok).expect("999 ops emit ok");
    let reparsed = parser::parse(&t, "t").expect("999 ops re-parse");
    assert!(programq(&ok, &reparsed));
    // 999 rapid + PEND = 1000 опов — PEND получает номер 1000 → лимит
    let too_many = build(999);
    let ds = emitter::emit(&too_many).unwrap_err();
    assert!(
        ds.iter().any(|d| d.code == ErrorCode::LimitExceeded),
        "expected E_LIMIT_EXCEEDED, got {ds:?}"
    );
}

// ===========================================================================
// Итерация 4: verify-pass (MLIR-паттерн) — validate на любом этапе пайплайна
// ===========================================================================

/// Хелперы построения AST для verify-тестов (структурно, не через текст).
mod astbuild {
    use eid_core::ast::*;
    use eid_core::emitter::fmt_num;

    pub fn n(v: f64) -> Num {
        Num::new(fmt_num(v), v)
    }
    pub fn p3(x: f64, y: f64, z: f64) -> Point3 {
        Point3 { x: n(x), y: n(y), z: n(z) }
    }
    #[allow(dead_code)]
    pub fn src() -> SrcSpan {
        SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 }
    }
    /// Снапшот состояния: feed (None = none), wcs, frame, tool.
    pub fn st(feed: Option<f64>, wcs: Wcs, frame: Frame, tool: Option<u32>) -> State {
        State {
            feed: feed.map(n),
            spindle: Spindle::Off,
            coolant: Coolant { flood: false, mist: false },
            tool,
            frame,
            wcs,
        }
    }
    pub fn program(ops: Vec<Op>) -> Program {
        Program {
            eid_version: "EID/L1 v0.3".into(),
            units: Units::Mm,
            path_mode: PathMode::ExactPath,
            frame_preset: None,
            ops,
        }
    }
    pub fn pend() -> Op {
        Op::ProgramEnd { src: SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 } }
    }
    pub fn rapid(start: StartPoint, end: Point3, st: State) -> Op {
        Op::Rapid { start, end, state: st, src: SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 }, sub: None }
    }
    pub fn linear(start: StartPoint, end: Point3, st: State) -> Op {
        Op::Linear { start, end, state: st, src: SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 }, sub: None }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn arc(start: StartPoint, end: Point3, center: Point3, normal: Point3, dir: Dir, sweep: f64, pitch: f64, st: State) -> Op {
        Op::Arc {
            start,
            end,
            center,
            normal,
            dir,
            sweep_deg: n(sweep),
            pitch: n(pitch),
            state: st,
            src: SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 },
            sub: None,
        }
    }
}

/// validate(golden) = пусто для всех 48 golden (и notices пусты: golden —
/// пост-канонизационная форма). Pass вызывается standalone на сыром
/// parse_raw-AST — «любой этап пайплайна».
#[test]
fn verify_all_goldens_clean() {
    for f in golden_files() {
        let text = fs::read_to_string(&f).unwrap();
        let name = f.display().to_string();
        let raw = parser::parse_raw(&text, &name)
            .unwrap_or_else(|ds| panic!("{name}: raw parse failed: {ds:#?}"));
        let ds = verify::validate(&raw);
        assert!(
            ds.is_empty(),
            "{name}: validate(golden) not empty: {:#?}",
            ds.iter().map(|d| (&d.code, &d.stage, &d.message)).collect::<Vec<_>>()
        );
        let report = verify::validate_with_notices(&raw);
        assert!(
            report.notices.is_empty(),
            "{name}: golden must contain no droppable ops: {:#?}",
            report.notices
        );
        assert!(
            report.diagnostics.is_empty(),
            "{name}: golden diagnostics: {:#?}",
            report.diagnostics
        );
        assert_eq!(report.kept_ops.len(), raw.ops.len(), "{name}: kept_ops must match when clean");
    }
}

/// validate(сломанный AST) → точные коды + Stage::Verify на каждой диагностике.
#[test]
fn verify_broken_ast_exact_codes() {
    use astbuild::*;
    let ok_st = st(None, Wcs::G54, Frame::Wcs, None);
    let feed_st = st(Some(100.0), Wcs::G54, Frame::Wcs, None);

    let check = |prog: Program, ctx: &str| -> Vec<Diagnostic> {
        let ds = verify::validate(&prog);
        assert!(!ds.is_empty(), "{ctx}: expected diagnostics, got empty");
        assert!(
            ds.iter().all(|d| d.code == ErrorCode::InvalidInput),
            "{ctx}: expected all E_INVALID_INPUT, got {:#?}",
            ds.iter().map(|d| d.code).collect::<Vec<_>>()
        );
        assert!(
            ds.iter().all(|d| d.stage == Stage::Verify),
            "{ctx}: expected all Stage::Verify, got {:#?}",
            ds.iter().map(|d| d.stage).collect::<Vec<_>>()
        );
        ds
    };

    // 1. пустой список опов (нет PEND)
    check(program(vec![]), "empty ops");
    // 2. PEND не последний
    check(
        program(vec![
            pend(),
            rapid(StartPoint::Unknown, p3(1.0, 0.0, 0.0), ok_st.clone()),
        ]),
        "PEND not last",
    );
    // 3. дубликат PEND
    check(
        program(vec![
            rapid(StartPoint::Unknown, p3(1.0, 0.0, 0.0), ok_st.clone()),
            pend(),
            pend(),
        ]),
        "duplicate PEND",
    );
    // 4. zero-length rapid в каноническом AST (путь эмиссии — reject, не notice)
    let ds = check(
        program(vec![
            rapid(StartPoint::Known(p3(1.0, 0.0, 0.0)), p3(1.0, 0.0, 0.0), ok_st.clone()),
            pend(),
        ]),
        "zero-length rapid in AST",
    );
    assert!(ds.iter().any(|d| d.message.contains("must be dropped")), "zero-length message");
    // 5. zero-length dwell в каноническом AST
    let ds = check(
        program(vec![
            Op::Dwell { seconds: n(0.0), state: ok_st.clone(), src: SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 }, sub: None },
            pend(),
        ]),
        "zero dwell in AST",
    );
    assert!(ds.iter().any(|d| d.message.contains("must be dropped")), "zero dwell message");
    // 6. linear с feed=none
    check(
        program(vec![
            linear(StartPoint::Unknown, p3(1.0, 0.0, 0.0), ok_st.clone()),
            pend(),
        ]),
        "linear feed none",
    );
    // 7. arc с feed=none
    check(
        program(vec![
            arc(StartPoint::Known(p3(0.0, 0.0, 0.0)), p3(-5.0, 5.0, 2.0), p3(0.0, 5.0, 0.0), p3(0.0, 0.0, 1.0), Dir::Cw, 90.0, 2.0, ok_st.clone()),
            pend(),
        ]),
        "arc feed none",
    );
    // 8. ARC с unknown-стартом
    check(
        program(vec![
            arc(StartPoint::Unknown, p3(-5.0, 5.0, 2.0), p3(0.0, 5.0, 0.0), p3(0.0, 0.0, 1.0), Dir::Cw, 90.0, 2.0, feed_st.clone()),
            pend(),
        ]),
        "arc unknown start",
    );
    // 9. sweep вне (0°, 360°]
    check(
        program(vec![
            arc(StartPoint::Known(p3(0.0, 0.0, 0.0)), p3(-5.0, 5.0, 2.0), p3(0.0, 5.0, 0.0), p3(0.0, 0.0, 1.0), Dir::Cw, 400.0, 2.0, feed_st.clone()),
            pend(),
        ]),
        "sweep 400",
    );
    // 10. |normal| != 1
    check(
        program(vec![
            arc(StartPoint::Known(p3(0.0, 0.0, 0.0)), p3(-5.0, 5.0, 2.0), p3(0.0, 5.0, 0.0), p3(0.0, 0.0, 2.0), Dir::Cw, 90.0, 2.0, feed_st.clone()),
            pend(),
        ]),
        "normal len 2",
    );
    // 11. end не согласован с геометрией (§6.3)
    check(
        program(vec![
            arc(StartPoint::Known(p3(0.0, 0.0, 0.0)), p3(1.0, 5.0, 2.0), p3(0.0, 5.0, 0.0), p3(0.0, 0.0, 1.0), Dir::Cw, 90.0, 2.0, feed_st.clone()),
            pend(),
        ]),
        "arc endpoint inconsistent",
    );
    // 12. D4: TOOL_CHANGE tool= != state.tool
    check(
        program(vec![
            Op::ToolChange { tool: 3, state: st(None, Wcs::G54, Frame::Wcs, None), src: SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 } },
            pend(),
        ]),
        "D4 mismatch",
    );
    // 13. D5: WCS_SET_HERE после motion-опа
    check(
        program(vec![
            rapid(StartPoint::Unknown, p3(1.0, 0.0, 0.0), ok_st.clone()),
            Op::WcsSetHere { p: 2, delta: p3(1.0, 0.0, 0.0), state: st(None, Wcs::G55, Frame::Wcs, None), src: SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 } },
            pend(),
        ]),
        "set_here after motion",
    );
    // 14. D7: p вне 1..9
    check(
        program(vec![
            Op::WcsDefine { p: 0, origin: p3(0.0, 0.0, 0.0), state: ok_st.clone(), src: SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 } },
            pend(),
        ]),
        "wcs_define p=0",
    );
    // 15. D7: dwell < 0
    check(
        program(vec![
            Op::Dwell { seconds: n(-1.0), state: ok_st.clone(), src: SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 }, sub: None },
            pend(),
        ]),
        "negative dwell",
    );
    // 16. Q11: unknown вне точки разрыва (второй motion, та же wcs)
    check(
        program(vec![
            rapid(StartPoint::Unknown, p3(1.0, 0.0, 0.0), ok_st.clone()),
            rapid(StartPoint::Unknown, p3(2.0, 0.0, 0.0), ok_st.clone()),
            pend(),
        ]),
        "unknown outside break",
    );
    // 17. Q11: known в точке разрыва (первый motion без якоря)
    check(
        program(vec![
            rapid(StartPoint::Known(p3(1.0, 0.0, 0.0)), p3(2.0, 0.0, 0.0), ok_st.clone()),
            pend(),
        ]),
        "known at break",
    );
    // 18. Q11: known после смены wcs без in-text origins (критерий разности)
    check(
        program(vec![
            rapid(StartPoint::Unknown, p3(1.0, 0.0, 0.0), ok_st.clone()),
            rapid(StartPoint::Known(p3(1.0, 0.0, 0.0)), p3(2.0, 0.0, 0.0), st(None, Wcs::G55, Frame::Wcs, None)),
            pend(),
        ]),
        "known at (b) break",
    );
}

/// Позитивный контроль: валидные AST — validate пуст, notices пуст.
#[test]
fn verify_valid_ast_clean() {
    use astbuild::*;
    let feed_st = st(Some(100.0), Wcs::G54, Frame::Wcs, None);
    // (a)-break: rapid unknown первым + linear (вычисленный start), PEND
    let p = program(vec![
        rapid(StartPoint::Unknown, p3(0.0, 0.0, 5.0), st(None, Wcs::G54, Frame::Wcs, None)),
        linear(StartPoint::Known(p3(0.0, 0.0, 5.0)), p3(10.0, 0.0, 5.0), feed_st.clone()),
        pend(),
    ]);
    let ds = verify::validate(&p);
    assert!(ds.is_empty(), "valid AST flagged: {ds:#?}");
    // валидная дуга (golden #46-образная)
    let p = program(vec![
        rapid(StartPoint::Unknown, p3(0.0, 0.0, 0.0), st(None, Wcs::G54, Frame::Wcs, None)),
        arc(StartPoint::Known(p3(0.0, 0.0, 0.0)), p3(-5.0, 5.0, 2.0), p3(0.0, 5.0, 0.0), p3(0.0, 0.0, 1.0), Dir::Cw, 90.0, 2.0, feed_st.clone()),
        pend(),
    ]);
    assert!(verify::validate(&p).is_empty(), "valid arc flagged");
    // минимальная программа (CORPUS #42)
    let p = program(vec![pend()]);
    assert!(verify::validate(&p).is_empty(), "minimal program flagged");
}

/// provenance Stage: Parse — лексика, Verify — валидатор, Emit — эмиссия.
#[test]
fn diagnostic_stage_provenance() {
    // Parse: неверный magic
    let ds = parser::parse(
        "EID/L1 v0.2\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET none\nOP 001 PEND\nEND\n",
        "t",
    )
    .unwrap_err();
    assert!(!ds.is_empty());
    assert!(
        ds.iter().all(|d| d.stage == Stage::Parse),
        "parse stage: {:#?}",
        ds.iter().map(|d| d.stage).collect::<Vec<_>>()
    );

    // Verify: сломанный AST (нет PEND)
    let bad = astbuild::program(vec![]);
    let ds = verify::validate(&bad);
    assert!(!ds.is_empty());
    assert!(ds.iter().all(|d| d.stage == Stage::Verify));

    // Emit: eid_version != v0.3
    let mut p = parser::parse(&prog("OP 001 PEND\n"), "t").unwrap();
    p.eid_version = "EID/L1 v0.2".into();
    let ds = emitter::emit(&p).unwrap_err();
    assert!(
        ds.iter().all(|d| d.stage == Stage::Emit),
        "emit stage: {:#?}",
        ds.iter().map(|d| d.stage).collect::<Vec<_>>()
    );

    // Emit: RAW reason с кавычкой
    let mut p = parser::parse(
        &prog("OP 001 RAW reason=\"x\" code=E_INVALID_INPUT src=0.1-0.0 payload=00\nOP 002 PEND\n"),
        "t",
    )
    .unwrap();
    for op in p.ops.iter_mut() {
        if let Op::Raw { reason, .. } = op {
            *reason = "bad\"quote".into();
        }
    }
    let ds = emitter::emit(&p).unwrap_err();
    assert!(ds.iter().all(|d| d.stage == Stage::Emit));

    // Emit: лимит нумерации (1000 опов → номер 1000 не представим 3 цифрами)
    use astbuild::*;
    let mut ops = Vec::new();
    for i in 0..999 {
        let start = if i == 0 {
            StartPoint::Unknown
        } else {
            StartPoint::Known(p3(i as f64, 0.0, 0.0))
        };
        ops.push(rapid(start, p3((i + 1) as f64, 0.0, 0.0), st(None, Wcs::G54, Frame::Wcs, None)));
    }
    ops.push(pend());
    let p = program(ops);
    let ds = emitter::emit(&p).unwrap_err();
    assert!(
        ds.iter().any(|d| d.code == ErrorCode::LimitExceeded && d.stage == Stage::Emit),
        "limit exceeded with Emit stage: {ds:?}"
    );
}
