//! pi (Practical Intelligence) session JSONL parser. Ported from
//! `docs/windows/spec/70-cost-scanning.md` §5.
//!
//! A pi session is a heterogeneous stream of `type="message"` turns
//! from multiple providers, interleaved with `type="model_change"`
//! context-updates. We attribute each assistant turn to Claude
//! (`anthropic`) or Codex (`openai-codex`); other providers are
//! dropped (they don't ship in the v1 popup card stack).
//!
//! Output: one `PiUsageRow` per assistant message, carrying the
//! attributed provider id, the resolved model, all four token axes,
//! and the local-time day bucket.

use chrono::{DateTime, FixedOffset, Local, TimeZone};
use serde_json::Value;

use super::claude_parser::MAX_LINE_BYTES;

/// Stable provider id strings used by the rest of the framework.
pub const PROVIDER_CLAUDE: &str = "claude";
pub const PROVIDER_CODEX: &str = "codex";

#[derive(Clone, Debug, PartialEq)]
pub struct PiUsageRow {
    pub provider_id: &'static str,
    pub day_key: String,
    pub timestamp_unix_secs: i64,
    pub model: String,
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub output_tokens: i64,
    /// `max(direct_total, input + cache_read + cache_write + output)`
    /// per spec §5.3. The aggregator prefers this when it diverges
    /// from the per-axis sum.
    pub total_tokens: i64,
    pub source_path: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PiModelContext {
    pub provider_id: Option<&'static str>,
    pub model: Option<String>,
}

/// Cheap byte filter mirroring the spec: a line must carry either a
/// `model_change` or a `message` type token before we pay for the
/// full JSON parse.
pub fn line_passes_prefilter(bytes: &[u8]) -> bool {
    if bytes.len() > MAX_LINE_BYTES {
        return false;
    }
    contains(bytes, b"\"type\":\"model_change\"")
        || contains(bytes, b"\"type\":\"message\"")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len()
        && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Streaming per-file scanner.
pub struct PiFileScanner {
    source_path: String,
    context: PiModelContext,
    emitted: Vec<PiUsageRow>,
}

impl PiFileScanner {
    pub fn new(source_path: impl Into<String>) -> Self {
        Self {
            source_path: source_path.into(),
            context: PiModelContext::default(),
            emitted: Vec::new(),
        }
    }

    pub fn take_rows(&mut self) -> Vec<PiUsageRow> {
        std::mem::take(&mut self.emitted)
    }

    pub fn context(&self) -> &PiModelContext {
        &self.context
    }

    pub fn feed_line(&mut self, bytes: &[u8]) -> usize {
        if !line_passes_prefilter(bytes) {
            return 0;
        }
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
            return 0;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("model_change") => {
                self.handle_model_change(&value);
                0
            }
            Some("message") => match self.handle_message(&value) {
                Some(row) => {
                    self.emitted.push(row);
                    1
                }
                None => 0,
            },
            _ => 0,
        }
    }

    fn handle_model_change(&mut self, value: &Value) {
        let provider = value
            .get("provider")
            .and_then(Value::as_str)
            .and_then(map_provider);
        let model = value
            .get("modelId")
            .or_else(|| value.get("model"))
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        if provider.is_none() {
            // Spec: "other providers are ignored (yields nil, so the
            // context is cleared)".
            self.context = PiModelContext::default();
            return;
        }
        self.context = PiModelContext {
            provider_id: provider,
            model,
        };
    }

    fn handle_message(&self, value: &Value) -> Option<PiUsageRow> {
        let message = value.get("message")?;
        let role = message.get("role").and_then(Value::as_str);
        if role != Some("assistant") {
            return None;
        }

        // Identity resolution per §5.2.
        let raw_provider = message
            .get("provider")
            .and_then(Value::as_str)
            .or_else(|| value.get("provider").and_then(Value::as_str));
        let mapped_provider = raw_provider.and_then(map_provider);
        // Spec rule 8: explicit provider text that maps to nothing
        // known is a drop.
        if raw_provider.is_some() && mapped_provider.is_none() {
            return None;
        }
        let explicit_model = message
            .get("model")
            .or_else(|| value.get("model"))
            .or_else(|| message.get("modelId"))
            .or_else(|| value.get("modelId"))
            .and_then(Value::as_str)
            .map(|s| s.to_string());

        let (provider_id, model) =
            match (mapped_provider, explicit_model.as_deref(), &self.context) {
                (Some(p), Some(m), _) => (p, m.to_string()),
                (Some(p), None, ctx) if ctx.provider_id == Some(p) => {
                    let m = ctx.model.clone()?;
                    (p, m)
                }
                (Some(_), None, _) => return None,
                (None, Some(m), ctx) => {
                    let p = ctx.provider_id?;
                    (p, m.to_string())
                }
                (None, None, ctx) => {
                    let p = ctx.provider_id?;
                    let m = ctx.model.clone()?;
                    (p, m)
                }
            };

        let usage = message.get("usage").or_else(|| value.get("usage"))?;
        let input = pick_i64(usage, &["input", "inputTokens", "input_tokens", "promptTokens", "prompt_tokens"]).unwrap_or(0);
        let cache_read = pick_i64(usage, &[
            "cacheRead",
            "cacheReadTokens",
            "cache_read",
            "cache_read_tokens",
            "cacheReadInputTokens",
            "cache_read_input_tokens",
        ]).unwrap_or(0);
        let cache_write = pick_i64(usage, &[
            "cacheWrite",
            "cacheWriteTokens",
            "cache_write",
            "cache_write_tokens",
            "cacheCreationTokens",
            "cache_creation_tokens",
            "cacheCreationInputTokens",
            "cache_creation_input_tokens",
        ]).unwrap_or(0);
        let output = pick_i64(usage, &["output", "outputTokens", "output_tokens", "completionTokens", "completion_tokens"]).unwrap_or(0);
        let direct_total = pick_i64(usage, &["totalTokens", "total_tokens", "tokenCount", "token_count", "tokens"]).unwrap_or(0);
        if input == 0 && cache_read == 0 && cache_write == 0 && output == 0 && direct_total == 0 {
            return None;
        }
        let sum = input + cache_read + cache_write + output;
        let total_tokens = direct_total.max(sum);

        let timestamp_raw = message
            .get("timestamp")
            .or_else(|| value.get("timestamp"));
        let (day_key, timestamp_unix_secs) = read_timestamp(timestamp_raw)?;

        Some(PiUsageRow {
            provider_id,
            day_key,
            timestamp_unix_secs,
            model,
            input_tokens: input,
            cache_read_tokens: cache_read,
            cache_write_tokens: cache_write,
            output_tokens: output,
            total_tokens,
            source_path: self.source_path.clone(),
        })
    }
}

fn map_provider(raw: &str) -> Option<&'static str> {
    match raw {
        "anthropic" => Some(PROVIDER_CLAUDE),
        "openai-codex" => Some(PROVIDER_CODEX),
        _ => None,
    }
}

