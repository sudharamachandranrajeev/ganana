//! Qwen Code: `~/.qwen/usage/token-usage-YYYY-MM.jsonl`, one line per model
//! call with its own id. OpenAI-style counts: `inputTokens` includes
//! `cachedTokens`, and `outputTokens` includes `thoughtsTokens`
//! (`totalTokens` = input + output).

use std::path::Path;

use serde::Deserialize;

use super::LineParser;
use crate::model::{Tokens, UsageEvent, parse_ts};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    id: String,
    timestamp: String,
    session_id: Option<String>,
    model: String,
    #[serde(default)]
    input_tokens: i64,
    #[serde(default)]
    output_tokens: i64,
    #[serde(default)]
    cached_tokens: i64,
    #[serde(default)]
    thoughts_tokens: i64,
}

pub struct Parser;

impl LineParser for Parser {
    fn new(_: &Path, _: Option<&str>) -> Self {
        Self
    }

    fn line(&mut self, line: &str, out: &mut Vec<UsageEvent>) {
        let Ok(l) = serde_json::from_str::<Line>(line) else { return };
        let Some(ts) = parse_ts(&l.timestamp) else { return };
        let tokens = Tokens {
            input: (l.input_tokens - l.cached_tokens).max(0),
            output: l.output_tokens,
            cache_read: l.cached_tokens,
            reasoning: l.thoughts_tokens,
            ..Tokens::default()
        };
        if tokens.total() == 0 {
            return;
        }
        out.push(UsageEvent {
            key: format!("qwen-code:{}", l.id),
            source: "qwen-code".into(),
            provider: "alibaba".into(),
            model: l.model,
            project: None,
            session: l.session_id,
            ts,
            tokens,
            reported_cost: None,
        });
    }
}
