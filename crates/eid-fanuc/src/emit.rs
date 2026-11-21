//! Эмиттер EID/L1 → текст Fanuc mill (Агент C, этап 3; COVERAGE §5.1 v0.8,
//! fanuc_dialect_analysis.md §8 Reverse gap, EQUIVALENCE §1.4/§2/§4).
//!
//! Структура вывода (детерминировано: один AST → одни байты; двойной emit —
//! байт-в-байт):
//!  1. заголовок `G21|G20 G90 G94 G17 G91.1` — всегда явно: units (§2.5),
//!     G90/G94 — дефолты, G17 — плоскость (дуги её переустанавливают),
//!     G91.1 — режим центров дуг ИНКРЕМЕНТАЛЬНЫЙ pinned (эмиссия всегда
//!     IJK-оффсеты от старта, EQUIVALENCE §4; опция G90.1/.91.1 на части
//!     серий — `?` реестр 15 анализа — задокументировано).
//!  2. path_mode — всегда явно, одна строка (SPEC §3: одно значение):
//!     exact_stop → `G61` (Fanuc G61 = exact stop — value shift, COVERAGE
//!     §2.2); exact_path → **warning + `G61`** (консервативный дефолт —
//!     «Fanuc exact_path эквивалента нет»; профиль bare G64 — этап 8);
//!     blend(t) → bare `G64` + warning при t (P-квантификатора в словаре
//!     Fanuc нет — t теряется).
//!  3. frame_preset (EQUIVALENCE §1.4): выборка preset.wcs, затем
//!     `G92 X_ Y_ Z_` — до любого motion-опа; запекание в G10 запрещено.
//!     G92-hygiene (`G92.1` первым блоком) — флаг профиля, НЕ дефолт
//!     (меняет глобальное состояние станка — операторский контракт
//!     EQUIVALENCE §1.2) — этап 8.
//!  4. опы по порядку: перед каждым — дельты его снапшота (фиксированный
//!     порядок WCS → шпиндель → СОЖ, затем строки хода).
//!  5. движение G0/G1/G2/G3: все три оси конца на каждом ходе; числа —
//!     4 dp С ДЕСЯТИЧНОЙ ТОЧКОЙ ВСЕГДА (DPI-ловушка §1.2 анализа: `X10.0000`
//!     не `X10` — bare integer при DPI=0 читается ×0.001); IS-B mm 3dp-
//!     значимы — трейлинг-нули Fanuc терпит (notice-политика НЕ нужна);
//!     frame=machine → префикс `G53 G0|G1` (одномодальный, явный G0/G1 в
//!     кадре — требование Fanuc manual).
//!  6. дуги: плоскость из канонической нормали (+Z→G17, +Y→G18, +X→G19);
//!     смена плоскости — отдельной строкой; центр — ТОЛЬКО in-plane пара
//!     I/J/K (G17→I+J, G18→I+K, G19→J+K; I/J/K literal X/Y/Z), инкремент
//!     от старта (G91.1 в заголовке); R НЕ эмитится никогда (EQUIVALENCE §4;
//!     профиль-флаг R+notice — этап 8); **полный круг (sweep=360°, pitch=0)
//!     БЕЗ end-слов** (`G2 I_ J_` — легальный Fanuc-формат; слова end==start
//!     при перепарсе квантуются в zero-length drop); винтовой полный оборот
//!     (pitch≠0) — с end-словом вне-плоскостной оси; частичная дуга — все
//!     три оси.
//!  7. dwell: **P = целые мс** (decimal point запрещён: `G4 P500` = 0.5 s);
//!     правило COVERAGE §5.1: seconds·1000 целое (в допуске) → P,
//!     иначе sub-ms остаток → **X-форма** (`G4 X0.0005` — Fanuc X принимает
//!     float-секунды).
//!  8. tool_change → `T## M06` в одном блоке (Fanuc-идиома); STOP → M0/M1;
//!     PEND → `M30` (дефолт, Fanuc-конвенция rewind; профиль M02 — этап 8);
//!     WCS_DEFINE → `G10 L2 P<n> X_ Y_ Z_`; **WCS_SET_HERE → отказ
//!     E_UNSUPPORTED_SEMANTICS** (Fanuc L20 = запись G54.1 Pn — другое;
//!     честный отказ, COVERAGE §5.1 ловушка №3).
//!
//! Хвост M5/M9 перед program_end НЕ эмитится (ruling COVERAGE §5: AST не
//! хранит пост-конечное состояние; Fanuc M30 сам глушит шпиндель/СОЖ).
//! Развёрнутые циклы §7.7 SPEC эмитятся как ходы self-собой (verbose —
//! документированное следствие, SPEC §1.2; обратное свёртывание запрещено).
//!
//! RAW: payload-hex декодируется и выводится дословно (same-dialect
//! fidelity, grammar D9 — включая терминатор строки). Кросс-диалектный
//! запрет `E_RAW_UNTRANSLATABLE` — НЕ здесь: это правило translate-конвейера
//! (SPEC §8.2), same-dialect эмиттер обязан воспроизводить байты.
//!
//! Warnings (не отказы) — в буфере адаптера (`FanucAdapter::take_warnings`):
//! потери path_mode exact_path/blend(t), не-эмитируемая normal-компонента
//! центра дуги, несвязные снапшоты state.tool/state.feed.
//!
//! Валидация: `eid_core::parser::validate` (канон-форма §3) → magic-проверка
//! → легальность эмиссии против [`FanucEmissionLegality`] (ConversionTarget-
//! паттерн MLIR: коды отказа берутся ТОЛЬКО из таблицы — единый путь
//! отказа, COVERAGE §5.1).

