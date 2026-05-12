//! Codex credentials. Spec 41 §3.1 documents the on-disk shape:
//!
//! ```json
//! {
//!   "access_token": "...",
//!   "refresh_token": "...",
//!   "id_token": "...",
//!   "last_refresh": "2026-05-12T08:30:00.123Z",
//!   "OPENAI_API_KEY": "sk-..."           // optional, degraded mode
//! }
//! ```
//!
//! The macOS reference accepts both `snake_case` and `camelCase`; we
//! mirror that to keep `auth.json` files written by Codex Code on macOS
//! readable on Windows. Writes always emit `snake_case` so a round
//! trip lands on the canonical shape.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexCredentialsFull {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    #[serde(default)]
    pub last_refresh: Option<String>,
    /// Optional plain API key. Some users stash an `OPENAI_API_KEY`
    /// without OAuth at all; we keep it so we do not strip the user's
    /// own data when round-tripping.
    #[serde(default, rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodexCredentials {
    Full(CodexCredentialsFull),
    /// Degraded: only an API key, no OAuth tokens. The Codex backend
    /// supports this for API-only usage; we treat it as unavailable for
    /// quota/usage queries but still surface the account in settings.
    ApiKeyOnly(String),
}

impl CodexCredentials {
    /// Parse the on-disk credentials. The real Codex CLI writes a
    /// nested shape (`tokens.access_token` etc.); older test fixtures
    /// and the macOS reference use the top-level shape. We accept
    /// both, plus camelCase aliases.
    pub fn parse(bytes: &[u8]) -> Result<Self, CredentialsParseError> {
        // Try the real Codex CLI shape first.
        #[derive(Deserialize)]
        struct NestedWire {
            #[serde(default, rename = "OPENAI_API_KEY")]
            openai_api_key: Option<String>,
            #[serde(default)]
            tokens: Option<TokensInner>,
            #[serde(default)]
            last_refresh: Option<String>,
        }
        #[derive(Deserialize)]
        struct TokensInner {
            #[serde(default)]
            access_token: Option<String>,
            #[serde(default)]
            refresh_token: Option<String>,
            #[serde(default)]
            id_token: Option<String>,
            #[serde(default)]
            #[allow(dead_code)]
            account_id: Option<String>,
        }
        if let Ok(nested) = serde_json::from_slice::<NestedWire>(bytes) {
            if let Some(tokens) = nested.tokens {
                if let (Some(a), Some(r), Some(i)) = (
                    tokens.access_token.filter(|s| !s.is_empty()),
                    tokens.refresh_token.filter(|s| !s.is_empty()),
                    tokens.id_token,
                ) {
                    return Ok(CodexCredentials::Full(CodexCredentialsFull {
                        access_token: a,
                        refresh_token: r,
                        id_token: i,
                        last_refresh: nested.last_refresh,
                        openai_api_key: nested.openai_api_key.filter(|s| !s.is_empty()),
                    }));
                }
            }
        }
        // Try the snake_case top-level shape.
        if let Ok(full) = serde_json::from_slice::<CodexCredentialsFull>(bytes) {
            if !full.access_token.is_empty() && !full.refresh_token.is_empty() {
                return Ok(CodexCredentials::Full(full));
            }
        }
        // Try camelCase reading. We accept the Mac key names as aliases.
        #[derive(Deserialize)]
        struct CamelWire {
            #[serde(rename = "accessToken")]
            access_token: Option<String>,
            #[serde(rename = "refreshToken")]
            refresh_token: Option<String>,
            #[serde(rename = "idToken")]
            id_token: Option<String>,
            #[serde(default, rename = "lastRefresh")]
            last_refresh: Option<String>,
            #[serde(default, rename = "OPENAI_API_KEY")]
            openai_api_key: Option<String>,
        }
        if let Ok(camel) = serde_json::from_slice::<CamelWire>(bytes) {
            if let (Some(access), Some(refresh), Some(id)) =
                (camel.access_token, camel.refresh_token, camel.id_token)
            {
                if !access.is_empty() && !refresh.is_empty() {
                    return Ok(CodexCredentials::Full(CodexCredentialsFull {
                        access_token: access,
                        refresh_token: refresh,
                        id_token: id,
                        last_refresh: camel.last_refresh,
                        openai_api_key: camel.openai_api_key,
                    }));
                }
            }
        }
        // Last resort: API-key-only file.
        #[derive(Deserialize)]
        struct ApiKeyWire {
            #[serde(rename = "OPENAI_API_KEY")]
            openai_api_key: String,
        }
        if let Ok(apikey) = serde_json::from_slice::<ApiKeyWire>(bytes) {
            if !apikey.openai_api_key.is_empty() {
                return Ok(CodexCredentials::ApiKeyOnly(apikey.openai_api_key));
            }
        }
        Err(CredentialsParseError::Malformed)
    }

