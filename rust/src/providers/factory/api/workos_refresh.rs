//! WorkOS refresh-token exchange for Factory. Ported from
//! `FactoryStatusProbe.fetchWorkOSAccessToken` + `fetchWorkOSAccessTokenWithCookies`.
//!
//! Factory authenticates via WorkOS. A stored refresh token (or a
//! WorkOS session cookie) can be traded for a fresh access token by
//! POSTing to `https://api.workos.com/user_management/authenticate`.
//! WorkOS rotates client IDs occasionally so we try each in turn until
//! one accepts the grant.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::providers::errors::ProviderFetchError;

pub const WORKOS_URL: &str = "https://api.workos.com/user_management/authenticate";

/// Known Factory client IDs registered with WorkOS. Ported verbatim
/// from the macOS source; new IDs would also be added on the Swift
/// side, so this list stays in sync.
pub const WORKOS_CLIENT_IDS: &[&str] = &[
    "client_01HXRMBQ9BJ3E7QSTQ9X2PHVB7",
    "client_01HNM792M5G5G1A2THWPXKFMXB",
];

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorkOSAuthResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub organization_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct RefreshRequestBody<'a> {
    client_id: &'a str,
    grant_type: &'static str,
    refresh_token: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    organization_id: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct CookieRefreshRequestBody<'a> {
    client_id: &'a str,
    grant_type: &'static str,
    #[serde(rename = "useCookie")]
    use_cookie: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    organization_id: Option<&'a str>,
}

pub struct WorkOSResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[async_trait]
pub trait WorkOSHttp: Send + Sync {
    async fn post_json(
        &self,
        url: &str,
        body: &str,
        cookie_header: Option<&str>,
    ) -> Result<WorkOSResponse, ProviderFetchError>;
}

/// Try every known client ID in turn until WorkOS accepts the refresh
/// token. Returns the first success; on failure, surfaces the last
/// error so the caller sees the most informative reason.
pub async fn exchange_refresh_token(
    http: &dyn WorkOSHttp,
    refresh_token: &str,
    organization_id: Option<&str>,
) -> Result<WorkOSAuthResponse, ProviderFetchError> {
    let mut last_error: Option<ProviderFetchError> = None;
    for client_id in WORKOS_CLIENT_IDS {
        let body = serde_json::to_string(&RefreshRequestBody {
            client_id,
            grant_type: "refresh_token",
            refresh_token,
            organization_id,
        })
        .map_err(|e| ProviderFetchError::ParseError(e.to_string()))?;
        match http.post_json(WORKOS_URL, &body, None).await {
            Ok(response) => match classify(&response) {
                Outcome::Success(auth) => return Ok(auth),
                Outcome::MissingRefreshToken => {
                    return Err(ProviderFetchError::UserConfigInvalid(
                        "WorkOS refresh token expired or missing; sign in again".into(),
                    ))
                }
                Outcome::Retry(err) => last_error = Some(err),
                Outcome::Terminal(err) => return Err(err),
            },
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        ProviderFetchError::Network("WorkOS rejected every known client ID".into())
    }))
}

/// Cookie-based variant: send the WorkOS session cookie header
/// instead of a stored refresh token. WorkOS extracts the refresh
/// token from the cookie when `useCookie: true` is set.
pub async fn exchange_cookie(
    http: &dyn WorkOSHttp,
    cookie_header: &str,
    organization_id: Option<&str>,
) -> Result<WorkOSAuthResponse, ProviderFetchError> {
    if cookie_header.trim().is_empty() {
        return Err(ProviderFetchError::NoCookies("factory"));
    }
    let mut last_error: Option<ProviderFetchError> = None;
    for client_id in WORKOS_CLIENT_IDS {
        let body = serde_json::to_string(&CookieRefreshRequestBody {
            client_id,
            grant_type: "refresh_token",
            use_cookie: true,
            organization_id,
        })
        .map_err(|e| ProviderFetchError::ParseError(e.to_string()))?;
        match http.post_json(WORKOS_URL, &body, Some(cookie_header)).await {
            Ok(response) => match classify(&response) {
                Outcome::Success(auth) => return Ok(auth),
                Outcome::MissingRefreshToken => {
                    return Err(ProviderFetchError::Unauthorized);
                }
                Outcome::Retry(err) => last_error = Some(err),
                Outcome::Terminal(err) => return Err(err),
            },
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        ProviderFetchError::Network("WorkOS cookie auth rejected every known client ID".into())
    }))
}

enum Outcome {
    Success(WorkOSAuthResponse),
    MissingRefreshToken,
    Retry(ProviderFetchError),
    Terminal(ProviderFetchError),
}

fn classify(response: &WorkOSResponse) -> Outcome {
    match response.status {
        200..=299 => match serde_json::from_slice::<WorkOSAuthResponse>(&response.body) {
            Ok(auth) => Outcome::Success(auth),
            Err(e) => Outcome::Terminal(ProviderFetchError::ParseError(format!(
                "WorkOS response: {e}"
            ))),
        },
        400 if is_missing_refresh_token(&response.body) => Outcome::MissingRefreshToken,
        // 400 / 401 with another body might be a client_id mismatch
        // — retry the next known ID. Anything else is terminal.
        400 | 401 => Outcome::Retry(ProviderFetchError::Network(format!(
            "WorkOS HTTP {} for this client id",
            response.status
        ))),
        other => Outcome::Terminal(ProviderFetchError::Network(format!(
            "WorkOS HTTP {other}"
        ))),
    }
}

