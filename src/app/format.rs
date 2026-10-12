//! Форматування дат, часу і тривалостей мовою інтерфейсу.

use crate::i18n::{Lang, current, pick};
use crate::tr;
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

/// Назва місяця англійською: «October».
fn month_name(date: impl Datelike) -> String {
    match current() {
        Lang::Uk => MONTHS_GENITIVE[date.month0() as usize].to_owned(),
        Lang::En => chrono::Month::try_from(date.month() as u8)
            .map(|m| m.name().to_owned())
            .unwrap_or_default(),
    }
}

/// Назва дня тижня англійською: «Monday».
fn weekday_en(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

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

/// «понеділок, 12 жовтня» / «Monday, October 12».
pub fn long_date(date: NaiveDate) -> String {
    let day = date.day();
    let month = month_name(date);
    match current() {
        Lang::Uk => format!("{}, {day} {month}", weekday_name(date.weekday())),
        Lang::En => format!("{}, {month} {day}", weekday_en(date.weekday())),
    }
}

/// Порядковий номер уроку: «3-й».
pub fn ordinal(n: usize) -> String {
    format!("{n}-й")
}

/// Тривалість словами: «1 год 5 хв», «28 хв», «менше хвилини». Неповна
/// хвилина округлюється вгору — для часу, що лишився до події.
pub fn duration_words(duration: Duration) -> String {
    let total = duration.num_seconds().max(0);
    if total < 60 {
        return pick("менше хвилини", "less than a minute").into();
    }
    minutes_words((total + 59) / 60)
}

/// Тривалість словами для часу, що вже минув: неповна хвилина
/// відкидається, щоб «30 хв 10 с» не ставали «31 хв».
pub fn elapsed_words(duration: Duration) -> String {
    let total = duration.num_seconds().max(0);
    if total < 60 {
        return pick("менше хвилини", "less than a minute").into();
    }
    minutes_words(total / 60)
}

fn minutes_words(minutes: i64) -> String {
    let days = minutes / (24 * 60);
    let hours = minutes % (24 * 60) / 60;
    let mins = minutes % 60;
    match (days, hours, mins) {
        (0, 0, m) => tr!("{m} хв", "{m} min"),
        (0, h, 0) => tr!("{h} год", "{h} h"),
        (0, h, m) => tr!("{h} год {m} хв", "{h} h {m} min"),
        (d, 0, _) => tr!("{d} дн", "{d} d"),
        (d, h, _) => tr!("{d} дн {h} год", "{d} d {h} h"),
    }
}

/// Таймер зворотного відліку: «28:12» або «1:05:10».
pub fn countdown(duration: Duration) -> String {
    let total = duration.num_seconds().max(0);
    let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60);
    if h >= 100 {
        let d = h / 24;
        tr!("{d} дн", "{d} d")
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
    let weekday = at.weekday();
    let (day, month) = (at.day(), month_name(at));
    match days {
        0 => tr!("сьогодні о {time}", "today at {time}"),
        1 => tr!("завтра о {time}", "tomorrow at {time}"),
        2..=6 => match current() {
            Lang::Uk => format!("у {} о {time}", weekday_accusative(weekday)),
            Lang::En => format!("on {} at {time}", weekday_en(weekday)),
        },
        _ => tr!("{day} {month} о {time}", "{month} {day} at {time}"),
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
    fn elapsed() {
        assert_eq!(elapsed_words(Duration::seconds(59)), "менше хвилини");
        assert_eq!(elapsed_words(Duration::seconds(60)), "1 хв");
        assert_eq!(elapsed_words(Duration::seconds(30 * 60 + 10)), "30 хв");
        assert_eq!(elapsed_words(Duration::seconds(3599)), "59 хв");
        assert_eq!(elapsed_words(Duration::minutes(125)), "2 год 5 хв");
        assert_eq!(elapsed_words(Duration::seconds(-5)), "менше хвилини");
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
    fn english() {
        crate::i18n::set(Lang::En);
        assert_eq!(long_date(dt(12, 0, 0).date()), "Monday, October 12");
        assert_eq!(duration_words(Duration::seconds(30)), "less than a minute");
        assert_eq!(duration_words(Duration::minutes(65)), "1 h 5 min");
        assert_eq!(countdown(Duration::hours(130)), "5 d");
        let now = dt(14, 16, 0);
        assert_eq!(relative_day(now, dt(14, 17, 5)), "today at 17:05");
        assert_eq!(relative_day(now, dt(15, 8, 30)), "tomorrow at 08:30");
        assert_eq!(relative_day(now, dt(16, 8, 30)), "on Friday at 08:30");
        assert_eq!(relative_day(now, dt(26, 8, 30)), "October 26 at 08:30");
        crate::i18n::set(Lang::Uk);
    }

    #[test]
    fn ordinals() {
        assert_eq!(ordinal(3), "3-й");
    }
}
