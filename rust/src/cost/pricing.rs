//! Per-model rate table. Rates are USD per million tokens
//! ($/MTok) for each of four token categories. Source: Anthropic's
//! public pricing page (2025-12 snapshot, prices below stable for at
//! least one billing cycle).

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RatePerMTok {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_creation: f64,
}

#[derive(Clone, Debug)]
pub struct PricingTable {
    /// Maps `(case-folded model id, exact form first)` to rate.
    /// `lookup` walks fallbacks; the raw map is exposed for tests.
    pub rates: HashMap<String, RatePerMTok>,
}

impl PricingTable {
    /// Build the default Anthropic pricing table. Numbers are in
    /// $/MTok per Anthropic's public table.
    pub fn anthropic_default() -> Self {
        let mut rates = HashMap::new();
        // Opus 4.x family.
        rates.insert(
            "claude-opus-4-1".into(),
            RatePerMTok {
                input: 15.00,
                output: 75.00,
                cache_read: 1.50,
                cache_creation: 18.75,
            },
        );
        rates.insert(
            "claude-opus-4".into(),
            RatePerMTok {
                input: 15.00,
                output: 75.00,
                cache_read: 1.50,
                cache_creation: 18.75,
            },
        );
        // Sonnet 4.x family.
        rates.insert(
            "claude-sonnet-4-7".into(),
            RatePerMTok {
                input: 3.00,
                output: 15.00,
                cache_read: 0.30,
                cache_creation: 3.75,
            },
        );
        rates.insert(
            "claude-sonnet-4-6".into(),
            RatePerMTok {
                input: 3.00,
                output: 15.00,
                cache_read: 0.30,
                cache_creation: 3.75,
            },
        );
        rates.insert(
            "claude-sonnet-4-5".into(),
            RatePerMTok {
                input: 3.00,
                output: 15.00,
                cache_read: 0.30,
                cache_creation: 3.75,
            },
        );
        // Haiku 4.x family.
        rates.insert(
            "claude-haiku-4-5".into(),
            RatePerMTok {
                input: 1.00,
                output: 5.00,
                cache_read: 0.10,
                cache_creation: 1.25,
            },
        );
        // Legacy 3.5/3 fallbacks.
        rates.insert(
            "claude-3-5-sonnet".into(),
            RatePerMTok {
                input: 3.00,
                output: 15.00,
                cache_read: 0.30,
                cache_creation: 3.75,
            },
        );
        rates.insert(
            "claude-3-5-haiku".into(),
            RatePerMTok {
                input: 0.80,
                output: 4.00,
                cache_read: 0.08,
                cache_creation: 1.00,
            },
        );
        rates.insert(
            "claude-3-opus".into(),
            RatePerMTok {
                input: 15.00,
                output: 75.00,
                cache_read: 1.50,
                cache_creation: 18.75,
            },
        );
        Self { rates }
    }

    /// Resolve a model id like `claude-sonnet-4-7-20251022` or
    /// `claude-sonnet-4-7@20251022` (Vertex AI form) to a rate. The
    /// suffix is stripped on either `-YYYYMMDD` or `@…` so we match
    /// the canonical family key.
    pub fn lookup(&self, raw_model: &str) -> Option<RatePerMTok> {
        let normalised = normalise_model_id(raw_model);
        // Try in priority: exact, then progressively shorter family.
        if let Some(rate) = self.rates.get(&normalised) {
            return Some(*rate);
        }
        // Walk parent families: strip the last `-<segment>` repeatedly
        // until either a match or we run out.
        let mut cursor: &str = &normalised;
        while let Some(idx) = cursor.rfind('-') {
            cursor = &cursor[..idx];
            if let Some(rate) = self.rates.get(cursor) {
                return Some(*rate);
            }
        }
        None
    }
}

/// Canonicalise a raw model id. Strips the trailing date stamp
/// (Anthropic: `-YYYYMMDD`, Vertex AI: `@YYYYMMDD`) and lower-cases.
pub fn normalise_model_id(raw: &str) -> String {
    let trimmed = raw.trim().to_ascii_lowercase();
    // Vertex form: `claude-x@version`.
    if let Some(idx) = trimmed.find('@') {
        return trimmed[..idx].to_string();
    }
    // Anthropic form: `claude-x-y-YYYYMMDD`. Strip the trailing
    // numeric block when at least 8 digits.
    let bytes = trimmed.as_bytes();
    if let Some(stripped) = strip_trailing_date(bytes) {
        return stripped.to_string();
    }
    trimmed
}

