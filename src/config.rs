//! Налаштування застосунку, що зберігаються у TOML-файлі.

use crate::schedule::Schedule;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const APP_DIR: &str = "shkilni-dzvinky";
const FILE_NAME: &str = "config.toml";

/// Джерело даних про повітряну тривогу.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlertProvider {
    /// api.alerts.in.ua — офіційний API, потрібен токен.
    #[default]
    AlertsInUa,
    /// ubilling.net.ua/aerialalerts — відкритий агрегатор без токена.
    Ubilling,
}

impl AlertProvider {
    pub const ALL: [AlertProvider; 2] = [AlertProvider::AlertsInUa, AlertProvider::Ubilling];

    pub fn title(self) -> &'static str {
        match self {
            AlertProvider::AlertsInUa => "alerts.in.ua",
            AlertProvider::Ubilling => "Ubilling (без токена)",
        }
    }

    pub fn needs_token(self) -> bool {
        matches!(self, AlertProvider::AlertsInUa)
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertConfig {
    pub enabled: bool,
    pub provider: AlertProvider,
    /// Токен доступу до API (для alerts.in.ua).
    pub token: String,
    /// Ідентифікатор області за класифікацією alerts.in.ua (31 — м. Київ).
    pub region_uid: u32,
    /// Інтервал опитування API в секундах.
    pub poll_seconds: u64,
    /// Не подавати шкільні дзвінки під час тривоги.
    pub mute_bells_during_alert: bool,
    /// Вважати тривогу активною, якщо вона оголошена лише в частині громад області.
    pub partial_is_alert: bool,
    /// Подавати сигнал відбою після завершення тривоги.
    pub play_all_clear: bool,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: AlertProvider::default(),
            token: String::new(),
            region_uid: 31,
            poll_seconds: 20,
            mute_bells_during_alert: true,
            partial_is_alert: false,
            play_all_clear: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundConfig {
    /// Гучність від 0.0 до 1.0.
    pub volume: f32,
    /// Тривалість шкільного дзвінка в секундах.
    pub bell_seconds: u32,
    /// Тривалість сирени повітряної тривоги в секундах.
    pub siren_seconds: u32,
    /// Власні аудіофайли (WAV, MP3, FLAC, OGG) замість синтезованих звуків.
    pub bell_file: Option<PathBuf>,
    pub siren_file: Option<PathBuf>,
    pub all_clear_file: Option<PathBuf>,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            volume: 0.8,
            bell_seconds: 5,
            siren_seconds: 30,
            bell_file: None,
            siren_file: None,
            all_clear_file: None,
        }
    }
}

/// Колірна тема інтерфейсу.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn index(self) -> i32 {
        match self {
            ThemeMode::System => 0,
            ThemeMode::Light => 1,
            ThemeMode::Dark => 2,
        }
    }

    pub fn from_index(index: i32) -> Self {
        match index {
            1 => ThemeMode::Light,
            2 => ThemeMode::Dark,
            _ => ThemeMode::System,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: ThemeMode,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schedule: Schedule,
    pub alert: AlertConfig,
    pub sound: SoundConfig,
    pub ui: UiConfig,
}

impl Config {
    /// Шлях до файлу налаштувань у системному каталозі конфігурації.
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_DIR)
            .join(FILE_NAME)
    }

    /// Завантажує налаштування; якщо файлу ще немає — повертає типові.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)
            .with_context(|| format!("не вдалося прочитати {}", path.display()))?;
        let mut config: Config = toml::from_str(&raw)
            .with_context(|| format!("помилка у файлі налаштувань {}", path.display()))?;
        config.normalize();
        config
            .schedule
            .validate()
            .context("некоректний розклад у файлі налаштувань")?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .with_context(|| format!("не вдалося створити {}", dir.display()))?;
        }
        let raw = toml::to_string_pretty(self).context("не вдалося серіалізувати налаштування")?;
        // Запис через тимчасовий файл, щоб не зіпсувати конфіг при збої.
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, raw).with_context(|| format!("не вдалося записати {}", tmp.display()))?;
        fs::rename(&tmp, path)
            .with_context(|| format!("не вдалося записати {}", path.display()))?;
        Ok(())
    }

    /// Приводить значення до допустимих меж.
    pub fn normalize(&mut self) {
        self.sound.volume = self.sound.volume.clamp(0.0, 1.0);
        self.sound.bell_seconds = self.sound.bell_seconds.clamp(1, 60);
        self.sound.siren_seconds = self.sound.siren_seconds.clamp(5, 300);
        self.alert.poll_seconds = self.alert.poll_seconds.clamp(10, 600);
        self.schedule.school_days.retain(|d| (1..=7).contains(d));
        self.schedule.school_days.sort_unstable();
        self.schedule.school_days.dedup();
        self.schedule.warning_minutes = self.schedule.warning_minutes.min(30);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(&dir.path().join("nope.toml")).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(FILE_NAME);
        let mut config = Config::default();
        config.alert.provider = AlertProvider::Ubilling;
        config.alert.region_uid = 14;
        config.sound.volume = 0.5;
        config.schedule.warning_minutes = 3;
        config.ui.theme = ThemeMode::Dark;
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
    }

    #[test]
    fn partial_file_is_completed_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        fs::write(
            &path,
            r#"
            [alert]
            provider = "ubilling"
            region_uid = 27

            [sound]
            volume = 3.0
            "#,
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.alert.provider, AlertProvider::Ubilling);
        assert_eq!(config.alert.region_uid, 27);
        assert_eq!(
            config.alert.poll_seconds,
            AlertConfig::default().poll_seconds
        );
        assert_eq!(config.sound.volume, 1.0);
        assert_eq!(config.schedule, Schedule::default());
    }

    #[test]
    fn theme_index_roundtrip() {
        for mode in [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark] {
            assert_eq!(ThemeMode::from_index(mode.index()), mode);
        }
        assert_eq!(ThemeMode::from_index(42), ThemeMode::System);
    }

    #[test]
    fn invalid_schedule_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        fs::write(
            &path,
            r#"
            [schedule]
            school_days = [1, 2, 3, 4, 5]
            lessons = [
                { start = "09:00", end = "09:45" },
                { start = "09:30", end = "10:15" },
            ]
            "#,
        )
        .unwrap();
        assert!(Config::load(&path).is_err());
    }
}
