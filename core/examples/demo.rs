//! Fills a new database with made-up usage and prints the dashboard's data
//! as JSON, for the README screenshots. No real logs or keys are read.
//!
//!     cargo run -p ganana-core --example demo -- /tmp/ganana-demo.db > demo.json

use chrono::{Duration, Local, Utc};
use ganana_core::accounts::{self, Batch, CostRow, UsageRow};
use ganana_core::db::Cursor;
use ganana_core::{Catalog, LimitSnapshot, Plan, Store, Tokens, UsageEvent, limits, overview};
use serde_json::json;

/// A small deterministic random source, so every run draws the same pictures.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn main() -> anyhow::Result<()> {
    let db = std::env::args().nth(1).expect("usage: demo <new db path>");
    let _ = std::fs::remove_file(&db);
    let store = Store::open(db.as_ref())?;
    let now = Local::now();
    let now_ms = now.timestamp_millis();
    let mut rng = Rng(7);

    // (source, provider, model, share of calls, typical input, typical output)
    let models = [
        ("claude-code", "anthropic", "claude-sonnet-4-5", 0.42, 6_000.0, 900.0),
        ("claude-code", "anthropic", "claude-opus-4-5", 0.12, 8_000.0, 1_400.0),
        ("claude-code", "anthropic", "claude-haiku-4-5", 0.10, 3_000.0, 400.0),
        ("codex", "openai", "gpt-5", 0.18, 5_000.0, 1_100.0),
        ("gemini-cli", "google", "gemini-2.5-pro", 0.08, 7_000.0, 800.0),
        ("qwen-code", "alibaba", "qwen-flash", 0.06, 4_000.0, 600.0),
        ("opencode", "openrouter", "team-router-fast", 0.04, 3_000.0, 500.0),
    ];
    let projects = ["garden-planner", "recipe-box", "photo-sorter", "budget-sheet", "website"];

    let since_midnight =
        (now - now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).unwrap()).num_minutes().max(1);
    let mut events = Vec::new();
    for day in 0..90i64 {
        let date = now - Duration::days(day);
        let weekend = matches!(date.format("%a").to_string().as_str(), "Sat" | "Sun");
        let busy = if weekend { 0.35 } else { 1.0 } * (0.6 + rng.next() * 0.8);
        let calls = (busy * 200.0) as i64;
        for c in 0..calls {
            let pick = rng.next();
            let mut acc = 0.0;
            let m = models.iter().find(|m| {
                acc += m.3;
                pick < acc
            });
            let (source, provider, model, _, input, output) = *m.unwrap_or(&models[0]);
            let minutes = if day == 0 {
                (rng.next() * since_midnight.min(600) as f64) as i64
            } else {
                (rng.next() * 600.0) as i64 + 480
            };
            let ts = if day == 0 {
                now_ms - minutes * 60_000
            } else {
                (date - Duration::hours(date.format("%H").to_string().parse::<i64>()?)).timestamp_millis()
                    + minutes * 60_000
            };
            let fresh = (input * (0.3 + rng.next())) as i64;
            let out = (output * (0.3 + rng.next() * 1.4)) as i64;
            events.push(UsageEvent {
                key: format!("demo:{day}:{c}"),
                source: source.into(),
                provider: provider.into(),
                model: model.into(),
                project: Some(projects[(rng.next().powf(1.8) * projects.len() as f64) as usize].into()),
                session: Some(format!("s{day}-{}", c / 10)),
                ts,
                tokens: Tokens {
                    input: fresh,
                    output: out,
                    cache_read: if provider == "anthropic" { fresh * 9 } else { fresh * 3 },
                    cache_write: if provider == "anthropic" { fresh / 2 } else { 0 },
                    cache_write_1h: 0,
                    reasoning: out / 4,
                },
                reported_cost: None,
            });
        }
    }

    // Plan windows: a few readings each, climbing towards now.
    let mut snaps = Vec::new();
    for (meter, provider, minutes, used, left_min) in [
        ("claude", "anthropic", 300, 64.0, 110),
        ("claude", "anthropic", 10_080, 41.0, 3 * 24 * 60 + 200),
        ("codex", "openai", 300, 22.0, 190),
        ("codex", "openai", 10_080, 57.0, 2 * 24 * 60 + 600),
    ] {
        let resets_at = now.timestamp() + left_min * 60;
        let start_ms = (resets_at - minutes * 60) * 1000;
        let span = (now_ms - start_ms).max(1);
        for i in 1..=8 {
            let f = i as f64 / 8.0;
            snaps.push(LimitSnapshot {
                meter: meter.into(),
                provider: provider.into(),
                window_minutes: minutes,
                resets_at,
                used_pct: (used * f).round(),
                seen_at: start_ms + (span as f64 * f) as i64 - 1,
            });
        }
    }
    store.commit("demo", &Cursor::default(), &events, &snaps)?;

    for (provider, name, usd, day) in [("anthropic", "Claude Max", 100.0, 12), ("openai", "ChatGPT Plus", 20.0, 3)] {
        store.save_plan(&Plan {
            id: 0,
            provider: provider.into(),
            name: name.into(),
            monthly_usd: usd,
            renews_day: day,
        })?;
    }

    // Two provider accounts, as if synced.
    let today = Utc::now().date_naive();
    let first = today - chrono::Days::new(89);
    let mut usage = Vec::new();
    let mut costs = Vec::new();
    for d in 0..90u64 {
        let day = (first + chrono::Days::new(d)).to_string();
        let scale = 0.5 + rng.next();
        usage.push(UsageRow {
            day: day.clone(),
            model: "claude-sonnet-4-5".into(),
            tokens: Tokens {
                input: (180_000.0 * scale) as i64,
                output: (30_000.0 * scale) as i64,
                ..Tokens::default()
            },
            requests: Some((40.0 * scale) as i64),
        });
        costs.push(CostRow {
            day,
            item: "Messages".into(),
            model: Some("claude-sonnet-4-5".into()),
            usd: 0.99 * scale,
        });
    }
    let anthropic = store.add_account("anthropic", "anthropic", "Side project API", Some("x7Qe"))?;
    let batch = Batch { from: first.to_string(), to: today.to_string(), usage, costs, credit: None };
    store.save_account_batch(anthropic, &batch, now_ms - 6 * 60_000)?;
    let router = store.add_account("openrouter", "openrouter", "OpenRouter", Some("m2Lk"))?;
    let batch = Batch {
        from: today.to_string(),
        to: today.to_string(),
        usage: Vec::new(),
        costs: vec![CostRow { day: today.to_string(), item: "team-router-fast".into(), model: None, usd: 1.84 }],
        credit: Some((25.0, 13.6)),
    };
    store.save_account_batch(router, &batch, now_ms - 6 * 60_000)?;

    let catalog = Catalog::load(&store)?;
    let out = json!({
        "overview": {
            "7": overview(&store, &catalog, 7, now)?,
            "30": overview(&store, &catalog, 30, now)?,
            "90": overview(&store, &catalog, 90, now)?,
        },
        "limits": limits(&store, &catalog, now)?,
        "accounts": {
            "7": accounts::report(&store, &catalog, 7, Utc::now())?,
            "30": accounts::report(&store, &catalog, 30, Utc::now())?,
            "90": accounts::report(&store, &catalog, 90, Utc::now())?,
        },
        "price_models": catalog.model_ids(),
        "now": now_ms,
    });
    println!("{out}");
    Ok(())
}
