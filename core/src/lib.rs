//! Gaṇanā core: reads the usage logs that AI tools write on this machine,
//! keeps one deduplicated SQLite store of usage events, prices them from the
//! models.dev catalog, and builds the reports the dashboard shows.
//!
//! Privacy rule: parsers read token counts, model names, timestamps and the
//! working folder only. Prompt and response text is never stored.

pub mod db;
mod jsonl;
pub mod model;
pub mod pricing;
pub mod report;
pub mod sources;

pub use db::Store;
pub use model::{Tokens, UsageEvent};
pub use pricing::Catalog;
pub use report::{Overview, overview};
pub use sources::{Roots, ScanReport, scan_all};
