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

fn bucket_to_window(label: &str, bucket: &RateBucket) -> RateWindow {
    RateWindow {
        label: label.to_string(),
        used: bucket.used,
        allotted: bucket.allotted,
        reset_at_unix_secs: bucket.resets_at_epoch,
        pace_delta_percent: bucket.pace_delta_percent,
    }
}

/// Fold a parsed OAuth payload into the framework snapshot. The
/// `account_token` is what `UsageStore` keys per-account writes by.
pub fn fold_oauth(
    payload: &OAuthUsageResponse,
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
        account_display_name: payload
            .account
            .as_ref()
            .and_then(|a| a.display_name.clone()),
        account_email: payload.account.as_ref().and_then(|a| a.email.clone()),
        plan_name: payload.account.as_ref().and_then(|a| a.plan.clone()),
        captured_at_unix_secs,
    }
}

/// Stable account token derived from the OAuth payload. We hash the
/// email (or fall back to a sentinel) so the same account always lands
/// in the same `UsageStore` slot without leaking the email through any
/// in-memory map key.
pub fn account_token_for(payload: &OAuthUsageResponse) -> String {
    let email = payload
        .account
        .as_ref()
        .and_then(|a| a.email.as_deref())
        .unwrap_or("unknown");
    format!("claude:{}", email)
}

/// Convenience constant for callers that already know the provider id.
pub const PROVIDER_ID: ProviderId = CLAUDE_ID;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_full_payload_into_four_windows() {
        let payload = OAuthUsageResponse {
            five_hour: Some(RateBucket {
                used: 5.0,
                allotted: Some(100.0),
                resets_at_epoch: Some(1),
                pace_delta_percent: None,
            }),
            seven_day: Some(RateBucket {
                used: 50.0,
                allotted: Some(500.0),
                resets_at_epoch: Some(2),
                pace_delta_percent: None,
            }),
            seven_day_sonnet: Some(RateBucket {
                used: 40.0,
                allotted: Some(400.0),
                resets_at_epoch: Some(2),
                pace_delta_percent: None,
            }),
            seven_day_opus: Some(RateBucket {
                used: 10.0,
                allotted: Some(50.0),
                resets_at_epoch: Some(2),
                pace_delta_percent: None,
            }),
            extra_usage: None,
            account: None,
        };
        let snap = fold_oauth(&payload, "acct-1", 1_700_000_000);
        assert_eq!(snap.windows.len(), 4);
        assert_eq!(snap.windows[0].key, KEY_FIVE_HOUR);
        assert_eq!(snap.windows[3].key, KEY_SEVEN_DAY_OPUS);
        assert_eq!(snap.identity.provider_id, "claude");
        assert_eq!(snap.identity.account_token, "acct-1");
    }

    #[test]
    fn account_token_derives_from_email() {
        let payload = OAuthUsageResponse {
            account: Some(crate::providers::claude::oauth::response::AccountInfo {
                email: Some("user@example.com".into()),
                display_name: None,
                plan: None,
            }),
            ..Default::default()
        };
        assert_eq!(account_token_for(&payload), "claude:user@example.com");
    }

    #[test]
    fn account_token_falls_back_when_email_missing() {
        let payload = OAuthUsageResponse::default();
        assert_eq!(account_token_for(&payload), "claude:unknown");
    }
}
