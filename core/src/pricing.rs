//! Model prices from models.dev (MIT), in USD per million tokens.
//!
//! A trimmed snapshot ships in the binary; the app refreshes it at most
//! daily and keeps the download in the store. Costs are computed when a
//! report is built, so a price fix or a new alias applies to all history.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Store;
use crate::model::UsageEvent;

pub const SOURCE_URL: &str = "https://models.dev/api.json";

/// Providers kept from the catalog, in lookup order for models whose
/// provider is unknown.
const PROVIDERS: &[&str] = &[
    "anthropic", "openai", "google", "alibaba", "deepseek", "xai", "mistral", "moonshotai", "zai",
];

const BUNDLED: &str = include_str!("../data/prices.json");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    pub cache_read: Option<f64>,
    pub cache_write: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Price {
    #[serde(flatten)]
    pub base: Rates,
    /// Higher rates once a prompt passes `tier_size` tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<Rates>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier_size: Option<i64>,
}

type Table = BTreeMap<String, BTreeMap<String, Price>>;

#[derive(Debug, Clone)]
pub struct Catalog {
    table: Table,
    /// Raw model name -> "provider/model".
    aliases: HashMap<String, String>,
}

/// Resolved price for one event's model.
pub struct Match<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub price: &'a Price,
}

impl Catalog {
    pub fn bundled() -> Self {
        Self::from_json(BUNDLED).expect("bundled price snapshot is valid")
    }

    pub fn from_json(json: &str) -> Result<Self> {
        Ok(Self { table: serde_json::from_str(json)?, aliases: HashMap::new() })
    }

    /// The newest catalog in the store (else the bundled one), with the
    /// user's aliases.
    pub fn load(store: &Store) -> Result<Self> {
        let mut catalog = match store.catalog_json()? {
            Some((_, json)) => Self::from_json(&json).unwrap_or_else(|_| Self::bundled()),
            None => Self::bundled(),
        };
        catalog.aliases = store.aliases()?;
        Ok(catalog)
    }

    /// Downloads models.dev when the stored copy is older than `max_age`.
    /// Returns true when a new catalog was saved.
    pub fn refresh_if_stale(store: &Store, max_age: Duration) -> Result<bool> {
        let now = chrono::Utc::now().timestamp_millis();
        if let Some((fetched, _)) = store.catalog_json()?
            && now - fetched < max_age.as_millis() as i64
        {
            return Ok(false);
        }
        let agent: ureq::Agent =
            ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).build().into();
        let body = agent
            .get(SOURCE_URL)
            .call()
            .context("download models.dev prices")?
            .body_mut()
            .with_config()
            .limit(64 << 20)
            .read_to_string()?;
        let trimmed = trim(&body)?;
        Self::from_json(&trimmed)?; // never save something we cannot read back
        store.save_catalog_json(now, &trimmed)?;
        Ok(true)
    }

    /// Every priced model as "provider/model", for the alias picker.
    pub fn model_ids(&self) -> Vec<String> {
        self.table.iter().flat_map(|(p, models)| models.keys().map(move |m| format!("{p}/{m}"))).collect()
    }

    pub fn lookup(&self, provider: &str, model: &str) -> Option<Match<'_>> {
        if let Some(target) = self.aliases.get(model) {
            let (p, m) = target.split_once('/')?;
            return self.exact(p, m);
        }
        let name = normalise(model);
        let candidates = [model.to_string(), name.clone(), strip_date(&name).to_string()];
        let providers = std::iter::once(provider).chain(PROVIDERS.iter().copied().filter(|p| *p != provider));
        for p in providers {
            for m in &candidates {
                if let Some(found) = self.exact(p, m) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn exact(&self, provider: &str, model: &str) -> Option<Match<'_>> {
        let (provider, models) = self.table.get_key_value(provider)?;
        let (model, price) = models.get_key_value(model)?;
        Some(Match { provider, model, price })
    }

    /// Cost of one event in USD at list price, or `None` when the model has
    /// no known price. A cost the tool reported itself wins.
    pub fn cost(&self, e: &UsageEvent) -> Option<f64> {
        if let Some(c) = e.reported_cost {
            return Some(c);
        }
        let price = self.lookup(&e.provider, &e.model)?.price;
        Some(cost_at(price, e))
    }

    /// What cache reads saved compared with paying full input price.
    pub fn cache_savings(&self, e: &UsageEvent) -> f64 {
        let Some(m) = self.lookup(&e.provider, &e.model) else { return 0.0 };
        let r = rates_for(m.price, e);
        let read = r.cache_read.unwrap_or(r.input);
        e.tokens.cache_read as f64 * (r.input - read).max(0.0) / 1e6
    }
}

fn rates_for<'a>(price: &'a Price, e: &UsageEvent) -> &'a Rates {
    match (&price.tier, price.tier_size) {
        (Some(tier), Some(size)) if e.tokens.prompt() > size => tier,
        _ => &price.base,
    }
}

fn cost_at(price: &Price, e: &UsageEvent) -> f64 {
    let r = rates_for(price, e);
    let t = &e.tokens;
    // Anthropic bills 1-hour cache writes at 2x input (5-minute ones at 1.25x).
    let write_1h = 2.0 * r.input;
    (t.input as f64 * r.input
        + t.output as f64 * r.output
        + t.cache_read as f64 * r.cache_read.unwrap_or(r.input)
        + t.cache_write as f64 * r.cache_write.unwrap_or(r.input)
        + t.cache_write_1h as f64 * write_1h)
        / 1e6
}

/// Lower case, without a context-size suffix like "[1m]" or a vendor prefix
/// like "anthropic/".
fn normalise(model: &str) -> String {
    let m = model.to_ascii_lowercase();
    let m = m.split('[').next().unwrap_or(&m).trim();
    m.rsplit('/').next().unwrap_or(m).to_string()
}

/// Drops a trailing release date: "claude-sonnet-4-5-20250929" -> "claude-sonnet-4-5".
fn strip_date(model: &str) -> &str {
    match model.rsplit_once('-') {
        Some((head, date)) if date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => model,
    }
}

/// Reduces the full models.dev `api.json` to the providers and price fields
/// Gaṇanā uses.
pub fn trim(api_json: &str) -> Result<String> {
    let all: Value = serde_json::from_str(api_json)?;
    let rates = |c: &Value| -> Option<Rates> {
        Some(Rates {
            input: c["input"].as_f64()?,
            output: c["output"].as_f64()?,
            cache_read: c["cache_read"].as_f64(),
            cache_write: c["cache_write"].as_f64(),
        })
    };
    let mut table = Table::new();
    for p in PROVIDERS {
        let Some(models) = all[p]["models"].as_object() else { continue };
        let priced = models
            .iter()
            .filter_map(|(id, m)| {
                let cost = &m["cost"];
                let base = rates(cost)?;
                let (tier, tier_size) = match cost["tiers"].get(0) {
                    Some(t) => (rates(t), t["tier"]["size"].as_i64()),
                    None => (rates(&cost["context_over_200k"]), Some(200_000)),
                };
                let tier_size = tier.as_ref().and(tier_size);
                Some((id.clone(), Price { base, tier, tier_size }))
            })
            .collect::<BTreeMap<_, _>>();
        if !priced.is_empty() {
            table.insert(p.to_string(), priced);
        }
    }
    anyhow::ensure!(!table.is_empty(), "no prices found in catalog");
    Ok(serde_json::to_string(&table)?)
}
