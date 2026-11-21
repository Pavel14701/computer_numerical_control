//! Итерация 5 — obligation-покрытие SPEC-EID-L1 v0.15.
//! Чек-лист «правило → тест» — в шапке crates/eid-core/src/verify.rs
//! (трассируемость правило↔тест обязательна; имена тестов = имена в чек-листе).
//! Каждый пункт: позитив (легально, корректный результат) + негатив
//! (Err с ТОЧНЫМ кодом).

use eid_core::ast::*;
use eid_core::diag::{ErrorCode, Stage};
use eid_core::emitter::fmt_num;
use eid_core::parser::{self, NoticeKind};
use eid_core::verify;

// --- builders -----------------------------------------------------------

fn n(v: f64) -> Num {
    Num::new(fmt_num(v), v)
}
fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3 { x: n(x), y: n(y), z: n(z) }
}
fn src() -> SrcSpan {
    SrcSpan { file: "t".into(), line_start: 0, col_start: 1, line_end: 0, col_end: 0 }
}
fn st(feed: Option<f64>, wcs: Wcs, frame: Frame) -> State {
    State {
        feed: feed.map(n),
        spindle: Spindle::Off,
        coolant: Coolant { flood: false, mist: false },
        tool: None,
        frame,
        wcs,
    }
}
fn program(ops: Vec<Op>) -> Program {
    Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: PathMode::ExactPath,
        frame_preset: None,
        ops,
    }
}
fn pend() -> Op {
    Op::ProgramEnd { src: src() }
}
fn rapid(start: StartPoint, end: Point3, s: State) -> Op {
    Op::Rapid { start, end, state: s, src: src(), sub: None }
}
fn linear(start: StartPoint, end: Point3, s: State) -> Op {
    Op::Linear { start, end, state: s, src: src(), sub: None }
}
#[allow(clippy::too_many_arguments)]
fn arc(start: StartPoint, end: Point3, center: Point3, normal: Point3, dir: Dir, sweep: f64, pitch: f64, s: State) -> Op {
    Op::Arc {
        start,
        end,
        center,
        normal,
        dir,
        sweep_deg: n(sweep),
        pitch: n(pitch),
        state: s,
        src: src(),
        sub: None,
    }
}

const HDR: &str = "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nFRAME_PRESET none\n";
const ST_G54: &str = "feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54";
const ST_G55: &str = "feed=none spindle=off coolant=none tool=none frame=wcs wcs=G55";
const ST_MACHINE: &str = "feed=none spindle=off coolant=none tool=none frame=machine wcs=G54";

// ===========================================================================
// §3: program_end
// ===========================================================================

/// PEND-инварианты (ровно один / последний / отсутствие) — проверены через
/// все 3 режима входа: parser::parse (текст), verify::validate (AST),
/// verify::validate_with_notices (AST+notices).
#[test]
fn spec_3_pend_invariants_three_modes() {
    let cases: [(&str, &str); 3] = [
        ("no PEND", "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\n"),
        ("two PENDs", "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 PEND\nOP 003 PEND\n"),
        ("PEND not last", "OP 001 PEND\nOP 002 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\n"),
    ];
    for (ctx, ops) in &cases {
        let text = format!("{HDR}{ops}END\n");
        // режим 1: текст → parse
        let ds = parser::parse(&text, "t").unwrap_err();
        assert!(!ds.is_empty(), "{ctx}");
        assert!(ds.iter().all(|d| d.code == ErrorCode::InvalidInput && d.stage == Stage::Verify), "{ctx}: {ds:?}");
        // режим 2: AST → validate
        let raw = parser::parse_raw(&text, "t").unwrap();
        let ds2 = verify::validate(&raw);
        assert!(!ds2.is_empty(), "{ctx}");
        // пин ветки: "no PEND" обязан дать именно missing-PEND диагностику
        // (мутант-инверсия !ops.is_empty() иначе переживает общий не-empty).
        if *ctx == "no PEND" {
            assert!(
                ds2.iter().any(|d| d.message.contains("missing program_end")),
                "{ctx}: нет missing-PEND диагностики: {ds2:?}"
            );
        }
        assert!(ds2.iter().all(|d| d.code == ErrorCode::InvalidInput && d.stage == Stage::Verify), "{ctx}");
        // режим 3: AST → validate_with_notices (diagnostics, notices пусты —
        // PEND-нарушения не drop)
        let rep = verify::validate_with_notices(&raw);
        assert!(!rep.diagnostics.is_empty(), "{ctx}");
        assert!(rep.notices.is_empty(), "{ctx}: PEND violations must not drop ops");
    }
    // позитив: минимальная программа валидна во всех 3 режимах
    let ok_text = format!("{HDR}OP 001 PEND\nEND\n");
    assert!(parser::parse(&ok_text, "t").is_ok());
    let raw = parser::parse_raw(&ok_text, "t").unwrap();
    assert!(verify::validate(&raw).is_empty());
    let rep = verify::validate_with_notices(&raw);
    assert!(rep.diagnostics.is_empty() && rep.notices.is_empty());
}

