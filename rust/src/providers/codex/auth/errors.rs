//! Refresh and OAuth error taxonomy. The strategy layer folds these
//! into the framework's `ProviderFetchError`; the auth module needs the
//! finer-grained variants to decide which 401 codes are terminal.

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RefreshError {
    /// The refresh token itself is no longer accepted. Maps to
    /// `error.code == "refresh_token_expired"` or unknown 401 codes.
    #[error("refresh token expired; user must run `codex login` again")]
    Expired,
    /// The refresh token has already been spent; another client beat
    /// us to it. We surface this as `Reused` so the UI can hint that a
    /// second device is also signed in.
    #[error("refresh token reused")]
    Reused,
    /// Server explicitly revoked the token.
    #[error("refresh token revoked")]
    Revoked,
    /// Transport-level failure. Retryable.
    #[error("refresh network error: {0}")]
    Network(String),
    /// Server returned a payload we cannot parse.
    #[error("refresh response was malformed: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CodexOAuthError {
    #[error("auth.json not found")]
    CredentialsNotFound,
    #[error("auth.json present but missing required tokens")]
    CredentialsMissingTokens,
    #[error("usage endpoint returned 401")]
    Unauthorized,
    #[error("usage endpoint returned non-200 status {0}")]
    ServerError(u16),
    #[error("usage response decode failed: {0}")]
    DecodeFailed(String),
    #[error("usage response had no usable windows or credits")]
    InvalidResponse,
    #[error("network error talking to usage endpoint: {0}")]
    NetworkError(String),
    #[error("refresh failed: {0}")]
    RefreshExpired(String),
    #[error("refresh token revoked")]
    RefreshRevoked,
    #[error("refresh token reused by another client")]
    RefreshReused,
    #[error("refresh network error: {0}")]
    RefreshNetworkError(String),
    #[error("refresh response was malformed: {0}")]
    RefreshInvalidResponse(String),
}

impl From<RefreshError> for CodexOAuthError {
    fn from(value: RefreshError) -> Self {
        match value {
            RefreshError::Expired => CodexOAuthError::RefreshExpired("expired".into()),
            RefreshError::Reused => CodexOAuthError::RefreshReused,
            RefreshError::Revoked => CodexOAuthError::RefreshRevoked,
            RefreshError::Network(msg) => CodexOAuthError::RefreshNetworkError(msg),
            RefreshError::InvalidResponse(msg) => CodexOAuthError::RefreshInvalidResponse(msg),
        }
    }
}
