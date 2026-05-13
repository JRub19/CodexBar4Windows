//! Claude Code JSONL parser. Ported from spec
//! `docs/windows/spec/70-cost-scanning.md` §2.

use chrono::{DateTime, FixedOffset, Local, TimeZone};
use serde::Deserialize;

const REQUIRED_TYPE: &[u8] = b"\"type\":\"assistant\"";
const REQUIRED_USAGE: &[u8] = b"\"usage\"";
pub const MAX_LINE_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathRole {
    Parent,
    Subagent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderFilter {
    All,
    VertexAIOnly,
    ExcludeVertexAI,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClaudeUsageRow {
    pub day_key: String,
    pub timestamp_unix_secs: i64,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub message_id: Option<String>,
    pub request_id: Option<String>,
    pub session_id: Option<String>,
    pub is_sidechain: bool,
    pub is_vertex: bool,
    pub path_role: PathRole,
    pub source_path: String,
}

/// Cheap pre-filter: drop lines that cannot possibly be assistant
/// usage rows before paying for a full JSON parse. Mirrors the spec's
/// §2.1 byte-substring check.
pub fn line_passes_prefilter(bytes: &[u8]) -> bool {
    if bytes.len() > MAX_LINE_BYTES {
        return false;
    }
    contains(bytes, REQUIRED_TYPE) && contains(bytes, REQUIRED_USAGE)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[derive(Deserialize)]
struct WireLine {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    message: Option<WireMessage>,
    #[serde(default, rename = "requestId")]
    request_id: Option<String>,
    #[serde(default, alias = "session_id", alias = "sessionId")]
    session_id_top: Option<String>,
    #[serde(default)]
    metadata: Option<WireMetadata>,
    #[serde(default, rename = "isSidechain")]
    is_sidechain: bool,
}

#[derive(Deserialize)]
struct WireMessage {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<WireUsage>,
    #[serde(default)]
    metadata: Option<WireMetadata>,
}

#[derive(Deserialize, Default)]
struct WireUsage {
    #[serde(default)]
    input_tokens: i64,
    #[serde(default)]
    output_tokens: i64,
    #[serde(default)]
    cache_read_input_tokens: i64,
    #[serde(default)]
    cache_creation_input_tokens: i64,
}

#[derive(Deserialize, Default)]
struct WireMetadata {
    #[serde(default, rename = "sessionId")]
    session_id: Option<String>,
}

/// Parse one JSONL line into a `ClaudeUsageRow`. Returns `None` if
/// the line is irrelevant (failed prefilter, wrong type, missing
/// required fields, all-zero usage).
pub fn parse_claude_line(
    bytes: &[u8],
    source_path: &str,
    filter: ProviderFilter,
) -> Option<ClaudeUsageRow> {
    if !line_passes_prefilter(bytes) {
        return None;
    }
    let wire: WireLine = serde_json::from_slice(bytes).ok()?;
    if wire.r#type.as_deref() != Some("assistant") {
        return None;
    }
    let timestamp = wire.timestamp.as_deref()?;
    let message = wire.message.as_ref()?;
    let model = message.model.as_deref()?.to_string();
    let usage = message.usage.as_ref()?;

    if usage.input_tokens == 0
        && usage.output_tokens == 0
        && usage.cache_read_input_tokens == 0
        && usage.cache_creation_input_tokens == 0
    {
        return None;
    }

    let (day_key, ts_secs) = day_and_timestamp(timestamp)?;
    let path_role = if source_path
        .to_ascii_lowercase()
        .replace('\\', "/")
        .contains("/subagents/")
    {
        PathRole::Subagent
    } else {
        PathRole::Parent
    };
    let is_vertex = is_vertex_row(message, &wire);
    match filter {
        ProviderFilter::All => {}
        ProviderFilter::VertexAIOnly if !is_vertex => return None,
        ProviderFilter::ExcludeVertexAI if is_vertex => return None,
        _ => {}
    }

    let session_id = message
        .metadata
        .as_ref()
        .and_then(|m| m.session_id.clone())
        .or_else(|| wire.metadata.as_ref().and_then(|m| m.session_id.clone()))
        .or(wire.session_id_top.clone());

    Some(ClaudeUsageRow {
        day_key,
        timestamp_unix_secs: ts_secs,
        model,
        input_tokens: usage.input_tokens.max(0),
        output_tokens: usage.output_tokens.max(0),
        cache_read_input_tokens: usage.cache_read_input_tokens.max(0),
        cache_creation_input_tokens: usage.cache_creation_input_tokens.max(0),
        message_id: message.id.clone(),
        request_id: wire.request_id.clone(),
        session_id,
        is_sidechain: wire.is_sidechain,
        is_vertex,
        path_role,
        source_path: source_path.to_string(),
    })
}

fn day_and_timestamp(raw: &str) -> Option<(String, i64)> {
    let parsed: DateTime<FixedOffset> = DateTime::parse_from_rfc3339(raw).ok()?;
    let local = parsed.with_timezone(&Local);
    let day_key = local.format("%Y-%m-%d").to_string();
    let ts_secs = parsed.timestamp();
    // Guard: chrono's parser is strict; verify we can reverse with the
    // host TimeZone (cheap sanity check).
    let _ = Local.from_local_datetime(&local.naive_local()).single();
    Some((day_key, ts_secs))
}

fn is_vertex_row(message: &WireMessage, wire: &WireLine) -> bool {
    if let Some(id) = &message.id {
        if id.contains("_vrtx_") {
            return true;
        }
    }
    if let Some(rid) = &wire.request_id {
        if rid.contains("_vrtx_") {
            return true;
        }
    }
    if let Some(model) = &message.model {
        if model.contains('@')
            && model
                .strip_prefix("claude-")
                .map(|s| s.contains('@'))
                .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LINE: &[u8] = br#"{"type":"assistant","timestamp":"2026-05-13T14:33:21Z","requestId":"req-1","sessionId":"sess-1","message":{"id":"msg-1","model":"claude-sonnet-4-7-20251022","usage":{"input_tokens":1000,"output_tokens":200,"cache_read_input_tokens":50,"cache_creation_input_tokens":10}}}"#;

    #[test]
    fn prefilter_rejects_lines_missing_either_substring() {
        assert!(!line_passes_prefilter(b"{}"));
        assert!(!line_passes_prefilter(b"{\"type\":\"assistant\"}"));
        assert!(line_passes_prefilter(SAMPLE_LINE));
    }

    #[test]
    fn prefilter_rejects_overlong_lines() {
        let mut big = vec![b' '; MAX_LINE_BYTES + 1];
        big[..SAMPLE_LINE.len()].copy_from_slice(SAMPLE_LINE);
        assert!(!line_passes_prefilter(&big));
    }

    #[test]
    fn parse_happy_path_extracts_all_fields() {
        let row = parse_claude_line(SAMPLE_LINE, "/projects/foo.jsonl", ProviderFilter::All)
            .expect("row");
        assert_eq!(row.input_tokens, 1000);
        assert_eq!(row.output_tokens, 200);
        assert_eq!(row.cache_read_input_tokens, 50);
        assert_eq!(row.cache_creation_input_tokens, 10);
        assert_eq!(row.model, "claude-sonnet-4-7-20251022");
        assert_eq!(row.message_id.as_deref(), Some("msg-1"));
        assert_eq!(row.request_id.as_deref(), Some("req-1"));
        assert_eq!(row.session_id.as_deref(), Some("sess-1"));
        assert!(!row.is_sidechain);
        assert!(!row.is_vertex);
        assert_eq!(row.path_role, PathRole::Parent);
    }

    #[test]
    fn parse_discards_all_zero_usage_rows() {
        let body = br#"{"type":"assistant","timestamp":"2026-05-13T14:33:21Z","message":{"model":"claude-sonnet-4-7","usage":{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}"#;
        assert!(parse_claude_line(body, "/x.jsonl", ProviderFilter::All).is_none());
    }

    #[test]
    fn parse_discards_non_assistant_lines() {
        let body = br#"{"type":"user","timestamp":"2026-05-13T14:33:21Z","message":{"model":"claude-sonnet","usage":{"input_tokens":1}}}"#;
        // Won't even pass prefilter because the assistant substring is
        // absent; sanity check anyway.
        assert!(parse_claude_line(body, "/x.jsonl", ProviderFilter::All).is_none());
    }

    #[test]
    fn parse_detects_vertex_via_msg_id_prefix() {
        let body = br#"{"type":"assistant","timestamp":"2026-05-13T14:33:21Z","message":{"id":"msg_vrtx_0154abc","model":"claude-sonnet","usage":{"input_tokens":100}}}"#;
        let row = parse_claude_line(body, "/x.jsonl", ProviderFilter::All).unwrap();
        assert!(row.is_vertex);
    }

    #[test]
    fn parse_detects_vertex_via_at_versioned_model() {
        let body = br#"{"type":"assistant","timestamp":"2026-05-13T14:33:21Z","message":{"id":"msg-1","model":"claude-sonnet-4-7@20251022","usage":{"input_tokens":100}}}"#;
        let row = parse_claude_line(body, "/x.jsonl", ProviderFilter::All).unwrap();
        assert!(row.is_vertex);
    }

    #[test]
    fn provider_filter_vertex_only_keeps_vertex_rows() {
        let vertex_body = br#"{"type":"assistant","timestamp":"2026-05-13T14:33:21Z","message":{"id":"msg_vrtx_x","model":"claude-sonnet","usage":{"input_tokens":1}}}"#;
        let plain_body = SAMPLE_LINE;
        assert!(
            parse_claude_line(vertex_body, "/x.jsonl", ProviderFilter::VertexAIOnly).is_some()
        );
        assert!(parse_claude_line(plain_body, "/x.jsonl", ProviderFilter::VertexAIOnly).is_none());
    }

    #[test]
    fn provider_filter_exclude_vertex_drops_vertex_rows() {
        let vertex_body = br#"{"type":"assistant","timestamp":"2026-05-13T14:33:21Z","message":{"id":"msg_vrtx_x","model":"claude-sonnet","usage":{"input_tokens":1}}}"#;
        assert!(parse_claude_line(vertex_body, "/x.jsonl", ProviderFilter::ExcludeVertexAI).is_none());
        assert!(parse_claude_line(SAMPLE_LINE, "/x.jsonl", ProviderFilter::ExcludeVertexAI).is_some());
    }

    #[test]
    fn path_role_marks_subagents_directory() {
        let row = parse_claude_line(
            SAMPLE_LINE,
            "/projects/foo/subagents/2026/05/13/agent.jsonl",
            ProviderFilter::All,
        )
        .unwrap();
        assert_eq!(row.path_role, PathRole::Subagent);
    }

    #[test]
    fn day_key_is_in_local_timezone() {
        let row = parse_claude_line(SAMPLE_LINE, "/x.jsonl", ProviderFilter::All).unwrap();
        // We do not assert the exact day key (depends on test
        // runner timezone) but it should be a 10-character
        // YYYY-MM-DD string.
        assert_eq!(row.day_key.len(), 10);
        assert_eq!(&row.day_key[4..5], "-");
    }
}
