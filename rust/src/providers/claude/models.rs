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
/// Claude "Designs" feature — separate 7-day utilization bucket
/// for the in-app design surface. Internal codename: `omelette`.
pub const KEY_CLAUDE_DESIGN: &str = "claude_design";
/// Claude "Daily Routines" feature — 7-day utilization bucket for
/// the Routines product. Internal codename: `cowork`.
pub const KEY_CLAUDE_ROUTINES: &str = "claude_routines";

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
    // Extra rate windows from Anthropic's product utilization
    // counters (Designs + Daily Routines). They're appended after
    // the standard token windows so the macOS row order is
    // preserved (Session, Weekly, Sonnet, Opus, Designs, Routines).
    // We render them only when the API reports the bucket — Free
    // accounts whose payload omits these keys won't see the bars.
    if let Some(b) = &payload.seven_day_omelette {
        windows.push(NamedRateWindow {
            key: KEY_CLAUDE_DESIGN.into(),
            window: bucket_to_window("Designs", b),
        });
    }
    if let Some(b) = &payload.seven_day_cowork {
        windows.push(NamedRateWindow {
            key: KEY_CLAUDE_ROUTINES.into(),
            window: bucket_to_window("Daily Routines", b),
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
    fn folds_designs_and_routines_buckets_after_token_windows() {
        let payload = OAuthUsageResponse {
            five_hour: Some(bucket(25.0)),
            seven_day: Some(bucket(32.0)),
            seven_day_sonnet: Some(bucket(0.0)),
            seven_day_omelette: Some(bucket(48.0)),
            seven_day_cowork: Some(bucket(12.0)),
            ..Default::default()
        };
        let account = AccountSummary::default();
        let snap = fold_oauth(&payload, &account, "claude:test", 1_700_000_000);
        // Order: Session, Weekly, Sonnet, Designs, Daily Routines.
        // Opus is absent so it's skipped — Designs/Routines slide up.
        let keys: Vec<&str> = snap.windows.iter().map(|w| w.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                KEY_FIVE_HOUR,
                KEY_SEVEN_DAY,
                KEY_SEVEN_DAY_SONNET,
                KEY_CLAUDE_DESIGN,
                KEY_CLAUDE_ROUTINES,
            ]
        );
        let designs = snap
            .windows
            .iter()
            .find(|w| w.key == KEY_CLAUDE_DESIGN)
            .unwrap();
        assert_eq!(designs.window.label, "Designs");
        assert_eq!(designs.window.used, 48.0);
        let routines = snap
            .windows
            .iter()
            .find(|w| w.key == KEY_CLAUDE_ROUTINES)
            .unwrap();
        assert_eq!(routines.window.label, "Daily Routines");
        assert_eq!(routines.window.used, 12.0);
    }

    #[test]
    fn alias_keys_for_designs_and_routines_deserialize() {
        // Anthropic ships these under several spellings. Verify every
        // alias decodes into the canonical field. Code-side only the
        // canonical field name needs to be in tests.
        let alt_payloads = [
            r#"{"seven_day_design":{"utilization":7.0,"resets_at":null}}"#,
            r#"{"seven_day_claude_design":{"utilization":7.0,"resets_at":null}}"#,
            r#"{"claude_design":{"utilization":7.0,"resets_at":null}}"#,
            r#"{"design":{"utilization":7.0,"resets_at":null}}"#,
            r#"{"omelette":{"utilization":7.0,"resets_at":null}}"#,
        ];
        for raw in alt_payloads {
            let parsed: OAuthUsageResponse = serde_json::from_str(raw).expect(raw);
            assert!(
                parsed.seven_day_omelette.is_some(),
                "alias failed: {raw}"
            );
        }
        let routine_payloads = [
            r#"{"seven_day_routines":{"utilization":3.0,"resets_at":null}}"#,
            r#"{"routines":{"utilization":3.0,"resets_at":null}}"#,
            r#"{"cowork":{"utilization":3.0,"resets_at":null}}"#,
        ];
        for raw in routine_payloads {
            let parsed: OAuthUsageResponse = serde_json::from_str(raw).expect(raw);
            assert!(parsed.seven_day_cowork.is_some(), "alias failed: {raw}");
        }
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
