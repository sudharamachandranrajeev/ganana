//! Reports for the dashboard, computed from raw events on demand.

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use chrono::{DateTime, Days, Local, NaiveDate, TimeZone};
use serde::Serialize;

use crate::db::Store;
use crate::model::{Tokens, UsageEvent};
use crate::pricing::Catalog;

#[derive(Debug, Default, Clone, Serialize)]
pub struct Totals {
    pub tokens: Tokens,
    pub total_tokens: i64,
    /// List-price cost of the calls whose model has a known price.
    pub cost: f64,
    /// Money cache reads saved compared with full input price.
    pub savings: f64,
    pub calls: i64,
    /// Tokens from models without a known price (not in `cost`).
    pub unpriced_tokens: i64,
}

impl Totals {
    fn add(&mut self, e: &UsageEvent, cost: Option<f64>, savings: f64) {
        self.tokens.add(&e.tokens);
        self.total_tokens += e.tokens.total();
        self.calls += 1;
        self.savings += savings;
        match cost {
            Some(c) => self.cost += c,
            None => self.unpriced_tokens += e.tokens.total(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Day {
    /// Local date, YYYY-MM-DD.
    pub date: String,
    pub cost: f64,
    pub total_tokens: i64,
    pub cost_by_provider: BTreeMap<String, f64>,
    pub tokens_by_provider: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Slice {
    pub name: String,
    /// Set for model slices.
    pub provider: Option<String>,
    #[serde(flatten)]
    pub totals: Totals,
}

#[derive(Debug, Clone, Serialize)]
pub struct Unpriced {
    pub provider: String,
    pub model: String,
    pub total_tokens: i64,
    pub calls: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub generated_at: i64,
    pub days: u32,
    pub today: Totals,
    pub week: Totals,
    /// The whole `days` window.
    pub period: Totals,
    /// One entry per day in the window, oldest first, empty days included.
    pub daily: Vec<Day>,
    pub providers: Vec<Slice>,
    pub models: Vec<Slice>,
    pub projects: Vec<Slice>,
    pub sources: Vec<Slice>,
    pub unpriced: Vec<Unpriced>,
    pub events_stored: i64,
}

/// Builds the overview for the last `days` local days, today included.
pub fn overview(store: &Store, catalog: &Catalog, days: u32, now: DateTime<Local>) -> Result<Overview> {
    let days = days.clamp(1, 366);
    let today = now.date_naive();
    let first_day = today - Days::new(u64::from(days - 1));
    let week_start = start_of(today - Days::new(6));
    let today_start = start_of(today);
    let since = start_of(first_day).min(week_start);

    let mut daily: Vec<Day> = (0..days)
        .map(|i| Day {
            date: (first_day + Days::new(u64::from(i))).format("%Y-%m-%d").to_string(),
            cost: 0.0,
            total_tokens: 0,
            cost_by_provider: BTreeMap::new(),
            tokens_by_provider: BTreeMap::new(),
        })
        .collect();
    let (mut today_t, mut week_t, mut period_t) = (Totals::default(), Totals::default(), Totals::default());
    let mut providers: HashMap<String, Totals> = HashMap::new();
    let mut models: HashMap<(String, String), Totals> = HashMap::new();
    let mut projects: HashMap<String, Totals> = HashMap::new();
    let mut sources: HashMap<String, Totals> = HashMap::new();

    for e in store.events_since(since)? {
        let cost = catalog.cost(&e);
        let savings = catalog.cache_savings(&e);
        if e.ts >= today_start {
            today_t.add(&e, cost, savings);
        }
        if e.ts >= week_start {
            week_t.add(&e, cost, savings);
        }
        let Some(date) = Local.timestamp_millis_opt(e.ts).single().map(|t| t.date_naive()) else { continue };
        if date < first_day || date > today {
            continue;
        }
        period_t.add(&e, cost, savings);
        let day = &mut daily[(date - first_day).num_days() as usize];
        day.cost += cost.unwrap_or(0.0);
        day.total_tokens += e.tokens.total();
        *day.cost_by_provider.entry(e.provider.clone()).or_default() += cost.unwrap_or(0.0);
        *day.tokens_by_provider.entry(e.provider.clone()).or_default() += e.tokens.total();
        providers.entry(e.provider.clone()).or_default().add(&e, cost, savings);
        models.entry((e.provider.clone(), e.model.clone())).or_default().add(&e, cost, savings);
        projects.entry(e.project.clone().unwrap_or_else(|| "(no folder)".into())).or_default().add(&e, cost, savings);
        sources.entry(e.source.clone()).or_default().add(&e, cost, savings);
    }

    let mut unpriced: Vec<Unpriced> = models
        .iter()
        .filter(|(_, t)| t.unpriced_tokens > 0)
        .map(|((p, m), t)| Unpriced { provider: p.clone(), model: m.clone(), total_tokens: t.unpriced_tokens, calls: t.calls })
        .collect();
    unpriced.sort_by_key(|u| std::cmp::Reverse(u.total_tokens));

    Ok(Overview {
        generated_at: now.timestamp_millis(),
        days,
        today: today_t,
        week: week_t,
        period: period_t,
        daily,
        providers: ranked(providers.into_iter().map(|(n, t)| slice(n, None, t))),
        models: ranked(models.into_iter().map(|((p, m), t)| slice(m, Some(p), t))),
        projects: ranked(projects.into_iter().map(|(n, t)| slice(n, None, t))),
        sources: ranked(sources.into_iter().map(|(n, t)| slice(n, None, t))),
        unpriced,
        events_stored: store.event_count()?,
    })
}

fn slice(name: String, provider: Option<String>, totals: Totals) -> Slice {
    Slice { name, provider, totals }
}

/// Biggest first: by cost, then by tokens.
fn ranked(slices: impl Iterator<Item = Slice>) -> Vec<Slice> {
    let mut v: Vec<Slice> = slices.collect();
    v.sort_by(|a, b| {
        b.totals.cost.total_cmp(&a.totals.cost).then(b.totals.total_tokens.cmp(&a.totals.total_tokens))
    });
    v
}

fn start_of(date: NaiveDate) -> i64 {
    let midnight = date.and_hms_opt(0, 0, 0).expect("midnight exists");
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map_or_else(|| midnight.and_utc().timestamp_millis(), |t| t.timestamp_millis())
}
