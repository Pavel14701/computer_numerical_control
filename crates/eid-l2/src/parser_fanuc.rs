//! ПАРСЕР L2 — FANUC Custom Macro B (этап 12.1, агент B2).
//! Контракт (зафиксирован оркестратором, сигнатура не меняется):
//!
//! ```text
//! pub fn parse_program(src: &str) -> Result<L2Program, Vec<L2Error>>
//! ```
//!
//! Реализация — общий Macro B core [`crate::parser_macro_common`] с
//! Fanuc-диалектной надстройкой:
//! * карта переменных: #1–#33 (локальные), #100–#999 (глобальные: 100–199
//!   volatile, 500–999 power-off), #4001–#4030 → `SysVar::ModalGroup`,
//!   #5021–#5026/#5041–#5046/#5061–#5066 → `AbsMachinePos`/`WorkPos`/
//!   `SkipPos`, #52xx → `SysVar::WcsOffset` (#5201 — external p=0,
//!   #5221 = G54 … #5321 = G59), прочее → `SysVar::VendorSpecific`;
//! * G65/G66 (modal) / G67 / M98 (слип `P52000` = 5×O2000) / M99;
//! * O#### тела (после первого O-заголовка) → `L2Program.subs`;
//! * bare-IF БЕЗ THEN — не Fanuc-синтаксис → ошибка (у Haas — wrap,
//!   правило 3 рулинга); M97 — не Fanuc → носитель;
//! * вне §2.4 (G31-пробинг, G10/G28/G53/G92, циклы G70–G89, vendor M,
//!   state-only кадры) → честный L1Op-носитель (`Op::Raw`, lossless hex
//!   строки; правило 5 рулинга «вне §2.4, Raw-носитель»).
//!
//! Ошибки — `Vec<L2Error>`; классы см. шапку parser_macro_common
//! (RuntimeError/E_RUNTIME_STATE — отклонение от E_INVALID_INPUT из
//! заготовки агента A, расширение `L2Error` вне зоны B2; стоп-отчёт).

use crate::ast::{L2Program, SysVar, VarRef};
use crate::eval::L2Error;
use crate::parser_macro_common::{map_common_system, parse_macro_b, DialectCfg, DwellSemantics};

/// Fanuc-карта номера макро-переменной → [`VarRef`] (§2.2).
pub fn fanuc_map_var(n: u32) -> VarRef {
    match n {
        // Локальные #1–#33; глобальные #100–#199 (volatile), #500–#999
        // (power-off); #200–#499 — Fanuc держит свободными → Direct.
        1..=999 => VarRef::Direct(n),
        _ => map_common_system(n).unwrap_or(VarRef::Sys(SysVar::VendorSpecific(n))),
    }
}

/// Конфигурация диалекта FANUC Macro B (см. модульную шапку).
pub fn fanuc_cfg() -> DialectCfg {
    DialectCfg {
        name: "fanuc-MB",
        allow_bare_if: false,
        allow_m97: false,
        mirror_g101: false,
        // BUG-D2 (амендрмент v0.1.2): Fanuc G4 P = МИЛЛИсекунды всегда
        // (P2000 = 2 s), X/U = секунды.
        dwell: DwellSemantics::Fanuc,
        map_var: fanuc_map_var,
    }
}

/// Разобрать программу FANUC в L2 AST (контракт — см. модульную шапку).
pub fn parse_program(src: &str) -> Result<L2Program, Vec<L2Error>> {
    parse_macro_b(&fanuc_cfg(), src)
}

