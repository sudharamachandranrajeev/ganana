//! Usage and billing from provider accounts: the Anthropic and OpenAI admin
//! APIs, OpenRouter, and CSV exports from anywhere else.
//!
//! Account data is kept apart from local log events. An account report
//! often covers calls the local logs already counted (a tool running on an
//! API key), while local logs also hold subscription use that no API bill
//! covers, so the two are shown side by side and never summed.
//!
//! API keys never reach the store: the app keeps them in the system keyring
//! and passes one in for each sync.

mod anthropic;
pub mod csv_import;
mod openai;
mod openrouter;

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Days, NaiveDate, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::db::Store;
use crate::model::Tokens;
use crate::pricing::Catalog;
use crate::report::{Day, Slice, Totals};

/// Account kinds synced through an API, with the provider they bill for.
pub const API_KINDS: &[(&str, &str)] =
    &[("anthropic", "anthropic"), ("openai", "openai"), ("openrouter", "openrouter")];

/// Kind of an account filled from CSV files.
pub const CSV: &str = "csv";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Account {
    pub id: i64,
    /// anthropic, openai, openrouter or csv.
    pub kind: String,
    /// Who serves the models, as in `UsageEvent::provider`: fixed by the
    /// kind, or chosen for a CSV account.
    pub provider: String,
    pub name: String,
    /// Last four characters of the key, so the user can tell keys apart.
    pub key_hint: Option<String>,
    /// Unix ms of the last successful sync or import.
    pub synced_at: Option<i64>,
    pub error: Option<String>,
    /// OpenRouter prepaid credit in USD: bought, and spent.
    pub credit_total: Option<f64>,
    pub credit_used: Option<f64>,
}

impl Account {
    /// Prepaid credit left, when it is low: under a tenth of what was bought,
    /// or under a dollar.
    pub fn low_credit(&self) -> Option<f64> {
        let (total, used) = (self.credit_total?, self.credit_used?);
        let left = total - used;
        (total > 0.0 && left < (total * 0.1).max(1.0)).then_some(left.max(0.0))
    }
}

/// Usage of one model on one UTC day.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRow {
    pub day: String,
    pub model: String,
    pub tokens: Tokens,
    pub requests: Option<i64>,
}

/// One billed line on one UTC day.
#[derive(Debug, Clone, PartialEq)]
pub struct CostRow {
    pub day: String,
    pub item: String,
    pub model: Option<String>,
    pub usd: f64,
}

/// What one sync or import read. It replaces the account's rows for the
/// UTC days `from` to `to` (YYYY-MM-DD, inclusive).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Batch {
    pub from: String,
    pub to: String,
    pub usage: Vec<UsageRow>,
    pub costs: Vec<CostRow>,
    /// OpenRouter credit: (bought, spent) in USD.
    pub credit: Option<(f64, f64)>,
}

impl Batch {
    fn new(from: NaiveDate, to: NaiveDate) -> Self {
        Self { from: from.to_string(), to: to.to_string(), ..Self::default() }
    }
}

/// An HTTP error status with the provider's message.
#[derive(Debug)]
pub struct HttpStatus {
    pub status: u16,
    pub message: String,
}

impl std::fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {}: {}", self.status, self.message)
    }
}

impl std::error::Error for HttpStatus {}

/// GETs a URL and returns its JSON. Fetchers take this as a closure so
/// tests can feed recorded responses.
pub type Get<'a> = dyn FnMut(&str) -> Result<Value> + 'a;

fn http_get(agent: &ureq::Agent, url: &str, headers: &[(&str, &str)]) -> Result<Value> {
    let mut request = agent.get(url).header("User-Agent", concat!("Ganana/", env!("CARGO_PKG_VERSION")));
    for (k, v) in headers {
        request = request.header(*k, *v);
    }
    let host = url.split('/').nth(2).unwrap_or(url);
    let mut response = request.call().with_context(|| format!("could not reach {host}"))?;
    let status = response.status().as_u16();
    let body = response.body_mut().with_config().limit(32 << 20).read_to_string()?;
    if status >= 400 {
        let json: Option<Value> = serde_json::from_str(&body).ok();
        let message = json
            .as_ref()
            .and_then(|j| j["error"]["message"].as_str().or(j["error"].as_str()).or(j["message"].as_str()))
            .map_or_else(|| body.chars().take(200).collect(), str::to_string);
        return Err(HttpStatus { status, message }.into());
    }
    serde_json::from_str(&body).with_context(|| format!("{host} sent a response that is not JSON"))
}

