//! Per-cycle aggregator. Folds a stream of parsed rows into the
//! `ProviderCostSnapshot` the popup renders: a current-cycle USD
//! total, the prior cycle for comparison, a 30-day day series, and
//! a per-model breakdown.

use std::collections::BTreeMap;

use crate::providers::models::provider_cost::{ProviderCostSnapshot, ServiceCost};

use super::claude_parser::ClaudeUsageRow;
use super::dedup::{dedup_cross_file, dedup_in_file};
use super::pricing::{cost_for_row, normalise_model_id, PricingTable};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AggregatedCost {
    pub total_usd: f64,
    pub by_model_usd: BTreeMap<String, f64>,
    pub by_day_usd: BTreeMap<String, f64>,
    /// Per-day total token count (input + output + cache_read +
    /// cache_creation). Useful for the chart's hover panel.
    pub by_day_tokens: BTreeMap<String, i64>,
    /// `day -> model -> (cost_usd, total_tokens)` — populated so the
    /// chart hover panel can render up-to-four per-model rows for the
    /// selected day without a separate data fetch.
    pub by_day_models: BTreeMap<String, BTreeMap<String, (f64, i64)>>,
}

impl AggregatedCost {
    /// Convert to the framework's `ProviderCostSnapshot`. Selects the
    /// "current cycle" as everything in the supplied `current_cycle`
    /// day-key window and the optional "previous cycle" likewise.
    pub fn to_provider_snapshot(
        &self,
        current_cycle: &[&str],
        previous_cycle: Option<&[&str]>,
        last_30_days: &[&str],
    ) -> ProviderCostSnapshot {
        let current_total: f64 = current_cycle
            .iter()
            .filter_map(|d| self.by_day_usd.get(*d))
            .copied()
            .sum();
        let previous_total = previous_cycle.map(|days| {
            days.iter()
                .filter_map(|d| self.by_day_usd.get(*d))
                .copied()
                .sum::<f64>()
        });
        let last_30: Vec<f64> = last_30_days
            .iter()
            .map(|d| self.by_day_usd.get(*d).copied().unwrap_or(0.0))
            .collect();

        // Top 4 services by spend.
        let mut services: Vec<(String, f64)> = self
            .by_model_usd
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        services.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        let breakdown_by_service = services
            .into_iter()
            .take(4)
            .map(|(name, total)| ServiceCost {
                service_name: name,
                current_cycle_usd: total,
            })
            .collect();

        // Build per-day entries aligned with `last_30_days_usd`,
        // each populated with per-model rows sorted by cost desc.
        use crate::providers::models::provider_cost::{DailyCostEntry, ModelCost};
        let daily: Vec<DailyCostEntry> = last_30_days
            .iter()
            .map(|d| {
                let cost = self.by_day_usd.get(*d).copied().unwrap_or(0.0);
                let total_tokens = self.by_day_tokens.get(*d).copied().unwrap_or(0);
                let mut models: Vec<ModelCost> = self
                    .by_day_models
                    .get(*d)
                    .map(|m| {
                        m.iter()
                            .map(|(id, (cost, tokens))| ModelCost {
                                model_id: id.clone(),
                                cost_usd: *cost,
                                total_tokens: *tokens,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                models.sort_by(|a, b| {
                    b.cost_usd
                        .partial_cmp(&a.cost_usd)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(b.total_tokens.cmp(&a.total_tokens))
                        .then(a.model_id.cmp(&b.model_id))
                });
                DailyCostEntry {
                    date: (*d).to_string(),
                    cost_usd: cost,
                    total_tokens,
                    models,
                }
            })
            .collect();
        let total_window_usd = last_30.iter().sum::<f64>();
        let updated_at_unix_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        ProviderCostSnapshot {
            current_cycle_usd: current_total,
            previous_cycle_usd: previous_total,
            last_30_days_usd: last_30,
            daily,
            total_window_usd,
            updated_at_unix_secs,
            breakdown_by_service,
        }
    }
}

/// Build an `AggregatedCost` from a list of files' rows. Applies the
/// full dedup pipeline (in-file then cross-file) before aggregation,
/// then folds rows into per-day + per-day-per-model rollups so the
/// popup's hover panel can render without a follow-up call.
pub fn aggregate_rows(files: Vec<Vec<ClaudeUsageRow>>, pricing: &PricingTable) -> AggregatedCost {
    let mut deduped: Vec<ClaudeUsageRow> = Vec::new();
    for file_rows in files {
        deduped.extend(dedup_in_file(file_rows));
    }
    let deduped = dedup_cross_file(deduped);
    let mut by_model_usd: BTreeMap<String, f64> = BTreeMap::new();
    let mut by_day_usd: BTreeMap<String, f64> = BTreeMap::new();
    let mut by_day_tokens: BTreeMap<String, i64> = BTreeMap::new();
    let mut by_day_models: BTreeMap<String, BTreeMap<String, (f64, i64)>> = BTreeMap::new();
    let mut total_usd = 0.0;
    for row in deduped {
        let usd = cost_for_row(pricing, &row).unwrap_or(0.0);
        let tokens = row.input_tokens
            + row.output_tokens
            + row.cache_read_input_tokens
            + row.cache_creation_input_tokens;
        let model_id = normalise_model_id(&row.model);
        total_usd += usd;
        *by_model_usd.entry(model_id.clone()).or_default() += usd;
        *by_day_usd.entry(row.day_key.clone()).or_default() += usd;
        *by_day_tokens.entry(row.day_key.clone()).or_default() += tokens;
        let day_models = by_day_models.entry(row.day_key.clone()).or_default();
        let entry = day_models.entry(model_id).or_insert((0.0, 0));
        entry.0 += usd;
        entry.1 += tokens;
    }
    AggregatedCost {
        total_usd,
        by_model_usd,
        by_day_usd,
        by_day_tokens,
        by_day_models,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost::claude_parser::PathRole;

    fn row(
        message_id: &str,
        request_id: &str,
        model: &str,
        day: &str,
        input: i64,
        output: i64,
    ) -> ClaudeUsageRow {
        ClaudeUsageRow {
            day_key: day.into(),
            timestamp_unix_secs: 0,
            model: model.into(),
            input_tokens: input,
            output_tokens: output,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
            message_id: Some(message_id.into()),
            request_id: Some(request_id.into()),
            session_id: Some("sess-1".into()),
            is_sidechain: false,
            is_vertex: false,
            path_role: PathRole::Parent,
            source_path: "/a.jsonl".into(),
        }
    }

    #[test]
    fn aggregator_sums_costs_per_model_per_day() {
        let pricing = PricingTable::anthropic_default();
        let rows = vec![
            row(
                "m1",
                "r1",
                "claude-3-5-sonnet-20251022",
                "2026-05-13",
                1_000_000,
                0,
            ),
            row(
                "m2",
                "r2",
                "claude-haiku-4-5-20251015",
                "2026-05-13",
                1_000_000,
                0,
            ),
            row(
                "m3",
                "r3",
                "claude-3-5-sonnet-20251022",
                "2026-05-14",
                500_000,
                100_000,
            ),
        ];
        let agg = aggregate_rows(vec![rows], &pricing);
        // sonnet: 1M*3 + (500k*3 + 100k*15)/1e6 = 3 + 1.5 + 1.5 = 6.00
        // haiku:  1M*1 = 1.00
        // total = 7.00
        assert!((agg.total_usd - 7.0).abs() < 1e-9);
        assert!(
            (agg.by_model_usd
                .get("claude-3-5-sonnet")
                .copied()
                .unwrap_or(0.0)
                - 6.0)
                .abs()
                < 1e-9
        );
        assert!(
            (agg.by_model_usd
                .get("claude-haiku-4-5")
                .copied()
                .unwrap_or(0.0)
                - 1.0)
                .abs()
                < 1e-9
        );
        let day_5_13 = agg.by_day_usd.get("2026-05-13").copied().unwrap();
        assert!((day_5_13 - 4.0).abs() < 1e-9);
    }

    #[test]
    fn aggregator_dedups_across_files_using_canonical_key() {
        let pricing = PricingTable::anthropic_default();
        // Same canonical key (sess-1, m1, r1), two files. Parent
        // should win over subagent.
        let parent = row(
            "m1",
            "r1",
            "claude-3-5-sonnet-20251022",
            "2026-05-13",
            1_000_000,
            0,
        );
        let mut subagent = parent.clone();
        subagent.path_role = PathRole::Subagent;
        subagent.source_path = "/a/sub.jsonl".into();
        subagent.input_tokens = 999_999; // smoke
        let agg = aggregate_rows(vec![vec![parent], vec![subagent]], &pricing);
        // Only one row contributes; parent wins ⇒ 1M*3 = $3.00.
        assert!((agg.total_usd - 3.0).abs() < 1e-9);
    }

    #[test]
    fn aggregator_dedups_in_file_streaming_chunks() {
        let pricing = PricingTable::anthropic_default();
        // Two chunks of the same message/request: only the final one
        // should be counted.
        let early = row(
            "m1",
            "r1",
            "claude-3-5-sonnet-20251022",
            "2026-05-13",
            500_000,
            0,
        );
        let final_ = row(
            "m1",
            "r1",
            "claude-3-5-sonnet-20251022",
            "2026-05-13",
            1_000_000,
            0,
        );
        let agg = aggregate_rows(vec![vec![early, final_]], &pricing);
        // Only $3.00 from the 1M cumulative.
        assert!((agg.total_usd - 3.0).abs() < 1e-9);
    }

    #[test]
    fn to_provider_snapshot_picks_only_current_cycle_days() {
        let pricing = PricingTable::anthropic_default();
        let rows = vec![
            row(
                "m1",
                "r1",
                "claude-3-5-sonnet-20251022",
                "2026-05-12",
                1_000_000,
                0,
            ),
            row(
                "m2",
                "r2",
                "claude-3-5-sonnet-20251022",
                "2026-05-13",
                2_000_000,
                0,
            ),
            row(
                "m3",
                "r3",
                "claude-3-5-sonnet-20251022",
                "2026-05-14",
                500_000,
                0,
            ),
        ];
        let agg = aggregate_rows(vec![rows], &pricing);
        // Current cycle = May 13-14: $6 + $1.5 = $7.5.
        let snap = agg.to_provider_snapshot(
            &["2026-05-13", "2026-05-14"],
            Some(&["2026-05-12"]),
            &["2026-05-12", "2026-05-13", "2026-05-14"],
        );
        assert!((snap.current_cycle_usd - 7.5).abs() < 1e-9);
        assert!((snap.previous_cycle_usd.unwrap() - 3.0).abs() < 1e-9);
        assert_eq!(snap.last_30_days_usd.len(), 3);
        assert!((snap.last_30_days_usd[1] - 6.0).abs() < 1e-9);
    }

    #[test]
    fn to_provider_snapshot_breakdown_sorted_by_spend_desc() {
        let pricing = PricingTable::anthropic_default();
        let rows = vec![
            row(
                "m1",
                "r1",
                "claude-haiku-4-5-20251015",
                "2026-05-13",
                1_000_000,
                0,
            ),
            row(
                "m2",
                "r2",
                "claude-3-5-sonnet-20251022",
                "2026-05-13",
                1_000_000,
                0,
            ),
        ];
        let agg = aggregate_rows(vec![rows], &pricing);
        let snap = agg.to_provider_snapshot(&["2026-05-13"], None, &["2026-05-13"]);
        let names: Vec<&str> = snap
            .breakdown_by_service
            .iter()
            .map(|s| s.service_name.as_str())
            .collect();
        // sonnet $3 > haiku $1 → sonnet first.
        assert_eq!(names, vec!["claude-3-5-sonnet", "claude-haiku-4-5"]);
    }

    #[test]
    fn unknown_model_contributes_zero_dollars_but_still_dedup_counted() {
        let pricing = PricingTable::anthropic_default();
        let rows = vec![
            row(
                "m1",
                "r1",
                "future-experimental-model",
                "2026-05-13",
                1_000_000,
                0,
            ),
            row(
                "m2",
                "r2",
                "claude-3-5-sonnet-20251022",
                "2026-05-13",
                1_000_000,
                0,
            ),
        ];
        let agg = aggregate_rows(vec![rows], &pricing);
        assert!((agg.total_usd - 3.0).abs() < 1e-9);
        assert!(agg.by_model_usd.contains_key("future-experimental-model"));
    }
}
