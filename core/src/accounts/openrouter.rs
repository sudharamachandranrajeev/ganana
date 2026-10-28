//! OpenRouter activity and credits. Both need a management key.

use anyhow::{Context, Result};
use chrono::{Days, NaiveDate};

use super::{Batch, CostRow, Get, UsageRow, int};
use crate::model::Tokens;

const BASE: &str = "https://openrouter.ai/api/v1";

/// Activity covers the last 30 completed UTC days, so today is not touched.
pub(super) fn fetch(get: &mut Get, today: NaiveDate) -> Result<Batch> {
    let mut batch = Batch::new(today - Days::new(30), today - Days::new(1));

    let activity = get(&format!("{BASE}/activity"))?;
    for r in activity["data"].as_array().context("the activity response has no data list")? {
        let day = r["date"].as_str().context("an activity row has no date")?.to_string();
        let model = r["model"].as_str().unwrap_or("unknown").to_string();
        // Cached tokens are part of the prompt tokens.
        let cached = int(&r["cached_tokens"]);
        batch.usage.push(UsageRow {
            day: day.clone(),
            model: model.clone(),
            tokens: Tokens {
                input: (int(&r["prompt_tokens"]) - cached).max(0),
                output: int(&r["completion_tokens"]),
                cache_read: cached,
                reasoning: int(&r["reasoning_tokens"]),
                ..Tokens::default()
            },
            requests: Some(int(&r["requests"])),
        });
        // `usage` is OpenRouter credit spent. Bring-your-own-key spend is
        // billed by the other provider, so it is left out here.
        batch.costs.push(CostRow {
            day,
            item: model.clone(),
            model: Some(model),
            usd: r["usage"].as_f64().unwrap_or(0.0),
        });
    }

    let credits = get(&format!("{BASE}/credits"))?;
    let data = &credits["data"];
    if let (Some(total), Some(used)) = (data["total_credits"].as_f64(), data["total_usage"].as_f64()) {
        batch.credit = Some((total, used));
    }
    Ok(batch)
}
