//! Runtime glue: walks discovered JSONL files, reads them line by
//! line through the appropriate parser, converts to a common row
//! shape, and aggregates. Produces a `ProviderCostSnapshot` ready
//! for the popup chart.
//!
//! This is the layer the Tauri command calls into. It deliberately
//! does no caching — the surrounding `CostStore` owns the cache.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Datelike, Local, TimeZone};

use crate::cost::aggregator::aggregate_rows;
use crate::cost::claude_parser::{
    parse_claude_line, ClaudeUsageRow, PathRole, ProviderFilter, MAX_LINE_BYTES,
};
use crate::cost::codex_parser::{CodexFileScanner, CodexUsageRow};
use crate::cost::pricing::PricingTable;
use crate::cost::walker::{discover, DiscoveredFile, Env, Filesystem, JsonlFamily, OsEnv, OsFilesystem};
use crate::providers::models::provider_cost::ProviderCostSnapshot;

/// Snapshot store shared between the Tauri command and the periodic
/// scan task. Held inside `Arc` so cloning is cheap.
pub struct CostStore {
    snapshots: Arc<RwLock<HashMap<String, ProviderCostSnapshot>>>,
    pricing: PricingTable,
}

impl Default for CostStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CostStore {
    pub fn new() -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            pricing: PricingTable::default(),
        }
    }

    /// Read the cached snapshot for one provider. Returns `None` if
    /// no scan has yet produced data for that provider id.
    pub fn get(&self, provider_id: &str) -> Option<ProviderCostSnapshot> {
        self.snapshots
            .read()
            .ok()?
            .get(provider_id)
            .cloned()
    }

    /// Snapshot of every provider's latest cost data. Empty when no
    /// scan has yet run.
    pub fn snapshots(&self) -> HashMap<String, ProviderCostSnapshot> {
        self.snapshots
            .read()
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// Run a single scan pass over the host's JSONL directories.
    /// Updates the snapshots map for `claude` and `codex` provider
    /// ids. Cheap when the directories are empty (≈ a single
    /// filesystem walk).
    pub fn scan_once(&self) {
        let env = OsEnv;
        let fs = OsFilesystem;
        self.scan_with(&env, &fs);
    }

    pub fn scan_with(&self, env: &dyn Env, fs: &dyn Filesystem) {
        // Build the 30-day window of local-time day keys, oldest
        // first so the resulting Vec aligns with `last_30_days_usd`.
        let day_keys = last_30_days_local();
        let day_key_refs: Vec<&str> = day_keys.iter().map(String::as_str).collect();

        // Claude — exclude Vertex AI rows since this snapshot is
        // scoped to the Anthropic provider. (Vertex traffic is
        // priced differently and surfaces in its own provider id.)
        let claude_files = discover(JsonlFamily::ClaudeCode, env, fs, None);
        let claude_rows = read_claude_files(&claude_files, ProviderFilter::ExcludeVertexAI);
        let claude_snap = build_snapshot(&claude_rows, &self.pricing, &day_key_refs);

        // Codex.
        let codex_files = discover(JsonlFamily::Codex, env, fs, None);
        let codex_rows = read_codex_files(&codex_files);
        let codex_snap = build_snapshot(&codex_rows, &self.pricing, &day_key_refs);

        let mut w = self.snapshots.write().expect("cost store poisoned");
        if !claude_rows.is_empty() {
            w.insert("claude".to_string(), claude_snap);
        }
        if !codex_rows.is_empty() {
            w.insert("codex".to_string(), codex_snap);
        }
    }
}

fn build_snapshot(
    files: &[Vec<ClaudeUsageRow>],
    pricing: &PricingTable,
    day_key_refs: &[&str],
) -> ProviderCostSnapshot {
    // Current cycle = today (last day in the window).
    let current_cycle = if let Some(last) = day_key_refs.last() {
        vec![*last]
    } else {
        Vec::new()
    };
    let agg = aggregate_rows(files.to_vec(), pricing);
    agg.to_provider_snapshot(&current_cycle, None, day_key_refs)
}

