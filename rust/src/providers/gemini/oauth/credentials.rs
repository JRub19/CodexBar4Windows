//! Gemini credential discovery. Reads `~/.gemini/oauth_creds.json` and
//! `~/.gemini/settings.json` to mirror what the macOS app does. Only
//! `oauth-personal` auth is supported here; `api-key` and `vertex-ai`
//! are explicit rejections matching `GeminiStatusProbe.swift` so the
//! popup surfaces a "use Google account" hint.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::providers::errors::ProviderFetchError;

pub const CREDENTIALS_RELATIVE: &str = ".gemini/oauth_creds.json";
pub const SETTINGS_RELATIVE: &str = ".gemini/settings.json";

#[derive(Clone, Debug, PartialEq)]
pub struct GeminiOAuthCredentials {
    pub access_token: Option<String>,
    pub id_token: Option<String>,
    pub refresh_token: Option<String>,
    /// `expiry_date` from the CLI is unix milliseconds, here normalised
    /// to seconds.
    pub expiry_unix_secs: Option<i64>,
}

impl GeminiOAuthCredentials {
    pub fn is_expired(&self, now_unix_secs: i64) -> bool {
        match self.expiry_unix_secs {
            Some(t) => t <= now_unix_secs,
            None => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeminiAuthType {
    OauthPersonal,
    ApiKey,
    VertexAI,
    Unknown,
}

impl GeminiAuthType {
    pub fn from_raw(value: &str) -> Self {
        match value {
            "oauth-personal" => GeminiAuthType::OauthPersonal,
            "api-key" => GeminiAuthType::ApiKey,
            "vertex-ai" => GeminiAuthType::VertexAI,
            _ => GeminiAuthType::Unknown,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CredsWire {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    /// Unix milliseconds, per the Gemini CLI on-disk format.
    #[serde(default)]
    expiry_date: Option<f64>,
}

pub fn credentials_path(home: &Path) -> PathBuf {
    home.join(CREDENTIALS_RELATIVE)
}

pub fn settings_path(home: &Path) -> PathBuf {
    home.join(SETTINGS_RELATIVE)
}

pub fn load_credentials(home: &Path) -> Result<GeminiOAuthCredentials, ProviderFetchError> {
    let path = credentials_path(home);
    let bytes = std::fs::read(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ProviderFetchError::NoToken("gemini")
        } else {
            ProviderFetchError::UserConfigInvalid(format!(
                "gemini oauth_creds.json read failed: {e}"
            ))
        }
    })?;
    parse_credentials(&bytes)
}

pub fn parse_credentials(bytes: &[u8]) -> Result<GeminiOAuthCredentials, ProviderFetchError> {
    let wire: CredsWire = serde_json::from_slice(bytes).map_err(|e| {
        ProviderFetchError::UserConfigInvalid(format!("gemini oauth_creds.json invalid: {e}"))
    })?;
    let expiry_unix_secs = wire.expiry_date.map(|ms| (ms / 1000.0) as i64);
    Ok(GeminiOAuthCredentials {
        access_token: wire.access_token,
        id_token: wire.id_token,
        refresh_token: wire.refresh_token,
        expiry_unix_secs,
    })
}

pub fn load_auth_type(home: &Path) -> GeminiAuthType {
    let path = settings_path(home);
    let Ok(bytes) = std::fs::read(&path) else {
        return GeminiAuthType::Unknown;
    };
    parse_auth_type(&bytes)
}

pub fn parse_auth_type(bytes: &[u8]) -> GeminiAuthType {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return GeminiAuthType::Unknown;
    };
    value
        .get("security")
        .and_then(|s| s.get("auth"))
        .and_then(|a| a.get("selectedType"))
        .and_then(|t| t.as_str())
        .map(GeminiAuthType::from_raw)
        .unwrap_or(GeminiAuthType::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_credentials_with_expiry_ms() {
        let body = br#"{
            "access_token": "tok",
            "id_token": "header.eyJlbWFpbCI6InVAeC5jb20ifQ.sig",
            "refresh_token": "rt",
            "expiry_date": 1700000000000.0
        }"#;
        let creds = parse_credentials(body).unwrap();
        assert_eq!(creds.access_token.as_deref(), Some("tok"));
        assert_eq!(creds.refresh_token.as_deref(), Some("rt"));
        assert_eq!(creds.expiry_unix_secs, Some(1_700_000_000));
    }

    #[test]
    fn credentials_missing_optional_fields_default_to_none() {
        let body = br#"{"expiry_date": null}"#;
        let creds = parse_credentials(body).unwrap();
        assert!(creds.access_token.is_none());
        assert!(creds.refresh_token.is_none());
        assert!(creds.expiry_unix_secs.is_none());
    }

    #[test]
    fn is_expired_returns_true_when_expiry_in_past() {
        let creds = GeminiOAuthCredentials {
            access_token: None,
            id_token: None,
            refresh_token: None,
            expiry_unix_secs: Some(100),
        };
        assert!(creds.is_expired(200));
        assert!(!creds.is_expired(50));
    }

    #[test]
    fn is_expired_returns_false_when_expiry_absent() {
        let creds = GeminiOAuthCredentials {
            access_token: None,
            id_token: None,
            refresh_token: None,
            expiry_unix_secs: None,
        };
        assert!(!creds.is_expired(99999));
    }

    #[test]
    fn parses_auth_type_oauth_personal() {
        let body = br#"{"security": {"auth": {"selectedType": "oauth-personal"}}}"#;
        assert_eq!(parse_auth_type(body), GeminiAuthType::OauthPersonal);
    }

    #[test]
    fn parses_auth_type_api_key_and_vertex() {
        assert_eq!(
            parse_auth_type(br#"{"security": {"auth": {"selectedType": "api-key"}}}"#),
            GeminiAuthType::ApiKey
        );
        assert_eq!(
            parse_auth_type(br#"{"security": {"auth": {"selectedType": "vertex-ai"}}}"#),
            GeminiAuthType::VertexAI
        );
    }

    #[test]
    fn unknown_auth_type_when_field_missing_or_garbage() {
        assert_eq!(parse_auth_type(b"{}"), GeminiAuthType::Unknown);
        assert_eq!(parse_auth_type(b"not json"), GeminiAuthType::Unknown);
        assert_eq!(
            parse_auth_type(br#"{"security": {"auth": {"selectedType": "future-mode"}}}"#),
            GeminiAuthType::Unknown
        );
    }

    #[test]
    fn invalid_credentials_json_maps_to_user_config_invalid() {
        let err = parse_credentials(b"not json").unwrap_err();
        assert!(matches!(err, ProviderFetchError::UserConfigInvalid(_)));
    }
}
