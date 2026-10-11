//! Отримання стану повітряної тривоги.

pub mod alerts_in_ua;
pub mod monitor;
pub mod regions;
pub mod tracker;
pub mod ubilling;

use crate::config::{AlertConfig, AlertProvider, YellowMode};
use crate::i18n::pick;
use crate::tr;
use regions::{District, Region};
use std::fmt;
use std::time::Duration;

/// Стан тривоги в області.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlertStatus {
    /// Даних ще немає або джерело недоступне.
    #[default]
    Unknown,
    /// Тривоги немає.
    Clear,
    /// Тривога оголошена лише в частині громад області.
    Partial,
    /// Тривога по всій області.
    Active,
}

/// Рівень тривоги за alerts.in.ua. Інші джерела рівнів не розрізняють — для
/// них тривога завжди червона.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreatLevel {
    #[default]
    Red,
    Yellow,
}

/// Стан тривоги разом з її рівнем.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reading {
    pub status: AlertStatus,
    /// Має сенс лише для `Active` і `Partial`.
    pub level: ThreatLevel,
}

impl From<AlertStatus> for Reading {
    fn from(status: AlertStatus) -> Self {
        Self {
            status,
            level: ThreatLevel::Red,
        }
    }
}

impl Reading {
    #[cfg(test)]
    pub fn yellow(status: AlertStatus) -> Self {
        Self {
            status,
            level: ThreatLevel::Yellow,
        }
    }

    /// Рівень тривоги, на яку треба реагувати, з урахуванням налаштувань
    /// часткових і жовтих тривог; `None` — реагувати не треба.
    pub fn alert(self, config: &AlertConfig) -> Option<ThreatLevel> {
        let alert = match self.status {
            AlertStatus::Active => true,
            AlertStatus::Partial => config.partial_is_alert,
            AlertStatus::Clear | AlertStatus::Unknown => false,
        };
        let ignored = self.level == ThreatLevel::Yellow && config.yellow == YellowMode::Ignore;
        (alert && !ignored).then_some(self.level)
    }

    /// Чи є тривога жовтого рівня (повна або часткова).
    pub fn is_yellow(self) -> bool {
        self.level == ThreatLevel::Yellow
            && matches!(self.status, AlertStatus::Active | AlertStatus::Partial)
    }
}

impl fmt::Display for AlertStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AlertStatus::Unknown => pick("Немає даних", "No data"),
            AlertStatus::Clear => pick("Тривоги немає", "No alert"),
            AlertStatus::Partial => pick("Тривога в частині громад", "Alert in some communities"),
            AlertStatus::Active => pick("Повітряна тривога", "Air raid alert"),
        })
    }
}

#[derive(Debug)]
pub enum FetchError {
    MissingToken,
    Unauthorized,
    RateLimited,
    Http(u16),
    Network(String),
    Parse(String),
    RegionNotFound,
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::MissingToken => {
                f.write_str(pick("не вказано токен API", "API token is not set"))
            }
            FetchError::Unauthorized => {
                f.write_str(pick("токен API недійсний", "API token is invalid"))
            }
            FetchError::RateLimited => f.write_str(pick(
                "перевищено ліміт запитів, збільште інтервал",
                "request limit exceeded, increase the interval",
            )),
            FetchError::Http(code) => f.write_str(&tr!(
                "сервер повернув помилку {code}",
                "server returned error {code}"
            )),
            FetchError::Network(e) => {
                f.write_str(&tr!("немає з'єднання: {e}", "no connection: {e}"))
            }
            FetchError::Parse(e) => f.write_str(&tr!(
                "незрозуміла відповідь сервера: {e}",
                "unexpected server response: {e}"
            )),
            FetchError::RegionNotFound => f.write_str(pick(
                "область не знайдено у відповіді",
                "region not found in the response",
            )),
        }
    }
}

impl std::error::Error for FetchError {}

