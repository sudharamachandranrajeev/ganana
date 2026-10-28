use ganana_core::pricing::Catalog;
use ganana_core::{Tokens, UsageEvent};

const TABLE: &str = r#"{
  "anthropic": {"claude-opus-9": {"input": 5, "output": 25, "cache_read": 0.5, "cache_write": 6.25}},
  "openai": {"gpt-9": {"input": 2, "output": 10, "cache_read": 0.2,
                       "tier": {"input": 4, "output": 15, "cache_read": 0.4, "cache_write": null}, "tier_size": 1000}}
}"#;

fn event(provider: &str, model: &str, tokens: Tokens) -> UsageEvent {
    UsageEvent {
        key: "k".into(),
        source: "test".into(),
        provider: provider.into(),
        model: model.into(),
        project: None,
        session: None,
        ts: 0,
        tokens,
        reported_cost: None,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn prices_every_token_kind() {
    let c = Catalog::from_json(TABLE).unwrap();
    let t = Tokens {
        input: 1_000_000,
        output: 1_000_000,
        cache_read: 1_000_000,
        cache_write: 1_000_000,
        cache_write_1h: 1_000_000,
        reasoning: 0,
    };
    // 5 + 25 + 0.5 + 6.25 + 1h writes at 2x input (10)
    assert!(close(c.cost(&event("anthropic", "claude-opus-9", t)).unwrap(), 46.75));
}

#[test]
fn matches_dated_suffixed_and_prefixed_names() {
    let c = Catalog::from_json(TABLE).unwrap();
    for name in ["claude-opus-9-20260101", "claude-opus-9[1m]", "anthropic/claude-opus-9", "Claude-Opus-9"] {
        assert!(c.lookup("anthropic", name).is_some(), "{name}");
    }
    // An unknown provider still finds the model.
    assert_eq!(c.lookup("opencode", "gpt-9").unwrap().provider, "openai");
    assert!(c.lookup("anthropic", "router-alias").is_none());
}

#[test]
fn long_prompts_use_tier_rates() {
    let c = Catalog::from_json(TABLE).unwrap();
    let small = Tokens { input: 1000, output: 1000, ..Tokens::default() };
    let large = Tokens { input: 1001, output: 1000, ..Tokens::default() };
    assert!(close(c.cost(&event("openai", "gpt-9", small)).unwrap(), (1000.0 * 2.0 + 1000.0 * 10.0) / 1e6));
    assert!(close(c.cost(&event("openai", "gpt-9", large)).unwrap(), (1001.0 * 4.0 + 1000.0 * 15.0) / 1e6));
}

#[test]
fn reported_cost_wins_and_cache_savings_are_counted() {
    let c = Catalog::from_json(TABLE).unwrap();
    let mut e = event("anthropic", "claude-opus-9", Tokens { cache_read: 1_000_000, ..Tokens::default() });
    assert!(close(c.cache_savings(&e), 4.5));
    e.reported_cost = Some(1.23);
    assert_eq!(c.cost(&e), Some(1.23));
}

#[test]
fn bundled_snapshot_loads() {
    let c = Catalog::bundled();
    assert!(c.lookup("anthropic", "claude-opus-5-5").is_some());
    assert!(c.model_ids().len() > 50);
}
