//! Claude-specific error sub-types. The shared `ProviderFetchError`
//! enum carries the runtime-facing variants; this module adds detail
//! enums that the Claude paths fold into a `ProviderFetchError` at the
//! boundary so the surface area stays narrow.

use thiserror::Error;

/// Errors raised while resolving the OAuth credential bundle. Mapped to
/// `ProviderFetchError::NoToken`, `Unauthorized`, or `ParseError` by the
/// strategy layer per spec 40 section 2.3.
#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("no credentials available at any source")]
    Missing,
    #[error("failed to read credentials file at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("malformed credentials file: {0}")]
    DecodeFailed(String),
    #[error("oauth token missing required scope {0}; reauth in Claude Code")]
    MissingScope(&'static str),
    #[error("DPAPI cache error: {0}")]
    Cache(String),
}
