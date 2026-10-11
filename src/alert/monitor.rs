//! Фонове опитування джерела тривоги.

use super::{AlertSource, AlertStatus, Location, Reading, source_for};
use crate::config::AlertConfig;
use crate::i18n::pick;
use chrono::{DateTime, Local};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

/// Результат чергового опитування.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertUpdate {
    pub enabled: bool,
    pub reading: Reading,
    /// Назва області або «район, область».
    pub region: String,
    pub error: Option<String>,
    pub checked_at: DateTime<Local>,
}

enum Command {
    Configure(AlertConfig),
    Refresh,
}

/// Потік, що періодично опитує API і повідомляє про стан тривоги.
///
/// Потік завершується, коли монітор знищено.
pub struct AlertMonitor {
    tx: Sender<Command>,
}

type SourceFactory = dyn Fn(&AlertConfig) -> Box<dyn AlertSource> + Send;

impl AlertMonitor {
    pub fn spawn(config: AlertConfig, on_update: impl Fn(AlertUpdate) + Send + 'static) -> Self {
        Self::spawn_with(config, Box::new(source_for), on_update)
    }

    fn spawn_with(
        mut config: AlertConfig,
        factory: Box<SourceFactory>,
        on_update: impl Fn(AlertUpdate) + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("alert-monitor".into())
            .spawn(move || {
                let mut source = factory(&config);
                loop {
                    on_update(poll(&config, source.as_ref()));
                    let interval = Duration::from_secs(config.poll_seconds.max(1));
                    match rx.recv_timeout(interval) {
                        Ok(Command::Configure(new)) => {
                            config = new;
                            source = factory(&config);
                        }
                        Ok(Command::Refresh) | Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("не вдалося запустити потік опитування тривоги");
        Self { tx }
    }

    /// Застосовує нові налаштування і одразу опитує джерело.
    pub fn configure(&self, config: AlertConfig) {
        let _ = self.tx.send(Command::Configure(config));
    }

    /// Позачергове опитування.
    pub fn refresh(&self) {
        let _ = self.tx.send(Command::Refresh);
    }
}

fn poll(config: &AlertConfig, source: &dyn AlertSource) -> AlertUpdate {
    let location = Location::from_config(config);
    let mut update = AlertUpdate {
        enabled: config.enabled,
        reading: AlertStatus::Unknown.into(),
        region: location.map_or_else(
            || pick("Невідома область", "Unknown region").to_owned(),
            |l| l.title(),
        ),
        error: None,
        checked_at: Local::now(),
    };
    if !config.enabled {
        return update;
    }
    let Some(location) = location else {
        update.error = Some(pick("область не вибрано", "no region selected").into());
        return update;
    };
    match source.fetch(&location) {
        Ok(reading) => update.reading = reading,
        Err(err) => update.error = Some(err.to_string()),
    }
    update
}

#[cfg(test)]
mod tests {
    use super::super::FetchError;
    use super::*;
    use std::sync::mpsc::Receiver;

    struct Fixed(AlertStatus);

    impl AlertSource for Fixed {
        fn fetch(&self, _: &Location) -> Result<Reading, FetchError> {
            Ok(self.0.into())
        }
    }

    struct Failing;

    impl AlertSource for Failing {
        fn fetch(&self, _: &Location) -> Result<Reading, FetchError> {
            Err(FetchError::RateLimited)
        }
    }

    fn spawn(config: AlertConfig) -> (AlertMonitor, Receiver<AlertUpdate>) {
        let (tx, rx) = mpsc::channel();
        let factory = Box::new(|c: &AlertConfig| -> Box<dyn AlertSource> {
            if c.token == "fail" {
                Box::new(Failing)
            } else {
                Box::new(Fixed(AlertStatus::Active))
            }
        });
        let monitor = AlertMonitor::spawn_with(config, factory, move |u| {
            let _ = tx.send(u);
        });
        (monitor, rx)
    }

    fn next(rx: &Receiver<AlertUpdate>) -> AlertUpdate {
        rx.recv_timeout(Duration::from_secs(5)).expect("оновлення")
    }

    fn config() -> AlertConfig {
        AlertConfig {
            poll_seconds: 600,
            ..AlertConfig::default()
        }
    }

    #[test]
    fn reports_status_immediately() {
        let (_monitor, rx) = spawn(config());
        let update = next(&rx);
        assert_eq!(update.reading.status, AlertStatus::Active);
        assert_eq!(update.region, "м. Київ");
        assert!(update.error.is_none());
    }

    #[test]
    fn reconfigure_and_refresh() {
        let (monitor, rx) = spawn(config());
        next(&rx);

        monitor.configure(AlertConfig {
            token: "fail".into(),
            ..config()
        });
        let update = next(&rx);
        assert_eq!(update.reading.status, AlertStatus::Unknown);
        assert!(update.error.unwrap().contains("ліміт"));

        monitor.configure(AlertConfig {
            enabled: false,
            ..config()
        });
        let update = next(&rx);
        assert!(!update.enabled);
        assert_eq!(update.reading.status, AlertStatus::Unknown);

        monitor.configure(config());
        next(&rx);
        monitor.refresh();
        assert_eq!(next(&rx).reading.status, AlertStatus::Active);
    }
}
