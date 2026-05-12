//! Chromium cookie reader (Chrome, Edge, Brave) for the v10 encryption
//! scheme, with v20 (App-Bound Encryption) detection.
//!
//! Flow per spec/60-auth-cookies-secrets.md §6:
//!
//! 1. Read `Local State`, parse JSON, pull `os_crypt.encrypted_key`.
//! 2. Base64 decode, strip the 5 byte `"DPAPI"` prefix.
//! 3. `dpapi_unprotect` → 32 byte AES-256 key.
//! 4. Copy `Network/Cookies` (and `-wal`, `-shm` if present) to a temp
//!    dir; opening directly from the live profile path locks the file
//!    against the running browser.
//! 5. Open SQLite read only.
//! 6. For each row matching the requested domain, inspect
//!    `encrypted_value`:
//!    - empty → fall back to plaintext `value`
//!    - starts with `v10` → AES-256-GCM with the unwrapped key, nonce =
//!      bytes [3..15], ciphertext = bytes [15..], 16 byte tag at end.
//!      Chrome 116+ prepends a 32 byte SHA-256 prefix to the plaintext;
//!      we strip it if present.
//!    - starts with `v20` → surface `ImportError::V20OnlyForDomain`
//!      after the loop so the manual paste path can be offered.
//! 7. Translate `SQLITE_BUSY` / `SQLITE_LOCKED` into
//!    `ImportError::DbLocked`.

use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;

use super::detect::BrowserPresence;
use super::errors::ImportError;
use super::{BrowserCookieImporter, BrowserId, HttpCookie};
use crate::secrets;

const DPAPI_PREFIX: &[u8] = b"DPAPI";
const V10_PREFIX: &[u8] = b"v10";
const V20_PREFIX: &[u8] = b"v20";
const SHA256_PREFIX_LEN: usize = 32;

pub struct ChromiumCookieReader {
    presence: BrowserPresence,
}

impl ChromiumCookieReader {
    pub fn new(presence: BrowserPresence) -> Self {
        Self { presence }
    }

    fn require_paths(&self) -> Result<(&Path, &Path), ImportError> {
        let local_state = self
            .presence
            .local_state_path
            .as_deref()
            .ok_or(ImportError::BrowserNotInstalled(self.presence.browser))?;
        let cookies = self
            .presence
            .cookie_db_path
            .as_deref()
            .ok_or(ImportError::BrowserNotInstalled(self.presence.browser))?;
        Ok((local_state, cookies))
    }

    fn load_aes_key(&self, local_state: &Path) -> Result<Vec<u8>, ImportError> {
        let text = std::fs::read_to_string(local_state).map_err(|source| ImportError::Io {
            path: local_state.to_path_buf(),
            source,
        })?;
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| ImportError::LocalStateMalformed(e.to_string()))?;
        let b64 = json
            .get("os_crypt")
            .and_then(|c| c.get("encrypted_key"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ImportError::LocalStateMalformed("os_crypt.encrypted_key not found".to_string())
            })?;
        let raw = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| ImportError::Base64Decode)?;
        let payload = raw
            .strip_prefix(DPAPI_PREFIX)
            .ok_or_else(|| ImportError::LocalStateMalformed("DPAPI prefix missing".to_string()))?;
        let key = secrets::dpapi::dpapi_unprotect(payload).map_err(ImportError::Secrets)?;
        if key.len() != 32 {
            return Err(ImportError::LocalStateMalformed(format!(
                "decrypted key is {} bytes, expected 32",
                key.len()
            )));
        }
        Ok(key)
    }

    fn copy_cookies_db(&self, cookies: &Path) -> Result<(tempfile::TempDir, PathBuf), ImportError> {
        let dir = tempfile::tempdir().map_err(|source| ImportError::Io {
            path: cookies.to_path_buf(),
            source,
        })?;
        let dest = dir.path().join("Cookies");
        std::fs::copy(cookies, &dest).map_err(|source| match classify_copy_error(&source) {
            true => ImportError::DbLocked(self.presence.browser),
            false => ImportError::Io {
                path: cookies.to_path_buf(),
                source,
            },
        })?;
        // -wal and -shm are optional; ignore copy errors silently.
        for ext in ["-wal", "-shm"] {
            let from = cookies.with_file_name(format!(
                "{}{ext}",
                cookies.file_name().and_then(|n| n.to_str()).unwrap_or("")
            ));
            let to = dest.with_file_name(format!(
                "{}{ext}",
                dest.file_name().and_then(|n| n.to_str()).unwrap_or("")
            ));
            let _ = std::fs::copy(&from, &to);
        }
        Ok((dir, dest))
    }
}

