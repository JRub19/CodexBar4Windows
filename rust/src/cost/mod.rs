//! Cost-usage scanning subsystem. Phase 7A — Claude Code JSONL slice.
//!
//! Spec: `docs/windows/spec/70-cost-scanning.md`.
//!
//! This module ships the focused subset:
//!
//! - `pricing` — per-model rate table ($/MTok for input, output,
//!   cache read, cache creation).
//! - `claude_parser` — line filter, `serde` decode, day-key
//!   derivation, Vertex AI detection.
//! - `dedup` — in-file (`messageId:requestId`) + cross-file
//!   (`sessionId:messageId:requestId`) canonical dedup with the
//!   sidechain / pathRole tie-breaker.
//! - `aggregator` — turns parsed rows into a daily / cycle USD
//!   totals + per-model breakdown the popup renders.
//!
//! Codex and pi parsers, the recursive filesystem walker, projection
//! algorithm, and on-disk cache are explicitly out of scope here and
//! land in a follow-up.

pub mod aggregator;
pub mod claude_parser;
pub mod dedup;
pub mod pricing;

pub use aggregator::{aggregate_rows, AggregatedCost};
pub use claude_parser::{
    line_passes_prefilter, parse_claude_line, ClaudeUsageRow, PathRole, ProviderFilter,
};
pub use dedup::{canonical_row_key, dedup_in_file, RowWinsOver};
pub use pricing::{cost_for_row, PricingTable, RatePerMTok};
