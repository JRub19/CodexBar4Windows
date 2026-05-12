//! DPAPI-wrapped cache for the resolved Claude OAuth credentials.
//!
//! The cache lives next to other DPAPI blobs at
//! `%LOCALAPPDATA%\CodexBar4Windows\cache\claude-oauth.bin`. We fingerprint
//! the source file's `(mtime, size)` so changes to the on-disk Claude
//! Code credentials invalidate the cache instead of serving stale tokens.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::providers::claude::errors::CredentialError;
use crate::providers::claude::oauth::credentials::OAuthCredentials;
use crate::secrets::blob_store::{SecretBlobStore, SecretKey};

pub const CACHE_CATEGORY: &str = "oauth";
pub const CACHE_IDENTIFIER: &str = "claude-oauth";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedCredentials {
    pub credentials: OAuthCredentials,
    pub source_fingerprint: Option<Fingerprint>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Fingerprint {
    pub mtime_unix_secs: i64,
    pub size_bytes: u64,
}

pub fn fingerprint_for(path: &Path) -> Option<Fingerprint> {
    let metadata = std::fs::metadata(path).ok()?;
    let size_bytes = metadata.len();
    let mtime = metadata.modified().ok()?;
    let secs = mtime.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    Some(Fingerprint {
        mtime_unix_secs: secs,
        size_bytes,
    })
}

pub fn read(store: &dyn SecretBlobStore) -> Result<Option<CachedCredentials>, CredentialError> {
    let key = SecretKey::new(CACHE_CATEGORY, CACHE_IDENTIFIER);
    let bytes = store
        .read(&key)
        .map_err(|e| CredentialError::Cache(e.to_string()))?;
    match bytes {
        None => Ok(None),
        Some(b) => {
            let parsed: CachedCredentials = serde_json::from_slice(&b)
                .map_err(|e| CredentialError::DecodeFailed(e.to_string()))?;
            Ok(Some(parsed))
        }
    }
}

pub fn write(
    store: &dyn SecretBlobStore,
    cached: &CachedCredentials,
) -> Result<(), CredentialError> {
    let key = SecretKey::new(CACHE_CATEGORY, CACHE_IDENTIFIER);
    let bytes = serde_json::to_vec(cached).map_err(|e| CredentialError::Cache(e.to_string()))?;
    store
        .write(&key, &bytes)
        .map_err(|e| CredentialError::Cache(e.to_string()))
}

pub fn delete(store: &dyn SecretBlobStore) -> Result<(), CredentialError> {
    let key = SecretKey::new(CACHE_CATEGORY, CACHE_IDENTIFIER);
    store
        .delete(&key)
        .map_err(|e| CredentialError::Cache(e.to_string()))
}

/// Returns true when the cached fingerprint differs from the live file
/// fingerprint, indicating the file changed since we cached.
pub fn is_stale(cached: &CachedCredentials, file_path: Option<&Path>) -> bool {
    let Some(path) = file_path else {
        return false;
    };
    let live = fingerprint_for(path);
    match (&cached.source_fingerprint, live) {
        (Some(a), Some(b)) => a != &b,
        (Some(_), None) => true, // file was deleted under us
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::claude::oauth::credentials::OAuthCredentials;
    use crate::secrets::errors::SecretsError;
    use std::sync::Mutex;

    /// In-memory `SecretBlobStore` for tests. The real store uses DPAPI
    /// and lives at `%LOCALAPPDATA%`; we want hermetic tests instead.
    #[derive(Default)]
    struct FakeStore {
        inner: Mutex<std::collections::HashMap<(String, String), Vec<u8>>>,
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

    fn sample_creds() -> OAuthCredentials {
        OAuthCredentials {
            access_token: "tok".into(),
            refresh_token: Some("ref".into()),
            expires_at_unix_secs: Some(1),
            scopes: vec!["user:profile".into()],
        }
    }

    #[test]
    fn round_trips_through_dpapi_fake_store() {
        let store = FakeStore::default();
        let original = CachedCredentials {
            credentials: sample_creds(),
            source_fingerprint: Some(Fingerprint {
                mtime_unix_secs: 100,
                size_bytes: 200,
            }),
        };
        write(&store, &original).unwrap();
        let back = read(&store).unwrap().unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn missing_cache_returns_none() {
        let store = FakeStore::default();
        assert!(read(&store).unwrap().is_none());
    }

    #[test]
    fn is_stale_returns_true_when_fingerprint_differs() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"new content").unwrap();
        let cached = CachedCredentials {
            credentials: sample_creds(),
            source_fingerprint: Some(Fingerprint {
                mtime_unix_secs: 0,
                size_bytes: 99,
            }),
        };
        assert!(is_stale(&cached, Some(f.path())));
    }
}
