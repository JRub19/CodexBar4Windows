//! Claude-specific model helpers. Folds the OAuth wire shape into the
//! generic `UsageSnapshot` used by the framework so the popup card and
//! tray renderer do not need to know which strategy produced the data.

use crate::core::ProviderId;
use crate::providers::claude::descriptor::CLAUDE_ID;
use crate::providers::claude::oauth::response::{OAuthUsageResponse, RateBucket};
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;

pub const KEY_FIVE_HOUR: &str = "five_hour";
pub const KEY_SEVEN_DAY: &str = "seven_day";
pub const KEY_SEVEN_DAY_SONNET: &str = "seven_day_sonnet";
pub const KEY_SEVEN_DAY_OPUS: &str = "seven_day_opus";

/// Account info coming from `/api/oauth/account`. The strategy layer
/// fetches this separately from `/api/oauth/usage` because Anthropic
/// keeps the two endpoints distinct.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountSummary {
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub plan_name: Option<String>,
    pub account_uuid: Option<String>,
}

fn bucket_to_window(label: &str, bucket: &RateBucket) -> RateWindow {
    // Anthropic returns `utilization` as a percent (0..=100), so the
    // bucket carries no token count. We surface `used` as the percent
    // and `allotted = 100.0` so the framework's percentage logic stays
    // identical to the other providers.
    RateWindow {
        label: label.to_string(),
        used: bucket.utilization,
        allotted: Some(100.0),
        reset_at_unix_secs: bucket.resets_at_unix_secs(),
        pace_delta_percent: None,
    }
}

/// Fold a parsed OAuth payload into the framework snapshot.
pub fn fold_oauth(
    payload: &OAuthUsageResponse,
    account: &AccountSummary,
    account_token: impl Into<String>,
    captured_at_unix_secs: i64,
) -> UsageSnapshot {
    let mut windows = Vec::new();
    if let Some(b) = &payload.five_hour {
        windows.push(NamedRateWindow {
            key: KEY_FIVE_HOUR.into(),
            window: bucket_to_window("Session", b),
        });
    }
    if let Some(b) = &payload.seven_day {
        windows.push(NamedRateWindow {
            key: KEY_SEVEN_DAY.into(),
            window: bucket_to_window("Week", b),
        });
    }
    if let Some(b) = &payload.seven_day_sonnet {
        windows.push(NamedRateWindow {
            key: KEY_SEVEN_DAY_SONNET.into(),
            window: bucket_to_window("Week (Sonnet)", b),
        });
    }
    if let Some(b) = &payload.seven_day_opus {
        windows.push(NamedRateWindow {
            key: KEY_SEVEN_DAY_OPUS.into(),
            window: bucket_to_window("Week (Opus)", b),
        });
    }
    UsageSnapshot {
        identity: ProviderIdentitySnapshot::new(CLAUDE_ID, account_token),
        windows,
        credits: None,
        cost: None,
        account_display_name: account.display_name.clone(),
        account_email: account.email.clone(),
        plan_name: account.plan_name.clone(),
        captured_at_unix_secs,
    }
}

/// Stable account token. Prefers the UUID from the account endpoint
/// (constant across email changes) and falls back to the email or a
/// "unknown" sentinel.
pub fn account_token_for(account: &AccountSummary) -> String {
    if let Some(uuid) = account.account_uuid.as_deref().filter(|s| !s.is_empty()) {
        return format!("claude:{}", uuid);
    }
    if let Some(email) = account.email.as_deref().filter(|s| !s.is_empty()) {
        return format!("claude:{}", email);
    }
    "claude:unknown".into()
}

/// Convenience constant for callers that already know the provider id.
pub const PROVIDER_ID: ProviderId = CLAUDE_ID;

#[cfg(test)]
mod tests {
    use super::*;

    fn bucket(percent: f64) -> RateBucket {
        RateBucket {
            utilization: percent,
            resets_at: Some("2026-05-12T23:20:00.915200+00:00".into()),
        }
    }

    #[test]
    fn folds_full_payload_into_four_windows() {
        let payload = OAuthUsageResponse {
            five_hour: Some(bucket(25.0)),
            seven_day: Some(bucket(32.0)),
            seven_day_sonnet: Some(bucket(0.0)),
            seven_day_opus: Some(bucket(10.0)),
            ..Default::default()
        };
        let account = AccountSummary {
            email: Some("jonas@skrylabs.com".into()),
            display_name: Some("Jonas".into()),
            plan_name: Some("Max".into()),
            account_uuid: Some("uuid-1".into()),
        };
        let snap = fold_oauth(&payload, &account, "claude:uuid-1", 1_700_000_000);
        assert_eq!(snap.windows.len(), 4);
        assert_eq!(snap.windows[0].key, KEY_FIVE_HOUR);
        assert_eq!(snap.windows[0].window.used, 25.0);
        assert_eq!(snap.windows[0].window.allotted, Some(100.0));
        assert_eq!(snap.windows[3].key, KEY_SEVEN_DAY_OPUS);
        assert_eq!(snap.account_email.as_deref(), Some("jonas@skrylabs.com"));
        assert_eq!(snap.plan_name.as_deref(), Some("Max"));
        assert_eq!(snap.identity.account_token, "claude:uuid-1");
    }

    #[test]
    fn account_token_prefers_uuid_then_email_then_unknown() {
        assert_eq!(
            account_token_for(&AccountSummary {
                account_uuid: Some("uuid-1".into()),
                email: Some("u@x.com".into()),
                ..Default::default()
            }),
            "claude:uuid-1"
        );
        assert_eq!(
            account_token_for(&AccountSummary {
                email: Some("u@x.com".into()),
                ..Default::default()
            }),
            "claude:u@x.com"
        );
        assert_eq!(
            account_token_for(&AccountSummary::default()),
            "claude:unknown"
        );
    }
}
