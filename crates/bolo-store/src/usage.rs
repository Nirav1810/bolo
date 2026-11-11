//! The usage meter: one row per paid cloud request (Deepgram and the other cloud speech
//! engines, Claude polish), in the history database's `usage` table.
//!
//! Rows hold numbers only, never text, so they are kept when history is off or cleared
//! and are never pruned. Costs are estimates at list price, worked out when the request
//! finished.

use rusqlite::params;
use serde::Serialize;

use crate::Result;
use crate::history::History;

/// Schema version 2: the `usage` table.
pub(crate) const SCHEMA_V2: &str = "BEGIN;
     CREATE TABLE IF NOT EXISTS usage (
         id                 INTEGER PRIMARY KEY,
         at_ms              INTEGER NOT NULL,
         provider           TEXT NOT NULL,
         detail             TEXT NOT NULL,
         audio_secs         REAL NOT NULL DEFAULT 0,
         input_tokens       INTEGER NOT NULL DEFAULT 0,
         output_tokens      INTEGER NOT NULL DEFAULT 0,
         cache_read_tokens  INTEGER NOT NULL DEFAULT 0,
         cache_write_tokens INTEGER NOT NULL DEFAULT 0,
         cost_usd           REAL NOT NULL DEFAULT 0
     );
     CREATE INDEX IF NOT EXISTS usage_at ON usage (at_ms);
     PRAGMA user_version = 2;
     COMMIT;";

/// One paid request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageEvent {
    /// Unix time in milliseconds.
    pub at_ms: i64,
    /// Who bills it: `deepgram`, `anthropic`, `groq`, `sarvam`.
    pub provider: String,
    /// The engine or model, e.g. `deepgram-stream:nova-3`, `claude-haiku-5-5`.
    pub detail: String,
    pub audio_secs: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost_usd: f64,
}

/// One provider's totals over a period.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ProviderUsage {
    pub provider: String,
    pub requests: u64,
    pub audio_secs: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost_usd: f64,
}

/// The provider that bills an engine id: `deepgram-stream:nova-3` → `deepgram`.
pub fn provider_of(engine_id: &str) -> &str {
    let name = engine_id.split(':').next().unwrap_or(engine_id);
    name.strip_suffix("-stream").unwrap_or(name)
}