// ===========================================================================
// Тесты: golden-пары из целевых файлов ценза (fanuc-MB ×7) + конструкции
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        BinaryOp, Expr, ExprPose, L2Stmt, MoveKind, SubTarget, SysVar, UnaryOp, VarRef,
    };
    use eid_core::ast::{Dir, Op};

    fn corpus(rel: &str) -> String {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/tier_b/");
        std::fs::read_to_string(format!("{path}{rel}"))
            .unwrap_or_else(|e| panic!("corpus file {rel} not found: {e}"))
    }

    fn p(src: &str) -> crate::ast::L2Program {
        parse_program(src).unwrap()
    }

    fn v(n: u32) -> Expr {
        Expr::var(n)
    }
    fn bin(op: BinaryOp, a: Expr, b: Expr) -> Expr {
        Expr::Binary(op, Box::new(a), Box::new(b))
    }

    // ---------- ГЛАДКОЕ ЗОЛОТО: все 7 fanuc-MB файлов ценза парсятся Ok ----

    #[test]
    fn golden_mpe_g65_macro_deep_drill_parses_ok() {
        let src = corpus("inbox/mpe_g65_macro_deep_drill.nc");
        let prog = parse_program(&src).unwrap();
        // Assign ×несколько, IfGoto (#0-тесты), G1-кадры, M99, N-метки.
        let assigns = prog
            .stmts
            .iter()
            .filter(|s| matches!(s, L2Stmt::Assign { .. }))
            .count();
        let if_gotos = prog
            .stmts
            .iter()
            .filter(|s| matches!(s, L2Stmt::IfGoto { .. }))
            .count();
        let moves = prog
            .stmts
            .iter()
            .filter(|s| matches!(s, L2Stmt::Move { .. }))
            .count();
        assert!(assigns >= 5, "assigns: {assigns}");
        assert!(if_gotos >= 5, "if_gotos: {if_gotos}");
        assert!(moves >= 3);
    }

    #[test]
    fn golden_mpe_g65_macro_material_verify_parses_ok() {
        let src = corpus("inbox/mpe_g65_macro_material_verify.nc");
        let prog = parse_program(&src).unwrap();
        // G65 P9832 / P9810 / P9811 / P9833 вызовы.
        let calls: Vec<&L2Stmt> = prog
            .stmts
            .iter()
            .filter(|s| matches!(s, L2Stmt::CallSub { .. }))
            .collect();
        assert!(calls.len() >= 10, "calls: {}", calls.len());
        // G65 P9810 X[#24 + [-#21]] — вложенный унарный минус в аргументе.
        let nested = calls.iter().any(|s| match s {
            L2Stmt::CallSub { target, args, .. } => {
                *target == SubTarget::ONumber(9810)
                    && args.iter().any(|(l, e)| {
                        *l == b'X'
                            && *e
                                == bin(
                                    BinaryOp::Add,
                                    v(24),
                                    Expr::Unary(UnaryOp::Neg, Box::new(v(21))),
                                )
                    })
            }
            _ => false,
        });
        assert!(nested, "G65 P9810 X[#24 + [-#21]] expected");
        // IF[#148 NE 0] GOTO911 (PATH OBSTRUCTION) ×несколько.
        let ne_gotos = prog.stmts.iter().any(|s| {
            matches!(
                s,
                L2Stmt::IfGoto { cond: Expr::Binary(BinaryOp::Ne, a, b), target_label: 911, .. }
                if **a == v(148) && matches!(**b, Expr::Const(ref n) if n.value() == 0.0)
            )
        });
        assert!(ne_gotos);
    }

    #[test]
    fn golden_mpe_g65_macro_centerline_probe_parses_ok() {
        let src = corpus("inbox/mpe_g65_macro_centerline_probe.nc");
        let prog = parse_program(&src).unwrap();
        // Безусловный GOTO70/GOTO3000 — IfGoto{cond=1}.
        let uncond = prog
            .stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    L2Stmt::IfGoto {
                        cond: Expr::Const(_),
                        ..
                    }
                )
            })
            .count();
        assert!(uncond >= 4, "unconditional GOTO: {uncond}");
        // G65 P9812 X#24 Z[#26 + #18] S#19 — 4 аргумента.
        let with_s = prog.stmts.iter().any(|s| {
            matches!(
                s,
                L2Stmt::CallSub { target: SubTarget::ONumber(9812), args, .. }
                if args.len() == 3
                    && args[0] == (b'X', v(24))
                    && args[1] == (b'Z', bin(BinaryOp::Add, v(26), v(18)))
                    && args[2] == (b'S', v(19))
            )
        });
        assert!(with_s, "G65 P9812 … S#19 expected");
    }

    #[test]
    fn golden_mpe_while_clear_variables_parses_ok() {
        let src = corpus("inbox/mpe_while_clear_variables.nc");
        let prog = parse_program(&src).unwrap();
        assert_eq!(prog.stmts.len(), 2); // #1 = 100 ; WHILE
        match &prog.stmts[1] {
            L2Stmt::While {
                cond, do_id, body, ..
            } => {
                assert_eq!(*do_id, 1);
                assert_eq!(*cond, bin(BinaryOp::Le, v(1), Expr::num(147.0)));
                assert_eq!(body.len(), 2);
                // #[#1] = #0 — косвенная цель, Null-значение (#0-семантика).
                assert_eq!(
                    body[0],
                    L2Stmt::Assign {
                        target: VarRef::Indirect(Box::new(v(1))),
                        value: Expr::Null,
                        src: body_src(&body[0]),
                    }
                );
            }
            _ => panic!("while expected, got {:?}", prog.stmts[1]),
        }
    }

    fn body_src(s: &L2Stmt) -> eid_core::ast::SrcSpan {
        s.src().cloned().unwrap()
    }

    #[test]
    fn golden_gce_full_industrial_lathe_parses_ok() {
        let src = corpus("inbox/gce_full_industrial_lathe.g");
        let prog = parse_program(&src).unwrap();
        // Латть: много Move (в т.ч. с B/C-осями) + носители для циклов.
        let moves: Vec<&L2Stmt> = prog
            .stmts
            .iter()
            .filter(|s| matches!(s, L2Stmt::Move { .. }))
            .collect();
        assert!(moves.len() >= 20, "moves: {}", moves.len());
        // R-дуги: G3X92.3035Z-1.407R10.5.
        let arc = moves.iter().any(|s| {
            matches!(
                s,
                L2Stmt::Move {
                    kind: MoveKind::Arc {
                        r: Some(_),
                        dir: Dir::Ccw,
                        ..
                    },
                    ..
                }
            )
        });
        assert!(arc, "R-arc expected");
        // B-ось как слово позы: G0B-405.7 → target.b.
        let b_move = moves.iter().any(|s| match s {
            L2Stmt::Move { target, .. } => {
                matches!(&target.b, Some(Expr::Unary(UnaryOp::Neg, _)))
            }
            _ => false,
        });
        assert!(b_move, "B-axis word expected");
        // `G1G94B-438.70F2000` — Move с feed.
        let b_feed = moves.iter().any(|s| match s {
            L2Stmt::Move { target, feed, .. } => {
                matches!(&target.b, Some(Expr::Unary(UnaryOp::Neg, _))) && feed.is_some()
            }
            _ => false,
        });
        assert!(b_feed);
        // #1133=80 → VendorSpecific (вне канонических диапазонов Fanuc).
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Assign {
                target: VarRef::Sys(SysVar::VendorSpecific(1133)),
                ..
            }
        )));
        // `#100=10.0` + `X[69+#100]` — выражение в слове X.
        let x_expr = moves.iter().any(|s| match s {
            L2Stmt::Move { target, .. } => {
                matches!(&target.x, Some(Expr::Binary(BinaryOp::Add, _, _)))
            }
            _ => false,
        });
        assert!(x_expr);
        // Вне-§2.4 носители: G10L2P1 / G71 / G74 / G76 / G28 / G53 / M64…
        let raws = prog
            .stmts
            .iter()
            .filter(|s| matches!(s, L2Stmt::L1Op(Op::Raw { .. })))
            .count();
        assert!(raws >= 40, "raw carriers: {raws}");
    }

    #[test]
    fn golden_cncsim_m98_m99_duas_morsas_parses_ok() {
        let src = corpus("local/cncsim_m98_m99_duas_morsas.nc");
        let prog = parse_program(&src).unwrap();
        // M98 P2102 ×2 → CallSub O2102.
        let calls = prog
            .stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    L2Stmt::CallSub {
                        target: SubTarget::ONumber(2102),
                        repeat: None,
                        modal: false,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(calls, 2);
        // O2102/O2103 → subs.
        assert_eq!(prog.subs.len(), 2);
        assert_eq!(prog.subs[0].target, SubTarget::ONumber(2102));
        assert_eq!(prog.subs[1].target, SubTarget::ONumber(2103));
        // `,R4.` — comma-слова вне §2.4 → носители (по 3 в O2102).
        let raws = prog.subs[0]
            .body
            .iter()
            .filter(
                |s| matches!(s, L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("junk")),
            )
            .count();
        assert_eq!(raws, 3, ",R-comma words in O2102");
    }

    #[test]
    fn golden_ncm_g434_rtcp_5x_milling_parses_ok() {
        let src = corpus("local/ncm_g434_rtcp_5x_milling.nc");
        let prog = parse_program(&src).unwrap();
        // РОВНО один включающий RtcpMode (G43.4, ценз §5: «RtcpMode
        // подтверждён ровно одной мишенью»); G49-отмены (N110 до, N446180
        // после) — тоже RtcpMode{false} (§2.4: «G43.4 / G49»).
        let rtcp: Vec<&L2Stmt> = prog
            .stmts
            .iter()
            .filter(|s| matches!(s, L2Stmt::RtcpMode { .. }))
            .collect();
        assert_eq!(rtcp.len(), 3, "G49 + G43.4 + G49");
        let enabled = rtcp
            .iter()
            .filter(|s| matches!(s, L2Stmt::RtcpMode { enabled: true, .. }))
            .count();
        assert_eq!(enabled, 1, "exactly one G43.4 enable");
        // 44k строк: ходы с A/C-осями — Move с a/c (кинематика — 12.2).
        let rotary: Vec<&ExprPose> = prog
            .stmts
            .iter()
            .filter_map(|s| match s {
                L2Stmt::Move { target, .. } if target.has_rotary() => Some(target),
                _ => None,
            })
            .collect();
        assert!(rotary.len() > 1000, "rotary moves: {}", rotary.len());
        // Отрицательные значения с точкой без ведущего нуля: C-.985.
        let neg = prog.stmts.iter().any(|s| match s {
            L2Stmt::Move { target, .. } => matches!(
                &target.c,
                Some(Expr::Unary(UnaryOp::Neg, inner))
                if matches!(**inner, Expr::Const(ref n) if n.text == ".985")
            ),
            _ => false,
        });
        assert!(neg, "C-.985 negative dot-value expected");
    }

    // ---------- Конструкции ценза (unit- golden-пары) ----------

    #[test]
    fn deep_drill_if_goto_null_checks() {
        let prog = p("IF[#26 EQ #0] GOTO942\nIF[#7 GT 0] GOTO946\n");
        match &prog.stmts[0] {
            L2Stmt::IfGoto {
                cond, target_label, ..
            } => {
                assert_eq!(*cond, bin(BinaryOp::Eq, v(26), Expr::Null));
                assert_eq!(*target_label, 942);
            }
            _ => panic!("ifgoto expected"),
        }
        match &prog.stmts[1] {
            L2Stmt::IfGoto {
                cond, target_label, ..
            } => {
                assert_eq!(*cond, bin(BinaryOp::Gt, v(7), Expr::num(0.0)));
                assert_eq!(*target_label, 946);
            }
            _ => panic!("ifgoto expected"),
        }
    }

    #[test]
    fn deep_drill_feed_expression() {
        // G1 Z#26 F[#9 * 0.33 ] — выражение в F-слове.
        let prog = p("G1 Z#26 F[#9 * 0.33 ]\n");
        match &prog.stmts[0] {
            L2Stmt::Move {
                kind, target, feed, ..
            } => {
                assert!(matches!(kind, MoveKind::Linear));
                assert_eq!(target.z, Some(v(26)));
                assert_eq!(*feed, Some(bin(BinaryOp::Mul, v(9), Expr::num(0.33))));
            }
            _ => panic!("move expected"),
        }
    }

    #[test]
    fn deep_drill_spindle_state_only_is_carrier() {
        // S#100 M4 — state-only кадр вне §2.4 → носитель ( честный Raw).
        let prog = p("S#100 M4\n");
        assert!(matches!(
            &prog.stmts[0],
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("state-only")
        ));
    }

    #[test]
    fn material_verify_abs_function() {
        // N10 #17=ABS[#17] — метка (узел Label, v0.20.1) + функция ABS.
        let prog = p("N10 #17=ABS[#17]\n");
        assert!(matches!(&prog.stmts[0], L2Stmt::Label { n: 10, .. }));
        match &prog.stmts[1] {
            L2Stmt::Assign { target, value, .. } => {
                assert_eq!(*target, VarRef::Direct(17));
                assert_eq!(*value, Expr::Unary(UnaryOp::Abs, Box::new(v(17))));
            }
            _ => panic!("assign expected"),
        }
        assert_eq!(prog.labels.get(&10), Some(&0));
    }

    #[test]
    fn material_verify_g65_bare_and_args() {
        let prog = p("G65 P9832\nG65 P9810 Z2. M1.\n");
        match &prog.stmts[0] {
            L2Stmt::CallSub {
                target,
                args,
                repeat,
                ..
            } => {
                assert_eq!(*target, SubTarget::ONumber(9832));
                assert!(args.is_empty());
                assert!(repeat.is_none());
            }
            _ => panic!("callsub expected"),
        }
        match &prog.stmts[1] {
            L2Stmt::CallSub { args, .. } => {
                // Z→#26=2., M→#13=1. (verbatim "2."/"1." — сравнение по значению).
                assert_eq!(args[0].0, b'Z');
                assert!(matches!(&args[0].1, Expr::Const(n) if n.value() == 2.0));
                assert_eq!(args[1].0, b'M');
                assert!(matches!(&args[1].1, Expr::Const(n) if n.value() == 1.0));
            }
            _ => panic!("callsub expected"),
        }
    }

    #[test]
    fn material_verify_alarm_block() {
        let src = "N941#3000=1(STOCK ABOVE Z0 MUST BE POSITIVE)\n";
        let prog = p(src);
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Assign {
                target: VarRef::Sys(SysVar::VendorSpecific(3000)),
                value: Expr::Const(_),
                ..
            }
        )));
    }

    #[test]
    fn o_number_header_and_main() {
        // Один O-заголовок → главное тело (подпрограмм нет).
        let prog = p("%\nO7981(DEEP DRILL V2)\n#100=300\nM99\n%\n");
        assert!(prog.subs.is_empty());
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Assign {
                target: VarRef::Direct(100),
                ..
            }
        )));
    }

    #[test]
    fn if_without_goto_then_fanuc_error() {
        // bare-IF — не Fanuc: ошибка (у Haas — IfThen-обёртка, правило 3).
        let err = parse_program("IF [#1 GT 0] #1=1.\n").unwrap_err();
        assert!(
            err.iter()
                .any(|e| e.message().contains("without GOTO/THEN")),
            "{err:?}"
        );
        assert_eq!(err[0].error_code(), eid_core::diag::ErrorCode::RuntimeState);
    }

    #[test]
    fn fanuc_m97_is_not_dialect_feature() {
        // M97 — Haas: в Fanuc-парсере vendor M → носитель.
        let prog = p("M97 P301\n");
        assert!(matches!(
            &prog.stmts[0],
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("M97")
        ));
    }

    #[test]
    fn fanuc_var_ranges() {
        assert_eq!(fanuc_map_var(1), VarRef::Direct(1));
        assert_eq!(fanuc_map_var(33), VarRef::Direct(33));
        assert_eq!(fanuc_map_var(199), VarRef::Direct(199));
        assert_eq!(fanuc_map_var(599), VarRef::Direct(599));
        assert_eq!(fanuc_map_var(999), VarRef::Direct(999));
        // Системные: #5043 → WorkPos(Z), #5021 → AbsMachinePos(X).
        assert_eq!(
            fanuc_map_var(5043),
            VarRef::Sys(SysVar::WorkPos(crate::ast::Axis::Z))
        );
        assert_eq!(
            fanuc_map_var(5021),
            VarRef::Sys(SysVar::AbsMachinePos(crate::ast::Axis::X))
        );
        assert_eq!(
            fanuc_map_var(5221),
            VarRef::Sys(SysVar::WcsOffset {
                p: 1,
                axis: crate::ast::Axis::X
            })
        );
        assert_eq!(
            fanuc_map_var(5323),
            VarRef::Sys(SysVar::WcsOffset {
                p: 6,
                axis: crate::ast::Axis::Z
            })
        );
        assert_eq!(
            fanuc_map_var(3000),
            VarRef::Sys(SysVar::VendorSpecific(3000))
        );
        assert_eq!(
            fanuc_map_var(1133),
            VarRef::Sys(SysVar::VendorSpecific(1133))
        );
    }

    #[test]
    fn fanuc_g229_vendor_carrier() {
        // G229 Z-2.03 T910 — vendor-код (mpe-файлы) → носитель.
        let prog = p("G229 Z-2.03 T910\n");
        assert!(matches!(
            &prog.stmts[0],
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G229")
        ));
    }

    #[test]
    fn fanuc_while_with_move_body() {
        // Гибрид: WHILE с Move в теле (параметрическая развёртка — 12.2).
        let prog = p("WHILE [#1 GT 0] DO1\nG1 Z#1 F50.\n#1 = #1 - 1\nEND1\n");
        match &prog.stmts[0] {
            L2Stmt::While { body, do_id, .. } => {
                assert_eq!(*do_id, 1);
                assert_eq!(body.len(), 2);
                assert!(matches!(&body[0], L2Stmt::Move { .. }));
            }
            _ => panic!("while expected"),
        }
    }

    #[test]
    fn fanuc_n_prefix_move_frame() {
        // N102G1X81.803F0.15 — метка слитно с кадром (латть): узел Label +
        // Move (v0.20.1).
        let prog = p("N102G1X81.803F0.15\n");
        assert!(matches!(&prog.stmts[0], L2Stmt::Label { n: 102, .. }));
        match &prog.stmts[1] {
            L2Stmt::Move {
                kind, target, feed, ..
            } => {
                assert!(matches!(kind, MoveKind::Linear));
                match &target.x {
                    Some(Expr::Const(n)) => assert_eq!(n.text, "81.803"),
                    _ => panic!("x const expected"),
                }
                match feed {
                    Some(Expr::Const(n)) => assert_eq!(n.text, "0.15"),
                    _ => panic!("feed const expected"),
                }
            }
            _ => panic!("move expected"),
        }
        assert_eq!(prog.labels.get(&102), Some(&0));
    }

    #[test]
    fn fanuc_g53_g28_g92_carriers() {
        let prog = p("G53Z-250 \nG28U0V0\nG92S2500 \nG10L2P1Z139.5(G54 BROCHE)\n");
        assert_eq!(prog.stmts.len(), 4);
        for (i, g) in ["G53", "G28", "G92", "G10"].iter().enumerate() {
            assert!(
                matches!(&prog.stmts[i], L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains(g)),
                "{g}: {:?}",
                prog.stmts[i]
            );
        }
    }

    #[test]
    fn fanuc_percent_and_comments_skipped() {
        let prog = p("%\n(comment)\n\n   \n%\n");
        assert!(prog.stmts.is_empty());
    }

    #[test]
    fn fanuc_span_lines() {
        let prog = p("G0 X1\nG1 Y2 F10\n");
        match (&prog.stmts[0], &prog.stmts[1]) {
            (L2Stmt::Move { src: s1, .. }, L2Stmt::Move { src: s2, .. }) => {
                assert_eq!(s1.line_start, 1);
                assert_eq!(s1.line_end, 1);
                assert_eq!(s2.line_start, 2);
                assert_eq!(s1.file.as_ref(), "fanuc-MB");
            }
            _ => panic!("moves expected"),
        }
    }

    // ---------- дополнительные golden-пары (документированные вызовы) ----

    #[test]
    fn deep_drill_documented_g65_call_args() {
        // Шапка mpe_deep_drill: `G65 P7981 Z-7.88 D-.4 S6000 F60.` —
        // 4 аргумента (Z/D/S/F) с отрицательными значениями без скобок.
        let prog = p("G65 P7981 Z-7.88 D-.4 S6000 F60.\n");
        match &prog.stmts[0] {
            L2Stmt::CallSub { target, args, .. } => {
                assert_eq!(*target, SubTarget::ONumber(7981));
                assert_eq!(args.len(), 4);
                assert_eq!(args[0].0, b'Z');
                assert!(matches!(&args[0].1, Expr::Unary(UnaryOp::Neg, _)));
                assert_eq!(args[1].0, b'D');
                assert!(matches!(&args[1].1, Expr::Unary(UnaryOp::Neg, _)));
                assert_eq!(args[2].0, b'S');
                assert!(matches!(&args[2].1, Expr::Const(n) if n.value() == 6000.0));
                assert_eq!(args[3].0, b'F');
                assert!(matches!(&args[3].1, Expr::Const(n) if n.value() == 60.0));
            }
            _ => panic!("callsub expected"),
        }
    }

    #[test]
    fn material_verify_ne_null_defaulting() {
        // IF[#17 NE #0] GOTO10 / #17=.04 — стандартный дефолт-паттерн.
        let prog = p("IF[#17 NE #0] GOTO10\n#17=.04\n");
        match &prog.stmts[0] {
            L2Stmt::IfGoto {
                cond, target_label, ..
            } => {
                assert_eq!(*cond, bin(BinaryOp::Ne, v(17), Expr::Null));
                assert_eq!(*target_label, 10);
            }
            _ => panic!("ifgoto expected"),
        }
        match &prog.stmts[1] {
            L2Stmt::Assign { target, value, .. } => {
                assert_eq!(*target, VarRef::Direct(17));
                assert!(matches!(value, Expr::Const(n) if n.value() == 0.04));
            }
            _ => panic!("assign expected"),
        }
    }

    #[test]
    fn fanuc_lowercase_keywords() {
        // Регистронезависимость keyword-детекта (цензор: case-insensitive).
        let prog = p("if [#1 eq #0] goto10\nwhile [#2 lt 5] do1\nend1\n");
        assert!(matches!(
            &prog.stmts[0],
            L2Stmt::IfGoto {
                target_label: 10,
                ..
            }
        ));
        assert!(matches!(&prog.stmts[1], L2Stmt::While { do_id: 1, .. }));
    }

    #[test]
    fn fanuc_m99_return_m30_end() {
        // v0.20.1: M99 → Return; M30 → L1Op(ProgramEnd).
        let prog = p("M99\nM30\n");
        assert!(matches!(&prog.stmts[0], L2Stmt::Return { .. }));
        assert!(matches!(
            &prog.stmts[1],
            L2Stmt::L1Op(Op::ProgramEnd { .. })
        ));
    }

    #[test]
    fn fanuc_assign_null_direct() {
        // #4 = #0 — присваивание копирует Null (таблица 2.1, сквозная).
        let prog = p("#4=#0\n");
        match &prog.stmts[0] {
            L2Stmt::Assign { target, value, .. } => {
                assert_eq!(*target, VarRef::Direct(4));
                assert_eq!(*value, Expr::Null);
            }
            _ => panic!("assign expected"),
        }
    }

    #[test]
    fn fanuc_while_body_with_callsub() {
        // Гибрид: WHILE + CallSub в теле (параметрическая развёртка 12.2).
        let prog = p("WHILE [#3 GT 0] DO1\nM98 P2102\n#3 = #3 - 1\nEND1\n");
        match &prog.stmts[0] {
            L2Stmt::While { body, .. } => {
                assert_eq!(body.len(), 2);
                assert!(matches!(
                    &body[0],
                    L2Stmt::CallSub {
                        target: SubTarget::ONumber(2102),
                        ..
                    }
                ));
            }
            _ => panic!("while expected"),
        }
    }
}
