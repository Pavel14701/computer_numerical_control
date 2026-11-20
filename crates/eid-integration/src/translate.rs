//! translate — live-конвейер CLI (итерация 4): registry адаптеров диалектов +
//! parse(источник) → emit(цель). Реализует критерий MVP #9/#10 (demo round-trip).
//!
//! Реальные адаптеры: GrblAdapter / LinuxCncAdapter / EidTextAdapter (eid-l1) —
//! вызываются через trait object (`&dyn Adapter`, freeze v0.1). grbl-hal/fluidnc —
//! pending (Агент C, семейство GRBL). Профиль станка в конвейер не входит:
//! сигнатуры Adapter (v0.1) профиль не принимают; профиль консюмят verify/sim
//! (structural-проверки) и CLI (display).

use eid_core::adapter::Adapter;
use eid_core::ast::Program;
use eid_core::diag::Diagnostic;
use eid_grbl::GrblAdapter;
use eid_linuxcnc::LinuxCncAdapter;
use std::path::Path;

/// Имя канонического текстового формата EID (registry-имя EidTextAdapter).
pub const EID_TEXT_DIALECT: &str = "eid-l1";

// ---------------------------------------------------------------------------
// Registry адаптеров
// ---------------------------------------------------------------------------

/// Обёртка registry: конкретный адаптер + буферы notices/warnings
/// (trait v0.1 не имеет канала для Ok-диагностик; буферы — паттерн адаптеров).
pub enum AnyAdapter {
    Grbl(GrblAdapter),
    LinuxCnc(LinuxCncAdapter),
    EidText(eid_core::adapter::EidTextAdapter),
}

impl AnyAdapter {
    /// Registry по каноническому имени диалекта ("grbl-1.1", "linuxcnc",
    /// "eid-l1"); алиас "grbl" = "grbl-1.1" (CLI, критерий #9).
    pub fn for_name(name: &str) -> Result<AnyAdapter, TranslateError> {
        match name {
            "grbl-1.1" | "grbl" => Ok(AnyAdapter::Grbl(GrblAdapter::new())),
            "linuxcnc" => Ok(AnyAdapter::LinuxCnc(LinuxCncAdapter::new())),
            "eid-l1" => Ok(AnyAdapter::EidText(eid_core::adapter::EidTextAdapter::new())),
            "grbl-hal" | "fluidnc" => Err(TranslateError::AdapterPending {
                dialect: name.to_string(),
                message: format!(
                    "adapter pending (C iteration 4+: семейство GRBL); доступно: grbl-1.1, linuxcnc, eid-l1"
                ),
            }),
            other => Err(TranslateError::UnknownDialect(other.to_string())),
        }
    }

    /// Имя диалекта адаптера (как `Adapter::dialect_name`).
    pub fn name(&self) -> &'static str {
        self.as_adapter().dialect_name()
    }

    fn as_adapter(&self) -> &dyn Adapter {
        match self {
            AnyAdapter::Grbl(a) => a,
            AnyAdapter::LinuxCnc(a) => a,
            AnyAdapter::EidText(a) => a,
        }
    }

    /// Notices последнего parse (drop-with-notice, SPEC §3) — единый строковый
    /// формат для CLI/sim.
    pub fn take_notices(&mut self) -> Vec<String> {
        match self {
            AnyAdapter::Grbl(a) => a
                .take_notices()
                .into_iter()
                .map(|n| format!("{:?}: {} (op {})", n.kind, n.message, n.op_index))
                .collect(),
            AnyAdapter::LinuxCnc(a) => a
                .take_notices()
                .into_iter()
                .map(|n| format!("{:?}: {} (op {})", n.kind, n.message, n.op_index))
                .collect(),
            AnyAdapter::EidText(a) => a
                .take_notices()
                .into_iter()
                .map(|n| format!("{:?}: {} (op {})", n.kind, n.message, n.op_index))
                .collect(),
        }
    }

    /// Emit-стадия warnings последнего emit (Q3: потеря метаданных — не отказ).
    pub fn take_emit_diagnostics(&mut self) -> Vec<Diagnostic> {
        match self {
            AnyAdapter::LinuxCnc(a) => a.take_diagnostics(),
            _ => Vec::new(),
        }
    }
}

impl Adapter for AnyAdapter {
    fn dialect_name(&self) -> &'static str {
        self.as_adapter().dialect_name()
    }

    fn parse(&self, src: &str, file: &str) -> Result<Program, Vec<Diagnostic>> {
        self.as_adapter().parse(src, file)
    }

    fn emit(&self, program: &Program) -> Result<String, Vec<Diagnostic>> {
        self.as_adapter().emit(program)
    }
}

