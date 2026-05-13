//! OpenRouter API-key strategy. Ported from `OpenRouterUsageStats.swift`.
//!
//! Calls `GET /credits` (required) and `GET /key` (best-effort, bounded
//! to 1 s). Surfaces:
//! - `credits` snapshot with `balance` in USD and a single 30-day-empty
//!   placeholder series (we don't poll history yet).
//! - Optional primary rate window when the key has a hard limit, using
//!   `usage / limit` as the percent and the rate-limit interval as the
//!   label.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use super::response::{CreditsResponse, KeyData, KeyResponse};
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::credits::{CreditUnit, CreditsSnapshot};
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;
use crate::providers::openrouter::descriptor::OPENROUTER_ID;

pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub const CLIENT_TITLE: &str = "CodexBar4Windows";
pub const KEY_FETCH_TIMEOUT: Duration = Duration::from_secs(1);
pub const CREDITS_TIMEOUT: Duration = Duration::from_secs(15);

#[async_trait]
pub trait OpenRouterHttp: Send + Sync {
    async fn get(
        &self,
        url: &str,
        bearer: &str,
        headers: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<OpenRouterResponse, ProviderFetchError>;
}

pub struct OpenRouterResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[async_trait]
pub trait OpenRouterCredentialsResolver: Send + Sync {
    /// Returns the OpenRouter API key (`sk-or-v1-...`) and the optional
    /// base URL override. `None` signals NoToken.
    async fn resolve(&self) -> Result<Option<OpenRouterCredentials>, ProviderFetchError>;
}

#[derive(Clone, Debug)]
pub struct OpenRouterCredentials {
    pub api_key: String,
    pub base_url: Option<String>,
    pub http_referer: Option<String>,
    pub client_title: Option<String>,
}

pub struct OpenRouterApiStrategy {
    http: Arc<dyn OpenRouterHttp>,
    creds: Arc<dyn OpenRouterCredentialsResolver>,
}

impl OpenRouterApiStrategy {
    pub fn new(
        http: Arc<dyn OpenRouterHttp>,
        creds: Arc<dyn OpenRouterCredentialsResolver>,
    ) -> Self {
        Self { http, creds }
    }
}

#[async_trait]
impl Strategy for OpenRouterApiStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::ApiKey
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let creds = self
            .creds
            .resolve()
            .await?
            .ok_or(ProviderFetchError::NoToken("openrouter"))?;
        if creds.api_key.trim().is_empty() {
            return Err(ProviderFetchError::NoToken("openrouter"));
        }

        let base = creds.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL);
        let bearer = format!("Bearer {}", creds.api_key);
        let title = creds.client_title.as_deref().unwrap_or(CLIENT_TITLE);

        let mut headers: Vec<(&str, &str)> = vec![("Accept", "application/json"), ("X-Title", title)];
        if let Some(ref referer) = creds.http_referer {
            headers.push(("HTTP-Referer", referer.as_str()));
        }

        let credits_url = format!("{}/credits", base.trim_end_matches('/'));
        let credits_response = self
            .http
            .get(&credits_url, &bearer, &headers, CREDITS_TIMEOUT)
            .await?;
        let credits = match credits_response.status {
            200..=299 => {
                serde_json::from_slice::<CreditsResponse>(&credits_response.body)
                    .map_err(|e| ProviderFetchError::ParseError(format!("/credits: {e}")))?
                    .data
            }
            401 | 403 => return Err(ProviderFetchError::Unauthorized),
            other => {
                return Err(ProviderFetchError::Network(format!(
                    "openrouter /credits returned {other}"
                )))
            }
        };

        // /key is best-effort. Failure or timeout falls back to None.
        let key_data = self.fetch_key(&bearer, base, &headers).await;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();

        let mut windows = Vec::new();
        if let Some(ref key) = key_data {
            if let Some(used) = key.key_used_percent() {
                windows.push(NamedRateWindow {
                    key: "key".into(),
                    window: RateWindow {
                        label: rate_limit_label(key).unwrap_or_else(|| "Key".into()),
                        used,
                        allotted: Some(100.0),
                        reset_at_unix_secs: None,
                        pace_delta_percent: None,
                    },
                });
            }
        }
        // If no per-key cap, surface the overall credits-spent % so the
        // icon still has a primary signal.
        if windows.is_empty() {
            windows.push(NamedRateWindow {
                key: "credits".into(),
                window: RateWindow {
                    label: "Credits".into(),
                    used: credits.used_percent(),
                    allotted: Some(100.0),
                    reset_at_unix_secs: None,
                    pace_delta_percent: None,
                },
            });
        }

        let credits_snapshot = CreditsSnapshot {
            balance: credits.balance(),
            unit: CreditUnit::UsdCents,
            recent_events: Vec::new(),
        };

        Ok(UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(
                OPENROUTER_ID,
                format!("openrouter:key:{}", short_key_token(&creds.api_key)),
            ),
            windows,
            credits: Some(credits_snapshot),
            cost: None,
            account_display_name: None,
            account_email: None,
            plan_name: Some(format!("Balance: ${:.2}", credits.balance())),
            captured_at_unix_secs: now,
        })
    }
}

