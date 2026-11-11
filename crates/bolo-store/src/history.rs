//! Dictation history: text only (never audio), in SQLite at
//! `%LOCALAPPDATA%\Bolo\history.sqlite`.
//!
//! The app prunes rows older than the retention period at startup and daily. Turning
//! history off ([`History::set_enabled`]) stops writes and keeps the existing rows;
//! [`History::clear`] deletes them and vacuums the file.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{Result, io_err};

/// The schema this build creates, stored in SQLite's `user_version`.
const SCHEMA_VERSION: i64 = 1;

const MS_PER_DAY: i64 = 86_400_000;

/// One dictation as the app records it.
#[derive(Debug, Clone, PartialEq)]
pub struct Dictation {
    /// Unix time in milliseconds.
    pub at_ms: i64,
    /// The target app, e.g. `notepad.exe`.
    pub app: String,
    /// `english`, `hinglish` or `auto` (the engine's own multilingual mode decided).
    pub language: String,
    /// The engine that produced the text, after fallback.
    pub engine: String,
    /// The instant text that was inserted.
    pub final_text: String,
    pub polished_text: Option<String>,
    /// What happened to the polish swap: `swapped`, or the reason it was skipped.
    pub swap: Option<String>,
    /// Latency breakdown as JSON (engine timings plus the app's own).
    pub timings: serde_json::Value,
}

impl Dictation {
    /// A dictation at the current time.
    pub fn now(app: &str, language: &str, engine: &str, final_text: &str) -> Self {
        Self {
            at_ms: now_ms(),
            app: app.into(),
            language: language.into(),
            engine: engine.into(),
            final_text: final_text.into(),
            polished_text: None,
            swap: None,
            timings: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryRow {
    pub id: i64,
    pub dictation: Dictation,
}

pub struct History {
    conn: Connection,
    enabled: bool,
}

impl std::fmt::Debug for History {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("History").field("path", &self.conn.path()).field("enabled", &self.enabled).finish()
    }
}

impl History {
    /// `%LOCALAPPDATA%\Bolo\history.sqlite`.
    pub fn default_path() -> Result<PathBuf> {
        Ok(crate::data_local_dir()?.join("history.sqlite"))
    }

    /// Opens (or creates) the database at `path`, with recording on.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
        }
        Self::init(Connection::open(path)?)
    }

