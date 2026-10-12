//! Історія подій: кожен запис журналу зберігається в базі SQLite поруч із
//! файлом налаштувань і переживає перезапуск програми.

use crate::LogKind;
use crate::tr;
use anyhow::{Context, Result};
use chrono::{DateTime, Local, TimeZone};
use rusqlite::{Connection, params, params_from_iter};
use std::path::{Path, PathBuf};
use std::time::Duration;

const FILE_NAME: &str = "history.sqlite3";
/// Версія схеми бази (`PRAGMA user_version`).
const SCHEMA_VERSION: i64 = 1;

/// Подія з бази.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: i64,
    pub at: DateTime<Local>,
    pub kind: LogKind,
    pub text: String,
}

/// Фільтр історії за видом подій.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    /// Тривоги і відбої.
    Alerts,
    Bells,
    Memorial,
    Errors,
    /// Решта: запуск, зміна налаштувань, перевірка звуку тощо.
    Other,
}

impl Filter {
    pub fn from_index(index: i32) -> Self {
        match index {
            1 => Self::Alerts,
            2 => Self::Bells,
            3 => Self::Memorial,
            4 => Self::Errors,
            5 => Self::Other,
            _ => Self::All,
        }
    }

    /// Види подій, що проходять фільтр; порожній список — усі.
    fn kinds(self) -> &'static [LogKind] {
        match self {
            Self::All => &[],
            Self::Alerts => &[LogKind::Alert, LogKind::AllClear],
            Self::Bells => &[LogKind::Bell],
            Self::Memorial => &[LogKind::Memorial],
            Self::Errors => &[LogKind::Error],
            Self::Other => &[LogKind::Info],
        }
    }

    pub fn matches(self, kind: LogKind) -> bool {
        let kinds = self.kinds();
        kinds.is_empty() || kinds.contains(&kind)
    }

    /// Умова `WHERE` для SQL і її параметри.
    fn condition(self) -> (String, Vec<&'static str>) {
        let kinds: Vec<_> = self.kinds().iter().map(|k| kind_name(*k)).collect();
        if kinds.is_empty() {
            return ("1".into(), kinds);
        }
        let marks = vec!["?"; kinds.len()].join(", ");
        (format!("kind IN ({marks})"), kinds)
    }
}

/// Назва виду події в базі. Не змінювати: за нею читаються старі записи.
fn kind_name(kind: LogKind) -> &'static str {
    match kind {
        LogKind::Info => "info",
        LogKind::Bell => "bell",
        LogKind::Alert => "alert",
        LogKind::AllClear => "all-clear",
        LogKind::Error => "error",
        LogKind::Memorial => "memorial",
    }
}

fn kind_from_name(name: &str) -> LogKind {
    match name {
        "bell" => LogKind::Bell,
        "alert" => LogKind::Alert,
        "all-clear" => LogKind::AllClear,
        "error" => LogKind::Error,
        "memorial" => LogKind::Memorial,
        _ => LogKind::Info,
    }
}

pub struct History {
    conn: Connection,
}

impl History {
    /// Шлях до бази: поруч із файлом налаштувань.
    pub fn path_for(config_path: &Path) -> PathBuf {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(FILE_NAME)
    }

