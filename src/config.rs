//! Налаштування застосунку, що зберігаються у TOML-файлі.

use crate::alert::regions;
use crate::i18n::{Lang, pick};
use crate::schedule::{Schedule, hhmm};
use crate::tr;
use anyhow::{Context, Result};
use chrono::{NaiveDateTime, NaiveTime};
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
    AlertsInUa,
    /// ubilling.net.ua/aerialalerts — відкритий агрегатор без токена.
    /// Типовий, щоб програма працювала одразу після встановлення.
    #[default]
    Ubilling,
}

impl AlertProvider {
    pub const ALL: [AlertProvider; 2] = [AlertProvider::AlertsInUa, AlertProvider::Ubilling];

    pub fn title(self) -> &'static str {
        match self {
            AlertProvider::AlertsInUa => "alerts.in.ua",
            AlertProvider::Ubilling => pick("Ubilling (без токена)", "Ubilling (no token)"),
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }
}

/// Як реагувати на тривогу жовтого рівня (лише alerts.in.ua).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum YellowMode {
    /// Лише показувати, без сирени і без вимкнення дзвінків.
    Ignore,
    /// Як тривога червоного рівня, з тими самими звуками.
    #[default]
    SameAsRed,
    /// Як тривога, але з окремими звуками початку і відбою.
    Separate,
}

impl YellowMode {
    pub const ALL: [YellowMode; 3] = [
        YellowMode::Ignore,
        YellowMode::SameAsRed,
        YellowMode::Separate,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0)
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
    /// Район області (лише для alerts.in.ua). Без нього — тривога по всій області.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub district_uid: Option<u32>,
    /// Інтервал опитування API в секундах.
    pub poll_seconds: u64,
    /// Не подавати шкільні дзвінки під час тривоги.
    pub mute_bells_during_alert: bool,
    /// Вважати тривогу активною, якщо вона оголошена лише в частині громад області.
    pub partial_is_alert: bool,
    /// Реакція на тривогу жовтого рівня.
    pub yellow: YellowMode,
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
            district_uid: None,
            poll_seconds: 20,
            mute_bells_during_alert: true,
            partial_is_alert: false,
            yellow: YellowMode::default(),
            play_all_clear: true,
        }
    }
}

/// Щоденна загальнонаціональна хвилина мовчання і державний гімн після неї.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemorialConfig {
    pub enabled: bool,
    /// Час початку хвилини мовчання.
    #[serde(with = "hhmm")]
    pub time: NaiveTime,
    /// Лише в навчальні дні з розкладу; інакше — щодня.
    pub school_days_only: bool,
    /// Після хвилини мовчання подавати гімн (потрібен файл `anthem_file`).
    pub anthem: bool,
}

impl Default for MemorialConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            time: NaiveTime::from_hms_opt(9, 0, 0).expect("коректний час"),
            school_days_only: false,
            anthem: true,
        }
    }
}

impl MemorialConfig {
    /// Момент хвилини мовчання в напіввідкритому інтервалі `(from, to]`.
    pub fn due_between(
        &self,
        from: NaiveDateTime,
        to: NaiveDateTime,
        schedule: &Schedule,
    ) -> Option<NaiveDateTime> {
        if !self.enabled || to <= from {
            return None;
        }
        let mut date = from.date();
        while date <= to.date() {
            let at = date.and_time(self.time);
            if at > from && at <= to && (!self.school_days_only || schedule.is_school_day(date)) {
                return Some(at);
            }
            date = date.succ_opt()?;
        }
        None
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
    /// Окремі звуки тривоги жовтого рівня (режим `yellow = "separate"`).
    pub yellow_siren_file: Option<PathBuf>,
    pub yellow_all_clear_file: Option<PathBuf>,
    /// Звукова доріжка хвилини мовчання; без неї — вбудований метроном.
    pub silence_file: Option<PathBuf>,
    /// Звукова доріжка державного гімну.
    pub anthem_file: Option<PathBuf>,
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
            yellow_siren_file: None,
            yellow_all_clear_file: None,
            silence_file: None,
            anthem_file: None,
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

/// Мова інтерфейсу.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    #[default]
    System,
    Uk,
    En,
}

impl Language {
    pub fn index(self) -> i32 {
        match self {
            Language::System => 0,
            Language::Uk => 1,
            Language::En => 2,
        }
    }

    pub fn from_index(index: i32) -> Self {
        match index {
            1 => Language::Uk,
            2 => Language::En,
            _ => Language::System,
        }
    }

