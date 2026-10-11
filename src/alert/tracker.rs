//! Відстеження початку і кінця тривоги за послідовністю опитувань.

use super::AlertStatus;
use chrono::{DateTime, Local};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    None,
    Started,
    Ended,
}

#[derive(Debug, Clone, Default)]
pub struct AlertTracker {
    active: bool,
    /// Останній достовірний стан (без `Unknown`).
    last_known: AlertStatus,
    started_at: Option<DateTime<Local>>,
    ended_at: Option<DateTime<Local>>,
}

impl AlertTracker {
    /// Обробляє черговий стан. `Unknown` (немає даних) не змінює стану тривоги,
    /// щоб збій мережі не вмикав відбій посеред тривоги.
    pub fn update(
        &mut self,
        status: AlertStatus,
        partial_is_alert: bool,
        now: DateTime<Local>,
    ) -> Transition {
        if status == AlertStatus::Unknown {
            return Transition::None;
        }
        self.last_known = status;
        let alert = status.is_alert(partial_is_alert);
        match (self.active, alert) {
            (false, true) => {
                self.active = true;
                self.started_at = Some(now);
                Transition::Started
            }
            (true, false) => {
                self.active = false;
                self.ended_at = Some(now);
                Transition::Ended
            }
            _ => Transition::None,
        }
    }

    /// Скидає стан (наприклад, коли моніторинг вимкнено або змінено область).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn last_known(&self) -> AlertStatus {
        self.last_known
    }

    pub fn started_at(&self) -> Option<DateTime<Local>> {
        self.started_at.filter(|_| self.active)
    }

    pub fn ended_at(&self) -> Option<DateTime<Local>> {
        self.ended_at.filter(|_| !self.active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn at(minutes: i64) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 12, 9, 0, 0).unwrap() + Duration::minutes(minutes)
    }

    #[test]
    fn start_and_end() {
        let mut t = AlertTracker::default();
        assert_eq!(t.update(AlertStatus::Clear, false, at(0)), Transition::None);
        assert_eq!(
            t.update(AlertStatus::Active, false, at(1)),
            Transition::Started
        );
        assert!(t.is_active());
        assert_eq!(t.started_at(), Some(at(1)));
        assert_eq!(
            t.update(AlertStatus::Active, false, at(2)),
            Transition::None
        );
        assert_eq!(
            t.update(AlertStatus::Clear, false, at(30)),
            Transition::Ended
        );
        assert!(!t.is_active());
        assert_eq!(t.started_at(), None);
        assert_eq!(t.ended_at(), Some(at(30)));
    }

    #[test]
    fn active_at_startup_starts_alert() {
        let mut t = AlertTracker::default();
        assert_eq!(
            t.update(AlertStatus::Active, false, at(0)),
            Transition::Started
        );
    }

    #[test]
    fn unknown_keeps_state() {
        let mut t = AlertTracker::default();
        t.update(AlertStatus::Active, false, at(0));
        assert_eq!(
            t.update(AlertStatus::Unknown, false, at(1)),
            Transition::None
        );
        assert!(t.is_active());
        assert_eq!(t.last_known(), AlertStatus::Active);
    }

    #[test]
    fn partial_depends_on_setting() {
        let mut t = AlertTracker::default();
        assert_eq!(
            t.update(AlertStatus::Partial, false, at(0)),
            Transition::None
        );
        assert_eq!(t.last_known(), AlertStatus::Partial);
        assert_eq!(
            t.update(AlertStatus::Partial, true, at(1)),
            Transition::Started
        );
        assert_eq!(
            t.update(AlertStatus::Partial, false, at(2)),
            Transition::Ended
        );
    }

    #[test]
    fn reset_clears_everything() {
        let mut t = AlertTracker::default();
        t.update(AlertStatus::Active, false, at(0));
        t.reset();
        assert!(!t.is_active());
        assert_eq!(t.last_known(), AlertStatus::Unknown);
    }
}
