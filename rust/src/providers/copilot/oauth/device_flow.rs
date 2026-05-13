//! GitHub OAuth device-code flow for Copilot. Ported from
//! `Sources/CodexBarCore/Providers/Copilot/CopilotDeviceFlow.swift`.
//!
//! The flow has three phases:
//! 1. POST `/login/device/code` → returns a `user_code` the user types
//!    into the verification URL plus a `device_code` we poll with.
//! 2. POST `/login/oauth/access_token` at the suggested interval until
//!    GitHub either issues the access token or returns a terminal
//!    error (`expired_token`, `access_denied`, …).
//! 3. Caller stores the access token in the secrets store and the
//!    Copilot usage strategy starts reading it on the next refresh.
//!
//! The transport is a pluggable trait so tests drive the whole state
//! machine without hitting github.com.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::providers::copilot::oauth::endpoints::normalize_host;
use crate::providers::errors::ProviderFetchError;

/// VS Code's published client ID. GitHub special-cases it so the
/// resulting access token has Copilot's editor scope. Stable for years
/// and shared by the official Copilot extensions.
pub const VS_CODE_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
pub const SCOPE: &str = "read:user";
pub const SLOW_DOWN_ADDITIONAL_SECS: u64 = 5;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DeviceCodeResponse {
    #[serde(rename = "device_code")]
    pub device_code: String,
    #[serde(rename = "user_code")]
    pub user_code: String,
    #[serde(rename = "verification_uri")]
    pub verification_uri: String,
    #[serde(default, rename = "verification_uri_complete")]
    pub verification_uri_complete: Option<String>,
    #[serde(rename = "expires_in")]
    pub expires_in: i64,
    pub interval: u64,
}

impl DeviceCodeResponse {
    /// URL we point the user at — the `complete` variant when GitHub
    /// emits it (pre-filled with the code), else the bare URI.
    pub fn verification_url_to_open(&self) -> &str {
        self.verification_uri_complete
            .as_deref()
            .unwrap_or(&self.verification_uri)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AccessTokenResponse {
    #[serde(rename = "access_token")]
    pub access_token: String,
    #[serde(default, rename = "token_type")]
    pub token_type: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

/// Terminal poll outcomes per GitHub's docs. `Pending` and `SlowDown`
/// are handled inside `poll_for_token`; everything else surfaces to the
/// caller as a `DeviceFlowError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceFlowError {
    /// `expired_token` — the user took too long.
    Expired,
    /// `access_denied` — the user clicked Cancel on github.com.
    AccessDenied,
    /// `incorrect_device_code` — usually a programming bug here.
    IncorrectDeviceCode,
    /// Any other GitHub-reported error code, with the raw value.
    GithubError(String),
    /// Network/HTTP-level failure.
    Transport(String),
    /// Response could not be parsed.
    Decode(String),
}

impl std::fmt::Display for DeviceFlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceFlowError::Expired => write!(f, "device code expired before the user finished"),
            DeviceFlowError::AccessDenied => write!(f, "user denied the Copilot login request"),
            DeviceFlowError::IncorrectDeviceCode => {
                write!(f, "GitHub rejected the device code")
            }
            DeviceFlowError::GithubError(code) => write!(f, "GitHub returned error `{code}`"),
            DeviceFlowError::Transport(msg) => write!(f, "transport error: {msg}"),
            DeviceFlowError::Decode(msg) => write!(f, "decode error: {msg}"),
        }
    }
}

impl std::error::Error for DeviceFlowError {}

impl From<DeviceFlowError> for ProviderFetchError {
    fn from(err: DeviceFlowError) -> Self {
        match err {
            DeviceFlowError::Transport(msg) => ProviderFetchError::Network(msg),
            DeviceFlowError::Decode(msg) => ProviderFetchError::ParseError(msg),
            DeviceFlowError::Expired
            | DeviceFlowError::AccessDenied
            | DeviceFlowError::IncorrectDeviceCode => ProviderFetchError::Unauthorized,
            DeviceFlowError::GithubError(_) => ProviderFetchError::Unauthorized,
        }
    }
}

