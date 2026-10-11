//! Модель розкладу дзвінків і обчислення подій.
//!
//! Розклад складається зі списку уроків (час початку і кінця) та набору
//! навчальних днів тижня. З нього обчислюються події дзвінків і поточний
//! стан дня: урок, перерва, до чи після уроків.

use crate::i18n::pick;
use crate::tr;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Серіалізація часу у форматі `ГГ:ХХ`.
pub(crate) mod hhmm {
    use chrono::NaiveTime;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    const FORMAT: &str = "%H:%M";

    pub fn serialize<S: Serializer>(time: &NaiveTime, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&time.format(FORMAT).to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<NaiveTime, D::Error> {
        let raw = String::deserialize(d)?;
        NaiveTime::parse_from_str(raw.trim(), FORMAT).map_err(D::Error::custom)
    }
}

/// Розбирає рядок `ГГ:ХХ` у час.
pub fn parse_hhmm(raw: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(raw.trim(), "%H:%M").ok()
}

/// Форматує час як `ГГ:ХХ`.
pub fn format_hhmm(time: NaiveTime) -> String {
    time.format("%H:%M").to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lesson {
    #[serde(with = "hhmm")]
    pub start: NaiveTime,
    #[serde(with = "hhmm")]
    pub end: NaiveTime,
}

impl Lesson {
    pub fn new(start: NaiveTime, end: NaiveTime) -> Self {
        Self { start, end }
    }

    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BellKind {
    /// Попереджувальний дзвінок за кілька хвилин до уроку.
    Warning,
    /// Дзвінок на урок.
    LessonStart,
    /// Дзвінок з уроку.
    LessonEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BellEvent {
    pub at: NaiveDateTime,
    pub kind: BellKind,
    /// Номер уроку, починаючи з 1.
    pub lesson: usize,
}

/// Стан навчального дня в конкретний момент.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayStatus {
    DayOff,
    BeforeLessons { first_start: NaiveTime },
    Lesson { number: usize, ends_at: NaiveTime },
    Break { next: usize, next_start: NaiveTime },
    AfterLessons,
}

impl fmt::Display for DayStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            DayStatus::DayOff => f.write_str(pick("Вихідний", "Day off")),
            DayStatus::BeforeLessons { .. } => {
                f.write_str(pick("До початку уроків", "Before lessons"))
            }
            DayStatus::Lesson { number, .. } => {
                f.write_str(&tr!("{number}-й урок", "Lesson {number}"))
            }
            DayStatus::Break { .. } => f.write_str(pick("Перерва", "Break")),
            DayStatus::AfterLessons => f.write_str(pick("Уроки закінчились", "Lessons are over")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleError {
    EmptyLesson(usize),
    Overlap(usize),
}

impl fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScheduleError::EmptyLesson(n) => f.write_str(&tr!(
                "{n}-й урок закінчується раніше, ніж починається",
                "lesson {n} ends before it starts"
            )),
            ScheduleError::Overlap(n) => f.write_str(&tr!(
                "{n}-й урок починається до закінчення попереднього",
                "lesson {n} starts before the previous one ends"
            )),
        }
    }
}

impl std::error::Error for ScheduleError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub lessons: Vec<Lesson>,
    /// Навчальні дні: 1 — понеділок, 7 — неділя.
    pub school_days: Vec<u8>,
    /// За скільки хвилин до уроку подавати попереджувальний дзвінок (0 — вимкнено).
    #[serde(default)]
    pub warning_minutes: u32,
}

impl Default for Schedule {
    /// Типовий розклад: уроки по 45 хвилин з 8:30, перерви 10/20/20/10/10/10.
    fn default() -> Self {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).expect("коректний час");
        Self {
            lessons: vec![
                Lesson::new(t(8, 30), t(9, 15)),
                Lesson::new(t(9, 25), t(10, 10)),
                Lesson::new(t(10, 30), t(11, 15)),
                Lesson::new(t(11, 35), t(12, 20)),
                Lesson::new(t(12, 30), t(13, 15)),
                Lesson::new(t(13, 25), t(14, 10)),
                Lesson::new(t(14, 20), t(15, 5)),
            ],
            school_days: vec![1, 2, 3, 4, 5],
            warning_minutes: 0,
        }
    }
}

