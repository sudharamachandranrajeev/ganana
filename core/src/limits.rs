//! Plan limits and plans: where each usage window stands, how fast it is
//! filling, which alerts are due, and what each plan's usage would cost at
//! API prices.

use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{DateTime, Datelike, Local, Months, NaiveDate};
use serde::Serialize;

use crate::db::Store;
use crate::model::{LimitSnapshot, Plan};
use crate::pricing::Catalog;
use crate::report::{Totals, start_of};

/// Readings of one window whose reset times are this close (seconds) belong
/// to the same window.
const SAME_WINDOW_S: i64 = 600;

pub const DEFAULT_THRESHOLDS: [i64; 3] = [75, 90, 100];

#[derive(Debug, Clone, Serialize)]
pub struct Window {
    pub meter: String,
    pub provider: String,
    pub window_minutes: i64,
    /// Latest reading; 0 once the window has reset.
    pub used_pct: f64,
    /// Unix seconds.
    pub resets_at: i64,
    /// Unix ms of the latest reading.
    pub seen_at: i64,
    /// The window reset after the latest reading, so it starts again at 0.
    pub reset_passed: bool,
    /// Percentage points used per hour recently; None without enough readings.
    pub rate_per_hour: Option<f64>,
    /// Unix seconds when the window would be full at that rate, if that is
    /// before it resets.
    pub full_at: Option<i64>,
    /// (unix ms, used %) readings in this window, oldest first.
    pub history: Vec<(i64, f64)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanValue {
    #[serde(flatten)]
    pub plan: Plan,
    /// The app whose usage this plan covers, such as `claude-code`.
    pub source: Option<String>,
    /// Current billing period, local dates YYYY-MM-DD; `end` is exclusive.
    pub period_start: String,
    pub period_end: String,
    /// Usage in the period, priced at API list prices.
    pub usage: Totals,
}

#[derive(Debug, Clone, Serialize)]
pub struct Limits {
    pub windows: Vec<Window>,
    pub plans: Vec<PlanValue>,
    pub thresholds: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Alert {
    pub meter: String,
    pub window_minutes: i64,
    pub threshold: i64,
    pub used_pct: f64,
    pub resets_at: i64,
}

pub fn limits(store: &Store, catalog: &Catalog, now: DateTime<Local>) -> Result<Limits> {
    let now_ms = now.timestamp_millis();
    let snapshots = store.limits_since(now_ms - 8 * 24 * 3_600_000)?;
    let plans = store.plans()?.into_iter().map(|p| plan_value(store, catalog, p, now)).collect::<Result<_>>()?;
    Ok(Limits { windows: windows(&snapshots, now_ms), plans, thresholds: thresholds(store)? })
}

/// The latest window per meter and length.
pub fn windows(snapshots: &[LimitSnapshot], now_ms: i64) -> Vec<Window> {
    let mut groups: BTreeMap<(&str, i64), Vec<&LimitSnapshot>> = BTreeMap::new();
    for s in snapshots {
        groups.entry((&s.meter, s.window_minutes)).or_default().push(s);
    }
    groups
        .into_values()
        .filter_map(|mut readings| {
            readings.sort_by_key(|s| s.seen_at);
            let latest = *readings.last()?;
            let current: Vec<(i64, f64)> = readings
                .iter()
                .filter(|s| (s.resets_at - latest.resets_at).abs() < SAME_WINDOW_S)
                .map(|s| (s.seen_at, s.used_pct))
                .collect();
            let reset_passed = latest.resets_at * 1000 <= now_ms;
            let rate = if reset_passed { None } else { rate_per_hour(&current, latest.window_minutes, now_ms) };
            let full_at = rate.filter(|r| *r > 0.0 && latest.used_pct < 100.0).and_then(|r| {
                let at = now_ms / 1000 + ((100.0 - latest.used_pct) / r * 3600.0) as i64;
                (at < latest.resets_at).then_some(at)
            });
            Some(Window {
                meter: latest.meter.clone(),
                provider: latest.provider.clone(),
                window_minutes: latest.window_minutes,
                used_pct: if reset_passed { 0.0 } else { latest.used_pct },
                resets_at: latest.resets_at,
                seen_at: latest.seen_at,
                reset_passed,
                rate_per_hour: rate,
                full_at,
                history: if reset_passed { Vec::new() } else { current },
            })
        })
        .collect()
}

/// Growth over the recent part of the window (a fifth of it, at least 30
/// minutes), measured up to now so the rate falls while usage pauses.
fn rate_per_hour(history: &[(i64, f64)], window_minutes: i64, now_ms: i64) -> Option<f64> {
    let lookback_ms = (window_minutes * 60_000 / 5).max(30 * 60_000);
    let &(_, latest) = history.last()?;
    let &(start_ms, start) = history.iter().find(|(t, _)| *t >= now_ms - lookback_ms)?;
    let hours = (now_ms - start_ms) as f64 / 3_600_000.0;
    (hours >= 5.0 / 60.0).then(|| ((latest - start) / hours).max(0.0))
}

/// Alerts newly due: per window, the highest threshold reached that was not
/// announced before. Lower thresholds reached at the same time are marked
/// announced too, so starting the app at 95% gives one alert, not three.
pub fn new_alerts(store: &Store, windows: &[Window], thresholds: &[i64]) -> Result<Vec<Alert>> {
    let mut alerts = Vec::new();
    for w in windows.iter().filter(|w| !w.reset_passed) {
        let mut due = None;
        for &t in thresholds.iter().filter(|&&t| w.used_pct >= t as f64) {
            if store.claim_alert(&w.meter, w.window_minutes, w.resets_at, t)? {
                due = due.max(Some(t));
            }
        }
        if let Some(threshold) = due {
            alerts.push(Alert {
                meter: w.meter.clone(),
                window_minutes: w.window_minutes,
                threshold,
                used_pct: w.used_pct,
                resets_at: w.resets_at,
            });
        }
    }
    Ok(alerts)
}

pub fn thresholds(store: &Store) -> Result<Vec<i64>> {
    let saved = store.setting("alert_thresholds")?.and_then(|s| serde_json::from_str::<Vec<i64>>(&s).ok());
    Ok(saved.unwrap_or_else(|| DEFAULT_THRESHOLDS.to_vec()))
}

pub fn set_thresholds(store: &Store, thresholds: &[i64]) -> Result<()> {
    let mut t: Vec<i64> = thresholds.iter().copied().filter(|t| (1..=100).contains(t)).collect();
    t.sort_unstable();
    t.dedup();
    store.set_setting("alert_thresholds", &serde_json::to_string(&t)?)
}

/// The local app whose usage a plan from `provider` pays for.
fn plan_source(provider: &str) -> Option<&'static str> {
    match provider {
        "anthropic" => Some("claude-code"),
        "openai" => Some("codex"),
        "google" => Some("gemini-cli"),
        "alibaba" => Some("qwen-code"),
        _ => None,
    }
}

fn plan_value(store: &Store, catalog: &Catalog, plan: Plan, now: DateTime<Local>) -> Result<PlanValue> {
    let (start, end) = billing_period(now.date_naive(), plan.renews_day);
    let source = plan_source(&plan.provider);
    let mut usage = Totals::default();
    if let Some(source) = source {
        for e in store.events_since(start_of(start))?.iter().filter(|e| e.source == source) {
            usage.add(e, catalog.cost(e), catalog.cache_savings(e));
        }
    }
    Ok(PlanValue {
        plan,
        source: source.map(str::to_string),
        period_start: start.format("%Y-%m-%d").to_string(),
        period_end: end.format("%Y-%m-%d").to_string(),
        usage,
    })
}

/// The billing period containing `today` for a plan renewing on `day` of
/// each month. In shorter months the renewal falls on the last day.
pub fn billing_period(today: NaiveDate, day: u32) -> (NaiveDate, NaiveDate) {
    let in_month = |d: NaiveDate| {
        let first = d.with_day(1).expect("day 1 exists");
        let last = (first + Months::new(1)).pred_opt().expect("month has a last day");
        first.with_day(day.clamp(1, last.day())).expect("clamped day exists")
    };
    let this = in_month(today);
    let start = if this <= today { this } else { in_month(today - Months::new(1)) };
    (start, in_month(start + Months::new(1)))
}
