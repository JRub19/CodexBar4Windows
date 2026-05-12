//! Wire shape for `GET https://chatgpt.com/backend-api/wham/usage` —
//! the Codex CLI's usage endpoint. Live-verified on 2026-05-13 against
//! a real OpenAI Plus account with `User-Agent: codex_cli_rs/<version>`.
//!
//! The endpoint accepts the OAuth bearer + `ChatGPT-Account-Id` header
//! and returns a single rate_limit object with primary (5-hour) and
//! secondary (7-day) windows, plus account + credits metadata.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WhamResponse {
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan_type: Option<String>,
    #[serde(default)]
    pub rate_limit: Option<RateLimitWire>,
    #[serde(default)]
    pub credits: Option<CreditsWire>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RateLimitWire {
    #[serde(default)]
    pub allowed: bool,
    #[serde(default)]
    pub limit_reached: bool,
    #[serde(default)]
    pub primary_window: Option<RateWindowWire>,
    #[serde(default)]
    pub secondary_window: Option<RateWindowWire>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RateWindowWire {
    /// Percent (0..=100) of the window consumed.
    #[serde(default)]
    pub used_percent: f64,
    #[serde(default)]
    pub limit_window_seconds: i64,
    #[serde(default)]
    pub reset_after_seconds: i64,
    /// Unix epoch seconds for the next reset.
    #[serde(default)]
    pub reset_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CreditsWire {
    #[serde(default)]
    pub has_credits: bool,
    #[serde(default)]
    pub unlimited: bool,
    #[serde(default)]
    pub overage_limit_reached: bool,
    /// The API returns balance as a string ("0", "12.34"). We keep
    /// the string verbatim and let the popup format with locale rules.
    #[serde(default)]
    pub balance: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DecodeFlags {
    pub primary_window_decode_failed: bool,
    pub secondary_window_decode_failed: bool,
    pub credits_decode_failed: bool,
}

/// Tolerant decode. Per-window failures are isolated so a single
/// schema change does not drop the whole snapshot.
pub fn decode_tolerant(body: &[u8]) -> (WhamResponse, DecodeFlags) {
    let mut flags = DecodeFlags::default();
    let raw: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return (WhamResponse::default(), flags),
    };
    let mut response = WhamResponse {
        user_id: raw
            .get("user_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        account_id: raw
            .get("account_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        email: raw
            .get("email")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        plan_type: raw
            .get("plan_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        rate_limit: None,
        credits: None,
    };
    if let Some(rl) = raw.get("rate_limit") {
        let allowed = rl.get("allowed").and_then(|v| v.as_bool()).unwrap_or(false);
        let limit_reached = rl
            .get("limit_reached")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let primary_window = rl
            .get("primary_window")
            .and_then(|w| decode_window(w, &mut flags.primary_window_decode_failed));
        let secondary_window = rl
            .get("secondary_window")
            .and_then(|w| decode_window(w, &mut flags.secondary_window_decode_failed));
        response.rate_limit = Some(RateLimitWire {
            allowed,
            limit_reached,
            primary_window,
            secondary_window,
        });
    }
    if let Some(c) = raw.get("credits") {
        match serde_json::from_value::<CreditsWire>(c.clone()) {
            Ok(c) => response.credits = Some(c),
            Err(_) => flags.credits_decode_failed = true,
        }
    }
    (response, flags)
}

fn decode_window(value: &serde_json::Value, failed: &mut bool) -> Option<RateWindowWire> {
    if value.is_null() {
        return None;
    }
    match serde_json::from_value::<RateWindowWire>(value.clone()) {
        Ok(w) => Some(w),
        Err(_) => {
            *failed = true;
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanTier {
    Free,
    Plus,
    Pro,
    Team,
    Enterprise,
    Unknown(String),
}

impl PlanTier {
    pub fn from_str_open(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "free" => PlanTier::Free,
            "plus" => PlanTier::Plus,
            "pro" => PlanTier::Pro,
            "team" => PlanTier::Team,
            "enterprise" => PlanTier::Enterprise,
            other => PlanTier::Unknown(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured verbatim from a live `/wham/usage` call on 2026-05-13
    /// (Plus plan, single account).
    const LIVE_PAYLOAD: &str = r#"{
        "user_id": "user-HIJcVhHgnvU76ek4DffSDGSG",
        "account_id": "user-HIJcVhHgnvU76ek4DffSDGSG",
        "email": "jonas@skrylabs.com",
        "plan_type": "plus",
        "rate_limit": {
            "allowed": true,
            "limit_reached": false,
            "primary_window": {
                "used_percent": 1,
                "limit_window_seconds": 18000,
                "reset_after_seconds": 18000,
                "reset_at": 1778645804
            },
            "secondary_window": {
                "used_percent": 0,
                "limit_window_seconds": 604800,
                "reset_after_seconds": 145977,
                "reset_at": 1778773780
            }
        },
        "credits": {
            "has_credits": false,
            "unlimited": false,
            "overage_limit_reached": false,
            "balance": "0"
        }
    }"#;

    #[test]
    fn parses_live_payload_into_account_metadata_and_two_windows() {
        let (parsed, flags) = decode_tolerant(LIVE_PAYLOAD.as_bytes());
        assert_eq!(parsed.email.as_deref(), Some("jonas@skrylabs.com"));
        assert_eq!(parsed.plan_type.as_deref(), Some("plus"));
        let rate_limit = parsed.rate_limit.unwrap();
        assert!(rate_limit.allowed);
        let primary = rate_limit.primary_window.unwrap();
        assert_eq!(primary.used_percent, 1.0);
        assert_eq!(primary.reset_at, Some(1778645804));
        let secondary = rate_limit.secondary_window.unwrap();
        assert_eq!(secondary.limit_window_seconds, 604800);
        let credits = parsed.credits.unwrap();
        assert_eq!(credits.balance.as_deref(), Some("0"));
        assert!(!flags.primary_window_decode_failed);
        assert!(!flags.secondary_window_decode_failed);
    }

    #[test]
    fn malformed_primary_window_does_not_drop_credits() {
        let raw = br#"{
            "plan_type": "plus",
            "rate_limit": { "primary_window": {"used_percent": "not-a-number"} },
            "credits": {"has_credits": false, "balance": "0"}
        }"#;
        let (parsed, flags) = decode_tolerant(raw);
        let rate_limit = parsed.rate_limit.unwrap();
        assert!(rate_limit.primary_window.is_none());
        assert!(flags.primary_window_decode_failed);
        assert!(parsed.credits.is_some());
    }

    #[test]
    fn fully_malformed_payload_returns_default() {
        let (parsed, _) = decode_tolerant(b"not json");
        assert_eq!(parsed, WhamResponse::default());
    }

    #[test]
    fn known_plan_types_normalize() {
        assert_eq!(PlanTier::from_str_open("PLUS"), PlanTier::Plus);
        assert_eq!(PlanTier::from_str_open(" pro "), PlanTier::Pro);
    }

    #[test]
    fn unknown_plan_type_keeps_string() {
        assert_eq!(
            PlanTier::from_str_open("Mystery"),
            PlanTier::Unknown("mystery".into())
        );
    }
}