use eid_core::ast::{Coolant, Dir, Frame, Num, Op, Program, Spindle, StartPoint, Units, Wcs};
use eid_core::diag::{Diagnostic, ErrorCode, Stage};
use eid_core::emitter::fmt_num;
use eid_core::parser;

use crate::state::{plane_axes, GCode};

/// Результат эмиссии: текст + warnings (не отказы; забрать через
/// [`crate::FanucAdapter::take_warnings`]).
pub struct EmitOut {
    pub text: String,
    pub warnings: Vec<String>,
}

// ===========================================================================
// FanucEmissionLegality — ConversionTarget-паттерн (MLIR), зеркально
// eid-grbl/eid-linuxcnc (COVERAGE §5.1): легальность эмиссии в Fanuc —
// ДАННЫЕ, не размазанные проверки; валидатор берёт ErrorCode ТОЛЬКО из
// таблицы (единый путь отказа). E_RAW_UNTRANSLATABLE в таблицу НЕ входит —
// это translate-путь (SPEC §8.2), не same-dialect эмиссия.
//
// Состав (10 видов опов; 9 легальны — wcs_set_here запрещён ловушкой №3):
//   rapid/linear/arc — G0/G1/G2-G3 (IJK всегда); dwell — G4 P-мс/X-сек;
//   tool_change — T## M06; program_stop — M0/M1; program_end — M30;
//   wcs_define — G10 L2 P1..6; raw — same-dialect verbatim;
//   frame_preset — G92 первым блоком (EQUIVALENCE §1.4).
//   wcs_set_here — НЕ представимо: Fanuc L20 = запись origin G54.1 Pn,
//   НЕ «set here» (COVERAGE §5.1); «set here» через G92 требует машинной
//   позиции — не статично → честный отказ SEMANTICS.
// ===========================================================================

/// Статическая таблица легальности эмиссии в Fanuc mill.
#[derive(Debug, Clone, PartialEq)]
pub struct FanucEmissionLegality {
    /// Виды опов, представимые ходами Fanuc (имена = op_kind()).
    pub legal: Vec<&'static str>,
    /// Условия отказа: (имя условия, код класса легальности).
    /// Классы — правило COVERAGE §5: конструкция вне словаря цели →
    /// E_UNSUPPORTED_SYNTAX; не представима ходами → E_UNSUPPORTED_SEMANTICS;
    /// нарушение канон-формы AST → E_INVALID_INPUT.
    pub illegal: Vec<(&'static str, ErrorCode)>,
    /// Легально-но-потерьные условия: (условие, текст warning). Эмиттер пишет
    /// текст в warnings-буфер адаптера, отказом НЕ является.
    pub warnings: Vec<(&'static str, &'static str)>,
}

impl FanucEmissionLegality {
    /// Код отказа по имени условия; None = условие не объявлено.
    pub fn code_of(&self, condition: &str) -> Option<ErrorCode> {
        self.illegal
            .iter()
            .find(|(c, _)| *c == condition)
            .map(|(_, code)| *code)
    }

    /// Текст warning по имени условия; None = не объявлено.
    pub fn warning_of(&self, condition: &str) -> Option<&'static str> {
        self.warnings
            .iter()
            .find(|(c, _)| *c == condition)
            .map(|(_, text)| *text)
    }

    /// Все коды отказа таблицы (без дубликатов).
    pub fn codes(&self) -> Vec<ErrorCode> {
        let mut v: Vec<ErrorCode> = self.illegal.iter().map(|(_, c)| *c).collect();
        v.sort_by_key(|c| c.as_str());
        v.dedup();
        v
    }

    /// Вид опа легален?
    pub fn is_legal_kind(&self, kind: &str) -> bool {
        self.legal.contains(&kind)
    }
}