    /// A throwaway database (tests).
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < SCHEMA_VERSION {
            conn.execute_batch(
                "BEGIN;
                 CREATE TABLE IF NOT EXISTS dictations (
                     id            INTEGER PRIMARY KEY,
                     at_ms         INTEGER NOT NULL,
                     app           TEXT NOT NULL,
                     language      TEXT NOT NULL,
                     engine        TEXT NOT NULL,
                     final_text    TEXT NOT NULL,
                     polished_text TEXT,
                     swap          TEXT,
                     timings       TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS dictations_at ON dictations (at_ms);
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        Ok(Self { conn, enabled: true })
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Off: inserts and updates are skipped. Existing rows stay.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Records `d`. Its row id, or `None` while history is off.
    pub fn insert(&self, d: &Dictation) -> Result<Option<i64>> {
        if !self.enabled {
            return Ok(None);
        }
        self.conn.execute(
            "INSERT INTO dictations (at_ms, app, language, engine, final_text, polished_text, swap, timings)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![d.at_ms, d.app, d.language, d.engine, d.final_text, d.polished_text, d.swap, d.timings.to_string()],
        )?;
        Ok(Some(self.conn.last_insert_rowid()))
    }

    /// Adds the polished text and the swap outcome to row `id` (polish arrives after the
    /// insert). Skipped while history is off.
    pub fn record_polish(&self, id: i64, polished: Option<&str>, swap: &str) -> Result<()> {
        if self.enabled {
            self.conn.execute(
                "UPDATE dictations SET polished_text = ?2, swap = ?3 WHERE id = ?1",
                params![id, polished, swap],
            )?;
        }
        Ok(())
    }

    pub fn get(&self, id: i64) -> Result<Option<HistoryRow>> {
        Ok(self.conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], row).optional()?)
    }

    /// The newest `limit` rows, newest first.
    pub fn latest(&self, limit: usize) -> Result<Vec<HistoryRow>> {
        self.rows(&format!("{SELECT} ORDER BY at_ms DESC, id DESC LIMIT ?1"), params![limit as i64])
    }

    /// Rows whose final or polished text contains `query` (case-insensitive for ASCII),
    /// newest first. An empty query is [`History::latest`].
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<HistoryRow>> {
        if query.is_empty() {
            return self.latest(limit);
        }
        let pattern = format!("%{}%", query.replace('\\', r"\\").replace('%', r"\%").replace('_', r"\_"));
        self.rows(
            &format!(
                "{SELECT} WHERE final_text LIKE ?1 ESCAPE '\\' OR polished_text LIKE ?1 ESCAPE '\\'
                 ORDER BY at_ms DESC, id DESC LIMIT ?2"
            ),
            params![pattern, limit as i64],
        )
    }

    /// Deletes rows more than `retention_days` older than `now_ms`. The number deleted.
    pub fn prune(&self, retention_days: u32, now_ms: i64) -> Result<usize> {
        let cutoff = now_ms - i64::from(retention_days) * MS_PER_DAY;
        Ok(self.conn.execute("DELETE FROM dictations WHERE at_ms < ?1", [cutoff])?)
    }

    /// Deletes row `id`. Whether there was one.
    pub fn delete(&self, id: i64) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM dictations WHERE id = ?1", [id])? > 0)
    }

