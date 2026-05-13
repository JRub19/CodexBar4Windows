//! Moonshot API-key strategy.

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
use crate::providers::models::UsageSnapshot;
use crate::providers::moonshot::descriptor::{MoonshotRegion, MOONSHOT_ID};

pub const PER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[async_trait]
pub trait MoonshotHttp: Send + Sync {
    async fn get(&self, url: &str, bearer: &str) -> Result<MoonshotResponse, ProviderFetchError>;
}

pub struct MoonshotResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct MoonshotCredentials {
    pub api_key: String,
    pub region: MoonshotRegion,
}

#[async_trait]
pub trait MoonshotCredentialsResolver: Send + Sync {
    async fn resolve(&self) -> Result<Option<MoonshotCredentials>, ProviderFetchError>;
}

pub struct MoonshotApiStrategy {
    http: Arc<dyn MoonshotHttp>,
    creds: Arc<dyn MoonshotCredentialsResolver>,
}

impl MoonshotApiStrategy {
    pub fn new(http: Arc<dyn MoonshotHttp>, creds: Arc<dyn MoonshotCredentialsResolver>) -> Self {
        Self { http, creds }
    }
}

#[async_trait]
impl Strategy for MoonshotApiStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::ApiKey
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let creds = self
            .creds
            .resolve()
            .await?
            .filter(|c| !c.api_key.trim().is_empty())
            .ok_or(ProviderFetchError::NoToken("moonshot"))?;
        let url = creds.region.balance_url();
        let bearer = format!("Bearer {}", creds.api_key);

        let response = self.http.get(&url, &bearer).await?;
        let parsed: BalanceResponse = match response.status {
            200..=299 => serde_json::from_slice(&response.body)
                .map_err(|e| ProviderFetchError::ParseError(format!("/users/me/balance: {e}")))?,
            401 | 403 => return Err(ProviderFetchError::Unauthorized),
            other => {
                return Err(ProviderFetchError::Network(format!(
                    "moonshot balance returned {other}"
                )))
            }
        };

        if !parsed.is_ok() {
            return Err(ProviderFetchError::Network(format!(
                "moonshot API error code {} ({})",
                parsed.code, parsed.scode
            )));
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();

        let plan_name = if parsed.data.cash_balance < 0.0 {
            Some(format!(
                "Balance: ${:.2} · ${:.2} in deficit",
                parsed.data.available_balance,
                parsed.data.cash_balance.abs()
            ))
        } else {
            Some(format!("Balance: ${:.2}", parsed.data.available_balance))
        };

        Ok(UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(
                MOONSHOT_ID,
                format!("moonshot:key:{}", short_key_token(&creds.api_key)),
            ),
            windows: Vec::new(),
            credits: Some(CreditsSnapshot {
                balance: parsed.data.available_balance,
                unit: CreditUnit::UsdCents,
                recent_events: Vec::new(),
            }),
            cost: None,
            account_display_name: None,
            account_email: None,
            plan_name,
            captured_at_unix_secs: now,
        })
    }
}

