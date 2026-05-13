//! Wire types for the DeepSeek `/user/balance` endpoint. Ported from
//! `Sources/CodexBarCore/Providers/DeepSeek/DeepSeekUsageFetcher.swift`.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct BalanceResponse {
    #[serde(rename = "is_available")]
    pub is_available: bool,
    #[serde(rename = "balance_infos")]
    pub balance_infos: Vec<BalanceInfo>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BalanceInfo {
    pub currency: String,
    /// DeepSeek serialises numeric balances as JSON strings.
    #[serde(rename = "total_balance")]
    pub total_balance: String,
    #[serde(rename = "granted_balance")]
    pub granted_balance: String,
    #[serde(rename = "topped_up_balance")]
    pub topped_up_balance: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedBalance {
    pub currency: String,
    pub total: f64,
    pub granted: f64,
    pub topped_up: f64,
}

impl BalanceInfo {
    pub fn parse(&self) -> Result<ParsedBalance, String> {
        let total = self
            .total_balance
            .parse::<f64>()
            .map_err(|_| format!("non-numeric total_balance: {}", self.total_balance))?;
        let granted = self
            .granted_balance
            .parse::<f64>()
            .map_err(|_| format!("non-numeric granted_balance: {}", self.granted_balance))?;
        let topped_up = self
            .topped_up_balance
            .parse::<f64>()
            .map_err(|_| format!("non-numeric topped_up_balance: {}", self.topped_up_balance))?;
        Ok(ParsedBalance {
            currency: self.currency.clone(),
            total,
            granted,
            topped_up,
        })
    }
}

/// Select the most representative balance from the response. The Swift
/// source's preference order:
/// 1. A funded USD balance.
/// 2. Any balance with a positive total.
/// 3. The USD balance, even if empty.
/// 4. The first balance in the list.
pub fn pick_balance(balances: &[ParsedBalance]) -> Option<&ParsedBalance> {
    balances
        .iter()
        .find(|b| b.currency == "USD" && b.total > 0.0)
        .or_else(|| balances.iter().find(|b| b.total > 0.0))
        .or_else(|| balances.iter().find(|b| b.currency == "USD"))
        .or_else(|| balances.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_info_parses_string_numerics() {
        let raw = BalanceInfo {
            currency: "USD".into(),
            total_balance: "12.34".into(),
            granted_balance: "5.00".into(),
            topped_up_balance: "7.34".into(),
        };
        let parsed = raw.parse().unwrap();
        assert_eq!(parsed.currency, "USD");
        assert!((parsed.total - 12.34).abs() < 1e-9);
        assert!((parsed.granted - 5.0).abs() < 1e-9);
        assert!((parsed.topped_up - 7.34).abs() < 1e-9);
    }

    #[test]
    fn balance_info_rejects_non_numeric_strings() {
        let raw = BalanceInfo {
            currency: "USD".into(),
            total_balance: "n/a".into(),
            granted_balance: "0".into(),
            topped_up_balance: "0".into(),
        };
        assert!(raw.parse().is_err());
    }

    #[test]
    fn pick_balance_prefers_funded_usd() {
        let balances = vec![
            ParsedBalance {
                currency: "CNY".into(),
                total: 50.0,
                granted: 50.0,
                topped_up: 0.0,
            },
            ParsedBalance {
                currency: "USD".into(),
                total: 10.0,
                granted: 0.0,
                topped_up: 10.0,
            },
        ];
        let picked = pick_balance(&balances).unwrap();
        assert_eq!(picked.currency, "USD");
        assert_eq!(picked.total, 10.0);
    }

    #[test]
    fn pick_balance_falls_back_to_any_funded_currency() {
        let balances = vec![
            ParsedBalance {
                currency: "USD".into(),
                total: 0.0,
                granted: 0.0,
                topped_up: 0.0,
            },
            ParsedBalance {
                currency: "CNY".into(),
                total: 99.0,
                granted: 99.0,
                topped_up: 0.0,
            },
        ];
        let picked = pick_balance(&balances).unwrap();
        assert_eq!(picked.currency, "CNY");
    }

    #[test]
    fn pick_balance_falls_back_to_empty_usd_when_no_funded_row() {
        let balances = vec![
            ParsedBalance {
                currency: "CNY".into(),
                total: 0.0,
                granted: 0.0,
                topped_up: 0.0,
            },
            ParsedBalance {
                currency: "USD".into(),
                total: 0.0,
                granted: 0.0,
                topped_up: 0.0,
            },
        ];
        let picked = pick_balance(&balances).unwrap();
        assert_eq!(picked.currency, "USD");
    }

    #[test]
    fn parses_full_balance_response() {
        let body = br#"{
            "is_available": true,
            "balance_infos": [
                {"currency": "USD", "total_balance": "12.34", "granted_balance": "5.00", "topped_up_balance": "7.34"}
            ]
        }"#;
        let resp: BalanceResponse = serde_json::from_slice(body).unwrap();
        assert!(resp.is_available);
        assert_eq!(resp.balance_infos.len(), 1);
        assert_eq!(resp.balance_infos[0].currency, "USD");
    }
}
