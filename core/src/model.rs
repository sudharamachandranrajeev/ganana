use serde::{Deserialize, Serialize};

/// Token counts for one model call, normalised across providers:
/// - `input` is fresh (uncached) input only;
/// - `output` includes reasoning, because providers bill reasoning as output;
/// - `reasoning` is the part of `output` spent thinking, kept for display.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    /// Cache writes at the default (5-minute) lifetime.
    pub cache_write: i64,
    /// Anthropic cache writes with a 1-hour lifetime, billed higher.
    pub cache_write_1h: i64,
    pub reasoning: i64,
}

impl Tokens {
    pub fn total(&self) -> i64 {
        self.input + self.output + self.cache_read + self.cache_write + self.cache_write_1h
    }

    /// Everything the model read for this call: the prompt size.
    pub fn prompt(&self) -> i64 {
        self.input + self.cache_read + self.cache_write + self.cache_write_1h
    }

    pub fn add(&mut self, o: &Tokens) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
        self.cache_write_1h += o.cache_write_1h;
        self.reasoning += o.reasoning;
    }
}

/// One model call. `key` is unique per call across every source, so reading
/// the same log twice never counts a call twice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageEvent {
    pub key: String,
    /// The tool that recorded it: claude-code, codex, gemini-cli, qwen-code, opencode.
    pub source: String,
    /// Who serves the model: anthropic, openai, google, alibaba, ...
    pub provider: String,
    pub model: String,
    pub project: Option<String>,
    pub session: Option<String>,
    /// Unix time in milliseconds.
    pub ts: i64,
    pub tokens: Tokens,
    /// Cost the tool itself reported, when it reports one.
    pub reported_cost: Option<f64>,
}

/// One reading of a plan's usage limit, such as Claude's 5-hour window at
/// 23%. Only changes are stored, so the rows form the window's history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimitSnapshot {
    /// The limit's owner and name, such as `claude` or `codex`.
    pub meter: String,
    pub provider: String,
    /// Window length: 300 for the 5-hour window, 10080 for the weekly one.
    pub window_minutes: i64,
    /// Unix seconds when the window resets. Drifts by a few seconds between
    /// readings of the same window.
    pub resets_at: i64,
    pub used_pct: f64,
    /// Unix milliseconds when this value was first seen.
    pub seen_at: i64,
}

/// A subscription the user pays for, entered by hand.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct Plan {
    /// 0 for a plan not saved yet.
    pub id: i64,
    pub provider: String,
    pub name: String,
    pub monthly_usd: f64,
    /// Day of the month the plan renews, 1 to 31.
    pub renews_day: u32,
}

/// Last path component of a working directory, used as the project name.
pub fn project_name(cwd: &str) -> Option<String> {
    let name = cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next()?;
    (!name.is_empty()).then(|| name.to_string())
}

/// Parses an RFC 3339 timestamp into Unix milliseconds.
pub fn parse_ts(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|t| t.timestamp_millis())
}
