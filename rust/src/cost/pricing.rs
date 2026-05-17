//! Per-model rate table. Rates are USD per million tokens
//! ($/MTok) for each of four token categories.
//!
//! Mirrors the macOS source's `CostUsagePricing.swift`. The macOS app
//! uses per-token rates (e.g. `1.25e-6` for `gpt-5` input); we use
//! per-MTok rates so the numbers in source read like the public
//! pricing pages (`$1.25 / 1M tokens`). Multiply per-token by
//! 1_000_000 to get the per-MTok form.
//!
//! Sources:
//! - OpenAI public pricing page + macOS `CostUsagePricing.swift`
//!   `codex: [String: CodexPricing]` block.
//! - Anthropic public pricing page + macOS `claude: [String: ClaudePricing]`.
//!
//! Tiered pricing (Sonnet 4.5/4.6 doubling past 200k tokens) is
//! captured via `AboveThresholdRates`. Models without a tier set
//! that field to `None`.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RatePerMTok {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_creation: f64,
    /// Tier-2 rates that apply once cumulative tokens exceed
    /// `threshold_tokens`. Anthropic uses this for Sonnet 4.5/4.6
    /// past 200k tokens. `None` means a single flat rate.
    pub above_threshold: Option<AboveThresholdRates>,
}

/// Anthropic-style tiered rate: once cumulative input tokens cross
/// `threshold_tokens` in a single request, everything above gets
/// charged at the higher `*` rate. Mirrors `*CostPerTokenAboveThreshold`
/// fields on the macOS `ClaudePricing` struct.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AboveThresholdRates {
    pub threshold_tokens: i64,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_creation: f64,
}