    pub fn resolve(self) -> Lang {
        match self {
            Language::System => Lang::from_system(),
            Language::Uk => Lang::Uk,
            Language::En => Lang::En,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: ThemeMode,
    pub language: Language,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schedule: Schedule,
    pub alert: AlertConfig,
    pub memorial: MemorialConfig,
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
        let raw = fs::read_to_string(path).with_context(|| {
            tr!(
                "не вдалося прочитати {}",
                "could not read {}",
                path.display()
            )
        })?;
        let mut config: Config = toml::from_str(&raw).with_context(|| {
            tr!(
                "помилка у файлі налаштувань {}",
                "error in settings file {}",
                path.display()
            )
        })?;
        config.normalize();
        config.schedule.validate().context(pick(
            "некоректний розклад у файлі налаштувань",
            "invalid schedule in settings file",
        ))?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| {
                tr!(
                    "не вдалося створити {}",
                    "could not create {}",
                    dir.display()
                )
            })?;
        }
        let raw = toml::to_string_pretty(self).context(pick(
            "не вдалося серіалізувати налаштування",
            "could not serialize settings",
        ))?;
        // Запис через тимчасовий файл, щоб не зіпсувати конфіг при збої.
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, raw).with_context(|| {
            tr!(
                "не вдалося записати {}",
                "could not write {}",
                tmp.display()
            )
        })?;
        fs::rename(&tmp, path).with_context(|| {
            tr!(
                "не вдалося записати {}",
                "could not write {}",
                path.display()
            )
        })?;
        Ok(())
    }

    /// Приводить значення до допустимих меж.
    pub fn normalize(&mut self) {
        self.sound.volume = self.sound.volume.clamp(0.0, 1.0);
        self.sound.bell_seconds = self.sound.bell_seconds.clamp(1, 60);
        self.sound.siren_seconds = self.sound.siren_seconds.clamp(5, 300);
        self.alert.poll_seconds = self.alert.poll_seconds.clamp(10, 600);
        // Район з іншої області (або невідомий) не застосовується.
        let region_uid = self.alert.region_uid;
        self.alert.district_uid = self.alert.district_uid.filter(|&uid| {
            regions::district_by_uid(uid).is_some_and(|d| d.region_uid == region_uid)
        });
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
        config.alert.provider = AlertProvider::AlertsInUa;
        config.alert.token = "token".into();
        config.alert.region_uid = 14;
        config.alert.district_uid = Some(78);
        config.alert.yellow = YellowMode::Separate;
        config.sound.yellow_siren_file = Some("/tmp/yellow.wav".into());
        config.sound.volume = 0.5;
        config.schedule.warning_minutes = 3;
        config.ui.theme = ThemeMode::Dark;
        config.ui.language = Language::En;
        config.memorial.time = NaiveTime::from_hms_opt(9, 30, 0).unwrap();
        config.memorial.anthem = false;
        config.sound.anthem_file = Some("/tmp/anthem.mp3".into());
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
            provider = "alerts-in-ua"
            region_uid = 27

            [sound]
            volume = 3.0
            "#,
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.alert.provider, AlertProvider::AlertsInUa);
        assert_eq!(config.alert.region_uid, 27);
        assert_eq!(
            config.alert.poll_seconds,
            AlertConfig::default().poll_seconds
        );
        assert_eq!(config.sound.volume, 1.0);
        assert_eq!(config.schedule, Schedule::default());
    }

    #[test]
    fn district_from_other_region_is_dropped() {
        let mut config = Config::default();
        config.alert.region_uid = 14;
        config.alert.district_uid = Some(78);
        config.normalize();
        assert_eq!(config.alert.district_uid, Some(78));

        config.alert.region_uid = 27;
        config.normalize();
        assert_eq!(config.alert.district_uid, None);

        config.alert.district_uid = Some(99_999);
        config.normalize();
        assert_eq!(config.alert.district_uid, None);
    }

    #[test]
    fn yellow_mode_index_roundtrip() {
        for mode in YellowMode::ALL {
            assert_eq!(YellowMode::from_index(mode.index()), mode);
        }
        assert_eq!(YellowMode::from_index(9), YellowMode::SameAsRed);
    }

    #[test]
    fn theme_index_roundtrip() {
        for mode in [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark] {
            assert_eq!(ThemeMode::from_index(mode.index()), mode);
        }
        assert_eq!(ThemeMode::from_index(42), ThemeMode::System);
    }

    #[test]
    fn language_index_roundtrip() {
        for language in [Language::System, Language::Uk, Language::En] {
            assert_eq!(Language::from_index(language.index()), language);
        }
        assert_eq!(Language::from_index(-1), Language::System);
        assert_eq!(Language::En.resolve(), Lang::En);
    }

    #[test]
    fn example_config_is_valid() {
        let config: Config = toml::from_str(include_str!("../config.example.toml")).unwrap();
        assert_eq!(config.schedule.validate(), Ok(()));
        assert_eq!(config.schedule, Schedule::default());
        assert_eq!(config.alert, AlertConfig::default());
        assert_eq!(config.sound, SoundConfig::default());
        assert_eq!(config.memorial, MemorialConfig::default());
    }

    fn at(day: u32, h: u32, m: u32) -> NaiveDateTime {
        // 2026-10-12 — понеділок, 2026-10-17 — субота.
        chrono::NaiveDate::from_ymd_opt(2026, 10, day)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    #[test]
    fn memorial_is_due_once_at_its_time() {
        let memorial = MemorialConfig::default();
        let schedule = Schedule::default();
        assert_eq!(
            memorial.due_between(at(12, 8, 59), at(12, 9, 0), &schedule),
            Some(at(12, 9, 0))
        );
        assert_eq!(
            memorial.due_between(at(12, 9, 0), at(12, 9, 1), &schedule),
            None
        );
        assert_eq!(
            memorial.due_between(at(12, 10, 0), at(13, 8, 0), &schedule),
            None
        );
    }

    #[test]
    fn memorial_days_and_switch() {
        let schedule = Schedule::default();
        let mut memorial = MemorialConfig::default();
        let saturday = (at(17, 8, 59), at(17, 9, 0));
        assert!(
            memorial
                .due_between(saturday.0, saturday.1, &schedule)
                .is_some()
        );

        memorial.school_days_only = true;
        assert!(
            memorial
                .due_between(saturday.0, saturday.1, &schedule)
                .is_none()
        );

        memorial.school_days_only = false;
        memorial.enabled = false;
        assert!(
            memorial
                .due_between(saturday.0, saturday.1, &schedule)
                .is_none()
        );
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
