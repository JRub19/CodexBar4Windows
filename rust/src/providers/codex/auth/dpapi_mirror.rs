//! DPAPI-wrapped sidecar for `~/.codex/auth.json`. We mirror the live
//! file into `%APPDATA%\CodexBar4Windows\secrets\codex.json` so that:
//!
//! 1. If the user uninstalls the Codex CLI, our app still has a copy of
//!    the OAuth tokens and can seed a fresh `auth.json` after a one-time
//!    user confirmation.
//! 2. A Mac `credentials.json` byte stream is accepted unchanged by our
//!    reader, so users migrating across platforms can drop the file in.
//!
//! The mirror uses the shared `SecretBlobStore` from Phase 2 and adds
//! no provider-specific cryptography of its own.

use crate::providers::codex::auth::credentials::CodexCredentials;
use crate::providers::codex::auth::errors::RefreshError;
use crate::secrets::blob_store::{SecretBlobStore, SecretKey};

pub const SIDECAR_CATEGORY: &str = "codex";
pub const SIDECAR_IDENTIFIER: &str = "auth";

#[derive(Debug, thiserror::Error)]
pub enum MirrorError {
    #[error("dpapi mirror read failed: {0}")]
    Read(String),
    #[error("dpapi mirror write failed: {0}")]
    Write(String),
    #[error("auth.json could not be encoded: {0}")]
    Encode(String),
    #[error("auth.json could not be parsed: {0}")]
    Parse(String),
}

impl From<MirrorError> for RefreshError {
    fn from(value: MirrorError) -> Self {
        RefreshError::InvalidResponse(value.to_string())
    }
}

pub struct DpapiMirror<'a> {
    store: &'a dyn SecretBlobStore,
}

impl<'a> DpapiMirror<'a> {
    pub fn new(store: &'a dyn SecretBlobStore) -> Self {
        Self { store }
    }

    /// Read the mirrored credentials. Returns `None` when no mirror has
    /// been written yet.
    pub fn read(&self) -> Result<Option<CodexCredentials>, MirrorError> {
        let key = SecretKey::new(SIDECAR_CATEGORY, SIDECAR_IDENTIFIER);
        let bytes = self
            .store
            .read(&key)
            .map_err(|e| MirrorError::Read(e.to_string()))?;
        let Some(bytes) = bytes else {
            return Ok(None);
        };
        let creds =
            CodexCredentials::parse(&bytes).map_err(|e| MirrorError::Parse(e.to_string()))?;
        Ok(Some(creds))
    }

    /// Write the mirrored credentials. The on-disk shape is the same
    /// `auth.json` text the CLI writes; we only wrap it in DPAPI here.
    pub fn write(&self, creds: &CodexCredentials) -> Result<(), MirrorError> {
        let bytes = creds
            .to_json()
            .map_err(|e| MirrorError::Encode(e.to_string()))?;
        let key = SecretKey::new(SIDECAR_CATEGORY, SIDECAR_IDENTIFIER);
        self.store
            .write(&key, &bytes)
            .map_err(|e| MirrorError::Write(e.to_string()))
    }

    pub fn delete(&self) -> Result<(), MirrorError> {
        let key = SecretKey::new(SIDECAR_CATEGORY, SIDECAR_IDENTIFIER);
        self.store
            .delete(&key)
            .map_err(|e| MirrorError::Write(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::codex::auth::credentials::CodexCredentialsFull;
    use crate::secrets::errors::SecretsError;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeStore {
        inner: Mutex<HashMap<(String, String), Vec<u8>>>,
    }

    impl SecretBlobStore for FakeStore {
        fn read(&self, key: &SecretKey) -> Result<Option<Vec<u8>>, SecretsError> {
            Ok(self
                .inner
                .lock()
                .unwrap()
                .get(&(key.category.clone(), key.identifier.clone()))
                .cloned())
        }
        fn write(&self, key: &SecretKey, value: &[u8]) -> Result<(), SecretsError> {
            self.inner.lock().unwrap().insert(
                (key.category.clone(), key.identifier.clone()),
                value.to_vec(),
            );
            Ok(())
        }
        fn delete(&self, key: &SecretKey) -> Result<(), SecretsError> {
            self.inner
                .lock()
                .unwrap()
                .remove(&(key.category.clone(), key.identifier.clone()));
            Ok(())
        }
    }

    fn sample() -> CodexCredentials {
        CodexCredentials::Full(CodexCredentialsFull {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            id_token: "it".into(),
            last_refresh: None,
            openai_api_key: None,
        })
    }

    #[test]
    fn round_trips_credentials_through_the_mirror() {
        let store = FakeStore::default();
        let mirror = DpapiMirror::new(&store);
        mirror.write(&sample()).unwrap();
        let back = mirror.read().unwrap().unwrap();
        assert_eq!(back, sample());
    }

    #[test]
    fn read_with_no_mirror_returns_none() {
        let store = FakeStore::default();
        let mirror = DpapiMirror::new(&store);
        assert!(mirror.read().unwrap().is_none());
    }

    #[test]
    fn delete_clears_the_mirror() {
        let store = FakeStore::default();
        let mirror = DpapiMirror::new(&store);
        mirror.write(&sample()).unwrap();
        mirror.delete().unwrap();
        assert!(mirror.read().unwrap().is_none());
    }

    #[test]
    fn mac_credentials_drop_in_accepted_unchanged() {
        // Bytes produced by the macOS CLI: camelCase keys, no
        // last_refresh, no API key. The reader must accept this and the
        // mirror must round-trip it through DPAPI without mutating the
        // payload semantics.
        let mac_bytes = br#"{
            "accessToken": "at",
            "refreshToken": "rt",
            "idToken": "it"
        }"#;
        let creds = CodexCredentials::parse(mac_bytes).unwrap();
        let store = FakeStore::default();
        let mirror = DpapiMirror::new(&store);
        mirror.write(&creds).unwrap();
        let back = mirror.read().unwrap().unwrap();
        assert_eq!(back, creds);
    }
}
