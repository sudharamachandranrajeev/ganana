//! The SQLite store. One file, WAL mode, so the scanner can write while the
//! dashboard reads.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use crate::accounts::{Account, Batch, CostRow, UsageRow};
use crate::model::{LimitSnapshot, Plan, Tokens, UsageEvent};

const MIGRATIONS: &[&str] = &[
    r#"
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
"#,
    r#"
-- Plan limit readings. Only changes are kept: a repeated value keeps the
-- time it was first seen.
CREATE TABLE limit_snapshot (
    meter TEXT NOT NULL,
    provider TEXT NOT NULL,
    window_minutes INTEGER NOT NULL,
    resets_at INTEGER NOT NULL,
    used_pct REAL NOT NULL,
    seen_at INTEGER NOT NULL,
    PRIMARY KEY (meter, window_minutes, resets_at, used_pct)
) WITHOUT ROWID;
CREATE INDEX limit_snapshot_seen ON limit_snapshot(seen_at);
-- Subscriptions the user pays for, entered by hand.
CREATE TABLE plan (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    name TEXT NOT NULL,
    monthly_usd REAL NOT NULL,
    renews_day INTEGER NOT NULL
);
CREATE TABLE setting (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- Limit alerts already shown, so a restart does not repeat them.
CREATE TABLE alert_sent (
    meter TEXT NOT NULL,
    window_minutes INTEGER NOT NULL,
    resets_at INTEGER NOT NULL,
    threshold INTEGER NOT NULL,
    PRIMARY KEY (meter, window_minutes, resets_at, threshold)
) WITHOUT ROWID;
-- Read Codex logs again for the limit readings older versions skipped.
-- Events keep their keys, so nothing is counted twice.
DELETE FROM source_cursor WHERE path LIKE '%rollout-%.jsonl';
"#,
    r#"
-- Provider accounts: synced through an API (the key lives in the system
-- keyring, never here) or filled from CSV imports.
CREATE TABLE account (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    provider TEXT NOT NULL,
    name TEXT NOT NULL,
    key_hint TEXT,
    synced_at INTEGER,
    error TEXT,
    credit_total REAL,
    credit_used REAL
);
-- Daily usage per model as the account reports it, by UTC day.
CREATE TABLE account_usage (
    account INTEGER NOT NULL,
    day TEXT NOT NULL,
    model TEXT NOT NULL,
    input INTEGER NOT NULL,
    output INTEGER NOT NULL,
    cache_read INTEGER NOT NULL,
    cache_write INTEGER NOT NULL,
    cache_write_1h INTEGER NOT NULL,
    reasoning INTEGER NOT NULL,
    requests INTEGER,
    PRIMARY KEY (account, day, model)
) WITHOUT ROWID;
-- Daily billed lines, by UTC day.
CREATE TABLE account_cost (
    account INTEGER NOT NULL,
    day TEXT NOT NULL,
    item TEXT NOT NULL,
    model TEXT,
    usd REAL NOT NULL,
    PRIMARY KEY (account, day, item)
) WITHOUT ROWID;
"#,
];

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
            .query_row("SELECT offset, size, mtime, state FROM source_cursor WHERE path = ?1", [path], |r| {
                Ok(Cursor {
                    offset: r.get::<_, i64>(0)? as u64,
                    size: r.get::<_, i64>(1)? as u64,
                    mtime: r.get(2)?,
                    state: r.get(3)?,
                })
            })
            .optional()?)
    }

    /// Saves a batch of events and limit readings with the new read position
    /// in one transaction, so a crash never leaves events counted without the
    /// cursor (or the reverse). Events with an existing key are replaced, not
    /// added, and keep their earliest time.
    pub fn commit(&self, path: &str, cursor: &Cursor, events: &[UsageEvent], limits: &[LimitSnapshot]) -> Result<()> {
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
                    e.key,
                    e.source,
                    e.provider,
                    e.model,
                    e.project,
                    e.session,
                    e.ts,
                    t.input,
                    t.output,
                    t.cache_read,
                    t.cache_write,
                    t.cache_write_1h,
                    t.reasoning,
                    e.reported_cost,
                ])?;
            }
            let mut limit = tx.prepare_cached(
                "INSERT INTO limit_snapshot (meter, provider, window_minutes, resets_at, used_pct, seen_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT DO UPDATE SET seen_at = min(limit_snapshot.seen_at, excluded.seen_at)",
            )?;
            for l in limits {
                limit.execute(params![l.meter, l.provider, l.window_minutes, l.resets_at, l.used_pct, l.seen_at])?;
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
            .query_row("SELECT fetched_at, json FROM price_catalog WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
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

    /// Limit readings first seen at or after `since_ms`, oldest first.
    pub fn limits_since(&self, since_ms: i64) -> Result<Vec<LimitSnapshot>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT meter, provider, window_minutes, resets_at, used_pct, seen_at
             FROM limit_snapshot WHERE seen_at >= ?1 ORDER BY seen_at",
        )?;
        let rows = stmt.query_map([since_ms], |r| {
            Ok(LimitSnapshot {
                meter: r.get(0)?,
                provider: r.get(1)?,
                window_minutes: r.get(2)?,
                resets_at: r.get(3)?,
                used_pct: r.get(4)?,
                seen_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn plans(&self) -> Result<Vec<Plan>> {
        let mut stmt =
            self.conn.prepare_cached("SELECT id, provider, name, monthly_usd, renews_day FROM plan ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(Plan {
                id: r.get(0)?,
                provider: r.get(1)?,
                name: r.get(2)?,
                monthly_usd: r.get(3)?,
                renews_day: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Adds a plan (`id` 0) or updates one; returns its id.
    pub fn save_plan(&self, p: &Plan) -> Result<i64> {
        if p.id == 0 {
            self.conn.execute(
                "INSERT INTO plan (provider, name, monthly_usd, renews_day) VALUES (?1, ?2, ?3, ?4)",
                params![p.provider, p.name, p.monthly_usd, p.renews_day],
            )?;
            return Ok(self.conn.last_insert_rowid());
        }
        self.conn.execute(
            "UPDATE plan SET provider = ?2, name = ?3, monthly_usd = ?4, renews_day = ?5 WHERE id = ?1",
            params![p.id, p.provider, p.name, p.monthly_usd, p.renews_day],
        )?;
        Ok(p.id)
    }

    pub fn delete_plan(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM plan WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO setting (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    /// Records an alert for a window; false if it was already recorded. A
    /// window's reset time drifts a little between readings, so any reset
    /// within ten minutes counts as the same window.
    pub fn claim_alert(&self, meter: &str, window_minutes: i64, resets_at: i64, threshold: i64) -> Result<bool> {
        let seen: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM alert_sent WHERE meter = ?1 AND window_minutes = ?2
                AND abs(resets_at - ?3) < 600 AND threshold = ?4)",
            params![meter, window_minutes, resets_at, threshold],
            |r| r.get(0),
        )?;
        if !seen {
            self.conn.execute(
                "INSERT OR IGNORE INTO alert_sent (meter, window_minutes, resets_at, threshold) VALUES (?1, ?2, ?3, ?4)",
                params![meter, window_minutes, resets_at, threshold],
            )?;
        }
        Ok(!seen)
    }

    pub fn accounts(&self) -> Result<Vec<Account>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, kind, provider, name, key_hint, synced_at, error, credit_total, credit_used
             FROM account ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Account {
                id: r.get(0)?,
                kind: r.get(1)?,
                provider: r.get(2)?,
                name: r.get(3)?,
                key_hint: r.get(4)?,
                synced_at: r.get(5)?,
                error: r.get(6)?,
                credit_total: r.get(7)?,
                credit_used: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn account(&self, id: i64) -> Result<Option<Account>> {
        Ok(self.accounts()?.into_iter().find(|a| a.id == id))
    }

    /// Adds an account; returns its id.
    pub fn add_account(&self, kind: &str, provider: &str, name: &str, key_hint: Option<&str>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO account (kind, provider, name, key_hint) VALUES (?1, ?2, ?3, ?4)",
            params![kind, provider, name, key_hint],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Removes an account and everything read for it.
    pub fn delete_account(&self, id: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM account_usage WHERE account = ?1", [id])?;
        tx.execute("DELETE FROM account_cost WHERE account = ?1", [id])?;
        tx.execute("DELETE FROM account WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    /// Replaces the account's rows for the batch's days with the batch, and
    /// marks the account synced at `now_ms`. Rows for the same day and model
    /// (or item) are added together.
    pub fn save_account_batch(&self, id: i64, batch: &Batch, now_ms: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let range = params![id, batch.from, batch.to];
            tx.execute("DELETE FROM account_usage WHERE account = ?1 AND day BETWEEN ?2 AND ?3", range)?;
            tx.execute("DELETE FROM account_cost WHERE account = ?1 AND day BETWEEN ?2 AND ?3", range)?;
            let mut usage = tx.prepare_cached(
                "INSERT INTO account_usage (account, day, model, input, output, cache_read, cache_write,
                    cache_write_1h, reasoning, requests)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT DO UPDATE SET
                    input = input + excluded.input, output = output + excluded.output,
                    cache_read = cache_read + excluded.cache_read, cache_write = cache_write + excluded.cache_write,
                    cache_write_1h = cache_write_1h + excluded.cache_write_1h,
                    reasoning = reasoning + excluded.reasoning,
                    requests = CASE WHEN requests IS NULL AND excluded.requests IS NULL THEN NULL
                        ELSE coalesce(requests, 0) + coalesce(excluded.requests, 0) END",
            )?;
            for u in &batch.usage {
                let t = &u.tokens;
                usage.execute(params![
                    id,
                    u.day,
                    u.model,
                    t.input,
                    t.output,
                    t.cache_read,
                    t.cache_write,
                    t.cache_write_1h,
                    t.reasoning,
                    u.requests
                ])?;
            }
            let mut cost = tx.prepare_cached(
                "INSERT INTO account_cost (account, day, item, model, usd) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT DO UPDATE SET usd = usd + excluded.usd",
            )?;
            for c in &batch.costs {
                cost.execute(params![id, c.day, c.item, c.model, c.usd])?;
            }
            tx.execute("UPDATE account SET synced_at = ?2, error = NULL WHERE id = ?1", params![id, now_ms])?;
            if let Some((total, used)) = batch.credit {
                tx.execute(
                    "UPDATE account SET credit_total = ?2, credit_used = ?3 WHERE id = ?1",
                    params![id, total, used],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_account_error(&self, id: i64, error: Option<&str>) -> Result<()> {
        self.conn.execute("UPDATE account SET error = ?2 WHERE id = ?1", params![id, error])?;
        Ok(())
    }

    /// Account usage rows on or after `since_day` (YYYY-MM-DD), with the
    /// account id.
    pub fn account_usage(&self, since_day: &str) -> Result<Vec<(i64, UsageRow)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT account, day, model, input, output, cache_read, cache_write, cache_write_1h, reasoning, requests
             FROM account_usage WHERE day >= ?1 ORDER BY day",
        )?;
        let rows = stmt.query_map([since_day], |r| {
            Ok((
                r.get(0)?,
                UsageRow {
                    day: r.get(1)?,
                    model: r.get(2)?,
                    tokens: Tokens {
                        input: r.get(3)?,
                        output: r.get(4)?,
                        cache_read: r.get(5)?,
                        cache_write: r.get(6)?,
                        cache_write_1h: r.get(7)?,
                        reasoning: r.get(8)?,
                    },
                    requests: r.get(9)?,
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Account cost rows on or after `since_day`, with the account id.
    pub fn account_costs(&self, since_day: &str) -> Result<Vec<(i64, CostRow)>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT account, day, item, model, usd FROM account_cost WHERE day >= ?1 ORDER BY day")?;
        let rows = stmt.query_map([since_day], |r| {
            Ok((r.get(0)?, CostRow { day: r.get(1)?, item: r.get(2)?, model: r.get(3)?, usd: r.get(4)? }))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
