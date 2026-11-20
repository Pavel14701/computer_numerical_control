//! MachineProfile — замороженный интерфейс v0.1 (минимум для MVP; полная схема —
//! этап 8 исходной карты: кинематика, макросы, safety, escape hatch).
//!
//! Итерация 2 (Агент E): к замороженным типам добавлены только
//! `#[derive(Serialize, Deserialize)]` (YAML, kebab-case) и чистые дополнения —
//! `MachineProfile::load_yaml` / `grbl_default` / `linuxcnc_default`,
//! `DialectKind::from_name` / `as_name`. Поля и семантика типов не изменены
//! (freeze v0.1-interface). Дефолтные профили лежат в `profiles/*.yaml` и
//! встроены в крейт через `include_str!` (single source of truth — YAML;
//! `grbl_default()`/`linuxcnc_default()` строятся кодом и сверяются с YAML
//! тестом `tests/profile.rs`).

use std::path::Path;

/// Встроенный дефолтный профиль GRBL 1.1 (`profiles/grbl_default.yaml`).
pub const GRBL_DEFAULT_YAML: &str = include_str!("../profiles/grbl_default.yaml");

/// Встроенный дефолтный профиль LinuxCNC (`profiles/linuxcnc_default.yaml`).
pub const LINUXCNC_DEFAULT_YAML: &str = include_str!("../profiles/linuxcnc_default.yaml");

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
    /// Каноническое имя диалекта (совпадает с YAML-представлением и
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MachineProfile {
    pub name: String,
    pub dialect: DialectKind,
    pub limits: Limits,
    /// R-эмиссия дуг — по флагу профиля с notice (EQUIVALENCE §4: по умолчанию
    /// всегда IJK; R1 всегда через IJK).
    pub emit_r_arcs: bool,
}

impl MachineProfile {
    /// Загрузка профиля из YAML-файла.
    ///
    /// Ошибка — `String` (замороженная сигнатура; отдельный `ProfileError`
    /// не вводится): чтение файла или парсинг/валидация YAML не прошли.
    pub fn load_yaml(path: &Path) -> Result<MachineProfile, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("профиль {}: не удалось прочитать: {e}", path.display()))?;
        let profile: MachineProfile = serde_yaml::from_str(&text)
            .map_err(|e| format!("профиль {}: невалидный YAML: {e}", path.display()))?;
        Ok(profile)
    }

    /// Дефолтный профиль GRBL 1.1 (лимиты COVERAGE §1.2; emit_r_arcs: false —
    /// политика EQUIVALENCE §4). Сверяется с `GRBL_DEFAULT_YAML` тестом.
    pub fn grbl_default() -> Self {
        Self {
            name: "GRBL 1.1 default".to_string(),
            dialect: DialectKind::Grbl11,
            limits: Limits::default(),
            emit_r_arcs: false,
        }
    }

    /// Дефолтный профиль LinuxCNC (лимиты COVERAGE §1.2; emit_r_arcs: false —
    /// политика EQUIVALENCE §4). Сверяется с `LINUXCNC_DEFAULT_YAML` тестом.
    pub fn linuxcnc_default() -> Self {
        Self {
            name: "LinuxCNC default".to_string(),
            dialect: DialectKind::LinuxCnc,
            limits: Limits::default(),
            emit_r_arcs: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!(serde_yaml::to_string(&kind).unwrap().trim(), name);
            let back: DialectKind = serde_yaml::from_str(name).unwrap();
            assert_eq!(back, kind);
        }
        assert_eq!(DialectKind::from_name("grbl"), None);
        assert_eq!(DialectKind::from_name("fanuc"), None);
    }
}
