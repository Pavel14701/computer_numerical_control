//! ПАРСЕР L2 — HAAS NGC (Next-Gen Control, Macro B) (этап 12.1, агент B2).
//! Контракт (зафиксирован оркестратором, сигнатура не меняется):
//!
//! ```text
//! pub fn parse_program(src: &str) -> Result<L2Program, Vec<L2Error>>
//! ```
//!
//! Реализация — общий Macro B core [`crate::parser_macro_common`] с
//! Haas-диалектной надстройкой (всё ядро Fanuc-MB плюс отличия):
//! * **bare-IF без THEN**: `IF [#3026 GT 0] #3026=0` →
//!   IfThen{then_stmt: Assign} — wrap, не новый вариант (правило 3
//!   рулинга); bare-IF с не-присваиванием (`IF [..] S800 M04`) →
//!   L1Op-носитель;
//! * **M97 P<block> L<expr>** — локальный вызов по N-метке →
//!   CallSub{SubTarget::NLabel} (правило 4); тела M97-подпрограмм
//!   (N-метка … M99) вырезаются в `L2Program.subs` (stmts остаются и в
//!   main — поток исполнения сохранён);
//! * **M98 P<программа> [L]** — слип `P52000` = 5×O2000, M99/M30;
//! * **G101** — зеркало → WithTransform{TransformKind::Mirror}
//!   (§2.3: «G51.1 X0 / Haas G101»);
//! * карта переменных: #1–#33 (локальные), #100–#199 (глобальные),
//!   #500–#699 → VendorSpecific (Haas settings-диапазон, рулинг
//!   оркестратора), #3000–#3099 (таймеры/алармы #3000/#3001/#3003/#3004)
//!   → VendorSpecific, #4001–#4030 → ModalGroup, позиции #5021+/#5041+/
//!   #5061+ → SysVar, #52xx → WcsOffset, **#100000–#199999** (#10189,
//!   #10582–#10586 и др.) → VendorSpecific, прочее → VendorSpecific
//!   (#3026 — номер инструмента, #598/#561/#179/#20045+ — настройки);
//! * G154/G110–G129 (расширенные WCS), G103 (block delete), G152,
//!   M96/M59/M69/M89 и прочий vendor — вне §2.4 → L1Op-носитель
//!   (`Op::Raw`, lossless hex строки; правило 5 рулинга);
//! * G31-пробинг, циклы G70–G89 (G81/G83…) — носители (цензор §6.1/§8).
//!
//! Ошибки — `Vec<L2Error>`; классы см. шапку parser_macro_common
//! (RuntimeError/E_RUNTIME_STATE — отклонение от E_INVALID_INPUT из
//! заготовки агента A, документировано в стоп-отчёте B2).

use crate::ast::{L2Program, SysVar, VarRef};
use crate::eval::L2Error;
use crate::parser_macro_common::{map_common_system, parse_macro_b, DialectCfg};

/// Haas-карта номера макро-переменной → [`VarRef`] (§2.2 + рулинг
/// «#500–#699/#109xxx → VendorSpecific»).
pub fn haas_map_var(n: u32) -> VarRef {
    match n {
        // Локальные #1–#33; глобальные #100–#199 (в т.ч. Renishaw-результаты
        // #140–#199 в корпусе). #200–#499 Haas не определяет → Direct.
        1..=499 => VarRef::Direct(n),
        // Рулинг оркестратора: #500–#699 → VendorSpecific (settings).
        500..=699 => VarRef::Sys(SysVar::VendorSpecific(n)),
        // Таймеры/алармы Haas (#3000 alarm, #3001/#3003/#3004 timers).
        3000..=3099 => VarRef::Sys(SysVar::VendorSpecific(n)),
        // Haas system-переменные #100000–#199999 (#10189 B-axis, #10582+).
        100000..=199999 => VarRef::Sys(SysVar::VendorSpecific(n)),
        _ => map_common_system(n).unwrap_or(VarRef::Sys(SysVar::VendorSpecific(n))),
    }
}

/// Конфигурация диалекта HAAS NGC (см. модульную шапку).
pub fn haas_cfg() -> DialectCfg {
    DialectCfg {
        name: "haas-MB",
        allow_bare_if: true,
        allow_m97: true,
        mirror_g101: true,
        map_var: haas_map_var,
    }
}

/// Разобрать программу Haas в L2 AST (контракт — см. модульную шапку).
pub fn parse_program(src: &str) -> Result<L2Program, Vec<L2Error>> {
    parse_macro_b(&haas_cfg(), src)
}

