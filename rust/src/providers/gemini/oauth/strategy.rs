//! Gemini OAuth strategy. Ported from `GeminiStatusProbe.swift`.
//!
//! Token freshness:
//! - Token still valid → call `loadCodeAssist`, optionally discover
//!   project, then POST `retrieveUserQuota`.
//! - Token expired + a `RefreshHook` is installed → POST
//!   `oauth2.googleapis.com/token` with the embedded client_id /
//!   secret from @google/gemini-cli, persist the new token, retry.
//! - Token expired + no refresh hook → `Unauthorized` (the user must
//!   re-run `gemini` to mint a fresh token).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;

use super::code_assist::{
    parse_status, plan_label, CodeAssistStatus, LOAD_CODE_ASSIST_BODY, LOAD_CODE_ASSIST_URL,
};
use super::client_locator::OAuthClientCredentials;
use super::credentials::{GeminiAuthType, GeminiOAuthCredentials};
use super::jwt_claims::extract_claims;
use super::response::{classify_models, fold_buckets, QuotaResponse};
use super::token_refresh::{apply_in_memory, persist_to_disk, refresh, RefreshHttp};
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::gemini::descriptor::GEMINI_ID;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;

pub const QUOTA_URL: &str = "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";
pub const PROJECTS_URL: &str = "https://cloudresourcemanager.googleapis.com/v1/projects";
pub const PER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[async_trait]
pub trait GoogleHttp: Send + Sync {
    async fn request(
        &self,
        method: HttpMethod,
        url: &str,
        bearer: &str,
        body: Option<&[u8]>,
    ) -> Result<GoogleResponse, ProviderFetchError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

pub struct GoogleResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Resolves credentials and signals the auth type. Production reads
/// `~/.gemini/{oauth_creds.json,settings.json}`; tests inject canned
/// values.
#[async_trait]
pub trait GeminiCredentialsResolver: Send + Sync {
    async fn resolve(&self) -> Result<GeminiCredentialsState, ProviderFetchError>;
}

#[derive(Clone, Debug)]
pub struct GeminiCredentialsState {
    pub auth_type: GeminiAuthType,
    pub credentials: Option<GeminiOAuthCredentials>,
}

/// Optional refresh hook. When present, the strategy will try to
/// refresh an expired access_token before giving up.
pub struct RefreshHook {
    /// HTTP transport for the POST to oauth2.googleapis.com/token.
    pub http: Arc<dyn RefreshHttp>,
    /// OAuth client credentials extracted from @google/gemini-cli on
    /// disk. Resolved lazily so the strategy can construct itself even
    /// when the package is not yet installed.
    pub client: Arc<dyn ClientCredentialsProvider>,
    /// Home directory for writing the refreshed token back to
    /// `~/.gemini/oauth_creds.json`.
    pub home_dir: PathBuf,
}

#[async_trait]
pub trait ClientCredentialsProvider: Send + Sync {
    /// Returns `Ok(None)` when @google/gemini-cli is not installed; the
    /// strategy treats that the same as "no refresh hook" and surfaces
    /// `Unauthorized` so the user is prompted to install / re-auth.
    async fn resolve(&self) -> Result<Option<OAuthClientCredentials>, ProviderFetchError>;
}

pub struct GeminiOAuthStrategy {
    http: Arc<dyn GoogleHttp>,
    resolver: Arc<dyn GeminiCredentialsResolver>,
    refresh: Option<RefreshHook>,
}

impl GeminiOAuthStrategy {
    pub fn new(
        http: Arc<dyn GoogleHttp>,
        resolver: Arc<dyn GeminiCredentialsResolver>,
    ) -> Self {
        Self {
            http,
            resolver,
            refresh: None,
        }
    }

    pub fn with_refresh(mut self, refresh: RefreshHook) -> Self {
        self.refresh = Some(refresh);
        self
    }
}

#[async_trait]
impl Strategy for GeminiOAuthStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::OAuth
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let state = self.resolver.resolve().await?;
        match state.auth_type {
            GeminiAuthType::ApiKey => {
                return Err(ProviderFetchError::UserConfigInvalid(
                    "Gemini API-key auth is not supported; switch to a Google account".into(),
                ));
            }
            GeminiAuthType::VertexAI => {
                return Err(ProviderFetchError::UserConfigInvalid(
                    "Gemini Vertex AI auth is not supported; switch to a Google account".into(),
                ));
            }
            GeminiAuthType::OauthPersonal | GeminiAuthType::Unknown => {}
        }