    /// Відкриває базу, створюючи файл і таблицю, якщо їх ще немає.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).with_context(|| {
                tr!(
                    "не вдалося створити каталог {}",
                    "could not create directory {}",
                    dir.display()
                )
            })?;
        }
        let conn = Connection::open(path).with_context(|| {
            tr!(
                "не вдалося відкрити базу {}",
                "could not open database {}",
                path.display()
            )
        })?;
        // Журнал WAL: запис не блокує читання і не губиться при збої живлення.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(2))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            anyhow::bail!(tr!(
                "база створена новішою версією програми",
                "the database was created by a newer version of the app"
            ));
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                 id   INTEGER PRIMARY KEY AUTOINCREMENT,
                 at   INTEGER NOT NULL,
                 kind TEXT    NOT NULL,
                 text TEXT    NOT NULL
             );
             CREATE INDEX IF NOT EXISTS events_kind ON events (kind, id);",
        )?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Self { conn })
    }

    /// Додає подію; повертає її номер.
    pub fn add(&self, at: DateTime<Local>, kind: LogKind, text: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO events (at, kind, text) VALUES (?1, ?2, ?3)",
            params![at.timestamp(), kind_name(kind), text],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Сторінка подій від новіших до старіших: до `limit` подій, старших за
    /// подію з номером `before` (або найновіші, якщо `None`).
    pub fn page(&self, filter: Filter, before: Option<i64>, limit: usize) -> Result<Vec<Event>> {
        let (condition, kinds) = filter.condition();
        let sql = format!(
            "SELECT id, at, kind, text FROM events
             WHERE {condition} AND id < ?
             ORDER BY id DESC LIMIT ?"
        );
        let mut values: Vec<rusqlite::types::Value> =
            kinds.into_iter().map(|k| k.to_owned().into()).collect();
        values.push(before.unwrap_or(i64::MAX).into());
        values.push(i64::try_from(limit).unwrap_or(i64::MAX).into());
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), |row| {
            let at: i64 = row.get(1)?;
            let kind: String = row.get(2)?;
            Ok(Event {
                id: row.get(0)?,
                at: Local
                    .timestamp_opt(at, 0)
                    .single()
                    .unwrap_or_else(|| DateTime::UNIX_EPOCH.with_timezone(&Local)),
                kind: kind_from_name(&kind),
                text: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Кількість подій, що проходять фільтр.
    pub fn count(&self, filter: Filter) -> Result<u64> {
        let (condition, kinds) = filter.condition();
        let sql = format!("SELECT COUNT(*) FROM events WHERE {condition}");
        let count: i64 = self
            .conn
            .query_row(&sql, params_from_iter(kinds), |r| r.get(0))?;
        Ok(count.max(0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

    fn at(minutes: i64) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 12, 9, 0, 0).unwrap() + ChronoDuration::minutes(minutes)
    }

    fn sample() -> History {
        let h = History::open_in_memory().unwrap();
        h.add(at(0), LogKind::Info, "Програму запущено").unwrap();
        h.add(at(1), LogKind::Bell, "Дзвінок на 1-й урок").unwrap();
        h.add(at(2), LogKind::Alert, "Повітряна тривога: Київ")
            .unwrap();
        h.add(
            at(32),
            LogKind::AllClear,
            "Відбій тривоги: Київ · тривала 30 хв",
        )
        .unwrap();
        h.add(at(40), LogKind::Error, "API тривоги: таймаут")
            .unwrap();
        h.add(at(50), LogKind::Memorial, "Хвилина мовчання")
            .unwrap();
        h
    }

    #[test]
    fn newest_first() {
        let h = sample();
        let events = h.page(Filter::All, None, 100).unwrap();
        assert_eq!(events.len(), 6);
        assert_eq!(events[0].kind, LogKind::Memorial);
        assert_eq!(events[0].at, at(50));
        assert_eq!(events[5].text, "Програму запущено");
        assert!(events.windows(2).all(|w| w[0].id > w[1].id));
    }

    #[test]
    fn paging() {
        let h = sample();
        let first = h.page(Filter::All, None, 4).unwrap();
        assert_eq!(first.len(), 4);
        let rest = h.page(Filter::All, Some(first[3].id), 4).unwrap();
        assert_eq!(rest.len(), 2);
        assert_eq!(rest[1].kind, LogKind::Info);
        assert!(h.page(Filter::All, Some(rest[1].id), 4).unwrap().is_empty());
    }

    #[test]
    fn filters() {
        let h = sample();
        let alerts = h.page(Filter::Alerts, None, 100).unwrap();
        let kinds: Vec<_> = alerts.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, [LogKind::AllClear, LogKind::Alert]);
        assert_eq!(h.count(Filter::Alerts).unwrap(), 2);
        assert_eq!(h.count(Filter::Bells).unwrap(), 1);
        assert_eq!(h.count(Filter::Other).unwrap(), 1);
        assert_eq!(h.count(Filter::All).unwrap(), 6);
        assert!(Filter::Alerts.matches(LogKind::AllClear));
        assert!(!Filter::Alerts.matches(LogKind::Bell));
        assert!(Filter::All.matches(LogKind::Error));
    }

    #[test]
    fn kind_names_round_trip() {
        for kind in [
            LogKind::Info,
            LogKind::Bell,
            LogKind::Alert,
            LogKind::AllClear,
            LogKind::Error,
            LogKind::Memorial,
        ] {
            assert_eq!(kind_from_name(kind_name(kind)), kind);
        }
        assert_eq!(kind_from_name("щось нове"), LogKind::Info);
    }

    #[test]
    fn survives_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(FILE_NAME);
        {
            let h = History::open(&path).unwrap();
            h.add(at(0), LogKind::Bell, "Дзвінок").unwrap();
        }
        let h = History::open(&path).unwrap();
        let events = h.page(Filter::All, None, 10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].text, "Дзвінок");
        h.add(at(1), LogKind::Bell, "Ще дзвінок").unwrap();
        assert_eq!(h.count(Filter::All).unwrap(), 2);
    }

    #[test]
    fn path_next_to_config() {
        let path = History::path_for(Path::new("/etc/app/config.toml"));
        assert_eq!(path, Path::new("/etc/app").join(FILE_NAME));
        assert_eq!(
            History::path_for(Path::new("config.toml")),
            Path::new("").join(FILE_NAME)
        );
    }
}
