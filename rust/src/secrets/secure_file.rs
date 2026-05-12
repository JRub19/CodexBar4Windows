//! Generic atomic JSON file storage.
//!
//! `SecureFile<T>` holds a path and gives you three operations:
//!
//! - `save(&value)` serializes `T` to pretty JSON, writes to `<path>.tmp`,
//!   then renames over `<path>`. Survives a power loss mid write.
//! - `load()` returns `Ok(None)` when the file is missing, `Ok(Some(T))`
//!   on a successful parse, and `Err` on a parse failure.
//! - `delete()` removes the file if present; returns `Ok(())` when not.
//!
//! The file itself is plain JSON. Encryption is the caller's responsibility:
//! wrap any sensitive *fields* of `T` with `dpapi::wrap_string` and unwrap
//! on read. This decoupling lets non Windows CI exercise the round trip
//! without DPAPI being available.

use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use super::errors::SecretsError;

pub struct SecureFile<T> {
    path: PathBuf,
    _phantom: PhantomData<T>,
}

impl<T: Serialize + DeserializeOwned> SecureFile<T> {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            _phantom: PhantomData,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn save(&self, value: &T) -> Result<(), SecretsError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| SecretsError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(value)?;
        std::fs::write(&tmp, &bytes).map_err(|source| SecretsError::Io {
            path: tmp.clone(),
            source,
        })?;
        std::fs::rename(&tmp, &self.path).map_err(|source| SecretsError::Io {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }

    pub fn load(&self) -> Result<Option<T>, SecretsError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(SecretsError::Io {
                    path: self.path.clone(),
                    source,
                })
            }
        };
        let value: T = serde_json::from_str(&text)?;
        Ok(Some(value))
    }

    pub fn delete(&self) -> Result<(), SecretsError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(SecretsError::Io {
                path: self.path.clone(),
                source,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Bag {
        name: String,
        sensitive_envelope: String,
    }

    #[test]
    fn save_load_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = SecureFile::<Bag>::new(dir.path().join("bag.json"));
        let bag = Bag {
            name: "alice".into(),
            sensitive_envelope: "dpapi:v1:abcdef".into(),
        };
        file.save(&bag).expect("save");
        let back = file.load().expect("load").expect("present");
        assert_eq!(back, bag);
    }

    #[test]
    fn load_missing_returns_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = SecureFile::<Bag>::new(dir.path().join("missing.json"));
        assert!(file.load().expect("load").is_none());
    }

    #[test]
    fn delete_missing_returns_ok() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = SecureFile::<Bag>::new(dir.path().join("missing.json"));
        file.delete().expect("delete missing");
    }

    #[test]
    fn delete_present_removes_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = SecureFile::<Bag>::new(dir.path().join("bag.json"));
        let bag = Bag {
            name: "alice".into(),
            sensitive_envelope: "dpapi:v1:abc".into(),
        };
        file.save(&bag).expect("save");
        file.delete().expect("delete");
        assert!(file.load().expect("load missing").is_none());
    }

    #[test]
    fn load_returns_err_on_corrupt_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("corrupt.json");
        std::fs::write(&path, b"{ this is not json").unwrap();
        let file = SecureFile::<Bag>::new(&path);
        assert!(file.load().is_err());
    }
}