    /// Serialize back to the canonical snake_case form. ApiKeyOnly files
    /// round-trip as `{ "OPENAI_API_KEY": "..." }`.
    pub fn to_json(&self) -> Result<Vec<u8>, CredentialsParseError> {
        match self {
            CodexCredentials::Full(full) => serde_json::to_vec_pretty(full)
                .map_err(|e| CredentialsParseError::Encode(e.to_string())),
            CodexCredentials::ApiKeyOnly(key) => {
                let value = serde_json::json!({ "OPENAI_API_KEY": key });
                serde_json::to_vec_pretty(&value)
                    .map_err(|e| CredentialsParseError::Encode(e.to_string()))
            }
        }
    }

    pub fn as_full(&self) -> Option<&CodexCredentialsFull> {
        match self {
            CodexCredentials::Full(f) => Some(f),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CredentialsParseError {
    #[error("auth.json is malformed or missing required tokens")]
    Malformed,
    #[error("encode failed: {0}")]
    Encode(String),
}

/// Resolve the canonical Codex auth path. Respects `CODEX_HOME` when
/// set, otherwise falls back to `~/.codex/auth.json`.
pub fn auth_path() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("CODEX_HOME") {
        return Some(PathBuf::from(home).join("auth.json"));
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".codex").join("auth.json"))
}

/// Atomic write: stage to `auth.json.tmp.<nanos>`, rename over the
/// target. The intermediate file inherits the parent directory's ACL,
/// which on a normal user profile already restricts to owner only.
pub fn atomic_write(target: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = target.with_extension(format!("json.tmp.{nanos}"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAKE: &[u8] = br#"{
        "access_token": "at",
        "refresh_token": "rt",
        "id_token": "it",
        "last_refresh": "2026-05-12T00:00:00Z"
    }"#;

    const CAMEL: &[u8] = br#"{
        "accessToken": "at",
        "refreshToken": "rt",
        "idToken": "it"
    }"#;

    const APIKEY: &[u8] = br#"{ "OPENAI_API_KEY": "sk-abc" }"#;

    const LIVE_NESTED: &[u8] = br#"{
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": "eyJ.id.token",
            "access_token": "ek-cb1.access",
            "refresh_token": "ek-cb1.refresh",
            "account_id": "acct_01"
        },
        "last_refresh": "2026-05-13T00:00:00Z"
    }"#;

    #[test]
    fn parses_real_codex_cli_nested_shape() {
        let creds = CodexCredentials::parse(LIVE_NESTED).unwrap();
        let full = creds.as_full().unwrap();
        assert_eq!(full.access_token, "ek-cb1.access");
        assert_eq!(full.refresh_token, "ek-cb1.refresh");
        assert_eq!(full.id_token, "eyJ.id.token");
        assert_eq!(full.last_refresh.as_deref(), Some("2026-05-13T00:00:00Z"));
        assert!(full.openai_api_key.is_none());
    }

    #[test]
    fn parses_snake_case_payload() {
        let creds = CodexCredentials::parse(SNAKE).unwrap();
        let full = creds.as_full().unwrap();
        assert_eq!(full.access_token, "at");
        assert_eq!(full.refresh_token, "rt");
    }

    #[test]
    fn parses_camel_case_payload_as_full() {
        let creds = CodexCredentials::parse(CAMEL).unwrap();
        let full = creds.as_full().unwrap();
        assert_eq!(full.access_token, "at");
        assert_eq!(full.id_token, "it");
    }

    #[test]
    fn parses_api_key_only_file() {
        let creds = CodexCredentials::parse(APIKEY).unwrap();
        match creds {
            CodexCredentials::ApiKeyOnly(k) => assert_eq!(k, "sk-abc"),
            _ => panic!("expected ApiKeyOnly"),
        }
    }

    #[test]
    fn malformed_payload_returns_error() {
        let err = CodexCredentials::parse(b"not json").unwrap_err();
        assert!(matches!(err, CredentialsParseError::Malformed));
    }

    #[test]
    fn round_trip_emits_snake_case() {
        let creds = CodexCredentials::parse(CAMEL).unwrap();
        let bytes = creds.to_json().unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("\"access_token\""));
        assert!(!text.contains("\"accessToken\""));
        // Second write must be byte-identical for stability.
        let parsed2 = CodexCredentials::parse(text.as_bytes()).unwrap();
        let bytes2 = parsed2.to_json().unwrap();
        assert_eq!(text.as_bytes(), bytes2.as_slice());
    }

    #[test]
    fn auth_path_respects_codex_home_env() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("CODEX_HOME", dir.path());
        let resolved = auth_path().unwrap();
        assert!(resolved.ends_with("auth.json"));
        assert_eq!(resolved.parent(), Some(dir.path()));
        std::env::remove_var("CODEX_HOME");
    }

    #[test]
    fn atomic_write_replaces_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        std::fs::write(&path, b"old").unwrap();
        atomic_write(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }
}
