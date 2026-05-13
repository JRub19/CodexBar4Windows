//! Google OAuth token refresh for Gemini. Ported from
//! `GeminiStatusProbe.refreshAccessToken`.
//!
//! When the on-disk access_token has expired we POST a
//! `grant_type=refresh_token` request to oauth2.googleapis.com using
//! the OAuth client credentials embedded in the installed @google/gemini-cli
//! package. On success the response carries a fresh `access_token` (and
//! sometimes a fresh `id_token` + `expires_in`); we persist all three
//! back to `~/.gemini/oauth_creds.json` so the next refresh tick reuses
//! them.

use std::path::Path;

use async_trait::async_trait;
use serde::Deserialize;

use super::client_locator::OAuthClientCredentials;
use super::credentials::{credentials_path, GeminiOAuthCredentials};
use crate::providers::errors::ProviderFetchError;

pub const REFRESH_URL: &str = "https://oauth2.googleapis.com/token";

#[async_trait]
pub trait RefreshHttp: Send + Sync {
    async fn post_form(&self, url: &str, body: &str)
        -> Result<RefreshResponse, ProviderFetchError>;
}

pub struct RefreshResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Debug, Deserialize)]
pub struct RefreshedTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub id_token: Option<String>,
    /// Lifetime in seconds. Google currently issues 1 hour tokens.
    #[serde(default)]
    pub expires_in: Option<f64>,
}

/// Single attempt at refreshing the access token. The caller passes in
/// the located OAuth client credentials and the existing on-disk
/// credentials; we return the new token bundle (caller persists it).
pub async fn refresh(
    http: &dyn RefreshHttp,
    client: &OAuthClientCredentials,
    refresh_token: &str,
) -> Result<RefreshedTokenResponse, ProviderFetchError> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", &client.client_id)
        .append_pair("client_secret", &client.client_secret)
        .append_pair("refresh_token", refresh_token)
        .append_pair("grant_type", "refresh_token")
        .finish();
    let response = http.post_form(REFRESH_URL, &body).await?;
    match response.status {
        200..=299 => serde_json::from_slice::<RefreshedTokenResponse>(&response.body)
            .map_err(|e| ProviderFetchError::ParseError(format!("token refresh: {e}"))),
        400 | 401 | 403 => Err(ProviderFetchError::Unauthorized),
        other => Err(ProviderFetchError::Network(format!(
            "google token endpoint returned {other}"
        ))),
    }
}