/// Read every Claude JSONL file into rows. Skips unreadable files
/// silently — a missing root is a normal case (e.g. user doesn't
/// have Claude Code installed).
fn read_claude_files(files: &[DiscoveredFile], filter: ProviderFilter) -> Vec<Vec<ClaudeUsageRow>> {
    let mut out = Vec::with_capacity(files.len());
    for f in files {
        let path_str = f.path.to_string_lossy().to_string();
        let Ok(file) = File::open(&f.path) else { continue };
        let reader = BufReader::with_capacity(64 * 1024, file);
        let mut rows = Vec::new();
        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if line.is_empty() || line.len() > MAX_LINE_BYTES {
                continue;
            }
            if let Some(row) = parse_claude_line(line.as_bytes(), &path_str, filter) {
                rows.push(row);
            }
        }
        if !rows.is_empty() {
            out.push(rows);
        }
    }
    out
}

/// Read every Codex JSONL file via the streaming scanner, then convert
/// the resulting `CodexUsageRow`s into the common `ClaudeUsageRow`
/// shape the aggregator consumes. The aggregator is agnostic to
/// provider — it keys on model id alone.
fn read_codex_files(files: &[DiscoveredFile]) -> Vec<Vec<ClaudeUsageRow>> {
    let mut out = Vec::with_capacity(files.len());
    for f in files {
        let path_str = f.path.to_string_lossy().to_string();
        let Ok(file) = File::open(&f.path) else { continue };
        let reader = BufReader::with_capacity(64 * 1024, file);
        let mut scanner = CodexFileScanner::new(path_str.clone());
        for line in reader.lines() {
            let Ok(line) = line else { continue };
            scanner.feed_line(line.as_bytes());
        }
        let codex_rows = scanner.take_rows();
        if codex_rows.is_empty() {
            continue;
        }
        let claude_rows: Vec<ClaudeUsageRow> =
            codex_rows.into_iter().map(codex_to_claude_row).collect();
        out.push(claude_rows);
    }
    out
}

/// Re-shape a Codex row into the aggregator's expected row format.
/// Cache-creation tokens don't exist in the Codex model — the
/// scanner only exposes cached input tokens, which we map to
/// `cache_read_input_tokens` (rate parity with macOS).
fn codex_to_claude_row(r: CodexUsageRow) -> ClaudeUsageRow {
    ClaudeUsageRow {
        day_key: r.day_key,
        timestamp_unix_secs: r.timestamp_unix_secs,
        model: r.model,
        input_tokens: r.input_tokens,
        output_tokens: r.output_tokens,
        cache_read_input_tokens: r.cached_input_tokens,
        cache_creation_input_tokens: 0,
        // Dedup uses (session_id, message_id, request_id). Codex
        // doesn't carry message/request ids at the row level, so we
        // synthesize a unique id from the source path + timestamp.
        // This means dedup is effectively per-file for Codex, which
        // matches the macOS source's behavior.
        message_id: Some(format!("{}#{}", r.source_path, r.timestamp_unix_secs)),
        request_id: None,
        session_id: r.session_id,
        is_sidechain: false,
        is_vertex: false,
        path_role: PathRole::Parent,
        source_path: r.source_path,
    }
}

/// Generate the last 30 local-time day keys (YYYY-MM-DD), oldest
/// first. Today is the last element.
pub fn last_30_days_local() -> Vec<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let today = Local.timestamp_opt(now, 0).single();
    let Some(today) = today else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(30);
    for back in (0..30).rev() {
        let day = today - chrono::Duration::days(back);
        out.push(format!(
            "{:04}-{:02}-{:02}",
            day.year(),
            day.month(),
            day.day()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_30_days_local_returns_30_keys_in_chronological_order() {
        let keys = last_30_days_local();
        assert_eq!(keys.len(), 30);
        for w in keys.windows(2) {
            assert!(w[0] < w[1], "expected sorted, got {w:?}");
        }
    }

    #[test]
    fn cost_store_starts_empty() {
        let store = CostStore::new();
        assert!(store.get("claude").is_none());
        assert!(store.snapshots().is_empty());
    }
}
