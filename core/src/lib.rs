//! Gaṇanā core: reads the usage logs that AI tools write on this machine,
//! keeps one deduplicated SQLite store of usage events, prices them from the
//! models.dev catalog, and builds the reports the dashboard shows.
//!
//! It also tracks plan limits (Claude Pro/Max through the status line relay,
//! ChatGPT through Codex logs) and the plans the user enters.
//!
//! Privacy rule: parsers read token counts, model names, timestamps, limit
//! percentages and the working folder only. Prompt and response text is
//! never stored.

pub mod claude_relay;
pub mod db;
mod jsonl;
pub mod limits;
pub mod model;
pub mod pricing;
pub mod report;
pub mod sources;

pub use db::Store;
pub use limits::{Limits, limits};
pub use model::{LimitSnapshot, Plan, Tokens, UsageEvent};
pub use pricing::Catalog;
pub use report::{Overview, overview};
pub use sources::{Roots, ScanReport, scan_all};
