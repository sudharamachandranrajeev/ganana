//! Scans this machine's logs into a database and prints a short report, or
//! with `--json` the dashboard's overview data:
//!
//!     cargo run --release -p ganana-core --example scan -- /tmp/ganana-test.db [days] [--json]

use std::time::Instant;

use ganana_core::{Catalog, Roots, Store, overview, scan_all};

fn main() -> anyhow::Result<()> {
    let json = std::env::args().any(|a| a == "--json");
    let mut args = std::env::args().skip(1).filter(|a| a != "--json");
    let db = args.next().expect("usage: scan <db> [days]");
    let days = args.next().map_or(Ok(30), |d| d.parse())?;
    let store = Store::open(db.as_ref())?;
    let roots = Roots::from_env().expect("HOME is set");

    let started = Instant::now();
    let report = scan_all(&store, &roots);
    if json {
        let o = overview(&store, &Catalog::load(&store)?, days, chrono::Local::now())?;
        println!("{}", serde_json::to_string(&o)?);
        return Ok(());
    }
    println!("scan: {} files, {} events in {:.1?}", report.files_read, report.events, started.elapsed());
    for e in &report.errors {
        println!("error: {e}");
    }

    let o = overview(&store, &Catalog::load(&store)?, days, chrono::Local::now())?;
    println!("stored events: {}", o.events_stored);
    for (label, t) in [("today", &o.today), ("7 days", &o.week), ("period", &o.period)] {
        println!(
            "{label:>7}: {:>14} tokens  ${:>10.2}  saved ${:>9.2}  unpriced {:>12}  calls {}",
            t.total_tokens, t.cost, t.savings, t.unpriced_tokens, t.calls
        );
    }
    for s in o.sources.iter().chain(&o.models) {
        println!("  {:<28} {:>14} tokens  ${:>10.2}", s.name, s.totals.total_tokens, s.totals.cost);
    }
    for u in &o.unpriced {
        println!("  unpriced {}/{}: {} tokens", u.provider, u.model, u.total_tokens);
    }
    let l = ganana_core::limits(&store, &Catalog::load(&store)?, chrono::Local::now())?;
    for w in &l.windows {
        println!(
            "  limit {} {}m: {:.0}% resets {} rate {:?}/h full {:?} ({} readings)",
            w.meter,
            w.window_minutes,
            w.used_pct,
            w.resets_at,
            w.rate_per_hour.map(|r| (r * 10.0).round() / 10.0),
            w.full_at,
            w.history.len()
        );
    }
    Ok(())
}
