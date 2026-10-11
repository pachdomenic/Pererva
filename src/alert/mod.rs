//! Отримання стану повітряної тривоги.

pub mod alerts_in_ua;
pub mod monitor;
pub mod regions;
pub mod tracker;
pub mod ubilling;

use crate::config::{AlertConfig, AlertProvider};
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

impl AlertStatus {
    /// Чи вважати стан тривогою з урахуванням налаштування для часткових тривог.
    pub fn is_alert(self, partial_is_alert: bool) -> bool {
        match self {
            AlertStatus::Active => true,
            AlertStatus::Partial => partial_is_alert,
            AlertStatus::Clear | AlertStatus::Unknown => false,
        }
    }
}

impl fmt::Display for AlertStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AlertStatus::Unknown => "Немає даних",
            AlertStatus::Clear => "Тривоги немає",
            AlertStatus::Partial => "Тривога в частині громад",
            AlertStatus::Active => "Повітряна тривога",
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
            FetchError::MissingToken => write!(f, "не вказано токен API"),
            FetchError::Unauthorized => write!(f, "токен API недійсний"),
            FetchError::RateLimited => write!(f, "перевищено ліміт запитів, збільште інтервал"),
            FetchError::Http(code) => write!(f, "сервер повернув помилку {code}"),
            FetchError::Network(e) => write!(f, "немає з'єднання: {e}"),
            FetchError::Parse(e) => write!(f, "незрозуміла відповідь сервера: {e}"),
            FetchError::RegionNotFound => write!(f, "область не знайдено у відповіді"),
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

    /// Ідентифікатор alerts.in.ua: району, якщо його вибрано, інакше області.
    pub fn uid(&self) -> u32 {
        self.district.map_or(self.region.uid, |d| d.uid)
    }

    pub fn title(&self) -> String {
        match self.district {
            Some(district) => format!("{}, {}", district.name, self.region.name),
            None => self.region.name.to_owned(),
        }
    }
}

/// Джерело даних про тривогу.
pub trait AlertSource: Send {
    fn fetch(&self, location: &Location) -> Result<AlertStatus, FetchError>;
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
    fn location_uses_district_for_alerts_in_ua() {
        let location =
            Location::from_config(&config(AlertProvider::AlertsInUa, 14, Some(78))).unwrap();
        assert_eq!(location.uid(), 78);
        assert_eq!(location.title(), "Бориспільський район, Київська область");
    }

    #[test]
    fn location_ignores_district_for_ubilling() {
        let location =
            Location::from_config(&config(AlertProvider::Ubilling, 14, Some(78))).unwrap();
        assert_eq!(location.district, None);
        assert_eq!(location.uid(), 14);
        assert_eq!(location.title(), "Київська область");
    }

    #[test]
    fn location_ignores_district_of_other_region() {
        let location =
            Location::from_config(&config(AlertProvider::AlertsInUa, 27, Some(78))).unwrap();
        assert_eq!(location.uid(), 27);
        assert!(Location::from_config(&config(AlertProvider::AlertsInUa, 1, None)).is_none());
    }
}
