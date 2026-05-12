//! `CodexOAuthStrategy`. Reads `~/.codex/auth.json`, sends the access
//! token to `chatgpt.com/backend-api/wham/usage`, and folds the
//! response into a `UsageSnapshot`. Live-verified on 2026-05-13.
//!
//! The endpoint requires `User-Agent: codex_cli_rs/<version>` plus
//! the `ChatGPT-Account-Id` header from `tokens.account_id`. The
//! reqwest transport wires those automatically.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use super::usage::{fetch_usage, UsageHttp, UsageRequest, DEFAULT_ENDPOINT};
use crate::core::ProviderId;
use crate::providers::codex::auth::credentials::CodexCredentials;
use crate::providers::codex::auth::errors::CodexOAuthError;
use crate::providers::codex::oauth::usage::windows_from_response;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::UsageSnapshot;

#[async_trait]
pub trait OAuthCredentialsResolver: Send + Sync {
    /// Returns the full credentials bundle from `~/.codex/auth.json`
    /// (or any equivalent source). Returning `None` signals
    /// `NoToken` to the framework.
    async fn resolve(&self) -> Result<CodexCredentials, CodexOAuthError>;
}

pub struct CodexOAuthStrategy {
    http: Arc<dyn UsageHttp>,
    resolver: Arc<dyn OAuthCredentialsResolver>,
    endpoint: String,
}

impl CodexOAuthStrategy {
    pub fn new(http: Arc<dyn UsageHttp>, resolver: Arc<dyn OAuthCredentialsResolver>) -> Self {
        Self {
            http,
            resolver,
            endpoint: DEFAULT_ENDPOINT.to_string(),
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }
}

#[async_trait]
impl Strategy for CodexOAuthStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::OAuth
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let creds = self
            .resolver
            .resolve()
            .await
            .map_err(codex_to_fetch_error)?;
        let full = creds
            .as_full()
            .ok_or(ProviderFetchError::NoToken("codex"))?;
        // The Codex CLI stores `tokens.account_id` next to the access
        // token; the wham endpoint requires it as `ChatGPT-Account-Id`.
        let account_id = parse_account_id(&full.id_token);
        let request = UsageRequest {
            access_token: &full.access_token,
            account_id: account_id.as_deref(),
        };
        let (parsed, _flags) = fetch_usage(self.http.as_ref(), &self.endpoint, request)
            .await
            .map_err(codex_to_fetch_error)?;
        let windows = windows_from_response(&parsed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let account_token = parsed
            .account_id
            .clone()
            .map(|id| format!("codex:{}", id.to_ascii_lowercase()))
            .or_else(|| {
                parsed
                    .email
                    .clone()
                    .map(|e| format!("codex:{}", e.to_ascii_lowercase()))
            })
            .unwrap_or_else(|| "codex:oauth".into());
        Ok(UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(ProviderId("codex"), account_token),
            windows,
            credits: None,
            cost: None,
            account_display_name: None,
            account_email: parsed.email,
            plan_name: parsed.plan_type,
            captured_at_unix_secs: now,
        })
    }
}

