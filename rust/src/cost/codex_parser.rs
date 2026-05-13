//! Codex JSONL parser. Ported from spec
//! `docs/windows/spec/70-cost-scanning.md` §4.
//!
//! A Codex session file is a streaming sequence of three line types:
//!
//! - `session_meta` — first line; carries `session_id` and an
//!   optional `forked_from_id`. Cross-file inheritance subtraction
//!   uses this.
//! - `turn_context` — updates the current model for subsequent
//!   token-count snapshots.
//! - `event_msg` with `payload.type == "token_count"` — periodic
//!   cumulative usage totals plus an optional per-turn last delta.
//!
//! Delta computation is stateful per file: we track `previous_totals`
//! across lines and emit positive deltas only. When a session is
//! forked from a parent, the parent's totals at the fork point are
//! subtracted on the first total snapshot seen (`remaining_inherited`).

use std::collections::HashMap;

use chrono::{DateTime, FixedOffset, Local};
use serde_json::Value;

use super::claude_parser::MAX_LINE_BYTES;

const PREFILTER_NEEDLES: &[&[u8]] = &[
    b"\"type\":\"event_msg\"",
    b"\"type\":\"turn_context\"",
    b"\"type\":\"session_meta\"",
];
const TOKEN_COUNT_NEEDLE: &[u8] = b"\"token_count\"";

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TokenTriple {
    pub input: i64,
    pub cached: i64,
    pub output: i64,
}