/// Persist a refreshed token bundle back to `~/.gemini/oauth_creds.json`,
/// preserving any fields the CLI cares about that we did not touch.
pub fn persist_to_disk(
    home: &Path,
    response: &RefreshedTokenResponse,
    now_unix_secs: i64,
) -> Result<(), ProviderFetchError> {
    let path = credentials_path(home);
    let existing = std::fs::read(&path).map_err(|e| {
        ProviderFetchError::UserConfigInvalid(format!("could not read oauth_creds.json: {e}"))
    })?;
    let mut value: serde_json::Value = serde_json::from_slice(&existing).map_err(|e| {
        ProviderFetchError::UserConfigInvalid(format!("oauth_creds.json invalid: {e}"))
    })?;
    let obj = value.as_object_mut().ok_or_else(|| {
        ProviderFetchError::UserConfigInvalid("oauth_creds.json is not a JSON object".into())
    })?;
    obj.insert(
        "access_token".into(),
        serde_json::Value::String(response.access_token.clone()),
    );
    if let Some(id_token) = response.id_token.as_ref() {
        obj.insert(
            "id_token".into(),
            serde_json::Value::String(id_token.clone()),
        );
    }
    if let Some(expires_in) = response.expires_in {
        let expiry_ms = ((now_unix_secs as f64) + expires_in) * 1000.0;
        if let Some(n) = serde_json::Number::from_f64(expiry_ms) {
            obj.insert("expiry_date".into(), serde_json::Value::Number(n));
        }
    }
    let serialized = serde_json::to_vec_pretty(&value)
        .map_err(|e| ProviderFetchError::ParseError(format!("serialize oauth_creds: {e}")))?;
    write_atomic(&path, &serialized)
        .map_err(|e| ProviderFetchError::UserConfigInvalid(format!("write oauth_creds.json: {e}")))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("oauth_creds.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Merge a refreshed token into an in-memory credential bundle so the
/// strategy can keep working without an extra disk roundtrip.
pub fn apply_in_memory(
    creds: &mut GeminiOAuthCredentials,
    response: &RefreshedTokenResponse,
    now_unix_secs: i64,
) {
    creds.access_token = Some(response.access_token.clone());
    if response.id_token.is_some() {
        creds.id_token = response.id_token.clone();
    }
    if let Some(expires_in) = response.expires_in {
        creds.expiry_unix_secs = Some(now_unix_secs + expires_in as i64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct StubHttp {
        reply: Mutex<Option<(u16, Vec<u8>)>>,
        captured: Mutex<Option<(String, String)>>,
    }
    impl StubHttp {
        fn new(status: u16, body: &[u8]) -> Self {
            Self {
                reply: Mutex::new(Some((status, body.to_vec()))),
                captured: Mutex::new(None),
            }
        }
    }
    #[async_trait]
    impl RefreshHttp for StubHttp {
        async fn post_form(
            &self,
            url: &str,
            body: &str,
        ) -> Result<RefreshResponse, ProviderFetchError> {
            *self.captured.lock().unwrap() = Some((url.into(), body.into()));
            let (status, body) = self.reply.lock().unwrap().take().unwrap();
            Ok(RefreshResponse { status, body })
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn client() -> OAuthClientCredentials {
        OAuthClientCredentials {
            client_id: "client-id.apps.googleusercontent.com".into(),
            client_secret: "GOCSPX-secret".into(),
        }
    }

    #[test]
    fn refresh_posts_form_encoded_body_and_returns_new_token() {
        let http = StubHttp::new(
            200,
            br#"{"access_token":"new-token","expires_in":3600,"id_token":"new-id"}"#,
        );
        let response = rt()
            .block_on(async { refresh(&http, &client(), "rt-1").await })
            .unwrap();
        assert_eq!(response.access_token, "new-token");
        assert_eq!(response.id_token.as_deref(), Some("new-id"));
        assert_eq!(response.expires_in, Some(3600.0));
        let (url, body) = http.captured.lock().unwrap().clone().unwrap();
        assert_eq!(url, REFRESH_URL);
        assert!(body.contains("client_id=client-id.apps.googleusercontent.com"));
        assert!(body.contains("client_secret=GOCSPX-secret"));
        assert!(body.contains("refresh_token=rt-1"));
        assert!(body.contains("grant_type=refresh_token"));
    }

    #[test]
    fn refresh_401_maps_to_unauthorized() {
        let http = StubHttp::new(401, br#"{"error":"invalid_grant"}"#);
        let err = rt()
            .block_on(async { refresh(&http, &client(), "rt-bad").await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn refresh_500_maps_to_network_error() {
        let http = StubHttp::new(500, br#"{"error":"backend_error"}"#);
        let err = rt()
            .block_on(async { refresh(&http, &client(), "rt").await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Network(_)));
    }

    #[test]
    fn refresh_malformed_response_maps_to_parse_error() {
        let http = StubHttp::new(200, b"not json");
        let err = rt()
            .block_on(async { refresh(&http, &client(), "rt").await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::ParseError(_)));
    }

    #[test]
    fn apply_in_memory_updates_access_id_and_expiry() {
        let mut creds = GeminiOAuthCredentials {
            access_token: Some("old".into()),
            id_token: Some("old-id".into()),
            refresh_token: Some("rt".into()),
            expiry_unix_secs: Some(1000),
        };
        let response = RefreshedTokenResponse {
            access_token: "new".into(),
            id_token: Some("new-id".into()),
            expires_in: Some(60.0),
        };
        apply_in_memory(&mut creds, &response, 2000);
        assert_eq!(creds.access_token.as_deref(), Some("new"));
        assert_eq!(creds.id_token.as_deref(), Some("new-id"));
        assert_eq!(creds.expiry_unix_secs, Some(2060));
        // refresh_token is preserved verbatim.
        assert_eq!(creds.refresh_token.as_deref(), Some("rt"));
    }

    #[test]
    fn apply_in_memory_preserves_id_token_when_response_omits_it() {
        let mut creds = GeminiOAuthCredentials {
            access_token: Some("old".into()),
            id_token: Some("keep-this".into()),
            refresh_token: Some("rt".into()),
            expiry_unix_secs: Some(0),
        };
        let response = RefreshedTokenResponse {
            access_token: "new".into(),
            id_token: None,
            expires_in: Some(60.0),
        };
        apply_in_memory(&mut creds, &response, 0);
        assert_eq!(creds.id_token.as_deref(), Some("keep-this"));
    }

    #[test]
    fn persist_to_disk_rewrites_oauth_creds_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let gemini_dir = dir.path().join(".gemini");
        std::fs::create_dir_all(&gemini_dir).unwrap();
        std::fs::write(
            gemini_dir.join("oauth_creds.json"),
            r#"{"access_token":"old","id_token":"old-id","refresh_token":"rt-keep","expiry_date":1000}"#,
        )
        .unwrap();
        let response = RefreshedTokenResponse {
            access_token: "fresh".into(),
            id_token: Some("fresh-id".into()),
            expires_in: Some(60.0),
        };
        persist_to_disk(dir.path(), &response, 2000).unwrap();
        let after = std::fs::read_to_string(gemini_dir.join("oauth_creds.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&after).unwrap();
        assert_eq!(value["access_token"], "fresh");
        assert_eq!(value["id_token"], "fresh-id");
        // expiry_date is in ms.
        assert_eq!(value["expiry_date"], 2060.0 * 1000.0);
        // refresh_token preserved unchanged.
        assert_eq!(value["refresh_token"], "rt-keep");
    }
}
