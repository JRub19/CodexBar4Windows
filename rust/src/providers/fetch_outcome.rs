//! Wraps a `UsageSnapshot` with per-attempt metadata. The refresh loop
//! collects an `Outcome` per provider per tick; the popup uses the
//! `attempts` list to render a debug source label inside Settings.

use serde::{Deserialize, Serialize};

use super::descriptor::FetchStrategy;
use super::errors::ProviderFetchError;
use super::models::UsageSnapshot;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderFetchAttempt {
    pub strategy: FetchStrategy,
    pub duration_ms: u64,
    pub error_kind: Option<String>,
    pub error_detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderFetchOutcome {
    pub provider_id: String,
    pub snapshot: Option<UsageSnapshot>,
    pub winning_strategy: Option<FetchStrategy>,
    pub attempts: Vec<ProviderFetchAttempt>,
}

impl ProviderFetchOutcome {
    pub fn from_error(provider_id: &str, error: &ProviderFetchError) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            snapshot: None,
            winning_strategy: None,
            attempts: vec![ProviderFetchAttempt {
                strategy: FetchStrategy::OAuth, // overwritten by the runtime
                duration_ms: 0,
                error_kind: Some(error.kind().to_string()),
                error_detail: Some(error.to_string()),
            }],
        }
    }
}
