//! Wire shape for `GET /api/oauth/usage` on `api.anthropic.com`.
//!
//! Validated against the live API on 2026-05-13 with `User-Agent:
//! claude-code/<version>`. Each window is `{utilization: percent,
//! resets_at: ISO-8601}`; missing windows decode to `None`. Account
//! metadata lives at a different endpoint (`/api/oauth/account`); the
//! strategy layer composes them.

use chrono::DateTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct OAuthUsageResponse {
    #[serde(default)]
    pub five_hour: Option<RateBucket>,
    #[serde(default)]
    pub seven_day: Option<RateBucket>,
    #[serde(default)]
    pub seven_day_sonnet: Option<RateBucket>,
    #[serde(default)]
    pub seven_day_opus: Option<RateBucket>,
    #[serde(default)]
    pub seven_day_oauth_apps: Option<RateBucket>,
    /// "Daily Routines" feature utilization (internal codename
    /// `cowork`). Single key — we previously tried to accept the
    /// other spellings macOS knows about via serde aliases, but
    /// having generic words like "routine" / "routines" as aliases
    /// caused unrelated fields in the response to fail to decode
    /// into `RateBucket`, which broke the entire response parse.
    /// Stick to the canonical key only; if Anthropic renames it
    /// later we'll add a permissive deserializer instead of
    /// alias-trapping the whole response.
    #[serde(default, alias = "seven_day_routines")]
    pub seven_day_cowork: Option<RateBucket>,
    /// "Designs" feature utilization (internal codename `omelette`).
    /// Single canonical key — see `seven_day_cowork` comment for
    /// why we don't alias-trap the unprefixed spellings.
    #[serde(default, alias = "seven_day_design")]
    pub seven_day_omelette: Option<RateBucket>,
    #[serde(default)]
    pub extra_usage: Option<ExtraUsage>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RateBucket {
    /// Percent of the quota window consumed (0..=100). The Anthropic
    /// payload calls this `utilization`; it is NOT a raw token count.
    #[serde(default)]
    pub utilization: f64,
    /// ISO-8601 reset timestamp. Convert to unix seconds via
    /// `resets_at_unix_secs()`.
    #[serde(default)]
    pub resets_at: Option<String>,
}

impl RateBucket {
    pub fn resets_at_unix_secs(&self) -> Option<i64> {
        let raw = self.resets_at.as_deref()?;
        DateTime::parse_from_rfc3339(raw)
            .ok()
            .map(|d| d.timestamp())
    }

    /// Convenience: returns `100 - utilization`, clamped to `[0, 100]`.
    pub fn remaining_percent(&self) -> f64 {
        (100.0 - self.utilization).clamp(0.0, 100.0)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ExtraUsage {
    #[serde(default)]
    pub is_enabled: bool,
    #[serde(default)]
    pub monthly_limit: Option<f64>,
    #[serde(default)]
    pub used_credits: Option<f64>,
    #[serde(default)]
    pub utilization: Option<f64>,
    #[serde(default)]
    pub currency: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured verbatim from a live call on 2026-05-13.
    const LIVE_PAYLOAD: &str = r#"{
        "five_hour":{"utilization":25.0,"resets_at":"2026-05-12T23:20:00.915200+00:00"},
        "seven_day":{"utilization":32.0,"resets_at":"2026-05-18T10:00:00.915220+00:00"},
        "seven_day_oauth_apps":null,
        "seven_day_opus":null,
        "seven_day_sonnet":{"utilization":0.0,"resets_at":null},
        "seven_day_cowork":null,
        "seven_day_omelette":{"utilization":0.0,"resets_at":null},
        "extra_usage":{"is_enabled":false,"monthly_limit":null,"used_credits":null,"utilization":null,"currency":null}
    }"#;

    #[test]
    fn parses_live_payload_into_five_known_windows() {
        let parsed: OAuthUsageResponse = serde_json::from_str(LIVE_PAYLOAD).unwrap();
        let five_hour = parsed.five_hour.as_ref().unwrap();
        assert_eq!(five_hour.utilization, 25.0);
        assert!(five_hour.resets_at.is_some());
        let seven_day = parsed.seven_day.as_ref().unwrap();
        assert_eq!(seven_day.utilization, 32.0);
        assert!(parsed.seven_day_opus.is_none());
        assert!(parsed.seven_day_oauth_apps.is_none());
        // sonnet is present but with null reset.
        let sonnet = parsed.seven_day_sonnet.as_ref().unwrap();
        assert_eq!(sonnet.utilization, 0.0);
        assert!(sonnet.resets_at.is_none());
        // extra_usage is structurally present but disabled.
        let extra = parsed.extra_usage.as_ref().unwrap();
        assert!(!extra.is_enabled);
    }

    #[test]
    fn resets_at_unix_secs_converts_iso8601_to_epoch() {
        let bucket = RateBucket {
            utilization: 5.0,
            resets_at: Some("2026-05-12T23:20:00.915200+00:00".into()),
        };
        let epoch = bucket.resets_at_unix_secs().unwrap();
        // 2026-05-12T23:20:00Z is well after 2026-01-01.
        assert!(epoch > 1_767_225_600);
    }

    #[test]
    fn remaining_percent_complements_utilization() {
        let bucket = RateBucket {
            utilization: 25.0,
            resets_at: None,
        };
        assert_eq!(bucket.remaining_percent(), 75.0);
    }

    #[test]
    fn missing_optional_fields_decode_as_none() {
        let parsed: OAuthUsageResponse = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, OAuthUsageResponse::default());
    }
}