impl TokenTriple {
    fn saturating_sub(&self, other: &Self) -> Self {
        Self {
            input: (self.input - other.input).max(0),
            cached: (self.cached - other.cached).max(0),
            output: (self.output - other.output).max(0),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodexSessionMeta {
    pub session_id: Option<String>,
    pub forked_from_id: Option<String>,
    pub fork_timestamp: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodexUsageRow {
    pub day_key: String,
    pub timestamp_unix_secs: i64,
    pub model: String,
    /// Delta input tokens for this snapshot.
    pub input_tokens: i64,
    /// Delta cached-input tokens for this snapshot, clamped to
    /// `<= input_tokens` so a misbehaving server cannot bill more
    /// cache reads than total input.
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub session_id: Option<String>,
    pub source_path: String,
}

/// Cheap byte filter mirroring §4.1: line must contain one of the
/// three line-type tokens, and `event_msg` lines must additionally
/// carry `token_count`.
pub fn line_passes_prefilter(bytes: &[u8]) -> bool {
    if bytes.len() > MAX_LINE_BYTES {
        return false;
    }
    let has_type = PREFILTER_NEEDLES
        .iter()
        .any(|needle| contains(bytes, needle));
    if !has_type {
        return false;
    }
    // The token_count needle is only required for event_msg lines.
    // We accept session_meta and turn_context unconditionally.
    if contains(bytes, b"\"type\":\"event_msg\"") {
        return contains(bytes, TOKEN_COUNT_NEEDLE);
    }
    true
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Streaming per-file Codex scanner. Construct once per `.jsonl`,
/// feed lines via `feed_line`, drain emitted rows with `take_rows`.
pub struct CodexFileScanner {
    source_path: String,
    session_meta: CodexSessionMeta,
    current_model: Option<String>,
    previous_totals: TokenTriple,
    remaining_inherited: TokenTriple,
    /// True once any total snapshot has been observed; clears the
    /// remaining_inherited counter per §4.5.
    saw_total: bool,
    emitted: Vec<CodexUsageRow>,
}

impl CodexFileScanner {
    pub fn new(source_path: impl Into<String>) -> Self {
        Self {
            source_path: source_path.into(),
            session_meta: CodexSessionMeta::default(),
            current_model: None,
            previous_totals: TokenTriple::default(),
            remaining_inherited: TokenTriple::default(),
            saw_total: false,
            emitted: Vec::new(),
        }
    }

    /// Seed the inheritance counter with the parent session's totals
    /// at the fork point. Must be called before the first total
    /// snapshot, typically after a top-level pass that built a
    /// `session_id → totals_at_fork` map.
    pub fn set_inherited_totals(&mut self, totals: TokenTriple) {
        self.remaining_inherited = totals;
    }

    pub fn session_meta(&self) -> &CodexSessionMeta {
        &self.session_meta
    }

    pub fn take_rows(&mut self) -> Vec<CodexUsageRow> {
        std::mem::take(&mut self.emitted)
    }

    /// Feed one JSONL line. Returns the number of rows added to the
    /// emitted buffer (zero on session_meta / turn_context / dropped
    /// lines).
    pub fn feed_line(&mut self, bytes: &[u8]) -> usize {
        if !line_passes_prefilter(bytes) {
            return 0;
        }
        let value: Value = match serde_json::from_slice(bytes) {
            Ok(v) => v,
            Err(_) => return 0,
        };
        let line_type = value.get("type").and_then(Value::as_str);
        match line_type {
            Some("session_meta") => {
                self.session_meta = parse_session_meta(&value);
                0
            }
            Some("turn_context") => {
                self.current_model = parse_turn_context_model(&value);
                0
            }
            Some("event_msg") => {
                let payload = match value.get("payload") {
                    Some(p) => p,
                    None => return 0,
                };
                if payload.get("type").and_then(Value::as_str) != Some("token_count") {
                    return 0;
                }
                let info = payload.get("info").unwrap_or(payload);
                let timestamp = value
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .or_else(|| payload.get("timestamp").and_then(Value::as_str));
                let Some((day_key, ts_secs)) = timestamp.and_then(day_and_timestamp) else {
                    return 0;
                };
                let model = resolve_model(self.current_model.as_deref(), info, payload, &value);
                let delta = self.compute_delta(info);
                if delta.input == 0 && delta.cached == 0 && delta.output == 0 {
                    return 0;
                }
                let cached_clamp = delta.cached.min(delta.input);
                self.emitted.push(CodexUsageRow {
                    day_key,
                    timestamp_unix_secs: ts_secs,
                    model,
                    input_tokens: delta.input,
                    cached_input_tokens: cached_clamp,
                    output_tokens: delta.output,
                    session_id: self.session_meta.session_id.clone(),
                    source_path: self.source_path.clone(),
                });
                1
            }
            _ => 0,
        }
    }

    fn compute_delta(&mut self, info: &Value) -> TokenTriple {
        let total = read_token_block(info.get("total_token_usage"));
        let last = read_token_block(info.get("last_token_usage"));
        if let Some(total) = total {
            let adjusted = total.saturating_sub(&self.remaining_inherited);
            let delta = adjusted.saturating_sub(&self.previous_totals);
            self.previous_totals = adjusted;
            self.saw_total = true;
            self.remaining_inherited = TokenTriple::default();
            delta
        } else if let Some(last) = last {
            // Consume inheritance from the per-turn delta when no
            // cumulative snapshot has been seen yet.
            let consumed = last.saturating_sub(&self.remaining_inherited);
            self.remaining_inherited = self
                .remaining_inherited
                .saturating_sub(&last_minus_consumed(&last, &consumed));
            self.previous_totals = TokenTriple {
                input: self.previous_totals.input + consumed.input,
                cached: self.previous_totals.cached + consumed.cached,
                output: self.previous_totals.output + consumed.output,
            };
            consumed
        } else {
            TokenTriple::default()
        }
    }
}

/// Helper that returns the portion of `last` we did not consume
/// from the inheritance counter — what remains to subtract on the
/// next line.
fn last_minus_consumed(last: &TokenTriple, consumed: &TokenTriple) -> TokenTriple {
    TokenTriple {
        input: (last.input - consumed.input).max(0),
        cached: (last.cached - consumed.cached).max(0),
        output: (last.output - consumed.output).max(0),
    }
}

fn read_token_block(value: Option<&Value>) -> Option<TokenTriple> {
    let block = value?;
    let input = block
        .get("input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    // `cached_input_tokens` is the modern name; accept the legacy
    // `cache_read_input_tokens` per spec §4.4.
    let cached = block
        .get("cached_input_tokens")
        .or_else(|| block.get("cache_read_input_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output = block
        .get("output_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if input == 0 && cached == 0 && output == 0 {
        return None;
    }
    Some(TokenTriple {
        input,
        cached,
        output,
    })
}

fn parse_session_meta(value: &Value) -> CodexSessionMeta {
    let payload = value.get("payload").unwrap_or(value);
    let session_id = pick_str(payload, &["session_id", "sessionId", "id"])
        .or_else(|| pick_str(value, &["session_id", "sessionId", "id"]));
    let forked_from_id = pick_str(
        payload,
        &[
            "forked_from_id",
            "forkedFromId",
            "parent_session_id",
            "parentSessionId",
        ],
    );
    let fork_timestamp =
        pick_str(payload, &["timestamp"]).or_else(|| pick_str(value, &["timestamp"]));
    CodexSessionMeta {
        session_id,
        forked_from_id,
        fork_timestamp,
    }
}

fn parse_turn_context_model(value: &Value) -> Option<String> {
    let payload = value.get("payload")?;
    let direct = payload.get("model").and_then(Value::as_str);
    if let Some(m) = direct {
        return Some(m.to_string());
    }
    payload
        .get("info")
        .and_then(|i| i.get("model"))
        .and_then(Value::as_str)
        .map(|s| s.to_string())
}

fn pick_str(obj: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = obj.get(*key).and_then(Value::as_str) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn resolve_model(
    current_model: Option<&str>,
    info: &Value,
    payload: &Value,
    top: &Value,
) -> String {
    if let Some(m) = current_model {
        return normalize(m);
    }
    let candidates = [
        info.get("model").and_then(Value::as_str),
        info.get("model_name").and_then(Value::as_str),
        payload.get("model").and_then(Value::as_str),
        top.get("model").and_then(Value::as_str),
    ];
    for candidate in candidates.into_iter().flatten() {
        if !candidate.is_empty() {
            return normalize(candidate);
        }
    }
    "gpt-5".to_string()
}

/// Normalise per spec §6.2: strip an `openai/` prefix and a trailing
/// `-YYYY-MM-DD` date stamp from known model names.
pub fn normalize(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if let Some(stripped) = s.strip_prefix("openai/") {
        s = stripped.to_string();
    }
    // Match a trailing `-YYYY-MM-DD` suffix (11 chars total). The
    // last 10 bytes of the string must match `YYYY-MM-DD` exactly
    // and the byte before must be `-`.
    let bytes = s.as_bytes();
    if bytes.len() >= 11 {
        let dash_idx = bytes.len() - 11;
        let date_bytes = &bytes[bytes.len() - 10..];
        let looks_like_date = bytes[dash_idx] == b'-'
            && date_bytes.iter().enumerate().all(|(i, b)| match i {
                4 | 7 => *b == b'-',
                _ => b.is_ascii_digit(),
            });
        if looks_like_date {
            let head = &s[..dash_idx];
            if head.starts_with("gpt-")
                || head.starts_with("codex-")
                || head.starts_with("o1-")
                || head.starts_with("o3-")
                || head.starts_with("o4-")
            {
                return head.to_string();
            }
        }
    }
    s
}

fn day_and_timestamp(raw: &str) -> Option<(String, i64)> {
    let parsed: DateTime<FixedOffset> = DateTime::parse_from_rfc3339(raw).ok()?;
    let local = parsed.with_timezone(&Local);
    let day_key = local.format("%Y-%m-%d").to_string();
    Some((day_key, parsed.timestamp()))
}

/// Build a per-session totals map for fork-inheritance subtraction.
/// Given the scanned rows of every file, returns
/// `session_id → cumulative totals across all that session's rows`.
/// The caller passes this to `CodexFileScanner::set_inherited_totals`
/// for every forked session before re-running the per-file scan.
pub fn build_parent_totals(rows: &[CodexUsageRow]) -> HashMap<String, TokenTriple> {
    let mut out: HashMap<String, TokenTriple> = HashMap::new();
    for row in rows {
        let Some(session) = row.session_id.as_deref() else {
            continue;
        };
        let entry = out.entry(session.to_string()).or_default();
        entry.input += row.input_tokens;
        entry.cached += row.cached_input_tokens;
        entry.output += row.output_tokens;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(scanner: &mut CodexFileScanner, ls: &[&str]) {
        for line in ls {
            scanner.feed_line(line.as_bytes());
        }
    }

    const SESSION_LINE: &str = r#"{"type":"session_meta","timestamp":"2026-05-13T10:00:00Z","payload":{"session_id":"sess-1","timestamp":"2026-05-13T10:00:00Z"}}"#;

    const TURN_CONTEXT: &str = r#"{"type":"turn_context","payload":{"model":"gpt-5.1-codex"}}"#;

    #[test]
    fn prefilter_rejects_unrelated_lines() {
        assert!(!line_passes_prefilter(b"{}"));
        // event_msg without token_count is rejected.
        assert!(!line_passes_prefilter(
            br#"{"type":"event_msg","payload":{"type":"chunk"}}"#
        ));
        // session_meta passes without token_count.
        assert!(line_passes_prefilter(SESSION_LINE.as_bytes()));
    }

    #[test]
    fn first_total_snapshot_emits_full_delta() {
        let mut scanner = CodexFileScanner::new("/a.jsonl");
        feed_all(&mut scanner, &[SESSION_LINE, TURN_CONTEXT]);
        let line = r#"{"type":"event_msg","timestamp":"2026-05-13T10:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":200,"output_tokens":300}}}}"#;
        scanner.feed_line(line.as_bytes());
        let rows = scanner.take_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].input_tokens, 1000);
        assert_eq!(rows[0].cached_input_tokens, 200);
        assert_eq!(rows[0].output_tokens, 300);
        assert_eq!(rows[0].model, "gpt-5.1-codex");
        assert_eq!(rows[0].session_id.as_deref(), Some("sess-1"));
    }

    #[test]
    fn subsequent_total_snapshot_emits_only_delta_above_prev() {
        let mut scanner = CodexFileScanner::new("/a.jsonl");
        feed_all(&mut scanner, &[SESSION_LINE, TURN_CONTEXT]);
        scanner.feed_line(br#"{"type":"event_msg","timestamp":"2026-05-13T10:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"output_tokens":300}}}}"#);
        scanner.feed_line(br#"{"type":"event_msg","timestamp":"2026-05-13T10:02:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1700,"output_tokens":900}}}}"#);
        let rows = scanner.take_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].input_tokens, 700);
        assert_eq!(rows[1].output_tokens, 600);
    }

    #[test]
    fn cached_clamp_caps_cached_above_input() {
        let mut scanner = CodexFileScanner::new("/a.jsonl");
        feed_all(&mut scanner, &[SESSION_LINE, TURN_CONTEXT]);
        scanner.feed_line(br#"{"type":"event_msg","timestamp":"2026-05-13T10:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":999,"output_tokens":50}}}}"#);
        let rows = scanner.take_rows();
        assert_eq!(rows[0].input_tokens, 100);
        assert_eq!(rows[0].cached_input_tokens, 100);
    }

    #[test]
    fn cache_read_input_tokens_alias_is_accepted() {
        let mut scanner = CodexFileScanner::new("/a.jsonl");
        feed_all(&mut scanner, &[SESSION_LINE, TURN_CONTEXT]);
        scanner.feed_line(br#"{"type":"event_msg","timestamp":"2026-05-13T10:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":500,"cache_read_input_tokens":120,"output_tokens":80}}}}"#);
        let rows = scanner.take_rows();
        assert_eq!(rows[0].cached_input_tokens, 120);
    }

    #[test]
    fn fork_inheritance_subtracts_parent_totals_at_first_snapshot() {
        let mut scanner = CodexFileScanner::new("/child.jsonl");
        feed_all(&mut scanner, &[SESSION_LINE, TURN_CONTEXT]);
        scanner.set_inherited_totals(TokenTriple {
            input: 800,
            cached: 100,
            output: 200,
        });
        scanner.feed_line(br#"{"type":"event_msg","timestamp":"2026-05-13T10:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1200,"cached_input_tokens":200,"output_tokens":350}}}}"#);
        let rows = scanner.take_rows();
        // Delta = total - inherited = (400, 100, 150). cached_clamp
        // caps cached at delta.input = 400 (100 <= 400).
        assert_eq!(rows[0].input_tokens, 400);
        assert_eq!(rows[0].cached_input_tokens, 100);
        assert_eq!(rows[0].output_tokens, 150);
    }

    #[test]
    fn last_token_usage_consumes_inheritance_when_no_total() {
        let mut scanner = CodexFileScanner::new("/child.jsonl");
        feed_all(&mut scanner, &[SESSION_LINE, TURN_CONTEXT]);
        scanner.set_inherited_totals(TokenTriple {
            input: 100,
            cached: 0,
            output: 50,
        });
        // last_token_usage delta of (150, 0, 80) consumes inheritance
        // first: emitted (50, 0, 30) — what's left after subtracting
        // the inheritance, since the per-turn delta did not exceed
        // it on the input axis (150-100=50).
        scanner.feed_line(br#"{"type":"event_msg","timestamp":"2026-05-13T10:01:00Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":150,"output_tokens":80}}}}"#);
        let rows = scanner.take_rows();
        assert_eq!(rows[0].input_tokens, 50);
        assert_eq!(rows[0].output_tokens, 30);
    }

    #[test]
    fn model_fallback_chain_when_no_turn_context() {
        let mut scanner = CodexFileScanner::new("/a.jsonl");
        feed_all(&mut scanner, &[SESSION_LINE]);
        scanner.feed_line(br#"{"type":"event_msg","timestamp":"2026-05-13T10:01:00Z","payload":{"type":"token_count","info":{"model":"gpt-5-codex","total_token_usage":{"input_tokens":1}}}}"#);
        let rows = scanner.take_rows();
        assert_eq!(rows[0].model, "gpt-5-codex");
    }

    #[test]
    fn model_fallback_to_literal_gpt5_when_nothing_set() {
        let mut scanner = CodexFileScanner::new("/a.jsonl");
        feed_all(&mut scanner, &[SESSION_LINE]);
        scanner.feed_line(br#"{"type":"event_msg","timestamp":"2026-05-13T10:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1}}}}"#);
        let rows = scanner.take_rows();
        assert_eq!(rows[0].model, "gpt-5");
    }

    #[test]
    fn normalise_strips_openai_prefix_and_known_date_suffix() {
        assert_eq!(normalize("openai/gpt-5.1-codex"), "gpt-5.1-codex");
        assert_eq!(normalize("gpt-5.1-codex-2026-05-13"), "gpt-5.1-codex");
        assert_eq!(normalize("o3-mini-2026-05-13"), "o3-mini");
        // Unknown family is left alone.
        assert_eq!(
            normalize("future-model-2026-05-13"),
            "future-model-2026-05-13"
        );
    }

    #[test]
    fn session_meta_picks_payload_session_id_first() {
        let value: Value = serde_json::from_str(SESSION_LINE).unwrap();
        let meta = parse_session_meta(&value);
        assert_eq!(meta.session_id.as_deref(), Some("sess-1"));
    }

    #[test]
    fn session_meta_picks_forked_from_id_fallback_names() {
        let line = r#"{"type":"session_meta","payload":{"session_id":"sess-c","parent_session_id":"sess-p","timestamp":"2026-05-13T10:00:00Z"}}"#;
        let value: Value = serde_json::from_str(line).unwrap();
        let meta = parse_session_meta(&value);
        assert_eq!(meta.forked_from_id.as_deref(), Some("sess-p"));
    }

    #[test]
    fn build_parent_totals_sums_rows_by_session() {
        let rows = vec![
            row("sess-1", 100, 20, 50),
            row("sess-1", 200, 40, 80),
            row("sess-2", 300, 60, 100),
        ];
        let map = build_parent_totals(&rows);
        let one = map.get("sess-1").unwrap();
        assert_eq!(one.input, 300);
        assert_eq!(one.cached, 60);
        assert_eq!(one.output, 130);
        assert_eq!(map.get("sess-2").unwrap().input, 300);
    }

    fn row(session: &str, input: i64, cached: i64, output: i64) -> CodexUsageRow {
        CodexUsageRow {
            day_key: "2026-05-13".into(),
            timestamp_unix_secs: 0,
            model: "gpt-5".into(),
            input_tokens: input,
            cached_input_tokens: cached,
            output_tokens: output,
            session_id: Some(session.into()),
            source_path: "/a.jsonl".into(),
        }
    }

    #[test]
    fn turn_context_updates_model_after_initial_line() {
        let mut scanner = CodexFileScanner::new("/a.jsonl");
        feed_all(&mut scanner, &[SESSION_LINE]);
        scanner.feed_line(br#"{"type":"event_msg","timestamp":"2026-05-13T10:01:00Z","payload":{"type":"token_count","info":{"model":"gpt-5-codex","total_token_usage":{"input_tokens":100}}}}"#);
        scanner.feed_line(TURN_CONTEXT.as_bytes());
        scanner.feed_line(br#"{"type":"event_msg","timestamp":"2026-05-13T10:02:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":250}}}}"#);
        let rows = scanner.take_rows();
        // First row had no turn_context yet → fell back to info.model.
        assert_eq!(rows[0].model, "gpt-5-codex");
        // Second row picks up the turn_context model.
        assert_eq!(rows[1].model, "gpt-5.1-codex");
    }

    #[test]
    fn unrelated_lines_emit_nothing() {
        let mut scanner = CodexFileScanner::new("/a.jsonl");
        scanner.feed_line(b"{\"type\":\"user_message\"}");
        scanner.feed_line(b"junk");
        scanner.feed_line(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"chunk\"}}");
        assert!(scanner.take_rows().is_empty());
    }
}
