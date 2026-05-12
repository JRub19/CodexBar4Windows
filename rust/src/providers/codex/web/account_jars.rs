//! Per-account cookie jar persistence.
//!
//! Each Codex account (identified by its lowercase email) gets its own
//! cookie jar on disk under `%LOCALAPPDATA%\CodexBar4Windows\jars`. The
//! filename is a SHA-256 prefix of the email masked into UUID v4 shape
//! so the on-disk name is constant-width but reveals nothing about the
//! account.
//!
//! The jar is a thin serializable wrapper around a list of cookies; we
//! avoid pulling in `reqwest::cookie::Jar` directly because its API is
//! `dyn CookieStore` heavy and we only need a value-typed container.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CookieJarSnapshot {
    pub cookies: Vec<JarCookie>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct JarCookie {
    pub host: String,
    pub name: String,
    pub value: String,
    #[serde(default = "default_path")]
    pub path: String,
    #[serde(default)]
    pub is_secure: bool,
    #[serde(default)]
    pub is_http_only: bool,
}

fn default_path() -> String {
    "/".to_string()
}

/// Resolve a stable UUID-shaped identifier from a normalized email. We
/// hash the lowercased, trimmed email and reshape the first 16 bytes
/// into a UUID v4 layout: `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`.
pub fn jar_id(email: &str) -> String {
    let normalized = email.trim().to_ascii_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // Set the UUID variant + version bits.
    bytes[6] = (bytes[6] & 0x0F) | 0x40; // version = 4
    bytes[8] = (bytes[8] & 0x3F) | 0x80; // variant = 10
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes(bytes[0..4].try_into().unwrap()),
        u16::from_be_bytes(bytes[4..6].try_into().unwrap()),
        u16::from_be_bytes(bytes[6..8].try_into().unwrap()),
        u16::from_be_bytes(bytes[8..10].try_into().unwrap()),
        u64::from_be_bytes({
            let mut padded = [0u8; 8];
            padded[2..].copy_from_slice(&bytes[10..16]);
            padded
        }) & 0xFFFF_FFFF_FFFF,
    )
}

pub struct JarStore {
    root: PathBuf,
}

impl JarStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path_for(&self, email: &str) -> PathBuf {
        self.root.join(format!("{}.json", jar_id(email)))
    }

    pub fn read(&self, email: &str) -> std::io::Result<Option<CookieJarSnapshot>> {
        let path = self.path_for(email);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(
                serde_json::from_slice(&bytes).map_err(std::io::Error::other)?,
            )),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(other) => Err(other),
        }
    }

    pub fn write(&self, email: &str, jar: &CookieJarSnapshot) -> std::io::Result<()> {
        if let Some(parent) = self.path_for(email).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(jar).map_err(std::io::Error::other)?;
        let path = self.path_for(email);
        atomic_write(&path, &bytes)
    }

    pub fn delete(&self, email: &str) -> std::io::Result<()> {
        let path = self.path_for(email);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(other) => Err(other),
        }
    }
}

fn atomic_write(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
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

    #[test]
    fn jar_id_is_stable_and_uuid_shaped() {
        let id1 = jar_id("user@example.com");
        let id2 = jar_id("  User@Example.COM  ");
        assert_eq!(id1, id2, "email normalization must collapse case + ws");
        // UUID v4 shape: 8-4-4-4-12 hex digits, version byte starts with 4, variant nibble in {8,9,a,b}.
        let parts: Vec<&str> = id1.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert!(parts[2].starts_with('4'));
        assert_eq!(parts[3].len(), 4);
        let variant = parts[3].chars().next().unwrap();
        assert!(matches!(variant, '8' | '9' | 'a' | 'b'));
        assert_eq!(parts[4].len(), 12);
    }

    #[test]
    fn jar_id_differs_for_different_emails() {
        assert_ne!(jar_id("a@x.com"), jar_id("b@x.com"));
    }

    #[test]
    fn write_then_read_round_trips_the_jar() {
        let dir = tempfile::tempdir().unwrap();
        let store = JarStore::new(dir.path());
        let snapshot = CookieJarSnapshot {
            cookies: vec![
                JarCookie {
                    host: "chatgpt.com".into(),
                    name: "__Secure-next-auth.session-token".into(),
                    value: "xyz".into(),
                    path: "/".into(),
                    is_secure: true,
                    is_http_only: true,
                },
                JarCookie {
                    host: "openai.com".into(),
                    name: "_puid".into(),
                    value: "abc".into(),
                    path: "/".into(),
                    is_secure: true,
                    is_http_only: false,
                },
            ],
        };
        store.write("user@example.com", &snapshot).unwrap();
        let back = store.read("user@example.com").unwrap().unwrap();
        assert_eq!(back, snapshot);
    }

    #[test]
    fn read_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = JarStore::new(dir.path());
        assert!(store.read("nobody@example.com").unwrap().is_none());
    }

    #[test]
    fn delete_clears_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = JarStore::new(dir.path());
        store
            .write(
                "u@x.com",
                &CookieJarSnapshot {
                    cookies: vec![JarCookie {
                        host: "x.com".into(),
                        name: "k".into(),
                        value: "v".into(),
                        path: "/".into(),
                        is_secure: false,
                        is_http_only: false,
                    }],
                },
            )
            .unwrap();
        store.delete("u@x.com").unwrap();
        assert!(store.read("u@x.com").unwrap().is_none());
    }
}
