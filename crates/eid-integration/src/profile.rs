//! MachineProfile — замороженный интерфейс v0.1 (минимум для MVP; полная схема —
//! этап 8 исходной карты: кинематика, макросы, safety, escape hatch).
//!
//! Итерация 2 (Агент E): к замороженным типам добавлены только
//! `#[derive(Serialize, Deserialize)]` (kebab-case) и чистые дополнения —
//! `MachineProfile::load` / `grbl_default` / `linuxcnc_default`,
//! `DialectKind::from_name` / `as_name`. Поля и семантика типов не изменены
//! (freeze v0.1-interface).
//!
//! Формат профиля — TOML (миграция с YAML, итерация 3): причина — R-1
//! (IMPLEMENTATION-PLAN §6) — serde_yaml в maintenance mode и тянет
//! unsafe-libyaml транзитивно в CLI-бинарь; hand-parse — цель v1.0.
//! Дефолтные профили лежат в `profiles/*.toml` и встроены в крейт через
//! `include_str!` (single source of truth — TOML; `grbl_default()`/
//! `linuxcnc_default()` строятся кодом и сверяются с TOML тестом
//! `tests/profile.rs`).

use std::path::Path;

/// Встроенный дефолтный профиль GRBL 1.1 (`profiles/grbl_default.toml`).
pub const GRBL_DEFAULT_PROFILE: &str = include_str!("../profiles/grbl_default.toml");

/// Встроенный дефолтный профиль LinuxCNC (`profiles/linuxcnc_default.toml`).
pub const LINUXCNC_DEFAULT_PROFILE: &str = include_str!("../profiles/linuxcnc_default.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DialectKind {
    #[serde(rename = "grbl-1.1")]
    Grbl11,
    GrblHal,
    #[serde(rename = "fluidnc")]
    FluidNc,
    #[serde(rename = "linuxcnc")]
    LinuxCnc,
}

impl DialectKind {
    /// Каноническое имя диалекта (совпадает с TOML-представлением и
    /// `Adapter::dialect_name` для двух MVP-диалектов: "grbl-1.1", "linuxcnc").
    pub fn as_name(self) -> &'static str {
        match self {
            DialectKind::Grbl11 => "grbl-1.1",
            DialectKind::GrblHal => "grbl-hal",
            DialectKind::FluidNc => "fluidnc",
            DialectKind::LinuxCnc => "linuxcnc",
        }
    }

    /// Разбор канонического имени (обратен [`DialectKind::as_name`]).
    pub fn from_name(name: &str) -> Option<DialectKind> {
        match name {
            "grbl-1.1" => Some(DialectKind::Grbl11),
            "grbl-hal" => Some(DialectKind::GrblHal),
            "fluidnc" => Some(DialectKind::FluidNc),
            "linuxcnc" => Some(DialectKind::LinuxCnc),
            _ => None,
        }
    }
}

/// Пороги E_LIMIT_EXCEEDED — дефолты COVERAGE §1.2; профиль переопределяет.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Limits {
    pub max_ops_per_program: usize,
    pub max_unroll_iterations: usize,
    pub max_source_lines: usize,
    pub max_word_chars: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_ops_per_program: 200_000,
            max_unroll_iterations: 10_000,
            max_source_lines: 1_000_000,
            max_word_chars: 32,
        }
    }
}

/// Рабочий диапазон одной оси (станочные лимиты хода; structural-проверка
/// mock-сима, итерация 4). Единицы — единицы программы (mm/in не различаются
/// на этом уровне; профиль задаёт их для своего станка).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AxisRange {
    pub min: f64,
    pub max: f64,
}

/// Пер-осевые лимиты хода станка. Опционально (дефолт — None: проверка
/// отключена); это НЕ retract_clearance (ruling SPEC: константа, не параметр
/// профиля — детерминизм) и не допуски эквивалентности (EQUIVALENCE §4).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AxisLimits {
    pub x: AxisRange,
    pub y: AxisRange,
    pub z: AxisRange,
}

impl AxisLimits {
    /// Внутри диапазона по всем трём осям?
    pub fn contains(&self, x: f64, y: f64, z: f64) -> bool {
        let in_range = |r: &AxisRange, v: f64| r.min <= v && v <= r.max;
        in_range(&self.x, x) && in_range(&self.y, y) && in_range(&self.z, z)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MachineProfile {
    pub name: String,
    pub dialect: DialectKind,
    pub limits: Limits,
    /// R-эмиссия дуг — по флагу профиля с notice (EQUIVALENCE §4: по умолчанию
    /// всегда IJK; R1 всегда через IJK).
    pub emit_r_arcs: bool,
    /// Пер-осевые лимиты хода станка (mock-sim, итерация 4); None = не заданы.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis_limits: Option<AxisLimits>,
}

impl MachineProfile {
    /// Загрузка профиля из TOML-файла (формат профиля — TOML, причина R-1:
    /// IMPLEMENTATION-PLAN §6 — serde_yaml в maintenance mode + unsafe-libyaml
    /// транзитивно; hand-parse — цель v1.0).
    ///
    /// Ошибка — `String` (замороженная сигнатура; отдельный `ProfileError`
    /// не вводится): чтение файла или парсинг/валидация TOML не прошли.
    pub fn load(path: &Path) -> Result<MachineProfile, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("профиль {}: не удалось прочитать: {e}", path.display()))?;
        let profile: MachineProfile = toml::from_str(&text)
            .map_err(|e| format!("профиль {}: невалидный TOML: {e}", path.display()))?;
        Ok(profile)
    }

    /// Дефолтный профиль GRBL 1.1 (лимиты COVERAGE §1.2; emit_r_arcs: false —
    /// политика EQUIVALENCE §4). Сверяется с `GRBL_DEFAULT_PROFILE` тестом.
    pub fn grbl_default() -> Self {
        Self {
            name: "GRBL 1.1 default".to_string(),
            dialect: DialectKind::Grbl11,
            limits: Limits::default(),
            emit_r_arcs: false,
            axis_limits: None,
        }
    }

    /// Дефолтный профиль LinuxCNC (лимиты COVERAGE §1.2; emit_r_arcs: false —
    /// политика EQUIVALENCE §4). Сверяется с `LINUXCNC_DEFAULT_PROFILE` тестом.
    pub fn linuxcnc_default() -> Self {
        Self {
            name: "LinuxCNC default".to_string(),
            dialect: DialectKind::LinuxCnc,
            limits: Limits::default(),
            emit_r_arcs: false,
            axis_limits: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Обёртка: TOML-документ верхнего уровня обязан быть таблицей.
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Wrap {
        d: DialectKind,
    }

    #[test]
    fn dialect_names_agree_with_serde() {
        let all = [
            (DialectKind::Grbl11, "grbl-1.1"),
            (DialectKind::GrblHal, "grbl-hal"),
            (DialectKind::FluidNc, "fluidnc"),
            (DialectKind::LinuxCnc, "linuxcnc"),
        ];
        for (kind, name) in all {
            assert_eq!(kind.as_name(), name);
            assert_eq!(DialectKind::from_name(name), Some(kind));
            let ser = toml::to_string(&Wrap { d: kind }).unwrap();
            assert_eq!(ser.trim(), format!("d = \"{name}\""));
            let back: Wrap = toml::from_str(&format!("d = \"{name}\"")).unwrap();
            assert_eq!(back.d, kind);
        }
        assert_eq!(DialectKind::from_name("grbl"), None);
        assert_eq!(DialectKind::from_name("fanuc"), None);
    }
}
