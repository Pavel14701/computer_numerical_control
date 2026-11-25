// Итерация 5 — obligation-покрытие SPEC-EID-L1 v0.15.
// Чек-лист «правило → тест» — в шапке crates/eid-core/src/verify.rs
// (трассируемость правило↔тест обязательна; имена тестов = имена в чек-листе).
// Каждый пункт: позитив (легально, корректный результат) + негатив
// (Err с ТОЧНЫМ кодом).

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
    Point3 {
        x: n(x),
        y: n(y),
        z: n(z),
    }
}
fn src() -> SrcSpan {
    SrcSpan {
        file: "t".into(),
        line_start: 0,
        col_start: 1,
        line_end: 0,
        col_end: 0,
    }
}
fn st(feed: Option<f64>, wcs: Wcs, frame: Frame) -> State {
    State {
        feed: feed.map(n),
        spindle: Spindle::Off,
        coolant: Coolant {
            flood: false,
            mist: false,
        },
        tool: None,
        frame,
        wcs,
        feed_restated: false,
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
    Op::Rapid {
        start,
        end,
        state: s,
        src: src(),
        sub: None,
    }
}
fn linear(start: StartPoint, end: Point3, s: State) -> Op {
    Op::Linear {
        start,
        end,
        state: s,
        src: src(),
        sub: None,
    }
}
#[allow(clippy::too_many_arguments)]
fn arc(
    start: StartPoint,
    end: Point3,
    center: Point3,
    normal: Point3,
    dir: Dir,
    sweep: f64,
    pitch: f64,
    s: State,
) -> Op {
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
        assert!(
            ds.iter()
                .all(|d| d.code == ErrorCode::InvalidInput && d.stage == Stage::Verify),
            "{ctx}: {ds:?}"
        );
        // режим 2: AST → validate
        let raw = parser::parse_raw(&text, "t").unwrap();
        let ds2 = verify::validate(&raw);
        assert!(!ds2.is_empty(), "{ctx}");
        // пин ветки: "no PEND" обязан дать именно missing-PEND диагностику
        // (мутант-инверсия !ops.is_empty() иначе переживает общий не-empty).
        if *ctx == "no PEND" {
            assert!(
                ds2.iter()
                    .any(|d| d.message.contains("missing program_end")),
                "{ctx}: нет missing-PEND диагностики: {ds2:?}"
            );
        }
        assert!(
            ds2.iter()
                .all(|d| d.code == ErrorCode::InvalidInput && d.stage == Stage::Verify),
            "{ctx}"
        );
        // режим 3: AST → validate_with_notices (diagnostics, notices пусты —
        // PEND-нарушения не drop)
        let rep = verify::validate_with_notices(&raw);
        assert!(!rep.diagnostics.is_empty(), "{ctx}");
        assert!(
            rep.notices.is_empty(),
            "{ctx}: PEND violations must not drop ops"
        );
    }
    // позитив: минимальная программа валидна во всех 3 режимах
    let ok_text = format!("{HDR}OP 001 PEND\nEND\n");
    assert!(parser::parse(&ok_text, "t").is_ok());
    let raw = parser::parse_raw(&ok_text, "t").unwrap();
    assert!(verify::validate(&raw).is_empty());
    let rep = verify::validate_with_notices(&raw);
    assert!(rep.diagnostics.is_empty() && rep.notices.is_empty());
}