/// Имя вида опа — ключ таблицы легальности (словарь manifest r4_expect).
pub fn op_kind(op: &Op) -> &'static str {
    match op {
        Op::Rapid { .. } => "rapid",
        Op::Linear { .. } => "linear",
        Op::Arc { .. } => "arc",
        Op::Dwell { .. } => "dwell",
        Op::ToolChange { .. } => "tool_change",
        Op::ProgramStop { .. } => "program_stop",
        Op::ProgramEnd { .. } => "program_end",
        Op::WcsDefine { .. } => "wcs_define",
        Op::WcsSetHere { .. } => "wcs_set_here",
        Op::Raw { .. } => "raw",
    }
}

/// G-слово WCS-выбора; None = вне словаря Fanuc (G59.1/.2/.3 — кодов нет,
/// COVERAGE §5.1: G54.1 P1..P3 — другая таблица с чужим происхождением;
/// подмена без предупреждения запрещена — аналогия golden_preset_no_bake).
fn wcs_word(w: Wcs) -> Option<&'static str> {
    Some(match w {
        Wcs::G54 => "G54",
        Wcs::G55 => "G55",
        Wcs::G56 => "G56",
        Wcs::G57 => "G57",
        Wcs::G58 => "G58",
        Wcs::G59 => "G59",
        _ => return None,
    })
}

/// Плоскость по канонической нормали (SPEC §6: единичная +ось;
/// +Z→G17, +Y→G18, +X→G19). Возвращает (плоскость, axis_0, axis_1, normal-
/// ось). Не-осевой/отрицательный normal → Err(текст) — канон-форма такого не
/// содержит; валидатор оборачивает в код таблицы, писатель — INVALID.
fn plane_of(normal: &[Num; 3]) -> Result<(GCode, usize, usize, usize), String> {
    let c = [normal[0].value(), normal[1].value(), normal[2].value()];
    let (mut best, mut bi) = (0.0f64, 0usize);
    for (i, &x) in c.iter().enumerate() {
        if x.abs() > best {
            best = x.abs();
            bi = i;
        }
    }
    let axial = (best - 1.0).abs() <= 0.001
        && c[(bi + 1) % 3].abs() <= 0.001
        && c[(bi + 2) % 3].abs() <= 0.001;
    if !axial || c[bi] < 0.0 {
        return Err(format!(
            "arc: normal = ({:.4},{:.4},{:.4}) — не единичная +ось (SPEC §6: G17→+Z, G18→+Y, G19→+X)",
            c[0], c[1], c[2]
        ));
    }
    Ok(match bi {
        2 => {
            let (a0, a1, nl) = plane_axes(GCode::PlaneXY);
            (GCode::PlaneXY, a0, a1, nl)
        }
        1 => {
            let (a0, a1, nl) = plane_axes(GCode::PlaneZX);
            (GCode::PlaneZX, a0, a1, nl)
        }
        _ => {
            let (a0, a1, nl) = plane_axes(GCode::PlaneYZ);
            (GCode::PlaneYZ, a0, a1, nl)
        }
    })
}

fn to_f64(p: &eid_core::ast::Point3) -> [f64; 3] {
    [p.x.value(), p.y.value(), p.z.value()]
}

fn diag(code: ErrorCode, msg: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code,
        stage: Stage::Emit,
        message: msg.into(),
        src: None,
        options: Vec::new(),
        limit: None,
    }
}

fn diag_src(code: ErrorCode, msg: impl Into<String>, src: &eid_core::ast::SrcSpan) -> Diagnostic {
    Diagnostic {
        code,
        stage: Stage::Emit,
        message: msg.into(),
        src: Some(src.clone()),
        options: Vec::new(),
        limit: None,
    }
}

// ===========================================================================
// Точка входа + валидация легальности
// ===========================================================================

/// Эмиссия без warnings-буфера (совместимость; предупреждения теряются).
pub fn emit(program: &Program) -> Result<String, Vec<Diagnostic>> {
    let mut sink = Vec::new();
    emit_with_warnings(program, &mut sink).map(|out| out.text)
}