pub struct DeviceFlowResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// HTTP transport used by the device flow. Production uses reqwest; the
/// test suite injects a state-machine stub.
#[async_trait]
pub trait DeviceFlowHttp: Send + Sync {
    async fn post_form(
        &self,
        url: &str,
        body: &str,
        headers: &[(&str, &str)],
    ) -> Result<DeviceFlowResponse, DeviceFlowError>;
}

/// Async sleep, injected so tests can run the polling loop without
/// real wall-clock waits.
#[async_trait]
pub trait Sleeper: Send + Sync {
    async fn sleep(&self, duration: Duration);
}

/// Production sleeper using tokio.
pub struct TokioSleeper;
#[async_trait]
impl Sleeper for TokioSleeper {
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

#[derive(Clone, Debug)]
pub struct DeviceFlowConfig {
    /// Optional GHE host. `None` resolves to `github.com`.
    pub enterprise_host: Option<String>,
    /// Override the client ID — primarily for tests, since the live
    /// flow only works with VS Code's published ID.
    pub client_id: String,
    pub scope: String,
}

impl Default for DeviceFlowConfig {
    fn default() -> Self {
        Self {
            enterprise_host: None,
            client_id: VS_CODE_CLIENT_ID.into(),
            scope: SCOPE.into(),
        }
    }
}

impl DeviceFlowConfig {
    pub fn host(&self) -> String {
        normalize_host(self.enterprise_host.as_deref())
    }

    pub fn device_code_url(&self) -> String {
        format!("https://{}/login/device/code", self.host())
    }

    pub fn access_token_url(&self) -> String {
        format!("https://{}/login/oauth/access_token", self.host())
    }
}

/// Step 1 of the device flow. Returns the user-facing code + URL and a
/// `device_code` we poll with.
pub async fn request_device_code(
    http: &dyn DeviceFlowHttp,
    config: &DeviceFlowConfig,
) -> Result<DeviceCodeResponse, DeviceFlowError> {
    let body = form_encode(&[("client_id", &config.client_id), ("scope", &config.scope)]);
    let response = http
        .post_form(
            &config.device_code_url(),
            &body,
            &[
                ("Accept", "application/json"),
                ("Content-Type", "application/x-www-form-urlencoded"),
            ],
        )
        .await?;
    if !(200..=299).contains(&response.status) {
        return Err(DeviceFlowError::Transport(format!(
            "device-code returned HTTP {}",
            response.status
        )));
    }
    serde_json::from_slice::<DeviceCodeResponse>(&response.body)
        .map_err(|e| DeviceFlowError::Decode(e.to_string()))
}

/// Step 2 of the device flow. Polls until GitHub issues the token or
/// returns a terminal error. The caller passes the `interval` from
/// `request_device_code` (seconds); we adjust dynamically when the
/// server sends `slow_down`.
pub async fn poll_for_token(
    http: &dyn DeviceFlowHttp,
    sleeper: &dyn Sleeper,
    config: &DeviceFlowConfig,
    device_code: &str,
    initial_interval_secs: u64,
) -> Result<AccessTokenResponse, DeviceFlowError> {
    let body = form_encode(&[
        ("client_id", &config.client_id),
        ("device_code", device_code),
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
    ]);
    let url = config.access_token_url();
    let mut interval = Duration::from_secs(initial_interval_secs.max(1));
    loop {
        sleeper.sleep(interval).await;
        let response = http
            .post_form(
                &url,
                &body,
                &[
                    ("Accept", "application/json"),
                    ("Content-Type", "application/x-www-form-urlencoded"),
                ],
            )
            .await?;
        match classify_poll_response(&response.body) {
            PollOutcome::Token(t) => return Ok(t),
            PollOutcome::Pending => continue,
            PollOutcome::SlowDown => {
                interval += Duration::from_secs(SLOW_DOWN_ADDITIONAL_SECS);
                continue;
            }
            PollOutcome::Terminal(err) => return Err(err),
        }
    }
}

#[derive(Debug, PartialEq)]
enum PollOutcome {
    Token(AccessTokenResponse),
    Pending,
    SlowDown,
    Terminal(DeviceFlowError),
}

fn classify_poll_response(body: &[u8]) -> PollOutcome {
    // GitHub returns either `{ "error": "...", ... }` or
    // `{ "access_token": "...", ... }`. Try the error shape first so a
    // payload carrying both fields (it does not, in practice, but
    // defending against future drift) routes to the right branch.
    if let Ok(envelope) = serde_json::from_slice::<ErrorEnvelope>(body) {
        if let Some(code) = envelope.error {
            return match code.as_str() {
                "authorization_pending" => PollOutcome::Pending,
                "slow_down" => PollOutcome::SlowDown,
                "expired_token" => PollOutcome::Terminal(DeviceFlowError::Expired),
                "access_denied" => PollOutcome::Terminal(DeviceFlowError::AccessDenied),
                "incorrect_device_code" => {
                    PollOutcome::Terminal(DeviceFlowError::IncorrectDeviceCode)
                }
                other => PollOutcome::Terminal(DeviceFlowError::GithubError(other.into())),
            };
        }
    }
    match serde_json::from_slice::<AccessTokenResponse>(body) {
        Ok(token) => PollOutcome::Token(token),
        Err(e) => PollOutcome::Terminal(DeviceFlowError::Decode(e.to_string())),
    }
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    #[serde(default)]
    error: Option<String>,
}

fn form_encode(pairs: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        serializer.append_pair(k, v);
    }
    serializer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    type CapturedCall = (String, String, Vec<(String, String)>);

