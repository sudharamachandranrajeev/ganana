use serde::Serialize;

/// Token counts for one model call, normalised across providers:
/// - `input` is fresh (uncached) input only;
/// - `output` includes reasoning, because providers bill reasoning as output;
/// - `reasoning` is the part of `output` spent thinking, kept for display.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
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
#[derive(Debug, Clone, PartialEq)]
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

/// Last path component of a working directory, used as the project name.
pub fn project_name(cwd: &str) -> Option<String> {
    let name = cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next()?;
    (!name.is_empty()).then(|| name.to_string())
}

/// Parses an RFC 3339 timestamp into Unix milliseconds.
pub fn parse_ts(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|t| t.timestamp_millis())
}