/// Эмиссия + warnings (потери метаданных при целевой эмиссии, COVERAGE §5.1).
pub fn emit_with_warnings(
    program: &Program,
    warnings: &mut Vec<String>,
) -> Result<EmitOut, Vec<Diagnostic>> {
    // Каноническая форма до эмиссии (R1: parse(emit(x)) — тот же AST) —
    // с Fanuc-спецификой хвоста (ruling 1 этапа 2, COVERAGE §5.1
    // «хвост-subs-случай»): блоки после PEND — line-level Raw (subs-тела
    // O#### после M30 — норма Fanuc DNC-файлов) — ЛЕГАЛЬНЫ в same-dialect
    // эмиссии. Core-verify структурно требует PEND последним (не знает о
    // Fanuc-политике хвоста) — её «program_end must be the last op»
    // отфильтровывается при условии «после PEND только Raw»; не-Raw опы
    // после PEND остаются отказом (SPEC §3).
    let tail_raw_only = match program
        .ops
        .iter()
        .rposition(|o| !matches!(o, Op::Raw { .. }))
    {
        Some(i) => matches!(program.ops[i], Op::ProgramEnd { .. }),
        None => false,
    };
    let mut ds: Vec<Diagnostic> = match parser::validate(program) {
        Ok(()) => Vec::new(),
        Err(v) => v,
    };
    if tail_raw_only {
        ds.retain(|d| {
            !(d.code == ErrorCode::InvalidInput
                && d.message.contains("program_end must be the last op"))
        });
    }
    if !ds.is_empty() {
        return Err(ds);
    }
    // Magic — ровно "EID/L1 v0.3" (grammar v0.3; SPEC §10.4: v0.2 невалиден).
    if program.eid_version != "EID/L1 v0.3" {
        return Err(vec![diag(
            ErrorCode::InvalidInput,
            format!(
                "eid_version `{}` не поддерживается (ожидается EID/L1 v0.3)",
                program.eid_version
            ),
        )]);
    }
    // Легальность эмиссии — против таблицы (COVERAGE §5.1), fail-fast.
    let table = crate::FanucAdapter::default().emission_legality();
    let problems = validate_fanuc(&table, program);
    if !problems.is_empty() {
        return Err(problems);
    }
    let mut w = Writer::new(program);
    w.run(&table, warnings)?;
    // Копия warnings в EmitOut — буфер вызывателя НЕ опустошается (и адаптер,
    // и тест читают свой Vec).
    Ok(EmitOut {
        text: w.text,
        warnings: warnings.clone(),
    })
}

/// Валидация легальности эмиссии против таблицы (fail-fast, COVERAGE §5.1).
/// Коды отказов берутся ИСКЛЮЧИТЕЛЬНО из таблицы — единый путь отказа.
fn validate_fanuc(table: &FanucEmissionLegality, program: &Program) -> Vec<Diagnostic> {
    // Условие обязано быть объявлено в таблице — иначе баг объявления, не паника пути.
    let code = |cond: &'static str| {
        table
            .code_of(cond)
            .unwrap_or_else(|| panic!("emission-legality: условие `{cond}` не объявлено в таблице"))
    };
    let mut v = Vec::new();
    // «Последний не-Raw оп = PEND» — Fanuc-хвост (Raw-subs после M30, ruling
    // 1) не нарушает структуру; не-Raw опыт после PEND — нарушение.
    let mut last_is_end = true; // отсутствует не-Raw оп ⇒ handled end_count==0
    for op in &program.ops {
        if !matches!(op, Op::Raw { .. }) {
            last_is_end = matches!(op, Op::ProgramEnd { .. });
        }
        // Вид опа, объявленный illegal в таблице (wcs_set_here) — отсекается
        // своим условием ниже; generic-ветка — только для НЕОБЪЯВленных видов
        // (защита полноты: новый вид опа ⇒ дополнить таблицу). Код — fallback
        // SYN (`op_kind_not_legal` в таблицу не входит — ветка покрытия).
        let kind = op_kind(op);
        if !table.is_legal_kind(kind) && !table.illegal.iter().any(|(c, _)| *c == kind) {
            v.push(diag_src(
                table
                    .code_of("op_kind_not_legal")
                    .unwrap_or(ErrorCode::UnsupportedSyntax),
                format!("op_kind_not_legal: `{kind}` не легален для цели Fanuc"),
                op_src(op),
            ));
        }
        match op {
            Op::WcsSetHere { src, .. } => v.push(diag_src(
                code("wcs_set_here"),
                "wcs_set_here: эмиссия в Fanuc запрещена как G10 L20 — Fanuc L20 = запись origin G54.1 Pn, НЕ «set here» (ловушка №3, COVERAGE §5.1); через G92 требует машинной позиции — не статично",
                src,
            )),
            Op::Arc { start, normal, state, src, .. } => {
                if matches!(start, StartPoint::Unknown) {
                    v.push(diag_src(
                        code("arc_unknown_start"),
                        "arc with unknown start — IJK offsets require numeric start (SPEC §2 Q11)",
                        src,
                    ));
                }
                let n = [normal.x.clone(), normal.y.clone(), normal.z.clone()];
                if let Err(m) = plane_of(&n) {
                    v.push(diag_src(code("arc_normal_non_canonical"), m, src));
                }
                check_motion_state(table, &mut v, state, src, true);
                if state.frame == Frame::Machine {
                    v.push(diag_src(
                        code("machine_frame_arc"),
                        "machine_frame_arc: дуга в G53 не эмитится (machine-frame ходы прямые; SPEC §5.3)",
                        src,
                    ));
                }
            }
            Op::Linear { state, src, .. } => check_motion_state(table, &mut v, state, src, true),
            Op::Rapid { state, src, .. } => check_motion_state(table, &mut v, state, src, false),
            Op::Dwell { state, src, .. } | Op::ProgramStop { state, src, .. } => {
                check_wcs(table, &mut v, &state.wcs, src);
            }
            Op::WcsDefine { p, state, src, .. } => {
                check_wcs(table, &mut v, &state.wcs, src);
                if !(1..=6).contains(p) {
                    v.push(diag_src(
                        code("wcs_define_p_outside_g54_g59"),
                        format!("wcs_define P{p} вне Fanuc-домена G54–G59 (P1..P6; COVERAGE §5.1 — кодов нет)"),
                        src,
                    ));
                }
            }
            Op::Raw { payload_hex, src, .. } => {
                if payload_hex.len() % 2 != 0
                    || !payload_hex
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(&c))
                {
                    v.push(diag_src(
                        code("raw_payload_not_hex"),
                        "raw_payload_not_hex: payload не чётный hex (grammar D9 v0.3)",
                        src,
                    ));
                }
            }
            _ => {}
        }
    }
    // Инварианты §3 (енды): отсутствует / дубликат / не последний.
    let end_count = program
        .ops
        .iter()
        .filter(|o| matches!(o, Op::ProgramEnd { .. }))
        .count();
    if end_count == 0 {
        v.push(diag(
            code("program_end_missing"),
            "program_end_missing: PEND обязателен ровно один раз, последним (SPEC §3)",
        ));
    }
    if end_count > 1 {
        v.push(diag(
            code("program_end_duplicate"),
            format!("program_end_duplicate: {end_count} × PEND (SPEC §3)"),
        ));
    }
    if !last_is_end && end_count > 0 {
        v.push(diag(
            code("program_end_not_last"),
            "program_end_not_last: опы после PEND запрещены (SPEC §3)",
        ));
    }
    v
}