    #[derive(Default)]
    struct ScriptedHttp {
        responses: Mutex<Vec<DeviceFlowResponse>>,
        captured: Mutex<Vec<CapturedCall>>,
    }

    impl ScriptedHttp {
        fn enqueue(&self, status: u16, body: &[u8]) {
            self.responses.lock().unwrap().push(DeviceFlowResponse {
                status,
                body: body.to_vec(),
            });
        }
    }

    #[async_trait]
    impl DeviceFlowHttp for ScriptedHttp {
        async fn post_form(
            &self,
            url: &str,
            body: &str,
            headers: &[(&str, &str)],
        ) -> Result<DeviceFlowResponse, DeviceFlowError> {
            self.captured.lock().unwrap().push((
                url.to_string(),
                body.to_string(),
                headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ));
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                return Err(DeviceFlowError::Transport("stub exhausted".into()));
            }
            Ok(responses.remove(0))
        }
    }

    struct InstantSleeper {
        recorded: Mutex<Vec<Duration>>,
    }
    impl Default for InstantSleeper {
        fn default() -> Self {
            Self {
                recorded: Mutex::new(Vec::new()),
            }
        }
    }
    #[async_trait]
    impl Sleeper for InstantSleeper {
        async fn sleep(&self, duration: Duration) {
            self.recorded.lock().unwrap().push(duration);
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn request_device_code_posts_form_and_decodes_response() {
        let http = ScriptedHttp::default();
        http.enqueue(
            200,
            br#"{
                "device_code": "dc-1",
                "user_code": "ABCD-1234",
                "verification_uri": "https://github.com/login/device",
                "verification_uri_complete": "https://github.com/login/device?user_code=ABCD-1234",
                "expires_in": 900,
                "interval": 5
            }"#,
        );
        let config = DeviceFlowConfig::default();
        let response = rt()
            .block_on(async { request_device_code(&http, &config).await })
            .unwrap();
        assert_eq!(response.user_code, "ABCD-1234");
        assert_eq!(response.interval, 5);
        assert_eq!(
            response.verification_url_to_open(),
            "https://github.com/login/device?user_code=ABCD-1234"
        );
        let captured = http.captured.lock().unwrap();
        let (url, body, headers) = &captured[0];
        assert_eq!(url, "https://github.com/login/device/code");
        assert!(body.contains("client_id="));
        assert!(body.contains("scope=read%3Auser"));
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Accept" && v == "application/json"));
    }

    #[test]
    fn poll_for_token_returns_token_on_success_response() {
        let http = ScriptedHttp::default();
        http.enqueue(
            200,
            br#"{"access_token": "ghu_token", "token_type": "bearer", "scope": "read:user"}"#,
        );
        let sleeper = InstantSleeper::default();
        let config = DeviceFlowConfig::default();
        let token = rt()
            .block_on(async { poll_for_token(&http, &sleeper, &config, "dc-1", 1).await })
            .unwrap();
        assert_eq!(token.access_token, "ghu_token");
        assert_eq!(sleeper.recorded.lock().unwrap().len(), 1);
    }

    #[test]
    fn poll_for_token_keeps_waiting_on_authorization_pending() {
        let http = ScriptedHttp::default();
        http.enqueue(200, br#"{"error": "authorization_pending"}"#);
        http.enqueue(200, br#"{"error": "authorization_pending"}"#);
        http.enqueue(200, br#"{"access_token": "ghu_final"}"#);
        let sleeper = InstantSleeper::default();
        let config = DeviceFlowConfig::default();
        let token = rt()
            .block_on(async { poll_for_token(&http, &sleeper, &config, "dc-1", 2).await })
            .unwrap();
        assert_eq!(token.access_token, "ghu_final");
        let intervals = sleeper.recorded.lock().unwrap().clone();
        assert_eq!(intervals.len(), 3);
        assert!(intervals.iter().all(|d| *d == Duration::from_secs(2)));
    }

    #[test]
    fn poll_for_token_increases_interval_on_slow_down() {
        let http = ScriptedHttp::default();
        http.enqueue(200, br#"{"error": "slow_down"}"#);
        http.enqueue(200, br#"{"access_token": "ghu_after_backoff"}"#);
        let sleeper = InstantSleeper::default();
        let config = DeviceFlowConfig::default();
        let token = rt()
            .block_on(async { poll_for_token(&http, &sleeper, &config, "dc-1", 5).await })
            .unwrap();
        assert_eq!(token.access_token, "ghu_after_backoff");
        let intervals = sleeper.recorded.lock().unwrap().clone();
        assert_eq!(intervals.len(), 2);
        // First poll waits the initial 5s; after slow_down we add 5 → 10s.
        assert_eq!(intervals[0], Duration::from_secs(5));
        assert_eq!(intervals[1], Duration::from_secs(10));
    }

    #[test]
    fn poll_for_token_surfaces_expired_token_terminal_error() {
        let http = ScriptedHttp::default();
        http.enqueue(200, br#"{"error": "expired_token"}"#);
        let sleeper = InstantSleeper::default();
        let config = DeviceFlowConfig::default();
        let err = rt()
            .block_on(async { poll_for_token(&http, &sleeper, &config, "dc-1", 1).await })
            .unwrap_err();
        assert_eq!(err, DeviceFlowError::Expired);
    }

    #[test]
    fn poll_for_token_surfaces_access_denied_terminal_error() {
        let http = ScriptedHttp::default();
        http.enqueue(200, br#"{"error": "access_denied"}"#);
        let sleeper = InstantSleeper::default();
        let config = DeviceFlowConfig::default();
        let err = rt()
            .block_on(async { poll_for_token(&http, &sleeper, &config, "dc-1", 1).await })
            .unwrap_err();
        assert_eq!(err, DeviceFlowError::AccessDenied);
    }

    #[test]
    fn poll_for_token_passes_through_unknown_github_error_codes() {
        let http = ScriptedHttp::default();
        http.enqueue(200, br#"{"error": "novel_failure_mode"}"#);
        let sleeper = InstantSleeper::default();
        let config = DeviceFlowConfig::default();
        let err = rt()
            .block_on(async { poll_for_token(&http, &sleeper, &config, "dc-1", 1).await })
            .unwrap_err();
        assert_eq!(
            err,
            DeviceFlowError::GithubError("novel_failure_mode".into())
        );
    }

    #[test]
    fn enterprise_host_is_normalized_and_built_into_urls() {
        let config = DeviceFlowConfig {
            enterprise_host: Some("https://github.example.com/some/path".into()),
            ..DeviceFlowConfig::default()
        };
        assert_eq!(config.host(), "github.example.com");
        assert_eq!(
            config.device_code_url(),
            "https://github.example.com/login/device/code"
        );
        assert_eq!(
            config.access_token_url(),
            "https://github.example.com/login/oauth/access_token"
        );
    }
}
