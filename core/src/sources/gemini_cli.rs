//! Gemini CLI: `~/.gemini/tmp/<project>/chats/*.json`, one JSON document per
//! session, rewritten as the session grows, so a changed file is read whole.
//!
//! Gemini-style counts: `input` includes `cached`, and `output` excludes
//! `thoughts` (thinking is billed as output, so it is added back).

use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use crate::db::{Cursor, Store};
use crate::jsonl::stat;
use crate::model::{Tokens, UsageEvent, parse_ts};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Session {
    session_id: String,
    #[serde(default)]
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: String,
    timestamp: Option<String>,
    model: Option<String>,
    tokens: Option<Counts>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Counts {
    input: i64,
    output: i64,
    cached: i64,
    thoughts: i64,
}

/// Reads one session file if it changed. Returns the number of events.
pub fn scan(store: &Store, path: &Path) -> Result<Option<usize>> {
    let key = path.to_string_lossy();
    let (size, mtime) = stat(path)?;
    if store.cursor(&key)?.is_some_and(|c| c.size == size && c.mtime == mtime) {
        return Ok(None);
    }
    let events = parse(&std::fs::read_to_string(path)?, project_of(path));
    store.commit(&key, &Cursor { offset: size, size, mtime, state: None }, &events)?;
    Ok(Some(events.len()))
}

/// The folder above `chats` names the project, unless it is a hash.
fn project_of(path: &Path) -> Option<String> {
    let name = path.parent()?.parent()?.file_name()?.to_string_lossy().into_owned();
    let is_hash = name.len() >= 32 && name.chars().all(|c| c.is_ascii_hexdigit());
    (!is_hash).then_some(name)
}

fn parse(text: &str, project: Option<String>) -> Vec<UsageEvent> {
    let Ok(session) = serde_json::from_str::<Session>(text) else { return Vec::new() };
    session
        .messages
        .into_iter()
        .filter_map(|m| {
            let c = m.tokens?;
            let tokens = Tokens {
                input: (c.input - c.cached).max(0),
                output: c.output + c.thoughts,
                cache_read: c.cached,
                reasoning: c.thoughts,
                ..Tokens::default()
            };
            (tokens.total() > 0).then(|| UsageEvent {
                key: format!("gemini-cli:{}:{}", session.session_id, m.id),
                source: "gemini-cli".into(),
                provider: "google".into(),
                model: m.model.unwrap_or_else(|| "unknown".into()),
                project: project.clone(),
                session: Some(session.session_id.clone()),
                ts: m.timestamp.as_deref().and_then(parse_ts).unwrap_or(0),
                tokens,
                reported_cost: None,
            })
        })
        .filter(|e| e.ts > 0)
        .collect()
}
