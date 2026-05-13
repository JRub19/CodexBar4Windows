//! Cost-usage scanning subsystem.
//!
//! Spec: `docs/windows/spec/70-cost-scanning.md`.
//!
//! Modules:
//!
//! - `pricing` — per-model rate table ($/MTok for input, output,
//!   cache read, cache creation).
//! - `claude_parser` — Claude Code JSONL line filter, serde decode,
//!   day-key derivation, Vertex AI detection.
//! - `codex_parser` — Codex session JSONL streaming scanner
//!   (`session_meta` + `turn_context` + `event_msg/token_count`
//!   with fork-inheritance subtraction).
//! - `pi_parser` — pi (Practical Intelligence) session JSONL parser
//!   with provider attribution between Claude and Codex.
//! - `dedup` — in-file (`messageId:requestId`) + cross-file
//!   (`sessionId:messageId:requestId`) canonical dedup with the
//!   sidechain / pathRole tie-breaker.
//! - `aggregator` — turns parsed rows into a daily / cycle USD
//!   totals + per-model breakdown the popup renders.
//! - `walker` — filesystem walker that resolves Codex / Claude / pi
//!   roots from env vars + standard fallback paths, walks them with
//!   mtime prefiltering, and skips hidden components.
//!
//! Still pending: projection algorithm, on-disk row cache.

pub mod aggregator;
pub mod claude_parser;
pub mod codex_parser;
pub mod dedup;
pub mod pi_parser;
pub mod pricing;
pub mod storage;
pub mod walker;

pub use aggregator::{aggregate_rows, AggregatedCost};
pub use claude_parser::{
    line_passes_prefilter, parse_claude_line, ClaudeUsageRow, PathRole, ProviderFilter,
};
pub use codex_parser::{
    build_parent_totals, CodexFileScanner, CodexSessionMeta, CodexUsageRow, TokenTriple,
};
pub use dedup::{canonical_row_key, dedup_in_file, RowWinsOver};
pub use pi_parser::{PiFileScanner, PiModelContext, PiUsageRow};
pub use pricing::{cost_for_row, PricingTable, RatePerMTok};
pub use storage::{
    footprint_signature, resolve_provider_roots, scan_all, scan_provider, FilesystemSize,
    OsStorageFs, ProviderStorageFootprint, StorageComponent, StorageProvider,
};
pub use walker::{discover, resolve_roots, DiscoveredFile, JsonlFamily};
