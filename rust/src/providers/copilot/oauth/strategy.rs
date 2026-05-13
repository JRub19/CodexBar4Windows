//! Copilot OAuth strategy. Uses the user's GitHub PAT/OAuth token to
//! call `https://api.github.com/copilot_internal/user` (or the GHE
//! equivalent). Headers are fingerprinted to match the macOS app:
//! `Editor-Version`, `Editor-Plugin-Version`, `User-Agent`, and
//! `X-Github-Api-Version` are all required for the API to honour the
//! request.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use super::endpoints::{usage_url, user_identity_url};
use super::response::{parse, CopilotUsage, QuotaSnapshot};
use crate::providers::copilot::descriptor::COPILOT_ID;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;

pub const PER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const EDITOR_VERSION: &str = "vscode/1.96.2";
pub const EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.26.7";
pub const USER_AGENT: &str = "GitHubCopilotChat/0.26.7";
pub const GITHUB_API_VERSION: &str = "2025-04-01";

#[async_trait]
pub trait GithubHttp: Send + Sync {
    async fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<GithubResponse, ProviderFetchError>;
}

pub struct GithubResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct CopilotCredentials {
    /// GitHub OAuth token (the device-flow access_token or a PAT).
    pub access_token: String,
    /// Enterprise host (`github.example.com`); `None` is github.com.
    pub enterprise_host: Option<String>,
}

#[async_trait]
pub trait CopilotCredentialsResolver: Send + Sync {
    async fn resolve(&self) -> Result<Option<CopilotCredentials>, ProviderFetchError>;
}

pub struct CopilotOAuthStrategy {
    http: Arc<dyn GithubHttp>,
    creds: Arc<dyn CopilotCredentialsResolver>,
}

impl CopilotOAuthStrategy {
    pub fn new(http: Arc<dyn GithubHttp>, creds: Arc<dyn CopilotCredentialsResolver>) -> Self {
        Self { http, creds }
    }
}

#[async_trait]
impl Strategy for CopilotOAuthStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::OAuth
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let creds = self
            .creds
            .resolve()
            .await?
            .ok_or(ProviderFetchError::NoToken("copilot"))?;
        let bearer = format!("token {}", creds.access_token);

        let usage = fetch_copilot_usage(
            self.http.as_ref(),
            &bearer,
            creds.enterprise_host.as_deref(),
        )
        .await?;

        let identity = fetch_identity(self.http.as_ref(), &bearer).await.ok();

        let mut windows = Vec::new();
        if let Some(snap) = usage.premium.as_ref().and_then(snapshot_to_window_premium) {
            windows.push(snap);
        }
        if let Some(snap) = usage.chat.as_ref().and_then(snapshot_to_window_chat) {
            windows.push(snap);
        }
        if windows.is_empty() {
            return Err(ProviderFetchError::ParseError(
                "copilot quota response missing usable percent_remaining".into(),
            ));
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();

        let account_token = identity
            .as_ref()
            .map(|i| format!("copilot:{}", i.login.to_ascii_lowercase()))
            .unwrap_or_else(|| "copilot:token".into());

        let plan_label = if usage.copilot_plan.is_empty() || usage.copilot_plan == "unknown" {
            None
        } else {
            Some(capitalize_first(&usage.copilot_plan))
        };

        Ok(UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(COPILOT_ID, account_token),
            windows,
            credits: None,
            cost: None,
            account_display_name: identity.as_ref().map(|i| i.login.clone()),
            account_email: None,
            plan_name: plan_label,
            captured_at_unix_secs: now,
        })
    }
}

fn snapshot_to_window_premium(snapshot: &QuotaSnapshot) -> Option<NamedRateWindow> {
    let used = snapshot.used_percent()?;
    Some(NamedRateWindow {
        key: "premium".into(),
        window: RateWindow {
            label: "Premium".into(),
            used,
            allotted: Some(100.0),
            reset_at_unix_secs: None,
            pace_delta_percent: None,
        },
    })
}

fn snapshot_to_window_chat(snapshot: &QuotaSnapshot) -> Option<NamedRateWindow> {
    let used = snapshot.used_percent()?;
    Some(NamedRateWindow {
        key: "chat".into(),
        window: RateWindow {
            label: "Chat".into(),
            used,
            allotted: Some(100.0),
            reset_at_unix_secs: None,
            pace_delta_percent: None,
        },
    })
}

async fn fetch_copilot_usage(
    http: &dyn GithubHttp,
    bearer: &str,
    enterprise_host: Option<&str>,
) -> Result<CopilotUsage, ProviderFetchError> {
    let url = usage_url(enterprise_host);
    let headers: Vec<(&str, &str)> = vec![
        ("Authorization", bearer),
        ("Accept", "application/json"),
        ("Editor-Version", EDITOR_VERSION),
        ("Editor-Plugin-Version", EDITOR_PLUGIN_VERSION),
        ("User-Agent", USER_AGENT),
        ("X-Github-Api-Version", GITHUB_API_VERSION),
    ];
    let response = http.get(&url, &headers).await?;
    match response.status {
        200..=299 => {
            parse(&response.body).map_err(|e| ProviderFetchError::ParseError(e.to_string()))
        }
        401 | 403 => Err(ProviderFetchError::Unauthorized),
        other => Err(ProviderFetchError::Network(format!(
            "copilot usage returned {other}"
        ))),
    }
}

#[derive(serde::Deserialize)]
struct UserIdentity {
    #[serde(default)]
    login: String,
}