// ---------------------------------------------------------------------------
// Ошибки конвейера
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum TranslateError {
    /// Имя диалекта вне registry.
    UnknownDialect(String),
    /// Диалект известен, адаптер ещё не реализован.
    AdapterPending { dialect: String, message: String },
    /// Стадия parse (источник).
    Parse(Vec<Diagnostic>),
    /// Стадия emit (цель) — напр. #27 tool_change → grbl-1.1 (r4_expect).
    Emit(Vec<Diagnostic>),
    /// Файловый ввод-вывод.
    Io(String),
}

impl TranslateError {
    /// Диагностики стадии parse/emit (пусто для UnknownDialect/Pending/Io).
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            TranslateError::Parse(v) | TranslateError::Emit(v) => v,
            _ => &[],
        }
    }

    /// Стадия отказа ("parse"/"emit"/"config"/"io") — для CLI/тестов.
    pub fn stage(&self) -> &'static str {
        match self {
            TranslateError::Parse(_) => "parse",
            TranslateError::Emit(_) => "emit",
            TranslateError::UnknownDialect(_) | TranslateError::AdapterPending { .. } => "config",
            TranslateError::Io(_) => "io",
        }
    }
}

/// Единый формат печати диагностики: code + message + src (CLI, e2e).
pub fn format_diagnostic(d: &Diagnostic) -> String {
    let src = match &d.src {
        Some(s) => format!(
            " @ {file}:{ls}.{cs}-{le}.{ce}",
            file = s.file,
            ls = s.line_start,
            cs = s.col_start,
            le = s.line_end,
            ce = s.col_end
        ),
        None => String::new(),
    };
    let mut s = format!("{}: {}{}", d.code.as_str(), d.message, src);
    for o in &d.options {
        s.push_str(&format!("\n  вариант: {o}"));
    }
    s
}

// ---------------------------------------------------------------------------
// Конвейер
// ---------------------------------------------------------------------------

/// Результат translate: parse-программа источника + эмиссия цели + буферы
/// notices/warnings (для CLI/сима; терять Q3-warnings нельзя).
#[derive(Debug)]
pub struct TranslateOutcome {
    pub from: String,
    pub to: String,
    pub program: Program,
    pub emitted: String,
    /// Parse-stage notices источника (drop-with-notice — не ошибка).
    pub notices: Vec<String>,
    /// Emit-stage warnings цели (Ok-путь; Q3).
    pub emit_warnings: Vec<Diagnostic>,
}

/// Текст → текст: parse(источник) → emit(цель).
pub fn translate_text(
    from: &str,
    to: &str,
    src: &str,
    file: &str,
) -> Result<TranslateOutcome, TranslateError> {
    let mut source = AnyAdapter::for_name(from)?;
    let mut target = AnyAdapter::for_name(to)?;

    let program = source.parse(src, file).map_err(TranslateError::Parse)?;
    let notices = source.take_notices();

    let emitted = target.emit(&program).map_err(TranslateError::Emit)?;
    let emit_warnings = target.take_emit_diagnostics();

    Ok(TranslateOutcome {
        from: source.name().to_string(),
        to: target.name().to_string(),
        program,
        emitted,
        notices,
        emit_warnings,
    })
}

/// Файл → файл (CLI-путь, критерий #9): чтение, конвейер, запись.
pub fn translate_file(
    from: &str,
    to: &str,
    input: &Path,
    output: &Path,
) -> Result<TranslateOutcome, TranslateError> {
    let src = std::fs::read_to_string(input)
        .map_err(|e| TranslateError::Io(format!("input {}: {e}", input.display())))?;
    let file_name = input
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| input.display().to_string());
    let outcome = translate_text(from, to, &src, &file_name)?;
    std::fs::write(output, &outcome.emitted)
        .map_err(|e| TranslateError::Io(format!("output {}: {e}", output.display())))?;
    Ok(outcome)
}

/// Разбор текста конкретным диалектом (verify, e2e).
pub fn parse_with(dialect: &str, src: &str, file: &str) -> Result<Program, TranslateError> {
    let mut adapter = AnyAdapter::for_name(dialect)?;
    let program = adapter.parse(src, file).map_err(TranslateError::Parse)?;
    let _ = adapter.take_notices(); // verify печатает их отдельно при желании; здесь гасим
    Ok(program)
}
