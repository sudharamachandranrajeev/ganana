//! Provider accounts: the three API readers against recorded response
//! shapes, the stored batches, the report and CSV import. Synthetic data
//! only; nothing here touches the network.

use std::collections::HashMap;

use anyhow::{Result, bail};
use chrono::{DateTime, TimeZone, Utc};
use ganana_core::accounts::{self, Batch, csv_import};
use ganana_core::{Catalog, Store, Tokens, UsageEvent};
use serde_json::{Value, json};
use tempfile::TempDir;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap()
}

/// Serves the first recorded response whose pattern the URL contains, and
/// remembers every URL asked.
struct Recorded {
    responses: Vec<(&'static str, Value)>,
    asked: Vec<String>,
}

impl Recorded {
    fn new(responses: Vec<(&'static str, Value)>) -> Self {
        Self { responses, asked: Vec::new() }
    }

    fn fetch(&mut self, kind: &str, first: bool) -> Result<Batch> {
        let responses = &self.responses;
        let asked = &mut self.asked;
        accounts::fetch(
            kind,
            &mut |url: &str| {
                asked.push(url.to_string());
                match responses.iter().find(|(prefix, _)| url.contains(prefix)) {
                    Some((_, v)) => Ok(v.clone()),
                    None => bail!("no recorded response for {url}"),
                }
            },
            first,
            now(),
        )
    }
}

fn store() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    let store = Store::open(&dir.path().join("ganana.db")).unwrap();
    (dir, store)
}

#[test]
fn anthropic_reads_usage_and_cost_across_pages() {
    let usage = "https://api.anthropic.com/v1/organizations/usage_report/messages";
    let cost = "https://api.anthropic.com/v1/organizations/cost_report";
    let mut r = Recorded::new(vec![
        (
            // The second page first: its URL also contains the first's.
            "/usage_report/messages?starting_at=2026-07-31T00:00:00Z&bucket_width=1d&group_by%5B%5D=model&limit=31&page=page_Mj%3D",
            json!({"data": [{"starting_at": "2026-10-28T00:00:00Z", "ending_at": "2026-10-29T00:00:00Z", "results": []}],
                   "has_more": false, "next_page": null}),
        ),
        (
            usage,
            json!({"data": [{"starting_at": "2026-10-27T00:00:00Z", "ending_at": "2026-10-28T00:00:00Z", "results": [{
                "uncached_input_tokens": 1500, "cache_read_input_tokens": 200, "output_tokens": 500,
                "cache_creation": {"ephemeral_5m_input_tokens": 40, "ephemeral_1h_input_tokens": 10},
                "server_tool_use": {"web_search_requests": 0}, "model": "claude-opus-5-5",
                "api_key_id": null, "workspace_id": null, "service_tier": null}]}],
                "has_more": true, "next_page": "page_Mj="}),
        ),
        (
            cost,
            json!({"data": [{"starting_at": "2026-10-27T00:00:00Z", "ending_at": "2026-10-28T00:00:00Z", "results": [
                {"amount": "123.45", "currency": "USD", "description": "Claude Opus 5.5 Usage - Input Tokens",
                 "cost_type": "tokens", "model": "claude-opus-5-5", "token_type": "uncached_input_tokens"},
                {"amount": "50", "currency": "USD", "description": "Web Search", "cost_type": "web_search", "model": null}]}],
                "has_more": false, "next_page": null}),
        ),
    ]);
    let batch = r.fetch("anthropic", true).unwrap();

    assert_eq!((batch.from.as_str(), batch.to.as_str()), ("2026-07-31", "2026-10-28"), "first sync reads 90 days");
    assert_eq!(r.asked.len(), 3, "{:?}", r.asked);
    assert!(r.asked[1].ends_with("&page=page_Mj%3D"), "cursor is encoded: {}", r.asked[1]);
    assert_eq!(batch.usage.len(), 1);
    let u = &batch.usage[0];
    assert_eq!((u.day.as_str(), u.model.as_str(), u.requests), ("2026-10-27", "claude-opus-5-5", None));
    assert_eq!(
        u.tokens,
        Tokens { input: 1500, output: 500, cache_read: 200, cache_write: 40, cache_write_1h: 10, reasoning: 0 }
    );
    assert_eq!(batch.costs.len(), 2);
    assert!((batch.costs[0].usd - 1.2345).abs() < 1e-9, "amounts are cents");
    assert_eq!(batch.costs[0].model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!((batch.costs[1].item.as_str(), batch.costs[1].usd), ("Web Search", 0.5));

    // A later sync re-reads only the last 31 days.
    r.responses.retain(|(p, _)| !p.contains("page="));
    r.responses[0].1["has_more"] = json!(false);
    let later = r.fetch("anthropic", false).unwrap();
    assert_eq!(later.from, "2026-09-28");
    assert!(r.asked[3].contains("starting_at=2026-09-28T00:00:00Z"), "{}", r.asked[3]);
}

#[test]
fn openai_splits_cached_input_and_reads_costs() {
    let start = Utc.with_ymd_and_hms(2026, 10, 27, 0, 0, 0).unwrap().timestamp();
    let mut r = Recorded::new(vec![
        (
            "https://api.openai.com/v1/organization/usage/completions",
            json!({"object": "page", "has_more": false, "next_page": null, "data": [{"object": "bucket",
                "start_time": start, "end_time": start + 86400, "result": [{
                    "object": "organization.usage.completions.result", "input_tokens": 1000,
                    "input_cached_tokens": 600, "output_tokens": 300, "num_model_requests": 7, "model": "gpt-6-sol"}]}]}),
        ),
        (
            "https://api.openai.com/v1/organization/costs",
            json!({"object": "page", "has_more": false, "next_page": null, "data": [{"object": "bucket",
                "start_time": start, "end_time": start + 86400, "result": [{
                    "object": "organization.costs.result", "amount": {"value": 0.42, "currency": "usd"},
                    "line_item": "gpt-6-sol, input", "project_id": null}]}]}),
        ),
    ]);
    let batch = r.fetch("openai", false).unwrap();

    let from = Utc.with_ymd_and_hms(2026, 9, 28, 0, 0, 0).unwrap().timestamp();
    assert!(r.asked[0].contains(&format!("start_time={from}&bucket_width=1d&group_by=model")), "{}", r.asked[0]);
    let u = &batch.usage[0];
    assert_eq!((u.day.as_str(), u.requests), ("2026-10-27", Some(7)));
    assert_eq!(u.tokens, Tokens { input: 400, output: 300, cache_read: 600, ..Tokens::default() });
    assert_eq!((batch.costs[0].item.as_str(), batch.costs[0].usd), ("gpt-6-sol, input", 0.42));

    let mut euro = Recorded::new(vec![
        ("https://api.openai.com/v1/organization/usage/completions", json!({"data": [], "has_more": false})),
        (
            "https://api.openai.com/v1/organization/costs",
            json!({"data": [{"start_time": start, "result": [{"amount": {"value": 1.0, "currency": "eur"}}]}], "has_more": false}),
        ),
    ]);
    let e = euro.fetch("openai", false).unwrap_err();
    assert!(e.to_string().contains("eur"), "{e}");
}

#[test]
fn openrouter_reads_activity_and_credit() {
    let mut r = Recorded::new(vec![
        (
            "https://openrouter.ai/api/v1/activity",
            json!({"data": [{"date": "2026-10-20", "model": "openai/gpt-6-sol", "model_permaslug": "openai/gpt-6-sol",
                "endpoint_id": "x", "provider_name": "OpenAI", "prompt_tokens": 50, "cached_tokens": 10,
                "completion_tokens": 125, "reasoning_tokens": 25, "requests": 5, "usage": 0.015,
                "byok_usage_inference": 0.012}]}),
        ),
        ("https://openrouter.ai/api/v1/credits", json!({"data": {"total_credits": 100.5, "total_usage": 25.75}})),
    ]);
    let batch = r.fetch("openrouter", true).unwrap();

    assert_eq!((batch.from.as_str(), batch.to.as_str()), ("2026-09-28", "2026-10-27"), "30 completed days");
    let u = &batch.usage[0];
    assert_eq!(u.tokens, Tokens { input: 40, output: 125, cache_read: 10, reasoning: 25, ..Tokens::default() });
    assert_eq!(u.requests, Some(5));
    assert_eq!(batch.costs[0].usd, 0.015, "credit spent only, not the other provider's BYOK bill");
    assert_eq!(batch.credit, Some((100.5, 25.75)));
}

#[test]
fn csv_accounts_cannot_sync() {
    let mut r = Recorded::new(vec![]);
    assert!(r.fetch("csv", false).is_err());
    assert!(r.asked.is_empty());
}

fn usage(day: &str, model: &str, input: i64) -> accounts::UsageRow {
    accounts::UsageRow {
        day: day.into(),
        model: model.into(),
        tokens: Tokens { input, ..Tokens::default() },
        requests: Some(1),
    }
}

fn cost(day: &str, item: &str, usd: f64) -> accounts::CostRow {
    accounts::CostRow { day: day.into(), item: item.into(), model: None, usd }
}

#[test]
fn a_sync_replaces_its_days_and_keeps_older_ones() {
    let (_dir, store) = store();
    let id = store.add_account("anthropic", "anthropic", "Work org", Some("ab12")).unwrap();
    let first = Batch {
        from: "2026-10-01".into(),
        to: "2026-10-28".into(),
        usage: vec![usage("2026-10-01", "m", 100), usage("2026-10-27", "m", 5)],
        costs: vec![cost("2026-10-01", "x", 1.0), cost("2026-10-27", "x", 2.0)],
        credit: None,
    };
    store.save_account_batch(id, &first, 1).unwrap();
    // A later sync covers the 20th onwards, with two rows for one model and day.
    let later = Batch {
        from: "2026-10-20".into(),
        to: "2026-10-28".into(),
        usage: vec![usage("2026-10-27", "m", 7), usage("2026-10-27", "m", 3)],
        costs: vec![cost("2026-10-27", "x", 4.0)],
        credit: Some((10.0, 4.0)),
    };
    store.save_account_batch(id, &later, 2).unwrap();

    let rows: HashMap<String, i64> =
        store.account_usage("2026-01-01").unwrap().into_iter().map(|(_, u)| (u.day, u.tokens.input)).collect();
    assert_eq!(rows["2026-10-01"], 100, "outside the later range: kept");
    assert_eq!(rows["2026-10-27"], 10, "replaced, and same-key rows added");
    let costs: Vec<f64> = store.account_costs("2026-01-01").unwrap().into_iter().map(|(_, c)| c.usd).collect();
    assert_eq!(costs, vec![1.0, 4.0]);
    let a = store.account(id).unwrap().unwrap();
    assert_eq!((a.synced_at, a.credit_total, a.credit_used, a.error), (Some(2), Some(10.0), Some(4.0), None));

    store.set_account_error(id, Some("refused")).unwrap();
    assert_eq!(store.account(id).unwrap().unwrap().error.as_deref(), Some("refused"));

    store.delete_account(id).unwrap();
    assert!(store.accounts().unwrap().is_empty());
    assert!(store.account_usage("2026-01-01").unwrap().is_empty());
    assert!(store.account_costs("2026-01-01").unwrap().is_empty());
}

#[test]
fn report_totals_accounts_by_day_beside_the_local_estimate() {
    let (_dir, store) = store();
    let a = store.add_account("anthropic", "anthropic", "Work org", Some("ab12")).unwrap();
    let o = store.add_account("csv", "google", "Gemini export", None).unwrap();
    let mut batch = Batch {
        from: "2026-10-27".into(),
        to: "2026-10-28".into(),
        usage: vec![usage("2026-10-27", "claude-sonnet-5-5", 1000), usage("2026-10-28", "claude-opus-5-5", 50)],
        costs: vec![cost("2026-10-27", "Sonnet input", 3.0), cost("2026-10-28", "Opus input", 1.0)],
        credit: None,
    };
    batch.costs[0].model = Some("claude-sonnet-5-5".into());
    store.save_account_batch(a, &batch, 1).unwrap();
    store
        .save_account_batch(
            o,
            &Batch {
                from: "2026-10-28".into(),
                to: "2026-10-28".into(),
                costs: vec![cost("2026-10-28", "gemini", 0.5)],
                ..Batch::default()
            },
            1,
        )
        .unwrap();
    // A row before the report's first day stays out.
    store
        .save_account_batch(
            a,
            &Batch {
                from: "2026-09-01".into(),
                to: "2026-09-01".into(),
                costs: vec![cost("2026-09-01", "old", 99.0)],
                ..Batch::default()
            },
            1,
        )
        .unwrap();
    // Local Anthropic use on the 27th, priced from the bundled catalog.
    let event = UsageEvent {
        key: "e1".into(),
        source: "claude-code".into(),
        provider: "anthropic".into(),
        model: "claude-sonnet-5-5".into(),
        project: None,
        session: None,
        ts: Utc.with_ymd_and_hms(2026, 10, 27, 9, 0, 0).unwrap().timestamp_millis(),
        tokens: Tokens { input: 1_000_000, ..Tokens::default() },
        reported_cost: None,
    };
    store.commit("test", &Default::default(), &[event], &[]).unwrap();

    let r = accounts::report(&store, &Catalog::bundled(), 7, now()).unwrap();
    assert_eq!(r.daily.len(), 7);
    assert_eq!(r.daily.last().unwrap().date, "2026-10-28");
    assert!((r.billed - 4.5).abs() < 1e-9, "{}", r.billed);
    let day27 = r.daily.iter().find(|d| d.date == "2026-10-27").unwrap();
    assert_eq!(day27.cost_by_provider["anthropic"], 3.0);
    assert_eq!(day27.tokens_by_provider["anthropic"], 1000);
    let day28 = r.daily.last().unwrap();
    assert_eq!((day28.cost_by_provider["anthropic"], day28.cost_by_provider["google"]), (1.0, 0.5));

    let work = r.accounts.iter().find(|v| v.account.id == a).unwrap();
    assert_eq!((work.totals.cost, work.totals.total_tokens, work.totals.calls), (4.0, 1050, 2));
    assert_eq!(work.items[0].name, "Sonnet input", "biggest bill first");
    assert_eq!(work.models[0].name, "claude-sonnet-5-5");
    assert_eq!(work.models[0].totals.cost, 3.0, "cost joins its model");
    assert!((work.local_cost - 2.0).abs() < 1e-9, "1M Sonnet 5.5 input tokens at $2/M: {}", work.local_cost);
    let gemini = r.accounts.iter().find(|v| v.account.id == o).unwrap();
    assert_eq!((gemini.totals.cost, gemini.local_cost), (0.5, 0.0));
}

#[test]
fn csv_import_guesses_columns_and_sums_days() {
    let text = "\u{feff}Date,Model,Input tokens,Cached input tokens,Output tokens,\"Cost (USD)\"\n\
                2026-10-01,gemini-3.5-flash,\"1,000\",5,200,$0.50\n\
                2026-10-01 14:00:00,gemini-3.5-flash,500,0,100,0.25\n\
                2026-10-03,,10,0,0,\n\
                ,,,,,\n";
    let i = csv_import::inspect(text).unwrap();
    assert_eq!(i.headers[0], "Date", "byte order mark dropped");
    assert_eq!(i.rows, 4);
    assert_eq!(i.sample[0][1], "gemini-3.5-flash");
    let m = &i.guess;
    assert_eq!((m.date, m.model, m.input, m.output, m.cost), (Some(0), Some(1), Some(2), Some(4), Some(5)));

    let b = csv_import::parse(text, m).unwrap();
    assert_eq!((b.from.as_str(), b.to.as_str()), ("2026-10-01", "2026-10-03"));
    assert_eq!(b.usage.len(), 2);
    assert_eq!((b.usage[0].tokens.input, b.usage[0].tokens.output), (1500, 300));
    assert_eq!(b.usage[1].model, "unknown");
    assert!((b.costs[0].usd - 0.75).abs() < 1e-9);

    let bad = "Date,Cost\n2026-10-01,1\n31/10/2026,2\n";
    let m = csv_import::inspect(bad).unwrap().guess;
    let e = csv_import::parse(bad, &m).unwrap_err();
    assert!(format!("{e:#}").contains("line 3"), "{e:#}");
    let none = csv_import::Mapping { date: Some(0), ..Default::default() };
    assert!(csv_import::parse(bad, &none).is_err(), "needs a tokens or cost column");
}
