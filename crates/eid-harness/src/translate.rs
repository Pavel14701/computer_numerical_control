//! translate_checked — RAW-правило кросс-диалектной трансляции как функция
//! (SPEC §8: same-dialect RAW вербатимен, кросс-диалект при RAW — отказ
//! целиком; CORPUS v0.5.3; ruling оркестратора, контрольная точка 2–3).
//!
//! Единая точка правила вместо разбросанных проверок (runner R4, будущий CLI
//! translate): [`translate_checked`] для файловых источников,
//! [`translate_checked_golden`] для golden-AST (диалектно-нейтральный источник:
//! любой Raw-оп отказывает любой цели).

use eid_core::adapter::Adapter;
use eid_core::ast::{Op, Program, SrcSpan};
use eid_core::diag::{Diagnostic, ErrorCode, Stage};

/// Кросс-диалектная трансляция с проверкой RAW-правила:
/// 1) `parse` от источника;
/// 2) AST содержит любой Raw-оп И `to != from` (кросс-диалект) → Err
///    `E_RAW_UNTRANSLATABLE` (ровно одна диагностика; все Raw-спаны
///    перечислены в `options`, первый — в `src`);
/// 3) иначе `to.emit`.
/// Same-dialect → plain emit (verbatim RAW легален, SPEC §8.1; замороженный
/// trait остаётся без source-dialect — правило замыкается здесь).
pub fn translate_checked(
    from: &dyn Adapter,
    to: &dyn Adapter,
    src: &str,
    file: &str,
) -> Result<(Program, String), Vec<Diagnostic>> {
    let program = from.parse(src, file)?;
    let cross = to.dialect_name() != from.dialect_name();
    checked_emit(to, &program, cross)
}

/// golden-AST (`src_dialect` отсутствует): любой Raw-оп → отказ любой цели
/// (golden не является диалектом — same-dialect-ветки нет).
pub fn translate_checked_golden(
    golden: &Program,
    to: &dyn Adapter,
) -> Result<(Program, String), Vec<Diagnostic>> {
    checked_emit(to, golden, true)
}

fn checked_emit(
    to: &dyn Adapter,
    program: &Program,
    cross: bool,
) -> Result<(Program, String), Vec<Diagnostic>> {
    let raw_spans: Vec<&SrcSpan> = program
        .ops
        .iter()
        .filter_map(|o| match o {
            Op::Raw { src, .. } => Some(src),
            _ => None,
        })
        .collect();
    if cross && !raw_spans.is_empty() {
        return Err(vec![raw_refusal(&raw_spans)]);
    }
    let text = to.emit(program)?;
    Ok((program.clone(), text))
}

/// Одна диагностика отказа: код `E_RAW_UNTRANSLATABLE`, все Raw-спаны
/// перечислены (`options` + первый в `src`), количество — в message.
fn raw_refusal(raw_spans: &[&SrcSpan]) -> Diagnostic {
    Diagnostic {
        stage: Stage::Emit,
        code: ErrorCode::RawUntranslatable,
        message: format!(
            "cross-dialect translation of a program containing {} RAW op(s) is forbidden (SPEC §8.2: same-dialect only)",
            raw_spans.len()
        ),
        src: raw_spans.first().map(|s| (*s).clone()),
        options: raw_spans.iter().map(|s| format_span(s)).collect(),
        limit: None,
        // Translate (EID → целевой диалект) — emit-сторона потока
        // parse → canonicalize → verify → emit (diag.rs, итерация 4).
    }
}

