//! Unified error type for the secrets subsystem.
//!
//! Variants intentionally do not embed the raw bytes or pre redaction
//! strings of any secret. The audit gate in phase 2.19 grep checks this
//! file for `expose_secret` or `String` payloads pointing at sensitive
//! data; do not add either without a written justification.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretsError {
    #[error("dpapi call failed with code {code}")]
    Dpapi { code: i32 },

    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("serialize secret bag: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("base64 decode failed for blob")]
    Base64Decode,

    #[error("blob is not in the expected dpapi:v1 envelope")]
    BadEnvelope,

    #[error("credential manager call failed: {0}")]
    CredentialManager(String),

    #[error("identifier was empty or contained only invalid characters")]
    EmptyIdentifier,
}
