//! Codex CLI: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`.
//!
//! Newer files have a `token_usage_record` per response; older ones only
//! have `event_msg/token_count`. Both carry the running total for the
//! session, so the key is the session plus that total: the record and the
//! count for one response share a key and count once.
//!
//! OpenAI-style counts: `input_tokens` includes cached input, and
//! `output_tokens` includes reasoning.
//!
//! `token_count` events also carry the ChatGPT plan's limits: a `primary`
//! (5-hour) and `secondary` (weekly) window with the percentage used.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::LineParser;
use crate::model::{LimitSnapshot, Tokens, UsageEvent, parse_ts, project_name};

/// Context that earlier lines set and later usage lines need.
#[derive(Serialize, Deserialize, Default)]
struct State {
    session: Option<String>,
    model: Option<String>,
    cwd: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Usage {
    input_tokens: i64,
    cached_input_tokens: i64,
    cache_write_input_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
}

impl Usage {
    fn tokens(&self) -> Tokens {
        Tokens {
            input: (self.input_tokens - self.cached_input_tokens - self.cache_write_input_tokens).max(0),
            output: self.output_tokens,
            cache_read: self.cached_input_tokens,
            cache_write: self.cache_write_input_tokens,
            cache_write_1h: 0,
            reasoning: self.reasoning_output_tokens,
        }
    }
}

pub struct Parser {
    state: State,
    file_stem: String,
    limits: Vec<LimitSnapshot>,
}

/// Readings from one `rate_limits` object. `limit_id` names the meter;
/// meters without windows (such as `premium`) give nothing.
fn limits(rate_limits: &Value, seen_at: i64, out: &mut Vec<LimitSnapshot>) {
    let meter = rate_limits["limit_id"].as_str().unwrap_or("codex");
    for w in [&rate_limits["primary"], &rate_limits["secondary"]] {
        let (Some(used_pct), Some(window_minutes), Some(resets_at)) =
            (w["used_percent"].as_f64(), w["window_minutes"].as_i64(), w["resets_at"].as_i64())
        else {
            continue;
        };
        out.push(LimitSnapshot {
            meter: meter.to_string(),
            provider: "openai".into(),
            window_minutes,
            resets_at,
            used_pct,
            seen_at,
        });
    }
}

impl LineParser for Parser {
    fn new(path: &Path, state: Option<&str>) -> Self {
        Self {
            state: state.and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default(),
            file_stem: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            limits: Vec::new(),
        }
    }

    fn state(&self) -> Option<String> {
        serde_json::to_string(&self.state).ok()
    }

    fn take_limits(&mut self) -> Vec<LimitSnapshot> {
        std::mem::take(&mut self.limits)
    }

    fn line(&mut self, line: &str, out: &mut Vec<UsageEvent>) {
        let Ok(v) = serde_json::from_str::<Value>(line) else { return };
        let payload = &v["payload"];
        let kind = v["type"].as_str().unwrap_or_default();
        let text = |k: &str| payload[k].as_str().map(str::to_string);

        // (usage of this response, running total for the session)
        let (usage, running_total) = match (kind, payload["type"].as_str()) {
            ("session_meta", _) => {
                self.state.session = text("id").or_else(|| text("session_id"));
                self.state.cwd = text("cwd").or(self.state.cwd.take());
                return;
            }
            ("turn_context", _) => {
                self.state.model = text("model").or(self.state.model.take());
                self.state.cwd = text("cwd").or(self.state.cwd.take());
                return;
            }
            ("token_usage_record", _) => {
                let usage: Usage = serde_json::from_value(payload["usage"].clone()).unwrap_or_default();
                let total = payload["thread_token_usage"]["total_tokens"].as_i64();
                (usage, total)
            }
            ("event_msg", Some("token_count")) => {
                if let Some(ts) = v["timestamp"].as_str().and_then(parse_ts) {
                    limits(&payload["rate_limits"], ts, &mut self.limits);
                }
                let info = &payload["info"];
                let usage: Usage = serde_json::from_value(info["last_token_usage"].clone()).unwrap_or_default();
                let total = info["total_token_usage"]["total_tokens"].as_i64();
                (usage, total)
            }
            _ => return,
        };
        let Some(running_total) = running_total else { return };
        let Some(ts) = v["timestamp"].as_str().and_then(parse_ts) else { return };
        let tokens = usage.tokens();
        if usage.total_tokens == 0 && tokens.total() == 0 {
            return;
        }
        let session = self.state.session.clone().unwrap_or_else(|| self.file_stem.clone());
        out.push(UsageEvent {
            key: format!("codex:{session}:{running_total}"),
            source: "codex".into(),
            provider: "openai".into(),
            model: self.state.model.clone().unwrap_or_else(|| "unknown".into()),
            project: self.state.cwd.as_deref().and_then(project_name),
            session: Some(session),
            ts,
            tokens,
            reported_cost: None,
        });
    }
}
