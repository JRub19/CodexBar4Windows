//! Wire types for `/copilot_internal/user`. Ported from
//! `Sources/CodexBarCore/CopilotUsageModels.swift` so the same payload
//! parses identically on Windows. The Swift version uses a custom
//! decoder to coerce strings/numbers and to fall back to monthly /
//! limited_user_quotas when `quota_snapshots` is absent; we do the same
//! with serde + a hand-written normaliser.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct QuotaSnapshot {
    pub entitlement: f64,
    pub remaining: f64,
    pub percent_remaining: f64,
    pub quota_id: String,
    pub has_percent_remaining: bool,
}

impl QuotaSnapshot {
    pub fn is_placeholder(&self) -> bool {
        self.entitlement == 0.0
            && self.remaining == 0.0
            && self.percent_remaining == 0.0
            && self.quota_id.is_empty()
    }

    pub fn used_percent(&self) -> Option<f64> {
        if !self.has_percent_remaining {
            return None;
        }
        Some((100.0 - self.percent_remaining).clamp(0.0, 100.0))
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct CopilotUsage {
    pub premium: Option<QuotaSnapshot>,
    pub chat: Option<QuotaSnapshot>,
    pub copilot_plan: String,
    pub quota_reset_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawResponse {
    #[serde(default)]
    quota_snapshots: Option<Value>,
    #[serde(default)]
    monthly_quotas: Option<QuotaCounts>,
    #[serde(default)]
    limited_user_quotas: Option<QuotaCounts>,
    #[serde(default)]
    copilot_plan: Option<String>,
    #[serde(default)]
    quota_reset_date: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct QuotaCounts {
    #[serde(default, deserialize_with = "lenient_number")]
    chat: Option<f64>,
    #[serde(default, deserialize_with = "lenient_number")]
    completions: Option<f64>,
}

fn lenient_number<'de, D>(de: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<Value>::deserialize(de)?;
    Ok(v.and_then(value_to_number))
}

fn value_to_number(value: Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn quota_snapshot_from_value(value: &Value) -> Option<QuotaSnapshot> {
    let obj = value.as_object()?;
    let entitlement = obj.get("entitlement").cloned().and_then(value_to_number);
    let remaining = obj.get("remaining").cloned().and_then(value_to_number);
    let explicit_percent = obj
        .get("percent_remaining")
        .cloned()
        .and_then(value_to_number);
    let quota_id = obj
        .get("quota_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let (percent_remaining, has_percent) = match explicit_percent {
        Some(p) => (p.clamp(0.0, 100.0), true),
        None => match (entitlement, remaining) {
            (Some(ent), Some(rem)) if ent > 0.0 => (((rem / ent) * 100.0).clamp(0.0, 100.0), true),
            _ => (0.0, false),
        },
    };

    Some(QuotaSnapshot {
        entitlement: entitlement.unwrap_or(0.0),
        remaining: remaining.unwrap_or(0.0),
        percent_remaining,
        quota_id,
        has_percent_remaining: has_percent,
    })
}

/// Walk an arbitrary `quota_snapshots` object to pick out premium /
/// chat candidates. Matches the macOS `QuotaSnapshots.init` recovery
/// path: first try named keys; if either is still nil, scan the
/// remaining keys and bucket them by name substring.
fn pick_quota_snapshots(value: &Value) -> (Option<QuotaSnapshot>, Option<QuotaSnapshot>) {
    let Some(obj) = value.as_object() else {
        return (None, None);
    };

    let mut premium = obj
        .get("premium_interactions")
        .and_then(quota_snapshot_from_value)
        .filter(|s| !s.is_placeholder());
    let mut chat = obj
        .get("chat")
        .and_then(quota_snapshot_from_value)
        .filter(|s| !s.is_placeholder());

    if premium.is_none() || chat.is_none() {
        let mut fallback_premium: Option<QuotaSnapshot> = None;
        let mut fallback_chat: Option<QuotaSnapshot> = None;
        let mut first_usable: Option<QuotaSnapshot> = None;

        for (key, raw) in obj.iter() {
            let Some(snap) = quota_snapshot_from_value(raw) else {
                continue;
            };
            if snap.is_placeholder() {
                continue;
            }
            if first_usable.is_none() {
                first_usable = Some(snap.clone());
            }
            let lower = key.to_ascii_lowercase();
            if fallback_chat.is_none() && lower.contains("chat") {
                fallback_chat = Some(snap.clone());
                continue;
            }
            if fallback_premium.is_none()
                && (lower.contains("premium")
                    || lower.contains("completion")
                    || lower.contains("code"))
            {
                fallback_premium = Some(snap);
            }
        }

        if premium.is_none() {
            premium = fallback_premium;
        }
        if chat.is_none() {
            chat = fallback_chat;
        }
        if premium.is_none() && chat.is_none() {
            chat = first_usable;
        }
    }

    (premium, chat)
}

fn synthesise_from_counts(
    monthly: Option<&QuotaCounts>,
    limited: Option<&QuotaCounts>,
) -> (Option<QuotaSnapshot>, Option<QuotaSnapshot>) {
    fn one(monthly: Option<f64>, limited: Option<f64>, id: &str) -> Option<QuotaSnapshot> {
        let monthly = monthly?;
        let limited = limited?;
        let entitlement = monthly.max(0.0);
        if entitlement <= 0.0 {
            return None;
        }
        let remaining = limited.max(0.0);
        let percent = ((remaining / entitlement) * 100.0).clamp(0.0, 100.0);
        Some(QuotaSnapshot {
            entitlement,
            remaining,
            percent_remaining: percent,
            quota_id: id.to_string(),
            has_percent_remaining: true,
        })
    }

    let premium = one(
        monthly.and_then(|m| m.completions),
        limited.and_then(|l| l.completions),
        "completions",
    );
    let chat = one(
        monthly.and_then(|m| m.chat),
        limited.and_then(|l| l.chat),
        "chat",
    );
    (premium, chat)
}

pub fn parse(body: &[u8]) -> Result<CopilotUsage, serde_json::Error> {
    let raw: RawResponse = serde_json::from_slice(body)?;
    let (direct_premium, direct_chat) = raw
        .quota_snapshots
        .as_ref()
        .map(pick_quota_snapshots)
        .unwrap_or((None, None));

    let (fallback_premium, fallback_chat) = synthesise_from_counts(
        raw.monthly_quotas.as_ref(),
        raw.limited_user_quotas.as_ref(),
    );

    let premium = filter_usable(direct_premium).or_else(|| filter_usable(fallback_premium));
    let chat = filter_usable(direct_chat).or_else(|| filter_usable(fallback_chat));

    Ok(CopilotUsage {
        premium,
        chat,
        copilot_plan: raw.copilot_plan.unwrap_or_else(|| "unknown".into()),
        quota_reset_date: raw.quota_reset_date,
    })
}

fn filter_usable(value: Option<QuotaSnapshot>) -> Option<QuotaSnapshot> {
    let snap = value?;
    if snap.is_placeholder() || !snap.has_percent_remaining {
        None
    } else {
        Some(snap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_premium_and_chat_with_percent_remaining() {
        let body = br#"{
            "copilot_plan": "business",
            "quota_reset_date": "2026-06-01",
            "quota_snapshots": {
                "premium_interactions": {
                    "entitlement": 300, "remaining": 240,
                    "percent_remaining": 80, "quota_id": "premium"
                },
                "chat": {
                    "entitlement": 1000, "remaining": 750,
                    "percent_remaining": 75, "quota_id": "chat"
                }
            }
        }"#;
        let usage = parse(body).unwrap();
        assert_eq!(usage.copilot_plan, "business");
        let premium = usage.premium.unwrap();
        assert_eq!(premium.used_percent(), Some(20.0));
        let chat = usage.chat.unwrap();
        assert_eq!(chat.used_percent(), Some(25.0));
    }

    #[test]
    fn derives_percent_from_entitlement_when_field_absent() {
        let body = br#"{
            "quota_snapshots": {
                "premium_interactions": {"entitlement": 100, "remaining": 25}
            }
        }"#;
        let usage = parse(body).unwrap();
        let premium = usage.premium.unwrap();
        assert_eq!(premium.used_percent(), Some(75.0));
    }

    #[test]
    fn drops_placeholder_zeroes() {
        let body = br#"{
            "quota_snapshots": {
                "premium_interactions": {"entitlement": 0, "remaining": 0, "percent_remaining": 0, "quota_id": ""}
            }
        }"#;
        let usage = parse(body).unwrap();
        assert!(usage.premium.is_none());
    }

    #[test]
    fn falls_back_to_monthly_limited_counts_when_snapshots_missing() {
        let body = br#"{
            "monthly_quotas": {"completions": 1000, "chat": 500},
            "limited_user_quotas": {"completions": 250, "chat": 100}
        }"#;
        let usage = parse(body).unwrap();
        let premium = usage.premium.unwrap();
        // 250/1000 → 25 remaining → 75% used
        assert_eq!(premium.used_percent(), Some(75.0));
        let chat = usage.chat.unwrap();
        // 100/500 → 20 remaining → 80% used
        assert_eq!(chat.used_percent(), Some(80.0));
    }

    #[test]
    fn fallback_scan_picks_named_keys_by_substring() {
        let body = br#"{
            "quota_snapshots": {
                "code_completion": {"entitlement": 100, "remaining": 80, "quota_id": "code"},
                "chatBot": {"entitlement": 50, "remaining": 10, "quota_id": "chat"}
            }
        }"#;
        let usage = parse(body).unwrap();
        let premium = usage.premium.unwrap();
        assert_eq!(premium.quota_id, "code");
        assert_eq!(premium.used_percent(), Some(20.0));
        let chat = usage.chat.unwrap();
        assert_eq!(chat.quota_id, "chat");
        assert_eq!(chat.used_percent(), Some(80.0));
    }

    #[test]
    fn defaults_plan_to_unknown_when_field_missing() {
        let usage = parse(b"{}").unwrap();
        assert_eq!(usage.copilot_plan, "unknown");
        assert!(usage.premium.is_none());
        assert!(usage.chat.is_none());
    }

    #[test]
    fn coerces_string_numbers() {
        let body = br#"{
            "monthly_quotas": {"completions": "1000", "chat": "500"},
            "limited_user_quotas": {"completions": "250", "chat": "100"}
        }"#;
        let usage = parse(body).unwrap();
        assert_eq!(usage.premium.unwrap().used_percent(), Some(75.0));
    }
}