    /// Deletes every row and vacuums, so the text is gone from the file too.
    pub fn clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM dictations", [])?;
        self.conn.execute_batch("VACUUM")?;
        Ok(())
    }

    pub fn len(&self) -> Result<usize> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM dictations", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }

    fn rows(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<HistoryRow>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params, row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

const SELECT: &str =
    "SELECT id, at_ms, app, language, engine, final_text, polished_text, swap, timings FROM dictations";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryRow> {
    let timings: String = r.get(8)?;
    Ok(HistoryRow {
        id: r.get(0)?,
        dictation: Dictation {
            at_ms: r.get(1)?,
            app: r.get(2)?,
            language: r.get(3)?,
            engine: r.get(4)?,
            final_text: r.get(5)?,
            polished_text: r.get(6)?,
            swap: r.get(7)?,
            timings: serde_json::from_str(&timings).unwrap_or(serde_json::Value::Null),
        },
    })
}

/// The current Unix time in milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn at(at_ms: i64, text: &str) -> Dictation {
        Dictation {
            at_ms,
            timings: json!({ "total_ms": 120.5 }),
            ..Dictation::now("notepad.exe", "english", "local:nemotron-stream", text)
        }
    }

    #[test]
    fn insert_get_and_latest_newest_first() {
        let h = History::open_in_memory().unwrap();
        let a = h.insert(&at(1_000, "first")).unwrap().unwrap();
        let b = h.insert(&at(3_000, "third")).unwrap().unwrap();
        h.insert(&at(2_000, "second")).unwrap();
        assert_eq!(h.get(a).unwrap().unwrap().dictation, at(1_000, "first"));
        let texts: Vec<_> = h.latest(2).unwrap().into_iter().map(|r| r.dictation.final_text).collect();
        assert_eq!(texts, ["third", "second"]);
        assert_eq!(h.latest(2).unwrap()[0].id, b);
        assert_eq!(h.len().unwrap(), 3);
    }

    #[test]
    fn polish_and_swap_outcome_are_recorded_on_the_row() {
        let h = History::open_in_memory().unwrap();
        let id = h.insert(&at(1_000, "um hello")).unwrap().unwrap();
        h.record_polish(id, Some("Hello."), "swapped").unwrap();
        let d = h.get(id).unwrap().unwrap().dictation;
        assert_eq!((d.polished_text.as_deref(), d.swap.as_deref()), (Some("Hello."), Some("swapped")));
        assert_eq!(d.timings, json!({ "total_ms": 120.5 }));
    }

    #[test]
    fn search_matches_substrings_of_final_or_polished_text() {
        let h = History::open_in_memory().unwrap();
        h.insert(&at(1_000, "Send the Report to Nirav")).unwrap();
        let id = h.insert(&at(2_000, "kal milte hain")).unwrap().unwrap();
        h.record_polish(id, Some("Kal milte hain, report ready."), "skipped: touched").unwrap();
        h.insert(&at(3_000, "100% done_now")).unwrap();
        let found = |q: &str| h.search(q, 10).unwrap().into_iter().map(|r| r.dictation.at_ms).collect::<Vec<_>>();
        assert_eq!(found("report"), [2_000, 1_000]);
        assert_eq!(found("%"), [3_000]);
        assert_eq!(found("_"), [3_000]);
        assert_eq!(found("e_n"), [3_000]);
        assert_eq!(found("enon"), Vec::<i64>::new());
        assert_eq!(found("").len(), 3);
    }

    #[test]
    fn delete_removes_one_row() {
        let h = History::open_in_memory().unwrap();
        let a = h.insert(&at(1_000, "keep")).unwrap().unwrap();
        let b = h.insert(&at(2_000, "drop")).unwrap().unwrap();
        assert!(h.delete(b).unwrap());
        assert!(!h.delete(b).unwrap());
        assert_eq!(h.latest(5).unwrap().iter().map(|r| r.id).collect::<Vec<_>>(), [a]);
    }

    #[test]
    fn prune_drops_rows_older_than_the_retention() {
        let h = History::open_in_memory().unwrap();
        let now = 100 * MS_PER_DAY;
        h.insert(&at(now - 31 * MS_PER_DAY, "old")).unwrap();
        h.insert(&at(now - 29 * MS_PER_DAY, "recent")).unwrap();
        h.insert(&at(now, "today")).unwrap();
        assert_eq!(h.prune(30, now).unwrap(), 1);
        assert_eq!(h.len().unwrap(), 2);
        assert_eq!(h.prune(0, now).unwrap(), 1);
        assert_eq!(h.latest(5).unwrap()[0].dictation.final_text, "today");
    }

    #[test]
    fn clear_empties_the_file_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Bolo").join("history.sqlite");
        let h = History::open(&path).unwrap();
        for i in 0..50 {
            h.insert(&at(i, &"secret words ".repeat(20))).unwrap();
        }
        h.clear().unwrap();
        assert!(h.is_empty().unwrap());
        drop(h);
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.windows(12).any(|w| w == b"secret words"), "text left in the file after clear");
        assert!(History::open(&path).unwrap().is_empty().unwrap());
    }

    #[test]
    fn turning_history_off_stops_writes_and_keeps_rows() {
        let mut h = History::open_in_memory().unwrap();
        let id = h.insert(&at(1_000, "kept")).unwrap().unwrap();
        h.set_enabled(false);
        assert_eq!(h.insert(&at(2_000, "dropped")).unwrap(), None);
        h.record_polish(id, Some("Kept."), "swapped").unwrap();
        assert_eq!(h.len().unwrap(), 1);
        assert_eq!(h.get(id).unwrap().unwrap().dictation.polished_text, None);
        h.set_enabled(true);
        assert!(h.insert(&at(3_000, "back")).unwrap().is_some());
    }

    #[test]
    fn reopening_keeps_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");
        History::open(&path).unwrap().insert(&at(5, "persisted")).unwrap();
        assert_eq!(History::open(&path).unwrap().latest(1).unwrap()[0].dictation.final_text, "persisted");
    }
}
