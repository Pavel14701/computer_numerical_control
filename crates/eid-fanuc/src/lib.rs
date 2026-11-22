//! eid-fanuc — канонизатор и эмиттер Fanuc mill (0i-D/F, 16i/18i/21i,
//! 30i/31i/32i, статическое CAM-подмножество) ↔ EID/L1 (Агент C, этапы 1–3).
//! Контракт: fanuc_dialect_analysis.md v0.0.1 §8 (reverse gap), SPEC-EID-L1
//! v0.17, COVERAGE v0.9 Fanuc-колонка (§2/§3/§5.1) + rulings оркестратора
//! 1–8 (этапы 2–3).
//! Этап 1: скелет + лексер (EOB `;` — НЕ комментарий, §1.3 ловушка №2) +
//! модальные группы 01–06 + движение G0–G3 (§9.1(1)–(3)).
//! Этап 2: фреймы (G54–G59/G10 L2/G92/G53 machine-frame/G52-ERR; G10 L20
//! Fanuc-семантика — ловушка №3), циклы §5 по канон-шаблону §7.7 SPEC
//! (P dwell = целые мс; δ = 0.5mm + W_CYCLE_PARAM_ASSUMED; K/L-повторы;
//! K0-in-G91 → Raw; G98/G99; модальность группы 09), M-словарь (стопы/
//! шпиндель/СОЖ/инструмент; T-предвыбор → Raw; M19/M29/M98/99/198 → Raw;
//! generic-ведро), Macro B → line-level Raw (`#`/`[ ]`/IF-GOTO/WHILE/G65/
//! M98/O-структуры; byte-exact payload фрагментов, ruling 5), компенсации
//! (G43/G44 → Raw{TABLE}; семейства G40/G49/G11 COVERAGE §1.3), G93 ABSORB
//! (ruling 4), DPI-F фикс (ruling 2: bare F = целое mm/min verbatim,
//! параметр 1422). Этап 3: эмиссия EID → Fanuc (emit.rs, COVERAGE §5.1
//! v0.8 / анализ §8): FanucEmissionLegality (ConversionTarget-паттерн),
//! G0–G3 всегда IJK (R — профиль-флаг этапа 8), полный круг БЕЗ end-слов,
//! decimal point ВСЕГДА (DPI-ловушка), dwell P-целые-мс / X-sub-ms,
//! G54–G59 (G59.x → SYNTAX-отказ; wcs_set_here → SEMANTICS-отказ),
//! path_mode G61/G64 (+warnings), program_end M30 (профиль M02 — этап 8),
//! T## M06; модальная абсорбция на RAW-строках (SPEC §8.6 v0.17) и
//! explicit-end полный круг (SPEC §3 v0.17) — зеркала аудита.
//! Этап 8 (emission-профили, agent C): `FanucAdapter::emit_with_opts` —
//! program_end M30/M02, allow_r_arcs (R-форма + notice, COVERAGE §5.1),
//! g59_map (G59.x → G54.1 Pn + notice «map применён»; неполная мапа → отказ
//! с перечнем незмапленных). Опции входят через обёртку registry
//! (eid-integration FanucNamedAdapter::with_emission_opts; trait v0.2
//! заморожен); дефолтные опции = прежняя эмиссия байт-в-байт.
//! explicit-end полный круг (SPEC §3 v0.17) — зеркала аудита.
#![allow(dead_code)]

use eid_core::adapter::{Adapter, EmitOutput, ParseOutput};
use eid_core::ast::Program;
use eid_core::diag::Diagnostic;
use eid_core::verify::{Notice, NoticeKind};

pub mod block;
pub mod canonicalize;
pub mod emit;
pub mod lexer;
pub mod state;

/// Адаптер Fanuc mill (dialect_name = "fanuc-mill").
/// drop-with-notice (zero-length / DPI-assumed) — не ошибка: notices
/// возвращаются в owning-результате [`ParseOutput`] (v0.2 #22), warnings
/// эмиссии — в [`EmitOutput`] (без interior mutability; Send + Sync).
#[derive(Debug, Default)]
pub struct FanucAdapter;

impl FanucAdapter {
    pub fn new() -> Self {
        Self
    }

