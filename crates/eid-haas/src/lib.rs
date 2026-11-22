//! eid-haas — канонизатор и эмиттер Haas mill NGC (VF/ST-вертикальные,
//! mini-mill, DT/DM; статическое CAM-подмножество) ↔ EID/L1 (Агент C, этапы
//! 1–3). Контракт: haas_dialect_analysis.md v0.0 (§1 профиль, §2 G-матрица,
//! §9.1 v0.1-скоуп, §10 реестр 34 `?`), SPEC-EID-L1 v0.18, COVERAGE v0.9.2
//! Haas-колонка. Зеркало архитектуры eid-fanuc.
//! Этап 1: скелет + лексер (EOB `;` — НЕ комментарий, §1.3 ловушка №2;
//! пробел-в-слове — строгий отказ `?`) + модальные группы 01–06 + 15
//! (G61/G64 value-shift) + движение G0–G3 (§9.1(1)–(3)): IJK-инкремент
//! (группы 04 в словаре НЕТ — G90.1/G91.1 ABSORB `?`), R-форма, helical,
//! полный круг 360° по геометрии (SPEC §3 v0.18), §8.6 модальная абсорбция
//! на RAW-строках, auto-PEND, DPI: X/Y/Z/A/B/C/I/J/K/R/Q bare → × least
//! increment; **F bare → × least increment + notice Setting 77** (домен F у
//! Haas ≠ Fanuc-ruling: F12 = 0.0012 units/min при Setting 77 OFF),
//! G70/G71/G72 → Raw-болтовые (НЕ units-псевдонимы), G28/G29/G31/G12/G13 →
//! raw-таблица.
//! Этап 2: фреймы (G54–G59 → p=1..6; **G110–G112 → p=7..9** — нативная
//! симметрия EID-слотов #7–#9 = Wcs::G59_1..G59_3 с notice W_WCS_EXTENDED;
//! G154 P1–P3 → 7..9; G113–G129 → Raw SEM; **G52 → Err** — 2-й жёсткий
//! отказ; G53 machine-frame §5.3; G10 L2 P1–P6 → wcs_define, **L2 P0 = слот
//! G52 → Raw RT** (≠ Fanuc external offset), **L20 → Raw RT** (НЕ
//! wcs_set_here), L1/L10–L13 → Raw RT; G92 пресетка, S-кламп → Raw SEM;
//! absorb_frame_structs на raw-строках); циклы ×Haas-семантика (G81/G82/
//! G83/G73/G85/G86/G89 разворачивание §7.7 + клиренс 0.01in unit-scaled
//! v0.18; **L-повторы — НЕ K** (K = min-пек G83), G91-шаги, **G83: Q ИЛИ
//! I/J-K** (d1=I, dk=max(K, d_(k−1)−J), mutually exclusive) — полный отвод,
//! **G73 δ = канон 0.01in/0.254mm** + notice W_CYCLE_PARAM_ASSUMED
//! (документированный дефолт Setting 22 0.02"/0.5mm ≠ канон — COVERAGE
//! §6.19), G86 рестарт шпинделя + **P-dot-rule dotted=сек/bare=мс**
//! (G4/G82/G86/G89), дефолт G99, G98-known-position, G74/G76 (I/J-направ-
//! ление в тексте)/G77/G84/G156/G174/G184 → Raw SEM, L0 `?` → Raw,
//! G80 + группа 09-модальность); M-словарь (M0/M1 → program_stop; M6/M16+T
//! идиома `T01 M06`; одиночное T → Raw SEM; M07 Shower → Raw SEM — НЕ mist;
//! M10–M13 тормоза 4/5-й осей → Raw SEM; M29 output-relay → Raw SEM — ≠
//! Fanuc rigid-tap; M19 → Raw; M97/M98/M99/M46/M96 → Raw SYNTAX; M109 → Raw
//! RUNTIME); компенсации G43/G44/G143 → Raw RT + семейства Haas-состава
//! G40={G41,G42,G141}, G49={G43,G44,G143}; Macro B → line-level Raw
//! (система-переменные #500–#699/#109xxx ≠ Fanuc — в диагностике).
//! Этап 3: эмиссия EID → Haas NGC (emit.rs, COVERAGE §5.2 / анализ §8):
//! HaasEmissionLegality (ConversionTarget-паттерн) с ТИПИЗИРОВАННЫМИ
//! условиями HaasEmissionCondition/HaasEmissionWarning (enum + тотальные
//! code()/text() — INTERFACE-V0.2 §2.5 В1, паник на emit-пути нет),
//! заголовок G20|G21 G90 G94 G17 (G91.1 НЕ эмитится — группы 04 в словаре
//! нет), p7..9 → G110–G112 + warning (зеркало W_WCS_EXTENDED; G59.x-огово-
//! рение происхождения таблиц), WCS_DEFINE p>6 → SYNTAX-отказ (L2-домен
//! P0..P6; L20 = profile-опция), wcs_set_here → SEMANTICS-отказ (ловушка
//! №2b), mist → SEMANTICS-отказ (M07 = Shower — другой канал), dwell
//! P-dot-rule обратный (целые мс → bare P500; sub-ms → dotted P0.0005 сек),
//! полный круг БЕЗ end-слов, decimal point ВСЕГДА, frame_preset G92 первым
//! блоком (G92.1 не эмитится — кода нет, Setting 129 `?`), G53 G0|G1,
//! program_end M30, T## M06, F-правило sub=1 (v0.18).
#![allow(dead_code)]