impl BrowserCookieImporter for ChromiumCookieReader {
    fn browser(&self) -> BrowserId {
        self.presence.browser
    }

    fn import_for(&self, domains: &[&str]) -> Result<Vec<HttpCookie>, ImportError> {
        let (local_state, cookie_db) = self.require_paths()?;
        let key = self.load_aes_key(local_state)?;
        let (_temp, temp_db) = self.copy_cookies_db(cookie_db)?;

        let conn = rusqlite::Connection::open_with_flags(
            &temp_db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| match classify_sqlite_error(&e) {
            Some(err) => err,
            None => ImportError::Sqlite(e.to_string()),
        })?;

        let mut out = Vec::new();
        let mut v20_blocked: Vec<String> = Vec::new();

        for host_pattern in domains {
            let mut stmt = conn
                .prepare(
                    "SELECT host_key, name, value, encrypted_value, path, is_secure, is_httponly \
                     FROM cookies WHERE host_key LIKE ?1",
                )
                .map_err(|e| ImportError::Sqlite(e.to_string()))?;
            let pattern = if host_pattern.starts_with('.') || host_pattern.contains('%') {
                host_pattern.to_string()
            } else {
                format!("%{host_pattern}")
            };
            let rows = stmt
                .query_map([&pattern], |row| {
                    Ok(RawCookieRow {
                        host: row.get::<_, String>(0)?,
                        name: row.get::<_, String>(1)?,
                        plaintext_value: row.get::<_, String>(2)?,
                        encrypted: row.get::<_, Vec<u8>>(3).unwrap_or_default(),
                        path: row.get::<_, String>(4)?,
                        is_secure: row.get::<_, i64>(5)? != 0,
                        is_http_only: row.get::<_, i64>(6)? != 0,
                    })
                })
                .map_err(|e| ImportError::Sqlite(e.to_string()))?;
            for row in rows {
                let row = row.map_err(|e| ImportError::Sqlite(e.to_string()))?;
                match decrypt_row(&row, &key) {
                    DecryptOutcome::Plain(value) => out.push(HttpCookie {
                        host: row.host,
                        name: row.name,
                        value,
                        path: row.path,
                        is_secure: row.is_secure,
                        is_http_only: row.is_http_only,
                    }),
                    DecryptOutcome::V20 => v20_blocked.push(row.host.clone()),
                    DecryptOutcome::Failed(msg) => return Err(ImportError::Decrypt(msg)),
                }
            }
        }

        if !out.is_empty() {
            return Ok(out);
        }
        if let Some(host) = v20_blocked.first() {
            return Err(ImportError::V20OnlyForDomain { host: host.clone() });
        }
        Ok(out)
    }
}

struct RawCookieRow {
    host: String,
    name: String,
    plaintext_value: String,
    encrypted: Vec<u8>,
    path: String,
    is_secure: bool,
    is_http_only: bool,
}

enum DecryptOutcome {
    Plain(String),
    V20,
    Failed(String),
}

fn decrypt_row(row: &RawCookieRow, key: &[u8]) -> DecryptOutcome {
    if row.encrypted.is_empty() {
        return DecryptOutcome::Plain(row.plaintext_value.clone());
    }
    if row.encrypted.starts_with(V20_PREFIX) {
        return DecryptOutcome::V20;
    }
    if row.encrypted.starts_with(V10_PREFIX) {
        return decrypt_v10(&row.encrypted, key);
    }
    DecryptOutcome::Failed("unknown chromium cookie envelope".to_string())
}