/// Detects WorkOS's `400 invalid_grant` payload, which means the
/// stored refresh token has been revoked / expired. The Swift source
/// recognises the same JSON shape; we keep the same heuristic.
fn is_missing_refresh_token(bytes: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return false;
    };
    let error = value
        .get("error")
        .or_else(|| value.get("code"))
        .and_then(|v| v.as_str());
    let message = value
        .get("error_description")
        .or_else(|| value.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    matches!(error, Some("invalid_grant") | Some("invalid_request"))
        || message.to_ascii_lowercase().contains("refresh token")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    type CapturedCall = (String, Option<String>);

    struct StubHttp {
        replies: Mutex<Vec<WorkOSResponse>>,
        captured: Mutex<Vec<CapturedCall>>,
    }
    impl StubHttp {
        fn new() -> Self {
            Self {
                replies: Mutex::new(Vec::new()),
                captured: Mutex::new(Vec::new()),
            }
        }
        fn enqueue(&self, status: u16, body: &[u8]) {
            self.replies.lock().unwrap().push(WorkOSResponse {
                status,
                body: body.to_vec(),
            });
        }
    }
    #[async_trait]
    impl WorkOSHttp for StubHttp {
        async fn post_json(
            &self,
            _url: &str,
            body: &str,
            cookie: Option<&str>,
        ) -> Result<WorkOSResponse, ProviderFetchError> {
            self.captured
                .lock()
                .unwrap()
                .push((body.into(), cookie.map(|s| s.to_string())));
            let mut replies = self.replies.lock().unwrap();
            if replies.is_empty() {
                return Err(ProviderFetchError::Network("stub exhausted".into()));
            }
            Ok(replies.remove(0))
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn exchange_refresh_token_returns_first_success() {
        let http = StubHttp::new();
        http.enqueue(
            200,
            br#"{"access_token":"at","refresh_token":"new-rt","organization_id":"org-1"}"#,
        );
        let auth = rt()
            .block_on(async { exchange_refresh_token(&http, "rt-1", None).await })
            .unwrap();
        assert_eq!(auth.access_token, "at");
        assert_eq!(auth.refresh_token.as_deref(), Some("new-rt"));
        assert_eq!(auth.organization_id.as_deref(), Some("org-1"));
        // POSTed body contains the first client ID and grant_type.
        let captured = http.captured.lock().unwrap();
        assert!(captured[0].0.contains("client_01HXRMBQ9BJ3E7QSTQ9X2PHVB7"));
        assert!(captured[0].0.contains("refresh_token"));
    }

    #[test]
    fn exchange_refresh_token_retries_second_client_id_after_400() {
        let http = StubHttp::new();
        // First client_id fails with "this client doesn't recognise the token" 400.
        http.enqueue(
            400,
            br#"{"error":"invalid_client_id","message":"unknown client"}"#,
        );
        // Second client_id succeeds.
        http.enqueue(200, br#"{"access_token":"at-2","refresh_token":"rt-2"}"#);
        let auth = rt()
            .block_on(async { exchange_refresh_token(&http, "rt-1", None).await })
            .unwrap();
        assert_eq!(auth.access_token, "at-2");
        assert_eq!(http.captured.lock().unwrap().len(), 2);
    }

    #[test]
    fn exchange_refresh_token_surfaces_user_config_invalid_on_invalid_grant() {
        let http = StubHttp::new();
        http.enqueue(
            400,
            br#"{"error":"invalid_grant","error_description":"refresh token expired"}"#,
        );
        let err = rt()
            .block_on(async { exchange_refresh_token(&http, "rt-stale", None).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::UserConfigInvalid(_)));
    }

    #[test]
    fn exchange_refresh_token_propagates_500_terminal() {
        let http = StubHttp::new();
        http.enqueue(500, br#"{"error":"internal"}"#);
        let err = rt()
            .block_on(async { exchange_refresh_token(&http, "rt", None).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Network(_)));
    }

    #[test]
    fn exchange_cookie_sends_use_cookie_flag_and_cookie_header() {
        let http = StubHttp::new();
        http.enqueue(200, br#"{"access_token":"at","refresh_token":"rt"}"#);
        let _ = rt()
            .block_on(async {
                exchange_cookie(&http, "wos-session=abc; other=1", None).await
            })
            .unwrap();
        let captured = http.captured.lock().unwrap();
        assert!(captured[0].0.contains("\"useCookie\":true"));
        assert_eq!(captured[0].1.as_deref(), Some("wos-session=abc; other=1"));
    }

    #[test]
    fn exchange_cookie_requires_a_non_empty_cookie_header() {
        let http = StubHttp::new();
        let err = rt()
            .block_on(async { exchange_cookie(&http, "   ", None).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoCookies("factory")));
    }

    #[test]
    fn is_missing_refresh_token_recognises_workos_invalid_grant() {
        assert!(is_missing_refresh_token(
            br#"{"error":"invalid_grant","error_description":"refresh token expired"}"#
        ));
        assert!(is_missing_refresh_token(
            br#"{"code":"invalid_request","message":"refresh token missing"}"#
        ));
        assert!(!is_missing_refresh_token(
            br#"{"error":"invalid_client_id","message":"unknown client"}"#
        ));
    }
}