/// Extract the `chatgpt_account_id` claim from a Codex JWT id token,
/// if present. Failure to parse is fine — the wham endpoint also
/// accepts requests without the header for single-account users.
fn parse_account_id(id_token: &str) -> Option<String> {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    use base64::Engine;
    let padded = pad_base64(parts[1]);
    let bytes = base64::engine::general_purpose::URL_SAFE
        .decode(padded.as_bytes())
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    // Standard claim used by Codex.
    if let Some(id) = value.get("chatgpt_account_id").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    // Anthropic-style namespaced claim, kept here for forward-compat.
    value
        .get("https://api.openai.com/auth")
        .and_then(|v| v.get("chatgpt_account_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn pad_base64(input: &str) -> String {
    let mut s: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    match s.len() % 4 {
        0 => s,
        n => {
            s.push_str(&"=".repeat(4 - n));
            s
        }
    }
}

fn codex_to_fetch_error(err: CodexOAuthError) -> ProviderFetchError {
    match err {
        CodexOAuthError::CredentialsNotFound | CodexOAuthError::CredentialsMissingTokens => {
            ProviderFetchError::NoToken("codex")
        }
        CodexOAuthError::Unauthorized => ProviderFetchError::Unauthorized,
        CodexOAuthError::ServerError(s) => {
            ProviderFetchError::Network(format!("codex usage returned {s}"))
        }
        CodexOAuthError::NetworkError(detail) => ProviderFetchError::Network(detail),
        CodexOAuthError::DecodeFailed(detail) => ProviderFetchError::ParseError(detail),
        CodexOAuthError::InvalidResponse => {
            ProviderFetchError::ParseError("wham/usage returned no windows".into())
        }
        CodexOAuthError::RefreshExpired(detail) => ProviderFetchError::UserConfigInvalid(detail),
        CodexOAuthError::RefreshRevoked | CodexOAuthError::RefreshReused => {
            ProviderFetchError::UserConfigInvalid(
                "codex refresh token revoked or reused; run `codex login` again".into(),
            )
        }
        CodexOAuthError::RefreshNetworkError(detail) => ProviderFetchError::Network(detail),
        CodexOAuthError::RefreshInvalidResponse(detail) => ProviderFetchError::ParseError(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::codex::auth::credentials::CodexCredentialsFull;
    use crate::providers::codex::oauth::usage::UsageResponse;
    use std::collections::HashMap;
    use std::sync::Mutex;

    type CapturedHeaders = Vec<(String, String)>;
    type CapturedCall = (String, CapturedHeaders);

    struct StubResolver(CodexCredentials);
    #[async_trait]
    impl OAuthCredentialsResolver for StubResolver {
        async fn resolve(&self) -> Result<CodexCredentials, CodexOAuthError> {
            Ok(self.0.clone())
        }
    }

    #[derive(Default)]
    struct StubHttp {
        replies: Mutex<HashMap<String, (u16, Vec<u8>)>>,
        captured: Mutex<Vec<CapturedCall>>,
    }

    impl StubHttp {
        fn put(&self, url: &str, status: u16, body: &[u8]) {
            self.replies
                .lock()
                .unwrap()
                .insert(url.into(), (status, body.to_vec()));
        }
    }

    #[async_trait]
    impl UsageHttp for StubHttp {
        async fn get(
            &self,
            url: &str,
            headers: &[(&str, &str)],
        ) -> Result<UsageResponse, CodexOAuthError> {
            self.captured.lock().unwrap().push((
                url.into(),
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
            Ok(UsageResponse { status, body })
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn ctx() -> ProviderFetchContext {
        use crate::providers::codex::descriptor::CODEX_ID;
        use crate::providers::fetch_context::{Runtime, SourceMode};
        use crate::secrets::token_account::TokenAccountStore;
        let tokens = Arc::new(TokenAccountStore::new(std::env::temp_dir()));
        ProviderFetchContext {
            provider_id: CODEX_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    fn live_creds() -> CodexCredentials {
        CodexCredentials::Full(CodexCredentialsFull {
            access_token: "ek-cb1.access".into(),
            refresh_token: "ek-cb1.refresh".into(),
            id_token: "header.eyJzdWIiOiJ1Iiwic2NvcGUiOiJ1c2VyIn0.sig".into(),
            last_refresh: None,
            openai_api_key: None,
        })
    }

    #[test]
    fn happy_path_returns_windows_and_account_metadata() {
        let http = Arc::new(StubHttp::default());
        http.put(
            DEFAULT_ENDPOINT,
            200,
            br#"{
                "user_id":"user-1","account_id":"user-1","email":"u@x.com","plan_type":"plus",
                "rate_limit": {
                    "allowed": true,
                    "primary_window": {"used_percent": 12, "reset_at": 1778645804},
                    "secondary_window": {"used_percent": 5, "reset_at": 1778773780}
                }
            }"#,
        );
        let resolver = Arc::new(StubResolver(live_creds()));
        let strategy = CodexOAuthStrategy::new(http, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 2);
        assert_eq!(snap.windows[0].window.used, 12.0);
        assert_eq!(snap.account_email.as_deref(), Some("u@x.com"));
        assert_eq!(snap.plan_name.as_deref(), Some("plus"));
        assert_eq!(snap.identity.account_token, "codex:user-1");
    }

    #[test]
    fn http_401_maps_to_unauthorized() {
        let http = Arc::new(StubHttp::default());
        http.put(DEFAULT_ENDPOINT, 401, b"{}");
        let resolver = Arc::new(StubResolver(live_creds()));
        let strategy = CodexOAuthStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn api_key_only_yields_no_token() {
        let http = Arc::new(StubHttp::default());
        let resolver = Arc::new(StubResolver(CodexCredentials::ApiKeyOnly("sk".into())));
        let strategy = CodexOAuthStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoToken("codex")));
    }
}