fn op_src(op: &Op) -> &eid_core::ast::SrcSpan {
    match op {
        Op::Rapid { src, .. }
        | Op::Linear { src, .. }
        | Op::Arc { src, .. }
        | Op::Dwell { src, .. }
        | Op::ToolChange { src, .. }
        | Op::ProgramStop { src, .. }
        | Op::ProgramEnd { src }
        | Op::WcsDefine { src, .. }
        | Op::WcsSetHere { src, .. }
        | Op::Raw { src, .. } => src,
    }
}

fn check_motion_state(
    table: &FanucEmissionLegality,
    v: &mut Vec<Diagnostic>,
    state: &eid_core::ast::State,
    src: &eid_core::ast::SrcSpan,
    need_feed: bool,
) {
    if need_feed && state.feed.is_none() {
        v.push(diag_src(
            table
                .code_of("motion_feed_none")
                .expect("emission-legality: условие `motion_feed_none` не объявлено в таблице"),
            "motion_feed_none: linear/arc без модального feed (SPEC §3/§4)",
            src,
        ));
    }
    check_wcs(table, v, &state.wcs, src);
}

fn check_wcs(
    table: &FanucEmissionLegality,
    v: &mut Vec<Diagnostic>,
    wcs: &Wcs,
    src: &eid_core::ast::SrcSpan,
) {
    if wcs_word(*wcs).is_none() {
        v.push(diag_src(
            table.code_of("wcs_outside_g54_g59").expect("emission-legality: условие `wcs_outside_g54_g59` не объявлено в таблице"),
            "wcs_outside_g54_g59: state.wcs = G59.1/.2/.3 — в словаре Fanuc кодов нет (COVERAGE §5.1; G54.1 — другая таблица; профиль-маппинг — этап 8, НЕ дефолт)",
            src,
        ));
    }
}

// ===========================================================================
// Писатель
// ===========================================================================

struct Writer<'a> {
    program: &'a Program,
    text: String,
    // Модальное состояние вывода (то, что увидит reparse).
    cur_wcs: Wcs,
    cur_plane: GCode,
    cur_spindle: Spindle,
    cur_coolant: Coolant,
    cur_feed: Option<Num>,
    cur_tool: Option<u32>,
}

impl<'a> Writer<'a> {
    fn new(program: &'a Program) -> Self {
        let cur_wcs = program
            .frame_preset
            .as_ref()
            .map(|p| p.wcs)
            .unwrap_or(Wcs::G54);
        Self {
            program,
            text: String::new(),
            cur_wcs,
            cur_plane: GCode::PlaneXY,
            cur_spindle: Spindle::Off,
            cur_coolant: Coolant::default(),
            cur_feed: None,
            cur_tool: None,
        }
    }

    fn line(&mut self, s: impl Into<String>) {
        self.text.push_str(&s.into());
        self.text.push('\n');
    }