impl OpenRouterApiStrategy {
    async fn fetch_key(
        &self,
        bearer: &str,
        base: &str,
        headers: &[(&str, &str)],
    ) -> Option<KeyData> {
        let url = format!("{}/key", base.trim_end_matches('/'));
        let response = self.http.get(&url, bearer, headers, KEY_FETCH_TIMEOUT).await.ok()?;
        if !(200..=299).contains(&response.status) {
            return None;
        }
        serde_json::from_slice::<KeyResponse>(&response.body)
            .ok()
            .map(|r| r.data)
    }
}

fn rate_limit_label(data: &KeyData) -> Option<String> {
    let rl = data.rate_limit.as_ref()?;
    let interval = rl.interval.as_deref()?;
    Some(format!("Key ({interval})"))
}

/// `sk-or-v1-AbCdEf...` → `AbCd` so two keys can be distinguished in
/// the popup without leaking the secret.
fn short_key_token(api_key: &str) -> String {
    let trimmed = api_key.trim();
    let suffix = trimmed
        .strip_prefix("sk-or-v1-")
        .unwrap_or(trimmed);
    suffix.chars().take(4).collect::<String>().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    type CapturedCall = (String, Vec<(String, String)>);

    #[derive(Default)]
    struct ScriptedHttp {
        replies: Mutex<HashMap<String, (u16, Vec<u8>)>>,
        captured: Mutex<Vec<CapturedCall>>,
    }

    impl ScriptedHttp {
        fn put(&self, url: &str, status: u16, body: &[u8]) {
            self.replies
                .lock()
                .unwrap()
                .insert(url.into(), (status, body.to_vec()));
        }
    }

    #[async_trait]
    impl OpenRouterHttp for ScriptedHttp {
        async fn get(
            &self,
            url: &str,
            _bearer: &str,
            headers: &[(&str, &str)],
            _timeout: Duration,
        ) -> Result<OpenRouterResponse, ProviderFetchError> {
            self.captured.lock().unwrap().push((
                url.to_string(),
                headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ));
            let (status, body) = self
                .replies
                .lock()
                .unwrap()
                .get(url)
                .cloned()
                .unwrap_or((404, b"{}".to_vec()));
            Ok(OpenRouterResponse { status, body })
        }
    }

    struct StubResolver(Option<OpenRouterCredentials>);
    #[async_trait]
    impl OpenRouterCredentialsResolver for StubResolver {
        async fn resolve(&self) -> Result<Option<OpenRouterCredentials>, ProviderFetchError> {
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
            provider_id: OPENROUTER_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    #[test]
    fn happy_path_returns_key_window_credits_balance_and_plan_label() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(
            "https://openrouter.ai/api/v1/credits",
            200,
            br#"{"data": {"total_credits": 50.0, "total_usage": 12.5}}"#,
        );
        http.put(
            "https://openrouter.ai/api/v1/key",
            200,
            br#"{"data": {"limit": 100.0, "usage": 25.0, "rate_limit": {"requests": 60, "interval": "1m"}}}"#,
        );
        let resolver = Arc::new(StubResolver(Some(OpenRouterCredentials {
            api_key: "sk-or-v1-AbCdEfGh".into(),
            base_url: None,
            http_referer: None,
            client_title: None,
        })));
        let strategy = OpenRouterApiStrategy::new(http.clone(), resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].window.label, "Key (1m)");
        assert_eq!(snap.windows[0].window.used, 25.0);
        let credits = snap.credits.unwrap();
        assert_eq!(credits.balance, 37.5);
        assert_eq!(credits.unit, CreditUnit::UsdCents);
        assert_eq!(snap.plan_name.as_deref(), Some("Balance: $37.50"));
        assert_eq!(snap.identity.account_token, "openrouter:key:abcd");
    }

    #[test]
    fn falls_back_to_credits_window_when_key_endpoint_unavailable() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(
            "https://openrouter.ai/api/v1/credits",
            200,
            br#"{"data": {"total_credits": 100.0, "total_usage": 80.0}}"#,
        );
        // /key intentionally 404 → fetch_key returns None.
        let resolver = Arc::new(StubResolver(Some(OpenRouterCredentials {
            api_key: "sk-or-v1-z".into(),
            base_url: None,
            http_referer: None,
            client_title: None,
        })));
        let strategy = OpenRouterApiStrategy::new(http, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].window.label, "Credits");
        assert_eq!(snap.windows[0].window.used, 80.0);
    }

    #[test]
    fn http_401_maps_to_unauthorized() {
        let http = Arc::new(ScriptedHttp::default());
        http.put("https://openrouter.ai/api/v1/credits", 401, b"{}");
        let resolver = Arc::new(StubResolver(Some(OpenRouterCredentials {
            api_key: "sk".into(),
            base_url: None,
            http_referer: None,
            client_title: None,
        })));
        let strategy = OpenRouterApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn missing_credentials_map_to_no_token() {
        let http = Arc::new(ScriptedHttp::default());
        let resolver = Arc::new(StubResolver(None));
        let strategy = OpenRouterApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoToken("openrouter")));
    }

    #[test]
    fn empty_api_key_maps_to_no_token() {
        let http = Arc::new(ScriptedHttp::default());
        let resolver = Arc::new(StubResolver(Some(OpenRouterCredentials {
            api_key: "   ".into(),
            base_url: None,
            http_referer: None,
            client_title: None,
        })));
        let strategy = OpenRouterApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoToken("openrouter")));
    }

    #[test]
    fn custom_base_url_is_honoured() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(
            "https://gateway.example.com/v1/credits",
            200,
            br#"{"data": {"total_credits": 10.0, "total_usage": 1.0}}"#,
        );
        let resolver = Arc::new(StubResolver(Some(OpenRouterCredentials {
            api_key: "sk".into(),
            base_url: Some("https://gateway.example.com/v1".into()),
            http_referer: Some("https://app.example.com".into()),
            client_title: Some("MyApp".into()),
        })));
        let strategy = OpenRouterApiStrategy::new(http.clone(), resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert!(snap.credits.is_some());
        let captured = http.captured.lock().unwrap();
        let call = captured
            .iter()
            .find(|(u, _)| u == "https://gateway.example.com/v1/credits")
            .unwrap();
        assert!(call.1.iter().any(|(k, v)| k == "X-Title" && v == "MyApp"));
        assert!(call
            .1
            .iter()
            .any(|(k, v)| k == "HTTP-Referer" && v == "https://app.example.com"));
    }
}
