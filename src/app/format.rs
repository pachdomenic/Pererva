//! Форматування дат, часу і тривалостей українською.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};

const MONTHS_GENITIVE: [&str; 12] = [
    "січня",
    "лютого",
    "березня",
    "квітня",
    "травня",
    "червня",
    "липня",
    "серпня",
    "вересня",
    "жовтня",
    "листопада",
    "грудня",
];

pub fn weekday_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "понеділок",
        Weekday::Tue => "вівторок",
        Weekday::Wed => "середа",
        Weekday::Thu => "четвер",
        Weekday::Fri => "п'ятниця",
        Weekday::Sat => "субота",
        Weekday::Sun => "неділя",
    }
}

/// Знахідний відмінок: «у понеділок», «у середу».
pub fn weekday_accusative(day: Weekday) -> &'static str {
    match day {
        Weekday::Wed => "середу",
        Weekday::Fri => "п'ятницю",
        Weekday::Sat => "суботу",
        Weekday::Sun => "неділю",
        other => weekday_name(other),
    }
}

/// «понеділок, 12 жовтня».
pub fn long_date(date: NaiveDate) -> String {
    format!(
        "{}, {} {}",
        weekday_name(date.weekday()),
        date.day(),
        MONTHS_GENITIVE[date.month0() as usize]
    )
}

/// Порядковий номер уроку: «3-й».
pub fn ordinal(n: usize) -> String {
    format!("{n}-й")
}

/// Тривалість словами: «1 год 5 хв», «28 хв», «менше хвилини».
pub fn duration_words(duration: Duration) -> String {
    let total = duration.num_seconds().max(0);
    let minutes = (total + 59) / 60;
    if total < 60 {
        return "менше хвилини".into();
    }
    let days = minutes / (24 * 60);
    let hours = minutes % (24 * 60) / 60;
    let mins = minutes % 60;
    match (days, hours, mins) {
        (0, 0, m) => format!("{m} хв"),
        (0, h, 0) => format!("{h} год"),
        (0, h, m) => format!("{h} год {m} хв"),
        (d, 0, _) => format!("{d} дн"),
        (d, h, _) => format!("{d} дн {h} год"),
    }
}

/// Таймер зворотного відліку: «28:12» або «1:05:10».
pub fn countdown(duration: Duration) -> String {
    let total = duration.num_seconds().max(0);
    let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60);
    if h >= 100 {
        format!("{} дн", h / 24)
    } else if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Коли відбудеться подія відносно `now`: «сьогодні о 09:15», «завтра о 08:30».
pub fn relative_day(now: NaiveDateTime, at: NaiveDateTime) -> String {
    let time = at.format("%H:%M");
    let days = (at.date() - now.date()).num_days();
    match days {
        0 => format!("сьогодні о {time}"),
        1 => format!("завтра о {time}"),
        2..=6 => format!("у {} о {time}", weekday_accusative(at.weekday())),
        _ => format!(
            "{} {} о {time}",
            at.day(),
            MONTHS_GENITIVE[at.month0() as usize]
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(d: u32, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, d)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    #[test]
    fn dates() {
        assert_eq!(long_date(dt(12, 0, 0).date()), "понеділок, 12 жовтня");
        assert_eq!(long_date(dt(16, 0, 0).date()), "п'ятниця, 16 жовтня");
    }

    #[test]
    fn durations() {
        assert_eq!(duration_words(Duration::seconds(30)), "менше хвилини");
        assert_eq!(duration_words(Duration::seconds(61)), "2 хв");
        assert_eq!(duration_words(Duration::minutes(28)), "28 хв");
        assert_eq!(duration_words(Duration::minutes(60)), "1 год");
        assert_eq!(duration_words(Duration::minutes(65)), "1 год 5 хв");
        assert_eq!(duration_words(Duration::hours(50)), "2 дн 2 год");
    }

    #[test]
    fn countdowns() {
        assert_eq!(countdown(Duration::seconds(5)), "00:05");
        assert_eq!(countdown(Duration::seconds(28 * 60 + 12)), "28:12");
        assert_eq!(countdown(Duration::seconds(3910)), "1:05:10");
        assert_eq!(countdown(Duration::hours(130)), "5 дн");
        assert_eq!(countdown(Duration::seconds(-3)), "00:00");
    }

    #[test]
    fn relative_days() {
        let now = dt(14, 16, 0); // середа
        assert_eq!(relative_day(now, dt(14, 17, 5)), "сьогодні о 17:05");
        assert_eq!(relative_day(now, dt(15, 8, 30)), "завтра о 08:30");
        assert_eq!(relative_day(now, dt(16, 8, 30)), "у п'ятницю о 08:30");
        assert_eq!(relative_day(now, dt(19, 8, 30)), "у понеділок о 08:30");
        assert_eq!(relative_day(now, dt(26, 8, 30)), "26 жовтня о 08:30");
    }

    #[test]
    fn ordinals() {
        assert_eq!(ordinal(3), "3-й");
    }
}