fn decrypt_v10(blob: &[u8], key: &[u8]) -> DecryptOutcome {
    if blob.len() < V10_PREFIX.len() + 12 + 16 {
        return DecryptOutcome::Failed("v10 blob too short".to_string());
    }
    let nonce_bytes = &blob[V10_PREFIX.len()..V10_PREFIX.len() + 12];
    let ciphertext = &blob[V10_PREFIX.len() + 12..];
    let cipher = match Aes256Gcm::new_from_slice(key) {
        Ok(c) => c,
        Err(e) => return DecryptOutcome::Failed(format!("aes key error: {e}")),
    };
    let nonce = Nonce::from_slice(nonce_bytes);
    match cipher.decrypt(nonce, ciphertext) {
        Ok(mut plain) => {
            // Chrome 116+ prepends a 32 byte SHA-256 of the plaintext.
            // Detect by sniffing the first 32 bytes for printable
            // characters; if they are not ASCII printable, strip them.
            if plain.len() > SHA256_PREFIX_LEN && !is_likely_printable(&plain[..SHA256_PREFIX_LEN])
            {
                plain.drain(..SHA256_PREFIX_LEN);
            }
            DecryptOutcome::Plain(String::from_utf8_lossy(&plain).into_owned())
        }
        Err(e) => DecryptOutcome::Failed(format!("aes-gcm decrypt: {e}")),
    }
}

fn is_likely_printable(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| (0x20..=0x7e).contains(b))
}

fn classify_sqlite_error(err: &rusqlite::Error) -> Option<ImportError> {
    let msg = err.to_string().to_lowercase();
    if msg.contains("busy") || msg.contains("locked") {
        return Some(ImportError::DbLocked(BrowserId::Chrome));
    }
    None
}

fn classify_copy_error(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::PermissionDenied
            | std::io::ErrorKind::Other
            | std::io::ErrorKind::AlreadyExists
    )
}

pub(crate) const _V10_PREFIX_EXPORT: &[u8] = V10_PREFIX;
pub(crate) const _V20_PREFIX_EXPORT: &[u8] = V20_PREFIX;

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_row(encrypted: Vec<u8>, plain: &str) -> RawCookieRow {
        RawCookieRow {
            host: "example.com".into(),
            name: "session".into(),
            plaintext_value: plain.into(),
            encrypted,
            path: "/".into(),
            is_secure: true,
            is_http_only: true,
        }
    }

    #[test]
    fn empty_encrypted_value_falls_back_to_plaintext() {
        let row = raw_row(Vec::new(), "fallback-value");
        let key = [0u8; 32];
        match decrypt_row(&row, &key) {
            DecryptOutcome::Plain(v) => assert_eq!(v, "fallback-value"),
            _ => panic!("expected plain fallback"),
        }
    }

    #[test]
    fn v20_prefix_is_detected() {
        let mut blob = V20_PREFIX.to_vec();
        blob.extend_from_slice(b"opaque-bytes");
        let row = raw_row(blob, "");
        let key = [0u8; 32];
        match decrypt_row(&row, &key) {
            DecryptOutcome::V20 => {}
            _ => panic!("expected v20 detection"),
        }
    }

    #[test]
    fn unknown_envelope_returns_failed() {
        let row = raw_row(b"v99garbage".to_vec(), "");
        let key = [0u8; 32];
        match decrypt_row(&row, &key) {
            DecryptOutcome::Failed(_) => {}
            _ => panic!("expected failed"),
        }
    }

    #[test]
    fn v10_round_trip_with_known_key() {
        // Deterministic key and nonce so the test does not need OsRng.
        let key: [u8; 32] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let nonce_bytes: [u8; 12] = [
            0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        ];
        let cipher = Aes256Gcm::new_from_slice(&key).expect("aes key");
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plain = b"hello-cookie-value";
        let ct = cipher.encrypt(nonce, plain.as_ref()).expect("encrypt");
        let mut blob = V10_PREFIX.to_vec();
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ct);
        let row = raw_row(blob, "");
        match decrypt_row(&row, &key) {
            DecryptOutcome::Plain(v) => assert_eq!(v, "hello-cookie-value"),
            _ => panic!("expected plain decryption"),
        }
    }

    #[test]
    fn v10_blob_below_minimum_length_is_failed() {
        let row = raw_row(b"v10short".to_vec(), "");
        let key = [0u8; 32];
        match decrypt_row(&row, &key) {
            DecryptOutcome::Failed(msg) => assert!(msg.contains("short")),
            _ => panic!("expected failed"),
        }
    }
}
