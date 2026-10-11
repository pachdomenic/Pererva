//! Клієнт офіційного API [alerts.in.ua](https://alerts.in.ua).
//!
//! Використовується легкий IoT-ендпоінт, який повертає один символ:
//! `A` — тривога в усій області, `P` — у частині громад, `N` — немає.
//! Токен можна отримати на <https://devs.alerts.in.ua>.

use super::{AlertSource, AlertStatus, FetchError, http_agent, regions::Region};

const BASE_URL: &str = "https://api.alerts.in.ua/v1/iot/active_air_raid_alerts";

pub struct AlertsInUa {
    agent: ureq::Agent,
    token: String,
}

impl AlertsInUa {
    pub fn new(token: String) -> Self {
        Self {
            agent: http_agent(),
            token: token.trim().to_owned(),
        }
    }
}

impl AlertSource for AlertsInUa {
    fn fetch(&self, region: &Region) -> Result<AlertStatus, FetchError> {
        if self.token.is_empty() {
            return Err(FetchError::MissingToken);
        }
        let url = format!("{BASE_URL}/{}.json", region.uid);
        let body = self
            .agent
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.token))
            .call()?
            .body_mut()
            .read_to_string()?;
        parse(&body)
    }
}

/// Розбирає відповідь IoT-ендпоінта: JSON-рядок `"A"`, `"P"` або `"N"`.
pub fn parse(body: &str) -> Result<AlertStatus, FetchError> {
    let value: String =
        serde_json::from_str(body.trim()).map_err(|e| FetchError::Parse(e.to_string()))?;
    match value.trim() {
        "A" => Ok(AlertStatus::Active),
        "P" => Ok(AlertStatus::Partial),
        "N" => Ok(AlertStatus::Clear),
        other => Err(FetchError::Parse(format!("невідомий статус «{other}»"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_statuses() {
        assert_eq!(parse("\"A\"").unwrap(), AlertStatus::Active);
        assert_eq!(parse("\"P\"\n").unwrap(), AlertStatus::Partial);
        assert_eq!(parse(" \"N\" ").unwrap(), AlertStatus::Clear);
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(parse("\"X\""), Err(FetchError::Parse(_))));
        assert!(matches!(parse("<html>"), Err(FetchError::Parse(_))));
    }

    #[test]
    fn empty_token_is_reported_without_request() {
        let client = AlertsInUa::new("   ".into());
        let region = super::super::regions::by_uid(31).unwrap();
        assert!(matches!(
            client.fetch(region),
            Err(FetchError::MissingToken)
        ));
    }
}
