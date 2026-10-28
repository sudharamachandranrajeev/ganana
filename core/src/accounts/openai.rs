//! OpenAI organization Usage and Costs API. Needs an Admin key, which only
//! organization owners can create.

use anyhow::{Result, bail};
use chrono::{DateTime, NaiveDate};

use super::{Batch, CostRow, Get, UsageRow, int, pages};
use crate::model::Tokens;

const BASE: &str = "https://api.openai.com/v1/organization";

pub(super) fn fetch(get: &mut Get, from: NaiveDate, today: NaiveDate) -> Result<Batch> {
    let mut batch = Batch::new(from, today);
    let start = from.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc().timestamp();

    let usage = format!("{BASE}/usage/completions?start_time={start}&bucket_width=1d&group_by=model&limit=31");
    for bucket in pages(get, &usage)? {
        let day = day_of(&bucket["start_time"])?;
        for r in results(&bucket) {
            // `input_tokens` includes the cached part.
            let cached = int(&r["input_cached_tokens"]);
            batch.usage.push(UsageRow {
                day: day.clone(),
                model: r["model"].as_str().unwrap_or("unknown").to_string(),
                tokens: Tokens {
                    input: (int(&r["input_tokens"]) - cached).max(0),
                    output: int(&r["output_tokens"]),
                    cache_read: cached,
                    ..Tokens::default()
                },
                requests: Some(int(&r["num_model_requests"])),
            });
        }
    }

    let costs = format!("{BASE}/costs?start_time={start}&bucket_width=1d&group_by=line_item&limit=180");
    for bucket in pages(get, &costs)? {
        let day = day_of(&bucket["start_time"])?;
        for r in results(&bucket) {
            let amount = &r["amount"];
            let currency = amount["currency"].as_str().unwrap_or("usd");
            if !currency.eq_ignore_ascii_case("usd") {
                bail!("OpenAI reported a cost in {currency}; only US dollars are supported");
            }
            batch.costs.push(CostRow {
                day: day.clone(),
                item: r["line_item"].as_str().unwrap_or("Other").to_string(),
                model: None,
                usd: amount["value"].as_f64().unwrap_or(0.0),
            });
        }
    }
    Ok(batch)
}

/// A bucket's items: the API names the list `result`.
fn results(bucket: &serde_json::Value) -> impl Iterator<Item = &serde_json::Value> {
    bucket["result"].as_array().or(bucket["results"].as_array()).into_iter().flatten()
}

/// The UTC day of a Unix-seconds bucket start.
fn day_of(v: &serde_json::Value) -> Result<String> {
    match v.as_i64().and_then(|s| DateTime::from_timestamp(s, 0)) {
        Some(t) => Ok(t.date_naive().to_string()),
        None => bail!("a bucket has no start time"),
    }
}