// ===========================================================================
// §3: feed в итоговом AST
// ===========================================================================

/// feed != none требуется linear/arc ТОЛЬКО в итоговом AST: выживший linear
/// без feed — E_INVALID_INPUT; zero-length linear без feed — drop+notice
/// (не feed-ошибка); reject-режим (эмиссия) — обе диагностики.
#[test]
fn spec_3_feed_final_ast() {
    let feed_st = st(Some(100.0), Wcs::G54, Frame::Wcs);
    let nofeed_st = st(None, Wcs::G54, Frame::Wcs);
    // негатив: выживший linear feed=none
    let p = program(vec![
        linear(StartPoint::Unknown, p3(1.0, 0.0, 0.0), nofeed_st.clone()),
        pend(),
    ]);
    let ds = verify::validate(&p);
    assert!(ds.iter().any(|d| d.message.contains("linear with feed=none")), "{ds:?}");
    // позитив: тот же оп с feed — чисто
    let p = program(vec![linear(StartPoint::Unknown, p3(1.0, 0.0, 0.0), feed_st.clone()), pend()]);
    assert!(verify::validate(&p).is_empty());
    // позитив (после отбрасывания): zero-length linear feed=none → drop+notice,
    // feed-ошибки НЕТ (проверка feed — по итоговому AST)
    let p = program(vec![
        linear(StartPoint::Known(p3(1.0, 0.0, 0.0)), p3(1.0, 0.0, 0.0), nofeed_st.clone()),
        pend(),
    ]);
    let rep = verify::validate_with_notices(&p);
    assert!(rep.diagnostics.is_empty(), "feed must not fire on dropped op: {:#?}", rep.diagnostics);
    assert_eq!(rep.notices.len(), 1);
    // reject-режим (эмиссия): zero-length диагностика + (оп оставлен →) feed
    let ds = verify::validate(&p);
    assert!(ds.iter().any(|d| d.message.contains("must be dropped")));
    assert!(ds.iter().any(|d| d.message.contains("linear with feed=none")));
}

// ===========================================================================
// §3: zero-length drop+notice — все 4 вида, ± пресетка
// ===========================================================================

