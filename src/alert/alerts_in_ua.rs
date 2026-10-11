//! Клієнт офіційного API [alerts.in.ua](https://alerts.in.ua).
//!
//! Використовується список активних тривог `/v1/alerts/active.json`: лише в
//! ньому є рівень тривоги (червоний чи жовтий). Стан області або району
//! визначається за записами тривог: тривога на всю область (район) — `Active`,
//! лише в частині громад — `Partial`. Токен можна отримати на
//! <https://devs.alerts.in.ua>.

use super::regions::normalize;
use super::{AlertSource, AlertStatus, FetchError, Location, Reading, ThreatLevel, http_agent};
use serde::{Deserialize, Deserializer};
use std::sync::Mutex;

const URL: &str = "https://api.alerts.in.ua/v1/alerts/active.json";

pub struct AlertsInUa {
    agent: ureq::Agent,
    token: String,
    /// Остання відповідь і її `Last-Modified`: якщо дані не змінилися, сервер
    /// відповідає 304 без тіла.
    cache: Mutex<Option<(String, Vec<Alert>)>>,
}

impl AlertsInUa {
    pub fn new(token: String) -> Self {
        Self {
            agent: http_agent(),
            token: token.trim().to_owned(),
            cache: Mutex::new(None),
        }
    }

    fn active_alerts(&self) -> Result<Vec<Alert>, FetchError> {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let mut request = self
            .agent
            .get(URL)
            .header("Authorization", &format!("Bearer {}", self.token));
        if let Some((modified, _)) = cache.as_ref() {
            request = request.header("If-Modified-Since", modified);
        }
        let mut response = request.call()?;
        if response.status() == 304
            && let Some((_, alerts)) = cache.as_ref()
        {
            return Ok(alerts.clone());
        }
        let modified = response
            .headers()
            .get("Last-Modified")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let alerts = parse(&response.body_mut().read_to_string()?)?;
        *cache = modified.map(|m| (m, alerts.clone()));
        Ok(alerts)
    }
}

impl AlertSource for AlertsInUa {
    fn fetch(&self, location: &Location) -> Result<Reading, FetchError> {
        if self.token.is_empty() {
            return Err(FetchError::MissingToken);
        }
        Ok(evaluate(&self.active_alerts()?, location))
    }
}

#[derive(Deserialize)]
struct Response {
    alerts: Vec<Alert>,
}

/// Запис активної тривоги (лише потрібні поля).
#[derive(Debug, Clone, Deserialize)]
pub struct Alert {
    #[serde(deserialize_with = "uid")]
    location_uid: Option<u32>,
    #[serde(default, deserialize_with = "uid")]
    location_oblast_uid: Option<u32>,
    #[serde(default)]
    location_raion: Option<String>,
    #[serde(default)]
    alert_type: String,
    #[serde(default)]
    alert_level: Option<String>,
}

/// Ідентифікатор може прийти рядком (`"16"`) або числом.
fn uid<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(u32),
        Text(String),
    }
    Ok(match Option::<Raw>::deserialize(d)? {
        Some(Raw::Number(n)) => Some(n),
        Some(Raw::Text(s)) => s.trim().parse().ok(),
        None => None,
    })
}

impl Alert {
    fn level(&self) -> ThreatLevel {
        match self.alert_level.as_deref() {
            Some("yellow") => ThreatLevel::Yellow,
            _ => ThreatLevel::Red,
        }
    }
}

pub fn parse(body: &str) -> Result<Vec<Alert>, FetchError> {
    serde_json::from_str::<Response>(body)
        .map(|r| r.alerts)
        .map_err(|e| FetchError::Parse(e.to_string()))
}

/// Стан тривоги для області або району за списком активних тривог.
pub fn evaluate(alerts: &[Alert], location: &Location) -> Reading {
    let region = location.region.uid;
    let district = location.district;
    let mut whole = Vec::new();
    let mut partial = Vec::new();
    for alert in alerts.iter().filter(|a| a.alert_type == "air_raid") {
        let uid = alert.location_uid;
        if uid == Some(region) || district.is_some_and(|d| uid == Some(d.uid)) {
            whole.push(alert);
            continue;
        }
        if alert.location_oblast_uid != Some(region) {
            continue;
        }
        // Громади району: у записі є назва району, до якого вони належать.
        let in_district = match district {
            None => true,
            Some(d) => alert
                .location_raion
                .as_deref()
                .is_some_and(|r| normalize(r) == normalize(d.name)),
        };
        if in_district {
            partial.push(alert);
        }
    }
    let (status, matched) = if !whole.is_empty() {
        (AlertStatus::Active, whole)
    } else if !partial.is_empty() {
        (AlertStatus::Partial, partial)
    } else {
        return AlertStatus::Clear.into();
    };
    // Жовтий рівень, лише якщо всі тривоги, що до нього призвели, жовтого рівня.
    let level = if matched.iter().all(|a| a.level() == ThreatLevel::Yellow) {
        ThreatLevel::Yellow
    } else {
        ThreatLevel::Red
    };
    Reading { status, level }
}