/// Follows `next_page` cursors (Anthropic and OpenAI style) and returns
/// every bucket in `data`.
fn pages(get: &mut Get, url: &str) -> Result<Vec<Value>> {
    let mut buckets = Vec::new();
    let mut page: Option<String> = None;
    for _ in 0..50 {
        let next = match &page {
            Some(p) => format!("{url}&page={}", encode(p)),
            None => url.to_string(),
        };
        let response = get(&next)?;
        let Some(data) = response["data"].as_array() else { bail!("the response has no data list") };
        buckets.extend(data.iter().cloned());
        match (response["has_more"].as_bool(), response["next_page"].as_str()) {
            (Some(true), Some(cursor)) => page = Some(cursor.to_string()),
            _ => return Ok(buckets),
        }
    }
    bail!("the report did not end after 50 pages")
}

/// Percent-encodes a query value.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn int(v: &Value) -> i64 {
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)).unwrap_or(0)
}

/// The UTC day of an RFC 3339 time.
fn day_of(v: &Value) -> Result<String> {
    let s = v.as_str().context("a bucket has no start time")?;
    Ok(DateTime::parse_from_rfc3339(s)
        .with_context(|| format!("bad time {s}"))?
        .with_timezone(&Utc)
        .date_naive()
        .to_string())
}

/// Reads an account's recent usage and billing through `get`. The first
/// sync reaches back 90 days; later ones read the last 31 again, because
/// providers fill in the latest days late.
pub fn fetch(kind: &str, get: &mut Get, first: bool, now: DateTime<Utc>) -> Result<Batch> {
    let today = now.date_naive();
    let from = today - Days::new(if first { 89 } else { 30 });
    match kind {
        "anthropic" => anthropic::fetch(get, from, today),
        "openai" => openai::fetch(get, from, today),
        "openrouter" => openrouter::fetch(get, today),
        other => bail!("{other} accounts are filled by importing files, not synced"),
    }
}

/// Syncs one account with `key` and stores the result, or the error.
pub fn sync(store: &Store, account: &Account, key: &str, now: DateTime<Utc>) -> Result<()> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .http_status_as_error(false)
        .build()
        .into();
    let bearer = format!("Bearer {key}");
    let headers: &[(&str, &str)] = match account.kind.as_str() {
        "anthropic" => &[("x-api-key", key), ("anthropic-version", "2023-06-01")],
        _ => &[("Authorization", &bearer)],
    };
    let result = fetch(&account.kind, &mut |url| http_get(&agent, url, headers), account.synced_at.is_none(), now);
    match result {
        Ok(batch) => store.save_account_batch(account.id, &batch, now.timestamp_millis()),
        Err(e) => {
            let e = explain(&account.kind, e);
            store.set_account_error(account.id, Some(&format!("{e:#}")))?;
            Err(e)
        }
    }
}

/// Adds a plain hint to a refused key.
fn explain(kind: &str, e: anyhow::Error) -> anyhow::Error {
    let Some(status) = e.downcast_ref::<HttpStatus>().map(|s| s.status) else { return e };
    let hint = match (kind, status) {
        ("anthropic", 401 | 403) => {
            "The key was refused. Usage reports need an Admin key (it starts with sk-ant-admin), \
             which only an organization admin can create in the Claude Console."
        }
        ("openai", 401 | 403) => {
            "The key was refused. Usage reports need an Admin key (it starts with sk-admin-), \
             which only an organization owner can create."
        }
        ("openrouter", 401 | 403) => {
            "The key was refused. Activity and credits need a management key, created under \
             Management keys on openrouter.ai."
        }
        (_, 429) => "The provider asked to slow down. Gaṇanā will try again later.",
        _ => return e,
    };
    e.context(hint)
}

