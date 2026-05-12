//! Wire shapes for `GET /api/oauth/usage`. The Anthropic response is
//! mapped one to one with spec 40 section 2.6. We deserialize into
//! `OAuthUsageResponse`, then fold into the framework's
//! `UsageSnapshot` in `strategy.rs`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct OAuthUsageResponse {
    #[serde(default)]
    pub five_hour: Option<RateBucket>,
    #[serde(default)]
    pub seven_day: Option<RateBucket>,
    #[serde(default, rename = "seven_day_sonnet")]
    pub seven_day_sonnet: Option<RateBucket>,
    #[serde(default, rename = "seven_day_opus")]
    pub seven_day_opus: Option<RateBucket>,
    #[serde(default)]
    pub extra_usage: Option<ExtraUsage>,
    #[serde(default)]
    pub account: Option<AccountInfo>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RateBucket {
    #[serde(default)]
    pub used: f64,
    #[serde(default)]
    pub allotted: Option<f64>,
    /// Unix epoch seconds; the Anthropic API returns ISO-8601 in some
    /// endpoints and epoch in others. The `resets_at_epoch` field is the
    /// canonical channel; we accept either via a custom deserializer in
    /// future work.
    #[serde(default)]
    pub resets_at_epoch: Option<i64>,
    #[serde(default)]
    pub pace_delta_percent: Option<f32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ExtraUsage {
    /// Cost in cents. We divide by 100 when surfacing dollar values.
    #[serde(default)]
    pub spend_cents: Option<i64>,
    #[serde(default)]
    pub overage_cents: Option<i64>,
}

impl ExtraUsage {
    pub fn spend_dollars(&self) -> Option<f64> {
        self.spend_cents.map(|c| c as f64 / 100.0)
    }

    pub fn overage_dollars(&self) -> Option<f64> {
        self.overage_cents.map(|c| c as f64 / 100.0)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AccountInfo {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_payload() {
        let raw = r#"{
            "five_hour": {"used": 12.5, "allotted": 100.0, "resets_at_epoch": 1700000000},
            "seven_day": {"used": 250.0, "allotted": 1000.0},
            "seven_day_sonnet": {"used": 200.0, "allotted": 800.0},
            "seven_day_opus": {"used": 30.0, "allotted": 100.0},
            "extra_usage": {"spend_cents": 1234, "overage_cents": 0},
            "account": {"email": "jonas@skrylabs.com", "display_name": "Jonas", "plan": "Max"}
        }"#;
        let parsed: OAuthUsageResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.five_hour.as_ref().unwrap().used, 12.5);
        assert_eq!(parsed.seven_day.as_ref().unwrap().allotted, Some(1000.0));
        assert_eq!(parsed.seven_day_sonnet.as_ref().unwrap().used, 200.0);
        assert_eq!(
            parsed.seven_day_opus.as_ref().unwrap().allotted,
            Some(100.0)
        );
        assert_eq!(
            parsed.extra_usage.as_ref().unwrap().spend_dollars(),
            Some(12.34)
        );
        assert_eq!(
            parsed.account.as_ref().unwrap().email.as_deref(),
            Some("jonas@skrylabs.com")
        );
    }

    #[test]
    fn missing_optional_fields_decode_as_none() {
        let parsed: OAuthUsageResponse = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, OAuthUsageResponse::default());
    }
}
