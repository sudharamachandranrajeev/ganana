//! The SQLite store. One file, WAL mode, so the scanner can write while the
//! dashboard reads.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{Tokens, UsageEvent};

const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE usage_event (
    key TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    project TEXT,
    session TEXT,
    ts INTEGER NOT NULL,
    input INTEGER NOT NULL,
    output INTEGER NOT NULL,
    cache_read INTEGER NOT NULL,
    cache_write INTEGER NOT NULL,
    cache_write_1h INTEGER NOT NULL,
    reasoning INTEGER NOT NULL,
    reported_cost REAL
);
CREATE INDEX usage_event_ts ON usage_event(ts);
-- How far each log has been read. `state` holds per-file parser context
-- (for example the current model) so a resumed read stays correct.
CREATE TABLE source_cursor (
    path TEXT PRIMARY KEY,
    offset INTEGER NOT NULL,
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    state TEXT
);
-- Maps a model name with no known price (a router alias, for example) to a
-- priced catalog model, written as "provider/model".
CREATE TABLE model_alias (
    alias TEXT PRIMARY KEY,
    target TEXT NOT NULL
);
CREATE TABLE price_catalog (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    fetched_at INTEGER NOT NULL,
    json TEXT NOT NULL
);
"#];

/// Read position in one log. `size` and `mtime` detect a replaced or
/// rewritten file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cursor {
    pub offset: u64,
    pub size: u64,
    pub mtime: i64,
    pub state: Option<String>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let version: usize = self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", i + 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn cursor(&self, path: &str) -> Result<Option<Cursor>> {
        Ok(self
            .conn
            .query_row(
                "SELECT offset, size, mtime, state FROM source_cursor WHERE path = ?1",
                [path],
                |r| {
                    Ok(Cursor {
                        offset: r.get::<_, i64>(0)? as u64,
                        size: r.get::<_, i64>(1)? as u64,
                        mtime: r.get(2)?,
                        state: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    /// Saves a batch of events and the new read position in one transaction,
    /// so a crash never leaves events counted without the cursor (or the
    /// reverse). Events with an existing key are replaced, not added, and
    /// keep their earliest time.
    pub fn commit(&self, path: &str, cursor: &Cursor, events: &[UsageEvent]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT INTO usage_event (key, source, provider, model, project, session, ts,
                    input, output, cache_read, cache_write, cache_write_1h, reasoning, reported_cost)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT(key) DO UPDATE SET
                    source = excluded.source, provider = excluded.provider, model = excluded.model,
                    project = excluded.project, session = excluded.session,
                    -- A resumed session copies old calls into a new log with
                    -- new times; the call happened at the first one.
                    ts = min(usage_event.ts, excluded.ts),
                    input = excluded.input, output = excluded.output,
                    cache_read = excluded.cache_read, cache_write = excluded.cache_write,
                    cache_write_1h = excluded.cache_write_1h, reasoning = excluded.reasoning,
                    reported_cost = excluded.reported_cost",
            )?;
            for e in events {
                let t = &e.tokens;
                insert.execute(params![
                    e.key, e.source, e.provider, e.model, e.project, e.session, e.ts,
                    t.input, t.output, t.cache_read, t.cache_write, t.cache_write_1h, t.reasoning,
                    e.reported_cost,
                ])?;
            }
            tx.execute(
                "INSERT INTO source_cursor (path, offset, size, mtime, state) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(path) DO UPDATE SET offset = excluded.offset, size = excluded.size,
                    mtime = excluded.mtime, state = excluded.state",
                params![path, cursor.offset as i64, cursor.size as i64, cursor.mtime, cursor.state],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Every event at or after `since_ms`, oldest first.
    pub fn events_since(&self, since_ms: i64) -> Result<Vec<UsageEvent>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT key, source, provider, model, project, session, ts, input, output, cache_read,
                cache_write, cache_write_1h, reasoning, reported_cost
             FROM usage_event WHERE ts >= ?1 ORDER BY ts",
        )?;
        let rows = stmt.query_map([since_ms], |r| {
            Ok(UsageEvent {
                key: r.get(0)?,
                source: r.get(1)?,
                provider: r.get(2)?,
                model: r.get(3)?,
                project: r.get(4)?,
                session: r.get(5)?,
                ts: r.get(6)?,
                tokens: Tokens {
                    input: r.get(7)?,
                    output: r.get(8)?,
                    cache_read: r.get(9)?,
                    cache_write: r.get(10)?,
                    cache_write_1h: r.get(11)?,
                    reasoning: r.get(12)?,
                },
                reported_cost: r.get(13)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn event_count(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT count(*) FROM usage_event", [], |r| r.get(0))?)
    }

    pub fn aliases(&self) -> Result<HashMap<String, String>> {
        let mut stmt = self.conn.prepare_cached("SELECT alias, target FROM model_alias")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Sets or, with `None`, removes a model alias.
    pub fn set_alias(&self, alias: &str, target: Option<&str>) -> Result<()> {
        match target {
            Some(t) => self.conn.execute(
                "INSERT INTO model_alias (alias, target) VALUES (?1, ?2)
                 ON CONFLICT(alias) DO UPDATE SET target = excluded.target",
                [alias, t],
            )?,
            None => self.conn.execute("DELETE FROM model_alias WHERE alias = ?1", [alias])?,
        };
        Ok(())
    }

    /// The last downloaded price catalog and when it was fetched (Unix ms).
    pub fn catalog_json(&self) -> Result<Option<(i64, String)>> {
        Ok(self
            .conn
            .query_row("SELECT fetched_at, json FROM price_catalog WHERE id = 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?)
    }

    pub fn save_catalog_json(&self, fetched_at: i64, json: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO price_catalog (id, fetched_at, json) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET fetched_at = excluded.fetched_at, json = excluded.json",
            params![fetched_at, json],
        )?;
        Ok(())
    }
}
