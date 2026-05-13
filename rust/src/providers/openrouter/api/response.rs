//! Wire types for OpenRouter's `/credits` and `/key` endpoints. Ported
//! from `Sources/CodexBarCore/Providers/OpenRouter/OpenRouterUsageStats.swift`.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreditsResponse {
    pub data: CreditsData,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct CreditsData {
    #[serde(default, rename = "total_credits")]
    pub total_credits: f64,
    #[serde(default, rename = "total_usage")]
    pub total_usage: f64,
}

impl CreditsData {
    pub fn balance(&self) -> f64 {
        (self.total_credits - self.total_usage).max(0.0)
    }

    pub fn used_percent(&self) -> f64 {
        if self.total_credits <= 0.0 {
            return 0.0;
        }
        ((self.total_usage / self.total_credits) * 100.0).clamp(0.0, 100.0)
    }
}

#[derive(Debug, Deserialize)]
pub struct KeyResponse {
    pub data: KeyData,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct KeyData {
    #[serde(default, rename = "rate_limit")]
    pub rate_limit: Option<RateLimitWire>,
    #[serde(default)]
    pub limit: Option<f64>,
    #[serde(default)]
    pub usage: Option<f64>,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct RateLimitWire {
    #[serde(default)]
    pub requests: Option<i64>,
    #[serde(default)]
    pub interval: Option<String>,
}

impl KeyData {
    pub fn key_used_percent(&self) -> Option<f64> {
        let limit = self.limit?;
        let usage = self.usage?;
        if limit <= 0.0 {
            return None;
        }
        Some(((usage / limit) * 100.0).clamp(0.0, 100.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_balance_subtracts_usage_from_total() {
        let body = br#"{"data": {"total_credits": 50.0, "total_usage": 12.34}}"#;
        let response: CreditsResponse = serde_json::from_slice(body).unwrap();
        assert!((response.data.balance() - 37.66).abs() < 1e-9);
        assert!((response.data.used_percent() - 24.68).abs() < 1e-9);
    }

    #[test]
    fn credits_balance_clamps_at_zero_when_overdrawn() {
        let body = br#"{"data": {"total_credits": 10.0, "total_usage": 25.0}}"#;
        let response: CreditsResponse = serde_json::from_slice(body).unwrap();
        assert_eq!(response.data.balance(), 0.0);
        assert_eq!(response.data.used_percent(), 100.0);
    }

    #[test]
    fn zero_total_credits_gives_zero_percent() {
        let body = br#"{"data": {"total_credits": 0, "total_usage": 0}}"#;
        let response: CreditsResponse = serde_json::from_slice(body).unwrap();
        assert_eq!(response.data.used_percent(), 0.0);
    }

    #[test]
    fn key_used_percent_when_limit_and_usage_present() {
        let body = br#"{"data": {"limit": 100.0, "usage": 25.0}}"#;
        let response: KeyResponse = serde_json::from_slice(body).unwrap();
        assert_eq!(response.data.key_used_percent(), Some(25.0));
    }

    #[test]
    fn key_used_percent_is_none_when_no_limit() {
        let body = br#"{"data": {"usage": 25.0}}"#;
        let response: KeyResponse = serde_json::from_slice(body).unwrap();
        assert!(response.data.key_used_percent().is_none());
    }

    #[test]
    fn key_rate_limit_parses_requests_and_interval() {
        let body = br#"{"data": {"rate_limit": {"requests": 60, "interval": "1m"}}}"#;
        let response: KeyResponse = serde_json::from_slice(body).unwrap();
        let rl = response.data.rate_limit.unwrap();
        assert_eq!(rl.requests, Some(60));
        assert_eq!(rl.interval.as_deref(), Some("1m"));
    }
}
