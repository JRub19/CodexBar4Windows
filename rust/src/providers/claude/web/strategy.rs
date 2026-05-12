//! `ClaudeWebStrategy`: uses a `Cookie: sessionKey=...` header to call
//! the same JSON endpoints the claude.ai web UI uses. Tries cookies in
//! Chrome → Edge → Brave → Firefox order (spec 40 section 3.1). Calls
//! the four endpoints from `endpoints.rs`, parses the relevant fields,
//! and folds everything into a `UsageSnapshot`.
//!
//! Cookie cache: read first, then fall back to a live browser import.
//! `401` and `403` clear the cached header; everything else leaves it.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;

use super::endpoints::{ClaudeWebEndpoint, HOST};
use super::org_selection::{pick, Organization};
use crate::providers::claude::descriptor::CLAUDE_ID;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;

pub const PER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Minimal cookie-aware HTTP transport. The default impl is reqwest;
/// tests pass in a stub.
#[async_trait]
pub trait WebClient: Send + Sync {
    async fn get_json(&self, url: &str, cookie: &str) -> Result<WebResponse, ProviderFetchError>;
}

pub struct WebResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Resolves a Cookie header for claude.ai. Production uses the cookie
/// importer chain; tests inject a static value.
#[async_trait]
pub trait CookieResolver: Send + Sync {
    async fn cookie(&self) -> Result<Option<String>, ProviderFetchError>;
    /// Called after the strategy receives a `401` or `403` so the
    /// resolver can clear any cached header.
    async fn invalidate(&self) -> Result<(), ProviderFetchError>;
}

pub struct ClaudeWebStrategy {
    client: Arc<dyn WebClient>,
    cookies: Arc<dyn CookieResolver>,
}

impl ClaudeWebStrategy {
    pub fn new(client: Arc<dyn WebClient>, cookies: Arc<dyn CookieResolver>) -> Self {
        Self { client, cookies }
    }
}

#[derive(Deserialize)]
struct UsageRollup {
    #[serde(default)]
    five_hour: Option<RateBucket>,
    #[serde(default)]
    seven_day: Option<RateBucket>,
    #[serde(default)]
    seven_day_opus: Option<RateBucket>,
}

#[derive(Deserialize)]
struct RateBucket {
    #[serde(default)]
    used: f64,
    #[serde(default)]
    allotted: Option<f64>,
    #[serde(default)]
    resets_at_epoch: Option<i64>,
}

#[derive(Deserialize)]
struct Subscription {
    #[serde(default)]
    plan_name: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

#[async_trait]
impl Strategy for ClaudeWebStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::Web
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let Some(cookie) = self.cookies.cookie().await? else {
            return Err(ProviderFetchError::NoCookies("claude"));
        };

        // Step 1: organizations.
        let orgs = self
            .get_parsed::<Vec<Organization>>(ClaudeWebEndpoint::Organizations, &cookie, None)
            .await?;
        let org = pick(&orgs).ok_or_else(|| {
            ProviderFetchError::UserConfigInvalid(
                "no Claude org with chat capability; sign in at claude.ai".into(),
            )
        })?;

        // Step 2-3: usage rollup and subscription. Breakdown is a nice
        // to have today (we will surface it on the cost chart in a later
        // commit) so we skip it for the bar/popup wiring.
        let rollup = self
            .get_parsed::<UsageRollup>(ClaudeWebEndpoint::UsageRollup, &cookie, Some(&org.uuid))
            .await?;
        let sub = self
            .get_parsed::<Subscription>(ClaudeWebEndpoint::Subscription, &cookie, Some(&org.uuid))
            .await
            .unwrap_or(Subscription {
                plan_name: None,
                email: None,
            });

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();

        let account_token = sub
            .email
            .clone()
            .map(|e| format!("claude:{e}"))
            .unwrap_or_else(|| format!("claude:org:{}", org.uuid));

        let mut windows = Vec::new();
        if let Some(b) = rollup.five_hour {
            windows.push(NamedRateWindow {
                key: "five_hour".into(),
                window: RateWindow {
                    label: "Session".into(),
                    used: b.used,
                    allotted: b.allotted,
                    reset_at_unix_secs: b.resets_at_epoch,
                    pace_delta_percent: None,
                },
            });
        }
        if let Some(b) = rollup.seven_day {
            windows.push(NamedRateWindow {
                key: "seven_day".into(),
                window: RateWindow {
                    label: "Week".into(),
                    used: b.used,
                    allotted: b.allotted,
                    reset_at_unix_secs: b.resets_at_epoch,
                    pace_delta_percent: None,
                },
            });
        }
        if let Some(b) = rollup.seven_day_opus {
            windows.push(NamedRateWindow {
                key: "seven_day_opus".into(),
                window: RateWindow {
                    label: "Week (Opus)".into(),
                    used: b.used,
                    allotted: b.allotted,
                    reset_at_unix_secs: b.resets_at_epoch,
                    pace_delta_percent: None,
                },
            });
        }

        Ok(UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(CLAUDE_ID, account_token),
            windows,
            credits: None,
            cost: None,
            account_display_name: org.name.clone(),
            account_email: sub.email,
            plan_name: sub.plan_name,
            captured_at_unix_secs: now,
        })
    }
}

