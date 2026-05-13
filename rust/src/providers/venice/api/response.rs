//! Wire types for Venice `/api/v1/billing/balance`. Ported from
//! `Sources/CodexBarCore/Providers/Venice/VeniceUsageFetcher.swift`.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct BalanceResponse {
    #[serde(rename = "canConsume")]
    pub can_consume: bool,
    #[serde(default, rename = "consumptionCurrency")]
    pub consumption_currency: Option<String>,
    pub balances: Balances,
    /// Per-epoch allocation when the active currency is DIEM. Venice
    /// occasionally serialises numerics as strings; we accept both.
    #[serde(default, rename = "diemEpochAllocation")]
    pub diem_epoch_allocation: Option<FlexibleNumber>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Balances {
    #[serde(default)]
    pub diem: Option<FlexibleNumber>,
    #[serde(default)]
    pub usd: Option<FlexibleNumber>,
}

#[derive(Debug, Clone, Copy)]
pub struct FlexibleNumber(pub f64);

impl<'de> Deserialize<'de> for FlexibleNumber {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(de)?;
        let parsed = match &value {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) => s.parse::<f64>().ok(),
            _ => None,
        };
        let v = parsed.ok_or_else(|| serde::de::Error::custom("not a number"))?;
        Ok(FlexibleNumber(v))
    }
}

impl FlexibleNumber {
    pub fn value(self) -> f64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numeric_and_string_balances() {
        let body = br#"{
            "canConsume": true,
            "consumptionCurrency": "DIEM",
            "balances": {"diem": 12.34, "usd": "5.00"},
            "diemEpochAllocation": "100.00"
        }"#;
        let parsed: BalanceResponse = serde_json::from_slice(body).unwrap();
        assert!(parsed.can_consume);
        assert_eq!(parsed.consumption_currency.as_deref(), Some("DIEM"));
        assert_eq!(parsed.balances.diem.unwrap().value(), 12.34);
        assert_eq!(parsed.balances.usd.unwrap().value(), 5.0);
        assert_eq!(parsed.diem_epoch_allocation.unwrap().value(), 100.0);
    }

    #[test]
    fn missing_balances_default_to_none() {
        let body = br#"{"canConsume": false, "balances": {}}"#;
        let parsed: BalanceResponse = serde_json::from_slice(body).unwrap();
        assert!(!parsed.can_consume);
        assert!(parsed.balances.diem.is_none());
        assert!(parsed.balances.usd.is_none());
    }
}
