//! Venice API-key strategy. The Venice billing API serves either a
//! USD balance (purchased credits) or a DIEM balance (granted per
//! epoch). The fold mirrors `VeniceUsageSnapshot.toUsageSnapshot` in
//! the macOS source: prefer the active currency; render a balance
//! window with `used = 100` when the account is dry or
//! `canConsume == false`.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use super::response::BalanceResponse;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::credits::{CreditUnit, CreditsSnapshot};
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;
use crate::providers::venice::descriptor::VENICE_ID;

pub const BALANCE_URL: &str = "https://api.venice.ai/api/v1/billing/balance";
pub const PER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[async_trait]
pub trait VeniceHttp: Send + Sync {
    async fn get(&self, url: &str, bearer: &str) -> Result<VeniceResponse, ProviderFetchError>;
}

pub struct VeniceResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[async_trait]
pub trait VeniceCredentialsResolver: Send + Sync {
    async fn resolve(&self) -> Result<Option<String>, ProviderFetchError>;
}

pub struct VeniceApiStrategy {
    http: Arc<dyn VeniceHttp>,
    creds: Arc<dyn VeniceCredentialsResolver>,
}

impl VeniceApiStrategy {
    pub fn new(http: Arc<dyn VeniceHttp>, creds: Arc<dyn VeniceCredentialsResolver>) -> Self {
        Self { http, creds }
    }
}

#[async_trait]
impl Strategy for VeniceApiStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::ApiKey
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let api_key = self
            .creds
            .resolve()
            .await?
            .filter(|k| !k.trim().is_empty())
            .ok_or(ProviderFetchError::NoToken("venice"))?;
        let bearer = format!("Bearer {api_key}");
        let response = self.http.get(BALANCE_URL, &bearer).await?;
        let parsed: BalanceResponse = match response.status {
            200..=299 => serde_json::from_slice(&response.body)
                .map_err(|e| ProviderFetchError::ParseError(format!("billing/balance: {e}")))?,
            401 | 403 => return Err(ProviderFetchError::Unauthorized),
            other => {
                return Err(ProviderFetchError::Network(format!(
                    "venice billing/balance returned {other}"
                )))
            }
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let (used_percent, label, credits_balance, credits_unit) = fold_balance(&parsed);

        let mut snap = UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(
                VENICE_ID,
                format!("venice:key:{}", short_key_token(&api_key)),
            ),
            windows: vec![NamedRateWindow {
                key: "balance".into(),
                window: RateWindow {
                    label: "Balance".into(),
                    used: used_percent,
                    allotted: Some(100.0),
                    reset_at_unix_secs: None,
                    pace_delta_percent: None,
                },
            }],
            credits: None,
            cost: None,
            account_display_name: None,
            account_email: None,
            plan_name: Some(label),
            captured_at_unix_secs: now,
        };
        if let Some(balance) = credits_balance {
            snap.credits = Some(CreditsSnapshot {
                balance,
                unit: credits_unit,
                recent_events: Vec::new(),
            });
        }
        Ok(snap)
    }
}

fn fold_balance(parsed: &BalanceResponse) -> (f64, String, Option<f64>, CreditUnit) {
    let active_currency = parsed
        .consumption_currency
        .as_deref()
        .map(str::to_ascii_uppercase);
    let active = active_currency.as_deref();
    let diem = parsed.balances.diem.map(|n| n.value());
    let usd = parsed.balances.usd.map(|n| n.value());
    let allocation = parsed.diem_epoch_allocation.map(|n| n.value());

    if !parsed.can_consume {
        return (
            100.0,
            "Balance unavailable for API calls".into(),
            None,
            CreditUnit::UsdCents,
        );
    }

    if active == Some("USD") {
        if let Some(usd) = usd.filter(|v| *v > 0.0) {
            return (
                0.0,
                format!("${:.2} USD remaining", usd),
                Some(usd),
                CreditUnit::UsdCents,
            );
        }
    }

    // DIEM with an epoch allocation → compute real used%.
    if active != Some("USD") {
        if let (Some(diem), Some(alloc)) = (diem, allocation) {
            if alloc > 0.0 {
                let used = ((alloc - diem) / alloc * 100.0).clamp(0.0, 100.0);
                return (
                    used,
                    format!("DIEM {:.2} / {:.2} epoch allocation", diem, alloc),
                    Some(diem),
                    CreditUnit::Credits,
                );
            }
        }
    }

    if active == Some("DIEM") {
        if let Some(diem) = diem.filter(|v| *v > 0.0) {
            return (
                0.0,
                format!("DIEM {:.2} remaining", diem),
                Some(diem),
                CreditUnit::Credits,
            );
        }
    }

    if let Some(diem) = diem.filter(|v| *v > 0.0) {
        return (
            0.0,
            format!("DIEM {:.2} remaining", diem),
            Some(diem),
            CreditUnit::Credits,
        );
    }
    if let Some(usd) = usd.filter(|v| *v > 0.0) {
        return (
            0.0,
            format!("${:.2} USD remaining", usd),
            Some(usd),
            CreditUnit::UsdCents,
        );
    }
    (
        100.0,
        "No Venice API balance available".into(),
        None,
        CreditUnit::UsdCents,
    )
}