    fn run(
        &mut self,
        table: &FanucEmissionLegality,
        warnings: &mut Vec<String>,
    ) -> Result<(), Vec<Diagnostic>> {
        // 1. Заголовок — всегда явно (units §2.5; G90/G94 — дефолты; G17;
        //    G91.1 — инкрементальные центры pinned, см. модуль-док).
        let units_word = if self.program.units == Units::In {
            "G20"
        } else {
            "G21"
        };
        self.line(format!("{units_word} G90 G94 G17 G91.1"));
        // 2. path_mode — всегда явно, одна строка (SPEC §3: одно на программу).
        //    Fanuc value shift (COVERAGE §2.2/§5.1): exact_stop → G61 (1:1 по
        //    значению); exact_path → G61 консервативно + warning (профиль
        //    bare G64 — этап 8); blend(t) → bare G64 + warning при t.
        match &self.program.path_mode {
            eid_core::ast::PathMode::ExactStop => self.line("G61"),
            eid_core::ast::PathMode::ExactPath => {
                self.line("G61");
                if let Some(t) = table.warning_of("path_mode_exact_path") {
                    warnings.push(t.to_string());
                }
            }
            eid_core::ast::PathMode::Blend { t } => {
                self.line("G64");
                if t.is_some() {
                    if let Some(w) = table.warning_of("path_mode_blend_t") {
                        warnings.push(w.to_string());
                    }
                }
            }
        }
        // 3. frame_preset: выборка WCS, затем G92 первым блоком (EQUIVALENCE
        //    §1.4); без запекания в G10. G92-hygiene (G92.1 первым блоком) —
        //    флаг профиля, НЕ дефолт — этап 8 (COVERAGE §5.1).
        if let Some(pr) = &self.program.frame_preset {
            let w = wcs_word(pr.wcs).ok_or_else(|| {
                vec![diag(
                    table
                        .code_of("wcs_outside_g54_g59")
                        .expect("emission-legality: условие не объявлено"),
                    "frame_preset.wcs вне словаря Fanuc (G59.x) — кодов нет (COVERAGE §5.1)",
                )]
            })?;
            self.line(w);
            self.line(format!(
                "G92 X{} Y{} Z{}",
                fmt_num(pr.values.x.value()),
                fmt_num(pr.values.y.value()),
                fmt_num(pr.values.z.value())
            ));
        }
        // 4. Опы.
        for op in &self.program.ops {
            self.op(op, warnings)?;
        }
        Ok(())
    }

    /// Дельты снапшота состояния перед строкой опа (фиксированный порядок:
    /// WCS → шпиндель → СОЖ; plane — только у дуг).
    fn pre_state(&mut self, state: &eid_core::ast::State) -> Result<(), Vec<Diagnostic>> {
        if state.wcs != self.cur_wcs {
            let w = wcs_word(state.wcs).ok_or_else(|| {
                vec![diag(
                ErrorCode::UnsupportedSyntax,
                "state.wcs вне словаря Fanuc — wcs-слово не эмитится (валидатор отсекает ранее)",
            )]
            })?;
            self.line(w);
            self.cur_wcs = state.wcs;
        }
        if !spindle_eq(&state.spindle, &self.cur_spindle) {
            let line = match &state.spindle {
                Spindle::Off => "M5".to_string(),
                Spindle::Cw(rpm) => format!("M3 S{}", fmt_num(rpm.value())),
                Spindle::Ccw(rpm) => format!("M4 S{}", fmt_num(rpm.value())),
            };
            self.line(line);
            self.cur_spindle = state.spindle.clone();
        }
        if state.coolant != self.cur_coolant {
            // Выключения (M9 гасит обе) — до включений; порядок M7, M8.
            let off_needed = (self.cur_coolant.flood && !state.coolant.flood)
                || (self.cur_coolant.mist && !state.coolant.mist);
            if off_needed {
                self.line("M9");
            }
            if state.coolant.mist {
                self.line("M7");
            }
            if state.coolant.flood {
                self.line("M8");
            }
            self.cur_coolant = state.coolant;
        }
        Ok(())
    }