impl From<ureq::Error> for FetchError {
    fn from(err: ureq::Error) -> Self {
        match err {
            ureq::Error::StatusCode(401 | 403) => FetchError::Unauthorized,
            ureq::Error::StatusCode(429) => FetchError::RateLimited,
            ureq::Error::StatusCode(code) => FetchError::Http(code),
            other => FetchError::Network(other.to_string()),
        }
    }
}

/// Місце, для якого запитується стан тривоги: область або район області.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub region: &'static Region,
    /// Район; `None` — уся область.
    pub district: Option<&'static District>,
}

impl Location {
    /// Місце з налаштувань. Район враховується лише для джерел, що його
    /// підтримують, і лише якщо він належить вибраній області.
    pub fn from_config(config: &AlertConfig) -> Option<Self> {
        let region = regions::by_uid(config.region_uid)?;
        let district = match config.provider {
            AlertProvider::AlertsInUa => config
                .district_uid
                .and_then(regions::district_by_uid)
                .filter(|d| d.region_uid == region.uid),
            AlertProvider::Ubilling => None,
        };
        Some(Self { region, district })
    }

    pub fn title(&self) -> String {
        match self.district {
            Some(district) => format!("{}, {}", district.title(), self.region.title()),
            None => self.region.title().to_owned(),
        }
    }
}

/// Джерело даних про тривогу.
pub trait AlertSource: Send {
    fn fetch(&self, location: &Location) -> Result<Reading, FetchError>;
}

pub(crate) fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .user_agent(concat!("shkilni-dzvinky/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Створює джерело відповідно до налаштувань.
pub fn source_for(config: &AlertConfig) -> Box<dyn AlertSource> {
    match config.provider {
        AlertProvider::AlertsInUa => Box::new(alerts_in_ua::AlertsInUa::new(config.token.clone())),
        AlertProvider::Ubilling => Box::new(ubilling::Ubilling::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(provider: AlertProvider, region_uid: u32, district_uid: Option<u32>) -> AlertConfig {
        AlertConfig {
            provider,
            region_uid,
            district_uid,
            ..AlertConfig::default()
        }
    }

    #[test]
    fn reading_respects_partial_and_yellow_settings() {
        let mut config = AlertConfig::default();
        let red = Reading::from(AlertStatus::Active);
        let yellow = Reading::yellow(AlertStatus::Active);
        let partial = Reading::yellow(AlertStatus::Partial);
        assert_eq!(red.alert(&config), Some(ThreatLevel::Red));
        assert_eq!(yellow.alert(&config), Some(ThreatLevel::Yellow));
        assert_eq!(partial.alert(&config), None);
        assert_eq!(Reading::yellow(AlertStatus::Clear).alert(&config), None);

        config.partial_is_alert = true;
        assert_eq!(partial.alert(&config), Some(ThreatLevel::Yellow));

        config.yellow = YellowMode::Ignore;
        assert_eq!(yellow.alert(&config), None);
        assert_eq!(partial.alert(&config), None);
        assert_eq!(red.alert(&config), Some(ThreatLevel::Red));
        assert!(yellow.is_yellow() && !red.is_yellow());
        assert!(!Reading::yellow(AlertStatus::Clear).is_yellow());
    }

    #[test]
    fn location_uses_district_for_alerts_in_ua() {
        let location =
            Location::from_config(&config(AlertProvider::AlertsInUa, 14, Some(78))).unwrap();
        assert_eq!(location.district.map(|d| d.uid), Some(78));
        assert_eq!(location.title(), "Бориспільський район, Київська область");
    }

    #[test]
    fn location_ignores_district_for_ubilling() {
        let location =
            Location::from_config(&config(AlertProvider::Ubilling, 14, Some(78))).unwrap();
        assert_eq!(location.district, None);
        assert_eq!(location.title(), "Київська область");
    }

    #[test]
    fn location_ignores_district_of_other_region() {
        let location =
            Location::from_config(&config(AlertProvider::AlertsInUa, 27, Some(78))).unwrap();
        assert_eq!(location.region.uid, 27);
        assert_eq!(location.district, None);
        assert!(Location::from_config(&config(AlertProvider::AlertsInUa, 1, None)).is_none());
    }
}
