//! Codex web strategy. Sends chatgpt.com session cookies to the same
//! `wham/usage` endpoint the OAuth path hits, and folds the response
//! into a `UsageSnapshot` with the same per-window decode tolerance.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;

use super::endpoints::{url, ACCOUNT_PATH, USAGE_PATH};
use crate::providers::claude::web::strategy::{CookieResolver, WebClient};
use crate::providers::codex::descriptor::CODEX_ID;
use crate::providers::codex::oauth::usage::windows_from_response;
use crate::providers::codex::oauth::wham_response::decode_tolerant;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::UsageSnapshot;

#[derive(Debug, Deserialize)]
struct AccountMe {
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    chatgpt_account_id: Option<String>,
    #[serde(default)]
    plan_type: Option<String>,
}

pub struct CodexWebStrategy {
    client: Arc<dyn WebClient>,
    cookies: Arc<dyn CookieResolver>,
}

impl CodexWebStrategy {
    pub fn new(client: Arc<dyn WebClient>, cookies: Arc<dyn CookieResolver>) -> Self {
        Self { client, cookies }
    }

    async fn fetch_account(&self, cookie: &str) -> Result<Option<AccountMe>, ProviderFetchError> {
        let response = self.client.get_json(&url(ACCOUNT_PATH), cookie).await?;
        match response.status {
            200..=299 => {
                let parsed: AccountMe = serde_json::from_slice(&response.body)
                    .map_err(|e| ProviderFetchError::ParseError(format!("/me decode: {e}")))?;
                Ok(Some(parsed))
            }
            401 => Err(ProviderFetchError::Unauthorized),
            403 => Err(ProviderFetchError::PermissionDenied(
                "chatgpt.com /me 403".into(),
            )),
            // /me is a nice-to-have; tolerate failures so the popup at
            // least gets the usage windows even if account data is gone.
            _ => Ok(None),
        }
    }
}