fn strip_trailing_date(bytes: &[u8]) -> Option<&str> {
    // The trailing date is "-NNNNNNNN" where N is 0-9 and length 6+.
    let len = bytes.len();
    let mut i = len;
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == len {
        return None;
    }
    let digit_count = len - i;
    if digit_count < 6 || i == 0 || bytes[i - 1] != b'-' {
        return None;
    }
    std::str::from_utf8(&bytes[..i - 1]).ok()
}

/// Compute the USD cost contribution of one parsed row given a
/// pricing table. Returns `None` if the model is unknown.
pub fn cost_for_row(table: &PricingTable, row: &super::ClaudeUsageRow) -> Option<f64> {
    let rate = table.lookup(&row.model)?;
    let scale = |tokens: i64, rate: f64| (tokens as f64 / 1_000_000.0) * rate;
    let dollars = scale(row.input_tokens, rate.input)
        + scale(row.output_tokens, rate.output)
        + scale(row.cache_read_input_tokens, rate.cache_read)
        + scale(row.cache_creation_input_tokens, rate.cache_creation);
    Some(dollars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost::claude_parser::{ClaudeUsageRow, PathRole};

    fn row(model: &str, input: i64, output: i64) -> ClaudeUsageRow {
        ClaudeUsageRow {
            day_key: "2026-05-13".into(),
            timestamp_unix_secs: 0,
            model: model.into(),
            input_tokens: input,
            output_tokens: output,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
            message_id: None,
            request_id: None,
            session_id: None,
            is_sidechain: false,
            is_vertex: false,
            path_role: PathRole::Parent,
            source_path: "/tmp/x.jsonl".into(),
        }
    }

    #[test]
    fn normalises_anthropic_dated_model_ids() {
        assert_eq!(
            normalise_model_id("claude-sonnet-4-7-20251022"),
            "claude-sonnet-4-7"
        );
        assert_eq!(
            normalise_model_id("Claude-Haiku-4-5-20251015"),
            "claude-haiku-4-5"
        );
    }

    #[test]
    fn normalises_vertex_at_versioned_model_ids() {
        assert_eq!(
            normalise_model_id("claude-sonnet-4-7@20251022"),
            "claude-sonnet-4-7"
        );
    }

    #[test]
    fn lookup_matches_exact_family_after_normalisation() {
        let table = PricingTable::anthropic_default();
        let rate = table.lookup("claude-sonnet-4-7-20251022").unwrap();
        assert_eq!(rate.input, 3.00);
        assert_eq!(rate.output, 15.00);
    }

    #[test]
    fn lookup_walks_parent_families_when_exact_missing() {
        let table = PricingTable::anthropic_default();
        // `claude-opus-4-1-20251020` → exact `claude-opus-4-1` hit.
        assert!(table.lookup("claude-opus-4-1-20251020").is_some());
        // Unknown sub-family falls back to a parent if registered.
        // `claude-3-5-sonnet-experimental` → walks down to
        // `claude-3-5-sonnet`.
        assert!(table
            .lookup("claude-3-5-sonnet-experimental")
            .is_some());
    }

    #[test]
    fn lookup_returns_none_for_unknown_families() {
        let table = PricingTable::anthropic_default();
        assert!(table.lookup("openai-gpt-5").is_none());
    }

    #[test]
    fn cost_for_row_uses_rates_at_per_million_scale() {
        let table = PricingTable::anthropic_default();
        // 1M input tokens of sonnet-4-5 at $3/MTok = $3.00.
        let r = row("claude-sonnet-4-5-20250101", 1_000_000, 0);
        assert_eq!(cost_for_row(&table, &r), Some(3.00));
        // Combined input + output.
        let r = row("claude-sonnet-4-5-20250101", 500_000, 200_000);
        let expected = (500_000_f64 / 1_000_000.0) * 3.00
            + (200_000_f64 / 1_000_000.0) * 15.00;
        let actual = cost_for_row(&table, &r).unwrap();
        assert!((actual - expected).abs() < 1e-9);
    }

    #[test]
    fn cost_for_row_returns_none_for_unknown_model() {
        let table = PricingTable::anthropic_default();
        let r = row("future-model", 1_000_000, 0);
        assert!(cost_for_row(&table, &r).is_none());
    }

    #[test]
    fn cache_read_and_creation_rates_apply() {
        let table = PricingTable::anthropic_default();
        let mut r = row("claude-sonnet-4-5-20250101", 0, 0);
        r.cache_read_input_tokens = 1_000_000; // $0.30
        r.cache_creation_input_tokens = 1_000_000; // $3.75
        assert!((cost_for_row(&table, &r).unwrap() - 4.05).abs() < 1e-9);
    }
}