fn pick_i64(obj: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(v) = obj.get(*key) {
            if let Some(n) = v.as_i64() {
                return Some(n);
            }
            if let Some(f) = v.as_f64() {
                return Some(f as i64);
            }
        }
    }
    None
}

fn read_timestamp(value: Option<&Value>) -> Option<(String, i64)> {
    let v = value?;
    let parsed_secs = if let Some(s) = v.as_str() {
        let parsed: DateTime<FixedOffset> = DateTime::parse_from_rfc3339(s).ok()?;
        parsed.timestamp()
    } else if let Some(n) = v.as_f64() {
        if n > 1e12 {
            (n / 1000.0) as i64
        } else {
            n as i64
        }
    } else if let Some(n) = v.as_i64() {
        if n > 1_000_000_000_000 {
            n / 1000
        } else {
            n
        }
    } else {
        return None;
    };
    let dt = Local.timestamp_opt(parsed_secs, 0).single()?;
    let day_key = dt.format("%Y-%m-%d").to_string();
    Some((day_key, parsed_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(scanner: &mut PiFileScanner, lines: &[&str]) {
        for line in lines {
            scanner.feed_line(line.as_bytes());
        }
    }

    #[test]
    fn explicit_anthropic_provider_plus_model_emits_claude_row() {
        let line = r#"{"type":"message","message":{"role":"assistant","provider":"anthropic","model":"claude-sonnet-4-7","timestamp":"2026-05-13T10:00:00Z","usage":{"input_tokens":100,"output_tokens":50}}}"#;
        let mut s = PiFileScanner::new("/x.jsonl");
        s.feed_line(line.as_bytes());
        let rows = s.take_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider_id, PROVIDER_CLAUDE);
        assert_eq!(rows[0].model, "claude-sonnet-4-7");
        assert_eq!(rows[0].input_tokens, 100);
        assert_eq!(rows[0].output_tokens, 50);
    }

    #[test]
    fn unknown_explicit_provider_drops_row() {
        let line = r#"{"type":"message","message":{"role":"assistant","provider":"some-other-vendor","model":"x","timestamp":"2026-05-13T10:00:00Z","usage":{"input_tokens":1}}}"#;
        let mut s = PiFileScanner::new("/x.jsonl");
        s.feed_line(line.as_bytes());
        assert!(s.take_rows().is_empty());
    }

    #[test]
    fn model_change_then_message_without_provider_inherits_context() {
        let change = r#"{"type":"model_change","provider":"anthropic","modelId":"claude-haiku-4-5"}"#;
        let msg = r#"{"type":"message","message":{"role":"assistant","timestamp":"2026-05-13T10:00:00Z","usage":{"input_tokens":42}}}"#;
        let mut s = PiFileScanner::new("/x.jsonl");
        feed_all(&mut s, &[change, msg]);
        let rows = s.take_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider_id, PROVIDER_CLAUDE);
        assert_eq!(rows[0].model, "claude-haiku-4-5");
    }

    #[test]
    fn model_change_to_unknown_provider_clears_context() {
        let change_known = r#"{"type":"model_change","provider":"anthropic","modelId":"claude-x"}"#;
        let change_unknown = r#"{"type":"model_change","provider":"future-llm","modelId":"x-1"}"#;
        let msg = r#"{"type":"message","message":{"role":"assistant","timestamp":"2026-05-13T10:00:00Z","usage":{"input_tokens":1}}}"#;
        let mut s = PiFileScanner::new("/x.jsonl");
        feed_all(&mut s, &[change_known, change_unknown, msg]);
        // Context was cleared, no provider to attribute → drop.
        assert!(s.take_rows().is_empty());
    }

    #[test]
    fn explicit_provider_matches_context_inherits_model() {
        let change = r#"{"type":"model_change","provider":"openai-codex","modelId":"gpt-5-codex"}"#;
        let msg = r#"{"type":"message","message":{"role":"assistant","provider":"openai-codex","timestamp":"2026-05-13T10:00:00Z","usage":{"input_tokens":99,"output_tokens":10}}}"#;
        let mut s = PiFileScanner::new("/x.jsonl");
        feed_all(&mut s, &[change, msg]);
        let rows = s.take_rows();
        assert_eq!(rows[0].provider_id, PROVIDER_CODEX);
        assert_eq!(rows[0].model, "gpt-5-codex");
    }

    #[test]
    fn explicit_provider_diverges_from_context_drops_without_explicit_model() {
        // Context says claude, message provider is codex but no
        // explicit model. We have no model to attribute → drop.
        let change = r#"{"type":"model_change","provider":"anthropic","modelId":"claude-x"}"#;
        let msg = r#"{"type":"message","message":{"role":"assistant","provider":"openai-codex","timestamp":"2026-05-13T10:00:00Z","usage":{"input_tokens":1}}}"#;
        let mut s = PiFileScanner::new("/x.jsonl");
        feed_all(&mut s, &[change, msg]);
        assert!(s.take_rows().is_empty());
    }

    #[test]
    fn usage_total_tokens_is_max_of_direct_and_summed() {
        let line = r#"{"type":"message","message":{"role":"assistant","provider":"anthropic","model":"claude-x","timestamp":"2026-05-13T10:00:00Z","usage":{"input_tokens":100,"output_tokens":50,"totalTokens":1000}}}"#;
        let mut s = PiFileScanner::new("/x.jsonl");
        s.feed_line(line.as_bytes());
        let rows = s.take_rows();
        // Direct total 1000 > sum 150 → take 1000.
        assert_eq!(rows[0].total_tokens, 1000);
    }

    #[test]
    fn usage_accepts_camel_case_keys_and_cache_axes() {
        let line = r#"{"type":"message","message":{"role":"assistant","provider":"anthropic","model":"claude-x","timestamp":"2026-05-13T10:00:00Z","usage":{"promptTokens":100,"cacheReadTokens":20,"cacheCreationTokens":30,"completionTokens":50}}}"#;
        let mut s = PiFileScanner::new("/x.jsonl");
        s.feed_line(line.as_bytes());
        let rows = s.take_rows();
        assert_eq!(rows[0].input_tokens, 100);
        assert_eq!(rows[0].cache_read_tokens, 20);
        assert_eq!(rows[0].cache_write_tokens, 30);
        assert_eq!(rows[0].output_tokens, 50);
    }

    #[test]
    fn timestamp_accepts_numeric_seconds_and_milliseconds() {
        let secs_line = r#"{"type":"message","message":{"role":"assistant","provider":"anthropic","model":"x","timestamp":1715593200,"usage":{"input_tokens":1}}}"#;
        let ms_line = r#"{"type":"message","message":{"role":"assistant","provider":"anthropic","model":"x","timestamp":1715593200000,"usage":{"input_tokens":1}}}"#;
        let mut s = PiFileScanner::new("/x.jsonl");
        s.feed_line(secs_line.as_bytes());
        s.feed_line(ms_line.as_bytes());
        let rows = s.take_rows();
        assert_eq!(rows.len(), 2);
        // Both should resolve to the same wall-clock instant.
        assert_eq!(rows[0].timestamp_unix_secs, rows[1].timestamp_unix_secs);
    }

    #[test]
    fn non_assistant_messages_are_dropped() {
        let line = r#"{"type":"message","message":{"role":"user","provider":"anthropic","model":"x","timestamp":"2026-05-13T10:00:00Z","usage":{"input_tokens":100}}}"#;
        let mut s = PiFileScanner::new("/x.jsonl");
        s.feed_line(line.as_bytes());
        assert!(s.take_rows().is_empty());
    }

    #[test]
    fn all_zero_usage_drops_row() {
        let line = r#"{"type":"message","message":{"role":"assistant","provider":"anthropic","model":"x","timestamp":"2026-05-13T10:00:00Z","usage":{"input_tokens":0,"output_tokens":0}}}"#;
        let mut s = PiFileScanner::new("/x.jsonl");
        s.feed_line(line.as_bytes());
        assert!(s.take_rows().is_empty());
    }

    #[test]
    fn prefilter_rejects_lines_with_no_type_keyword() {
        assert!(!line_passes_prefilter(b"{}"));
        assert!(!line_passes_prefilter(b"{\"foo\":1}"));
    }
}
