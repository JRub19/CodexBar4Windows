//! Immutable per-refresh result models. Spec 30 section 12 lists every
//! field; this module re-exports the canonical types so callers can
//! `use codexbar::providers::models::{RateWindow, UsageSnapshot};`.

pub mod credits;
pub mod provider_cost;
pub mod rate_window;
pub mod storage_footprint;
pub mod usage_snapshot;

pub use credits::{CreditEvent, CreditUnit, CreditsSnapshot};
pub use provider_cost::{ProviderCostSnapshot, ServiceCost};
pub use rate_window::{NamedRateWindow, RateWindow};
pub use storage_footprint::ProviderStorageFootprint;
pub use usage_snapshot::UsageSnapshot;
