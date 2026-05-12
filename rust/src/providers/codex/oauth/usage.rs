//! Compose the wham/usage request and fold the response into the
//! framework `UsageSnapshot`. The HTTP transport is pluggable so tests
//! can drive every error branch with a stub.

use std::time::Duration;

use async_trait::async_trait;

use super::wham_response::{decode_tolerant, RateWindowWire, WhamResponse};
use crate::providers::codex::auth::errors::CodexOAuthError;

pub const DEFAULT_ENDPOINT: &str = "https://chatgpt.com/backend-api/wham/usage";
pub const PER_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[async_trait]
pub trait UsageHttp: Send + Sync {
    async fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<UsageResponse, CodexOAuthError>;
}

pub struct UsageResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct UsageRequest<'a> {
    pub access_token: &'a str,
    pub account_id: Option<&'a str>,
}

/// Resolve the endpoint URL from an optional `chatgpt_base_url`. If the
/// base lacks `/backend-api`, we suffix the alt path per spec 41 §3.5.
pub fn resolve_endpoint(chatgpt_base_url: Option<&str>) -> String {
    let Some(base) = chatgpt_base_url else {
        return DEFAULT_ENDPOINT.to_string();
    };
    let trimmed = base.trim_end_matches('/');
    if trimmed.contains("/backend-api") {
        format!("{trimmed}/wham/usage")
    } else {
        format!("{trimmed}/api/codex/usage")
    }
}

pub async fn fetch_usage(
    http: &dyn UsageHttp,
    endpoint: &str,
    request: UsageRequest<'_>,
) -> Result<(WhamResponse, super::wham_response::DecodeFlags), CodexOAuthError> {
    let bearer = format!("Bearer {}", request.access_token);
    let mut headers: Vec<(&str, &str)> = vec![
        ("Authorization", bearer.as_str()),
        ("Accept", "application/json"),
        ("Accept-Language", "en-US,en;q=0.9"),
        ("User-Agent", "CodexBar"),
    ];
    if let Some(id) = request.account_id {
        headers.push(("ChatGPT-Account-Id", id));
    }
    let response = http.get(endpoint, &headers).await?;
    match response.status {
        200..=299 => {
            let (parsed, flags) = decode_tolerant(&response.body);
            if parsed.primary_window.is_none()
                && parsed.secondary_window.is_none()
                && parsed.credits.is_none()
            {
                return Err(CodexOAuthError::InvalidResponse);
            }
            Ok((parsed, flags))
        }
        401 => Err(CodexOAuthError::Unauthorized),
        other => Err(CodexOAuthError::ServerError(other)),
    }
}

/// Folds the wire response into framework windows. Spec 41 §3.5 maps
/// `primary_window` to the session bar and `secondary_window` to the
/// weekly bar.
pub fn windows_from_response(
    response: &WhamResponse,
) -> Vec<crate::providers::models::rate_window::NamedRateWindow> {
    use crate::providers::models::rate_window::NamedRateWindow;
    let mut out = Vec::new();
    if let Some(w) = &response.primary_window {
        out.push(NamedRateWindow {
            key: "session".into(),
            window: window_from_wire("Session", w),
        });
    }
    if let Some(w) = &response.secondary_window {
        out.push(NamedRateWindow {
            key: "weekly".into(),
            window: window_from_wire("Week", w),
        });
    }
    out
}

fn window_from_wire(
    label: &str,
    wire: &RateWindowWire,
) -> crate::providers::models::rate_window::RateWindow {
    crate::providers::models::rate_window::RateWindow {
        label: label.into(),
        used: wire.used.unwrap_or(0.0),
        allotted: wire.allotted,
        reset_at_unix_secs: wire.resets_at_epoch,
        pace_delta_percent: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    type CapturedHeaders = Vec<(String, String)>;
    type CapturedCall = (String, CapturedHeaders);

    struct StubHttp {
        next: Mutex<Option<(u16, Vec<u8>)>>,
        captured: Mutex<Vec<CapturedCall>>,
    }

    impl StubHttp {
        fn new(status: u16, body: &[u8]) -> Self {
            Self {
                next: Mutex::new(Some((status, body.to_vec()))),
                captured: Mutex::new(Vec::new()),
            }
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
                url.to_string(),
                headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ));
            let (status, body) = self.next.lock().unwrap().take().expect("stub used twice");
            Ok(UsageResponse { status, body })
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn resolves_default_endpoint_when_base_unset() {
        assert_eq!(resolve_endpoint(None), DEFAULT_ENDPOINT);
    }

    #[test]
    fn resolves_alt_endpoint_when_base_lacks_backend_api() {
        let url = resolve_endpoint(Some("https://my-corp.example.com"));
        assert_eq!(url, "https://my-corp.example.com/api/codex/usage");
    }

    #[test]
    fn resolves_appended_endpoint_when_base_contains_backend_api() {
        let url = resolve_endpoint(Some("https://proxy.example.com/backend-api"));
        assert_eq!(url, "https://proxy.example.com/backend-api/wham/usage");
    }

    #[test]
    fn happy_path_returns_response_and_emits_expected_headers() {
        let body = br#"{
            "primary_window": {"used": 10.0, "allotted": 100.0},
            "account": {"email": "u@x.com", "plan_type": "plus", "account_id": "acct"}
        }"#;
        let stub = StubHttp::new(200, body);
        let (response, flags) = rt()
            .block_on(async {
                fetch_usage(
                    &stub,
                    DEFAULT_ENDPOINT,
                    UsageRequest {
                        access_token: "tok",
                        account_id: Some("acct"),
                    },
                )
                .await
            })
            .unwrap();
        assert!(response.primary_window.is_some());
        assert!(!flags.primary_window_decode_failed);
        let captured = stub.captured.lock().unwrap();
        let (url, headers) = &captured[0];
        assert_eq!(url, DEFAULT_ENDPOINT);
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v.starts_with("Bearer ")));
        assert!(headers
            .iter()
            .any(|(k, v)| k == "ChatGPT-Account-Id" && v == "acct"));
    }

    #[test]
    fn http_401_maps_to_unauthorized() {
        let stub = StubHttp::new(401, b"{}");
        let err = rt()
            .block_on(async {
                fetch_usage(
                    &stub,
                    DEFAULT_ENDPOINT,
                    UsageRequest {
                        access_token: "t",
                        account_id: None,
                    },
                )
                .await
            })
            .unwrap_err();
        assert_eq!(err, CodexOAuthError::Unauthorized);
    }

    #[test]
    fn empty_payload_is_invalid_response() {
        let stub = StubHttp::new(200, b"{}");
        let err = rt()
            .block_on(async {
                fetch_usage(
                    &stub,
                    DEFAULT_ENDPOINT,
                    UsageRequest {
                        access_token: "t",
                        account_id: None,
                    },
                )
                .await
            })
            .unwrap_err();
        assert_eq!(err, CodexOAuthError::InvalidResponse);
    }

    #[test]
    fn windows_from_response_emits_session_and_week() {
        let body = br#"{
            "primary_window": {"used": 5.0, "allotted": 100.0},
            "secondary_window": {"used": 80.0, "allotted": 1000.0}
        }"#;
        let (parsed, _flags) = decode_tolerant(body);
        let windows = windows_from_response(&parsed);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "session");
        assert_eq!(windows[1].key, "weekly");
    }
}