impl RatePerMTok {
    const fn flat(input: f64, output: f64, cache_read: f64, cache_creation: f64) -> Self {
        Self {
            input,
            output,
            cache_read,
            cache_creation,
            above_threshold: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PricingTable {
    /// Maps `(case-folded model id, exact form first)` to rate.
    /// `lookup` walks fallbacks; the raw map is exposed for tests.
    pub rates: HashMap<String, RatePerMTok>,
}

impl Default for PricingTable {
    /// Combined Anthropic + OpenAI pricing table. The lookup is
    /// model-id based so a single table serves both ecosystems.
    fn default() -> Self {
        let mut t = Self::anthropic_default();
        t.merge(Self::openai_default());
        t
    }
}

impl PricingTable {
    fn merge(&mut self, other: Self) {
        self.rates.extend(other.rates);
    }

    /// Anthropic pricing table — public per-1M-token rates.
    pub fn anthropic_default() -> Self {
        // Sonnet 4.x: $3 input / $15 output / $0.30 cache-read /
        //             $3.75 cache-creation; doubles past 200k tokens.
        let sonnet_4 = RatePerMTok {
            input: 3.00,
            output: 15.00,
            cache_read: 0.30,
            cache_creation: 3.75,
            above_threshold: Some(AboveThresholdRates {
                threshold_tokens: 200_000,
                input: 6.00,
                output: 22.50,
                cache_read: 0.60,
                cache_creation: 7.50,
            }),
        };

        let opus_4_5 = RatePerMTok::flat(5.00, 25.00, 0.50, 6.25);
        let opus_4_1 = RatePerMTok::flat(15.00, 75.00, 1.50, 18.75);
        let haiku_4_5 = RatePerMTok::flat(1.00, 5.00, 0.10, 1.25);

        let mut rates = HashMap::new();

        // Opus 4.x family — Opus 4.5 / 4.6 / 4.7 share rates; legacy
        // 4 / 4.1 use the higher tier from May 2025.
        rates.insert("claude-opus-4-7".into(), opus_4_5);
        rates.insert("claude-opus-4-6".into(), opus_4_5);
        rates.insert("claude-opus-4-6-20260205".into(), opus_4_5);
        rates.insert("claude-opus-4-5".into(), opus_4_5);
        rates.insert("claude-opus-4-5-20251101".into(), opus_4_5);
        rates.insert("claude-opus-4-1".into(), opus_4_1);
        rates.insert("claude-opus-4-20250514".into(), opus_4_1);
        rates.insert("claude-opus-4".into(), opus_4_1);

        // Sonnet 4.x family — same rates, all share the 200k tier.
        rates.insert("claude-sonnet-4-7".into(), sonnet_4);
        rates.insert("claude-sonnet-4-6".into(), sonnet_4);
        rates.insert("claude-sonnet-4-5".into(), sonnet_4);
        rates.insert("claude-sonnet-4-5-20250929".into(), sonnet_4);
        rates.insert("claude-sonnet-4-20250514".into(), sonnet_4);

        // Haiku 4.5.
        rates.insert("claude-haiku-4-5".into(), haiku_4_5);
        rates.insert("claude-haiku-4-5-20251001".into(), haiku_4_5);

        // Legacy 3.5 / 3 fallbacks.
        rates.insert(
            "claude-3-5-sonnet".into(),
            RatePerMTok::flat(3.00, 15.00, 0.30, 3.75),
        );
        rates.insert(
            "claude-3-5-haiku".into(),
            RatePerMTok::flat(0.80, 4.00, 0.08, 1.00),
        );
        rates.insert(
            "claude-3-opus".into(),
            RatePerMTok::flat(15.00, 75.00, 1.50, 18.75),
        );

        Self { rates }
    }

    /// OpenAI / Codex pricing table — per-1M-token rates, no
    /// cache-creation concept (Codex doesn't expose one), no tiers.
    pub fn openai_default() -> Self {
        // Helper because Codex doesn't have a cache-creation rate;
        // we pass 0 and rely on the parser to never populate cache-
        // creation tokens for Codex rows.
        let codex = |input: f64, output: f64, cache_read: f64| RatePerMTok {
            input,
            output,
            cache_read,
            cache_creation: 0.0,
            above_threshold: None,
        };

        let mut rates = HashMap::new();
        rates.insert("gpt-5".into(), codex(1.25, 10.00, 0.125));
        rates.insert("gpt-5-codex".into(), codex(1.25, 10.00, 0.125));
        rates.insert("gpt-5-mini".into(), codex(0.25, 2.00, 0.025));
        rates.insert("gpt-5-nano".into(), codex(0.05, 0.40, 0.005));
        rates.insert("gpt-5-pro".into(), codex(15.00, 120.00, 0.0));
        rates.insert("gpt-5.1".into(), codex(1.25, 10.00, 0.125));
        rates.insert("gpt-5.1-codex".into(), codex(1.25, 10.00, 0.125));
        rates.insert("gpt-5.1-codex-max".into(), codex(1.25, 10.00, 0.125));
        rates.insert("gpt-5.1-codex-mini".into(), codex(0.25, 2.00, 0.025));
        rates.insert("gpt-5.2".into(), codex(1.75, 14.00, 0.175));
        rates.insert("gpt-5.2-codex".into(), codex(1.75, 14.00, 0.175));
        rates.insert("gpt-5.2-pro".into(), codex(21.00, 168.00, 0.0));
        rates.insert("gpt-5.3-codex".into(), codex(1.75, 14.00, 0.175));
        // Research preview — explicitly free.
        rates.insert("gpt-5.3-codex-spark".into(), codex(0.0, 0.0, 0.0));
        rates.insert("gpt-5.4".into(), codex(2.50, 15.00, 0.25));
        rates.insert("gpt-5.4-mini".into(), codex(0.75, 4.50, 0.075));
        rates.insert("gpt-5.4-nano".into(), codex(0.20, 1.25, 0.02));
        rates.insert("gpt-5.4-pro".into(), codex(30.00, 180.00, 0.0));
        rates.insert("gpt-5.5".into(), codex(5.00, 30.00, 0.50));
        rates.insert("gpt-5.5-pro".into(), codex(30.00, 180.00, 0.0));
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

    /// Display label for a model id when the pricing entry overrides
    /// the default formatter (e.g. `gpt-5.3-codex-spark` →
    /// "Research Preview"). Returns the model id itself for the
    /// common path.
    pub fn display_label_for(model: &str) -> Option<&'static str> {
        match normalise_model_id(model).as_str() {
            "gpt-5.3-codex-spark" => Some("Research Preview"),
            _ => None,
        }
    }
}

/// Canonicalise a raw model id. Strips the trailing date stamp
/// (Anthropic: `-YYYYMMDD`, Vertex AI: `@YYYYMMDD`) and lower-cases.
/// Also strips a `openai/` or `anthropic.` provider prefix so models
/// piped through OpenRouter / Bedrock land on the same key.
pub fn normalise_model_id(raw: &str) -> String {
    let mut trimmed = raw.trim().to_ascii_lowercase();

    // Provider prefixes used by some routers.
    if let Some(rest) = trimmed.strip_prefix("openai/") {
        trimmed = rest.to_string();
    } else if let Some(rest) = trimmed.strip_prefix("anthropic.") {
        trimmed = rest.to_string();
    }

    // Vertex form: `claude-x@version`.
    if let Some(idx) = trimmed.find('@') {
        return trimmed[..idx].to_string();
    }
    // Anthropic form: `claude-x-y-YYYYMMDD`. Strip the trailing
    // numeric block when at least 6 digits.
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
///
/// Tiered Claude pricing: when the row's input tokens exceed
/// `threshold_tokens`, the portion above the threshold is charged at
/// the higher `above_threshold.*` rate. The threshold is applied
/// independently to each of the four token categories.
pub fn cost_for_row(table: &PricingTable, row: &super::ClaudeUsageRow) -> Option<f64> {
    let rate = table.lookup(&row.model)?;
    let scale = |tokens: i64, rate: f64| (tokens as f64 / 1_000_000.0) * rate;
    let tiered = |tokens: i64, base: f64, tier: Option<(i64, f64)>| -> f64 {
        match tier {
            Some((threshold, above_rate)) if tokens > threshold => {
                let below = threshold;
                let above = tokens - threshold;
                scale(below, base) + scale(above, above_rate)
            }
            _ => scale(tokens, base),
        }
    };

    let tier = rate.above_threshold;
    let dollars = tiered(
        row.input_tokens,
        rate.input,
        tier.map(|t| (t.threshold_tokens, t.input)),
    ) + tiered(
        row.output_tokens,
        rate.output,
        tier.map(|t| (t.threshold_tokens, t.output)),
    ) + tiered(
        row.cache_read_input_tokens,
        rate.cache_read,
        tier.map(|t| (t.threshold_tokens, t.cache_read)),
    ) + tiered(
        row.cache_creation_input_tokens,
        rate.cache_creation,
        tier.map(|t| (t.threshold_tokens, t.cache_creation)),
    );
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
    fn normalises_openrouter_prefixed_model_ids() {
        assert_eq!(normalise_model_id("openai/gpt-5-codex"), "gpt-5-codex");
        assert_eq!(
            normalise_model_id("anthropic.claude-sonnet-4-5"),
            "claude-sonnet-4-5"
        );
    }

    #[test]
    fn lookup_matches_exact_family_after_normalisation() {
        let table = PricingTable::default();
        let rate = table.lookup("claude-sonnet-4-7-20251022").unwrap();
        assert_eq!(rate.input, 3.00);
        assert_eq!(rate.output, 15.00);
    }

    #[test]
    fn lookup_walks_parent_families_when_exact_missing() {
        let table = PricingTable::default();
        // `claude-opus-4-1-20251020` → exact `claude-opus-4-1` hit.
        assert!(table.lookup("claude-opus-4-1-20251020").is_some());
        // Unknown sub-family falls back to a parent if registered.
        assert!(table.lookup("claude-3-5-sonnet-experimental").is_some());
    }

    #[test]
    fn lookup_returns_none_for_unknown_families() {
        let table = PricingTable::default();
        assert!(table.lookup("zephyr-9000").is_none());
    }

    #[test]
    fn openai_codex_models_resolve() {
        let table = PricingTable::default();
        let r = table.lookup("gpt-5").unwrap();
        assert_eq!(r.input, 1.25);
        assert_eq!(r.output, 10.00);

        let mini = table.lookup("gpt-5-mini").unwrap();
        assert_eq!(mini.input, 0.25);
        assert_eq!(mini.output, 2.00);

        let spark = table.lookup("gpt-5.3-codex-spark").unwrap();
        assert_eq!(spark.input, 0.0);
        assert_eq!(spark.output, 0.0);
    }

    #[test]
    fn display_label_for_research_preview() {
        assert_eq!(
            PricingTable::display_label_for("gpt-5.3-codex-spark"),
            Some("Research Preview"),
        );
        assert!(PricingTable::display_label_for("gpt-5").is_none());
    }

    #[test]
    fn cost_for_row_uses_rates_at_per_million_scale() {
        let table = PricingTable::default();
        // Use 3.5 sonnet — same nominal rates as 4.5 but no 200k
        // tier so the math is a clean ($/MTok × tokens / 1M) check.
        let r = row("claude-3-5-sonnet-20250101", 1_000_000, 0);
        assert_eq!(cost_for_row(&table, &r), Some(3.00));
        // Combined input + output.
        let r = row("claude-3-5-sonnet-20250101", 500_000, 200_000);
        let expected = (500_000_f64 / 1_000_000.0) * 3.00 + (200_000_f64 / 1_000_000.0) * 15.00;
        let actual = cost_for_row(&table, &r).unwrap();
        assert!((actual - expected).abs() < 1e-9);
    }

    #[test]
    fn cost_for_row_returns_none_for_unknown_model() {
        let table = PricingTable::default();
        let r = row("future-model", 1_000_000, 0);
        assert!(cost_for_row(&table, &r).is_none());
    }

    #[test]
    fn cache_read_and_creation_rates_apply() {
        let table = PricingTable::default();
        // 3.5 sonnet is non-tiered so the cache math is unambiguous.
        let mut r = row("claude-3-5-sonnet-20250101", 0, 0);
        r.cache_read_input_tokens = 1_000_000; // $0.30
        r.cache_creation_input_tokens = 1_000_000; // $3.75
        assert!((cost_for_row(&table, &r).unwrap() - 4.05).abs() < 1e-9);
    }

    #[test]
    fn sonnet_above_threshold_doubles_rate() {
        let table = PricingTable::default();
        // 300k input tokens: 200k @ $3/MTok + 100k @ $6/MTok = $0.60 + $0.60 = $1.20.
        let r = row("claude-sonnet-4-5", 300_000, 0);
        let expected = 0.6 + 0.6;
        let actual = cost_for_row(&table, &r).unwrap();
        assert!(
            (actual - expected).abs() < 1e-9,
            "got {actual}, expected {expected}"
        );
    }

    #[test]
    fn codex_gpt_5_costs() {
        let table = PricingTable::default();
        // 1M input tokens of gpt-5 at $1.25/MTok = $1.25.
        let r = row("gpt-5", 1_000_000, 0);
        let actual = cost_for_row(&table, &r).unwrap();
        assert!((actual - 1.25).abs() < 1e-9);
    }
}