        let mut creds = state
            .credentials
            .ok_or(ProviderFetchError::NoToken("gemini"))?;
        if creds
            .access_token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .is_none()
        {
            return Err(ProviderFetchError::NoToken("gemini"));
        }

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        if creds.is_expired(now_secs) {
            self.refresh_access_token(&mut creds, now_secs).await?;
        }

        let access_token = creds
            .access_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or(ProviderFetchError::NoToken("gemini"))?;
        let bearer = format!("Bearer {access_token}");
        let claims = extract_claims(creds.id_token.as_deref());

        let ca_status = self.load_code_assist(&bearer).await.unwrap_or_default();
        let project_id = match ca_status.project_id.clone() {
            Some(id) => Some(id),
            None => self.discover_project_id(&bearer).await.ok().flatten(),
        };

        let response = self.retrieve_quota(&bearer, project_id.as_deref()).await?;
        let quotas = fold_buckets(&response);
        if quotas.is_empty() {
            return Err(ProviderFetchError::ParseError(
                "gemini retrieveUserQuota returned no buckets".into(),
            ));
        }

        let tiers = classify_models(&quotas);

        let mut windows = Vec::new();
        if let Some(pro) = tiers.pro.as_ref() {
            windows.push(tier_to_window("pro", "Pro", pro.percent_left, pro.reset_at_unix_secs));
        }
        if let Some(flash) = tiers.flash.as_ref() {
            windows.push(tier_to_window(
                "flash",
                "Flash",
                flash.percent_left,
                flash.reset_at_unix_secs,
            ));
        }
        if let Some(lite) = tiers.flash_lite.as_ref() {
            windows.push(tier_to_window(
                "flash_lite",
                "Flash Lite",
                lite.percent_left,
                lite.reset_at_unix_secs,
            ));
        }
        if windows.is_empty() {
            // Fallback: at least surface the lowest model under "pro"
            // slot so the popup is not empty.
            if let Some(first) = quotas.first() {
                windows.push(tier_to_window(
                    "pro",
                    "Pro",
                    first.percent_left,
                    first.reset_at_unix_secs,
                ));
            }
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();

        let account_token = claims
            .email
            .clone()
            .map(|e| format!("gemini:{}", e.to_ascii_lowercase()))
            .or_else(|| project_id.clone().map(|p| format!("gemini:project:{p}")))
            .unwrap_or_else(|| "gemini:oauth".into());

        Ok(UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(GEMINI_ID, account_token),
            windows,
            credits: None,
            cost: None,
            account_display_name: None,
            account_email: claims.email,
            plan_name: plan_label(ca_status.tier.as_ref(), claims.hosted_domain.as_deref()),
            captured_at_unix_secs: now,
        })
    }
}

impl GeminiOAuthStrategy {
    async fn refresh_access_token(
        &self,
        creds: &mut GeminiOAuthCredentials,
        now_secs: i64,
    ) -> Result<(), ProviderFetchError> {
        let Some(hook) = self.refresh.as_ref() else {
            return Err(ProviderFetchError::Unauthorized);
        };
        let refresh_token = creds
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or(ProviderFetchError::Unauthorized)?
            .to_string();
        // If the @google/gemini-cli package is not installed we cannot
        // refresh — surface Unauthorized so the popup tells the user
        // to install / re-auth.
        let client = match hook.client.resolve().await? {
            Some(c) => c,
            None => return Err(ProviderFetchError::Unauthorized),
        };
        let refreshed = refresh(hook.http.as_ref(), &client, &refresh_token).await?;
        apply_in_memory(creds, &refreshed, now_secs);
        // Disk write is best-effort: if it fails we still keep the
        // in-memory refresh so the current tick succeeds. The next
        // tick will simply re-refresh (with the old refresh_token,
        // which Google leaves valid).
        let _ = persist_to_disk(&hook.home_dir, &refreshed, now_secs);
        Ok(())
    }

