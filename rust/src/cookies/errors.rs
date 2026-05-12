//! Cookie import error taxonomy.

use std::path::PathBuf;

use thiserror::Error;

use super::BrowserId;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("browser {0:?} is not installed at the expected paths")]
    BrowserNotInstalled(BrowserId),

    #[error("cookie database for {0:?} is locked; close the browser and try again")]
    DbLocked(BrowserId),

    #[error("v20 app-bound encryption detected for {host}; manual paste required")]
    V20OnlyForDomain { host: String },

    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("sqlite error: {0}")]
    Sqlite(String),

    #[error("local state json malformed: {0}")]
    LocalStateMalformed(String),

    #[error("decryption failed: {0}")]
    Decrypt(String),

    #[error("base64 decode failed")]
    Base64Decode,

    #[error("secrets error: {0}")]
    Secrets(#[from] crate::secrets::SecretsError),
}
