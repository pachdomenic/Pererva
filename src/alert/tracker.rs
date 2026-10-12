//! Відстеження початку і кінця тривоги за послідовністю опитувань.

use super::{AlertStatus, Reading, ThreatLevel};
use chrono::{DateTime, Duration, Local};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    None,
    Started(ThreatLevel),
    /// Тривога триває, але змінився її рівень (жовтий ↔ червоний).
    Changed(ThreatLevel),
    /// Тривога завершилась; рівень — той, що був перед відбоєм.
    Ended(ThreatLevel),
}

/// Скільки триває (або тривала) тривога.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lasting {
    pub duration: Duration,
    /// `false` — тривога вже йшла, коли почалося стеження (запуск програми,
    /// зміна області), і справжня тривалість більша: відомо лише «щонайменше».
    pub exact: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AlertTracker {
    /// Рівень тривоги, що триває.
    active: Option<ThreatLevel>,
    /// Останній достовірний стан (без `Unknown`).
    last_known: Reading,
    started_at: Option<DateTime<Local>>,
    ended_at: Option<DateTime<Local>>,
    /// Чи бачили, як почалася остання тривога (до неї був відомий стан без неї).
    seen_start: bool,
}

impl AlertTracker {
    /// Обробляє черговий стан. `alert` — рівень тривоги, на яку треба
    /// реагувати (див. [`Reading::alert`]). `Unknown` (немає даних) не змінює
    /// стану тривоги, щоб збій мережі не вмикав відбій посеред тривоги.
    pub fn update(
        &mut self,
        reading: Reading,
        alert: Option<ThreatLevel>,
        now: DateTime<Local>,
    ) -> Transition {
        if reading.status == AlertStatus::Unknown {
            return Transition::None;
        }
        let had_data = self.last_known.status != AlertStatus::Unknown;
        self.last_known = reading;
        let previous = std::mem::replace(&mut self.active, alert);
        match (previous, alert) {
            (None, Some(level)) => {
                self.started_at = Some(now);
                self.seen_start = had_data;
                Transition::Started(level)
            }
            (Some(level), None) => {
                self.ended_at = Some(now);
                Transition::Ended(level)
            }
            (Some(old), Some(new)) if old != new => Transition::Changed(new),
            _ => Transition::None,
        }
    }

