//! Эмиттер EID/L1 → текст LinuxCNC (Агент D, итерация 3; COVERAGE §5,
//! EQUIVALENCE §1.4). Детерминирован: один AST → одни байты (двойной emit
//! байт-в-байт); числа — 4 dp с verbatim-предпочтением (SPEC §10).
//!
//! Структура вывода:
//!  1. заголовок `G20|G21 G90 G94 G17 G91.1` — всегда явно (режим центров
//!     дуг — инкрементальный, COVERAGE §2.2 ABSORB-обратная сторона);
//!  2. path_mode — всегда явно: G61 / G61.1 / G64 [P<t>] (SPEC §3: одно
//!     значение на программу — одна строка после заголовка);
//!  3. frame_preset (EQUIVALENCE §1.4): выборка preset.wcs, затем G92
//!     X.. Y.. Z.. — до любого motion-опа и любой смены WCS; запекание в
//!     G10 запрещено (golden_preset_no_bake);
//!  4. опы по порядку: перед каждым опом — дельты его снапшота (WCS,
//!     шпиндель, СОЖ; фиксированный порядок), затем строка опа;
//!  5. дуги: плоскость из normal (+Z→G17, +Y→G18, +X→G19; не-осевой или
//!     отрицательный normal → E_INVALID_INPUT), смена плоскости — отдельной
//!     строкой перед дугой; центр — ТОЛЬКО in-plane пара I/J/K
//!     (G17→I+J, G18→I+K, G19→J+K), инкрементальная от старта (G91.1 в
//!     заголовке), normal-компонента не эмитится; R не эмитится
//!     (EQUIVALENCE §4: адаптеры эмитят всегда IJK); off-plane координата
//!     конца (напр. Z при G17) даёт винтовую компоненту сама;
//!  6. motion: все три оси конца на каждом ходе; frame=machine → префикс
//!     `G53 ` (одномодальный); F — при отличии модального feed от текущего
//!     выведенного (обычный rapid — без F; F-слово на rapid-кадре
//!     воспроизводится, т.к. меняет модальный feed);
//!  7. TOOL_CHANGE → `T<n> M6` одной строкой (детерминированно); STOP →
//!     M0/M1; PEND → M2; WCS_DEFINE → `G10 L2 P<n> X.. Y.. Z..`;
//!     WCS_SET_HERE → `G10 L20 P<n> X.. Y.. Z..`; DWELL → `G4 P<sec>`.
//!
//! RAW: payload декодируется и выводится дословно (same-dialect fidelity,
//! grammar D9 — без нормализации). Кросс-диалектный запрет RAW
//! (`E_RAW_UNTRANSLATABLE`) — НЕ здесь: это правило translate-конвейера
//! (Агент E, CLI), same-dialect эмиттер обязан воспроизводить байты.
//!
//! Warnings (не отказы) — в diagnostics-буфер адаптера
//! (`LinuxCncAdapter::take_diagnostics`): потеря normal-компоненты центра
//! дуги (каноническая форма требует center ⟂-компоненту = старта),
//! не-UTF8 payload (lossy-вывод), скачок state.tool / state.feed=none без
//! порождающего кода (несвязные снапшоты eid-text AST — парсер EID не
//! проверяет непрерывность состояния между опами).
//!
//! Валидация: перед эмиссией — `eid_core::parser::validate` (каноническая
//! форма §3: zero-length отсутствуют, feed на linear/arc, PEND ровно один
//! раз и последний, геометрия §6, связывание Q11) — отказ пробрасывается
//! как Err(Vec<Diagnostic>).

use crate::LinuxCncAdapter;
use crate::canonicalize::{plane_def, to_f64, Plane};
use eid_core::ast::{Coolant, Dir, Frame, Num, Op, Program, Spindle, StartPoint, Units, Wcs};
use eid_core::diag::{Diagnostic, ErrorCode, Stage};
use eid_core::emitter::fmt_num;
use eid_core::parser;

/// Результат эмиссии: текст + warnings (не отказы).
pub struct EmitOut {
    pub text: String,
    pub warnings: Vec<Diagnostic>,
}

