//! A small summary for status bars: the fullest plan window, today's cost
//! and low credit, with a plain-language tooltip. `ganana bar` prints it.

use anyhow::Result;
use chrono::{DateTime, Local};
use serde::Serialize;

use crate::db::Store;
use crate::limits::{self, Window, clock, duration, meter_name, window_name};
use crate::pricing::Catalog;
use crate::report::start_of;

/// Used share at which a window counts as nearly full.
const CRITICAL_PCT: f64 = 90.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// No live plan window to show.
    Idle,
    Ok,
    /// Past the first alert level, or filling before it resets.
    Warning,
    /// Nearly or completely full.
    Critical,
}

#[derive(Debug, Clone, Serialize)]
pub struct BarWindow {
    pub meter: String,
    /// "Claude 5-hour".
    pub name: String,
    pub used_pct: f64,
    /// Unix seconds.
    pub resets_at: i64,
    /// Unix seconds when it fills at the recent pace, if before the reset.
    pub full_at: Option<i64>,
    pub level: Level,
    /// One plain sentence about this window.
    pub line: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LowCredit {
    pub name: String,
    pub left: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Bar {
    /// The most urgent window's level.
    pub level: Level,
    /// Short text for the bar: "Claude 62%", or today's cost.
    pub text: String,
    /// Plain sentences, one per line.
    pub tooltip: String,
    /// The fullest live window, 0 to 100.
    pub percentage: Option<u8>,
    /// Live windows, fullest first.
    pub windows: Vec<BarWindow>,
    pub today_cost: f64,
    pub today_tokens: i64,
    /// Today's tokens from models without a known price, left out of the cost.
    pub today_unpriced_tokens: i64,
    pub low_credit: Vec<LowCredit>,
}

pub fn bar(store: &Store, catalog: &Catalog, now: DateTime<Local>) -> Result<Bar> {
    let now_ms = now.timestamp_millis();
    let warn_at = limits::thresholds(store)?.first().copied().unwrap_or(75) as f64;
    let mut windows: Vec<BarWindow> = limits::windows(&store.limits_since(now_ms - 8 * 24 * 3_600_000)?, now_ms)
        .iter()
        .filter(|w| !w.reset_passed)
        .map(|w| bar_window(w, warn_at, now))
        .collect();
    windows.sort_by(|a, b| b.level.cmp(&a.level).then(b.used_pct.total_cmp(&a.used_pct)));

    let (mut today_cost, mut today_tokens, mut today_unpriced_tokens) = (0.0, 0, 0);
    for e in store.events_since(start_of(now.date_naive()))? {
        match catalog.cost(&e) {
            Some(cost) => today_cost += cost,
            None => today_unpriced_tokens += e.tokens.total(),
        }
        today_tokens += e.tokens.total();
    }
    let low_credit: Vec<LowCredit> = store
        .accounts()?
        .into_iter()
        .filter_map(|a| a.low_credit().map(|left| LowCredit { name: a.name, left }))
        .collect();

    let top = windows.first();
    let money = |usd: f64| format!("${usd:.2}");
    let text = top.map_or_else(
        || format!("{} today", money(today_cost)),
        |w| format!("{} {:.0}%", meter_name(&w.meter).split(' ').next().unwrap_or_default(), w.used_pct),
    );
    let mut lines: Vec<String> = windows.iter().map(|w| w.line.clone()).collect();
    if lines.is_empty() {
        lines.push("No plan limits seen recently.".into());
    }
    let mut today = format!("Today: {} of AI use at API prices, {} tokens", money(today_cost), compact(today_tokens));
    if today_unpriced_tokens > 0 {
        today.push_str(&format!(" ({} from models without a price, not in the cost)", compact(today_unpriced_tokens)));
    }
    lines.push(today + ".");
    for c in &low_credit {
        lines.push(format!("{} credit is low: {} left.", c.name, money(c.left)));
    }
    Ok(Bar {
        level: top.map_or(Level::Idle, |w| w.level),
        percentage: top.map(|w| w.used_pct.clamp(0.0, 100.0).round() as u8),
        text,
        tooltip: lines.join("\n"),
        windows,
        today_cost,
        today_tokens,
        today_unpriced_tokens,
        low_credit,
    })
}

fn bar_window(w: &Window, warn_at: f64, now: DateTime<Local>) -> BarWindow {
    let level = if w.used_pct >= CRITICAL_PCT {
        Level::Critical
    } else if w.used_pct >= warn_at || w.full_at.is_some() {
        Level::Warning
    } else {
        Level::Ok
    };
    let name = format!("{} {}", meter_name(&w.meter), window_name(w.window_minutes));
    let resets = format!("resets {} (in {})", clock(w.resets_at, now), duration(w.resets_at - now.timestamp()));
    let mut line = if w.used_pct >= 100.0 {
        format!("{name}: full, {resets}.")
    } else {
        format!("{name}: {:.0}% used, {resets}.", w.used_pct)
    };
    if let Some(full) = w.full_at {
        line.push_str(&format!(" At this pace it fills around {}.", clock(full, now)));
    }
    BarWindow {
        meter: w.meter.clone(),
        name,
        used_pct: w.used_pct,
        resets_at: w.resets_at,
        full_at: w.full_at,
        level,
        line,
    }
}

/// 950, 12.4K, 3.1M.
fn compact(n: i64) -> String {
    let n_f = n as f64;
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}K", n_f / 1e3)
    } else if n < 1_000_000_000 {
        format!("{:.1}M", n_f / 1e6)
    } else {
        format!("{:.1}B", n_f / 1e9)
    }
}

/// The bar as JSON for Waybar's `custom` module.
pub fn waybar(b: &Bar) -> serde_json::Value {
    serde_json::json!({
        "text": b.text,
        "tooltip": b.tooltip,
        "class": b.level,
        "alt": b.level,
        "percentage": b.percentage.unwrap_or(0),
    })
}