    /// Скидає стан (наприклад, коли моніторинг вимкнено або змінено область).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn last_known(&self) -> Reading {
        self.last_known
    }

    pub fn started_at(&self) -> Option<DateTime<Local>> {
        self.started_at.filter(|_| self.is_active())
    }

    pub fn ended_at(&self) -> Option<DateTime<Local>> {
        self.ended_at.filter(|_| !self.is_active())
    }

    /// Тривалість поточної тривоги на момент `now` або, якщо її вже немає,
    /// останньої завершеної. `None` — тривоги ще не було.
    pub fn lasting(&self, now: DateTime<Local>) -> Option<Lasting> {
        let start = self.started_at?;
        let end = if self.is_active() {
            now
        } else {
            self.ended_at?
        };
        Some(Lasting {
            // Якщо годинник перевели назад, тривалість не стає від'ємною.
            duration: (end - start).max(Duration::zero()),
            exact: self.seen_start,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AlertConfig, YellowMode};
    use chrono::{Duration, TimeZone};

    fn at(minutes: i64) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 12, 9, 0, 0).unwrap() + Duration::minutes(minutes)
    }

    /// Оновлення трекера з урахуванням налаштувань часткових і жовтих тривог.
    fn feed(t: &mut AlertTracker, reading: Reading, config: &AlertConfig, m: i64) -> Transition {
        t.update(reading, reading.alert(config), at(m))
    }

    const RED: ThreatLevel = ThreatLevel::Red;
    const YELLOW: ThreatLevel = ThreatLevel::Yellow;

    #[test]
    fn start_and_end() {
        let c = AlertConfig::default();
        let mut t = AlertTracker::default();
        let active = Reading::from(AlertStatus::Active);
        let clear = Reading::from(AlertStatus::Clear);
        assert_eq!(feed(&mut t, clear, &c, 0), Transition::None);
        assert_eq!(feed(&mut t, active, &c, 1), Transition::Started(RED));
        assert!(t.is_active());
        assert_eq!(t.started_at(), Some(at(1)));
        assert_eq!(feed(&mut t, active, &c, 2), Transition::None);
        assert_eq!(feed(&mut t, clear, &c, 30), Transition::Ended(RED));
        assert!(!t.is_active());
        assert_eq!(t.started_at(), None);
        assert_eq!(t.ended_at(), Some(at(30)));
        assert_eq!(
            t.lasting(at(45)),
            Some(Lasting {
                duration: Duration::minutes(29),
                exact: true,
            })
        );
    }

    #[test]
    fn lasting_of_current_alert() {
        let c = AlertConfig::default();
        let mut t = AlertTracker::default();
        assert_eq!(t.lasting(at(0)), None);
        feed(&mut t, AlertStatus::Clear.into(), &c, 0);
        feed(&mut t, AlertStatus::Active.into(), &c, 10);
        let lasting = t.lasting(at(40)).unwrap();
        assert_eq!(lasting.duration, Duration::minutes(30));
        assert!(lasting.exact);
        // Зміна рівня не перезапускає відлік.
        feed(&mut t, Reading::yellow(AlertStatus::Active), &c, 20);
        assert_eq!(t.lasting(at(40)).unwrap().duration, Duration::minutes(30));
        // Годинник переведено назад.
        assert_eq!(t.lasting(at(5)).unwrap().duration, Duration::zero());
    }

    #[test]
    fn lasting_is_inexact_when_alert_was_already_on() {
        let c = AlertConfig::default();
        let mut t = AlertTracker::default();
        // Збій мережі до першої відповіді — початку тривоги ще не видно.
        feed(&mut t, AlertStatus::Unknown.into(), &c, 0);
        feed(&mut t, AlertStatus::Active.into(), &c, 1);
        feed(&mut t, AlertStatus::Clear.into(), &c, 31);
        assert_eq!(
            t.lasting(at(31)),
            Some(Lasting {
                duration: Duration::minutes(30),
                exact: false,
            })
        );
        // Наступну тривогу вже видно від початку.
        feed(&mut t, AlertStatus::Active.into(), &c, 40);
        assert!(t.lasting(at(50)).unwrap().exact);
        // Після скидання (зміна області) — знову невідомо.
        t.reset();
        feed(&mut t, AlertStatus::Active.into(), &c, 60);
        assert!(!t.lasting(at(70)).unwrap().exact);
    }

    #[test]
    fn active_at_startup_starts_alert() {
        let c = AlertConfig::default();
        let mut t = AlertTracker::default();
        assert_eq!(
            feed(&mut t, AlertStatus::Active.into(), &c, 0),
            Transition::Started(RED)
        );
    }

    #[test]
    fn unknown_keeps_state() {
        let c = AlertConfig::default();
        let mut t = AlertTracker::default();
        feed(&mut t, AlertStatus::Active.into(), &c, 0);
        assert_eq!(
            feed(&mut t, AlertStatus::Unknown.into(), &c, 1),
            Transition::None
        );
        assert!(t.is_active());
        assert_eq!(t.last_known().status, AlertStatus::Active);
    }

    #[test]
    fn partial_depends_on_setting() {
        let mut c = AlertConfig::default();
        let partial = Reading::from(AlertStatus::Partial);
        let mut t = AlertTracker::default();
        assert_eq!(feed(&mut t, partial, &c, 0), Transition::None);
        assert_eq!(t.last_known().status, AlertStatus::Partial);
        c.partial_is_alert = true;
        assert_eq!(feed(&mut t, partial, &c, 1), Transition::Started(RED));
        c.partial_is_alert = false;
        assert_eq!(feed(&mut t, partial, &c, 2), Transition::Ended(RED));
    }

    #[test]
    fn yellow_alert_escalates_and_ends() {
        let c = AlertConfig::default();
        let mut t = AlertTracker::default();
        let yellow = Reading::yellow(AlertStatus::Active);
        let red = Reading::from(AlertStatus::Active);
        assert_eq!(feed(&mut t, yellow, &c, 0), Transition::Started(YELLOW));
        assert!(t.is_active());
        assert_eq!(feed(&mut t, red, &c, 5), Transition::Changed(RED));
        assert_eq!(t.started_at(), Some(at(0)));
        assert_eq!(feed(&mut t, yellow, &c, 9), Transition::Changed(YELLOW));
        assert_eq!(
            feed(&mut t, AlertStatus::Clear.into(), &c, 20),
            Transition::Ended(YELLOW)
        );
    }

    #[test]
    fn ignored_yellow_is_not_an_alert() {
        let c = AlertConfig {
            yellow: YellowMode::Ignore,
            ..AlertConfig::default()
        };
        let mut t = AlertTracker::default();
        let yellow = Reading::yellow(AlertStatus::Active);
        assert_eq!(feed(&mut t, yellow, &c, 0), Transition::None);
        assert!(!t.is_active());
        assert!(t.last_known().is_yellow());
        // Підвищення до червоного рівня — це вже початок тривоги.
        assert_eq!(
            feed(&mut t, AlertStatus::Active.into(), &c, 3),
            Transition::Started(RED)
        );
        assert_eq!(feed(&mut t, yellow, &c, 6), Transition::Ended(RED));
    }

    #[test]
    fn reset_clears_everything() {
        let c = AlertConfig::default();
        let mut t = AlertTracker::default();
        feed(&mut t, AlertStatus::Active.into(), &c, 0);
        t.reset();
        assert!(!t.is_active());
        assert_eq!(t.last_known().status, AlertStatus::Unknown);
    }
}
