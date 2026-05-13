//! DeepSeek API-key strategy. Ported from
//! `Sources/CodexBarCore/Providers/DeepSeek/DeepSeekUsageFetcher.swift`.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use super::response::{pick_balance, BalanceResponse};
use crate::providers::deepseek::descriptor::DEEPSEEK_ID;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::credits::{CreditUnit, CreditsSnapshot};
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;

pub const BALANCE_URL: &str = "https://api.deepseek.com/user/balance";
pub const PER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[async_trait]
pub trait DeepSeekHttp: Send + Sync {
    async fn get(
        &self,
        url: &str,
        bearer: &str,
    ) -> Result<DeepSeekResponse, ProviderFetchError>;
}

pub struct DeepSeekResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[async_trait]
pub trait DeepSeekCredentialsResolver: Send + Sync {
    /// Returns the active API key, or `None` to signal NoToken.
    async fn resolve(&self) -> Result<Option<String>, ProviderFetchError>;
}

pub struct DeepSeekApiStrategy {
    http: Arc<dyn DeepSeekHttp>,
    creds: Arc<dyn DeepSeekCredentialsResolver>,
}

impl DeepSeekApiStrategy {
    pub fn new(http: Arc<dyn DeepSeekHttp>, creds: Arc<dyn DeepSeekCredentialsResolver>) -> Self {
        Self { http, creds }
    }
}

#[async_trait]
impl Strategy for DeepSeekApiStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::ApiKey
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let api_key = self
            .creds
            .resolve()
            .await?
            .filter(|k| !k.trim().is_empty())
            .ok_or(ProviderFetchError::NoToken("deepseek"))?;
        let bearer = format!("Bearer {api_key}");

        let response = self.http.get(BALANCE_URL, &bearer).await?;
        let parsed: BalanceResponse = match response.status {
            200..=299 => serde_json::from_slice(&response.body)
                .map_err(|e| ProviderFetchError::ParseError(format!("/user/balance: {e}")))?,
            401 | 403 => return Err(ProviderFetchError::Unauthorized),
            other => {
                return Err(ProviderFetchError::Network(format!(
                    "deepseek /user/balance returned {other}"
                )))
            }
        };

        let mut balances = Vec::with_capacity(parsed.balance_infos.len());
        for info in &parsed.balance_infos {
            balances.push(info.parse().map_err(ProviderFetchError::ParseError)?);
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();

        let mut snap = UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(
                DEEPSEEK_ID,
                format!("deepseek:key:{}", short_key_token(&api_key)),
            ),
            windows: Vec::new(),
            credits: None,
            cost: None,
            account_display_name: None,
            account_email: None,
            plan_name: None,
            captured_at_unix_secs: now,
        };

        let Some(picked) = pick_balance(&balances) else {
            // Empty balance_infos array — surface as a 0% Balance window
            // so the popup is not blank.
            snap.windows.push(NamedRateWindow {
                key: "balance".into(),
                window: RateWindow {
                    label: "Balance".into(),
                    used: 100.0,
                    allotted: Some(100.0),
                    reset_at_unix_secs: None,
                    pace_delta_percent: None,
                },
            });
            snap.plan_name = Some("No balance".into());
            return Ok(snap);
        };

        // The DeepSeek popup leads with a balance summary. We use a
        // "Balance" window where `used = 100%` for empty balances and
        // `used = 0%` for funded balances, matching the macOS app's
        // visual cue (red bar when out of credits, full green when not).
        let used = if picked.total <= 0.0 || !parsed.is_available {
            100.0
        } else {
            0.0
        };
        snap.windows.push(NamedRateWindow {
            key: "balance".into(),
            window: RateWindow {
                label: "Balance".into(),
                used,
                allotted: Some(100.0),
                reset_at_unix_secs: None,
                pace_delta_percent: None,
            },
        });
        snap.credits = Some(CreditsSnapshot {
            balance: picked.total,
            unit: CreditUnit::UsdCents,
            recent_events: Vec::new(),
        });
        snap.plan_name = Some(format_plan_label(picked, parsed.is_available));
        Ok(snap)
    }
}

