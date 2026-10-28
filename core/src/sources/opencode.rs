//! opencode: `~/.local/share/opencode/opencode.db`. Assistant rows in
//! `session_message` hold the model and token counts as JSON. Messages are
//! updated while they stream, so the cursor is the newest `time_updated` read.
//!
//! opencode counts: `input` excludes cache reads, and `output` excludes
//! `reasoning` (added back, since reasoning is billed as output).

use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;

use crate::db::{Cursor, Store};
use crate::jsonl::stat;
use crate::model::{Tokens, UsageEvent, project_name};

#[derive(Deserialize)]
struct Data {
    model: Option<ModelRef>,
    #[serde(default)]
    tokens: Counts,
    cost: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelRef {
    id: String,
    #[serde(rename = "providerID")]
    provider_id: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Counts {
    input: i64,
    output: i64,
    reasoning: i64,
    cache: Cache,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Cache {
    read: i64,
    write: i64,
}

pub fn scan(store: &Store, path: &Path) -> Result<Option<usize>> {
    let key = format!("opencode:{}", path.to_string_lossy());
    let prev = store.cursor(&key)?;
    let since = prev.as_ref().map_or(0, |c| c.offset as i64);

    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    conn.busy_timeout(std::time::Duration::from_secs(2))?;
    let mut stmt = conn.prepare(
        "SELECT m.id, m.session_id, m.time_created, m.time_updated, m.data, s.directory
         FROM session_message m LEFT JOIN session_v2 s ON s.id = m.session_id
         WHERE m.type = 'assistant' AND m.time_updated > ?1
         ORDER BY m.time_updated",
    )?;
    let mut newest = since;
    let mut events = Vec::new();
    let mut rows = stmt.query([since])?;
    while let Some(r) = rows.next()? {
        let (id, session, created, updated): (String, String, i64, i64) = (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
        newest = newest.max(updated);
        let Ok(data) = serde_json::from_str::<Data>(&r.get::<_, String>(4)?) else { continue };
        let Some(model) = data.model else { continue };
        let c = data.tokens;
        let tokens = Tokens {
            input: c.input,
            output: c.output + c.reasoning,
            cache_read: c.cache.read,
            cache_write: c.cache.write,
            cache_write_1h: 0,
            reasoning: c.reasoning,
        };
        if tokens.total() == 0 {
            continue;
        }
        let directory: Option<String> = r.get(5)?;
        events.push(UsageEvent {
            key: format!("opencode:{id}"),
            source: "opencode".into(),
            provider: model.provider_id.unwrap_or_else(|| "unknown".into()),
            model: model.id,
            project: directory.as_deref().and_then(project_name),
            session: Some(session),
            ts: created,
            tokens,
            reported_cost: data.cost.filter(|c| *c > 0.0),
        });
    }
    if newest == since && prev.is_some() {
        return Ok(None);
    }
    let (size, mtime) = stat(path)?;
    store.commit(&key, &Cursor { offset: newest as u64, size, mtime, state: None }, &events, &[])?;
    Ok(Some(events.len()))
}
