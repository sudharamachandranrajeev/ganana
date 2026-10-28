//! Claude Pro/Max limit readings written by the status line relay
//! (`crate::claude_relay`): one line per change, such as
//! `{"five_hour":{"used_percentage":23.5,"resets_at":1738425600},"ts":...}`.

use std::path::Path;

use serde_json::Value;

use super::LineParser;
use crate::model::{LimitSnapshot, UsageEvent};

pub struct Parser {
    limits: Vec<LimitSnapshot>,
}

impl LineParser for Parser {
    fn new(_: &Path, _: Option<&str>) -> Self {
        Self { limits: Vec::new() }
    }

    fn line(&mut self, line: &str, _: &mut Vec<UsageEvent>) {
        let Ok(v) = serde_json::from_str::<Value>(line) else { return };
        let Some(seen_at) = v["ts"].as_i64() else { return };
        for (name, window_minutes) in [("five_hour", 300), ("seven_day", 10080)] {
            let w = &v[name];
            let (Some(used_pct), Some(resets_at)) = (w["used_percentage"].as_f64(), w["resets_at"].as_i64()) else {
                continue;
            };
            self.limits.push(LimitSnapshot {
                meter: "claude".into(),
                provider: "anthropic".into(),
                window_minutes,
                resets_at,
                used_pct,
                seen_at,
            });
        }
    }

    fn take_limits(&mut self) -> Vec<LimitSnapshot> {
        std::mem::take(&mut self.limits)
    }
}