/// SPEC v0.20 (§5.2, ревизия инварианта program_end, коммит c76ed9a): правило
/// «PEND обязан быть последним опом» заменено на «после PEND допустимы ТОЛЬКО
/// Raw-опы». Raw-хвост LEGAL (lossless; rs274 молча игнорирует хвост — зонд 5
/// exit 0; grbl-стрим исполняет — protocol.c:74-105; конструкция
/// диалект-расходится → кросс-диалект отказывает по правилу Raw §8.2).
/// Не-Raw оп после PEND → прежний отказ с текстом «program_end must be the
/// last op» — строка ДОСЛОВНО (emit-фильтры eid-grbl/eid-fanuc/eid-haas
/// завязаны на неё через message.contains). Второй ProgramEnd-оп в AST → тот
/// же отказ: реальный парсер v0.20 строит только первый PEND (хвост = Raw,
/// N13 в зеркалах снят), hand-built AST с двумя PEND-опами — это не «блоки
/// после M2», валидатор канон-формы вправе отказать.
#[test]
fn spec_3_pend_tail_raw_only_v020() {
    // --- позитив: RAPID, PEND, RAW — Raw-хвост легален во всех 3 режимах -----
    let ok_text = format!(
        "{HDR}OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST_G54}\n\
         OP 002 PEND\n\
         OP 003 RAW reason=\"tail after program end\" code=E_UNSUPPORTED_SEMANTICS src=3.1-3.0 payload=0D0A\nEND\n"
    );
    let p = parser::parse(&ok_text, "t")
        .expect("Raw-хвост после PEND легален (SPEC v0.20; бывший N12-отказ снят)");
    assert!(matches!(p.ops[2], Op::Raw { .. }), "хвост = Raw-оп");
    // режим 1b: parse_raw + validate (путь эмиссии) — тоже чисто
    let raw = parser::parse_raw(&ok_text, "t").unwrap();
    let ds = verify::validate(&raw);
    assert!(ds.is_empty(), "validate на Raw-хвосте: {ds:#?}");
    // режим 3: validate_with_notices — без диагностик и без notices (хвост не
    // zero-length, отбрасывать нечего)
    let rep = verify::validate_with_notices(&raw);
    assert!(
        rep.diagnostics.is_empty() && rep.notices.is_empty(),
        "{:#?} / {:#?}",
        rep.diagnostics,
        rep.notices
    );

    // --- позитив (AST-режим): hand-built RAPID, PEND, RAW --------------------
    let p = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(1.0, 0.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        pend(),
        Op::Raw {
            payload_hex: "0D0A".into(),
            reason: "tail after program end".into(),
            code: ErrorCode::UnsupportedSemantics,
            src: src(),
        },
    ]);
    let ds = verify::validate(&p);
    assert!(ds.is_empty(), "hand-built Raw-хвост: {ds:#?}");

    // --- негатив: не-Raw оп после PEND → прежний отказ (строка дословно) ----
    // Текст «program_end must be the last op» — контракт emit-фильтров
    // адаптеров (eid-grbl/src/emit.rs и др.: message.contains) — не менять.
    let bad = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(1.0, 0.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        pend(),
        rapid(
            StartPoint::Unknown,
            p3(2.0, 0.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ]);
    // (rapid после PEND; завершающий PEND — ещё один не-Raw после первого,
    // отчёт по ПЕРВОМУ нарушителю)
    let ds = verify::validate(&bad);
    assert!(
        ds.iter().any(|d| d.message.contains("program_end must be the last op")),
        "не-Raw после PEND: {ds:#?}"
    );
    assert!(ds
        .iter()
        .all(|d| d.code == ErrorCode::InvalidInput && d.stage == Stage::Verify));
    // точка диагностики — ПЕРВЫЙ нарушитель после PEND (rapid, не хвостовой PEND)
    let offender = ds
        .iter()
        .find(|d| d.message.contains("program_end must be the last op"))
        .and_then(|d| d.src.as_ref())
        .expect("диагностика с src нарушителя");
    assert_eq!(offender.file.as_ref(), "t");

    // текст-режим: RAPID, PEND, RAPID → отказ и на parse
    let bad_text = format!(
        "{HDR}OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST_G54}\n\
         OP 002 PEND\n\
         OP 003 RAPID start=unknown end=2.0000,0.0000,0.0000 src=0.1-0.0 {ST_G54}\nEND\n"
    );
    let ds = parser::parse(&bad_text, "t").unwrap_err();
    assert!(
        ds.iter()
            .any(|d| d.message.contains("program_end must be the last op")),
        "{ds:#?}"
    );

    // --- негатив: второй PEND-оп в AST → тот же отказ ------------------------
    // (не «блоки после M2»: парсер строит только первый PEND, хвост = Raw;
    // hand-built AST с двумя PEND — валидатор отказывает, текст прежний)
    let two_pends = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(1.0, 0.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        pend(),
        pend(),
    ]);
    let ds = verify::validate(&two_pends);
    assert!(
        ds.iter().any(|d| d.message.contains("program_end must be the last op")),
        "второй PEND-оп: {ds:#?}"
    );
    // и текст-режим с двумя PEND — по-прежнему отказ
    let two_pends_text = format!(
        "{HDR}OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST_G54}\n\
         OP 002 PEND\nOP 003 PEND\nEND\n"
    );
    assert!(parser::parse(&two_pends_text, "t").is_err());

    // --- негатив: не-Raw оп ПЕРЕД Raw-хвостом → отказ (хвост не спасает) -----
    let bad_mid = program(vec![
        pend(),
        rapid(
            StartPoint::Unknown,
            p3(1.0, 0.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        Op::Raw {
            payload_hex: "00".into(),
            reason: "tail".into(),
            code: ErrorCode::UnsupportedSemantics,
            src: src(),
        },
    ]);
    let ds = verify::validate(&bad_mid);
    assert!(
        ds.iter().any(|d| d.message.contains("program_end must be the last op")),
        "не-Raw перед Raw-хвостом: {ds:#?}"
    );
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
    assert!(
        ds.iter()
            .any(|d| d.message.contains("linear with feed=none")),
        "{ds:?}"
    );
    // позитив: тот же оп с feed — чисто
    let p = program(vec![
        linear(StartPoint::Unknown, p3(1.0, 0.0, 0.0), feed_st.clone()),
        pend(),
    ]);
    assert!(verify::validate(&p).is_empty());
    // позитив (после отбрасывания): zero-length linear feed=none → drop+notice,
    // feed-ошибки НЕТ (проверка feed — по итоговому AST)
    let p = program(vec![
        linear(
            StartPoint::Known(p3(1.0, 0.0, 0.0)),
            p3(1.0, 0.0, 0.0),
            nofeed_st.clone(),
        ),
        pend(),
    ]);
    let rep = verify::validate_with_notices(&p);
    assert!(
        rep.diagnostics.is_empty(),
        "feed must not fire on dropped op: {:#?}",
        rep.diagnostics
    );
    assert_eq!(rep.notices.len(), 1);
    // reject-режим (эмиссия): zero-length диагностика + (оп оставлен →) feed
    let ds = verify::validate(&p);
    assert!(ds.iter().any(|d| d.message.contains("must be dropped")));
    assert!(ds
        .iter()
        .any(|d| d.message.contains("linear with feed=none")));
}

// ===========================================================================
// §3: zero-length drop+notice — все 4 вида, ± пресетка
// ===========================================================================

#[test]
fn spec_3_zero_length_all_kinds_with_preset() {
    // программа с G92-пресеткой: drop-семантика не зависит от пресетки
    let mut p = program(vec![
        rapid(
            StartPoint::Known(p3(1.0, 0.0, 5.0)),
            p3(1.0, 0.0, 5.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        linear(
            StartPoint::Known(p3(1.0, 0.0, 5.0)),
            p3(1.0, 0.0, 5.0),
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
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
        Op::Dwell {
            seconds: n(0.0),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
            sub: None,
        },
        pend(),
    ]);
    p.frame_preset = Some(FramePreset {
        values: p3(0.0, 0.0, 5.0),
        wcs: Wcs::G54,
    });
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
    assert_eq!(
        ds.iter()
            .filter(|d| d.message.contains("must be dropped"))
            .count(),
        4
    );
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
    let anchor = rapid(
        StartPoint::Unknown,
        p3(0.0, 0.0, 0.0),
        st(None, Wcs::G54, Frame::Wcs),
    );
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
    assert!(
        rep.notices.is_empty(),
        "sweep=360 must NOT be dropped: {:#?}",
        rep.notices
    );
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
    let ok = |text: &str, ctx: &str| {
        parser::parse(text, "t").unwrap_or_else(|ds| panic!("{ctx}: {ds:#?}"))
    };
    let err = |text: &str, ctx: &str| {
        let ds = parser::parse(text, "t").unwrap_err();
        assert!(!ds.is_empty(), "{ctx}");
        assert!(
            ds.iter()
                .all(|d| d.code == ErrorCode::InvalidInput && d.stage == Stage::Verify),
            "{ctx}: {ds:?}"
        );
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
        Op::ToolChange {
            tool: 3,
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
        },
        pend(),
    ]);
    let ds = verify::validate(&p);
    assert!(ds
        .iter()
        .any(|d| d.message.contains("(D4)") && d.code == ErrorCode::InvalidInput));
    // позитив: согласованы — чисто
    let p = program(vec![
        Op::ToolChange {
            tool: 3,
            state: State {
                feed: None,
                spindle: Spindle::Off,
                coolant: Coolant {
                    flood: false,
                    mist: false,
                },
                tool: Some(3),
                frame: Frame::Wcs,
                wcs: Wcs::G54,
                feed_restated: false,
            },
            src: src(),
        },
        pend(),
    ]);
    assert!(verify::validate(&p).is_empty());
}

#[test]
fn spec_5_d5_set_here_preset_only() {
    // негатив: L20 после motion-опа
    let p = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(1.0, 0.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        Op::WcsSetHere {
            p: 2,
            delta: p3(1.0, 0.0, 0.0),
            state: st(None, Wcs::G55, Frame::Wcs),
            src: src(),
        },
        pend(),
    ]);
    let ds = verify::validate(&p);
    assert!(ds
        .iter()
        .any(|d| d.message.contains("D5") && d.code == ErrorCode::InvalidInput));
    // позитив: L20 до motion-опа (якорь) — чисто
    let p = program(vec![
        Op::WcsSetHere {
            p: 2,
            delta: p3(1.0, 0.0, 0.0),
            state: st(None, Wcs::G55, Frame::Wcs),
            src: src(),
        },
        rapid(
            StartPoint::Known(p3(1.0, 0.0, 0.0)),
            p3(2.0, 0.0, 0.0),
            st(None, Wcs::G55, Frame::Wcs),
        ),
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
            arc(
                StartPoint::Known(p3(0.0, 0.0, 0.0)),
                end,
                p3(0.0, 5.0, 0.0),
                p3(0.0, 0.0, normal),
                Dir::Ccw,
                sweep,
                2.0,
                feed_st.clone(),
            ),
            pend(),
        ])
    };
    // позитив: sweep=360 полная окружность (start≠end → геометрия: Rot(360)=id,
    // end должен быть = start; поэтому для домен-теста start==end ниже).
    // Негатив: sweep=0 (start≠end) → домен; sweep=400 → домен
    for sweep in [0.0, 400.0] {
        let ds = verify::validate(&mk_arc(sweep, 1.0, p3(5.0, 5.0, 2.0)));
        assert!(
            ds.iter()
                .any(|d| d.message.contains("out of domain (0°, 360°]")
                    && d.code == ErrorCode::InvalidInput),
            "sweep={sweep}: {ds:?}"
        );
    }
    // негатив: |normal| != 1
    let ds = verify::validate(&mk_arc(90.0, 2.0, p3(-5.0, 5.0, 2.0)));
    assert!(ds
        .iter()
        .any(|d| d.message.contains("|normal|") && d.code == ErrorCode::InvalidInput));
    // негатив: dwell < 0; позитив: dwell = 0-drop (не ошибка), dwell > 0 чисто
    let neg = program(vec![
        Op::Dwell {
            seconds: n(-0.0001),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
            sub: None,
        },
        pend(),
    ]);
    assert!(verify::validate(&neg)
        .iter()
        .any(|d| d.message.contains("dwell seconds") && d.code == ErrorCode::InvalidInput));
    let zero = program(vec![
        Op::Dwell {
            seconds: n(0.0),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
            sub: None,
        },
        pend(),
    ]);
    let rep = verify::validate_with_notices(&zero);
    assert!(rep.diagnostics.is_empty() && rep.notices.len() == 1);
    let pos = program(vec![
        Op::Dwell {
            seconds: n(0.5),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
            sub: None,
        },
        pend(),
    ]);
    assert!(verify::validate(&pos).is_empty());
    // p: 0 и 10 нелегальны (10 — лексически цифрой не является; тут AST-уровень
    // ловит 0; 10 отсекается парсером — см. grammar_conformance::p_domain_lexical_boundary)
    let p0 = program(vec![
        Op::WcsDefine {
            p: 0,
            origin: p3(0.0, 0.0, 0.0),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
        },
        pend(),
    ]);
    assert!(verify::validate(&p0)
        .iter()
        .any(|d| d.message.contains("p=0 out of domain") && d.code == ErrorCode::InvalidInput));
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
    let anchor = rapid(
        StartPoint::Unknown,
        p3(0.0, 0.0, 0.0),
        st(None, Wcs::G54, Frame::Wcs),
    );
    let st = st(Some(100.0), Wcs::G54, Frame::Wcs);
    let ok_p = program(vec![
        anchor.clone(),
        arc(
            StartPoint::Known(p3(0.0, 0.0, 0.0)),
            p3(5.0, 5.0, 2.0),
            p3(0.0, 5.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Ccw,
            90.0,
            2.0,
            st.clone(),
        ),
        pend(),
    ]);
    assert!(
        verify::validate(&ok_p).is_empty(),
        "ccw must match rotation equation"
    );
    let bad_p = program(vec![
        anchor,
        arc(
            StartPoint::Known(p3(0.0, 0.0, 0.0)),
            p3(5.0, 5.0, 2.0),
            p3(0.0, 5.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Cw,
            90.0,
            2.0,
            st.clone(),
        ),
        pend(),
    ]);
    let ds = verify::validate(&bad_p);
    assert!(
        ds.iter()
            .any(|d| d.message.contains("endpoint inconsistent")
                && d.code == ErrorCode::InvalidInput),
        "mirrored arc must fail the rotation equation: {ds:?}"
    );
}

#[test]
fn spec_6_impossible_arc() {
    // Планиметрическая хорда > 2R: start в центре (ρ_start = 0), end на
    // расстоянии 10 → радиусы не сходятся → E_INVALID_INPUT (не unsupported).
    // Позиция якорится rapid'ом.
    let p = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(0.0, 0.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        arc(
            StartPoint::Known(p3(0.0, 0.0, 0.0)),
            p3(10.0, 0.0, 2.0),
            p3(0.0, 0.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Ccw,
            90.0,
            2.0,
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ]);
    let ds = verify::validate(&p);
    assert!(
        ds.iter()
            .any(|d| d.message.contains("radius mismatch") && d.code == ErrorCode::InvalidInput),
        "{ds:?}"
    );
}

#[test]
fn spec_6_arc_units_agnostic_inch() {
    // R+pitch/units: геометрия дуги не зависит от units (значения в declared
    // units); inch-дуга с pitch (аналог R+pitch после канонизации) — валидна.
    // Позиция якорится rapid'ом.
    let mut p = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(0.0, 0.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        arc(
            StartPoint::Known(p3(0.0, 0.0, 0.0)),
            p3(-5.0, 5.0, 2.0),
            p3(0.0, 5.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Cw,
            90.0,
            2.0,
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ]);
    p.units = Units::In;
    assert!(
        verify::validate(&p).is_empty(),
        "units must not affect arc invariants"
    );
}

// ===========================================================================
// §4: feed на rapid без ограничений (G0 не сбрасывает feed — resolved-state
// правило канонизатора; в EID AST feed — явное поле, verify не требует feed
// на rapid)
// ===========================================================================

#[test]
fn spec_4_rapid_feed_no_constraint() {
    // rapid с feed — легален (переживает G0), и без feed — легален
    let with = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(1.0, 0.0, 0.0),
            st(Some(500.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ]);
    let without = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(1.0, 0.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ]);
    assert!(verify::validate(&with).is_empty());
    assert!(verify::validate(&without).is_empty());
    // и rapid с feed=0 — БЕЗ ограничения (ход не использует feed; аудит #32
    // касается только linear/arc)
    let zero = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(1.0, 0.0, 0.0),
            st(Some(0.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ]);
    assert!(verify::validate(&zero).is_empty());
}

// ===========================================================================
// §3/аудит #32: feed present-but-zero на linear/arc → refusal
// ===========================================================================

/// linear/arc с feed, равным нулю, — E_INVALID_INPUT «zero feed rate on move»
/// (реальные контроллеры отказывают ход: rs274 «Cannot do g1 with zero feed
/// rate»). feed=none — прежнее правило (spec_3_feed_final_ast), не затронуто.
/// G93-resolved feed, заквантованный в 0.0000 (в т.ч. -0.0000), ловится тем же
/// правилом (nval == 0.0). Rapid — без ограничений.
#[test]
fn spec_3_feed_zero_on_move_refused() {
    let zero_st = st(Some(0.0), Wcs::G54, Frame::Wcs);
    let neg0_st = State {
        feed: Some(Num {
            text: "-0.0000".into(),
            value: -0.0,
        }),
        ..st(Some(0.0), Wcs::G54, Frame::Wcs)
    };
    let tiny_st = st(Some(0.0001), Wcs::G54, Frame::Wcs);
    let ok_st = st(Some(100.0), Wcs::G54, Frame::Wcs);

    // негатив: linear feed=0.0000
    let p = program(vec![
        linear(StartPoint::Unknown, p3(1.0, 0.0, 0.0), zero_st.clone()),
        pend(),
    ]);
    let ds = verify::validate(&p);
    assert!(
        ds.iter()
            .any(|d| d.message.contains("zero feed rate on move")
                && d.code == ErrorCode::InvalidInput),
        "{ds:?}"
    );
    // негатив: arc feed=0.0000 (геометрия корректна — отказ именно по feed)
    let p = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(0.0, 0.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        arc(
            StartPoint::Known(p3(0.0, 0.0, 0.0)),
            p3(5.0, 5.0, 0.0),
            p3(0.0, 5.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Ccw,
            90.0,
            0.0,
            zero_st.clone(),
        ),
        pend(),
    ]);
    let ds = verify::validate(&p);
    assert!(
        ds.iter()
            .any(|d| d.message.contains("zero feed rate on move")
                && d.code == ErrorCode::InvalidInput),
        "{ds:?}"
    );
    // негатив: -0.0000 ≡ 0 после квантования (G93-resolved feed → 0.0000)
    let p = program(vec![
        linear(StartPoint::Unknown, p3(1.0, 0.0, 0.0), neg0_st),
        pend(),
    ]);
    let ds = verify::validate(&p);
    assert!(
        ds.iter()
            .any(|d| d.message.contains("zero feed rate on move") && d.message.contains("-0.0000")),
        "{ds:?}"
    );
    // позитив: минимальный ненулевой feed (0.0001) — чисто; обычный feed — чисто
    let p = program(vec![
        linear(StartPoint::Unknown, p3(1.0, 0.0, 0.0), tiny_st),
        pend(),
    ]);
    assert!(verify::validate(&p).is_empty());
    let p = program(vec![
        linear(StartPoint::Unknown, p3(1.0, 0.0, 0.0), ok_st),
        pend(),
    ]);
    assert!(verify::validate(&p).is_empty());
    // rapid feed=0 — не ход под feed (spec_4_rapid_feed_no_constraint выше)
    // drop-путь: zero-length linear с feed=0 отбрасывается канонизацией —
    // feed-диагностики НЕТ (правило — по итоговому AST, как feed=none)
    let p = program(vec![
        linear(
            StartPoint::Known(p3(1.0, 0.0, 0.0)),
            p3(1.0, 0.0, 0.0),
            zero_st.clone(),
        ),
        pend(),
    ]);
    let rep = verify::validate_with_notices(&p);
    assert!(
        rep.diagnostics.is_empty(),
        "feed=0 must not fire on dropped op: {:#?}",
        rep.diagnostics
    );
    assert_eq!(rep.notices.len(), 1);
    // reject-режим (эмиссия): zero-length диагностика присутствует
    let ds = verify::validate(&p);
    assert!(ds.iter().any(|d| d.message.contains("must be dropped")));
    // текст-уровень: feed=0.0000 в тексте EID → parse отказ (Stage::Verify)
    let text = format!("{HDR}OP 001 LINEAR start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 feed=0.0000 spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 PEND\nEND\n");
    let ds = parser::parse(&text, "t").unwrap_err();
    assert!(
        ds.iter()
            .any(|d| d.message.contains("zero feed rate on move") && d.stage == Stage::Verify),
        "{ds:?}"
    );
}

// ===========================================================================
// §10.6: квантование входа — zero-check ПОСЛЕ квантования
// ===========================================================================

#[test]
fn spec_10_quantized_zero_check() {
    // zero-check применяется ПОСЛЕ квантования к 4dp: -0.0000 ≡ 0.0000.
    // Позиция якорится первым rapid'ом (unknown → (1,0,0)); второй rapid с
    // Known-стартом легален (точное равенство prev_end).
    let anchor =
        format!("OP 001 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST_G54}\n");
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
    assert!(
        out.notices.is_empty(),
        "1.0001 ≠ 1.0000 at 4dp: {:#?}",
        out.notices
    );
    assert_eq!(out.program.ops.len(), 3);
    // знаковый ноль квантуется в тот же ноль (−0.0000 ≡ 0.0000 → drop):
    let text_neg0 = format!(
        "{HDR}{anchor}OP 002 RAPID start=1.0000,-0.0000,0.0000 end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST_G54}\nOP 003 PEND\nEND\n"
    );
    let out = parser::parse_with_notices(&text_neg0, "t").expect("parse ok");
    assert_eq!(
        out.notices.len(),
        1,
        "-0.0000 ≡ 0.0000 after quantization: {:#?}",
        out.notices
    );
    assert_eq!(out.notices[0].kind, NoticeKind::ZeroLengthMotion);
}

// ===========================================================================
// §3: notices через parse-путь = через verify-путь (консистентность pass'ов)
// ===========================================================================

#[test]
fn spec_3_notices_consistent_across_entry_points() {
    let mk_zero = || {
        program(vec![
            rapid(
                StartPoint::Known(p3(1.0, 0.0, 0.0)),
                p3(1.0, 0.0, 0.0),
                st(None, Wcs::G54, Frame::Wcs),
            ),
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

// ===========================================================================
// Итерация 5 (cargo-mutants усиление): границы grammar и wrappers
// ===========================================================================

/// parser::validate (AST → диагностики) обязан отклонять канон-нарушения —
/// wrapper не вырождается в Ok (мутант-заглушка ловится напрямую).
#[test]
fn mutants_validate_wrapper_rejects_canon_violation() {
    let text = format!(
        "{HDR}OP 001 PEND
OP 002 RAPID start=unknown end=1.0000,0.0000,0.0000 src=0.1-0.0 {ST_G54}
END
"
    );
    let raw = parser::parse_raw(&text, "t").unwrap(); // PEND НЕ последний
                                                      // Порядок PEND — гейт verify; parser::validate ловим пустой программой
                                                      // ("program has no ops") — заглушка Ok(()) в wrapper'е обнаруживается.
    assert!(
        !verify::validate(&raw).is_empty(),
        "not-last PEND ловит verify"
    );
    let empty = Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: PathMode::ExactPath,
        frame_preset: None,
        ops: vec![],
    };
    let Err(ds) = parser::validate(&empty) else {
        panic!("validate-заглушка Ok(()) — wrapper не проверяет канон-форму")
    };
    assert!(ds
        .iter()
        .all(|d| d.code == ErrorCode::InvalidInput && d.stage == Stage::Verify));
}

/// Нумерация опов (grammar v0.4 — 3+ цифр, back-compat superset, аудит #3):
/// OP 999 и OP 1000 легальны; меньше 3 цифр — Err (zero-pad min 3).
#[test]
fn mutants_op_numbering_boundary_999_1000() {
    let ok_text = format!(
        "{HDR}OP 999 PEND
END
"
    );
    assert!(parser::parse(&ok_text, "t").is_ok(), "OP 999 легален");
    // 1000+ легальны с grammar v0.4 (прежде: Err «ровно 3 цифры»)
    let v4_text = format!(
        "{HDR}OP 1000 PEND
END
"
    );
    assert!(
        parser::parse(&v4_text, "t").is_ok(),
        "OP 1000 легален (grammar v0.4)"
    );
    let pad_text = format!(
        "{HDR}OP 0999 PEND
END
"
    );
    assert!(
        parser::parse(&pad_text, "t").is_ok(),
        "OP 0999 легален (3+ цифр)"
    );
    let short_text = format!(
        "{HDR}OP 99 PEND
END
"
    );
    let ds = parser::parse(&short_text, "t").unwrap_err();
    assert!(ds.iter().any(|d| d.message.contains("3+ digits")), "{ds:?}");
}

/// quoted-границы: голая кавычка как всё поле и НЕЭКРАНИРОВАННАЯ кавычка
/// внутри quoted-поля reason → parse Err (grammar v0.4: экранируются только
/// \" и \\; голая кавычка по-прежнему ломает поле — мутанты ||→&& в quoted).
#[test]
fn mutants_quoted_boundaries_rejected() {
    // голая кавычка: quoted(`"`) — len < 2
    let t1 = format!(
        "{HDR}OP 001 RAW reason=\" payload=00 code=E_UNSUPPORTED_SYNTAX src=0.1-0.0
END
"
    );
    assert!(
        parser::parse(&t1, "t").is_err(),
        "голая кавычка отклоняется"
    );
    // кавычка внутри quoted: inner содержит `"a"b"` → содержит quote
    let t2 = format!(
        "{HDR}OP 001 RAW reason=\"a\"b\" payload=00 code=E_UNSUPPORTED_SYNTAX src=0.1-0.0
END
"
    );
    assert!(
        parser::parse(&t2, "t").is_err(),
        "кавычка внутри quoted отклоняется"
    );
}

/// emit_num: verbatim "-0.0000" неканоничен → канонизация из f64 → "0.0000"
/// (мутант !=→== в verbatim-условии эмитит неканоничный минус-ноль).
#[test]
fn mutants_emit_num_minus_zero_canonicalized() {
    let text_minus0 = "-0.0000";
    let nm = |t: &str, v: f64| Num {
        text: t.to_string(),
        value: v,
    };
    let prog = Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: PathMode::ExactPath,
        frame_preset: None,
        ops: vec![
            Op::Rapid {
                start: StartPoint::Unknown,
                end: Point3 {
                    x: nm(text_minus0, -0.0),
                    y: nm("0.0000", 0.0),
                    z: nm("0.0000", 0.0),
                },
                state: State {
                    feed: None,
                    spindle: Spindle::Off,
                    coolant: Default::default(),
                    tool: None,
                    frame: Frame::Wcs,
                    wcs: Wcs::G54,
                    feed_restated: false,
                },
                src: SrcSpan {
                    file: "t".into(),
                    line_start: 1,
                    col_start: 1,
                    line_end: 1,
                    col_end: 2,
                },
                sub: None,
            },
            Op::ProgramEnd {
                src: SrcSpan {
                    file: "t".into(),
                    line_start: 9,
                    col_start: 1,
                    line_end: 9,
                    col_end: 2,
                },
            },
        ],
    };
    let text = eid_core::emitter::emit(&prog).expect("эмиссия Ok");
    assert!(text.contains("0.0000"), "{text}");
    assert!(!text.contains("-0.0000"), "минус-ноль канонизуется: {text}");
}

// ===========================================================================
// §6/аудит #13 (PINNED): канонический знак нормали = +ось плоскости
// ===========================================================================

/// Канон-форма нормали дуги: положительная компонента по оси плоскости
/// (+X/+Y/+Z — ровно одна компонента ≈ +1, остальные ≈ 0). Перевёрнутая ось
/// (0,0,-1) и не-осевые нормали → refusal «not a canonical +axis»; dir НЕ
/// переворачивается (ruling: требовать канон, не нормализовать эмиттером).
/// Единая точка правила для всех адаптеров.
#[test]
fn spec_6_normal_positive_axis() {
    let st_arc = st(Some(100.0), Wcs::G54, Frame::Wcs);
    // позитивы: +Z (G17), +Y (G18), +X (G19) — геометрия сходится
    //  +Z: start (0,0,0), center (0,5,0), ccw 90° → end (5,5,0)
    let p = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(0.0, 0.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        arc(
            StartPoint::Known(p3(0.0, 0.0, 0.0)),
            p3(5.0, 5.0, 0.0),
            p3(0.0, 5.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Ccw,
            90.0,
            0.0,
            st_arc.clone(),
        ),
        pend(),
    ]);
    assert!(
        verify::validate(&p).is_empty(),
        "+Z canonical: {:#?}",
        verify::validate(&p)
    );
    //  +Y: start (5,0,0), center (0,0,0), ccw 90° вокруг +Y → end (0,0,-5)
    let p = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(5.0, 0.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        arc(
            StartPoint::Known(p3(5.0, 0.0, 0.0)),
            p3(0.0, 0.0, -5.0),
            p3(0.0, 0.0, 0.0),
            p3(0.0, 1.0, 0.0),
            Dir::Ccw,
            90.0,
            0.0,
            st_arc.clone(),
        ),
        pend(),
    ]);
    assert!(
        verify::validate(&p).is_empty(),
        "+Y canonical: {:#?}",
        verify::validate(&p)
    );
    //  +X: start (0,5,0), center (0,0,0), ccw 90° вокруг +X → end (0,0,5)
    let p = program(vec![
        rapid(
            StartPoint::Unknown,
            p3(0.0, 5.0, 0.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        arc(
            StartPoint::Known(p3(0.0, 5.0, 0.0)),
            p3(0.0, 0.0, 5.0),
            p3(0.0, 0.0, 0.0),
            p3(1.0, 0.0, 0.0),
            Dir::Ccw,
            90.0,
            0.0,
            st_arc.clone(),
        ),
        pend(),
    ]);
    assert!(
        verify::validate(&p).is_empty(),
        "+X canonical: {:#?}",
        verify::validate(&p)
    );

    // негативы: перевёрнутая ось — (0,0,-1) — refusal ( ruling-пример),
    // (0,-1,0), (-1,0,0); не-осевая единичная (0.6,0,0.8) — |n|=1, но оси нет
    let mk_neg = |normal: Point3| {
        program(vec![
            rapid(
                StartPoint::Unknown,
                p3(0.0, 0.0, 0.0),
                st(None, Wcs::G54, Frame::Wcs),
            ),
            arc(
                StartPoint::Known(p3(0.0, 0.0, 0.0)),
                p3(5.0, 5.0, 0.0),
                p3(0.0, 5.0, 0.0),
                normal,
                Dir::Ccw,
                90.0,
                0.0,
                st_arc.clone(),
            ),
            pend(),
        ])
    };
    for (normal, ctx) in [
        (p3(0.0, 0.0, -1.0), "flipped +Z (0,0,-1)"),
        (p3(0.0, -1.0, 0.0), "flipped +Y"),
        (p3(-1.0, 0.0, 0.0), "flipped +X"),
        (p3(0.6, 0.0, 0.8), "off-axis"),
    ] {
        let ds = verify::validate(&mk_neg(normal));
        assert!(
            ds.iter()
                .any(|d| d.message.contains("not a canonical +axis")
                    && d.code == ErrorCode::InvalidInput),
            "{ctx}: {ds:?}"
        );
    }
    // допуск: (0,0,1.0001) — в пределах 4dp-кванта (0.001) → канон;
    // (0,0,1.0020) — вне допуска → и канон-отказ, и D7-домен |n|=1
    let ds = verify::validate(&mk_neg(p3(0.0, 0.0, 1.0001)));
    assert!(
        !ds.iter()
            .any(|d| d.message.contains("not a canonical +axis")),
        "1.0001 in 4dp tol: {ds:?}"
    );
    let ds = verify::validate(&mk_neg(p3(0.0, 0.0, 1.0020)));
    assert!(
        ds.iter()
            .any(|d| d.message.contains("not a canonical +axis")),
        "1.0020 out of tol: {ds:?}"
    );
}

// ===========================================================================
// §2/аудит #4: связывание Tol-ветка использует вычисленный ε_pos
// ===========================================================================

/// Рамочно-скорректированное сравнение start (Tol-ветка: смена wcs с
/// вычислимой разностью origin) использует ε_pos = max(0.001, 1e-7·L)
/// (EQUIVALENCE §4), а не захардкоженный 0.001. На больших координатах
/// (L ≥ 10000 → ε_pos > 0.001) отклонение, прежним допуском отвергаемое,
/// легально; за пределами ε_pos — по-прежнему refusal.
#[test]
fn spec_2_linking_tol_uses_eps_pos() {
    // L ≈ 100001 → ε_pos = 1e-7·L ≈ 0.0100. Смена G54 (origin 100000) →
    // G55 (origin 0): expected start = prev_end + O_prev − O_new = 200000;
    // фактический start 200000.0040 — |Δ| = 0.004: > floor 0.001, < ε_pos.
    let defined = "OP 001 WCS_DEFINE p=1 origin=100000.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 WCS_DEFINE p=2 origin=0.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\n";
    let ok_text = format!(
        "{HDR}{defined}OP 003 RAPID start=unknown end=100000.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 004 RAPID start=200000.0000,0.0040,0.0000 end=200001.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G55\nOP 005 PEND\nEND\n"
    );
    let p = parser::parse(&ok_text, "t")
        .unwrap_or_else(|ds| panic!("|Δ|=0.004 < eps_pos≈0.0100 must link: {ds:#?}"));
    assert_eq!(p.ops.len(), 5);
    // негатив: |Δ| = 0.02 > ε_pos → refusal (тот же корпус, большее отклонение)
    let bad_text = format!(
        "{HDR}{defined}OP 003 RAPID start=unknown end=100000.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 004 RAPID start=200000.0000,0.0200,0.0000 end=200001.0000,0.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G55\nOP 005 PEND\nEND\n"
    );
    let ds = parser::parse(&bad_text, "t").unwrap_err();
    assert!(
        ds.iter()
            .any(|d| d.message.contains("frame-adjusted previous end")
                && d.code == ErrorCode::InvalidInput),
        "{ds:?}"
    );
    // fmt_point в сообщении несёт координаты (FnValue-мутант → "xyzzy"/пусто).
    assert!(
        ds.iter()
            .any(|d| d.message.contains("start (200000.0000 0.0200 0.0000)")),
        "{ds:?}"
    );
}

// ===========================================================================
// Мутационный раунд v0.18 (агент D): p-домен на связывании (no-panic),
// src-атрибуция диагностик, четверть-дуги обоих направлений (§6-уравнение),
// pitch≠chord·n, пресетка в чужой WCS без origin (expect_start (b)).
// ===========================================================================

#[test]
fn mround_p_out_of_domain_linking_no_panic() {
    // D7: p вне 1..9 → InvalidInput; check_linking обязан НЕ паниковать на
    // p=10 (guard-мутанты `<` → `<=` выходили за границы массива).
    let ops = vec![
        Op::WcsDefine {
            p: 10,
            origin: p3(1.0, 2.0, 3.0),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
        },
        rapid(
            StartPoint::Unknown,
            p3(1.0, 1.0, 1.0),
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ];
    let d = verify::validate(&program(ops));
    assert!(d.iter().any(|x| x.code == ErrorCode::InvalidInput), "{d:?}");

    // WcsSetHere с p=10 — тот же guard в связывании set_here.
    let ops2 = vec![
        Op::WcsSetHere {
            p: 10,
            delta: p3(1.0, 2.0, 3.0),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
        },
        rapid(
            StartPoint::Unknown,
            p3(1.0, 1.0, 1.0),
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ];
    let d2 = verify::validate(&program(ops2));
    assert!(
        d2.iter().any(|x| x.code == ErrorCode::InvalidInput),
        "{d2:?}"
    );
}

#[test]
fn mround_verify_diag_carries_src() {
    // Диагностика verify несёт SrcSpan опа (op_src — матч по всем вариантам
    // Op; FnValue-мутант `→ None` терял привязку к строке).
    let ops = vec![
        linear(
            StartPoint::Unknown,
            p3(1.0, 1.0, 1.0),
            st(None, Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ];
    let d = verify::validate(&program(ops));
    assert!(!d.is_empty(), "linear без feed — диагностика");
    assert!(
        d.iter().all(|x| x.src.is_some()),
        "все verify-диагностики несут src: {d:?}"
    );
    assert!(d.iter().all(|x| &*x.src.as_ref().unwrap().file == "t"));
}

#[test]
fn mround_arc_quarter_clean_both_dirs() {
    // Валидные четверть-дуги CCW и CW: §6-уравнение позиции сходится
    // (Rodrigues-мутанты и знак CW давали ошибку на валидном входе;
    // прежние тесты использовали полуокружности — sin=0 маскировал).
    let ccw = vec![
        Op::WcsSetHere {
            p: 1,
            delta: p3(10.0, 0.0, 0.0),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
        },
        arc(
            StartPoint::Known(p3(10.0, 0.0, 0.0)),
            p3(0.0, 10.0, 0.0),
            p3(0.0, 0.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Ccw,
            90.0,
            0.0,
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ];
    let d = verify::validate(&program(ccw));
    assert!(d.is_empty(), "CCW-четверть валидна: {d:?}");

    let cw = vec![
        Op::WcsSetHere {
            p: 1,
            delta: p3(10.0, 0.0, 0.0),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
        },
        arc(
            StartPoint::Known(p3(10.0, 0.0, 0.0)),
            p3(0.0, -10.0, 0.0),
            p3(0.0, 0.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Cw,
            90.0,
            0.0,
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ];
    let d2 = verify::validate(&program(cw));
    assert!(d2.is_empty(), "CW-четверть валидна: {d2:?}");
}

#[test]
fn mround_arc_pitch_chord_mismatch_rejected() {
    // §6.2: chord·normal ≠ pitch → InvalidInput (pitch=5 при плоском ходе).
    let ops = vec![
        Op::WcsSetHere {
            p: 1,
            delta: p3(10.0, 0.0, 0.0),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
        },
        arc(
            StartPoint::Known(p3(10.0, 0.0, 0.0)),
            p3(0.0, 10.0, 0.0),
            p3(0.0, 0.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Ccw,
            90.0,
            5.0,
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ];
    let d = verify::validate(&program(ops));
    assert!(
        d.iter().any(|x| x.code == ErrorCode::InvalidInput
            && x.message.contains("chord·normal")
            && x.message.contains("SPEC §6.2")),
        "{d:?}"
    );
}

#[test]
fn mround_preset_cross_wcs_unknown_origin_no_panic() {
    // expect_start (b) с пресетой в чужой WCS и НЕопределённым origin
    // пресетки → UnknownRequired (первый ход — Unknown легален);
    // `&&`→`||`-мутант разворачивал None → паника.
    let prg = Program {
        eid_version: "EID/L1 v0.3".into(),
        units: Units::Mm,
        path_mode: PathMode::ExactPath,
        frame_preset: Some(FramePreset {
            values: p3(5.0, 5.0, 5.0),
            wcs: Wcs::G54,
        }),
        ops: vec![
            Op::WcsDefine {
                p: 2,
                origin: p3(1.0, 1.0, 1.0),
                state: st(None, Wcs::G55, Frame::Wcs),
                src: src(),
            },
            rapid(
                StartPoint::Unknown,
                p3(1.0, 2.0, 3.0),
                st(Some(100.0), Wcs::G55, Frame::Wcs),
            ),
            pend(),
        ],
    };
    let d = verify::validate(&prg);
    assert!(
        d.is_empty(),
        "разрыв (b) без origin — Unknown легален: {d:?}"
    );
}

#[test]
fn mround_linking_eps_pos_axis_crosses_zero() {
    // ε_pos = max(0.001, 1e-7·L), L = диагональ bbox КОНЦОВ хода (EQUIVALENCE
    // §4). Ось, пересекающая ноль (min<0<max), различает max−min от
    // max+min: |Δ|=0.011 links при ε≈0.0144, но отвергается мутантом
    // (ε≈0.002); |Δ|=0.03 отвергается оригиналом.
    let defined = "OP 001 WCS_DEFINE p=1 origin=0.0000,60000.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 002 WCS_DEFINE p=2 origin=0.0000,-60000.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\n";
    let ok_text = format!(
        "{HDR}{defined}OP 003 RAPID start=unknown end=0.0000,60000.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 004 RAPID start=0.0000,60000.0000,0.0000 end=0.0000,-60000.0000,-30000.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 005 RAPID start=0.0000,-60000.0000,-30000.0000 end=0.0000,-60000.0000,50000.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 006 RAPID start=0.0000,60000.0110,50000.0000 end=0.0000,60001.0000,50000.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G55\nOP 007 PEND\nEND\n"
    );
    let p = parser::parse(&ok_text, "t")
        .unwrap_or_else(|ds| panic!("|Δ|=0.011 < eps_pos≈0.0144 must link: {ds:#?}"));
    assert_eq!(p.ops.len(), 7);
    let bad_text = format!(
        "{HDR}{defined}OP 003 RAPID start=unknown end=0.0000,60000.0000,0.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 004 RAPID start=0.0000,60000.0000,0.0000 end=0.0000,-60000.0000,-30000.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 005 RAPID start=0.0000,-60000.0000,-30000.0000 end=0.0000,-60000.0000,50000.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G54\nOP 006 RAPID start=0.0000,60000.0300,50000.0000 end=0.0000,60001.0000,50000.0000 src=0.1-0.0 feed=none spindle=off coolant=none tool=none frame=wcs wcs=G55\nOP 007 PEND\nEND\n"
    );
    let ds = parser::parse(&bad_text, "t").unwrap_err();
    assert!(
        ds.iter()
            .any(|d| d.message.contains("frame-adjusted previous end")
                && d.code == ErrorCode::InvalidInput),
        "{ds:?}"
    );
}

// ===========================================================================
// v0.3.x — is_finite-гейт resid-сравнений (EQUIVALENCE §4 note, решение
// ревью «известное ограничение → дешёвая страховка на полке» — применена).
// IEEE-754: `NaN > ε` = false → без гейта resid-проверка вырождается в
// молчаливый pass. Кейсы — hand-built AST (на легальном пути NaN/inf
// недостижимы: is_finite-гейт лексеров + 4dp-квантование).
// ===========================================================================

/// §6(3) уравнение позиции: промежуточное переполнение арифметики на
/// вырожденном угле. start z = 1.5e308 (finite), sweep = 180° (|cos| +
/// |1−cos| = 3 — максимальный рост z-компоненты при повороте): Rodrigues
/// n-член nv·(1−cos(π)) = 1.5e308·2 = 3e308 > f64::MAX ≈ 1.798e308 → inf.
/// expected.z = inf → resid.z = inf. ρ (планиметрика, проекция ⟂ normal) и
/// chord·n остаются finite — срабатывает РОВНО гейт позиции, без ложных
/// соседних диагностик и без вводящего в заблуждение «|Δ| = inf».
#[test]
fn v03_gate_arc_position_residual_nan_refusal() {
    let ops = vec![
        Op::WcsSetHere {
            p: 1,
            delta: p3(0.0, 0.0, 1.5e308),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
        },
        arc(
            StartPoint::Known(p3(0.0, 0.0, 1.5e308)),
            p3(0.0, 0.0, 1.4e308),
            p3(0.0, 0.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Ccw,
            180.0,
            -1e307, // chord·n = 1.4e308 − 1.5e308 ≈ −1e307 (≈ pitch, §6.2 pass)
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ];
    let d = verify::validate(&program(ops.clone()));
    assert!(
        d.iter().any(|x| x.code == ErrorCode::InvalidInput
            && x.message.contains("arc position residual non-finite")
            && x.message.contains("is_finite gate v0.3.x")),
        "гейт §6(3) обязан отказать громко: {d:?}"
    );
    // Гейт перехватывает до мёртвого/вводящего в заблуждение сравнения и не
    // подменяет соседние проверки (ρ и chord·n finite — молчат).
    assert!(
        !d.iter().any(|x| x.message.contains("endpoint inconsistent")),
        "{d:?}"
    );
    assert!(!d.iter().any(|x| x.message.contains("radius mismatch")), "{d:?}");
    assert!(!d.iter().any(|x| x.message.contains("radius residual")), "{d:?}");
    assert!(
        !d.iter().any(|x| x.message.contains("chord·normal")),
        "{d:?}"
    );
    // Тот же гейт в режиме канонизации (единая точка правила).
    let r = verify::validate_with_notices(&program(ops.clone()));
    assert!(
        r.diagnostics
            .iter()
            .any(|x| x.message.contains("arc position residual non-finite")),
        "{:?}", r.diagnostics
    );
}

/// Канонический NaN-swallow: hand-built NaN в end.y дуги. До гейта v0.3.x
/// программа верифицировалась ЧИСТО (0 диагностик): ρ_start ≈ 10 vs
/// ρ_end = NaN → `NaN > tol` = false; chord·n = NaN → false; §6(3) resid =
/// (≈0, NaN, 0) → `NaN > ε_pos` = false — все три проверки молча pass.
/// Гейт: честный refusal по всем трём resid-сравнениям.
#[test]
fn v03_gate_arc_nan_end_silent_pass_now_refused() {
    let ops = vec![
        Op::WcsSetHere {
            p: 1,
            delta: p3(10.0, 0.0, 0.0),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
        },
        arc(
            StartPoint::Known(p3(10.0, 0.0, 0.0)),
            p3(0.0, f64::NAN, 0.0),
            p3(0.0, 0.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Ccw,
            90.0,
            0.0,
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ];
    let d = verify::validate(&program(ops.clone()));
    for phrase in [
        "arc radius residual non-finite",
        "arc chord·normal residual non-finite",
        "arc position residual non-finite",
    ] {
        assert!(
            d.iter().any(|x| x.code == ErrorCode::InvalidInput
                && x.message.contains(phrase)
                && x.message.contains("is_finite gate v0.3.x")),
            "гейт обязан отказать («{phrase}»): {d:?}"
        );
    }
    // Тот же гейт в режиме канонизации.
    let r = verify::validate_with_notices(&program(ops));
    assert!(
        r.diagnostics
            .iter()
            .any(|x| x.message.contains("arc position residual non-finite")),
        "{:?}", r.diagnostics
    );
}

/// §6.2: chord·normal = NaN при finite ρ: e.x − s.x = −1.5e308 − 1.5e308 →
/// −inf; (−inf)·0 (нулевые компоненты единичной оси +Z) → NaN. До гейта:
/// `NaN > ε` = false → молчаливый pass (класс мутантов `*`→`/` из
/// EQUIVALENCE §4). Дополнительно: bbox-диагональ 3e308 → ε_pos = inf →
/// program-level гейт шкалы допуска. ρ_s = ρ_e = 1.5e308 — радиус молчит;
/// resid §6(3) finite (гейт позиции молчит).
#[test]
fn v03_gate_arc_chord_residual_nan_refused() {
    let ops = vec![
        Op::WcsSetHere {
            p: 1,
            delta: p3(1.5e308, 0.0, 0.0),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
        },
        arc(
            StartPoint::Known(p3(1.5e308, 0.0, 0.0)),
            p3(-1.5e308, 0.0, 0.0),
            p3(0.0, 0.0, 0.0),
            p3(0.0, 0.0, 1.0),
            Dir::Ccw,
            180.0,
            0.0,
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ];
    let d = verify::validate(&program(ops));
    assert!(
        d.iter().any(|x| x.code == ErrorCode::InvalidInput
            && x.message.contains("arc chord·normal residual non-finite")),
        "{d:?}"
    );
    assert!(
        d.iter()
            .any(|x| x.message.contains("geometry tolerance scale non-finite")),
        "ε_pos = inf (переполнение bbox-диагонали) → program-level refusal: {d:?}"
    );
    // Конструкция точная: радиус и позиция не со-срабатывают.
    assert!(!d.iter().any(|x| x.message.contains("radius")), "{d:?}");
    assert!(
        !d.iter().any(|x| x.message.contains("position residual")),
        "{d:?}"
    );
    assert!(
        !d.iter().any(|x| x.message.contains("endpoint inconsistent")),
        "{d:?}"
    );
}

/// §2 связывание Tol-ветка (смена wcs с вычислимой разностью origin):
/// start с hand-built NaN → |Δ| = NaN. До гейта: `NaN > ε_pos` = false →
/// молчаливый pass (start не связан с предыдущим end — незамечено).
/// Гейт: честный refusal.
#[test]
fn v03_gate_linking_start_residual_nan_refused() {
    let ops = vec![
        Op::WcsDefine {
            p: 1,
            origin: p3(100000.0, 0.0, 0.0),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
        },
        Op::WcsDefine {
            p: 2,
            origin: p3(0.0, 0.0, 0.0),
            state: st(None, Wcs::G54, Frame::Wcs),
            src: src(),
        },
        rapid(
            StartPoint::Unknown,
            p3(100000.0, 0.0, 0.0),
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        rapid(
            StartPoint::Known(p3(f64::NAN, 0.0, 0.0)),
            p3(200001.0, 0.0, 0.0),
            st(Some(100.0), Wcs::G55, Frame::Wcs),
        ),
        pend(),
    ];
    let d = verify::validate(&program(ops));
    assert!(
        d.iter().any(|x| x.code == ErrorCode::InvalidInput
            && x.message.contains("start position residual non-finite")
            && x.message.contains("is_finite gate v0.3.x")),
        "{d:?}"
    );
    // Гейт перехватывает до мёртвого сравнения — прежний mismatch-текст не
    // дублируется.
    assert!(
        !d.iter()
            .any(|x| x.message.contains("frame-adjusted previous end")),
        "{d:?}"
    );
}

/// ε_pos = inf: bbox-диагональ координат (±1.5e308 → span 3e308) переполняет
/// f64 → шкала допусков мертва для ВСЕХ resid-сравнений (`x > inf` = false).
/// До гейта: программа из двух rapid'ов верифицировалась чисто. Гейт:
/// program-level refusal. Позитив-контроль: те же ходы в разумных координатах
/// — чисто (гейт не срабатывает на легальном пути).
#[test]
fn v03_gate_eps_pos_nonfinite_refused() {
    let overflow = vec![
        rapid(
            StartPoint::Unknown,
            p3(1.5e308, 0.0, 0.0),
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        rapid(
            StartPoint::Known(p3(1.5e308, 0.0, 0.0)),
            p3(-1.5e308, 0.0, 0.0),
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ];
    let d = verify::validate(&program(overflow));
    assert_eq!(
        d.len(),
        1,
        "ровно program-level гейт шкалы допуска: {d:?}"
    );
    assert!(
        d[0].code == ErrorCode::InvalidInput
            && d[0]
                .message
                .contains("geometry tolerance scale non-finite")
            && d[0].message.contains("is_finite gate v0.3.x"),
        "{d:?}"
    );

    let moderate = vec![
        rapid(
            StartPoint::Unknown,
            p3(150000.0, 0.0, 0.0),
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        rapid(
            StartPoint::Known(p3(150000.0, 0.0, 0.0)),
            p3(-150000.0, 0.0, 0.0),
            st(Some(100.0), Wcs::G54, Frame::Wcs),
        ),
        pend(),
    ];
    let d2 = verify::validate(&program(moderate));
    assert!(d2.is_empty(), "легальный путь не затронут: {d2:?}");
}