#[async_trait]
impl Strategy for CodexWebStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::Web
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let Some(cookie) = self.cookies.cookie().await? else {
            return Err(ProviderFetchError::NoCookies("codex"));
        };

        // 1. Pull usage rollup. Same endpoint as the OAuth path; only
        //    the auth header differs.
        let response = self.client.get_json(&url(USAGE_PATH), &cookie).await?;
        let (parsed, _flags) = match response.status {
            200..=299 => decode_tolerant(&response.body),
            401 => {
                let _ = self.cookies.invalidate().await;
                return Err(ProviderFetchError::Unauthorized);
            }
            403 => {
                let _ = self.cookies.invalidate().await;
                return Err(ProviderFetchError::PermissionDenied(
                    "chatgpt.com /wham/usage 403".into(),
                ));
            }
            other => {
                return Err(ProviderFetchError::Network(format!(
                    "unexpected status {other} from /wham/usage"
                )));
            }
        };

        // 2. Account info. Best-effort; failure does not block.
        let account = self.fetch_account(&cookie).await.unwrap_or(None);

        let windows = windows_from_response(&parsed);
        if windows.is_empty() && parsed.credits.is_none() {
            return Err(ProviderFetchError::ParseError(
                "wham/usage returned no windows and no credits".into(),
            ));
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();

        let account_token = account
            .as_ref()
            .and_then(|a| a.chatgpt_account_id.clone())
            .map(|id| format!("codex:{}", id.to_ascii_lowercase()))
            .or_else(|| {
                account
                    .as_ref()
                    .and_then(|a| a.email.clone())
                    .map(|e| format!("codex:{}", e.to_ascii_lowercase()))
            })
            .or_else(|| {
                parsed
                    .account
                    .as_ref()
                    .and_then(|a| a.email.clone())
                    .map(|e| format!("codex:{}", e.to_ascii_lowercase()))
            })
            .unwrap_or_else(|| "codex:cookie".into());

        Ok(UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(CODEX_ID, account_token),
            windows,
            credits: None,
            cost: None,
            account_display_name: account.as_ref().and_then(|a| a.name.clone()),
            account_email: account
                .as_ref()
                .and_then(|a| a.email.clone())
                .or_else(|| parsed.account.as_ref().and_then(|a| a.email.clone())),
            plan_name: account
                .as_ref()
                .and_then(|a| a.plan_type.clone())
                .or_else(|| parsed.account.as_ref().and_then(|a| a.plan_type.clone())),
            captured_at_unix_secs: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::claude::web::strategy::WebResponse;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct ScriptedClient {
        replies: Mutex<HashMap<String, (u16, Vec<u8>)>>,
    }
    impl ScriptedClient {
        fn put(&self, url: &str, status: u16, body: &[u8]) {
            self.replies
                .lock()
                .unwrap()
                .insert(url.to_string(), (status, body.to_vec()));
        }
    }

    #[async_trait]
    impl WebClient for ScriptedClient {
        async fn get_json(&self, url: &str, _: &str) -> Result<WebResponse, ProviderFetchError> {
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

    struct StubCookies(Mutex<Option<String>>);
    #[async_trait]
    impl CookieResolver for StubCookies {
        async fn cookie(&self) -> Result<Option<String>, ProviderFetchError> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn invalidate(&self) -> Result<(), ProviderFetchError> {
            *self.0.lock().unwrap() = None;
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
            provider_id: CODEX_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    #[test]
    fn happy_path_returns_windows_and_account_metadata() {
        let client = Arc::new(ScriptedClient::default());
        client.put(
            "https://chatgpt.com/backend-api/wham/usage",
            200,
            br#"{
                "primary_window": {"used": 5.0, "allotted": 100.0},
                "secondary_window": {"used": 50.0, "allotted": 1000.0},
                "account": {"email": "u@example.com", "plan_type": "plus", "account_id": "abc"}
            }"#,
        );
        client.put(
            "https://chatgpt.com/backend-api/me",
            200,
            br#"{"email":"u@example.com","name":"User","chatgpt_account_id":"abc","plan_type":"Plus"}"#,
        );
        let cookies = Arc::new(StubCookies(Mutex::new(Some(
            "__Secure-next-auth.session-token=eyJabc.def".into(),
        ))));
        let strategy = CodexWebStrategy::new(client, cookies);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 2);
        assert_eq!(snap.account_email.as_deref(), Some("u@example.com"));
        assert_eq!(snap.plan_name.as_deref(), Some("Plus"));
        assert_eq!(snap.identity.account_token, "codex:abc");
    }

    #[test]
    fn no_cookie_maps_to_no_cookies_error() {
        let client = Arc::new(ScriptedClient::default());
        let cookies = Arc::new(StubCookies(Mutex::new(None)));
        let strategy = CodexWebStrategy::new(client, cookies);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoCookies("codex")));
    }

    #[test]
    fn http_401_clears_cookie_cache() {
        let client = Arc::new(ScriptedClient::default());
        client.put(
            "https://chatgpt.com/backend-api/wham/usage",
            401,
            b"{\"error\":\"unauthorized\"}",
        );
        let cookies = Arc::new(StubCookies(Mutex::new(Some("cookie=value".into()))));
        let cookies_clone = cookies.clone();
        let strategy = CodexWebStrategy::new(client, cookies);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
        assert!(cookies_clone.0.lock().unwrap().is_none());
    }

    #[test]
    fn me_endpoint_failure_does_not_block_snapshot() {
        let client = Arc::new(ScriptedClient::default());
        client.put(
            "https://chatgpt.com/backend-api/wham/usage",
            200,
            br#"{
                "primary_window": {"used": 1.0, "allotted": 100.0},
                "account": {"email": "fallback@x.com"}
            }"#,
        );
        // /me intentionally absent → returns 404 from ScriptedClient.
        let cookies = Arc::new(StubCookies(Mutex::new(Some("c".into()))));
        let strategy = CodexWebStrategy::new(client, cookies);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.account_email.as_deref(), Some("fallback@x.com"));
    }
}