    async fn load_code_assist(&self, bearer: &str) -> Option<CodeAssistStatus> {
        let response = self
            .http
            .request(
                HttpMethod::Post,
                LOAD_CODE_ASSIST_URL,
                bearer,
                Some(LOAD_CODE_ASSIST_BODY),
            )
            .await
            .ok()?;
        if !(200..=299).contains(&response.status) {
            return None;
        }
        Some(parse_status(&response.body))
    }

    async fn discover_project_id(&self, bearer: &str) -> Result<Option<String>, ProviderFetchError> {
        let response = self
            .http
            .request(HttpMethod::Get, PROJECTS_URL, bearer, None)
            .await?;
        if !(200..=299).contains(&response.status) {
            return Ok(None);
        }
        let projects: ProjectListWire = match serde_json::from_slice(&response.body) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        Ok(projects
            .projects
            .into_iter()
            .find_map(|p| {
                let id = p.project_id?;
                if id.starts_with("gen-lang-client") {
                    return Some(id);
                }
                if p.labels
                    .as_ref()
                    .is_some_and(|labels| labels.contains_key("generative-language"))
                {
                    return Some(id);
                }
                None
            }))
    }

    async fn retrieve_quota(
        &self,
        bearer: &str,
        project_id: Option<&str>,
    ) -> Result<QuotaResponse, ProviderFetchError> {
        let body = match project_id {
            Some(id) => format!("{{\"project\":{:?}}}", id),
            None => "{}".into(),
        };
        let response = self
            .http
            .request(HttpMethod::Post, QUOTA_URL, bearer, Some(body.as_bytes()))
            .await?;
        match response.status {
            200..=299 => serde_json::from_slice::<QuotaResponse>(&response.body)
                .map_err(|e| ProviderFetchError::ParseError(format!("retrieveUserQuota: {e}"))),
            401 | 403 => Err(ProviderFetchError::Unauthorized),
            other => Err(ProviderFetchError::Network(format!(
                "gemini retrieveUserQuota returned {other}"
            ))),
        }
    }
}

fn tier_to_window(
    key: &str,
    label: &str,
    percent_left: f64,
    reset_at_unix_secs: Option<i64>,
) -> NamedRateWindow {
    let used = (100.0 - percent_left).clamp(0.0, 100.0);
    NamedRateWindow {
        key: key.into(),
        window: RateWindow {
            label: label.into(),
            used,
            allotted: Some(100.0),
            reset_at_unix_secs,
            pace_delta_percent: None,
        },
    }
}

#[derive(Deserialize)]
struct ProjectListWire {
    #[serde(default)]
    projects: Vec<ProjectWire>,
}

#[derive(Deserialize)]
struct ProjectWire {
    #[serde(default, rename = "projectId")]
    project_id: Option<String>,
    #[serde(default)]
    labels: Option<std::collections::HashMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    type CapturedCall = (HttpMethod, String, Option<Vec<u8>>);

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
    impl GoogleHttp for ScriptedHttp {
        async fn request(
            &self,
            method: HttpMethod,
            url: &str,
            _bearer: &str,
            body: Option<&[u8]>,
        ) -> Result<GoogleResponse, ProviderFetchError> {
            self.captured.lock().unwrap().push((
                method,
                url.to_string(),
                body.map(|b| b.to_vec()),
            ));
            let (status, body) = self
                .replies
                .lock()
                .unwrap()
                .get(url)
                .cloned()
                .unwrap_or((404, b"{}".to_vec()));
            Ok(GoogleResponse { status, body })
        }
    }

    struct StubResolver(GeminiCredentialsState);
    #[async_trait]
    impl GeminiCredentialsResolver for StubResolver {
        async fn resolve(&self) -> Result<GeminiCredentialsState, ProviderFetchError> {
            Ok(self.0.clone())
        }
    }