pub mod block;
pub mod canonicalize;
pub mod emit;
pub mod lexer;
pub mod state;

use eid_core::adapter::{Adapter, EmitOutput, ParseOutput};
use eid_core::ast::Program;
use eid_core::diag::Diagnostic;
use eid_core::verify::{Notice, NoticeKind};

/// Адаптер Haas mill NGC (dialect_name = "haas-mill").
/// drop-with-notice (zero-length / DPI-assumed) — не ошибка: notices
/// возвращаются в owning-результате [`ParseOutput`] (v0.2 #22), warnings
/// эмиссии — в [`EmitOutput`] (без interior mutability; Send + Sync).
#[derive(Debug, Default)]
pub struct HaasAdapter;

impl HaasAdapter {
    pub fn new() -> Self {
        Self
    }

    /// ConversionTarget-таблица легальности эмиссии в Haas mill NGC
    /// (COVERAGE §5.2). legal — виды опов, представимые ходами; illegal —
    /// (условие, код класса), зеркало тотального `HaasEmissionCondition::
    /// code()` (согласие — тест `legality_table_complete` в emit.rs-тестах).
    /// `emit` валидирует программу ПРОТИВ этой таблицы — коды отказа берутся
    /// только из enum-match (единый путь отказа, без паник). E_RAW_
    /// UNTRANSLATABLE намеренно НЕ в таблице: это translate-путь (SPEC
    /// §8.2), не same-dialect эмиссия.
    pub fn emission_legality(&self) -> emit::HaasEmissionLegality {
        emit::HaasEmissionLegality {
            // 9 из 10 видов опов (wcs_set_here запрещён ловушкой №2b);
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
            illegal: vec![
                // Ловушка №2b: Haas L20 = запись origin доп. СК, НЕ «set here».
                (emit::HaasEmissionCondition::WcsSetHere, USEM),
                // L2-домен Haas P0..P6 (p7..9 = расширенные origin — L20,
                // profile-опция НЕ дефолт; COVERAGE §5.2).
                (emit::HaasEmissionCondition::WcsDefinePOutsideL2Domain, SYN),
                // Этап 8: неполная profile wcs_extended_map — незмапленный
                // расширенный слот в программе (SYN, правило §5).
                (emit::HaasEmissionCondition::WcsMapIncomplete, SYN),
                // M07 = Shower — другой канал СОЖ; mist-spray не представим.
                (
                    emit::HaasEmissionCondition::CoolantMistUnrepresentable,
                    USEM,
                ),
                // G53 ходы прямые (SPEC §5.3).
                (emit::HaasEmissionCondition::MachineFrameArc, USEM),
                (emit::HaasEmissionCondition::MotionFeedNone, INVAL), // §3/§4
                (emit::HaasEmissionCondition::ArcUnknownStart, INVAL),
                (emit::HaasEmissionCondition::ArcNormalNonCanonical, INVAL),
                (emit::HaasEmissionCondition::RawPayloadNotHex, INVAL),
                (emit::HaasEmissionCondition::ProgramEndMissing, INVAL),
                (emit::HaasEmissionCondition::ProgramEndDuplicate, INVAL),
                (emit::HaasEmissionCondition::ProgramEndNotLast, INVAL),
            ],
            warnings: vec![
                // COVERAGE §5.2: exact_path → ближайший G61 консервативно
                // (дефолт как Fanuc; профиль bare-дефолт — этап 8).
                (
                    emit::HaasEmissionWarning::PathModeExactPath,
                    emit::HaasEmissionWarning::PathModeExactPath.text(),
                ),
                // blend(t) → bare G64, t теряется.
                (
                    emit::HaasEmissionWarning::PathModeBlendT,
                    emit::HaasEmissionWarning::PathModeBlendT.text(),
                ),
                // p7..9 → G110–G112 + оговорка происхождения G59.x-таблиц
                // (нативная симметрия слотов #7–#9; зеркало W_WCS_EXTENDED).
                (
                    emit::HaasEmissionWarning::WcsExtendedSlot,
                    emit::HaasEmissionWarning::WcsExtendedSlot.text(),
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
                canonicalize::NoticeKind::NearDegenerateRArc => NoticeKind::NearDegenerateRArc,
                canonicalize::NoticeKind::MissingProgramEnd => NoticeKind::MissingProgramEnd,
                canonicalize::NoticeKind::DpiAssumed => NoticeKind::DpiAssumed,
                canonicalize::NoticeKind::ModalOnRaw => NoticeKind::ModalOnRaw,
                canonicalize::NoticeKind::WcsExtended => NoticeKind::WcsExtended,
                canonicalize::NoticeKind::CycleParamAssumed => NoticeKind::CycleParamAssumed,
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
    assert_send_sync::<HaasAdapter>();
};

impl Adapter for HaasAdapter {
    fn dialect_name(&self) -> &'static str {
        "haas-mill"
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
        self.emit_with_opts(program, &emit::HaasEmissionOpts::default())
    }
}

impl HaasAdapter {
    /// Эмиссия с профильными опциями (этап 8, COVERAGE §5.2): program_end
    /// M30/M02, allow_r_arcs (R-форма + notice), g59_map (G59.x → G154 Pn +
    /// notice). Trait Adapter v0.2 заморожен — опции входят НЕ через trait,
    /// а этим методом (обёртка registry eid-integration HaasNamedAdapter
    /// хранит Option<ProfileEmissionOpts> и вызывает его). Дефолтные опции
    /// = [`Adapter::emit`] байт-в-байт.
    pub fn emit_with_opts(
        &self,
        program: &Program,
        opts: &emit::HaasEmissionOpts,
    ) -> Result<EmitOutput, Vec<Diagnostic>> {
        let mut warnings = Vec::new();
        let out = emit::emit_with_profile(program, opts, &mut warnings)?;
        Ok(EmitOutput {
            text: out.text,
            warnings: out.warnings,
            diagnostics: Vec::new(),
        })
    }
}

use eid_core::diag::ErrorCode::{
    InvalidInput as INVAL, UnsupportedSemantics as USEM, UnsupportedSyntax as SYN,
};
