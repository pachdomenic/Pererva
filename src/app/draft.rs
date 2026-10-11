//! Чернетка розкладу, яку редагує користувач, і її перевірка.

use super::format::{duration_words, ordinal};
use crate::schedule::{Lesson, Schedule, format_hhmm, parse_hhmm};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftRow {
    pub start: String,
    pub end: String,
}

/// Результат перевірки одного рядка для показу в інтерфейсі.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RowCheck {
    pub note: String,
    pub start_error: bool,
    pub end_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleDraft {
    pub rows: Vec<DraftRow>,
    pub days: [bool; 7],
    pub warning_minutes: u32,
}

impl ScheduleDraft {
    pub fn from_schedule(schedule: &Schedule) -> Self {
        let mut days = [false; 7];
        for &d in &schedule.school_days {
            if (1..=7).contains(&d) {
                days[usize::from(d - 1)] = true;
            }
        }
        Self {
            rows: schedule
                .lessons
                .iter()
                .map(|l| DraftRow {
                    start: format_hhmm(l.start),
                    end: format_hhmm(l.end),
                })
                .collect(),
            days,
            warning_minutes: schedule.warning_minutes,
        }
    }

    /// Перевіряє чернетку: підказки для кожного рядка і розклад або текст помилки.
    pub fn check(&self) -> (Vec<RowCheck>, Result<Schedule, String>) {
        let parsed: Vec<(Option<_>, Option<_>)> = self
            .rows
            .iter()
            .map(|r| (parse_hhmm(&r.start), parse_hhmm(&r.end)))
            .collect();

        let mut checks = Vec::with_capacity(self.rows.len());
        let mut first_error: Option<String> = None;
        let mut report = |n: usize, message: &str| {
            if first_error.is_none() {
                first_error = Some(format!("{} урок: {message}", ordinal(n)));
            }
        };

        for (i, &(start, end)) in parsed.iter().enumerate() {
            let n = i + 1;
            let mut check = RowCheck::default();
            match (start, end) {
                (None, _) | (_, None) => {
                    check.start_error = start.is_none();
                    check.end_error = end.is_none();
                    check.note = "Час у форматі ГГ:ХХ".into();
                    report(n, "неправильний формат часу");
                }
                (Some(s), Some(e)) if e <= s => {
                    check.end_error = true;
                    check.note = "Кінець раніше початку".into();
                    report(n, "кінець раніше початку");
                }
                (Some(s), Some(e)) => {
                    let previous_end = i.checked_sub(1).and_then(|p| parsed[p].1);
                    if previous_end.is_some_and(|pe| s < pe) {
                        check.start_error = true;
                        check.note = "Перетинається з попереднім".into();
                        report(n, "починається до кінця попереднього");
                    } else {
                        check.note = duration_words(e - s);
                        if let Some(next_start) = parsed.get(i + 1).and_then(|n| n.0)
                            && next_start >= e
                        {
                            check.note += &format!(" · перерва {}", duration_words(next_start - e));
                        }
                    }
                }
            }
            checks.push(check);
        }

        let result = match first_error {
            Some(error) => Err(error),
            None => Ok(self.to_schedule(&parsed)),
        };
        (checks, result)
    }

    fn to_schedule(
        &self,
        parsed: &[(Option<chrono::NaiveTime>, Option<chrono::NaiveTime>)],
    ) -> Schedule {
        Schedule {
            lessons: parsed
                .iter()
                .filter_map(|&(s, e)| Some(Lesson::new(s?, e?)))
                .collect(),
            school_days: (1..=7u8)
                .filter(|d| self.days[usize::from(d - 1)])
                .collect(),
            warning_minutes: self.warning_minutes,
        }
    }

    /// Додає урок у тому ж ритмі, що й попередні.
    pub fn add_lesson(&mut self) {
        let mut schedule = match self.check().1 {
            Ok(schedule) => schedule,
            // Якщо чернетка некоректна — продовжуємо від останнього коректного уроку.
            Err(_) => Schedule {
                lessons: self
                    .rows
                    .iter()
                    .filter_map(|r| Some(Lesson::new(parse_hhmm(&r.start)?, parse_hhmm(&r.end)?)))
                    .filter(|l| l.end > l.start)
                    .collect(),
                ..Schedule::default()
            },
        };
        let before = schedule.lessons.len();
        schedule.push_next_lesson();
        match schedule.lessons.get(before) {
            Some(lesson) => self.rows.push(DraftRow {
                start: format_hhmm(lesson.start),
                end: format_hhmm(lesson.end),
            }),
            None => self.rows.push(DraftRow {
                start: String::new(),
                end: String::new(),
            }),
        }
    }

    pub fn remove_lesson(&mut self, index: usize) {
        if index < self.rows.len() {
            self.rows.remove(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(rows: &[(&str, &str)]) -> ScheduleDraft {
        ScheduleDraft {
            rows: rows
                .iter()
                .map(|(s, e)| DraftRow {
                    start: (*s).into(),
                    end: (*e).into(),
                })
                .collect(),
            days: [true, true, true, true, true, false, false],
            warning_minutes: 0,
        }
    }

    #[test]
    fn roundtrip_with_schedule() {
        let schedule = Schedule::default();
        let d = ScheduleDraft::from_schedule(&schedule);
        assert_eq!(
            d.rows[0],
            DraftRow {
                start: "08:30".into(),
                end: "09:15".into()
            }
        );
        assert_eq!(d.check().1, Ok(schedule));
    }

    #[test]
    fn notes_show_duration_and_break() {
        let (checks, result) = draft(&[("08:30", "09:15"), ("09:25", "10:10")]).check();
        assert!(result.is_ok());
        assert_eq!(checks[0].note, "45 хв · перерва 10 хв");
        assert_eq!(checks[1].note, "45 хв");
    }

    #[test]
    fn reports_bad_format() {
        let (checks, result) = draft(&[("08:30", "9.15")]).check();
        assert!(checks[0].end_error && !checks[0].start_error);
        assert_eq!(result.unwrap_err(), "1-й урок: неправильний формат часу");
    }

    #[test]
    fn reports_reversed_and_overlapping() {
        let (checks, result) =
            draft(&[("08:30", "09:15"), ("09:00", "09:40"), ("11:00", "10:00")]).check();
        assert!(checks[1].start_error);
        assert!(checks[2].end_error);
        assert_eq!(
            result.unwrap_err(),
            "2-й урок: починається до кінця попереднього"
        );
    }

    #[test]
    fn days_and_warning_are_kept() {
        let mut d = draft(&[("08:30", "09:15")]);
        d.days = [true, false, true, false, false, true, false];
        d.warning_minutes = 3;
        let schedule = d.check().1.unwrap();
        assert_eq!(schedule.school_days, vec![1, 3, 6]);
        assert_eq!(schedule.warning_minutes, 3);
    }

    #[test]
    fn add_and_remove() {
        let mut d = draft(&[("08:30", "09:15")]);
        d.add_lesson();
        assert_eq!(
            d.rows[1],
            DraftRow {
                start: "09:25".into(),
                end: "10:10".into()
            }
        );
        d.remove_lesson(0);
        assert_eq!(d.rows.len(), 1);
        d.remove_lesson(5);
        assert_eq!(d.rows.len(), 1);

        let mut empty = draft(&[]);
        empty.add_lesson();
        assert_eq!(
            empty.rows[0],
            DraftRow {
                start: "08:30".into(),
                end: "09:15".into()
            }
        );
    }
}