/// One account with its totals over the report's days.
#[derive(Debug, Clone, Serialize)]
pub struct AccountView {
    #[serde(flatten)]
    pub account: Account,
    /// `cost` is the billed amount; `calls` counts requests where the
    /// provider reports them.
    pub totals: Totals,
    pub models: Vec<Slice>,
    /// Billed lines, biggest first.
    pub items: Vec<Slice>,
    /// List-price estimate from the local logs for the same provider and
    /// days, for comparison.
    pub local_cost: f64,
    pub local_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountsReport {
    pub days: u32,
    /// Billed cost per UTC day, split by provider; oldest first.
    pub daily: Vec<Day>,
    pub accounts: Vec<AccountView>,
    pub billed: f64,
}

/// Account totals for the last `days` UTC days, today included.
pub fn report(store: &Store, catalog: &Catalog, days: u32, now: DateTime<Utc>) -> Result<AccountsReport> {
    let days = days.clamp(1, 366);
    let today = now.date_naive();
    let first = today - Days::new(u64::from(days - 1));
    let since = first.to_string();
    let accounts = store.accounts()?;
    let provider_of: HashMap<i64, &str> = accounts.iter().map(|a| (a.id, a.provider.as_str())).collect();

    let mut daily: Vec<Day> = (0..days)
        .map(|i| Day {
            date: (first + Days::new(u64::from(i))).to_string(),
            cost: 0.0,
            total_tokens: 0,
            cost_by_provider: BTreeMap::new(),
            tokens_by_provider: BTreeMap::new(),
        })
        .collect();
    let index = |day: &str| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok().map(|d| (d - first).num_days() as usize);

    let mut totals: HashMap<i64, Totals> = HashMap::new();
    let mut models: HashMap<(i64, String), Totals> = HashMap::new();
    let mut items: HashMap<(i64, String), Totals> = HashMap::new();
    for (id, u) in store.account_usage(&since)? {
        let Some(provider) = provider_of.get(&id) else { continue };
        let tokens = u.tokens.total();
        for t in [totals.entry(id).or_default(), models.entry((id, u.model.clone())).or_default()] {
            t.tokens.add(&u.tokens);
            t.total_tokens += tokens;
            t.calls += u.requests.unwrap_or(0);
        }
        if let Some(day) = index(&u.day).and_then(|i| daily.get_mut(i)) {
            day.total_tokens += tokens;
            *day.tokens_by_provider.entry(provider.to_string()).or_default() += tokens;
        }
    }
    for (id, c) in store.account_costs(&since)? {
        let Some(provider) = provider_of.get(&id) else { continue };
        totals.entry(id).or_default().cost += c.usd;
        items.entry((id, c.item.clone())).or_default().cost += c.usd;
        if let Some(model) = &c.model {
            models.entry((id, model.clone())).or_default().cost += c.usd;
        }
        if let Some(day) = index(&c.day).and_then(|i| daily.get_mut(i)) {
            day.cost += c.usd;
            *day.cost_by_provider.entry(provider.to_string()).or_default() += c.usd;
        }
    }

    // Local-log estimate per provider over the same UTC days.
    let start_ms = first.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc().timestamp_millis();
    let mut local: HashMap<String, (f64, i64)> = HashMap::new();
    for e in store.events_since(start_ms)? {
        if provider_of.values().any(|p| *p == e.provider) {
            let entry = local.entry(e.provider.clone()).or_default();
            entry.0 += catalog.cost(&e).unwrap_or(0.0);
            entry.1 += e.tokens.total();
        }
    }

    let billed = totals.values().map(|t| t.cost).sum();
    let views = accounts
        .into_iter()
        .map(|account| {
            let id = account.id;
            let slices = |map: &HashMap<(i64, String), Totals>| {
                let mut v: Vec<Slice> = map
                    .iter()
                    .filter(|((a, _), _)| *a == id)
                    .map(|((_, name), t)| Slice { name: name.clone(), provider: None, totals: t.clone() })
                    .collect();
                v.sort_by(|a, b| {
                    b.totals.cost.total_cmp(&a.totals.cost).then(b.totals.total_tokens.cmp(&a.totals.total_tokens))
                });
                v
            };
            let (local_cost, local_tokens) = local.get(&account.provider).copied().unwrap_or_default();
            AccountView {
                totals: totals.remove(&id).unwrap_or_default(),
                models: slices(&models),
                items: slices(&items),
                local_cost,
                local_tokens,
                account,
            }
        })
        .collect();
    Ok(AccountsReport { days, daily, accounts: views, billed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refused_keys_get_a_plain_hint() {
        let refused = |kind| {
            format!("{:#}", explain(kind, HttpStatus { status: 401, message: "invalid x-api-key".into() }.into()))
        };
        assert!(
            refused("anthropic")
                .starts_with("The key was refused. Usage reports need an Admin key (it starts with sk-ant-admin)")
        );
        assert!(refused("anthropic").ends_with("HTTP 401: invalid x-api-key"), "the provider's words stay");
        assert!(refused("openai").contains("sk-admin-"));
        assert!(refused("openrouter").contains("management key"));
        let other = explain("openai", HttpStatus { status: 500, message: "boom".into() }.into());
        assert_eq!(format!("{other:#}"), "HTTP 500: boom");
        assert_eq!(format!("{:#}", explain("openai", anyhow::anyhow!("offline"))), "offline");
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(encode("page_Mj=+/x y"), "page_Mj%3D%2B%2Fx%20y");
    }
}
