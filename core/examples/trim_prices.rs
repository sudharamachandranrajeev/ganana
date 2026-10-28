//! Regenerates the bundled price snapshot:
//!
//!     curl -sL https://models.dev/api.json -o /tmp/api.json
//!     cargo run -p ganana-core --example trim_prices -- /tmp/api.json > core/data/prices.json

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("usage: trim_prices <api.json>");
    println!("{}", ganana_core::pricing::trim(&std::fs::read_to_string(path)?)?);
    Ok(())
}