    fn op(&mut self, op: &Op, warnings: &mut Vec<String>) -> Result<(), Vec<Diagnostic>> {
        match op {
            Op::Rapid { end, state, .. } | Op::Linear { end, state, .. } => {
                self.pre_state(state)?;
                self.check_tool_jump(state, warnings);
                let f = self.feed_word(state, warnings)?;
                let g = match (op, state.frame) {
                    (_, Frame::Machine) => {
                        if matches!(op, Op::Rapid { .. }) {
                            "G53 G0"
                        } else {
                            "G53 G1"
                        }
                    }
                    (Op::Rapid { .. }, _) => "G0",
                    (_, _) => "G1",
                };
                self.line(format!(
                    "{g} X{} Y{} Z{}{f}",
                    fmt_num(end.x.value()),
                    fmt_num(end.y.value()),
                    fmt_num(end.z.value())
                ));
                Ok(())
            }
            Op::Arc {
                start: StartPoint::Known(start),
                end,
                center,
                normal,
                dir,
                sweep_deg,
                pitch,
                state,
                ..
            } => {
                self.pre_state(state)?;
                self.check_tool_jump(state, warnings);
                // Плоскость из канонической нормали (валидатор отсёк не-ось).
                let (plane, a0, a1, _naxis) =
                    plane_of(&[normal.x.clone(), normal.y.clone(), normal.z.clone()])
                        .map_err(|m| vec![diag(ErrorCode::InvalidInput, m)])?;
                if plane != self.cur_plane {
                    self.line(plane_word(plane));
                    self.cur_plane = plane;
                }
                // Центр: ТОЛЬКО in-plane пара, инкремент от старта (G91.1 в
                // заголовке); normal-компонента не эмитится (warning при
                // отличии от старта — канон-форма требует ⟂-компоненту = старта).
                let sv = to_f64(start);
                let cv = to_f64(center);
                let mut offs = [0.0f64; 3];
                offs[a0] = cv[a0] - sv[a0];
                offs[a1] = cv[a1] - sv[a1];
                let dn = cv[[0, 1, 2][_naxis]] - sv[[0, 1, 2][_naxis]];
                if dn.abs() > 0.001 {
                    warnings.push(format!(
                        "arc: normal-компонента центра {dn:+.4} не эмитится (каноническая форма требует center ⟂-компоненту = старта); геометрия дуги не меняется"
                    ));
                }
                let f = self.feed_word(state, warnings)?;
                let g = if *dir == Dir::Cw { "G2" } else { "G3" };
                // Слова смещений в порядке осей X→I, Y→J, Z→K (конвенция
                // grbl/linuxcnc; I/J/K literal X/Y/Z): ровно две in-plane
                // оси, G18 → I+K, G19 → J+K, G17 → I+J.
                let letter = ["I", "J", "K"];
                let mut arc_words = String::new();
                for ax in 0..3 {
                    if ax == a0 || ax == a1 {
                        arc_words.push_str(&format!("{}{} ", letter[ax], fmt_num(offs[ax])));
                    }
                }
                let arc_words = arc_words.trim_end().to_string();
                // Полный круг (sweep=360°, pitch=0) — БЕЗ end-слов (`G2 I_ J_`
                // — легальный Fanuc-формат; слова end==start перепарсер
                // квантует в zero-length drop). Винтовой полный оборот
                // (pitch≠0) — ТОЛЬКО end-слово вне-плоскостной оси (in-plane
                // наследуют старт). Частичная дуга — все три оси.
                let full_turn = (sweep_deg.value() - 360.0).abs() <= 1e-6;
                let helical_turn = full_turn && pitch.value().abs() > 1e-9;
                let axis_letter = ["X", "Y", "Z"][_naxis];
                let axes_part = if full_turn && !helical_turn {
                    String::new()
                } else if helical_turn {
                    let ev = to_f64(end);
                    format!("{}{} ", axis_letter, fmt_num(ev[_naxis]))
                } else {
                    format!(
                        "X{} Y{} Z{} ",
                        fmt_num(end.x.value()),
                        fmt_num(end.y.value()),
                        fmt_num(end.z.value())
                    )
                };
                self.line(format!("{g} {axes_part}{arc_words}{f}"));
                Ok(())
            }
            Op::Arc {
                start: StartPoint::Unknown,
                ..
            } => {
                // validate() отсекает такие программы ранее; защита.
                Err(vec![diag(
                    ErrorCode::RuntimeState,
                    "arc с start=unknown не эмитится (SPEC §2 Q11)",
                )])
            }
            Op::Dwell { seconds, state, .. } => {
                self.pre_state(state)?;
                self.check_tool_jump(state, warnings);
                if !seconds.value.is_finite() {
                    return Err(vec![diag(
                        ErrorCode::InvalidInput,
                        "dwell seconds: non-finite",
                    )]);
                }
                // Правило COVERAGE §5.1: seconds·1000 целое (в допуске) →
                // P = целые мс (decimal point ЗАПРЕЩЁН — DPI-домен P);
                // иначе sub-ms остаток → X-форма (float-секунды с точкой).
                let ms = seconds.value * 1000.0;
                if (ms - ms.round()).abs() < 1e-6 {
                    self.line(format!("G4 P{}", ms.round() as i64));
                } else {
                    self.line(format!("G4 X{}", fmt_num(seconds.value)));
                }
                Ok(())
            }
            Op::ToolChange { tool, state, .. } => {
                self.pre_state(state)?;
                // Fanuc-идиома: T## M06 в одном блоке (1:1, COVERAGE §5.1).
                self.line(format!("T{tool} M06"));
                self.cur_tool = Some(*tool);
                Ok(())
            }
            Op::ProgramStop {
                optional, state, ..
            } => {
                self.pre_state(state)?;
                self.check_tool_jump(state, warnings);
                self.line(if *optional { "M1" } else { "M0" });
                Ok(())
            }
            Op::ProgramEnd { .. } => {
                // Хвост M5/M9 НЕ эмитится (ruling COVERAGE §5: AST не хранит
                // пост-конечное состояние; Fanuc M30 сам глушит).
                // Дефолт M30 (Fanuc-конвенция rewind); профиль M02 — этап 8.
                self.line("M30");
                Ok(())
            }
            Op::WcsDefine {
                p, origin, state, ..
            } => {
                self.pre_state(state)?;
                self.check_tool_jump(state, warnings);
                self.line(format!(
                    "G10 L2 P{p} X{} Y{} Z{}",
                    fmt_num(origin.x.value()),
                    fmt_num(origin.y.value()),
                    fmt_num(origin.z.value())
                ));
                Ok(())
            }
            Op::WcsSetHere { .. } => {
                // Валидатор отсекает ранее (ловушка №3); защита полноты.
                Err(vec![diag(
                    ErrorCode::UnsupportedSemantics,
                    "wcs_set_here не эмитится в Fanuc: L20 = запись G54.1 Pn, НЕ «set here» (COVERAGE §5.1)",
                )])
            }
            Op::Raw { payload_hex, .. } => self.raw(payload_hex),
        }
    }

