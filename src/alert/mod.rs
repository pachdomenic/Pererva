//! Отримання стану повітряної тривоги.

pub mod alerts_in_ua;
pub mod regions;
pub mod ubilling;

use crate::config::{AlertConfig, AlertProvider};
use regions::Region;
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

/// Джерело даних про тривогу.
pub trait AlertSource: Send {
    fn fetch(&self, region: &Region) -> Result<AlertStatus, FetchError>;
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
