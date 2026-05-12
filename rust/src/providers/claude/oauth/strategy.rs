//! `ClaudeOAuthStrategy`. Calls `GET /api/oauth/usage` and
//! `GET /api/oauth/account` on `api.anthropic.com` with the resolved
//! bearer token, then folds both responses into a framework
//! `UsageSnapshot`.
//!
//! Live-verified on 2026-05-13. Anthropic requires `User-Agent:
//! claude-code/<version>`; any other UA returns 429 with an empty
//! body. The reqwest transport (`transport.rs`) handles that.
//!
//! The strategy takes pluggable `HttpClient` and `CredentialsResolver`
//! traits so tests can drive every error branch with stubs.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;

use crate::providers::claude::descriptor::CLAUDE_ID;
use crate::providers::claude::errors::CredentialError;
use crate::providers::claude::models::{account_token_for, fold_oauth, AccountSummary};
use crate::providers::claude::oauth::credentials::OAuthCredentials;
use crate::providers::claude::oauth::response::OAuthUsageResponse;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::models::UsageSnapshot;

pub const USAGE_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
pub const ACCOUNT_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/account";
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

#[derive(Debug, Deserialize)]
struct AccountWire {
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    email_address: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    memberships: Vec<MembershipWire>,
}

#[derive(Debug, Deserialize)]
struct MembershipWire {
    #[serde(default)]
    organization: Option<OrganizationWire>,
}

#[derive(Debug, Deserialize)]
struct OrganizationWire {
    #[serde(default)]
    rate_limit_tier: Option<String>,
}

fn account_from_wire(wire: AccountWire) -> AccountSummary {
    let plan = wire
        .memberships
        .iter()
        .find_map(|m| {
            m.organization
                .as_ref()
                .and_then(|o| o.rate_limit_tier.clone())
        })
        .map(prettify_plan_label);
    AccountSummary {
        email: wire.email_address,
        display_name: wire.display_name,
        plan_name: plan,
        account_uuid: wire.uuid,
    }
}

/// Anthropic's `rate_limit_tier` reads like `default_claude_max_20x`.
/// We strip the `default_claude_` prefix so the popup shows the
/// human-readable variant ("max_20x" -> "Max 20x").
fn prettify_plan_label(raw: String) -> String {
    let cleaned = raw.trim_start_matches("default_claude_").replace('_', " ");
    if cleaned.is_empty() {
        return raw;
    }
    let mut chars = cleaned.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => cleaned,
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

        // /api/oauth/usage — primary windows.
        let usage_response = self
            .client
            .get_json(USAGE_ENDPOINT, &creds.access_token)
            .await?;
        Self::translate_status(USAGE_ENDPOINT, &usage_response)?;
        let usage: OAuthUsageResponse = serde_json::from_slice(&usage_response.body)
            .map_err(|e| ProviderFetchError::ParseError(format!("/usage decode: {e}")))?;

        // /api/oauth/account — account metadata (email, plan).
        let account = match self
            .client
            .get_json(ACCOUNT_ENDPOINT, &creds.access_token)
            .await
        {
            Ok(response) if (200..300).contains(&response.status) => {
                serde_json::from_slice::<AccountWire>(&response.body)
                    .map(account_from_wire)
                    .unwrap_or_default()
            }
            // Account failures degrade the snapshot gracefully — the
            // popup still gets windows and resets even when /account
            // 4xx's (e.g., rate-limited).
            _ => AccountSummary::default(),
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let token = account_token_for(&account);
        let mut snapshot = fold_oauth(&usage, &account, token, now);
        snapshot.identity.provider_id = CLAUDE_ID.as_str().to_string();
        Ok(snapshot)
    }
}

