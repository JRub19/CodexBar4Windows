//! Cost snapshot per spec 30 section 12.6 + Cost History feature.
//!
//! Providers that bill by usage emit one of these per refresh so the
//! popup can render the cost chart. The shape mirrors the macOS
//! `CostUsageTokenSnapshot`: total, day series, per-model breakdown
//! per day so hover details have everything they need without a
//! second round-trip.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderCostSnapshot {
    /// Total cost in the current billing cycle, USD.
    pub current_cycle_usd: f64,
    /// Total cost in the previous cycle for comparison, USD.
    pub previous_cycle_usd: Option<f64>,
    /// Daily series for the last 30 days; index 0 is the oldest.
    /// Each entry is the day's USD total; per-model detail lives in
    /// `daily` below alongside the date key. Both arrays are aligned
    /// to the same window — this one stays for back-compat with
    /// existing UI code.
    pub last_30_days_usd: Vec<f64>,
    /// Per-day entries for the rolling window (length matches
    /// `last_30_days_usd`). Includes per-model breakdown so the
    /// chart's hover panel renders without another fetch.
    #[serde(default)]
    pub daily: Vec<DailyCostEntry>,
    /// Total cost across the entire `daily` window (matches the sum of
    /// `last_30_days_usd`). Cached so the UI doesn't recompute.
    #[serde(default)]
    pub total_window_usd: f64,
    /// Wall-clock seconds since the unix epoch when this snapshot was
    /// produced. The UI uses this to label "Updated 3 min ago".
    #[serde(default)]
    pub updated_at_unix_secs: i64,
    /// Per-service breakdown for the current cycle (top 4 services per
    /// spec 15 section 11.3).
    pub breakdown_by_service: Vec<ServiceCost>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServiceCost {
    pub service_name: String,
    pub current_cycle_usd: f64,
}

/// One day's worth of cost data, including per-model breakdown.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DailyCostEntry {
    /// Local-time date key, ISO format `YYYY-MM-DD`.
    pub date: String,
    /// Total USD spent that day.
    pub cost_usd: f64,
    /// Total tokens (input + output + cache) consumed that day.
    pub total_tokens: i64,
    /// Per-model split for the day, sorted by `cost_usd` descending.
    /// Empty when the source doesn't supply model granularity.
    pub models: Vec<ModelCost>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelCost {
    /// Raw model id (e.g. `claude-sonnet-4-5-20250929`); the UI
    /// formats it for display.
    pub model_id: String,
    pub cost_usd: f64,
    pub total_tokens: i64,
}
