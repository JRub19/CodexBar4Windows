//! Per-provider usage snapshot. The refresh loop writes one of these
//! into `UsageStore` after every successful strategy run; the tray
//! icon and popup read from the store.

use serde::{Deserialize, Serialize};

use super::credits::CreditsSnapshot;
use super::provider_cost::ProviderCostSnapshot;
use super::rate_window::NamedRateWindow;
use crate::providers::identity::ProviderIdentitySnapshot;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UsageSnapshot {
    pub identity: ProviderIdentitySnapshot,
    /// Windows in declared order. The first window drives the primary
    /// bar on the tray icon, the second drives the secondary bar.
    pub windows: Vec<NamedRateWindow>,
    pub credits: Option<CreditsSnapshot>,
    pub cost: Option<ProviderCostSnapshot>,
    /// Display name pinned to this snapshot. Phase 4 surfaces this in
    /// the provider card header; some providers update the friendly
    /// name asynchronously through OAuth.
    pub account_display_name: Option<String>,
    pub account_email: Option<String>,
    pub plan_name: Option<String>,
    /// Wall clock instant the snapshot was produced, as unix epoch
    /// seconds. Used by the popup's "Updated X ago" caption.
    pub captured_at_unix_secs: i64,
}

impl UsageSnapshot {
    pub fn primary(&self) -> Option<&NamedRateWindow> {
        self.windows.first()
    }

    pub fn secondary(&self) -> Option<&NamedRateWindow> {
        self.windows.get(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ProviderId;
    use crate::providers::models::rate_window::RateWindow;

    #[test]
    fn round_trips_through_serde_json() {
        let snap = UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(ProviderId("claude"), "acct"),
            windows: vec![NamedRateWindow {
                key: "session".into(),
                window: RateWindow {
                    label: "Session".into(),
                    used: 10.0,
                    allotted: Some(100.0),
                    reset_at_unix_secs: Some(1_700_000_000),
                    pace_delta_percent: Some(3.4),
                },
            }],
            credits: None,
            cost: None,
            account_display_name: Some("Jonas".into()),
            account_email: None,
            plan_name: Some("Max".into()),
            captured_at_unix_secs: 1_700_000_000,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: UsageSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, back);
    }
}