    /// ConversionTarget-таблица легальности эмиссии в Fanuc mill
    /// (COVERAGE §5.1). legal — виды опов, представимые ходами; illegal —
    /// (условие, код класса). `emit` валидирует программу ПРОТИВ этой
    /// таблицы — коды отказа берутся только отсюда (единый путь отказа).
    /// E_RAW_UNTRANSLATABLE намеренно НЕ в таблице: это translate-путь
    /// (SPEC §8.2), не same-dialect эмиссия.
    pub fn emission_legality(&self) -> emit::FanucEmissionLegality {
        use eid_core::diag::ErrorCode::{
            InvalidInput as INVAL, UnsupportedSemantics as USEM, UnsupportedSyntax as SYN,
        };
        use emit::FanucEmissionCondition as C;
        emit::FanucEmissionLegality {
            // 9 из 10 видов опов (wcs_set_here запрещён ловушкой №3);
            // frame_preset — мета программы (G92, EQUIVALENCE §1.4).
            legal: vec![
                "rapid",
                "linear",
                "arc",
                "dwell",
                "tool_change",
                "wcs_define",
                "program_stop",
                "program_end",
                "raw",
                "frame_preset",
            ],
            // v0.2 волна 4 (§2.5 В1): ключи — типизированный enum; код-зеркало
            // обязано совпадать с `cond.code()` (тест legality_table_complete).
            illegal: vec![
                // COVERAGE §5.1: wcs_set_here эмиссия запрещена как L20
                // (Fanuc L20 = запись origin G54.1 Pn) — честный отказ.
                (C::WcsSetHere, USEM),
                // G59.1/.2/.3 — кодов нет (правило §5: вне словаря → SYNTAX);
                // профиль «map → G54.1 Pn» с notice — этап 8, НЕ дефолт.
                (C::WcsOutsideG54G59, SYN),
                (C::WcsDefinePOutsideG54G59, SYN),
                // G53 ходы прямые (SPEC §5.3) — дуга не представима.
                (C::MachineFrameArc, USEM),
                (C::MotionFeedNone, INVAL),         // канон-форма §3/§4
                (C::ArcUnknownStart, INVAL),        // IJK требует числового старта
                (C::ArcNormalNonCanonical, INVAL),  // нормаль не +ось (§6)
                (C::RawPayloadNotHex, INVAL),       // grammar D9 v0.3
                (C::ProgramEndMissing, INVAL),      // структура (SPEC §3)
                (C::ProgramEndDuplicate, INVAL),
                (C::ProgramEndNotLast, INVAL),
            ],
            warnings: vec![
                // COVERAGE §5.1: exact_path → ближайший G61 консервативно
                // (дефолт); профиль bare G64 — этап 8.
                (
                    "path_mode_exact_path",
                    "Fanuc exact_path эквивалента нет — эмитится G61 (консервативный дефолт: G61 = exact stop, value shift COVERAGE §2.2; профиль bare G64 — этап 8)",
                ),
                // COVERAGE §5.1: blend(t) → bare G64, t теряется.
                (
                    "path_mode_blend_t",
                    "path_mode blend t теряется — bare G64 (P-квантификатора в словаре Fanuc нет; COVERAGE §5.1)",
                ),
            ],
        }
    }
}

/// Диалектный Notice → единый core-Notice (NoticeKind-супермножество v0.2;
/// ZeroLengthDwell унифицирован на core-имя ZeroDwell).
impl From<canonicalize::Notice> for Notice {
    fn from(n: canonicalize::Notice) -> Self {
        Notice {
            kind: match n.kind {
                canonicalize::NoticeKind::ZeroLengthMotion => NoticeKind::ZeroLengthMotion,
                canonicalize::NoticeKind::ZeroLengthArc => NoticeKind::ZeroLengthArc,
                canonicalize::NoticeKind::ZeroLengthDwell => NoticeKind::ZeroDwell,
                canonicalize::NoticeKind::MissingProgramEnd => NoticeKind::MissingProgramEnd,
                canonicalize::NoticeKind::NearDegenerateRArc => NoticeKind::NearDegenerateRArc,
                canonicalize::NoticeKind::DpiAssumed => NoticeKind::DpiAssumed,
                canonicalize::NoticeKind::ModalOnRaw => NoticeKind::ModalOnRaw,
            },
            message: n.message,
            op_index: n.op_index,
            op: n.op,
        }
    }
}

// T5 (INTERFACE-V0.2-DESIGN §5): compile-time assert Send + Sync —
// interior mutability у адаптера запрещён контрактом trait v0.2.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<FanucAdapter>();
};

impl Adapter for FanucAdapter {
    fn dialect_name(&self) -> &'static str {
        "fanuc-mill"
    }

    fn parse(&self, src: &str, file: &str) -> Result<ParseOutput, Vec<Diagnostic>> {
        let out = canonicalize::canonicalize_with_notices(src, file)?;
        Ok(ParseOutput {
            program: out.program,
            notices: out.notices.into_iter().map(Into::into).collect(),
            diagnostics: Vec::new(),
        })
    }

    fn emit(&self, program: &Program) -> Result<EmitOutput, Vec<Diagnostic>> {
        self.emit_with_opts(program, &emit::FanucEmissionOpts::default())
    }
}

impl FanucAdapter {
    /// Эмиссия с профильными опциями (этап 8, COVERAGE §5.1): program_end
    /// M30/M02, allow_r_arcs (R-форма + notice), g59_map (G59.x → G54.1 Pn +
    /// notice). Trait Adapter v0.2 заморожен — опции входят НЕ через trait,
    /// а этим методом (обёртка registry eid-integration FanucNamedAdapter
    /// хранит Option<ProfileEmissionOpts> и вызывает его). Дефолтные опции
    /// = [`Adapter::emit`] байт-в-байт.
    pub fn emit_with_opts(
        &self,
        program: &Program,
        opts: &emit::FanucEmissionOpts,
    ) -> Result<EmitOutput, Vec<Diagnostic>> {
        let mut warnings = Vec::new();
        let out = emit::emit_with_profile(program, opts, &mut warnings)?;
        // EmitOut.warnings и sink-Vec дублируют друг друга (fanuc
        // emit_with_warnings заполняет оба) — берём из EmitOut.
        Ok(EmitOutput {
            text: out.text,
            warnings: out.warnings,
            diagnostics: Vec::new(),
        })
    }
}