    /// F-слово при отличии модального feed от текущего выведенного; иначе "".
    /// F на rapid-кадре воспроизводится (меняет модальный feed — traverse
    /// развёртки §7.7-1 несёт feed кадра). Число — 4dp с точкой ВСЕГДА.
    fn feed_word(
        &mut self,
        state: &eid_core::ast::State,
        warnings: &mut Vec<String>,
    ) -> Result<String, Vec<Diagnostic>> {
        match &state.feed {
            Some(f) => {
                if !f.value.is_finite() {
                    return Err(vec![diag(ErrorCode::InvalidInput, "feed: non-finite")]);
                }
                let changed = match &self.cur_feed {
                    None => true,
                    Some(c) => c.value != f.value,
                };
                if changed {
                    self.cur_feed = Some(f.clone());
                    Ok(format!(" F{}", fmt_num(f.value)))
                } else {
                    Ok(String::new())
                }
            }
            None => {
                if self.cur_feed.is_some() {
                    // Несвязный снапшот: modal feed не может «исчезнуть».
                    warnings.push(
                        "state.feed = none при известном модальном feed — скачок снапшота не воспроизводим (F остаётся модальным)".into(),
                    );
                }
                Ok(String::new())
            }
        }
    }

    /// state.tool без ToolChange-опа — несвязный снапшот (eid-text AST не
    /// проверяет непрерывность состояния): воспроизвести нечем, warning.
    fn check_tool_jump(&mut self, state: &eid_core::ast::State, warnings: &mut Vec<String>) {
        if state.tool != self.cur_tool {
            warnings.push(format!(
                "state.tool = {:?} без ToolChange-опа — скачок снапшота не воспроизводим (выведенный tool = {:?})",
                state.tool, self.cur_tool
            ));
            // cur_tool не обновляем: в выводе инструмент = последний M06.
        }
    }

    /// RAW same-dialect fidelity: payload-hex → исходные байты строки ВКЛЮЧАЯ
    /// терминатор (grammar D9 v0.3) → дословно; внешний '\n' только если
    /// декод не кончается им.
    fn raw(&mut self, payload_hex: &str) -> Result<(), Vec<Diagnostic>> {
        if payload_hex.len() % 2 != 0 {
            return Err(vec![diag(
                ErrorCode::InvalidInput,
                "raw payload: bad hex length",
            )]);
        }
        let mut bytes = Vec::with_capacity(payload_hex.len() / 2);
        for i in (0..payload_hex.len()).step_by(2) {
            match u8::from_str_radix(&payload_hex[i..i + 2], 16) {
                Ok(b) => bytes.push(b),
                Err(_) => {
                    return Err(vec![diag(
                        ErrorCode::InvalidInput,
                        "raw payload: bad hex pair",
                    )])
                }
            }
        }
        let s = String::from_utf8_lossy(&bytes).into_owned();
        if s.ends_with('\n') {
            self.text.push_str(&s);
        } else {
            self.text.push_str(&s);
            self.text.push('\n');
        }
        Ok(())
    }
}

fn spindle_eq(a: &Spindle, b: &Spindle) -> bool {
    match (a, b) {
        (Spindle::Off, Spindle::Off) => true,
        (Spindle::Cw(x), Spindle::Cw(y)) | (Spindle::Ccw(x), Spindle::Ccw(y)) => {
            x.value() == y.value()
        }
        _ => false,
    }
}

fn plane_word(p: GCode) -> &'static str {
    match p {
        GCode::PlaneXY => "G17",
        GCode::PlaneZX => "G18",
        GCode::PlaneYZ => "G19",
        _ => "G17",
    }
}
