//! Hello provider strategies. Returns a deterministic snapshot so the
//! framework tests can verify the full pipeline without any network.

use std::sync::Arc;

use async_trait::async_trait;

use super::descriptor::HELLO_ID;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;

pub struct HelloStaticStrategy;

#[async_trait]
impl Strategy for HelloStaticStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::OAuth
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        Ok(UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(HELLO_ID, "demo"),
            windows: vec![NamedRateWindow {
                key: "session".into(),
                window: RateWindow {
                    label: "Session".into(),
                    used: 25.0,
                    allotted: Some(100.0),
                    reset_at_unix_secs: None,
                    pace_delta_percent: Some(-1.5),
                },
            }],
            credits: None,
            cost: None,
            account_display_name: Some("Hello demo".into()),
            account_email: None,
            plan_name: Some("Free".into()),
            captured_at_unix_secs: 0,
        })
    }
}

pub fn strategies() -> Vec<Arc<dyn Strategy>> {
    vec![Arc::new(HelloStaticStrategy)]
}