// ===========================================================================
// Тесты: golden-пары из целевых файлов ценза (haas-MB ×12 + vultr_g152)
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        Axis, BinaryOp, Expr, ExprPose, L2Stmt, MoveKind, SubTarget, SysVar, UnaryOp, VarRef,
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
    fn vs(n: u32) -> Expr {
        Expr::Var(VarRef::Sys(SysVar::VendorSpecific(n)))
    }

    // ---------- ГЛАДКОЕ ЗОЛОТО: все 12 haas-MB файлов ценза парсятся Ok ----

    #[test]
    fn golden_all_12_haas_files_parse_ok() {
        // 12 целевых файлов ценза §3 (haas-MB) + vultr_g152 (из задания
        // оркестратора; висячая `)` — толерантность лексера).
        let files = [
            "inbox/mpe_toolpath_profiling_m1.nc",
            "local/cncsim_g154_g43_haas_vf2.nc",
            "local/coyo_tram_b_probe_o09817.nc",
            "local/rensh_g04p_bare_xyz_o09811.nc",
            "local/rensh_g154p98_tbd_o09006.nc",
            "local/rensh_g65_probe_bore_o09351.nc",
            "local/rensh_settings_check_g103p1_o09725.nc",
            "local/vultr_m97_l_macro_o9016.nc",
            "local/zapcon_g31_tsc_o9613.nc",
            "local/zapcon_g65_ij_g154p11_endmill_ex.nc",
            "local/zapcon_g81_p0_g154p10_drill_ex.nc",
            "local/zodiac_toolbreak_m59_g154pvar.nc",
            "local/vultr_g152_npt_pipe_example.nc",
        ];
        for rel in files {
            let src = corpus(rel);
            match parse_program(&src) {
                Ok(prog) => assert!(!prog.stmts.is_empty(), "{rel}: parsed but empty"),
                Err(e) => panic!("{rel}: parse errors: {e:?}"),
            }
        }
    }

    // ---------- vultr_m97_l_macro_o9016 ----------

    #[test]
    fn vultr_m97_call_with_l_expr_and_sub_carving() {
        let src = corpus("local/vultr_m97_l_macro_o9016.nc");
        let prog = parse_program(&src).unwrap();
        // M97 P301 L[ [ ABS[#26] * #5 ] + 1 ] — CallSub{NLabel(301)} + L-expr.
        let call = prog.stmts.iter().find_map(|s| match s {
            L2Stmt::CallSub {
                target,
                repeat,
                modal,
                ..
            } if *target == SubTarget::NLabel(301) => Some((repeat, modal)),
            _ => None,
        });
        let Some((repeat, modal)) = call else {
            panic!("M97 P301 expected");
        };
        assert!(!*modal);
        assert_eq!(
            *repeat,
            Some(bin(
                BinaryOp::Add,
                bin(
                    BinaryOp::Mul,
                    Expr::Unary(UnaryOp::Abs, Box::new(v(26))),
                    v(5)
                ),
                Expr::num(1.0)
            ))
        );
        // Тело N301 … M99 вырезано в subs (и осталось в main).
        // v0.20.1: тело начинается с узла Label{301}.
        let sub = prog
            .subs
            .iter()
            .find(|s| s.target == SubTarget::NLabel(301))
            .expect("M97 sub body carved");
        assert!(matches!(&sub.body[0], L2Stmt::Label { n: 301, .. }));
        assert!(sub.body.len() >= 12, "quad-loop body: {}", sub.body.len());
        assert!(sub.body.iter().any(|s| matches!(
            s,
            L2Stmt::Move {
                kind: MoveKind::Arc { dir: Dir::Ccw, .. },
                ..
            }
        )));
        // В main поток сохранён (lossless): 8 ходов четырёх квадрантов.
        let main_moves = prog
            .stmts
            .iter()
            .filter(|s| matches!(s, L2Stmt::Move { .. }))
            .count();
        assert!(main_moves >= 10, "main moves: {main_moves}");
    }

    #[test]
    fn vultr_bare_if_assignments() {
        let src = corpus("local/vultr_m97_l_macro_o9016.nc");
        let prog = parse_program(&src).unwrap();
        // `IF [ #4 EQ #0 ] #3000= 1 (NO I: INPUT GAGE DIA.)` — bare-IF →
        // IfThen{Assign} на VendorSpecific(3000).
        let alarm_if = prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::IfThen { cond: Expr::Binary(BinaryOp::Eq, a, b), then_stmt, .. }
            if **a == v(4)
                && matches!(**b, Expr::Null)
                && matches!(**then_stmt, L2Stmt::Assign { target: VarRef::Sys(SysVar::VendorSpecific(3000)), .. })
        ));
        assert!(alarm_if, "bare-IF #3000=1 expected");
        // `IF [ #5 LT 0.9999 ] #5= #5 * 10000` — bare-IF с арифметикой.
        let mult_if = prog.stmts.iter().any(|s| {
            let L2Stmt::IfThen {
                cond: Expr::Binary(BinaryOp::Lt, a, _),
                then_stmt,
                ..
            } = s
            else {
                return false;
            };
            if **a != v(5) {
                return false;
            }
            let L2Stmt::Assign {
                value: Expr::Binary(BinaryOp::Mul, x, y),
                ..
            } = &**then_stmt
            else {
                return false;
            };
            **x == v(5) && matches!(**y, Expr::Const(ref n) if n.value() == 10000.0)
        });
        assert!(mult_if);
        // `IF [ #26 GE 0 ] #3000= 1 (Z MUST BE NEGATIVE)`.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::IfThen { cond: Expr::Binary(BinaryOp::Ge, a, _), .. } if **a == v(26)
        )));
    }

    #[test]
    fn vultr_nested_indirect_and_unary_word() {
        let src = corpus("local/vultr_m97_l_macro_o9016.nc");
        let prog = parse_program(&src).unwrap();
        // #7= [ #[ 2400 + #7 ] + #[ 2600 + #7 ] ] — вложенная косвенность.
        let nested = prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Assign { target: VarRef::Direct(7), value: Expr::Binary(BinaryOp::Add, a, b), .. }
            if matches!(**a, Expr::Var(VarRef::Indirect(_)))
                && matches!(**b, Expr::Var(VarRef::Indirect(_)))
        ));
        assert!(nested, "nested #[2400+#7] expected");
        // `X - [ #1 + #2 ]` — унарный минус перед скобкой в адресном слове.
        let neg_word = prog.stmts.iter().any(|s| match s {
            L2Stmt::Move { target, .. } => matches!(
                &target.x,
                Some(Expr::Unary(UnaryOp::Neg, inner))
                if matches!(**inner, Expr::Binary(BinaryOp::Add, _, _))
            ),
            _ => false,
        });
        assert!(neg_word);
        // `G03 G91 I [ #2 / 2 ] X#2 F#9` — дуга с I-выражением.
        let arc = prog.stmts.iter().any(|s| {
            matches!(
                s,
                L2Stmt::Move { kind: MoveKind::Arc { ijk, dir: Dir::Ccw, r: None }, .. }
                if matches!(&ijk[0], Some(Expr::Binary(BinaryOp::Div, a, b))
                    if **a == v(2) && **b == Expr::num(2.0))
            )
        });
        assert!(arc);
    }

    // ---------- zodiac_toolbreak_m59_g154pvar ----------

    #[test]
    fn zodiac_g154_pvar_and_goto() {
        let src = corpus("local/zodiac_toolbreak_m59_g154pvar.nc");
        let prog = parse_program(&src).unwrap();
        // `G0 G90 G154 P#5 X#3 Y#4` — G154 с переменной в P-слове (ценз:
        // «G154 P#5 — ПЕРЕМЕННАЯ в P-слове WCS») → носитель целиком
        // (правило 5: не парсить в L2Stmt).
        let g154 = prog.stmts.iter().any(|s| {
            matches!(
                s,
                L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G154")
            )
        });
        assert!(g154);
        // `G10 L20 P#5 G90 X#10585 …` — G10 → носитель.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G10")
        )));
        // `GOTO100` безусловный → IfGoto{cond=1, 100} (×2 в файле).
        let gotos = prog
            .stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    L2Stmt::IfGoto {
                        cond: Expr::Const(_),
                        target_label: 100,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(gotos, 2);
        // M59 P2 / M96 P10 Q17 — vendor → носители.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("M96")
        )));
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("M59")
        )));
        // `G43 H#3026 Z.2` — G43 → носитель (H-слово с sys-var #3026).
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G43")
        )));
        // #3000=100 (Tool Has Pulled Out) → Assign на VendorSpecific(3000).
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Assign {
                target: VarRef::Sys(SysVar::VendorSpecific(3000)),
                ..
            }
        )));
    }

    // ---------- zapcon ----------

    #[test]
    fn zapcon_g31_tsc_probing_and_sysvars() {
        let src = corpus("local/zapcon_g31_tsc_o9613.nc");
        let prog = parse_program(&src).unwrap();
        // G31 ×2 → носители (ценз §6.1: пробинг вне §2.4).
        let g31 = prog
            .stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G31")
                )
            })
            .count();
        assert_eq!(g31, 2, "two G31 lines");
        // #27= #5021 - #5041 — AbsMachinePos/WorkPos (Haas — Fanuc-совместно).
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Assign { value: Expr::Binary(BinaryOp::Sub, a, b), .. }
            if matches!(**a, Expr::Var(VarRef::Sys(SysVar::AbsMachinePos(_))))
                && matches!(**b, Expr::Var(VarRef::Sys(SysVar::WorkPos(_))))
        )));
        // bare-IF `IF [ #11 LT 0 ] #1=1.` → IfThen{Assign}.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::IfThen { then_stmt, .. }
            if matches!(**then_stmt, L2Stmt::Assign { target: VarRef::Direct(1), .. })
        )));
        // `IF [ #4 LT 0 ] S800 M04` ×4 → носители (не-assign тело).
        let s800 = prog
            .stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("bare-IF")
                )
            })
            .count();
        assert_eq!(s800, 4, "four bare-IF S800 M04 carriers");
        // G103 P1 ×несколько → носители.
        let g103 = prog
            .stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G103")
                )
            })
            .count();
        assert!(g103 >= 5, "G103 carriers: {g103}");
        // G43 H#3026 → носитель.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G43")
        )));
    }

    #[test]
    fn zapcon_g65_hij_args() {
        let src = corpus("local/zapcon_g65_ij_g154p11_endmill_ex.nc");
        let prog = parse_program(&src).unwrap();
        // G65 P9613 H0.01 I0.25 J0. → H→#11, I→#4, J→#5 (ценз §8:
        // «G65-аргументы H/I/J»).
        let call = prog.stmts.iter().find_map(|s| match s {
            L2Stmt::CallSub { target, args, .. } if *target == SubTarget::ONumber(9613) => {
                Some(args)
            }
            _ => None,
        });
        let Some(args) = call else {
            panic!("G65 P9613 expected");
        };
        assert!(args.contains(&(b'H', Expr::num(0.01))));
        assert!(args.contains(&(b'I', Expr::num(0.25))));
        // J0. — verbatim "0." — сравнение по значению.
        assert!(args
            .iter()
            .any(|(l, e)| *l == b'J' && matches!(e, Expr::Const(n) if n.value() == 0.0)));
        // G154 P11 строка → носитель.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G154")
        )));
        // T42 M06 → ToolChange{42}.
        assert!(prog
            .stmts
            .iter()
            .any(|s| matches!(s, L2Stmt::L1Op(Op::ToolChange { tool: 42, .. }))));
    }

    #[test]
    fn zapcon_g81_canned_cycle_carriers() {
        let src = corpus("local/zapcon_g81_p0_g154p10_drill_ex.nc");
        let prog = parse_program(&src).unwrap();
        // G81-строка + продолжения X1./X2. + G80 — всё носители (циклы вне §2.4).
        let canned = prog
            .stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    L2Stmt::L1Op(Op::Raw { reason, .. })
                        if reason.contains("G81")
                            || reason.contains("canned")
                            || reason.contains("G80")
                )
            })
            .count();
        assert!(canned >= 4, "canned carriers: {canned}");
        // G154 P10 → носитель.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G154")
        )));
    }

    // ---------- cncsim_g154_g43_haas_vf2 ----------

    #[test]
    fn cncsim_g154_m97_local_subs() {
        let src = corpus("local/cncsim_g154_g43_haas_vf2.nc");
        let prog = parse_program(&src).unwrap();
        // M97 P1000 / M97 P2000 → два CallSub{NLabel}.
        for n in [1000u32, 2000] {
            assert!(
                prog.stmts.iter().any(|s| matches!(
                    s,
                    L2Stmt::CallSub { target: SubTarget::NLabel(x), .. } if *x == n
                )),
                "M97 P{n} expected"
            );
            assert!(
                prog.subs.iter().any(|s| s.target == SubTarget::NLabel(n)),
                "carved sub N{n} expected"
            );
        }
        // G154 P1/P2 и G110/G111 → носители.
        let wcs = prog
            .stmts
            .iter()
            .filter(|s| matches!(
                s,
                L2Stmt::L1Op(Op::Raw { reason, .. })
                    if reason.contains("G154") || reason.contains("G110") || reason.contains("G111")
            ))
            .count();
        assert!(wcs >= 4, "extended WCS carriers: {wcs}");
    }

    // ---------- coyo_tram_b_probe ----------

    #[test]
    fn coyo_b_axis_and_10189() {
        let src = corpus("local/coyo_tram_b_probe_o09817.nc");
        let prog = parse_program(&src).unwrap();
        // `G00 B#10189` — B-слово с vendor-переменной #10189 → Move с Sys.
        let b_move = prog.stmts.iter().any(|s| {
            matches!(
                s,
                L2Stmt::Move { kind: MoveKind::Rapid, target: ExprPose { b: Some(e), .. }, .. }
                if *e == vs(10189)
            )
        });
        assert!(b_move, "G00 B#10189 expected");
        // G65 P9832/P9810/P9817/P9833 — Renishaw-цепочка.
        for onum in [9832u32, 9810, 9817, 9833] {
            assert!(
                prog.stmts.iter().any(|s| matches!(
                    s,
                    L2Stmt::CallSub { target: SubTarget::ONumber(x), .. } if *x == onum
                )),
                "G65 P{onum} expected"
            );
        }
        // `G65 P9810 X0 Y0 Z1. F75.` — F-аргумент → #9 (значение 75).
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::CallSub { target: SubTarget::ONumber(9810), args, .. }
            if args.iter().any(|(l, e)| *l == b'F' && matches!(e, Expr::Const(n) if n.value() == 75.0))
        )));
        // M00 → ProgramStop.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::ProgramStop {
                optional: false,
                ..
            })
        )));
        // `G00 B0` — константная B-ось.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Move {
                target: ExprPose {
                    b: Some(Expr::Const(_)),
                    ..
                },
                ..
            }
        )));
    }

    // ---------- rensh ----------

    #[test]
    fn rensh_g04p_while_and_indirect_assign_with_and() {
        let src = corpus("local/rensh_g04p_bare_xyz_o09811.nc");
        let prog = parse_program(&src).unwrap();
        // WHILE [ #1 LE 199 ] DO1 … #[ #1 ]= #0 … END1.
        let wh = prog.stmts.iter().find_map(|s| match s {
            L2Stmt::While {
                cond, do_id, body, ..
            } if *do_id == 1 => Some((cond, body)),
            _ => None,
        });
        let Some((cond, body)) = wh else {
            panic!("WHILE DO1 expected");
        };
        assert_eq!(*cond, bin(BinaryOp::Le, v(1), Expr::num(199.0)));
        assert!(body.iter().any(|s| matches!(
            s,
            L2Stmt::Assign {
                target: VarRef::Indirect(_),
                value: Expr::Null,
                ..
            }
        )));
        // `#[ 3006 - [ [ #170 AND 8 ] / 8 * 6 ] ]= 1` — косвенная цель
        // с AND (цензор: косвенная адресация #[#161+2]).
        let ind = prog.stmts.iter().any(|s| {
            matches!(
                s,
                L2Stmt::Assign { target: VarRef::Indirect(inner), value: Expr::Const(_), .. }
                if matches!(**inner, Expr::Binary(BinaryOp::Sub, _, _))
            )
        });
        assert!(ind, "indirect target with AND expected");
        // `IF [ #170 AND 4 EQ 4 ] GOTO13` — AND-приоритет.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::IfGoto { cond: Expr::Binary(BinaryOp::Eq, a, _), target_label: 13, .. }
            if matches!(**a, Expr::Binary(BinaryOp::And, _, _))
        )));
        // `#[ #161 + 2 ]` косвенность в выражении движения → G31-носитель
        // (пробинг вне §2.4), lossless-пейлоад сохраняет выражение.
        let g31 = prog
            .stmts
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G31")
                )
            })
            .count();
        assert!(g31 >= 1);
        // `G65 P9730 H#11 M#13 S#19 T#20 W#23 X#24 Y#25 Z#26` — 8 аргументов.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::CallSub { target: SubTarget::ONumber(9730), args, .. } if args.len() == 8
        )));
    }

    #[test]
    fn rensh_tbd_then_assign_and_var_tool() {
        let src = corpus("local/rensh_g154p98_tbd_o09006.nc");
        let prog = parse_program(&src).unwrap();
        // `IF [ #27 LT 0 ] THEN #27= [ -1 * #27 ]` — THEN-форма (ценз §8).
        assert!(prog.stmts.iter().any(|s| {
            let L2Stmt::IfThen {
                cond: Expr::Binary(BinaryOp::Lt, a, _),
                then_stmt,
                ..
            } = s
            else {
                return false;
            };
            if **a != v(27) {
                return false;
            }
            let L2Stmt::Assign {
                value: Expr::Binary(BinaryOp::Mul, x, _),
                ..
            } = &**then_stmt
            else {
                return false;
            };
            matches!(**x, Expr::Unary(UnaryOp::Neg, _))
        }));
        // `T#3026 M06` — T с переменной → носитель (G43 H#3026 тоже).
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("M6") && reason.contains("T")
        )));
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G43")
        )));
        // `G91 G31 Z-0.1 F [ 500. * #179 ]` — G31 → носитель.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G31")
        )));
        // `#20= #3026` — Haas tool-number sysvar → VendorSpecific(3026).
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Assign { target: VarRef::Direct(20), value: e, .. }
            if *e == vs(3026)
        )));
        // G154 P98 (вне p-домена) → носитель.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G154")
        )));
    }

    #[test]
    fn rensh_bore_indirect_cond_eq_null() {
        let src = corpus("local/rensh_g65_probe_bore_o09351.nc");
        let prog = parse_program(&src).unwrap();
        // `IF [ #[ #161 ] EQ #0 ] GOTO91` — косвенный cond против Null
        // (#0-семантика ценза) РЯДОМ с `EQ 0` (число) — оба в файле.
        let vs_null = prog.stmts.iter().any(|s| {
            matches!(
                s,
                L2Stmt::IfGoto { cond: Expr::Binary(BinaryOp::Eq, a, b), target_label: 91, .. }
                if matches!(**a, Expr::Var(VarRef::Indirect(_))) && matches!(**b, Expr::Null)
            )
        });
        let vs_zero = prog.stmts.iter().any(|s| {
            matches!(
                s,
                L2Stmt::IfGoto { cond: Expr::Binary(BinaryOp::Eq, a, b), target_label: 91, .. }
                if matches!(**a, Expr::Var(VarRef::Indirect(_)))
                    && matches!(**b, Expr::Const(ref n) if n.value() == 0.0)
            )
        });
        assert!(vs_null, "EQ #0 expected");
        assert!(vs_zero, "EQ 0 expected");
        // `G65 P9814 D#7 R#18 Q#17` — D→#7, R→#18, Q→#17.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::CallSub { target: SubTarget::ONumber(9814), args, .. }
            if args.contains(&(b'D', v(7)))
                && args.contains(&(b'R', v(18)))
                && args.contains(&(b'Q', v(17)))
        )));
        // `N91 #3000= 11` — метка + alarm-assign.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Assign {
                target: VarRef::Sys(SysVar::VendorSpecific(3000)),
                ..
            }
        )));
    }

    #[test]
    fn rensh_settings_bit_suffix_and_then() {
        let src = corpus("local/rensh_settings_check_g103p1_o09725.nc");
        let prog = parse_program(&src).unwrap();
        // `IF[#1 EQ #0] THEN #598= 1.` — THEN + слитные IF[.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::IfThen { cond: Expr::Binary(BinaryOp::Eq, a, b), then_stmt, .. }
            if **a == v(1)
                && matches!(**b, Expr::Null)
                && matches!(
                    **then_stmt,
                    L2Stmt::Assign { target: VarRef::Sys(SysVar::VendorSpecific(598)), .. }
                )
        )));
        // `IF [ #20045 EQ 0 ] GOTO10` — Haas settings #20045+ → VendorSpecific.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::IfGoto { cond: Expr::Binary(BinaryOp::Eq, a, _), target_label: 10, .. }
            if **a == vs(20045)
        )));
        // `IF [ #30057.25 ] GOTO70` — битовый суффикс → кодирование n*100+b.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::IfGoto {
                cond: Expr::Var(VarRef::Sys(SysVar::VendorSpecific(3005725))),
                target_label: 70,
                ..
            }
        )));
        // `IF [ #4011 EQ 50 ]` — ModalGroup(11).
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::IfGoto { cond: Expr::Binary(BinaryOp::Eq, a, _), .. }
            if **a == Expr::Var(VarRef::Sys(SysVar::ModalGroup(11)))
        )));
        // `#21=#179*.01(10 PERCENT …)` — слитное умножение.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Assign { target: VarRef::Direct(21), value: Expr::Binary(BinaryOp::Mul, a, b), .. }
            if **a == v(179) && matches!(**b, Expr::Const(ref n) if n.text == ".01")
        )));
        // `G103P1` слитно → носитель.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G103")
        )));
    }

    // ---------- mpe_toolpath_profiling_m1 ----------

    #[test]
    fn mpe_profiling_indirect_target() {
        let src = corpus("inbox/mpe_toolpath_profiling_m1.nc");
        let prog = parse_program(&src).unwrap();
        // #540 = 551 → VendorSpecific(540) (рулинг: #500–#699 → VendorSpecific).
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Assign {
                target: VarRef::Sys(SysVar::VendorSpecific(540)),
                value: Expr::Const(_),
                ..
            }
        )));
        // #[#540] = #3001 — косвенная цель (ценз: #[#540]).
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Assign { target: VarRef::Indirect(_), value: e, .. }
            if *e == vs(3001)
        )));
        // #[#540] = #[#540] / 1000 — деление (проверка деления — на eval,
        // parse строит дерево).
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Assign { target: VarRef::Indirect(_), value: Expr::Binary(BinaryOp::Div, a, b), .. }
            if matches!(**a, Expr::Var(VarRef::Indirect(_)))
                && matches!(**b, Expr::Const(ref n) if n.value() == 1000.0)
        )));
        // M30 в конце.
        assert!(prog
            .stmts
            .iter()
            .any(|s| matches!(s, L2Stmt::L1Op(Op::ProgramEnd { .. }))));
    }

    // ---------- vultr_g152 (задание оркестратора, вне 12-ценза) ----------

    #[test]
    fn vultr_g152_stray_paren_tolerated() {
        let src = corpus("local/vultr_g152_npt_pipe_example.nc");
        let prog = parse_program(&src).unwrap();
        // `G152 I2.81 J8 Z-.8 E50. F40. D22)` — висячая `)` отброшена,
        // G152 → носитель (lossless-пейлоад хранит байты строки целиком).
        let g152 = prog.stmts.iter().any(|s| {
            matches!(
                s,
                L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G152")
            )
        });
        assert!(g152, "G152 carrier expected");
        // Обычные ходы парсятся: G0 G54 X2. Y2. → Move Rapid.
        assert!(prog.stmts.iter().any(|s| matches!(
            s,
            L2Stmt::Move {
                kind: MoveKind::Rapid,
                target: ExprPose {
                    x: Some(Expr::Const(_)),
                    y: Some(Expr::Const(_)),
                    ..
                },
                ..
            }
        )));
        // `G53 Y0. M30` — носитель + ProgramEnd (оба stmt).
        assert!(prog
            .stmts
            .iter()
            .any(|s| matches!(s, L2Stmt::L1Op(Op::ProgramEnd { .. }))));
    }

    // ---------- Haas-диалектные unit-тесты ----------

    #[test]
    fn haas_var_ranges() {
        let vs_ref = |n: u32| VarRef::Sys(SysVar::VendorSpecific(n));
        assert_eq!(haas_map_var(1), VarRef::Direct(1));
        assert_eq!(haas_map_var(33), VarRef::Direct(33));
        assert_eq!(haas_map_var(199), VarRef::Direct(199));
        // Рулинг: #500–#699 → VendorSpecific.
        assert_eq!(haas_map_var(540), vs_ref(540));
        assert_eq!(haas_map_var(699), vs_ref(699));
        // #3026 (tool), #3001 (timer) → VendorSpecific.
        assert_eq!(haas_map_var(3026), vs_ref(3026));
        assert_eq!(haas_map_var(3001), vs_ref(3001));
        // #109xxx → VendorSpecific.
        assert_eq!(haas_map_var(10189), vs_ref(10189));
        assert_eq!(haas_map_var(10585), vs_ref(10585));
        assert_eq!(haas_map_var(5041), VarRef::Sys(SysVar::WorkPos(Axis::X)));
    }

    #[test]
    fn haas_m97_l_const_repeat() {
        let prog = p("M97 P301 L2\nN301\nM99\n");
        match &prog.stmts[0] {
            L2Stmt::CallSub {
                target,
                repeat,
                modal,
                args,
                ..
            } => {
                assert_eq!(*target, SubTarget::NLabel(301));
                assert_eq!(*repeat, Some(Expr::num(2.0)));
                assert!(!*modal);
                assert!(args.is_empty());
            }
            _ => panic!("callsub expected"),
        }
        // Тело N301 … M99 вырезано.
        assert!(prog.subs.iter().any(|s| s.target == SubTarget::NLabel(301)));
    }

    #[test]
    fn haas_g101_mirror_transform() {
        // Правило из задания: G101-зеркало → WithTransform{Mirror}.
        let prog = p("G101 X0\n");
        match &prog.stmts[0] {
            L2Stmt::WithTransform {
                transform,
                body,
                src: _,
            } => {
                match transform {
                    crate::ast::TransformKind::Mirror { axis, pivot } => {
                        assert_eq!(*axis, Axis::X);
                        assert_eq!(*pivot, Expr::num(0.0));
                    }
                    _ => panic!("mirror expected"),
                }
                assert!(body.is_empty());
            }
            _ => panic!("with-transform expected"),
        }
    }

    #[test]
    fn haas_g101_without_axis_is_carrier() {
        let prog = p("G101\n");
        assert!(matches!(
            &prog.stmts[0],
            L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G101")
        ));
    }

    #[test]
    fn haas_bare_if_missing_body_is_carrier() {
        // `IF [cond]` без тела — носитель (не ошибка, в отличие от Fanuc).
        let prog = p("IF [#1 GT 0]\n");
        assert!(matches!(&prog.stmts[0], L2Stmt::L1Op(Op::Raw { .. })));
    }

    #[test]
    fn haas_m98_slip_like_fanuc() {
        let prog = p("M98 P52000\n");
        match &prog.stmts[0] {
            L2Stmt::CallSub { target, repeat, .. } => {
                assert_eq!(*target, SubTarget::ONumber(2000));
                assert_eq!(*repeat, Some(Expr::num(5.0)));
            }
            _ => panic!("callsub expected"),
        }
    }

    #[test]
    fn haas_g103_block_delete_carrier() {
        // Ценз §8: G103 P1 (block-delete) ×6 файлов — носитель.
        let prog = p("G103 P1\nG103\n");
        for s in &prog.stmts {
            assert!(matches!(
                s,
                L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G103")
            ));
        }
    }

    #[test]
    fn haas_m96_m59_vendor_carriers() {
        let prog = p("M96 P10 Q17\nM59 P2\nM69 P3\n");
        assert_eq!(prog.stmts.len(), 3);
        for s in &prog.stmts {
            assert!(matches!(s, L2Stmt::L1Op(Op::Raw { .. })), "{s:?}");
        }
    }

    #[test]
    fn haas_eob_semicolon_comment() {
        // `;` — комментарий до конца строки (как в lib.rs strip_comments);
        // Haas `;`-EOB в целевых файлах ценза не встречается (стоп-отчёт).
        let prog = p("G1 X10 ; tail\n");
        assert!(matches!(&prog.stmts[0], L2Stmt::Move { .. }));
    }

    #[test]
    fn haas_unclosed_do_error() {
        let err = parse_program("WHILE [#1 LT 5] DO1\n#1 = #1 + 1\n").unwrap_err();
        assert!(err.iter().any(|e| e.message().contains("unterminated DO1")));
    }

    // ---------- дополнительные golden-пары (цензор §8) ----------

    #[test]
    fn haas_bare_if_ge_alarm_pattern() {
        // vultr: `IF [ #26 GE 0 ] #3000= 1 (Z MUST BE NEGATIVE)` — GE-cond.
        let prog = p("IF [ #26 GE 0 ] #3000= 1 (Z MUST BE NEGATIVE)\n");
        match &prog.stmts[0] {
            L2Stmt::IfThen {
                cond: Expr::Binary(BinaryOp::Ge, a, b),
                then_stmt,
                ..
            } => {
                assert_eq!(**a, v(26));
                assert_eq!(**b, Expr::num(0.0));
                assert!(matches!(
                    &**then_stmt,
                    L2Stmt::Assign {
                        target: VarRef::Sys(SysVar::VendorSpecific(3000)),
                        ..
                    }
                ));
            }
            _ => panic!("ifthen expected"),
        }
    }

    #[test]
    fn haas_g04_p_dwell_milliseconds_verbatim() {
        // rensh-цепочка: G04 P250 — значение verbatim (семантика P —
        // диалектная, но Op::Dwell несёт число).
        let prog = p("G04 P250\n");
        match &prog.stmts[0] {
            L2Stmt::L1Op(Op::Dwell { seconds, .. }) => {
                assert_eq!(seconds.text, "250");
            }
            _ => panic!("dwell expected"),
        }
    }

    #[test]
    fn haas_timer_cond_goto() {
        // rensh: `IF [ #3001 LT 200 ] GOTO999 (TEST RUNNING IN GRAPHICS)`.
        let prog = p("IF [ #3001 LT 200 ] GOTO999 (TEST RUNNING IN GRAPHICS)\n");
        match &prog.stmts[0] {
            L2Stmt::IfGoto {
                cond: Expr::Binary(BinaryOp::Lt, a, b),
                target_label,
                ..
            } => {
                assert_eq!(**a, vs(3001));
                assert_eq!(**b, Expr::num(200.0));
                assert_eq!(*target_label, 999);
            }
            _ => panic!("ifgoto expected"),
        }
    }

    #[test]
    fn haas_g110_extended_wcs_carrier() {
        // cncsim: `G90 G110 G0 X15. Y20. S2200 M3` — G110 (расширенная WCS
        // Haas, аналог G154 P1) — вне §2.4 → носитель строки.
        let prog = p("G90 G110 G0 X15. Y20. S2200 M3\n");
        assert!(
            matches!(&prog.stmts[0], L2Stmt::L1Op(Op::Raw { reason, .. }) if reason.contains("G110"))
        );
    }

    #[test]
    fn haas_m98_p9854_delay_call() {
        // rensh_tbd: `M98 P9854 (DELAY)` — обычный M98-вызов.
        let prog = p("M98 P9854 (DELAY)\n");
        match &prog.stmts[0] {
            L2Stmt::CallSub {
                target,
                repeat,
                modal,
                args,
                ..
            } => {
                assert_eq!(*target, SubTarget::ONumber(9854));
                assert!(repeat.is_none());
                assert!(!*modal);
                assert!(args.is_empty());
            }
            _ => panic!("callsub expected"),
        }
    }

    #[test]
    fn haas_o_sub_and_m99_return() {
        // Гибрид: O-тело с M99-Return (v0.20.1) после M30 главного.
        let prog = p("O09016 (NPT MACRO)\nM30\nO9017 (S)\nG0 Z5.\nM99\n");
        assert!(prog.subs.len() == 1);
        assert_eq!(prog.subs[0].target, SubTarget::ONumber(9017));
        assert!(matches!(&prog.subs[0].body[0], L2Stmt::Move { .. }));
        assert!(matches!(&prog.subs[0].body[1], L2Stmt::Return { .. }));
    }
}