fn short_key_token(api_key: &str) -> String {
    let trimmed = api_key.trim();
    let suffix = trimmed.strip_prefix("sk-").unwrap_or(trimmed);
    suffix
        .chars()
        .take(4)
        .collect::<String>()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct ScriptedHttp {
        reply: Mutex<Option<(u16, Vec<u8>)>>,
        captured_url: Mutex<Option<String>>,
    }

    #[async_trait]
    impl MoonshotHttp for ScriptedHttp {
        async fn get(&self, url: &str, _: &str) -> Result<MoonshotResponse, ProviderFetchError> {
            *self.captured_url.lock().unwrap() = Some(url.into());
            let (status, body) = self
                .reply
                .lock()
                .unwrap()
                .clone()
                .unwrap_or((404, b"{}".to_vec()));
            Ok(MoonshotResponse { status, body })
        }
    }

    struct StubResolver(Option<MoonshotCredentials>);
    #[async_trait]
    impl MoonshotCredentialsResolver for StubResolver {
        async fn resolve(&self) -> Result<Option<MoonshotCredentials>, ProviderFetchError> {
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
            provider_id: MOONSHOT_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    #[test]
    fn happy_path_international_region_hits_api_moonshot_ai() {
        let http = Arc::new(ScriptedHttp::default());
        *http.reply.lock().unwrap() = Some((
            200,
            br#"{
                "code": 0, "scode": "0", "status": true,
                "data": {"available_balance": 12.34, "voucher_balance": 2.0, "cash_balance": 10.34}
            }"#
            .to_vec(),
        ));
        let resolver = Arc::new(StubResolver(Some(MoonshotCredentials {
            api_key: "sk-abcd-1234".into(),
            region: MoonshotRegion::International,
        })));
        let strategy = MoonshotApiStrategy::new(http.clone(), resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.plan_name.as_deref(), Some("Balance: $12.34"));
        assert!((snap.credits.unwrap().balance - 12.34).abs() < 1e-9);
        assert_eq!(
            http.captured_url.lock().unwrap().as_deref(),
            Some("https://api.moonshot.ai/v1/users/me/balance")
        );
    }

    #[test]
    fn china_region_hits_api_moonshot_cn() {
        let http = Arc::new(ScriptedHttp::default());
        *http.reply.lock().unwrap() = Some((
            200,
            br#"{"code":0,"scode":"0","status":true,"data":{"available_balance":0,"voucher_balance":0,"cash_balance":0}}"#.to_vec(),
        ));
        let resolver = Arc::new(StubResolver(Some(MoonshotCredentials {
            api_key: "sk-x".into(),
            region: MoonshotRegion::China,
        })));
        let strategy = MoonshotApiStrategy::new(http.clone(), resolver);
        rt().block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(
            http.captured_url.lock().unwrap().as_deref(),
            Some("https://api.moonshot.cn/v1/users/me/balance")
        );
    }

    #[test]
    fn negative_cash_balance_renders_deficit_label() {
        let http = Arc::new(ScriptedHttp::default());
        *http.reply.lock().unwrap() = Some((
            200,
            br#"{
                "code": 0, "scode": "0", "status": true,
                "data": {"available_balance": 0.50, "voucher_balance": 5.00, "cash_balance": -4.50}
            }"#
            .to_vec(),
        ));
        let resolver = Arc::new(StubResolver(Some(MoonshotCredentials {
            api_key: "sk-x".into(),
            region: MoonshotRegion::International,
        })));
        let strategy = MoonshotApiStrategy::new(http, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(
            snap.plan_name.as_deref(),
            Some("Balance: $0.50 · $4.50 in deficit")
        );
    }

    #[test]
    fn http_401_maps_to_unauthorized() {
        let http = Arc::new(ScriptedHttp::default());
        *http.reply.lock().unwrap() = Some((401, b"{}".to_vec()));
        let resolver = Arc::new(StubResolver(Some(MoonshotCredentials {
            api_key: "sk-x".into(),
            region: MoonshotRegion::International,
        })));
        let strategy = MoonshotApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn api_level_failure_with_nonzero_code_maps_to_network_error() {
        let http = Arc::new(ScriptedHttp::default());
        *http.reply.lock().unwrap() = Some((
            200,
            br#"{
                "code": 1001, "scode": "AUTH_FAILED", "status": false,
                "data": {"available_balance": 0, "voucher_balance": 0, "cash_balance": 0}
            }"#
            .to_vec(),
        ));
        let resolver = Arc::new(StubResolver(Some(MoonshotCredentials {
            api_key: "sk-x".into(),
            region: MoonshotRegion::International,
        })));
        let strategy = MoonshotApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Network(_)));
    }

    #[test]
    fn missing_credentials_map_to_no_token() {
        let http = Arc::new(ScriptedHttp::default());
        let resolver = Arc::new(StubResolver(None));
        let strategy = MoonshotApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoToken("moonshot")));
    }
}