fn short_key_token(api_key: &str) -> String {
    let trimmed = api_key.trim();
    trimmed.chars().take(4).collect::<String>().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct StubHttp {
        reply: Mutex<Option<(u16, Vec<u8>)>>,
    }
    impl StubHttp {
        fn new(status: u16, body: &[u8]) -> Self {
            Self {
                reply: Mutex::new(Some((status, body.to_vec()))),
            }
        }
    }
    #[async_trait]
    impl VeniceHttp for StubHttp {
        async fn get(&self, _: &str, _: &str) -> Result<VeniceResponse, ProviderFetchError> {
            let (status, body) = self.reply.lock().unwrap().take().unwrap();
            Ok(VeniceResponse { status, body })
        }
    }
    struct StubResolver(Option<String>);
    #[async_trait]
    impl VeniceCredentialsResolver for StubResolver {
        async fn resolve(&self) -> Result<Option<String>, ProviderFetchError> {
            Ok(self.0.clone())
        }
    }
    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }
    fn ctx() -> ProviderFetchContext {
        use crate::providers::fetch_context::{Runtime, SourceMode};
        use crate::secrets::token_account::TokenAccountStore;
        let tokens = Arc::new(TokenAccountStore::new(std::env::temp_dir()));
        ProviderFetchContext {
            provider_id: VENICE_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    #[test]
    fn usd_funded_account_renders_zero_used_with_dollar_label() {
        let http = Arc::new(StubHttp::new(
            200,
            br#"{
                "canConsume": true,
                "consumptionCurrency": "USD",
                "balances": {"usd": 12.34, "diem": 0}
            }"#,
        ));
        let resolver = Arc::new(StubResolver(Some("sk-abcd".into())));
        let strategy = VeniceApiStrategy::new(http, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows[0].window.used, 0.0);
        assert_eq!(snap.plan_name.as_deref(), Some("$12.34 USD remaining"));
        let credits = snap.credits.unwrap();
        assert_eq!(credits.unit, CreditUnit::UsdCents);
        assert!((credits.balance - 12.34).abs() < 1e-9);
    }

    #[test]
    fn diem_with_allocation_computes_used_ratio() {
        let http = Arc::new(StubHttp::new(
            200,
            br#"{
                "canConsume": true,
                "consumptionCurrency": "DIEM",
                "balances": {"diem": 40, "usd": 0},
                "diemEpochAllocation": 100
            }"#,
        ));
        let resolver = Arc::new(StubResolver(Some("sk-x".into())));
        let strategy = VeniceApiStrategy::new(http, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows[0].window.used, 60.0);
        assert!(snap
            .plan_name
            .as_deref()
            .unwrap()
            .contains("DIEM 40.00 / 100.00"));
    }

    #[test]
    fn cannot_consume_renders_red_bar_with_hint() {
        let http = Arc::new(StubHttp::new(
            200,
            br#"{"canConsume": false, "balances": {}}"#,
        ));
        let resolver = Arc::new(StubResolver(Some("sk-x".into())));
        let strategy = VeniceApiStrategy::new(http, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows[0].window.used, 100.0);
        assert_eq!(
            snap.plan_name.as_deref(),
            Some("Balance unavailable for API calls")
        );
    }

    #[test]
    fn http_401_maps_to_unauthorized() {
        let http = Arc::new(StubHttp::new(401, b"{}"));
        let resolver = Arc::new(StubResolver(Some("sk".into())));
        let strategy = VeniceApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn missing_credentials_map_to_no_token() {
        let http = Arc::new(StubHttp::new(200, b"{}"));
        let resolver = Arc::new(StubResolver(None));
        let strategy = VeniceApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoToken("venice")));
    }
}
