//! Клієнт відкритого агрегатора тривог Ubilling.
//!
//! Ендпоінт <https://ubilling.net.ua/aerialalerts/> не потребує токена і
//! повертає стан усіх областей:
//! `{"states": {"Київська область": {"alertnow": true, ...}, ...}}`.

use super::{AlertSource, AlertStatus, FetchError, http_agent, regions::Region};
use serde::Deserialize;
use std::collections::HashMap;

const URL: &str = "https://ubilling.net.ua/aerialalerts/";

pub struct Ubilling {
    agent: ureq::Agent,
}

impl Ubilling {
    pub fn new() -> Self {
        Self {
            agent: http_agent(),
        }
    }
}

impl AlertSource for Ubilling {
    fn fetch(&self, region: &Region) -> Result<AlertStatus, FetchError> {
        let body = self.agent.get(URL).call()?.body_mut().read_to_string()?;
        parse(&body, region)
    }
}

#[derive(Deserialize)]
struct Response {
    states: HashMap<String, State>,
}

#[derive(Deserialize)]
struct State {
    alertnow: bool,
}

pub fn parse(body: &str, region: &Region) -> Result<AlertStatus, FetchError> {
    let response: Response =
        serde_json::from_str(body).map_err(|e| FetchError::Parse(e.to_string()))?;
    response
        .states
        .iter()
        .find(|(name, _)| region.matches(name))
        .map(|(_, state)| {
            if state.alertnow {
                AlertStatus::Active
            } else {
                AlertStatus::Clear
            }
        })
        .ok_or(FetchError::RegionNotFound)
}

#[cfg(test)]
mod tests {
    use super::super::regions::by_uid;
    use super::*;

    const SAMPLE: &str = r#"{
        "version": 1,
        "date": "2026-10-11 10:00:00",
        "states": {
            "Київська область": {"alertnow": true, "changed": "2026-10-11 09:40:00"},
            "м. Київ": {"alertnow": false, "changed": "2026-10-11 08:00:00"},
            "Львівська область": {"alertnow": false, "changed": "2026-10-10 22:15:00"}
        }
    }"#;

    #[test]
    fn finds_region_state() {
        assert_eq!(
            parse(SAMPLE, by_uid(14).unwrap()).unwrap(),
            AlertStatus::Active
        );
        assert_eq!(
            parse(SAMPLE, by_uid(31).unwrap()).unwrap(),
            AlertStatus::Clear
        );
        assert_eq!(
            parse(SAMPLE, by_uid(27).unwrap()).unwrap(),
            AlertStatus::Clear
        );
    }

    #[test]
    fn missing_region_is_an_error() {
        assert!(matches!(
            parse(SAMPLE, by_uid(18).unwrap()),
            Err(FetchError::RegionNotFound)
        ));
    }

    #[test]
    fn malformed_body_is_an_error() {
        assert!(matches!(
            parse("{}", by_uid(31).unwrap()),
            Err(FetchError::Parse(_))
        ));
    }
}