impl Schedule {
    pub fn validate(&self) -> Result<(), ScheduleError> {
        for (i, lesson) in self.lessons.iter().enumerate() {
            if lesson.end <= lesson.start {
                return Err(ScheduleError::EmptyLesson(i + 1));
            }
            if i > 0 && lesson.start < self.lessons[i - 1].end {
                return Err(ScheduleError::Overlap(i + 1));
            }
        }
        Ok(())
    }

    pub fn is_school_day(&self, date: NaiveDate) -> bool {
        let day = date.weekday().number_from_monday() as u8;
        self.school_days.contains(&day)
    }

    /// Усі дзвінки заданого дня у хронологічному порядку.
    pub fn events_on(&self, date: NaiveDate) -> Vec<BellEvent> {
        if !self.is_school_day(date) {
            return Vec::new();
        }
        let warning = Duration::minutes(i64::from(self.warning_minutes));
        let mut events = Vec::with_capacity(self.lessons.len() * 3);
        for (i, lesson) in self.lessons.iter().enumerate() {
            let number = i + 1;
            let start = date.and_time(lesson.start);
            if self.warning_minutes > 0 {
                let at = start - warning;
                let after_previous = i == 0 || at > date.and_time(self.lessons[i - 1].end);
                if at.date() == date && after_previous {
                    events.push(BellEvent {
                        at,
                        kind: BellKind::Warning,
                        lesson: number,
                    });
                }
            }
            events.push(BellEvent {
                at: start,
                kind: BellKind::LessonStart,
                lesson: number,
            });
            events.push(BellEvent {
                at: date.and_time(lesson.end),
                kind: BellKind::LessonEnd,
                lesson: number,
            });
        }
        events.sort_by_key(|e| e.at);
        events
    }

    /// Дзвінки в напіввідкритому інтервалі `(from, to]`.
    pub fn events_between(&self, from: NaiveDateTime, to: NaiveDateTime) -> Vec<BellEvent> {
        if to <= from {
            return Vec::new();
        }
        let mut result = Vec::new();
        let mut date = from.date();
        while date <= to.date() {
            result.extend(
                self.events_on(date)
                    .into_iter()
                    .filter(|e| e.at > from && e.at <= to),
            );
            date = date.succ_opt().expect("дата в межах діапазону");
        }
        result
    }

    /// Найближчий дзвінок строго після `now` (шукає на 14 днів уперед).
    pub fn next_event(&self, now: NaiveDateTime) -> Option<BellEvent> {
        let mut date = now.date();
        for _ in 0..14 {
            if let Some(event) = self.events_on(date).into_iter().find(|e| e.at > now) {
                return Some(event);
            }
            date = date.succ_opt()?;
        }
        None
    }

    pub fn status_at(&self, now: NaiveDateTime) -> DayStatus {
        if !self.is_school_day(now.date()) || self.lessons.is_empty() {
            return DayStatus::DayOff;
        }
        let time = now.time();
        let first = self.lessons[0];
        if time < first.start {
            return DayStatus::BeforeLessons {
                first_start: first.start,
            };
        }
        for (i, lesson) in self.lessons.iter().enumerate() {
            if time >= lesson.start && time < lesson.end {
                return DayStatus::Lesson {
                    number: i + 1,
                    ends_at: lesson.end,
                };
            }
            if let Some(next) = self.lessons.get(i + 1)
                && time >= lesson.end
                && time < next.start
            {
                return DayStatus::Break {
                    next: i + 2,
                    next_start: next.start,
                };
            }
        }
        DayStatus::AfterLessons
    }

