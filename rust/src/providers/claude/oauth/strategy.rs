//! `ClaudeOAuthStrategy`. Calls `GET /api/oauth/usage` with the resolved
//! bearer token, folds the response into a framework `UsageSnapshot`,
//! and maps HTTP-level errors to the typed `ProviderFetchError` variants
//! the runtime understands.
//!
//! The strategy takes a `HttpClient` trait object so tests can inject a
//! deterministic fake; production code wires it to `reqwest::Client`.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use crate::providers::claude::descriptor::CLAUDE_ID;
use crate::providers::claude::errors::CredentialError;
use crate::providers::claude::models::{account_token_for, fold_oauth};
use crate::providers::claude::oauth::credentials::OAuthCredentials;
use crate::providers::claude::oauth::response::OAuthUsageResponse;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::models::UsageSnapshot;

pub const USAGE_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
pub const ANTHROPIC_BETA_HEADER: &str = "oauth-2025-04-20";
pub const PER_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Minimal HTTP transport. The reqwest-backed implementation lives in
/// `transport.rs`; tests substitute a recording fake.
#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn get_json(&self, url: &str, bearer: &str) -> Result<HttpResponse, ProviderFetchError>;
}

pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

pub struct ClaudeOAuthStrategy {
    client: Arc<dyn HttpClient>,
    credentials_resolver: Arc<dyn CredentialsResolver>,
}

/// Pluggable credential resolver. Production uses
/// [`crate::providers::claude::oauth::credentials::resolve`] wired
/// through the secrets layer; tests inject a static bundle.
#[async_trait]
pub trait CredentialsResolver: Send + Sync {
    async fn resolve(&self) -> Result<OAuthCredentials, CredentialError>;
}

impl ClaudeOAuthStrategy {
    pub fn new(
        client: Arc<dyn HttpClient>,
        credentials_resolver: Arc<dyn CredentialsResolver>,
    ) -> Self {
        Self {
            client,
            credentials_resolver,
        }
    }
}

