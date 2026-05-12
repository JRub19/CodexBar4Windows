//! Cost snapshot per spec 30 section 12.6. Providers that bill by usage
//! emit one of these per refresh so the popup can render the cost chart.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderCostSnapshot {
    /// Total cost in the current billing cycle, USD.
    pub current_cycle_usd: f64,
    /// Total cost in the previous cycle for comparison, USD.
    pub previous_cycle_usd: Option<f64>,
    /// Daily series for the last 30 days; index 0 is the oldest.
    pub last_30_days_usd: Vec<f64>,
    /// Per-service breakdown for the current cycle (top 4 services per
    /// spec 15 section 11.3).
    pub breakdown_by_service: Vec<ServiceCost>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ServiceCost {
    pub service_name: String,
    pub current_cycle_usd: f64,
}