fn format_plan_label(balance: &super::response::ParsedBalance, available: bool) -> String {
    let symbol = if balance.currency == "CNY" { "¥" } else { "$" };
    if balance.total <= 0.0 {
        return format!("{symbol}0.00 — add credits at platform.deepseek.com");
    }
    if !available {
        return "Balance unavailable for API calls".into();
    }
    format!(
        "{symbol}{:.2} (Paid: {symbol}{:.2} / Granted: {symbol}{:.2})",
        balance.total, balance.topped_up, balance.granted
    )
}

fn short_key_token(api_key: &str) -> String {
    let trimmed = api_key.trim();
    let suffix = trimmed.strip_prefix("sk-").unwrap_or(trimmed);
    suffix.chars().take(4).collect::<String>().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct ScriptedHttp {
        reply: Mutex<Option<(u16, Vec<u8>)>>,
    }

    #[async_trait]
    impl DeepSeekHttp for ScriptedHttp {
        async fn get(
            &self,
            _: &str,
            _: &str,
        ) -> Result<DeepSeekResponse, ProviderFetchError> {
            let (status, body) = self
                .reply
                .lock()
                .unwrap()
                .clone()
                .unwrap_or((404, b"{}".to_vec()));
            Ok(DeepSeekResponse { status, body })
        }
    }

    struct StubResolver(Option<String>);
    #[async_trait]
    impl DeepSeekCredentialsResolver for StubResolver {
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
            provider_id: DEEPSEEK_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    fn put(http: &ScriptedHttp, status: u16, body: &[u8]) {
        *http.reply.lock().unwrap() = Some((status, body.to_vec()));
    }

    #[test]
    fn happy_path_returns_balance_credits_and_plan_label() {
        let http = Arc::new(ScriptedHttp::default());
        put(
            &http,
            200,
            br#"{
                "is_available": true,
                "balance_infos": [
                    {"currency":"USD","total_balance":"12.34","granted_balance":"5.00","topped_up_balance":"7.34"}
                ]
            }"#,
        );
        let resolver = Arc::new(StubResolver(Some("sk-abcd-1234".into())));
        let strategy = DeepSeekApiStrategy::new(http, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].window.label, "Balance");
        assert_eq!(snap.windows[0].window.used, 0.0);
        let credits = snap.credits.unwrap();
        assert_eq!(credits.unit, CreditUnit::UsdCents);
        assert!((credits.balance - 12.34).abs() < 1e-9);
        assert_eq!(
            snap.plan_name.as_deref(),
            Some("$12.34 (Paid: $7.34 / Granted: $5.00)")
        );
        assert_eq!(snap.identity.account_token, "deepseek:key:abcd");
    }

    #[test]
    fn empty_balance_shows_red_bar_with_topup_hint() {
        let http = Arc::new(ScriptedHttp::default());
        put(
            &http,
            200,
            br#"{
                "is_available": true,
                "balance_infos": [
                    {"currency":"USD","total_balance":"0","granted_balance":"0","topped_up_balance":"0"}
                ]
            }"#,
        );
        let resolver = Arc::new(StubResolver(Some("sk-x".into())));
        let strategy = DeepSeekApiStrategy::new(http, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows[0].window.used, 100.0);
        assert_eq!(
            snap.plan_name.as_deref(),
            Some("$0.00 — add credits at platform.deepseek.com")
        );
    }

    #[test]
    fn cny_balance_renders_with_yuan_symbol() {
        let http = Arc::new(ScriptedHttp::default());
        put(
            &http,
            200,
            br#"{
                "is_available": true,
                "balance_infos": [
                    {"currency":"CNY","total_balance":"99.00","granted_balance":"99.00","topped_up_balance":"0.00"}
                ]
            }"#,
        );
        let resolver = Arc::new(StubResolver(Some("sk-x".into())));
        let strategy = DeepSeekApiStrategy::new(http, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert!(snap.plan_name.as_deref().unwrap().starts_with("¥99.00"));
    }

    #[test]
    fn http_401_maps_to_unauthorized() {
        let http = Arc::new(ScriptedHttp::default());
        put(&http, 401, b"{}");
        let resolver = Arc::new(StubResolver(Some("sk-x".into())));
        let strategy = DeepSeekApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn missing_credentials_map_to_no_token() {
        let http = Arc::new(ScriptedHttp::default());
        let resolver = Arc::new(StubResolver(None));
        let strategy = DeepSeekApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoToken("deepseek")));
    }
}
