//! File backed DPAPI wrapped secret blob store.
//!
//! Each `SecretKey` resolves to `<root>/<category>/<normalized_identifier>.bin`.
//! On `write` the bytes are DPAPI wrapped and written; on `read` the file is
//! read and DPAPI unwrapped. Cross account theft is therefore as hard as
//! exfiltrating the live DPAPI master key, which the Windows credential
//! infrastructure protects.

use std::path::PathBuf;

use super::dpapi;
use super::errors::SecretsError;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SecretKey {
    pub category: String,
    pub identifier: String,
}

impl SecretKey {
    pub fn new(category: impl Into<String>, identifier: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            identifier: identifier.into(),
        }
    }
}

/// Trait that every blob backed secret store implements. Phase 4 providers
/// take a `&dyn SecretBlobStore` so they can be tested against an in
/// memory fake.
pub trait SecretBlobStore: Send + Sync {
    fn read(&self, key: &SecretKey) -> Result<Option<Vec<u8>>, SecretsError>;
    fn write(&self, key: &SecretKey, value: &[u8]) -> Result<(), SecretsError>;
    fn delete(&self, key: &SecretKey) -> Result<(), SecretsError>;
}

pub struct FileSecretBlobStore {
    root: PathBuf,
}

impl FileSecretBlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path_for(&self, key: &SecretKey) -> Result<PathBuf, SecretsError> {
        let category = normalize_id_part(&key.category)?;
        let identifier = normalize_id_part(&key.identifier)?;
        Ok(self.root.join(&category).join(format!("{identifier}.bin")))
    }
}

impl SecretBlobStore for FileSecretBlobStore {
    fn read(&self, key: &SecretKey) -> Result<Option<Vec<u8>>, SecretsError> {
        let path = self.path_for(key)?;
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(SecretsError::Io { path, source }),
        };
        dpapi::dpapi_unprotect(&bytes).map(Some)
    }

    fn write(&self, key: &SecretKey, value: &[u8]) -> Result<(), SecretsError> {
        let wrapped = dpapi::dpapi_protect(value)?;
        let path = self.path_for(key)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| SecretsError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let tmp = path.with_extension("bin.tmp");
        std::fs::write(&tmp, &wrapped).map_err(|source| SecretsError::Io {
            path: tmp.clone(),
            source,
        })?;
        std::fs::rename(&tmp, &path).map_err(|source| SecretsError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(())
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretsError> {
        let path = self.path_for(key)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(SecretsError::Io { path, source }),
        }
    }
}

/// Normalize one identifier segment: lowercase, trim, replace any char
/// outside `[a-z0-9._-]` with `_`. Returns `EmptyIdentifier` if the result
/// is empty.
fn normalize_id_part(raw: &str) -> Result<String, SecretsError> {
    let mut s = String::with_capacity(raw.len());
    for ch in raw.trim().chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            s.push(ch);
        } else {
            s.push('_');
        }
    }
    if s.is_empty() {
        Err(SecretsError::EmptyIdentifier)
    } else {
        Ok(s)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn write_read_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileSecretBlobStore::new(dir.path());
        let key = SecretKey::new("oauth", "claude");
        store.write(&key, b"hello-world").expect("write");
        let back = store.read(&key).expect("read").expect("present");
        assert_eq!(back, b"hello-world");
    }

    #[test]
    fn read_missing_returns_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileSecretBlobStore::new(dir.path());
        let key = SecretKey::new("oauth", "absent");
        assert!(store.read(&key).expect("read").is_none());
    }

    #[test]
    fn delete_present_removes_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileSecretBlobStore::new(dir.path());
        let key = SecretKey::new("oauth", "claude");
        store.write(&key, b"x").expect("write");
        store.delete(&key).expect("delete");
        assert!(store.read(&key).expect("read").is_none());
    }

    #[test]
    fn identifier_normalization_replaces_invalid_chars() {
        assert_eq!(normalize_id_part(" Claude/Code ").unwrap(), "claude_code");
        assert_eq!(normalize_id_part("simple").unwrap(), "simple");
        assert_eq!(
            normalize_id_part("dot.dash-under_score").unwrap(),
            "dot.dash-under_score"
        );
        assert!(normalize_id_part("").is_err());
        assert!(normalize_id_part("   ").is_err());
    }
}
