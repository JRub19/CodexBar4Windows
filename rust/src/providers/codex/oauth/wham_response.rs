//! Wire shape for `GET /backend-api/wham/usage`. Spec 41 §3.5 documents
//! the schema; we deserialize tolerantly so a partial response (eg.
//! `primary_window` decoded but `secondary_window` malformed) still
//! produces a usable snapshot.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WhamResponse {
    #[serde(default)]
    pub primary_window: Option<RateWindowWire>,
    #[serde(default)]
    pub secondary_window: Option<RateWindowWire>,
    #[serde(default)]
    pub credits: Option<CreditsWire>,
    #[serde(default)]
    pub account: Option<AccountWire>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RateWindowWire {
    #[serde(default)]
    pub used: Option<f64>,
    #[serde(default)]
    pub allotted: Option<f64>,
    #[serde(default)]
    pub resets_at_epoch: Option<i64>,
    #[serde(default)]
    pub plan_type: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CreditsWire {
    #[serde(default)]
    pub balance: Option<f64>,
    #[serde(default)]
    pub allotted: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AccountWire {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan_type: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
}

/// Open-enum plan type. Known tiers are spelled out so the popup can
/// branch on them; unknown tiers are kept as a `String` so we never
/// silently drop the user's plan label.
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecodeFlags {
    pub primary_window_decode_failed: bool,
    pub secondary_window_decode_failed: bool,
    pub credits_decode_failed: bool,
}

/// Tolerant decode: walks the JSON value once and isolates per-window
/// failures so a single mistyped field does not drop the snapshot.
pub fn decode_tolerant(body: &[u8]) -> (WhamResponse, DecodeFlags) {
    let raw: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return (WhamResponse::default(), DecodeFlags::default()),
    };
    let mut flags = DecodeFlags::default();
    let primary_window = decode_field::<RateWindowWire>(&raw, "primary_window")
        .inspect_err(|_| flags.primary_window_decode_failed = true)
        .ok();
    let secondary_window = decode_field::<RateWindowWire>(&raw, "secondary_window")
        .inspect_err(|_| flags.secondary_window_decode_failed = true)
        .ok();
    let credits = decode_field::<CreditsWire>(&raw, "credits")
        .inspect_err(|_| flags.credits_decode_failed = true)
        .ok();
    let account = decode_field::<AccountWire>(&raw, "account").ok();
    (
        WhamResponse {
            primary_window,
            secondary_window,
            credits,
            account,
        },
        flags,
    )
}

fn decode_field<T: serde::de::DeserializeOwned>(
    raw: &serde_json::Value,
    key: &str,
) -> Result<T, serde_json::Error> {
    let value = raw.get(key).cloned().unwrap_or(serde_json::Value::Null);
    serde_json::from_value(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_payload() {
        let raw = br#"{
            "primary_window": {"used": 12.0, "allotted": 100.0, "resets_at_epoch": 1, "plan_type": "plus"},
            "secondary_window": {"used": 200.0, "allotted": 1000.0},
            "credits": {"balance": 25.0, "allotted": 100.0},
            "account": {"email": "u@x.com", "plan_type": "Plus", "account_id": "abc"}
        }"#;
        let (parsed, flags) = decode_tolerant(raw);
        assert!(parsed.primary_window.is_some());
        assert!(parsed.secondary_window.is_some());
        assert_eq!(parsed.credits.as_ref().unwrap().balance, Some(25.0));
        assert!(!flags.primary_window_decode_failed);
        assert!(!flags.secondary_window_decode_failed);
    }

    #[test]
    fn malformed_primary_window_does_not_drop_credits() {
        let raw = br#"{
            "primary_window": {"used": "not-a-number"},
            "credits": {"balance": 10.0, "allotted": 100.0}
        }"#;
        let (parsed, flags) = decode_tolerant(raw);
        assert!(parsed.primary_window.is_none());
        assert!(flags.primary_window_decode_failed);
        assert!(parsed.credits.is_some());
    }

    #[test]
    fn unknown_plan_type_keeps_string() {
        assert_eq!(
            PlanTier::from_str_open("Mystery"),
            PlanTier::Unknown("mystery".into())
        );
    }

    #[test]
    fn known_plan_types_normalize() {
        assert_eq!(PlanTier::from_str_open("PLUS"), PlanTier::Plus);
        assert_eq!(PlanTier::from_str_open(" pro "), PlanTier::Pro);
        assert_eq!(PlanTier::from_str_open("team"), PlanTier::Team);
    }

    #[test]
    fn fully_malformed_payload_returns_default() {
        let (parsed, flags) = decode_tolerant(b"not json");
        assert_eq!(parsed, WhamResponse::default());
        assert!(!flags.primary_window_decode_failed);
    }
}