impl ClaudeOAuthStrategy {
    fn translate_status(url: &str, response: &HttpResponse) -> Result<(), ProviderFetchError> {
        match response.status {
            200..=299 => Ok(()),
            401 => Err(ProviderFetchError::Unauthorized),
            403 => {
                let body = String::from_utf8_lossy(&response.body);
                if body.contains("user:profile") {
                    Err(ProviderFetchError::UserConfigInvalid(
                        "oauth token missing required scope user:profile".into(),
                    ))
                } else {
                    Err(ProviderFetchError::PermissionDenied(format!(
                        "{url} returned 403"
                    )))
                }
            }
            429 => Err(ProviderFetchError::Network(format!(
                "{url} returned 429 (likely missing claude-code/<version> User-Agent)"
            ))),
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
        responses: Mutex<HashMap<String, (u16, Vec<u8>)>>,
    }

    impl StubClient {
        fn new() -> Self {
            Self {
                responses: Mutex::new(HashMap::new()),
            }
        }
        fn put(&self, url: &str, status: u16, body: &[u8]) {
            self.responses
                .lock()
                .unwrap()
                .insert(url.into(), (status, body.to_vec()));
        }
    }

    #[async_trait]
    impl HttpClient for StubClient {
        async fn get_json(&self, url: &str, _: &str) -> Result<HttpResponse, ProviderFetchError> {
            let (status, body) = self
                .responses
                .lock()
                .unwrap()
                .get(url)
                .cloned()
                .unwrap_or((404, b"{}".to_vec()));
            Ok(HttpResponse { status, body })
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
    fn happy_path_returns_windows_plus_metadata_from_two_endpoints() {
        let client = Arc::new(StubClient::new());
        client.put(
            USAGE_ENDPOINT,
            200,
            br#"{"five_hour":{"utilization":25.0,"resets_at":"2026-05-12T23:20:00+00:00"},
                 "seven_day":{"utilization":32.0,"resets_at":"2026-05-18T10:00:00+00:00"}}"#,
        );
        client.put(
            ACCOUNT_ENDPOINT,
            200,
            br#"{"uuid":"acct-uuid","email_address":"u@example.com","display_name":"User",
                 "memberships":[{"organization":{"rate_limit_tier":"default_claude_max_20x"}}]}"#,
        );
        let resolver = Arc::new(StubResolver {
            creds: good_credentials(),
        });
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 2);
        assert_eq!(snap.windows[0].window.used, 25.0);
        assert_eq!(snap.account_email.as_deref(), Some("u@example.com"));
        assert_eq!(snap.plan_name.as_deref(), Some("Max 20x"));
        assert_eq!(snap.identity.account_token, "claude:acct-uuid");
    }

    #[test]
    fn account_endpoint_failure_does_not_block_snapshot() {
        let client = Arc::new(StubClient::new());
        client.put(
            USAGE_ENDPOINT,
            200,
            br#"{"five_hour":{"utilization":5.0,"resets_at":"2026-05-12T23:20:00+00:00"}}"#,
        );
        // /account intentionally 404 → returns empty AccountSummary.
        let resolver = Arc::new(StubResolver {
            creds: good_credentials(),
        });
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert!(snap.account_email.is_none());
        assert_eq!(snap.identity.account_token, "claude:unknown");
    }

    #[test]
    fn http_401_maps_to_unauthorized() {
        let client = Arc::new(StubClient::new());
        client.put(USAGE_ENDPOINT, 401, b"{}");
        let resolver = Arc::new(StubResolver {
            creds: good_credentials(),
        });
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn http_403_with_scope_in_body_maps_to_user_config_invalid() {
        let client = Arc::new(StubClient::new());
        client.put(
            USAGE_ENDPOINT,
            403,
            br#"{"error":"oauth token missing user:profile scope"}"#,
        );
        let resolver = Arc::new(StubResolver {
            creds: good_credentials(),
        });
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::UserConfigInvalid(_)));
    }

    #[test]
    fn http_403_without_scope_message_maps_to_permission_denied() {
        let client = Arc::new(StubClient::new());
        client.put(USAGE_ENDPOINT, 403, b"forbidden");
        let resolver = Arc::new(StubResolver {
            creds: good_credentials(),
        });
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::PermissionDenied(_)));
    }

    #[test]
    fn http_429_hints_at_user_agent_requirement() {
        let client = Arc::new(StubClient::new());
        client.put(USAGE_ENDPOINT, 429, b"");
        let resolver = Arc::new(StubResolver {
            creds: good_credentials(),
        });
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        match err {
            ProviderFetchError::Network(msg) => assert!(msg.contains("claude-code")),
            other => panic!("expected Network with claude-code hint, got {other:?}"),
        }
    }

    #[test]
    fn missing_credentials_maps_to_no_token() {
        let resolver = Arc::new(FailingResolver(CredentialError::Missing));
        let client = Arc::new(StubClient::new());
        client.put(USAGE_ENDPOINT, 200, b"{}");
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoToken("claude")));
    }

    #[test]
    fn malformed_response_maps_to_parse_error() {
        let client = Arc::new(StubClient::new());
        client.put(USAGE_ENDPOINT, 200, b"not json");
        let resolver = Arc::new(StubResolver {
            creds: good_credentials(),
        });
        let strategy = ClaudeOAuthStrategy::new(client, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::ParseError(_)));
    }
}
