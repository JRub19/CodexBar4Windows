//! Resolve Claude OAuth credentials from the canonical sources, in
//! order:
//!
//! 1. `CODEXBAR_CLAUDE_OAUTH_TOKEN` env var.
//! 2. DPAPI-wrapped cache at
//!    `%LOCALAPPDATA%\CodexBar4Windows\cache\claude-oauth.bin`.
//! 3. Claude Code's own `.credentials.json` under `%USERPROFILE%\.claude`.
//!
//! The strategy layer (P4-12) consumes the resolved `OAuthCredentials`
//! struct. P4-11 is filesystem only; there is no network code here.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::providers::claude::errors::CredentialError;

pub const ENV_TOKEN: &str = "CODEXBAR_CLAUDE_OAUTH_TOKEN";
pub const REQUIRED_SCOPE: &str = "user:profile";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthCredentials {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at_unix_secs: Option<i64>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl OAuthCredentials {
    pub fn has_required_scope(&self) -> bool {
        self.scopes.is_empty() || self.scopes.iter().any(|s| s == REQUIRED_SCOPE)
    }
}

/// Result of one resolution attempt. The caller logs which source won
/// for the debug source pill in Settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialSource {
    Environment,
    DpapiCache,
    ClaudeCodeFile,
}

#[derive(Clone, Debug)]
pub struct ResolvedCredentials {
    pub credentials: OAuthCredentials,
    pub source: CredentialSource,
}

/// Parse the on-disk `~/.claude/.credentials.json` file.
pub fn parse_file(bytes: &[u8]) -> Result<OAuthCredentials, CredentialError> {
    #[derive(Deserialize)]
    struct Wire {
        #[serde(rename = "accessToken")]
        access_token: Option<String>,
        #[serde(rename = "refreshToken")]
        refresh_token: Option<String>,
        #[serde(rename = "expiresAt")]
        expires_at_unix_secs: Option<i64>,
        #[serde(default)]
        scopes: Vec<String>,
    }
    let parsed: Wire =
        serde_json::from_slice(bytes).map_err(|e| CredentialError::DecodeFailed(e.to_string()))?;
    let access_token = parsed
        .access_token
        .ok_or_else(|| CredentialError::DecodeFailed("missing accessToken".into()))?;
    Ok(OAuthCredentials {
        access_token,
        refresh_token: parsed.refresh_token,
        expires_at_unix_secs: parsed.expires_at_unix_secs,
        scopes: parsed.scopes,
    })
}

/// Read a single credential bundle from disk and parse. Returns `None`
/// when the file is missing; other IO errors propagate.
pub fn read_from_file(path: &Path) -> Result<Option<OAuthCredentials>, CredentialError> {
    match std::fs::read(path) {
        Ok(bytes) => parse_file(&bytes).map(Some),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CredentialError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Resolve credentials from the canonical chain. The caller passes in
/// abstract readers so the function is testable without touching the
/// real filesystem or env.
pub fn resolve(
    env_value: Option<String>,
    dpapi_cache_value: Option<OAuthCredentials>,
    file_path: Option<&Path>,
) -> Result<ResolvedCredentials, CredentialError> {
    if let Some(token) = env_value {
        let creds = OAuthCredentials {
            access_token: token,
            refresh_token: None,
            expires_at_unix_secs: None,
            scopes: Vec::new(),
        };
        return Ok(ResolvedCredentials {
            credentials: creds,
            source: CredentialSource::Environment,
        });
    }
    if let Some(creds) = dpapi_cache_value {
        if !creds.has_required_scope() {
            return Err(CredentialError::MissingScope(REQUIRED_SCOPE));
        }
        return Ok(ResolvedCredentials {
            credentials: creds,
            source: CredentialSource::DpapiCache,
        });
    }
    if let Some(path) = file_path {
        if let Some(creds) = read_from_file(path)? {
            if !creds.has_required_scope() {
                return Err(CredentialError::MissingScope(REQUIRED_SCOPE));
            }
            return Ok(ResolvedCredentials {
                credentials: creds,
                source: CredentialSource::ClaudeCodeFile,
            });
        }
    }
    Err(CredentialError::Missing)
}

/// Default file location under `%USERPROFILE%\.claude\.credentials.json`.
pub fn default_file_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(
        PathBuf::from(home)
            .join(".claude")
            .join(".credentials.json"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tmp(json: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(json.as_bytes()).unwrap();
        f
    }

    #[test]
    fn parses_valid_credentials_file() {
        let json = r#"{
            "accessToken": "sk-ant-oat01-abc",
            "refreshToken": "ref",
            "expiresAt": 1700000000,
            "scopes": ["user:profile", "user:inference"]
        }"#;
        let creds = parse_file(json.as_bytes()).unwrap();
        assert_eq!(creds.access_token, "sk-ant-oat01-abc");
        assert_eq!(creds.refresh_token.as_deref(), Some("ref"));
        assert_eq!(creds.expires_at_unix_secs, Some(1700000000));
        assert!(creds.has_required_scope());
    }

    #[test]
    fn malformed_json_returns_decode_failed() {
        let err = parse_file(b"not json").unwrap_err();
        assert!(matches!(err, CredentialError::DecodeFailed(_)));
    }

    #[test]
    fn missing_access_token_returns_decode_failed() {
        let err = parse_file(br#"{"refreshToken": "x"}"#).unwrap_err();
        assert!(matches!(err, CredentialError::DecodeFailed(_)));
    }

    #[test]
    fn env_wins_over_other_sources() {
        let cache = Some(OAuthCredentials {
            access_token: "cache".into(),
            refresh_token: None,
            expires_at_unix_secs: None,
            scopes: vec!["user:profile".into()],
        });
        let resolved = resolve(Some("env-token".into()), cache, None).unwrap();
        assert_eq!(resolved.credentials.access_token, "env-token");
        assert_eq!(resolved.source, CredentialSource::Environment);
    }

    #[test]
    fn missing_scope_raises_error() {
        let cache = Some(OAuthCredentials {
            access_token: "x".into(),
            refresh_token: None,
            expires_at_unix_secs: None,
            scopes: vec!["other:scope".into()],
        });
        let err = resolve(None, cache, None).unwrap_err();
        assert!(matches!(err, CredentialError::MissingScope("user:profile")));
    }

    #[test]
    fn file_fallback_used_when_other_sources_empty() {
        let f = write_tmp(r#"{"accessToken":"file","scopes":["user:profile"]}"#);
        let resolved = resolve(None, None, Some(f.path())).unwrap();
        assert_eq!(resolved.source, CredentialSource::ClaudeCodeFile);
        assert_eq!(resolved.credentials.access_token, "file");
    }

    #[test]
    fn empty_chain_returns_missing() {
        let err = resolve(None, None, None).unwrap_err();
        assert!(matches!(err, CredentialError::Missing));
    }
}
