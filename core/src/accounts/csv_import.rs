//! Usage exports from providers without a usage API (Gemini, TypeSafe,
//! Qwen Cloud, ...): the user picks which column holds what, guided by a
//! guess from the header names.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

use super::{Batch, CostRow, UsageRow};
use crate::model::Tokens;

/// Column index for each field; `None` when the file has no such column.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mapping {
    pub date: Option<usize>,
    pub model: Option<usize>,
    pub input: Option<usize>,
    pub output: Option<usize>,
    pub cost: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Inspect {
    pub headers: Vec<String>,
    pub guess: Mapping,
    /// The first few data rows, to show next to the column choices.
    pub sample: Vec<Vec<String>>,
    pub rows: usize,
}

fn reader(text: &str) -> csv::Reader<&[u8]> {
    csv::ReaderBuilder::new().flexible(true).trim(csv::Trim::All).from_reader(text.as_bytes())
}

/// Headers, a guessed mapping and sample rows.
pub fn inspect(text: &str) -> Result<Inspect> {
    let mut r = reader(text);
    let headers: Vec<String> = r.headers().context("the file has no header row")?.iter().map(str::to_string).collect();
    let mut sample = Vec::new();
    let mut rows = 0;
    for record in r.records() {
        let record = record.context("the file is not valid CSV")?;
        if sample.len() < 5 {
            sample.push(record.iter().map(str::to_string).collect());
        }
        rows += 1;
    }
    Ok(Inspect { guess: guess(&headers), headers, sample, rows })
}

/// First header containing one of the words, so "Input tokens" maps to input.
fn guess(headers: &[String]) -> Mapping {
    let lower: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();
    let find = |words: &[&str], not: &[&str]| {
        lower.iter().position(|h| words.iter().any(|w| h.contains(w)) && !not.iter().any(|n| h.contains(n)))
    };
    Mapping {
        date: find(&["date", "day", "time", "period", "start"], &[]),
        model: find(&["model"], &[]),
        input: find(&["input", "prompt"], &["cache", "cost", "price"]),
        output: find(&["output", "completion"], &["cost", "price"]),
        cost: find(&["cost", "amount", "usd", "spend", "charge", "price"], &[]),
    }
}

/// Reads the whole file into one batch covering its first to last day.
/// Rows for the same day and model are added together.
pub fn parse(text: &str, m: &Mapping) -> Result<Batch> {
    let Some(date_col) = m.date else { bail!("choose the column that holds the date") };
    if m.input.is_none() && m.output.is_none() && m.cost.is_none() {
        bail!("choose at least one column with tokens or cost");
    }
    let mut usage: BTreeMap<(String, String), Tokens> = BTreeMap::new();
    let mut costs: BTreeMap<(String, String), f64> = BTreeMap::new();
    for (i, record) in reader(text).records().enumerate() {
        let line = i + 2; // the header is line 1
        let record = record.with_context(|| format!("line {line} is not valid CSV"))?;
        let cell = |c: Option<usize>| c.and_then(|c| record.get(c)).unwrap_or("");
        let raw_date = cell(Some(date_col));
        if raw_date.is_empty() && record.iter().all(str::is_empty) {
            continue;
        }
        let day = parse_day(raw_date).with_context(|| format!("line {line}: cannot read the date \"{raw_date}\""))?;
        let model = Some(cell(m.model)).filter(|s| !s.is_empty()).unwrap_or("unknown").to_string();
        let number = |c: Option<usize>| -> Result<f64> {
            parse_number(cell(c)).with_context(|| format!("line {line}: \"{}\" is not a number", cell(c)))
        };
        let (input, output, cost) = (number(m.input)?, number(m.output)?, number(m.cost)?);
        if m.input.is_some() || m.output.is_some() {
            let t = usage.entry((day.clone(), model.clone())).or_default();
            t.input += input.round() as i64;
            t.output += output.round() as i64;
        }
        if m.cost.is_some() {
            *costs.entry((day, model)).or_default() += cost;
        }
    }
    let days = usage.keys().chain(costs.keys()).map(|(d, _)| d);
    let (Some(from), Some(to)) = (days.clone().min().cloned(), days.max().cloned()) else {
        bail!("the file has no data rows")
    };
    Ok(Batch {
        from,
        to,
        usage: usage
            .into_iter()
            .map(|((day, model), tokens)| UsageRow { day, model, tokens, requests: None })
            .collect(),
        costs: costs
            .into_iter()
            .map(|((day, model), usd)| CostRow { day, item: model.clone(), model: Some(model), usd })
            .collect(),
        credit: None,
    })
}

/// Accepts ISO dates and times, Unix seconds or milliseconds, and
/// year/month/day with slashes. Day/month order is never guessed.
fn parse_day(s: &str) -> Option<String> {
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Some(t.naive_utc().date().to_string());
    }
    if let Some(d) = s.get(..10).and_then(|p| NaiveDate::parse_from_str(p, "%Y-%m-%d").ok()) {
        return Some(d.to_string());
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y/%m/%d") {
        return Some(d.to_string());
    }
    if let Ok(t) = NaiveDateTime::parse_from_str(s, "%Y/%m/%d %H:%M:%S") {
        return Some(t.date().to_string());
    }
    let n: i64 = s.parse().ok()?;
    let secs = if n > 100_000_000_000 { n / 1000 } else { n };
    DateTime::from_timestamp(secs, 0).map(|t| t.date_naive().to_string())
}

/// A number with optional currency sign, thousands commas and unit text.
fn parse_number(s: &str) -> Option<f64> {
    let cleaned: String = s.chars().filter(|c| !matches!(c, '$' | ',' | ' ')).collect();
    let cleaned = cleaned.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    if cleaned.is_empty() {
        return Some(0.0);
    }
    cleaned.parse::<f64>().ok().filter(|n| n.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_numbers() {
        assert_eq!(parse_day("2026-10-01").as_deref(), Some("2026-10-01"));
        assert_eq!(parse_day("2026-10-01 13:00:00 UTC").as_deref(), Some("2026-10-01"));
        assert_eq!(parse_day("2026-10-01T23:30:00-05:00").as_deref(), Some("2026-10-02"));
        assert_eq!(parse_day("2026/10/01").as_deref(), Some("2026-10-01"));
        assert_eq!(parse_day("1790000000").as_deref(), Some("2026-09-21"));
        assert_eq!(parse_day("1790000000000").as_deref(), Some("2026-09-21"));
        assert_eq!(parse_day("01/10/2026"), None);
        assert_eq!(parse_number("$1,234.50"), Some(1234.5));
        assert_eq!(parse_number("12 USD"), Some(12.0));
        assert_eq!(parse_number(""), Some(0.0));
        assert_eq!(parse_number("n/a"), None);
    }
}