#[cfg(test)]
mod tests {
    use super::super::regions::{by_uid, district_by_uid};
    use super::*;

    fn oblast(uid: u32) -> Location {
        Location {
            region: by_uid(uid).unwrap(),
            district: None,
        }
    }

    fn raion(uid: u32) -> Location {
        let district = district_by_uid(uid).unwrap();
        Location {
            region: by_uid(district.region_uid).unwrap(),
            district: Some(district),
        }
    }

    const SAMPLE: &str = r#"{"alerts": [
        {"id": 1, "location_title": "Луганська область", "location_type": "oblast",
         "alert_type": "air_raid", "location_uid": "16", "location_oblast": "Луганська область",
         "location_oblast_uid": "16", "location_raion": null, "alert_level": "red"},
        {"id": 2, "location_title": "Бориспільський район", "location_type": "raion",
         "alert_type": "air_raid", "location_uid": "78", "location_oblast": "Київська область",
         "location_oblast_uid": 14, "alert_level": "yellow",
         "threats": [{"threat_type": "drones", "level": "yellow"}]},
        {"id": 3, "location_title": "Шосткинська територіальна громада", "location_type": "hromada",
         "alert_type": "air_raid", "location_uid": "1234", "location_oblast": "Сумська область",
         "location_oblast_uid": "20", "location_raion": "Шосткинський район"},
        {"id": 4, "location_title": "Херсонська область", "location_type": "oblast",
         "alert_type": "artillery_shelling", "location_uid": "23", "location_oblast_uid": "23"}
    ]}"#;

    fn sample() -> Vec<Alert> {
        parse(SAMPLE).unwrap()
    }

    #[test]
    fn whole_region_alert() {
        let reading = evaluate(&sample(), &oblast(16));
        assert_eq!(reading, Reading::from(AlertStatus::Active));
        // Район області з тривогою на всю область — теж під тривогою.
        assert_eq!(evaluate(&sample(), &raion(85)).status, AlertStatus::Active);
    }

    #[test]
    fn district_alert_is_partial_for_region_and_full_for_district() {
        assert_eq!(
            evaluate(&sample(), &oblast(14)),
            Reading::yellow(AlertStatus::Partial)
        );
        assert_eq!(
            evaluate(&sample(), &raion(78)),
            Reading::yellow(AlertStatus::Active)
        );
        // Сусідній район тієї ж області — без тривоги.
        assert_eq!(evaluate(&sample(), &raion(79)).status, AlertStatus::Clear);
    }

    #[test]
    fn hromada_alert_is_partial_for_its_district_only() {
        assert_eq!(
            evaluate(&sample(), &raion(115)),
            Reading::from(AlertStatus::Partial)
        );
        assert_eq!(evaluate(&sample(), &raion(114)).status, AlertStatus::Clear);
        assert_eq!(
            evaluate(&sample(), &oblast(20)).status,
            AlertStatus::Partial
        );
    }

    #[test]
    fn other_alert_types_and_regions_are_ignored() {
        assert_eq!(evaluate(&sample(), &oblast(23)).status, AlertStatus::Clear);
        assert_eq!(evaluate(&sample(), &oblast(31)).status, AlertStatus::Clear);
        assert_eq!(evaluate(&[], &oblast(16)).status, AlertStatus::Clear);
    }

    #[test]
    fn red_wins_over_yellow() {
        let body = r#"{"alerts": [
            {"alert_type": "air_raid", "location_uid": "78", "location_oblast_uid": "14", "alert_level": "yellow"},
            {"alert_type": "air_raid", "location_uid": "79", "location_oblast_uid": "14", "alert_level": "red"}
        ]}"#;
        assert_eq!(
            evaluate(&parse(body).unwrap(), &oblast(14)),
            Reading::from(AlertStatus::Partial)
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(parse("\"A\""), Err(FetchError::Parse(_))));
        assert!(matches!(parse("<html>"), Err(FetchError::Parse(_))));
    }

    #[test]
    fn empty_token_is_reported_without_request() {
        let client = AlertsInUa::new("   ".into());
        assert!(matches!(
            client.fetch(&oblast(31)),
            Err(FetchError::MissingToken)
        ));
    }
}