impl ClaudeWebStrategy {
    async fn get_parsed<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: ClaudeWebEndpoint,
        cookie: &str,
        org_uuid: Option<&str>,
    ) -> Result<T, ProviderFetchError> {
        let mut path = endpoint.path().to_string();
        if let Some(uuid) = org_uuid {
            path = path.replace("{org}", uuid);
        }
        let url = format!("{HOST}{path}");
        let response = self.client.get_json(&url, cookie).await?;
        match response.status {
            200..=299 => serde_json::from_slice::<T>(&response.body)
                .map_err(|e| ProviderFetchError::ParseError(e.to_string())),
            401 => {
                let _ = self.cookies.invalidate().await;
                Err(ProviderFetchError::Unauthorized)
            }
            403 => {
                let _ = self.cookies.invalidate().await;
                Err(ProviderFetchError::PermissionDenied(format!(
                    "claude.ai returned 403 at {url}"
                )))
            }
            other => Err(ProviderFetchError::Network(format!(
                "unexpected status {other} from {url}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingClient {
        replies: Mutex<HashMap<String, (u16, Vec<u8>)>>,
        calls: Mutex<Vec<String>>,
    }

    impl RecordingClient {
        fn put(&self, url: &str, status: u16, body: &[u8]) {
            self.replies
                .lock()
                .unwrap()
                .insert(url.to_string(), (status, body.to_vec()));
        }
    }

    #[async_trait]
    impl WebClient for RecordingClient {
        async fn get_json(&self, url: &str, _: &str) -> Result<WebResponse, ProviderFetchError> {
            self.calls.lock().unwrap().push(url.to_string());
            let (status, body) = self
                .replies
                .lock()
                .unwrap()
                .get(url)
                .cloned()
                .unwrap_or((404, b"{}".to_vec()));
            Ok(WebResponse { status, body })
        }
    }

    struct StubCookies {
        value: Mutex<Option<String>>,
    }

    #[async_trait]
    impl CookieResolver for StubCookies {
        async fn cookie(&self) -> Result<Option<String>, ProviderFetchError> {
            Ok(self.value.lock().unwrap().clone())
        }
        async fn invalidate(&self) -> Result<(), ProviderFetchError> {
            *self.value.lock().unwrap() = None;
            Ok(())
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
            provider_id: CLAUDE_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    #[test]
    fn happy_path_picks_chat_org_and_returns_three_windows() {
        let client = Arc::new(RecordingClient::default());
        client.put(
            "https://claude.ai/api/organizations",
            200,
            br#"[
                {"uuid":"api","name":"Api","capabilities":["api"]},
                {"uuid":"chat","name":"ChatCo","capabilities":["chat","api"]}
            ]"#,
        );
        client.put(
            "https://claude.ai/api/organizations/chat/usage/rollup",
            200,
            br#"{
                "five_hour": {"used": 10.0, "allotted": 100.0},
                "seven_day": {"used": 100.0, "allotted": 1000.0},
                "seven_day_opus": {"used": 5.0, "allotted": 50.0}
            }"#,
        );
        client.put(
            "https://claude.ai/api/organizations/chat/subscription",
            200,
            br#"{"plan_name":"Pro","email":"u@x.com"}"#,
        );
        let cookies = Arc::new(StubCookies {
            value: Mutex::new(Some("sessionKey=fake".into())),
        });
        let strategy = ClaudeWebStrategy::new(client, cookies);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 3);
        assert_eq!(snap.plan_name.as_deref(), Some("Pro"));
        assert_eq!(snap.account_email.as_deref(), Some("u@x.com"));
        assert_eq!(snap.identity.account_token, "claude:u@x.com");
    }

    #[test]
    fn missing_cookie_maps_to_no_cookies() {
        let client = Arc::new(RecordingClient::default());
        let cookies = Arc::new(StubCookies {
            value: Mutex::new(None),
        });
        let strategy = ClaudeWebStrategy::new(client, cookies);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoCookies("claude")));
    }

    #[test]
    fn unauthorized_clears_cookie_cache() {
        let client = Arc::new(RecordingClient::default());
        client.put(
            "https://claude.ai/api/organizations",
            401,
            b"{\"error\":\"unauthorized\"}",
        );
        let cookies = Arc::new(StubCookies {
            value: Mutex::new(Some("sessionKey=fake".into())),
        });
        let strategy = ClaudeWebStrategy::new(client.clone(), cookies.clone());
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
        assert!(cookies.value.lock().unwrap().is_none());
    }

    #[test]
    fn no_chat_capable_org_raises_user_config_invalid() {
        let client = Arc::new(RecordingClient::default());
        client.put(
            "https://claude.ai/api/organizations",
            200,
            br#"[{"uuid":"api","name":"Api","capabilities":["api"]}]"#,
        );
        let cookies = Arc::new(StubCookies {
            value: Mutex::new(Some("sessionKey=fake".into())),
        });
        let strategy = ClaudeWebStrategy::new(client, cookies);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::UserConfigInvalid(_)));
    }
}
