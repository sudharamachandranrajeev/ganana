//! Claude Code: `~/.claude/projects/**/*.jsonl`, one line per transcript
//! entry. Assistant entries carry `message.usage`. One API response can span
//! several lines with the same message id and request id, so those two form
//! the key.

use std::path::Path;

use serde::Deserialize;

use super::LineParser;
use crate::model::{Tokens, UsageEvent, parse_ts, project_name};

#[derive(Deserialize)]
struct Line {
    timestamp: Option<String>,
    cwd: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Usage {
    input_tokens: i64,
    output_tokens: i64,
    cache_creation_input_tokens: i64,
    cache_read_input_tokens: i64,
    cache_creation: Option<CacheCreation>,
    output_tokens_details: Option<OutputDetails>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CacheCreation {
    ephemeral_5m_input_tokens: i64,
    ephemeral_1h_input_tokens: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct OutputDetails {
    thinking_tokens: i64,
}

pub struct Parser;

impl LineParser for Parser {
    fn new(_: &Path, _: Option<&str>) -> Self {
        Self
    }

    fn line(&mut self, line: &str, out: &mut Vec<UsageEvent>) {
        // Most lines are user turns, tool output and metadata. Skip them
        // before paying for a JSON parse.
        if !line.contains("\"usage\"") {
            return;
        }
        let Ok(l) = serde_json::from_str::<Line>(line) else { return };
        let Some(msg) = l.message else { return };
        let (Some(usage), Some(model), Some(id)) = (msg.usage, msg.model, msg.id) else { return };
        if model == "<synthetic>" {
            return;
        }
        let Some(ts) = l.timestamp.as_deref().and_then(parse_ts) else { return };

        let (cache_write, cache_write_1h) = match &usage.cache_creation {
            Some(c) => (c.ephemeral_5m_input_tokens, c.ephemeral_1h_input_tokens),
            None => (usage.cache_creation_input_tokens, 0),
        };
        let tokens = Tokens {
            input: usage.input_tokens,
            output: usage.output_tokens,
            cache_read: usage.cache_read_input_tokens,
            cache_write,
            cache_write_1h,
            reasoning: usage.output_tokens_details.map_or(0, |d| d.thinking_tokens),
        };
        if tokens.total() == 0 {
            return;
        }
        out.push(UsageEvent {
            key: format!("claude-code:{id}:{}", l.request_id.unwrap_or_default()),
            source: "claude-code".into(),
            provider: "anthropic".into(),
            model,
            project: l.cwd.as_deref().and_then(project_name),
            session: l.session_id,
            ts,
            tokens,
            reported_cost: None,
        });
    }
}
