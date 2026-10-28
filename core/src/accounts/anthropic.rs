//! Anthropic Usage and Cost Admin API. Needs an Admin key; individual
//! (non-organization) accounts have none.
//! <https://platform.claude.com/docs/en/manage-claude/usage-cost-api>

use anyhow::Result;
use chrono::NaiveDate;

use super::{Batch, CostRow, Get, UsageRow, day_of, int, pages};
use crate::model::Tokens;

const BASE: &str = "https://api.anthropic.com/v1/organizations";

pub(super) fn fetch(get: &mut Get, from: NaiveDate, today: NaiveDate) -> Result<Batch> {
    let mut batch = Batch::new(from, today);
    let start = format!("{from}T00:00:00Z");

    let usage =
        format!("{BASE}/usage_report/messages?starting_at={start}&bucket_width=1d&group_by%5B%5D=model&limit=31");
    for bucket in pages(get, &usage)? {
        let day = day_of(&bucket["starting_at"])?;
        for r in bucket["results"].as_array().into_iter().flatten() {
            let cache = &r["cache_creation"];
            batch.usage.push(UsageRow {
                day: day.clone(),
                model: r["model"].as_str().unwrap_or("unknown").to_string(),
                tokens: Tokens {
                    input: int(&r["uncached_input_tokens"]),
                    output: int(&r["output_tokens"]),
                    cache_read: int(&r["cache_read_input_tokens"]),
                    cache_write: int(&cache["ephemeral_5m_input_tokens"]),
                    cache_write_1h: int(&cache["ephemeral_1h_input_tokens"]),
                    reasoning: 0,
                },
                requests: None,
            });
        }
    }

    let cost = format!("{BASE}/cost_report?starting_at={start}&bucket_width=1d&group_by%5B%5D=description&limit=31");
    for bucket in pages(get, &cost)? {
        let day = day_of(&bucket["starting_at"])?;
        for r in bucket["results"].as_array().into_iter().flatten() {
            // Amounts are decimal strings in cents.
            let cents: f64 = r["amount"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            batch.costs.push(CostRow {
                day: day.clone(),
                item: r["description"].as_str().unwrap_or("Other").to_string(),
                model: r["model"].as_str().map(str::to_string),
                usd: cents / 100.0,
            });
        }
    }
    Ok(batch)
}