    fn jwt(payload: &str) -> String {
        use base64::Engine;
        let encoded =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.as_bytes());
        format!("header.{encoded}.sig")
    }

    fn fresh_creds(email_claim: &str) -> GeminiOAuthCredentials {
        GeminiOAuthCredentials {
            access_token: Some("tok".into()),
            id_token: Some(jwt(email_claim)),
            refresh_token: Some("rt".into()),
            expiry_unix_secs: Some(i64::MAX),
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
            provider_id: GEMINI_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    #[test]
    fn happy_path_returns_pro_flash_flash_lite_with_paid_plan_label() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(
            LOAD_CODE_ASSIST_URL,
            200,
            br#"{"cloudaicompanionProject": "proj-1", "currentTier": {"id": "standard-tier"}}"#,
        );
        http.put(
            QUOTA_URL,
            200,
            br#"{"buckets": [
                {"modelId": "gemini-2.5-pro", "remainingFraction": 0.2, "tokenType": "input"},
                {"modelId": "gemini-2.5-flash", "remainingFraction": 0.5},
                {"modelId": "gemini-2.5-flash-lite", "remainingFraction": 0.9}
            ]}"#,
        );
        let resolver = Arc::new(StubResolver(GeminiCredentialsState {
            auth_type: GeminiAuthType::OauthPersonal,
            credentials: Some(fresh_creds(r#"{"email": "u@example.com"}"#)),
        }));
        let strategy = GeminiOAuthStrategy::new(http.clone(), resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 3);
        assert_eq!(snap.windows[0].window.label, "Pro");
        assert_eq!(snap.windows[0].window.used, 80.0);
        assert_eq!(snap.windows[1].window.label, "Flash");
        assert_eq!(snap.windows[1].window.used, 50.0);
        assert_eq!(snap.windows[2].window.label, "Flash Lite");
        assert_eq!(snap.windows[2].window.used, 10.0);
        assert_eq!(snap.plan_name.as_deref(), Some("Paid"));
        assert_eq!(snap.account_email.as_deref(), Some("u@example.com"));
        assert_eq!(snap.identity.account_token, "gemini:u@example.com");

        // Verify the retrieveUserQuota body included the project id.
        let captured = http.captured.lock().unwrap();
        let quota_call = captured.iter().find(|(_, u, _)| u == QUOTA_URL).unwrap();
        let body = String::from_utf8(quota_call.2.clone().unwrap_or_default()).unwrap();
        assert!(body.contains("proj-1"), "body was {body}");
    }

    #[test]
    fn free_tier_with_hosted_domain_maps_to_workspace_plan() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(
            LOAD_CODE_ASSIST_URL,
            200,
            br#"{"currentTier": {"id": "free-tier"}}"#,
        );
        http.put(PROJECTS_URL, 200, br#"{"projects": []}"#);
        http.put(
            QUOTA_URL,
            200,
            br#"{"buckets": [{"modelId": "gemini-2.5-flash", "remainingFraction": 0.75}]}"#,
        );
        let resolver = Arc::new(StubResolver(GeminiCredentialsState {
            auth_type: GeminiAuthType::OauthPersonal,
            credentials: Some(fresh_creds(
                r#"{"email": "u@corp.example.com", "hd": "corp.example.com"}"#,
            )),
        }));
        let strategy = GeminiOAuthStrategy::new(http, resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.plan_name.as_deref(), Some("Workspace"));
    }

    #[test]
    fn api_key_auth_is_rejected_with_user_config_invalid() {
        let http = Arc::new(ScriptedHttp::default());
        let resolver = Arc::new(StubResolver(GeminiCredentialsState {
            auth_type: GeminiAuthType::ApiKey,
            credentials: None,
        }));
        let strategy = GeminiOAuthStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::UserConfigInvalid(_)));
    }

    #[test]
    fn vertex_auth_is_rejected_with_user_config_invalid() {
        let http = Arc::new(ScriptedHttp::default());
        let resolver = Arc::new(StubResolver(GeminiCredentialsState {
            auth_type: GeminiAuthType::VertexAI,
            credentials: None,
        }));
        let strategy = GeminiOAuthStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::UserConfigInvalid(_)));
    }

    #[test]
    fn expired_token_maps_to_unauthorized() {
        let http = Arc::new(ScriptedHttp::default());
        let mut creds = fresh_creds(r#"{"email":"u@x.com"}"#);
        creds.expiry_unix_secs = Some(1); // past
        let resolver = Arc::new(StubResolver(GeminiCredentialsState {
            auth_type: GeminiAuthType::OauthPersonal,
            credentials: Some(creds),
        }));
        let strategy = GeminiOAuthStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn missing_credentials_map_to_no_token() {
        let http = Arc::new(ScriptedHttp::default());
        let resolver = Arc::new(StubResolver(GeminiCredentialsState {
            auth_type: GeminiAuthType::OauthPersonal,
            credentials: None,
        }));
        let strategy = GeminiOAuthStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoToken("gemini")));
    }

    #[test]
    fn project_discovery_picks_gen_lang_prefix() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(LOAD_CODE_ASSIST_URL, 200, br#"{"currentTier": {"id": "free-tier"}}"#);
        http.put(
            PROJECTS_URL,
            200,
            br#"{"projects": [
                {"projectId": "personal-project"},
                {"projectId": "gen-lang-client-abc123"}
            ]}"#,
        );
        http.put(QUOTA_URL, 200, br#"{"buckets": [{"modelId": "gemini-2.5-pro", "remainingFraction": 0.5}]}"#);
        let resolver = Arc::new(StubResolver(GeminiCredentialsState {
            auth_type: GeminiAuthType::OauthPersonal,
            credentials: Some(fresh_creds(r#"{"email":"u@x.com"}"#)),
        }));
        let strategy = GeminiOAuthStrategy::new(http.clone(), resolver);
        let _ = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        let captured = http.captured.lock().unwrap();
        let quota_body = captured
            .iter()
            .find(|(_, u, _)| u == QUOTA_URL)
            .and_then(|c| c.2.as_ref())
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default();
        assert!(
            quota_body.contains("gen-lang-client-abc123"),
            "expected project id in quota body, got {quota_body}"
        );
    }

    #[test]
    fn quota_401_maps_to_unauthorized() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(LOAD_CODE_ASSIST_URL, 200, br#"{"currentTier": {"id": "free-tier"}}"#);
        http.put(QUOTA_URL, 401, b"{}");
        let resolver = Arc::new(StubResolver(GeminiCredentialsState {
            auth_type: GeminiAuthType::OauthPersonal,
            credentials: Some(fresh_creds(r#"{"email":"u@x.com"}"#)),
        }));
        let strategy = GeminiOAuthStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    // ─── Refresh-hook tests ─────────────────────────────────────────

    use super::super::token_refresh::{RefreshHttp, RefreshResponse};
    use std::path::PathBuf;

    struct StubRefreshHttp {
        replies: Mutex<Vec<RefreshResponse>>,
        captured: Mutex<Vec<(String, String)>>,
    }
    impl StubRefreshHttp {
        fn new() -> Self {
            Self {
                replies: Mutex::new(Vec::new()),
                captured: Mutex::new(Vec::new()),
            }
        }
        fn enqueue(&self, status: u16, body: &[u8]) {
            self.replies.lock().unwrap().push(RefreshResponse {
                status,
                body: body.to_vec(),
            });
        }
    }
    #[async_trait]
    impl RefreshHttp for StubRefreshHttp {
        async fn post_form(
            &self,
            url: &str,
            body: &str,
        ) -> Result<RefreshResponse, ProviderFetchError> {
            self.captured.lock().unwrap().push((url.into(), body.into()));
            let mut replies = self.replies.lock().unwrap();
            if replies.is_empty() {
                return Err(ProviderFetchError::Network("stub exhausted".into()));
            }
            Ok(replies.remove(0))
        }
    }

    struct StubClient(Option<OAuthClientCredentials>);
    #[async_trait]
    impl ClientCredentialsProvider for StubClient {
        async fn resolve(
            &self,
        ) -> Result<Option<OAuthClientCredentials>, ProviderFetchError> {
            Ok(self.0.clone())
        }
    }

    fn expired_creds(email_claim: &str) -> GeminiOAuthCredentials {
        GeminiOAuthCredentials {
            access_token: Some("stale-token".into()),
            id_token: Some(jwt(email_claim)),
            refresh_token: Some("rt-1".into()),
            expiry_unix_secs: Some(1), // long in the past
        }
    }

    fn write_creds_file(dir: &std::path::Path) -> PathBuf {
        let gemini = dir.join(".gemini");
        std::fs::create_dir_all(&gemini).unwrap();
        let path = gemini.join("oauth_creds.json");
        std::fs::write(
            &path,
            r#"{"access_token":"stale-token","id_token":"old-id","refresh_token":"rt-1","expiry_date":1000}"#,
        )
        .unwrap();
        path
    }

    #[test]
    fn expired_token_with_refresh_hook_refreshes_and_proceeds() {
        let dir = tempfile::tempdir().unwrap();
        let creds_path = write_creds_file(dir.path());

        let http = Arc::new(ScriptedHttp::default());
        http.put(LOAD_CODE_ASSIST_URL, 200, br#"{"currentTier": {"id": "standard-tier"}}"#);
        http.put(
            QUOTA_URL,
            200,
            br#"{"buckets": [
                {"modelId": "gemini-2.5-pro", "remainingFraction": 0.5}
            ]}"#,
        );

        let refresh_http = Arc::new(StubRefreshHttp::new());
        refresh_http.enqueue(
            200,
            br#"{"access_token":"fresh-token","expires_in":3600,"id_token":"fresh-id"}"#,
        );

        let resolver = Arc::new(StubResolver(GeminiCredentialsState {
            auth_type: GeminiAuthType::OauthPersonal,
            credentials: Some(expired_creds(r#"{"email":"u@x.com"}"#)),
        }));
        let strategy = GeminiOAuthStrategy::new(http, resolver).with_refresh(RefreshHook {
            http: refresh_http.clone(),
            client: Arc::new(StubClient(Some(OAuthClientCredentials {
                client_id: "client.apps.googleusercontent.com".into(),
                client_secret: "GOCSPX-test".into(),
            }))),
            home_dir: dir.path().to_path_buf(),
        });
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 1);
        // Refresh POST was made with the expected form body.
        let captured = refresh_http.captured.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert!(captured[0].1.contains("refresh_token=rt-1"));
        assert!(captured[0].1.contains("grant_type=refresh_token"));
        // Disk was rewritten with the new access_token.
        let on_disk = std::fs::read_to_string(&creds_path).unwrap();
        assert!(on_disk.contains("fresh-token"));
    }

    #[test]
    fn expired_token_with_no_refresh_hook_remains_unauthorized() {
        let http = Arc::new(ScriptedHttp::default());
        let resolver = Arc::new(StubResolver(GeminiCredentialsState {
            auth_type: GeminiAuthType::OauthPersonal,
            credentials: Some(expired_creds(r#"{"email":"u@x.com"}"#)),
        }));
        let strategy = GeminiOAuthStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn expired_token_with_refresh_hook_but_no_client_falls_back_to_unauthorized() {
        let dir = tempfile::tempdir().unwrap();
        write_creds_file(dir.path());
        let http = Arc::new(ScriptedHttp::default());
        let refresh_http = Arc::new(StubRefreshHttp::new());
        let resolver = Arc::new(StubResolver(GeminiCredentialsState {
            auth_type: GeminiAuthType::OauthPersonal,
            credentials: Some(expired_creds(r#"{"email":"u@x.com"}"#)),
        }));
        // ClientCredentialsProvider returns None (gemini CLI not installed).
        let strategy = GeminiOAuthStrategy::new(http, resolver).with_refresh(RefreshHook {
            http: refresh_http,
            client: Arc::new(StubClient(None)),
            home_dir: dir.path().to_path_buf(),
        });
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn expired_token_with_refresh_endpoint_401_propagates_unauthorized() {
        let dir = tempfile::tempdir().unwrap();
        write_creds_file(dir.path());
        let http = Arc::new(ScriptedHttp::default());
        let refresh_http = Arc::new(StubRefreshHttp::new());
        refresh_http.enqueue(401, br#"{"error":"invalid_grant"}"#);
        let resolver = Arc::new(StubResolver(GeminiCredentialsState {
            auth_type: GeminiAuthType::OauthPersonal,
            credentials: Some(expired_creds(r#"{"email":"u@x.com"}"#)),
        }));
        let strategy = GeminiOAuthStrategy::new(http, resolver).with_refresh(RefreshHook {
            http: refresh_http,
            client: Arc::new(StubClient(Some(OAuthClientCredentials {
                client_id: "x".into(),
                client_secret: "y".into(),
            }))),
            home_dir: dir.path().to_path_buf(),
        });
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }
}