    /// Додає урок після останнього з такою ж тривалістю і перервою 10 хвилин.
    pub fn push_next_lesson(&mut self) {
        let lesson = match self.lessons.last() {
            Some(last) => {
                let start = last.end + Duration::minutes(10);
                Lesson::new(start, start + last.duration())
            }
            None => {
                let start = NaiveTime::from_hms_opt(8, 30, 0).expect("коректний час");
                Lesson::new(start, start + Duration::minutes(45))
            }
        };
        if lesson.start > self.lessons.last().map_or(NaiveTime::MIN, |l| l.end) {
            self.lessons.push(lesson);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    // 2026-10-12 — понеділок, 2026-10-17 — субота.

    #[test]
    fn default_schedule_is_valid() {
        assert_eq!(Schedule::default().validate(), Ok(()));
    }

    #[test]
    fn detects_overlap_and_empty_lessons() {
        let mut s = Schedule::default();
        s.lessons[1].start = t(9, 0);
        assert_eq!(s.validate(), Err(ScheduleError::Overlap(2)));

        let mut s = Schedule::default();
        s.lessons[0].end = s.lessons[0].start;
        assert_eq!(s.validate(), Err(ScheduleError::EmptyLesson(1)));
    }

    #[test]
    fn no_events_on_weekend() {
        let s = Schedule::default();
        assert!(s.events_on(dt(2026, 10, 17, 0, 0).date()).is_empty());
    }

    #[test]
    fn two_bells_per_lesson() {
        let s = Schedule::default();
        let events = s.events_on(dt(2026, 10, 12, 0, 0).date());
        assert_eq!(events.len(), s.lessons.len() * 2);
        assert_eq!(events[0].kind, BellKind::LessonStart);
        assert_eq!(events[0].at, dt(2026, 10, 12, 8, 30));
        assert_eq!(events[1].kind, BellKind::LessonEnd);
    }

    #[test]
    fn warning_bells_fit_in_breaks() {
        let s = Schedule {
            warning_minutes: 5,
            ..Schedule::default()
        };
        let events = s.events_on(dt(2026, 10, 12, 0, 0).date());
        let warnings: Vec<_> = events
            .iter()
            .filter(|e| e.kind == BellKind::Warning)
            .collect();
        assert_eq!(warnings.len(), s.lessons.len());
        assert_eq!(warnings[0].at, dt(2026, 10, 12, 8, 25));

        // Перерва 10 хвилин: попередження збіглося б із дзвінком з уроку.
        let s = Schedule {
            warning_minutes: 10,
            ..Schedule::default()
        };
        let warnings = s
            .events_on(dt(2026, 10, 12, 0, 0).date())
            .into_iter()
            .filter(|e| e.kind == BellKind::Warning)
            .count();
        assert_eq!(warnings, 3);
    }

    #[test]
    fn events_between_is_half_open() {
        let s = Schedule::default();
        let fired = s.events_between(dt(2026, 10, 12, 8, 29), dt(2026, 10, 12, 8, 30));
        assert_eq!(fired.len(), 1);
        let fired = s.events_between(dt(2026, 10, 12, 8, 30), dt(2026, 10, 12, 8, 31));
        assert!(fired.is_empty());
    }

    #[test]
    fn next_event_skips_weekend() {
        let s = Schedule::default();
        let next = s.next_event(dt(2026, 10, 16, 16, 0)).unwrap();
        assert_eq!(next.at, dt(2026, 10, 19, 8, 30));
        assert_eq!(next.kind, BellKind::LessonStart);
        assert_eq!(next.lesson, 1);
    }

    #[test]
    fn status_through_the_day() {
        let s = Schedule::default();
        assert_eq!(
            s.status_at(dt(2026, 10, 12, 7, 0)),
            DayStatus::BeforeLessons {
                first_start: t(8, 30)
            }
        );
        assert_eq!(
            s.status_at(dt(2026, 10, 12, 8, 30)),
            DayStatus::Lesson {
                number: 1,
                ends_at: t(9, 15)
            }
        );
        assert_eq!(
            s.status_at(dt(2026, 10, 12, 9, 15)),
            DayStatus::Break {
                next: 2,
                next_start: t(9, 25)
            }
        );
        assert_eq!(
            s.status_at(dt(2026, 10, 12, 18, 0)),
            DayStatus::AfterLessons
        );
        assert_eq!(s.status_at(dt(2026, 10, 17, 10, 0)), DayStatus::DayOff);
    }

    #[test]
    fn push_next_lesson_keeps_rhythm() {
        let mut s = Schedule::default();
        let count = s.lessons.len();
        s.push_next_lesson();
        assert_eq!(s.lessons.len(), count + 1);
        assert_eq!(*s.lessons.last().unwrap(), Lesson::new(t(15, 15), t(16, 0)));
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn hhmm_roundtrip() {
        assert_eq!(parse_hhmm(" 08:05 "), Some(t(8, 5)));
        assert_eq!(parse_hhmm("25:00"), None);
        assert_eq!(format_hhmm(t(8, 5)), "08:05");
    }
}
