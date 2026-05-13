//! Wire types for the Factory `/auth/me`, `/subscription/usage`, and
//! `/billing/limits` endpoints. Ported from
//! `Sources/CodexBarCore/Providers/Factory/FactoryStatusProbe.swift`.

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct AuthResponse {
    #[serde(default, rename = "userProfile")]
    pub user_profile: Option<UserProfile>,
    #[serde(default)]
    pub organization: Option<Organization>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UserProfile {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Organization {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub subscription: Option<Subscription>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Subscription {
    #[serde(default, rename = "factoryTier")]
    pub factory_tier: Option<String>,
    #[serde(default, rename = "orbSubscription")]
    pub orb_subscription: Option<OrbSubscription>,
}

#[derive(Debug, Default, Deserialize)]
pub struct OrbSubscription {
    #[serde(default)]
    pub plan: Option<Plan>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Plan {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UsageResponse {
    #[serde(default)]
    pub usage: Option<UsageData>,
    #[serde(default, rename = "userId")]
    pub user_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UsageData {
    #[serde(default, rename = "startDate")]
    pub start_date_ms: Option<i64>,
    #[serde(default, rename = "endDate")]
    pub end_date_ms: Option<i64>,
    #[serde(default)]
    pub standard: Option<TokenUsage>,
    #[serde(default)]
    pub premium: Option<TokenUsage>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TokenUsage {
    #[serde(default, rename = "userTokens")]
    pub user_tokens: Option<i64>,
    #[serde(default, rename = "totalAllowance")]
    pub total_allowance: Option<i64>,
    #[serde(default, rename = "usedRatio")]
    pub used_ratio: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct BillingLimitsResponse {
    #[serde(default, rename = "usesTokenRateLimitsBilling")]
    pub uses_token_rate_limits_billing: bool,
    #[serde(default)]
    pub limits: Option<TokenRateLimits>,
    #[serde(default, rename = "extraUsageBalanceCents")]
    pub extra_usage_balance_cents: i64,
    #[serde(default, rename = "overagePreference")]
    pub overage_preference: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TokenRateLimits {
    pub standard: LimitPool,
    #[serde(default)]
    pub core: Option<LimitPool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LimitPool {
    #[serde(rename = "fiveHour")]
    pub five_hour: BillingWindow,
    pub weekly: BillingWindow,
    pub monthly: BillingWindow,
}

#[derive(Debug, Default, Deserialize)]
pub struct BillingWindow {
    #[serde(default, rename = "usedPercent")]
    pub used_percent: f64,
    #[serde(default, rename = "windowEnd")]
    pub window_end: Option<FlexibleDate>,
    #[serde(default, rename = "secondsRemaining")]
    pub seconds_remaining: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlexibleDate {
    /// Unix seconds. The wire shape is either a number (seconds or ms)
    /// or a string holding the same, plus ISO-8601. We normalise on
    /// parse so the consumer never has to care.
    pub unix_secs: i64,
}

impl<'de> Deserialize<'de> for FlexibleDate {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use chrono::DateTime;
        let value = serde_json::Value::deserialize(de)?;
        let secs = match &value {
            serde_json::Value::Number(n) => n
                .as_f64()
                .map(|v| if v > 1e12 { (v / 1000.0) as i64 } else { v as i64 }),
            serde_json::Value::String(s) => {
                if let Ok(v) = s.parse::<f64>() {
                    Some(if v > 1e12 { (v / 1000.0) as i64 } else { v as i64 })
                } else {
                    DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp())
                }
            }
            _ => None,
        };
        let unix_secs = secs.ok_or_else(|| serde::de::Error::custom("flexible date unparsed"))?;
        Ok(FlexibleDate { unix_secs })
    }
}

impl BillingWindow {
    /// `secondsRemaining` wins; otherwise an absolute `windowEnd` past
    /// `now`. If both are absent or the window has already expired we
    /// return None, matching the Swift `resetAt`.
    pub fn reset_at(&self, now_unix_secs: i64) -> Option<i64> {
        if let Some(secs) = self.seconds_remaining {
            if secs > 0.0 {
                return Some(now_unix_secs + secs as i64);
            }
        }
        let end = self.window_end.as_ref()?.unix_secs;
        if end > now_unix_secs {
            Some(end)
        } else {
            None
        }
    }

    /// Stale-window guard from the Swift `effectiveUsedPercent`: when a
    /// short rolling window expired but Factory still reports stale
    /// values, the web UI treats it as reset.
    pub fn effective_used_percent(&self, now_unix_secs: i64) -> f64 {
        if self.reset_at(now_unix_secs).is_none()
            && self.window_end.is_some()
            && self.seconds_remaining.is_none()
        {
            return 0.0;
        }
        self.used_percent.clamp(0.0, 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_billing_window_with_seconds_remaining() {
        let body = br#"{"usedPercent": 42.0, "secondsRemaining": 3600}"#;
        let win: BillingWindow = serde_json::from_slice(body).unwrap();
        let now = 1_700_000_000;
        assert_eq!(win.reset_at(now), Some(now + 3600));
        assert_eq!(win.effective_used_percent(now), 42.0);
    }

    #[test]
    fn parses_billing_window_with_window_end_seconds() {
        let body = br#"{"usedPercent": 33.0, "windowEnd": 1700000500}"#;
        let win: BillingWindow = serde_json::from_slice(body).unwrap();
        assert_eq!(win.reset_at(1700000000), Some(1700000500));
    }

    #[test]
    fn parses_billing_window_with_window_end_millis() {
        let body = br#"{"usedPercent": 33.0, "windowEnd": 1700000500000.0}"#;
        let win: BillingWindow = serde_json::from_slice(body).unwrap();
        assert_eq!(win.reset_at(1700000000), Some(1700000500));
    }

    #[test]
    fn parses_billing_window_with_iso_window_end() {
        let body = br#"{"usedPercent": 33.0, "windowEnd": "2026-06-01T00:00:00Z"}"#;
        let win: BillingWindow = serde_json::from_slice(body).unwrap();
        assert_eq!(win.reset_at(1700000000), Some(1_780_272_000));
    }

    #[test]
    fn stale_window_with_no_seconds_remaining_returns_zero_percent() {
        let body = br#"{"usedPercent": 80.0, "windowEnd": 100}"#;
        let win: BillingWindow = serde_json::from_slice(body).unwrap();
        // Now is well past windowEnd, secondsRemaining absent → treat
        // as reset.
        assert_eq!(win.effective_used_percent(1_700_000_000), 0.0);
        assert!(win.reset_at(1_700_000_000).is_none());
    }

    #[test]
    fn parses_full_usage_response() {
        let body = br#"{
            "usage": {
                "startDate": 1700000000000,
                "endDate": 1702592000000,
                "standard": {"userTokens": 5000, "totalAllowance": 10000, "usedRatio": 0.5},
                "premium": {"userTokens": 250, "totalAllowance": 1000, "usedRatio": 0.25}
            },
            "userId": "user-1"
        }"#;
        let response: UsageResponse = serde_json::from_slice(body).unwrap();
        let usage = response.usage.unwrap();
        assert_eq!(usage.start_date_ms, Some(1700000000000));
        assert_eq!(usage.standard.unwrap().used_ratio, Some(0.5));
    }

    #[test]
    fn billing_limits_with_token_rate_limits_pool() {
        let body = br#"{
            "usesTokenRateLimitsBilling": true,
            "extraUsageBalanceCents": 250,
            "overagePreference": "auto",
            "limits": {
                "standard": {
                    "fiveHour": {"usedPercent": 10.0, "secondsRemaining": 3600},
                    "weekly": {"usedPercent": 20.0, "secondsRemaining": 86400},
                    "monthly": {"usedPercent": 30.0, "secondsRemaining": 604800}
                }
            }
        }"#;
        let response: BillingLimitsResponse = serde_json::from_slice(body).unwrap();
        assert!(response.uses_token_rate_limits_billing);
        let standard = response.limits.unwrap().standard;
        assert_eq!(standard.five_hour.used_percent, 10.0);
        assert_eq!(standard.monthly.used_percent, 30.0);
        assert_eq!(response.extra_usage_balance_cents, 250);
        assert_eq!(response.overage_preference.as_deref(), Some("auto"));
    }
}
