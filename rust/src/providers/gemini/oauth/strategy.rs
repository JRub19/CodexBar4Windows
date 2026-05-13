//! Gemini OAuth strategy. Ported from `GeminiStatusProbe.swift`. The
//! refresh path (which requires extracting the embedded
//! `OAUTH_CLIENT_ID/SECRET` from the installed @google/gemini-cli
//! package) is intentionally out of scope for the initial port:
//! - Token already valid → call `loadCodeAssist`, optionally discover
//!   project, then POST `retrieveUserQuota`.
//! - Token expired but resolver returns refreshed credentials → same
//!   path.
//! - Token expired with no refresh available → `Unauthorized`.
//!
//! The refresh-via-CLI plumbing will live in a separate file with a
//! Windows-specific npm/fnm/scoop locator. Until then the user runs
//! `gemini` once to get a fresh access_token.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;

use super::code_assist::{
    parse_status, plan_label, CodeAssistStatus, LOAD_CODE_ASSIST_BODY, LOAD_CODE_ASSIST_URL,
};
use super::credentials::{GeminiAuthType, GeminiOAuthCredentials};
use super::jwt_claims::extract_claims;
use super::response::{classify_models, fold_buckets, QuotaResponse};
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

pub struct GeminiOAuthStrategy {
    http: Arc<dyn GoogleHttp>,
    resolver: Arc<dyn GeminiCredentialsResolver>,
}

impl GeminiOAuthStrategy {
    pub fn new(
        http: Arc<dyn GoogleHttp>,
        resolver: Arc<dyn GeminiCredentialsResolver>,
    ) -> Self {
        Self { http, resolver }
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

        let creds = state.credentials.ok_or(ProviderFetchError::NoToken("gemini"))?;
        let access_token = creds
            .access_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or(ProviderFetchError::NoToken("gemini"))?;

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        if creds.is_expired(now_secs) {
            // No refresh wiring yet. Surface Unauthorized so the
            // runtime stops walking the plan and the popup tells the
            // user to re-auth via the gemini CLI.
            return Err(ProviderFetchError::Unauthorized);
        }

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
}