// ===========================================================================
// EmissionLegality — ConversionTarget-паттерн (MLIR), зеркально eid-grbl
// (COVERAGE §5, LinuxCNC-колонка): легальность эмиссии — ДАННЫЕ, не
// размазанные проверки; валидатор берёт ErrorCode ТОЛЬКО из таблицы
// (единый путь отказа). E_RAW_UNTRANSLATABLE в таблицу НЕ входит —
// translate-путь (Агент E, SPEC §8.2), не same-dialect эмиссия.
// LinuxCNC-специфика: словарь полный — ВСЕ 10 видов опов легальны;
// path_mode/WCS вне словаря невозможны (G54–G59.3, G61/G61.1/G64 — весь
// словарь целевого; «wcs вне enum» — закрытый тип, недостижимо).
// ===========================================================================

/// Статическая таблица легальности эмиссии LinuxCNC.
#[derive(Debug, Clone, PartialEq)]
pub struct EmissionLegality {
    /// Виды опов, представимые ходами LinuxCNC (имена = op_kind()).
    pub legal: Vec<&'static str>,
    /// Условия отказа: (имя условия, код класса легальности).
    pub illegal: Vec<(&'static str, ErrorCode)>,
}

impl EmissionLegality {
    /// Код отказа по имени условия; None = условие не объявлено.
    pub fn code_of(&self, condition: &str) -> Option<ErrorCode> {
        self.illegal.iter().find(|(c, _)| *c == condition).map(|(_, code)| *code)
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

/// Имя вида опа — ключ таблицы легальности (и словарь manifest r4_expect).
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

/// Валидация легальности эмиссии против таблицы (fail-fast, COVERAGE §5).
/// Коды отказов берутся ИСКЛЮЧИТЕЛЬНО из таблицы — единый путь отказа.
fn validate_linuxcnc(table: &EmissionLegality, program: &Program) -> Vec<Diagnostic> {
    // Условие обязано быть объявлено в таблице — иначе баг объявления,
    // не паника пути.
    let code = |cond: &'static str| {
        table.code_of(cond).unwrap_or_else(|| {
            panic!("emission-legality: условие `{cond}` не объявлено в таблице")
        })
    };
    let mut v = Vec::new();
    let mut last_is_end = false;
    for op in &program.ops {
        last_is_end = matches!(op, Op::ProgramEnd { .. });
        // Недостижимо: все 10 видов опов легальны для LinuxCNC (COVERAGE §5);
        // ветка — защита полноты таблицы (новый вид опа ⇒ дополнить таблицу).
        if !table.is_legal_kind(op_kind(op)) {
            v.push(diag(
                table.code_of("op_kind_not_legal").unwrap_or(ErrorCode::UnsupportedSyntax),
                format!(
                    "op_kind_not_legal: `{}` не легален для цели LinuxCNC",
                    op_kind(op)
                ),
            ));
        }
        match op {
            Op::Arc { start: StartPoint::Unknown, src, .. } => v.push(diag_src(
                code("arc_unknown_start"),
                "arc_unknown_start: дуга требует числового старта (SPEC §2 Q11)",
                Some(src.clone()),
            )),
            Op::Arc { normal, src, state, .. } => {
                // Не-каноническая нормаль (не единичная +ось) — §6.
                let c = [normal.x.value, normal.y.value, normal.z.value];
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
                    v.push(diag_src(
                        code("arc_normal_non_canonical"),
                        format!(
                            "arc_normal_non_canonical: normal = ({:.4},{:.4},{:.4}) — не единичная +ось (SPEC §6)",
                            c[0], c[1], c[2]
                        ),
                        Some(src.clone()),
                    ));
                }
                // G53 + дуга: machine-frame ходы — прямые (LinuxCNC G53
                // требует G0/G1).
                if state.frame == Frame::Machine {
                    v.push(diag_src(
                        code("machine_frame_arc"),
                        "machine_frame_arc: G53 с дугой не эмитится (ходы machine-frame прямые)",
                        Some(src.clone()),
                    ));
                }
                // Канон-форма §3: arc требует модального feed.
                if state.feed.is_none() {
                    v.push(diag_src(
                        code("motion_feed_none"),
                        "motion_feed_none: arc без модального feed (SPEC §3/§4)",
                        Some(src.clone()),
                    ));
                }
            }
            Op::Linear { state, src, .. } => {
                if state.feed.is_none() {
                    v.push(diag_src(
                        code("motion_feed_none"),
                        "motion_feed_none: linear без модального feed (SPEC §3/§4)",
                        Some(src.clone()),
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
                        Some(src.clone()),
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

/// Точка входа. `Err` — нарушена каноническая форма (SPEC §3), нелегальность
/// эмиссии (COVERAGE §5, через таблицу) или неосевой normal дуги; warnings
/// возвращаются отдельно.
pub fn emit_program(program: &Program) -> Result<EmitOut, Vec<Diagnostic>> {
    // Каноническая форма до эмиссии (R1: parse(emit(x)) — тот же AST).
    parser::validate(program)?;
    // Magic — ровно "EID/L1 v0.3" (grammar v0.3; SPEC §10.4: v0.2 невалиден).
    if program.eid_version != crate::parser::EID_VERSION {
        return Err(vec![diag(
            ErrorCode::InvalidInput,
            format!(
                "eid_version `{}` не поддерживается (ожидается {})",
                program.eid_version,
                crate::parser::EID_VERSION
            ),
        )]);
    }
    // Легальность эмиссии — против таблицы (COVERAGE §5), fail-fast до прохода.
    let table = LinuxCncAdapter::default().emission_legality();
    let problems = validate_linuxcnc(&table, program);
    if !problems.is_empty() {
        return Err(problems);
    }
    let mut w = Writer::new(program);
    w.run()?;
    Ok(EmitOut { text: w.text, warnings: w.warnings })
}

// ===========================================================================
// Числа: verbatim-предпочтение (зеркало eid-core emitter::emit_num)
// ===========================================================================

fn num_text(n: &Num) -> String {
    if n.value.is_finite() && parser::parse_num(&n.text).is_some() && n.text != "-0.0000" {
        n.text.clone()
    } else {
        fmt_num(n.value)
    }
}

fn diag(code: ErrorCode, msg: impl Into<String>) -> Diagnostic {
    Diagnostic { code, stage: Stage::Emit, message: msg.into(), src: None, options: Vec::new(), limit: None }
}

fn diag_src(
    code: ErrorCode,
    msg: impl Into<String>,
    src: Option<eid_core::ast::SrcSpan>,
) -> Diagnostic {
    Diagnostic { code, stage: Stage::Emit, message: msg.into(), src, options: Vec::new(), limit: None }
}

fn finite3(a: &Num, b: &Num, c: &Num, what: &'static str) -> Result<(), Vec<Diagnostic>> {
    for n in [a, b, c] {
        if !n.value.is_finite() {
            return Err(vec![diag(
                ErrorCode::InvalidInput,
                format!("{what}: non-finite value"),
            )]);
        }
    }
    Ok(())
}

// ===========================================================================
// Писатель
// ===========================================================================

struct Writer<'a> {
    program: &'a Program,
    text: String,
    warnings: Vec<Diagnostic>,
    // Модальное состояние вывода (то, что увидит reparse).
    cur_wcs: Wcs,
    cur_plane: Plane,
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
            warnings: Vec::new(),
            cur_wcs,
            cur_plane: Plane::G17,
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

    fn warn(&mut self, code: ErrorCode, msg: impl Into<String>) {
        self.warnings.push(diag(code, msg));
    }

    fn run(&mut self) -> Result<(), Vec<Diagnostic>> {
        // 1. Заголовок — всегда явно.
        let units_word = if self.program.units == Units::In { "G20" } else { "G21" };
        self.line(format!("{units_word} G90 G94 G17 G91.1"));
        // 2. path_mode — всегда явно, одна строка (SPEC §3: одно на программу).
        match &self.program.path_mode {
            eid_core::ast::PathMode::ExactPath => self.line("G61"),
            eid_core::ast::PathMode::ExactStop => self.line("G61.1"),
            eid_core::ast::PathMode::Blend { t } => match t {
                Some(v) => {
                    if !v.value.is_finite() {
                        return Err(vec![diag(ErrorCode::InvalidInput, "path_mode blend t: non-finite")]);
                    }
                    self.line(format!("G64 P{}", num_text(v)));
                }
                None => self.line("G64"),
            },
        }
        // 3. frame_preset: выборка WCS, затем G92 (EQUIVALENCE §1.4); без
        //    запекания в G10 (golden_preset_no_bake).
        if let Some(pr) = &self.program.frame_preset {
            finite3(&pr.values.x, &pr.values.y, &pr.values.z, "frame_preset value")?;
            self.line(wcs_word(pr.wcs));
            self.line(format!(
                "G92 X{} Y{} Z{}",
                num_text(&pr.values.x),
                num_text(&pr.values.y),
                num_text(&pr.values.z)
            ));
        }
        // 4. Опы.
        for op in &self.program.ops {
            self.op(op)?;
        }
        Ok(())
    }

    /// Дельты снапшота состояния перед строкой опа (фиксированный порядок:
    /// WCS → шпиндель → СОЖ; plane — только у дуг).
    fn pre_state(&mut self, state: &eid_core::ast::State) {
        if state.wcs != self.cur_wcs {
            let w = wcs_word(state.wcs);
            self.line(w);
            self.cur_wcs = state.wcs;
        }
        if state.spindle != self.cur_spindle {
            let line = match &state.spindle {
                Spindle::Off => "M5".to_string(),
                Spindle::Cw(rpm) => format!("M3 S{}", num_text(rpm)),
                Spindle::Ccw(rpm) => format!("M4 S{}", num_text(rpm)),
            };
            self.line(line);
            self.cur_spindle = state.spindle.clone();
        }
        if state.coolant != self.cur_coolant {
            // Выключения (M9 гасит обе) — до включений; порядок включений
            // фиксированный: M7 (mist), M8 (flood).
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
    }

    fn op(&mut self, op: &Op) -> Result<(), Vec<Diagnostic>> {
        match op {
            Op::Rapid { end, state, .. } => {
                self.pre_state(state);
                self.check_tool_jump(state);
                let f = self.feed_word(state)?;
                let g = if state.frame == Frame::Machine { "G53 G0" } else { "G0" };
                self.line(format!(
                    "{g} X{} Y{} Z{}{f}",
                    num_text(&end.x),
                    num_text(&end.y),
                    num_text(&end.z)
                ));
                Ok(())
            }
            Op::Linear { end, state, .. } => {
                self.pre_state(state);
                self.check_tool_jump(state);
                let f = self.feed_word(state)?;
                let g = if state.frame == Frame::Machine { "G53 G1" } else { "G1" };
                self.line(format!(
                    "{g} X{} Y{} Z{}{f}",
                    num_text(&end.x),
                    num_text(&end.y),
                    num_text(&end.z)
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
                self.pre_state(state);
                self.check_tool_jump(state);
                // Плоскость из normal (+ось, SPEC §6).
                let plane = plane_of(normal)?;
                if plane != self.cur_plane {
                    self.line(plane_word(plane));
                    self.cur_plane = plane;
                }
                // Центр: ТОЛЬКО in-plane пара, инкрементальная от старта
                // (G91.1 в заголовке); normal-компонента не эмитится.
                let (n_axis, u, v) = plane_def(plane);
                let sv = to_f64(start);
                let cv = to_f64(center);
                let mut offs = [0.0f64; 3];
                offs[u] = cv[u] - sv[u];
                offs[v] = cv[v] - sv[v];
                let dn = cv[n_axis] - sv[n_axis];
                if dn.abs() > 0.001 {
                    self.warn(
                        ErrorCode::UnsupportedSemantics,
                        format!(
                            "дуга: normal-компонента центра {dn:+.4} не эмитится (каноническая форма требует center ⟂-компоненту = старта); геометрия дуги не меняется"
                        ),
                    );
                }
                let f = self.feed_word(state)?;
                // G53+дуга отсекается раньше — validate_linuxcnc
                // (условие machine_frame_arc таблицы легальности); ветка
                // здесь недостижима (аудит #6) — удалена.
                let g = if *dir == Dir::Cw { "G2" } else { "G3" };
                // Слова смещений в порядке осей X→I, Y→J, Z→K (LinuxCNC-
                // конвенция, независимо от (u,v) пары плоскости); эмитятся
                // ровно две in-plane оси (normal-ось не эмитится вовсе).
                let letter = ["I", "J", "K"];
                let mut arc_words = String::new();
                for ax in 0..3 {
                    if ax == u || ax == v {
                        arc_words.push_str(&format!("{}{} ", letter[ax], fmt_num(offs[ax])));
                    }
                }
                let arc_words = arc_words.trim_end().to_string();
                // Полный круг (CORPUS #08, зеркально eid-grbl): sweep=360° и
                // pitch=0 → БЕЗ слов осей (`G2 I.. J..` — легальный LinuxCNC-
                // формат; с end-словами parse-back отбросил бы дугу как
                // zero-sweep explicit-end). Винтовой полный оборот (sweep=
                // 360°, pitch≠0) — только off-plane end-слово оси (in-plane
                // наследуют старт). Частичная дуга — все три оси.
                let full_turn = (sweep_deg.value - 360.0).abs() <= 1e-9;
                let helical_turn = full_turn && pitch.value.abs() > 1e-12;
                let axes_part = if full_turn && !helical_turn {
                    String::new()
                } else if helical_turn {
                    let ev = to_f64(end);
                    let letter = ["X", "Y", "Z"];
                    format!("{}{} ", letter[n_axis], fmt_num(ev[n_axis]))
                } else {
                    format!(
                        "X{} Y{} Z{} ",
                        num_text(&end.x),
                        num_text(&end.y),
                        num_text(&end.z)
                    )
                };
                self.line(format!("{g} {axes_part}{arc_words}{f}"));
                Ok(())
            }
            Op::Arc { start: StartPoint::Unknown, .. } => {
                // validate() отвергает такие программы ранее; защита.
                Err(vec![diag(
                    ErrorCode::RuntimeState,
                    "дуга с start=unknown не эмитится (SPEC §2 Q11)",
                )])
            }
            Op::Dwell { seconds, state, .. } => {
                self.pre_state(state);
                self.check_tool_jump(state);
                if !seconds.value.is_finite() {
                    return Err(vec![diag(ErrorCode::InvalidInput, "dwell seconds: non-finite")]);
                }
                self.line(format!("G4 P{}", num_text(seconds)));
                Ok(())
            }
            Op::ToolChange { tool, state, .. } => {
                self.pre_state(state);
                self.line(format!("T{tool} M6"));
                self.cur_tool = Some(*tool);
                Ok(())
            }
            Op::ProgramStop { optional, state, .. } => {
                self.pre_state(state);
                self.check_tool_jump(state);
                self.line(if *optional { "M1" } else { "M0" });
                Ok(())
            }
            Op::ProgramEnd { .. } => {
                self.line("M2");
                Ok(())
            }
            Op::WcsDefine { p, origin, state, .. } => {
                self.pre_state(state);
                self.check_tool_jump(state);
                finite3(&origin.x, &origin.y, &origin.z, "wcs_define origin")?;
                self.line(format!(
                    "G10 L2 P{p} X{} Y{} Z{}",
                    num_text(&origin.x),
                    num_text(&origin.y),
                    num_text(&origin.z)
                ));
                Ok(())
            }
            Op::WcsSetHere { p, delta, state, .. } => {
                self.pre_state(state);
                self.check_tool_jump(state);
                finite3(&delta.x, &delta.y, &delta.z, "wcs_set_here delta")?;
                self.line(format!(
                    "G10 L20 P{p} X{} Y{} Z{}",
                    num_text(&delta.x),
                    num_text(&delta.y),
                    num_text(&delta.z)
                ));
                Ok(())
            }
            Op::Raw { payload_hex, .. } => self.raw(payload_hex),
        }
    }

    /// F-слово при отличии модального feed от текущего выведенного; иначе "".
    fn feed_word(&mut self, state: &eid_core::ast::State) -> Result<String, Vec<Diagnostic>> {
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
                    Ok(format!(" F{}", num_text(f)))
                } else {
                    Ok(String::new())
                }
            }
            None => {
                if self.cur_feed.is_some() {
                    // Несвязный снапшот: modal feed не может «исчезнуть».
                    self.warn(
                        ErrorCode::UnsupportedSemantics,
                        "state.feed = none при известном модальном feed — скачок снапшота не воспроизводим (F остаётся модальным)",
                    );
                }
                Ok(String::new())
            }
        }
    }

    /// state.tool без ToolChange-опа — несвязный снапшот (eid-text AST не
    /// проверяет непрерывность состояния): воспроизвести нечем, warning.
    fn check_tool_jump(&mut self, state: &eid_core::ast::State) {
        if state.tool != self.cur_tool {
            self.warn(
                ErrorCode::UnsupportedSemantics,
                format!(
                    "state.tool = {:?} без ToolChange-опа — скачок снапшота не воспроизводим (выведенный tool = {:?})",
                    state.tool, self.cur_tool
                ),
            );
            // cur_tool не обновляем: в выводе инструмент = последний M6.
        }
    }

    fn raw(&mut self, payload_hex: &str) -> Result<(), Vec<Diagnostic>> {
        if payload_hex.len() % 2 != 0
            || !payload_hex
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(&c))
        {
            return Err(vec![diag(
                ErrorCode::InvalidInput,
                format!("raw payload: bad hex `{payload_hex}`"),
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
        // D9 v0.15: payload включает терминатор — декодированное выводится
        // дословно; дополнительный NL — только если декод не кончается им
        // (иначе следующий оп склеился бы в ту же строку).
        let finish = |w: &mut Self, s: String| {
            if s.ends_with('\n') {
                w.text.push_str(&s);
            } else {
                w.text.push_str(&s);
                w.text.push('\n');
            }
        };
        match String::from_utf8(bytes) {
            Ok(s) => finish(self, s),
            Err(e) => {
                self.warn(
                    ErrorCode::UnsupportedSemantics,
                    "raw payload: не-UTF8 байты — вывод lossy (fidelity байт нарушена)",
                );
                finish(self, String::from_utf8_lossy(e.as_bytes()).into_owned());
            }
        }
        Ok(())
    }
}

fn wcs_word(w: Wcs) -> &'static str {
    match w {
        Wcs::G54 => "G54",
        Wcs::G55 => "G55",
        Wcs::G56 => "G56",
        Wcs::G57 => "G57",
        Wcs::G58 => "G58",
        Wcs::G59 => "G59",
        Wcs::G59_1 => "G59.1",
        Wcs::G59_2 => "G59.2",
        Wcs::G59_3 => "G59.3",
    }
}

fn plane_word(p: Plane) -> &'static str {
    match p {
        Plane::G17 => "G17",
        Plane::G18 => "G18",
        Plane::G19 => "G19",
    }
}

/// normal должен быть единичной +осью (SPEC §6: канонический знак +axis);
/// не-осевой или отрицательный → E_INVALID_INPUT.
fn plane_of(normal: &eid_core::ast::Vec3) -> Result<Plane, Vec<Diagnostic>> {
    let c = [normal.x.value, normal.y.value, normal.z.value];
    let (mut best, mut bi) = (0.0f64, 0usize);
    for (i, &v) in c.iter().enumerate() {
        if v.abs() > best {
            best = v.abs();
            bi = i;
        }
    }
    let axial = (best - 1.0).abs() <= 0.001
        && c[(bi + 1) % 3].abs() <= 0.001
        && c[(bi + 2) % 3].abs() <= 0.001;
    if !axial || c[bi] < 0.0 {
        return Err(vec![diag(
            ErrorCode::InvalidInput,
            format!(
                "дуга: normal = ({:.4},{:.4},{:.4}) — не единичная +ось (SPEC §6: G17→+Z, G18→+Y, G19→+X)",
                c[0], c[1], c[2]
            ),
        )]);
    }
    Ok(match bi {
        2 => Plane::G17,
        1 => Plane::G18,
        _ => Plane::G19,
    })
}