fn format_span(s: &SrcSpan) -> String {
    format!(
        "{}:{}.{}-{}.{}",
        s.file, s.line_start, s.col_start, s.line_end, s.col_end
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use eid_core::ast::{
        Coolant, Frame, Num, PathMode, Spindle, SrcSpan, State, Units, Wcs,
    };

    fn num(v: f64) -> Num {
        Num::new(format!("{v:.4}"), v)
    }

    fn src() -> SrcSpan {
        SrcSpan { file: "t".into(), line_start: 7, col_start: 1, line_end: 7, col_end: 9 }
    }

    fn raw_program(n_raw: usize) -> Program {
        let mut ops = Vec::new();
        for _ in 0..n_raw {
            ops.push(Op::Raw {
                payload_hex: "2424".into(),
                reason: "vendor".into(),
                code: ErrorCode::UnsupportedSyntax,
                src: src(),
            });
        }
        ops.push(Op::ProgramEnd { src: src() });
        Program {
            eid_version: "EID/L1 v0.2".into(),
            units: Units::Mm,
            path_mode: PathMode::ExactPath,
            frame_preset: None,
            ops,
        }
    }

    /// Минимальные адаптеры: parse всегда Ok (заданный AST через текст-заглушку
    /// невозможен — проверяем правило на emit-стороне через golden-вариант и
    /// same/cross-маршрутизацию).
    struct Stub {
        name: &'static str,
        refuse_raw_cross: bool,
    }
    impl Adapter for Stub {
        fn dialect_name(&self) -> &'static str {
            self.name
        }
        fn parse(&self, _s: &str, _f: &str) -> Result<Program, Vec<Diagnostic>> {
            Ok(raw_program(1))
        }
        fn emit(&self, p: &Program) -> Result<String, Vec<Diagnostic>> {
            let raw = p.ops.iter().any(|o| matches!(o, Op::Raw { .. }));
            if raw && self.refuse_raw_cross {
                return Err(vec![Diagnostic {
                    stage: Stage::Emit,
                    code: ErrorCode::RawUntranslatable,
                    message: "stub refuses raw".into(),
                    src: None,
                    options: vec![],
                    limit: None,
                }]);
            }
            Ok("emitted".into())
        }
    }

    #[test]
    fn cross_with_raw_refuses_single_diag_with_all_spans() {
        let from = Stub { name: "a", refuse_raw_cross: false };
        let to = Stub { name: "b", refuse_raw_cross: true };
        let err = translate_checked(&from, &to, "x", "f.nc").unwrap_err();
        assert_eq!(err.len(), 1, "{err:?}");
        assert_eq!(err[0].code, ErrorCode::RawUntranslatable);
        assert_eq!(err[0].options.len(), 1, "все Raw-спаны перечислены: {err:?}");
        assert!(err[0].src.is_some());
        assert!(err[0].message.contains("1 RAW op(s)"), "{err:?}");
    }

    #[test]
    fn same_dialect_raw_is_plain_emit() {
        // Два экземпляра одного диалекта: cross=false → plain emit без
        // RAW-проверки (verbatim легален, SPEC §8.1) — отказник-флаг стаба
        // не должен срабатывать на same-dialect маршруте.
        let a = Stub { name: "a", refuse_raw_cross: false };
        let (p, text) = translate_checked(&a, &a, "x", "f.nc").expect("same-dialect OK");
        assert_eq!(text, "emitted");
        assert!(p.ops.iter().any(|o| matches!(o, Op::Raw { .. })), "RAW сохранён");
    }

    #[test]
    fn cross_without_raw_emits() {
        let from = Stub { name: "a", refuse_raw_cross: false };
        let to = Stub { name: "b", refuse_raw_cross: true };
        // parse стаба возвращает RAW — перекрыть нельзя; проверяемgolden-ветку
        // без Raw: cross=false-маршрутизация покрывается same_dialect тестом,
        // здесь — golden без Raw у чужого эмиттера.
        let mut clean = raw_program(0);
        clean.ops.insert(
            0,
            Op::Dwell {
                seconds: num(1.0),
                state: State {
                    feed: Some(num(100.0)),
                    spindle: Spindle::Off,
                    coolant: Coolant::default(),
                    tool: None,
                    frame: Frame::Wcs,
                    wcs: Wcs::G54,
                },
                src: src(),
                sub: None,
            },
        );
        let (_, text) = translate_checked_golden(&clean, &to).expect("no RAW -> emit");
        assert_eq!(text, "emitted");
        let _ = from;
        let _ = to;
    }

    #[test]
    fn golden_with_raw_refuses_any_target() {
        let b = Stub { name: "b", refuse_raw_cross: true };
        let c = Stub { name: "c", refuse_raw_cross: false };
        let g = raw_program(2);
        for to in [&b, &c] {
            let err = translate_checked_golden(&g, to).unwrap_err();
            assert_eq!(err.len(), 1);
            assert_eq!(err[0].code, ErrorCode::RawUntranslatable);
            assert_eq!(err[0].options.len(), 2, "оба Raw-спана: {err:?}");
        }
    }

    #[test]
    fn emit_failure_of_target_propagates() {
        // Target emit падает по своей причине (не RAW) — Err транслируется как есть.
        struct Broken;
        impl Adapter for Broken {
            fn dialect_name(&self) -> &'static str {
                "broken"
            }
            fn parse(&self, _s: &str, _f: &str) -> Result<Program, Vec<Diagnostic>> {
                Ok(raw_program(0))
            }
            fn emit(&self, _p: &Program) -> Result<String, Vec<Diagnostic>> {
                Err(vec![Diagnostic {
                    stage: Stage::Emit,
                    code: ErrorCode::UnsupportedSyntax,
                    message: "op вне словаря цели".into(),
                    src: None,
                    options: vec![],
                    limit: None,
                }])
            }
        }
        let clean = raw_program(0);
        let err = translate_checked_golden(&clean, &Broken).unwrap_err();
        assert_eq!(err[0].code, ErrorCode::UnsupportedSyntax);
    }
}