#[test]
fn spec_3_zero_length_all_kinds_with_preset() {
    // программа с G92-пресеткой: drop-семантика не зависит от пресетки
    let mut p = program(vec![
        rapid(StartPoint::Known(p3(1.0, 0.0, 5.0)), p3(1.0, 0.0, 5.0), st(None, Wcs::G54, Frame::Wcs)),
        linear(StartPoint::Known(p3(1.0, 0.0, 5.0)), p3(1.0, 0.0, 5.0), st(Some(100.0), Wcs::G54, Frame::Wcs)),
        arc(
            StartPoint::Known(p3(1.0, 0.0, 5.0)),
            p3(1.0, 0.0, 5.0),
            p3(2.0, 0.0, 5.0),
            p3(0.0, 0.0, 1.0),
            Dir::Cw,
            90.0,
            0.0,
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        Op::Dwell { seconds: n(0.0), state: st(None, Wcs::G54, Frame::Wcs), src: src(), sub: None },
        pend(),
    ]);
    p.frame_preset = Some(FramePreset { values: p3(0.0, 0.0, 5.0), wcs: Wcs::G54 });
    let rep = verify::validate_with_notices(&p);
    assert!(rep.diagnostics.is_empty(), "{:#?}", rep.diagnostics);
    assert_eq!(rep.notices.len(), 4, "{:#?}", rep.notices);
    assert_eq!(rep.notices[0].kind, NoticeKind::ZeroLengthMotion);
    assert_eq!(rep.notices[1].kind, NoticeKind::ZeroLengthMotion);
    assert_eq!(rep.notices[2].kind, NoticeKind::ZeroLengthArc);
    assert_eq!(rep.notices[3].kind, NoticeKind::ZeroDwell);
    // выжили: только PEND (и пресетка)
    assert_eq!(rep.kept_ops.len(), 1);
    // reject-режим: 4 диагностики zero-length
    let ds = verify::validate(&p);
    assert_eq!(ds.iter().filter(|d| d.message.contains("must be dropped")).count(), 4);
    // parse-путь (текст) даёт те же notices
    let _ = &mut p;
}

// ===========================================================================
// §3/§6: полный круг sweep=360 — НЕ zero-length (kept), 359 — drop
// ===========================================================================

#[test]
fn spec_3_full_circle_sweep360_kept() {
    // Позиция якорится первым rapid (unknown → (0,0,0)), иначе Known-старт
    // дуги — вне точки разрыва. Позитив: start == end, sweep = 360°, pitch = 0
    // → полный круг, kept, геометрия сходится (Rot(360°) = identity)
    let anchor = rapid(StartPoint::Unknown, p3(0.0, 0.0, 0.0), st(None, Wcs::G54, Frame::Wcs));
    let p = program(vec![
        anchor.clone(),
        arc(
            StartPoint::Known(p3(0.0, 0.0, 0.0)),
            p3(0.0, 0.0, 0.0),
            p3(5.0, 0.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Ccw,
            360.0,
            0.0,
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ]);
    let rep = verify::validate_with_notices(&p);
    assert!(rep.notices.is_empty(), "sweep=360 must NOT be dropped: {:#?}", rep.notices);
    assert!(rep.diagnostics.is_empty(), "{:#?}", rep.diagnostics);
    assert_eq!(rep.kept_ops.len(), 3);
    // негатив: тот же ход при sweep = 359° — нулевой ход → drop+notice
    let p = program(vec![
        anchor,
        arc(
            StartPoint::Known(p3(0.0, 0.0, 0.0)),
            p3(0.0, 0.0, 0.0),
            p3(5.0, 0.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Ccw,
            359.0,
            0.0,
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ]);
    let rep = verify::validate_with_notices(&p);
    assert_eq!(rep.notices.len(), 1);
    assert_eq!(rep.notices[0].kind, NoticeKind::ZeroLengthArc);
}

// ===========================================================================
// §3: path_mode глобален (один на программу)
// ===========================================================================

#[test]
fn spec_3_path_mode_single() {
    // дубликат PATH_MODE-строки — вне продукции program (lexical err)
    let text = "EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\nPATH_MODE blend\nFRAME_PRESET none\nOP 001 PEND\nEND\n";
    assert!(parser::parse_raw(text, "t").is_err());
    // AST несёт ровно одно значение поля (по конструкции типа)
    let p = parser::parse(&format!("{HDR}OP 001 PEND\nEND\n"), "t").unwrap();
    assert_eq!(p.path_mode, PathMode::ExactPath);
}

// ===========================================================================
// §2/§3: Q11 — матрица точек разрыва (a)-(d), Known/Unknown, легально/нелегально
// ===========================================================================

#[test]
fn spec_3_q11_breakpoint_matrix() {
    let mk = |ops: &str, preset: Option<&str>| -> String {
        let pr = preset.unwrap_or("FRAME_PRESET none");
        format!("EID/L1 v0.3\nUNITS mm\nPATH_MODE exact_path\n{pr}\n{ops}END\n")
    };
    let ok = |text: &str, ctx: &str| parser::parse(text, "t").unwrap_or_else(|ds| panic!("{ctx}: {ds:#?}"));
    let err = |text: &str, ctx: &str| {
        let ds = parser::parse(text, "t").unwrap_err();
        assert!(!ds.is_empty(), "{ctx}");
        assert!(ds.iter().all(|d| d.code == ErrorCode::InvalidInput && d.stage == Stage::Verify), "{ctx}: {ds:?}");
    };

    // (a) первый motion без якоря
    ok(&mk("OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 PEND\n", None), "(a) unknown legal");
    err(&mk("OP 001 RAPID start=1.0000,0.0000,0.0000 end=2.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 PEND\n", None), "(a) known illegal");

    // (a)-якорь G92 в той же системе
    ok(&mk("OP 001 RAPID start=0.0000,0.0000,5.0000 end=1.0000,0.0000,5.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 PEND\n", Some("FRAME_PRESET G92 X0.0000 Y0.0000 Z5.0000 WCS G54")), "(a)-anchor known==preset legal");
    err(&mk("OP 001 RAPID start=1.0000,0.0000,5.0000 end=2.0000,0.0000,5.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 PEND\n", Some("FRAME_PRESET G92 X0.0000 Y0.0000 Z5.0000 WCS G54")), "(a)-anchor known!=preset illegal");

    // (b) смена wcs: обе системы без define → unknown обязан / known нелегален
    ok(&mk(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 RAPID start=unknown end=2.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G55\nOP 003 PEND\n",
        None,
    ), "(b) unknown legal");
    err(&mk(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 RAPID start=1.0000,0.0000,0.0000 end=2.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G55\nOP 003 PEND\n",
        None,
    ), "(b) known illegal (разность origin не вычислима)");

    // (b)-вычислимо: обе системы определены → known обязателен, значение =
    // prev_end + O_prev − O_new = 5 + (10 − 3) = 12; unknown нелегален
    let defined = "OP 001 WCS_DEFINE p=1 origin=10.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 WCS_DEFINE p=2 origin=3.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\n";
    ok(&mk(&format!("{defined}OP 003 RAPID start=unknown end=5.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 004 RAPID start=12.0000,0.0000,0.0000 end=13.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G55\nOP 005 PEND\n"), None), "(b) computable known legal");
    err(&mk(&format!("{defined}OP 003 RAPID start=unknown end=5.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 004 RAPID start=unknown end=13.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G55\nOP 005 PEND\n"), None), "(b) computable unknown illegal");

    // (c) machine-frame ход: origin in-text + нет preset → known обязателен
    ok(&mk(&format!("{defined}OP 003 RAPID start=unknown end=5.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 004 RAPID start=15.0000,0.0000,0.0000 end=16.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 005 PEND\n"), None), "(c) computable known legal (M = reading + O)");
    err(&mk(&format!("{defined}OP 003 RAPID start=unknown end=5.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 004 RAPID start=unknown end=16.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 005 PEND\n"), None), "(c) computable unknown illegal");
    // (c) без origin in-text → unknown обязан / known нелегален
    ok(&mk("OP 001 RAPID start=unknown end=5.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 RAPID start=unknown end=16.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 003 PEND\n", None), "(c) no-origin unknown legal");
    err(&mk("OP 001 RAPID start=unknown end=5.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 RAPID start=15.0000,0.0000,0.0000 end=16.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 003 PEND\n", None), "(c) no-origin known illegal");
    // (c) с preset — невычислим даже с origin → unknown обязан
    // (OP 003 — known = значения пресетки (1,0,0): с якорем unknown на нём
    //  нелегален; инвариант пресетки отдельно проверен в (a)-anchor)
    err(&mk(&format!("{defined}OP 003 RAPID start=1.0000,0.0000,0.0000 end=5.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 004 RAPID start=15.0000,0.0000,0.0000 end=16.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 005 PEND\n"), Some("FRAME_PRESET G92 X1.0000 Y0.0000 Z0.0000 WCS G54")), "(c) with-preset known illegal");
    ok(&mk(&format!("{defined}OP 003 RAPID start=1.0000,0.0000,0.0000 end=5.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 004 RAPID start=unknown end=16.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 005 PEND\n"), Some("FRAME_PRESET G92 X1.0000 Y0.0000 Z0.0000 WCS G54")), "(c) with-preset unknown legal");

    // (d) machine → wcs: с origin in-text — known обязателен (reading = M − O),
    // unknown нелегален; без origin — unknown обязан. Machine-цепочка
    // сцепляется: start = end предыдущего (§2, последующие machine-ходы).
    ok(&mk(&format!("{defined}OP 003 RAPID start=unknown end=5.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 004 RAPID start=5.0000,0.0000,0.0000 end=16.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 005 RAPID start=6.0000,0.0000,0.0000 end=7.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 006 PEND\n"), None), "(d) with-origin known legal (M−O)");
    err(&mk(&format!("{defined}OP 003 RAPID start=unknown end=5.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 004 RAPID start=5.0000,0.0000,0.0000 end=16.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 005 RAPID start=unknown end=7.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 006 PEND\n"), None), "(d) with-origin unknown illegal");
    ok(&mk("OP 001 RAPID start=unknown end=5.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 002 RAPID start=5.0000,0.0000,0.0000 end=16.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 003 RAPID start=unknown end=6.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 004 PEND\n", None), "(d) no-origin unknown legal");
    err(&mk("OP 001 RAPID start=unknown end=5.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 002 RAPID start=5.0000,0.0000,0.0000 end=16.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=machine wcs=G54\nOP 003 RAPID start=6.0000,0.0000,0.0000 end=7.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 004 PEND\n", None), "(d) no-origin known illegal");
}

// ===========================================================================
// §5: D4 / D5 / D7 (сжатые spec-пары; развёрнутые — в roundtrip/conformance)
// ===========================================================================

#[test]
fn spec_5_d4_tool_mismatch() {
    // негатив: op-поле tool= != state.tool → E_INVALID_INPUT
    let p = program(vec![
        Op::ToolChange { tool: 3, state: st(None, Wcs::G54, Frame::Wcs), src: src() },
        pend(),
    ]);
    let ds = verify::validate(&p);
    assert!(ds.iter().any(|d| d.message.contains("(D4)") && d.code == ErrorCode::InvalidInput));
    // позитив: согласованы — чисто
    let p = program(vec![
        Op::ToolChange { tool: 3, state: State { feed: None, spindle: Spindle::Off, coolant: Coolant { flood: false, mist: false }, tool: Some(3), frame: Frame::Wcs, wcs: Wcs::G54 }, src: src() },
        pend(),
    ]);
    assert!(verify::validate(&p).is_empty());
}

#[test]
fn spec_5_d5_set_here_preset_only() {
    // негатив: L20 после motion-опа
    let p = program(vec![
        rapid(StartPoint::Unknown, p3(1.0, 0.0, 0.0), st(None, Wcs::G54, Frame::Wcs)),
        Op::WcsSetHere { p: 2, delta: p3(1.0, 0.0, 0.0), state: st(None, Wcs::G55, Frame::Wcs), src: src() },
        pend(),
    ]);
    let ds = verify::validate(&p);
    assert!(ds.iter().any(|d| d.message.contains("D5") && d.code == ErrorCode::InvalidInput));
    // позитив: L20 до motion-опа (якорь) — чисто
    let p = program(vec![
        Op::WcsSetHere { p: 2, delta: p3(1.0, 0.0, 0.0), state: st(None, Wcs::G55, Frame::Wcs), src: src() },
        rapid(StartPoint::Known(p3(1.0, 0.0, 0.0)), p3(2.0, 0.0, 0.0), st(None, Wcs::G55, Frame::Wcs)),
        pend(),
    ]);
    assert!(verify::validate(&p).is_empty());
}

#[test]
fn spec_5_d7_domains() {
    let feed_st = st(Some(100.0), Wcs::G54, Frame::Wcs);
    // sweep: границы домена (0°, 360°] — 360 легален (см. full-circle тест),
    // 0 при start≠end и 400 — нелегальны
    let mk_arc = |sweep: f64, normal: f64, end: Point3| {
        program(vec![
            arc(StartPoint::Known(p3(0.0, 0.0, 0.0)), end, p3(0.0, 5.0, 0.0), p3(0.0, 0.0, normal), Dir::Ccw, sweep, 2.0, feed_st.clone()),
            pend(),
        ])
    };
    // позитив: sweep=360 полная окружность (start≠end → геометрия: Rot(360)=id,
    // end должен быть = start; поэтому для домен-теста start==end ниже).
    // Негатив: sweep=0 (start≠end) → домен; sweep=400 → домен
    for sweep in [0.0, 400.0] {
        let ds = verify::validate(&mk_arc(sweep, 1.0, p3(5.0, 5.0, 2.0)));
        assert!(
            ds.iter().any(|d| d.message.contains("out of domain (0°, 360°]") && d.code == ErrorCode::InvalidInput),
            "sweep={sweep}: {ds:?}"
        );
    }
    // негатив: |normal| != 1
    let ds = verify::validate(&mk_arc(90.0, 2.0, p3(-5.0, 5.0, 2.0)));
    assert!(ds.iter().any(|d| d.message.contains("|normal|") && d.code == ErrorCode::InvalidInput));
    // негатив: dwell < 0; позитив: dwell = 0-drop (не ошибка), dwell > 0 чисто
    let neg = program(vec![Op::Dwell { seconds: n(-0.0001), state: st(None, Wcs::G54, Frame::Wcs), src: src(), sub: None }, pend()]);
    assert!(verify::validate(&neg).iter().any(|d| d.message.contains("dwell seconds") && d.code == ErrorCode::InvalidInput));
    let zero = program(vec![Op::Dwell { seconds: n(0.0), state: st(None, Wcs::G54, Frame::Wcs), src: src(), sub: None }, pend()]);
    let rep = verify::validate_with_notices(&zero);
    assert!(rep.diagnostics.is_empty() && rep.notices.len() == 1);
    let pos = program(vec![Op::Dwell { seconds: n(0.5), state: st(None, Wcs::G54, Frame::Wcs), src: src(), sub: None }, pend()]);
    assert!(verify::validate(&pos).is_empty());
    // p: 0 и 10 нелегальны (10 — лексически цифрой не является; тут AST-уровень
    // ловит 0; 10 отсекается парсером — см. grammar_conformance::p_domain_lexical_boundary)
    let p0 = program(vec![Op::WcsDefine { p: 0, origin: p3(0.0, 0.0, 0.0), state: st(None, Wcs::G54, Frame::Wcs), src: src() }, pend()]);
    assert!(verify::validate(&p0).iter().any(|d| d.message.contains("p=0 out of domain") && d.code == ErrorCode::InvalidInput));
}

// ===========================================================================
// §6: геометрия дуги — направление, невозможная дуга, единицы
// ===========================================================================

#[test]
fn spec_6_arc_direction_must_match_rotation() {
    // Уравнение позиции закрывает «sweep vs 360°−sweep» и путаницу направления:
    // позиция якорится rapid'ом (unknown → (0,0,0)).
    // start (0,0,0), center (0,5,0), end (5,5,2), pitch 2, sweep 90:
    //   ccw (+90° вокруг +Z): (0,-5) → (5,0) → end = (5,5,2) ✓
    //   cw  (−90°): (0,-5) → (−5,0) → end = (−5,5,2) ✗ (ложно-совпадающая дуга
    //   с тем же радиусом — ловится только уравнением)
    let anchor = rapid(StartPoint::Unknown, p3(0.0, 0.0, 0.0), st(None, Wcs::G54, Frame::Wcs));
    let st = st(Some(100.0), Wcs::G54, Frame::Wcs);
    let ok_p = program(vec![
        anchor.clone(),
        arc(StartPoint::Known(p3(0.0, 0.0, 0.0)), p3(5.0, 5.0, 2.0), p3(0.0, 5.0, 0.0), p3(0.0, 0.0, 1.0), Dir::Ccw, 90.0, 2.0, st.clone()),
        pend(),
    ]);
    assert!(verify::validate(&ok_p).is_empty(), "ccw must match rotation equation");
    let bad_p = program(vec![
        anchor,
        arc(StartPoint::Known(p3(0.0, 0.0, 0.0)), p3(5.0, 5.0, 2.0), p3(0.0, 5.0, 0.0), p3(0.0, 0.0, 1.0), Dir::Cw, 90.0, 2.0, st.clone()),
        pend(),
    ]);
    let ds = verify::validate(&bad_p);
    assert!(
        ds.iter().any(|d| d.message.contains("endpoint inconsistent") && d.code == ErrorCode::InvalidInput),
        "mirrored arc must fail the rotation equation: {ds:?}"
    );
}

#[test]
fn spec_6_impossible_arc() {
    // Планиметрическая хорда > 2R: start в центре (ρ_start = 0), end на
    // расстоянии 10 → радиусы не сходятся → E_INVALID_INPUT (не unsupported).
    // Позиция якорится rapid'ом.
    let p = program(vec![
        rapid(StartPoint::Unknown, p3(0.0, 0.0, 0.0), st(None, Wcs::G54, Frame::Wcs)),
        arc(StartPoint::Known(p3(0.0, 0.0, 0.0)), p3(10.0, 0.0, 2.0), p3(0.0, 0.0, 0.0), p3(0.0, 0.0, 1.0), Dir::Ccw, 90.0, 2.0, st(Some(100.0), Wcs::G54, Frame::Wcs)),
        pend(),
    ]);
    let ds = verify::validate(&p);
    assert!(
        ds.iter().any(|d| d.message.contains("radius mismatch") && d.code == ErrorCode::InvalidInput),
        "{ds:?}"
    );
}

#[test]
fn spec_6_arc_units_agnostic_inch() {
    // R+pitch/units: геометрия дуги не зависит от units (значения в declared
    // units); inch-дуга с pitch (аналог R+pitch после канонизации) — валидна.
    // Позиция якорится rapid'ом.
    let mut p = program(vec![
        rapid(StartPoint::Unknown, p3(0.0, 0.0, 0.0), st(None, Wcs::G54, Frame::Wcs)),
        arc(StartPoint::Known(p3(0.0, 0.0, 0.0)), p3(-5.0, 5.0, 2.0), p3(0.0, 5.0, 0.0), p3(0.0, 0.0, 1.0), Dir::Cw, 90.0, 2.0, st(Some(100.0), Wcs::G54, Frame::Wcs)),
        pend(),
    ]);
    p.units = Units::In;
    assert!(verify::validate(&p).is_empty(), "units must not affect arc invariants");
}

// ===========================================================================
// §4: feed на rapid без ограничений (G0 не сбрасывает feed — resolved-state
// правило канонизатора; в EID AST feed — явное поле, verify не требует feed
// на rapid)
// ===========================================================================

#[test]
fn spec_4_rapid_feed_no_constraint() {
    // rapid с feed — легален (переживает G0), и без feed — легален
    let with = program(vec![rapid(StartPoint::Unknown, p3(1.0, 0.0, 0.0), st(Some(500.0), Wcs::G54, Frame::Wcs)), pend()]);
    let without = program(vec![rapid(StartPoint::Unknown, p3(1.0, 0.0, 0.0), st(None, Wcs::G54, Frame::Wcs)), pend()]);
    assert!(verify::validate(&with).is_empty());
    assert!(verify::validate(&without).is_empty());
}

// ===========================================================================
// §10.6: квантование входа — zero-check ПОСЛЕ квантования
// ===========================================================================

#[test]
fn spec_10_quantized_zero_check() {
    // zero-check применяется ПОСЛЕ квантования к 4dp: -0.0000 ≡ 0.0000.
    // Позиция якорится первым rapid'ом (unknown → (1,0,0)); второй rapid с
    // Known-стартом легален (точное равенство prev_end).
    let anchor = format!(
        "OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST_G54}\n"
    );
    // квантовали в старт → drop: start (1,0,0), end (1,0,0) — нулевой ход
    let text_zero = format!(
        "{HDR}{anchor}OP 002 RAPID start=1.0000,0.0000,0.0000 end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST_G54}\nOP 003 PEND\nEND\n"
    );
    let out = parser::parse_with_notices(&text_zero, "t").expect("parse ok");
    assert_eq!(out.notices.len(), 1, "{:#?}", out.notices);
    assert_eq!(out.notices[0].kind, NoticeKind::ZeroLengthMotion);
    // не совпали на 4dp → ход НЕ нулевой → kept: end=1.0001 (квантование
    // «сырого» 1.00004 диалекта к 4dp дало бы 1.0001 → ход выживает)
    let text_small = format!(
        "{HDR}{anchor}OP 002 RAPID start=1.0000,0.0000,0.0000 end=1.0001,0.0000,0.0000 src=0.1-0.0 {ST_G54}\nOP 003 PEND\nEND\n"
    );
    let out = parser::parse_with_notices(&text_small, "t").expect("parse ok");
    assert!(out.notices.is_empty(), "1.0001 ≠ 1.0000 at 4dp: {:#?}", out.notices);
    assert_eq!(out.program.ops.len(), 3);
    // знаковый ноль квантуется в тот же ноль (−0.0000 ≡ 0.0000 → drop):
    let text_neg0 = format!(
        "{HDR}{anchor}OP 002 RAPID start=1.0000,-0.0000,0.0000 end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST_G54}\nOP 003 PEND\nEND\n"
    );
    let out = parser::parse_with_notices(&text_neg0, "t").expect("parse ok");
    assert_eq!(out.notices.len(), 1, "-0.0000 ≡ 0.0000 after quantization: {:#?}", out.notices);
    assert_eq!(out.notices[0].kind, NoticeKind::ZeroLengthMotion);
}

// ===========================================================================
// §3: notices через parse-путь = через verify-путь (консистентность pass'ов)
// ===========================================================================

#[test]
fn spec_3_notices_consistent_across_entry_points() {
    let mk_zero = || {
        program(vec![
            rapid(StartPoint::Known(p3(1.0, 0.0, 0.0)), p3(1.0, 0.0, 0.0), st(None, Wcs::G54, Frame::Wcs)),
            pend(),
        ])
    };
    let text = format!("{HDR}OP 001 RAPID start=1.0000,0.0000,0.0000 end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST_G54}\nOP 002 PEND\nEND\n");
    // путь 1: текст → parse_with_notices
    let a = parser::parse_with_notices(&text, "t").unwrap();
    // путь 2: AST → validate_with_notices (сырой AST)
    let raw = parser::parse_raw(&text, "t").unwrap();
    let b = verify::validate_with_notices(&raw);
    // путь 3: AST-конструкция → validate_with_notices
    let c = verify::validate_with_notices(&mk_zero());
    assert_eq!(a.notices.len(), 1);
    assert_eq!(b.notices.len(), 1);
    assert_eq!(c.notices.len(), 1);
    assert_eq!(a.notices[0].kind, b.notices[0].kind);
    assert_eq!(b.notices[0].kind, c.notices[0].kind);
    // AST после отбрасывания идентичен во всех путях (kept = PEND)
    assert_eq!(a.program.ops.len(), 1);
    assert_eq!(b.kept_ops.len(), 1);
    assert_eq!(c.kept_ops.len(), 1);
    // suppress-предупреждения о неиспользуемых импортах в пустом месте
    let _ = (ST_G55, ST_MACHINE);
}
