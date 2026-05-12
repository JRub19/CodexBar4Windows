//! 8-day OAuth refresh against `auth.openai.com`. Spec 41 §3.4 fixes
//! the request shape; we use a pluggable HTTP transport so tests can
//! exercise the error mapping without a real network.

use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use serde::Deserialize;

use super::errors::RefreshError;

pub const REFRESH_URL: &str = "https://auth.openai.com/oauth/token";
pub const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const SCOPE: &str = "openid profile email offline_access";
pub const PER_REFRESH_TIMEOUT: Duration = Duration::from_secs(30);
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(8 * 24 * 3600);

#[async_trait]
pub trait RefreshHttp: Send + Sync {
    async fn post_form(
        &self,
        url: &str,
        form: &[(&str, &str)],
    ) -> Result<RefreshResponse, RefreshError>;
}

pub struct RefreshResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Returns true when the credential file is stale enough that the next
/// strategy tick should refresh. We refresh proactively at the 8-day
/// mark so the token never expires mid-request.
pub fn needs_refresh(last_refresh: Option<&str>, now: SystemTime) -> bool {
    let Some(stamp) = last_refresh else {
        return true;
    };
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(stamp) else {
        return true;
    };
    let last = parsed.with_timezone(&chrono::Utc).timestamp();
    let now_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    (now_secs - last) > REFRESH_INTERVAL.as_secs() as i64
}

/// Format the `last_refresh` field per the Codex CLI convention. We use
/// RFC 3339 with fractional seconds so a round trip through the file
/// preserves the timestamp the CLI would have written.
pub fn now_stamp(now: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(now)
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

#[derive(Debug, Deserialize)]
struct OkBody {
    access_token: String,
    refresh_token: String,
    id_token: String,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: Option<ErrorPayload>,
    #[serde(default)]
    code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorPayload {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshedTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
}

/// Run one refresh attempt. Returns the refreshed token triple on 200,
/// otherwise maps the 401 error code per spec 41 §3.4.
pub async fn refresh(
    http: &dyn RefreshHttp,
    refresh_token: &str,
) -> Result<RefreshedTokens, RefreshError> {
    let form = [
        ("client_id", CLIENT_ID),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("scope", SCOPE),
    ];
    let response = http.post_form(REFRESH_URL, &form).await?;
    match response.status {
        200 => {
            let parsed: OkBody = serde_json::from_slice(&response.body)
                .map_err(|e| RefreshError::InvalidResponse(e.to_string()))?;
            Ok(RefreshedTokens {
                access_token: parsed.access_token,
                refresh_token: parsed.refresh_token,
                id_token: parsed.id_token,
            })
        }
        401 => Err(map_401(&response.body)),
        status => Err(RefreshError::InvalidResponse(format!(
            "unexpected status {status}"
        ))),
    }
}

fn map_401(body: &[u8]) -> RefreshError {
    let parsed: ErrorBody = serde_json::from_slice(body).unwrap_or(ErrorBody {
        error: None,
        code: None,
    });
    let code = parsed
        .error
        .as_ref()
        .and_then(|e| e.code.clone().or_else(|| e.error.clone()))
        .or(parsed.code);
    match code.as_deref() {
        Some("refresh_token_expired") | Some("invalid_grant") => RefreshError::Expired,
        Some("refresh_token_reused") => RefreshError::Reused,
        Some("refresh_token_invalidated") | Some("refresh_token_revoked") => RefreshError::Revoked,
        // Unknown 401 codes are terminal per spec 41 §3.4.
        _ => RefreshError::Expired,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct StaticHttp {
        responses: Mutex<Vec<(u16, Vec<u8>)>>,
    }

    impl StaticHttp {
        fn with(status: u16, body: Vec<u8>) -> Self {
            Self {
                responses: Mutex::new(vec![(status, body)]),
            }
        }
    }

    #[async_trait]
    impl RefreshHttp for StaticHttp {
        async fn post_form(
            &self,
            _: &str,
            _: &[(&str, &str)],
        ) -> Result<RefreshResponse, RefreshError> {
            let (status, body) = self
                .responses
                .lock()
                .unwrap()
                .pop()
                .expect("stub exhausted");
            Ok(RefreshResponse { status, body })
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn happy_path_parses_token_triple() {
        let http = StaticHttp::with(
            200,
            br#"{"access_token":"a","refresh_token":"r","id_token":"i"}"#.to_vec(),
        );
        let tokens = rt()
            .block_on(async { refresh(&http, "old").await })
            .unwrap();
        assert_eq!(tokens.access_token, "a");
    }

    #[test]
    fn unknown_401_code_maps_to_expired() {
        let http = StaticHttp::with(401, br#"{"error":{"code":"weird_thing"}}"#.to_vec());
        let err = rt()
            .block_on(async { refresh(&http, "old").await })
            .unwrap_err();
        assert_eq!(err, RefreshError::Expired);
    }

    #[test]
    fn known_401_codes_map_correctly() {
        for (code, expected) in [
            ("refresh_token_expired", RefreshError::Expired),
            ("invalid_grant", RefreshError::Expired),
            ("refresh_token_reused", RefreshError::Reused),
            ("refresh_token_revoked", RefreshError::Revoked),
            ("refresh_token_invalidated", RefreshError::Revoked),
        ] {
            let body = format!(r#"{{"error":{{"code":"{code}"}}}}"#);
            let http = StaticHttp::with(401, body.into_bytes());
            let err = rt()
                .block_on(async { refresh(&http, "old").await })
                .unwrap_err();
            assert_eq!(err, expected, "wrong mapping for {code}");
        }
    }

    #[test]
    fn needs_refresh_after_eight_days() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let nine_days_ago =
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 - 9 * 24 * 3600);
        assert!(needs_refresh(Some(&now_stamp(nine_days_ago)), now));
        assert!(!needs_refresh(Some(&now_stamp(now)), now));
    }

    #[test]
    fn needs_refresh_is_true_when_no_timestamp() {
        let now = SystemTime::now();
        assert!(needs_refresh(None, now));
    }

    #[test]
    fn malformed_timestamp_triggers_refresh() {
        let now = SystemTime::now();
        assert!(needs_refresh(Some("totally bogus"), now));
    }
}
