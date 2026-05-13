//! Wire types for Moonshot `/v1/users/me/balance`.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct BalanceResponse {
    pub code: i64,
    pub scode: String,
    pub status: bool,
    pub data: BalanceData,
}

#[derive(Debug, Deserialize)]
pub struct BalanceData {
    #[serde(rename = "available_balance")]
    pub available_balance: f64,
    #[serde(rename = "voucher_balance")]
    pub voucher_balance: f64,
    #[serde(rename = "cash_balance")]
    pub cash_balance: f64,
}

impl BalanceResponse {
    /// Per the Swift source, a successful payload has `code == 0` and
    /// `status == true`. Anything else is an upstream API error.
    pub fn is_ok(&self) -> bool {
        self.code == 0 && self.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_successful_balance_payload() {
        let body = br#"{
            "code": 0,
            "scode": "0",
            "status": true,
            "data": {
                "available_balance": 12.34,
                "voucher_balance": 2.0,
                "cash_balance": 10.34
            }
        }"#;
        let parsed: BalanceResponse = serde_json::from_slice(body).unwrap();
        assert!(parsed.is_ok());
        assert!((parsed.data.available_balance - 12.34).abs() < 1e-9);
    }

    #[test]
    fn detects_api_error_with_nonzero_code() {
        let body = br#"{
            "code": 1001,
            "scode": "AUTH_FAILED",
            "status": false,
            "data": {"available_balance": 0, "voucher_balance": 0, "cash_balance": 0}
        }"#;
        let parsed: BalanceResponse = serde_json::from_slice(body).unwrap();
        assert!(!parsed.is_ok());
    }
}