impl History {
    /// Records one paid request, whether or not history is on.
    pub fn record_usage(&self, u: &UsageEvent) -> Result<()> {
        self.conn.execute(
            "INSERT INTO usage (at_ms, provider, detail, audio_secs, input_tokens, output_tokens,
                                cache_read_tokens, cache_write_tokens, cost_usd)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                u.at_ms,
                u.provider,
                u.detail,
                u.audio_secs,
                u.input_tokens as i64,
                u.output_tokens as i64,
                u.cache_read_tokens as i64,
                u.cache_write_tokens as i64,
                u.cost_usd
            ],
        )?;
        Ok(())
    }

    /// Per-provider totals of the requests at or after `since_ms` (0 for all time),
    /// sorted by provider name.
    pub fn usage_since(&self, since_ms: i64) -> Result<Vec<ProviderUsage>> {
        let mut stmt = self.conn.prepare(
            "SELECT provider, COUNT(*), TOTAL(audio_secs), TOTAL(input_tokens), TOTAL(output_tokens),
                    TOTAL(cache_read_tokens), TOTAL(cache_write_tokens), TOTAL(cost_usd)
             FROM usage WHERE at_ms >= ?1 GROUP BY provider ORDER BY provider",
        )?;
        let rows = stmt
            .query_map([since_ms], |r| {
                Ok(ProviderUsage {
                    provider: r.get(0)?,
                    requests: r.get::<_, i64>(1)? as u64,
                    audio_secs: r.get(2)?,
                    input_tokens: r.get::<_, f64>(3)? as u64,
                    output_tokens: r.get::<_, f64>(4)? as u64,
                    cache_read_tokens: r.get::<_, f64>(5)? as u64,
                    cache_write_tokens: r.get::<_, f64>(6)? as u64,
                    cost_usd: r.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// When the oldest usage row was recorded, if there is one.
    pub fn usage_started(&self) -> Result<Option<i64>> {
        Ok(self.conn.query_row("SELECT MIN(at_ms) FROM usage", [], |r| r.get(0))?)
    }

    /// Deletes every usage row (the meter's Reset).
    pub fn clear_usage(&self) -> Result<()> {
        self.conn.execute("DELETE FROM usage", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speech(at_ms: i64, engine: &str, secs: f64, cost: f64) -> UsageEvent {
        UsageEvent {
            at_ms,
            provider: provider_of(engine).into(),
            detail: engine.into(),
            audio_secs: secs,
            cost_usd: cost,
            ..Default::default()
        }
    }

    fn polish(at_ms: i64, input: u64, output: u64, cost: f64) -> UsageEvent {
        UsageEvent {
            at_ms,
            provider: "anthropic".into(),
            detail: "claude-haiku-5-5".into(),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 600,
            cost_usd: cost,
            ..Default::default()
        }
    }

    #[test]
    fn engine_ids_map_to_who_bills_them() {
        assert_eq!(provider_of("deepgram-stream:nova-3"), "deepgram");
        assert_eq!(provider_of("deepgram:nova-3"), "deepgram");
        assert_eq!(provider_of("groq:whisper-large-v3-turbo"), "groq");
        assert_eq!(provider_of("sarvam"), "sarvam");
    }

    #[test]
    fn totals_add_up_per_provider_and_period() {
        let h = History::open_in_memory().unwrap();
        h.record_usage(&speech(1_000, "deepgram-stream:nova-3", 4.0, 0.0006)).unwrap();
        h.record_usage(&speech(5_000, "deepgram:nova-3", 2.5, 0.0002)).unwrap();
        h.record_usage(&polish(6_000, 120, 30, 0.00003)).unwrap();
        h.record_usage(&polish(7_000, 80, 20, 0.00002)).unwrap();

        let all = h.usage_since(0).unwrap();
        assert_eq!(all.iter().map(|p| p.provider.as_str()).collect::<Vec<_>>(), ["anthropic", "deepgram"]);
        let (claude, deepgram) = (&all[0], &all[1]);
        assert_eq!((claude.requests, claude.input_tokens, claude.output_tokens), (2, 200, 50));
        assert_eq!(claude.cache_read_tokens, 1_200);
        assert!((claude.cost_usd - 0.00005).abs() < 1e-12);
        assert_eq!(deepgram.requests, 2);
        assert!((deepgram.audio_secs - 6.5).abs() < 1e-9);

        let recent = h.usage_since(5_000).unwrap();
        assert_eq!(recent[1].requests, 1);
        assert!(h.usage_since(8_000).unwrap().is_empty());
    }

    #[test]
    fn usage_outlives_history_being_off_or_cleared() {
        let mut h = History::open_in_memory().unwrap();
        h.set_enabled(false);
        h.record_usage(&speech(1_000, "deepgram:nova-3", 1.0, 0.0001)).unwrap();
        h.clear().unwrap();
        assert_eq!(h.usage_since(0).unwrap().len(), 1);
        assert_eq!(h.usage_started().unwrap(), Some(1_000));
        h.clear_usage().unwrap();
        assert!(h.usage_since(0).unwrap().is_empty());
        assert_eq!(h.usage_started().unwrap(), None);
    }

    #[test]
    fn a_version_1_database_gains_the_usage_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE dictations (id INTEGER PRIMARY KEY, at_ms INTEGER NOT NULL, app TEXT NOT NULL,
                     language TEXT NOT NULL, engine TEXT NOT NULL, final_text TEXT NOT NULL,
                     polished_text TEXT, swap TEXT, timings TEXT NOT NULL);
                 INSERT INTO dictations (at_ms, app, language, engine, final_text, timings)
                     VALUES (1, 'notepad.exe', 'english', 'local:x', 'kept', 'null');
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        }
        let h = History::open(&path).unwrap();
        assert_eq!(h.latest(1).unwrap()[0].dictation.final_text, "kept");
        h.record_usage(&polish(2, 10, 5, 0.0)).unwrap();
        assert_eq!(h.usage_since(0).unwrap()[0].requests, 1);
    }
}