#[async_trait]
impl Strategy for ClaudeOAuthStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::OAuth
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let creds = self
            .credentials_resolver
            .resolve()
            .await
            .map_err(|err| match err {
                CredentialError::Missing => ProviderFetchError::NoToken("claude"),
                CredentialError::MissingScope(scope) => {
                    ProviderFetchError::UserConfigInvalid(format!(
                        "oauth token missing required scope {scope}; reauthenticate in Claude Code"
                    ))
                }
                CredentialError::DecodeFailed(detail) => ProviderFetchError::ParseError(detail),
                CredentialError::Io { path, source } => ProviderFetchError::Network(format!(
                    "credentials read failed at {path:?}: {source}"
                )),
                CredentialError::Cache(detail) => ProviderFetchError::Network(detail),
            })?;
        let response = self
            .client
            .get_json(USAGE_ENDPOINT, &creds.access_token)
            .await?;
        if response.status == 401 {
            return Err(ProviderFetchError::Unauthorized);
        }
        if response.status == 403 {
            // Spec 40 section 2.3: when the body mentions the missing
            // scope, surface a config error instead of a permission
            // error so the popup hints the user to reauth.
            let body = String::from_utf8_lossy(&response.body);
            if body.contains("user:profile") {
                return Err(ProviderFetchError::UserConfigInvalid(
                    "oauth token missing required scope user:profile".into(),
                ));
            }
            return Err(ProviderFetchError::PermissionDenied(
                "claude returned 403".into(),
            ));
        }
        if !(200..300).contains(&response.status) {
            return Err(ProviderFetchError::Network(format!(
                "unexpected status {} from {}",
                response.status, USAGE_ENDPOINT
            )));
        }
        let parsed: OAuthUsageResponse = serde_json::from_slice(&response.body)
            .map_err(|e| ProviderFetchError::ParseError(e.to_string()))?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let token = account_token_for(&parsed);
        let mut snapshot = fold_oauth(&parsed, token, now);
        // Re-anchor the identity to the canonical provider id so the
        // UsageStore identity check stays well-formed.
        snapshot.identity.provider_id = CLAUDE_ID.as_str().to_string();
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct StubResolver {
        creds: OAuthCredentials,
    }

    #[async_trait]
    impl CredentialsResolver for StubResolver {
        async fn resolve(&self) -> Result<OAuthCredentials, CredentialError> {
            Ok(self.creds.clone())
        }
    }

    struct FailingResolver(CredentialError);

    #[async_trait]
    impl CredentialsResolver for FailingResolver {
        async fn resolve(&self) -> Result<OAuthCredentials, CredentialError> {
            Err(match &self.0 {
                CredentialError::Missing => CredentialError::Missing,
                CredentialError::MissingScope(s) => CredentialError::MissingScope(s),
                _ => CredentialError::Missing,
            })
        }
    }

    struct StubClient {
        response: Mutex<Option<HttpResponse>>,
    }

    impl StubClient {
        fn with(status: u16, body: &[u8]) -> Self {
            Self {
                response: Mutex::new(Some(HttpResponse {
                    status,
                    body: body.to_vec(),
                })),
            }
        }
    }

    #[async_trait]
    impl HttpClient for StubClient {
        async fn get_json(&self, _: &str, _: &str) -> Result<HttpResponse, ProviderFetchError> {
            Ok(self
                .response
                .lock()
                .unwrap()
                .take()
                .expect("stub used twice"))
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

    fn good_credentials() -> OAuthCredentials {
        OAuthCredentials {
            access_token: "sk-ant-oat-fake".into(),
            refresh_token: None,
            expires_at_unix_secs: None,
            scopes: vec!["user:profile".into()],
        }
    }

    #[test]
    fn happy_path_returns_three_windows_with_account_metadata() {
        let body = br#"{
            "five_hour": {"used": 12.5, "allotted": 100.0},
            "seven_day": {"used": 250.0, "allotted": 1000.0},
            "seven_day_opus": {"used": 30.0, "allotted": 100.0},
            "account": {"email": "jonas@skrylabs.com", "plan": "Max"}
        }"#;
        let resolver = Arc::new(StubResolver {
            creds: good_credentials(),
        });
        let client = Arc::new(StubClient::with(200, body));
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 3);
        assert_eq!(snap.account_email.as_deref(), Some("jonas@skrylabs.com"));
        assert_eq!(snap.plan_name.as_deref(), Some("Max"));
        assert_eq!(snap.identity.provider_id, "claude");
        assert_eq!(snap.identity.account_token, "claude:jonas@skrylabs.com");
    }

    #[test]
    fn http_401_maps_to_unauthorized() {
        let resolver = Arc::new(StubResolver {
            creds: good_credentials(),
        });
        let client = Arc::new(StubClient::with(401, b"{}"));
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn http_403_with_scope_in_body_maps_to_user_config_invalid() {
        let resolver = Arc::new(StubResolver {
            creds: good_credentials(),
        });
        let client = Arc::new(StubClient::with(
            403,
            br#"{"error":"oauth token missing user:profile scope"}"#,
        ));
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::UserConfigInvalid(_)));
    }

    #[test]
    fn http_403_without_scope_message_maps_to_permission_denied() {
        let resolver = Arc::new(StubResolver {
            creds: good_credentials(),
        });
        let client = Arc::new(StubClient::with(403, b"forbidden"));
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::PermissionDenied(_)));
    }

    #[test]
    fn missing_credentials_maps_to_no_token() {
        let resolver = Arc::new(FailingResolver(CredentialError::Missing));
        let client = Arc::new(StubClient::with(200, b"{}"));
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoToken("claude")));
    }

    #[test]
    fn malformed_response_maps_to_parse_error() {
        let resolver = Arc::new(StubResolver {
            creds: good_credentials(),
        });
        let client = Arc::new(StubClient::with(200, b"not json"));
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::ParseError(_)));
    }
}