async fn fetch_identity(
    http: &dyn GithubHttp,
    bearer: &str,
) -> Result<UserIdentity, ProviderFetchError> {
    let headers: Vec<(&str, &str)> =
        vec![("Authorization", bearer), ("Accept", "application/json")];
    let response = http.get(user_identity_url(), &headers).await?;
    match response.status {
        200..=299 => serde_json::from_slice::<UserIdentity>(&response.body)
            .map_err(|e| ProviderFetchError::ParseError(e.to_string())),
        401 | 403 => Err(ProviderFetchError::Unauthorized),
        other => Err(ProviderFetchError::Network(format!(
            "github /user returned {other}"
        ))),
    }
}

fn capitalize_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    type CapturedHeaders = Vec<(String, String)>;
    type CapturedCall = (String, CapturedHeaders);

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
                .insert(url.to_string(), (status, body.to_vec()));
        }
    }

    #[async_trait]
    impl GithubHttp for ScriptedHttp {
        async fn get(
            &self,
            url: &str,
            headers: &[(&str, &str)],
        ) -> Result<GithubResponse, ProviderFetchError> {
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
            Ok(GithubResponse { status, body })
        }
    }

    struct StubResolver(Option<CopilotCredentials>);
    #[async_trait]
    impl CopilotCredentialsResolver for StubResolver {
        async fn resolve(&self) -> Result<Option<CopilotCredentials>, ProviderFetchError> {
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
            provider_id: COPILOT_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    #[test]
    fn happy_path_returns_premium_and_chat_windows_with_required_headers() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(
            "https://api.github.com/copilot_internal/user",
            200,
            br#"{
                "copilot_plan": "business",
                "quota_snapshots": {
                    "premium_interactions": {"entitlement": 300, "remaining": 240, "percent_remaining": 80},
                    "chat": {"entitlement": 1000, "remaining": 250, "percent_remaining": 25}
                }
            }"#,
        );
        http.put(
            "https://api.github.com/user",
            200,
            br#"{"id": 1, "login": "octocat"}"#,
        );
        let resolver = Arc::new(StubResolver(Some(CopilotCredentials {
            access_token: "tok".into(),
            enterprise_host: None,
        })));
        let strategy = CopilotOAuthStrategy::new(http.clone(), resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 2);
        assert_eq!(snap.windows[0].key, "premium");
        assert_eq!(snap.windows[0].window.used, 20.0);
        assert_eq!(snap.windows[1].key, "chat");
        assert_eq!(snap.windows[1].window.used, 75.0);
        assert_eq!(snap.identity.account_token, "copilot:octocat");
        assert_eq!(snap.plan_name.as_deref(), Some("Business"));

        // Verify the editor fingerprint headers were sent.
        let captured = http.captured.lock().unwrap();
        let usage_call = captured
            .iter()
            .find(|(u, _)| u == "https://api.github.com/copilot_internal/user")
            .unwrap();
        assert!(usage_call
            .1
            .iter()
            .any(|(k, v)| k == "Editor-Version" && v == EDITOR_VERSION));
        assert!(usage_call
            .1
            .iter()
            .any(|(k, v)| k == "User-Agent" && v == USER_AGENT));
        assert!(usage_call
            .1
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "token tok"));
    }

    #[test]
    fn enterprise_host_rewrites_api_url() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(
            "https://api.corp.example.com/copilot_internal/user",
            200,
            br#"{
                "copilot_plan": "enterprise",
                "quota_snapshots": {
                    "premium_interactions": {"entitlement": 300, "remaining": 300, "percent_remaining": 100}
                }
            }"#,
        );
        http.put(
            "https://api.github.com/user",
            200,
            br#"{"login": "ent-user"}"#,
        );
        let resolver = Arc::new(StubResolver(Some(CopilotCredentials {
            access_token: "tok".into(),
            enterprise_host: Some("corp.example.com".into()),
        })));
        let strategy = CopilotOAuthStrategy::new(http, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.plan_name.as_deref(), Some("Enterprise"));
        assert_eq!(snap.windows[0].window.used, 0.0);
    }

    #[test]
    fn http_401_maps_to_unauthorized() {
        let http = Arc::new(ScriptedHttp::default());
        http.put("https://api.github.com/copilot_internal/user", 401, b"{}");
        let resolver = Arc::new(StubResolver(Some(CopilotCredentials {
            access_token: "tok".into(),
            enterprise_host: None,
        })));
        let strategy = CopilotOAuthStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn missing_credentials_map_to_no_token() {
        let http = Arc::new(ScriptedHttp::default());
        let resolver = Arc::new(StubResolver(None));
        let strategy = CopilotOAuthStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoToken("copilot")));
    }

    #[test]
    fn identity_failure_does_not_block_snapshot() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(
            "https://api.github.com/copilot_internal/user",
            200,
            br#"{
                "copilot_plan": "individual",
                "quota_snapshots": {
                    "premium_interactions": {"entitlement": 300, "remaining": 240, "percent_remaining": 80}
                }
            }"#,
        );
        // /user returns 500 → identity is dropped, snapshot still ok.
        http.put("https://api.github.com/user", 500, b"err");
        let resolver = Arc::new(StubResolver(Some(CopilotCredentials {
            access_token: "tok".into(),
            enterprise_host: None,
        })));
        let strategy = CopilotOAuthStrategy::new(http, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.identity.account_token, "copilot:token");
        assert!(snap.account_display_name.is_none());
        assert_eq!(snap.plan_name.as_deref(), Some("Individual"));
    }
}
