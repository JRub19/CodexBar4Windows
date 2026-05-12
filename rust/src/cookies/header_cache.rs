//! Persistent cache of normalized `Cookie:` headers per provider.
//!
//! Each entry stores the header value (sensitive) plus the time it was
//! written. Entries are DPAPI wrapped at rest via [`SecretBlobStore`].
//! A TTL controls re import frequency: providers consult the cache first,
//! then fall back to a live browser import.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::secrets::{
    blob_store::{FileSecretBlobStore, SecretBlobStore, SecretKey},
    SecretsError,
};

const CATEGORY: &str = "cookie-cache";
pub const DEFAULT_TTL: Duration = Duration::from_secs(12 * 3600);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedHeader {
    pub provider_id: String,
    pub header: String,
    pub written_unix_secs: u64,
    pub source: String,
}

pub struct CookieHeaderCache {
    blob_store: FileSecretBlobStore,
    ttl: Duration,
}

impl CookieHeaderCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            blob_store: FileSecretBlobStore::new(root),
            ttl: DEFAULT_TTL,
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub fn read(&self, provider_id: &str) -> Result<Option<CachedHeader>, SecretsError> {
        let key = SecretKey::new(CATEGORY, provider_id);
        let bytes = match self.blob_store.read(&key)? {
            Some(b) => b,
            None => return Ok(None),
        };
        let cached: CachedHeader = serde_json::from_slice(&bytes)?;
        Ok(Some(cached))
    }

    pub fn write(
        &self,
        provider_id: &str,
        header: &str,
        source: impl Into<String>,
    ) -> Result<CachedHeader, SecretsError> {
        let cached = CachedHeader {
            provider_id: provider_id.to_string(),
            header: header.to_string(),
            written_unix_secs: now_unix_secs(),
            source: source.into(),
        };
        let bytes = serde_json::to_vec(&cached)?;
        let key = SecretKey::new(CATEGORY, provider_id);
        self.blob_store.write(&key, &bytes)?;
        Ok(cached)
    }

    pub fn invalidate(&self, provider_id: &str) -> Result<(), SecretsError> {
        let key = SecretKey::new(CATEGORY, provider_id);
        self.blob_store.delete(&key)
    }

    /// True when the cached entry is missing or older than the TTL.
    pub fn is_stale(&self, cached: &CachedHeader) -> bool {
        let now = now_unix_secs();
        now.saturating_sub(cached.written_unix_secs) > self.ttl.as_secs()
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn write_read_round_trip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = CookieHeaderCache::new(tmp.path());
        let written = cache
            .write(
                "claude",
                "sessionKey=sk-ant-abc; user=alice",
                "browser:chrome",
            )
            .expect("write");
        let back = cache.read("claude").expect("read").expect("present");
        assert_eq!(back.header, written.header);
        assert_eq!(back.source, "browser:chrome");
        assert_eq!(back.provider_id, "claude");
    }

    #[test]
    fn read_missing_returns_none() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = CookieHeaderCache::new(tmp.path());
        assert!(cache.read("absent").expect("read").is_none());
    }

    #[test]
    fn invalidate_removes_entry() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = CookieHeaderCache::new(tmp.path());
        cache
            .write("claude", "session=abc", "browser:edge")
            .expect("write");
        cache.invalidate("claude").expect("invalidate");
        assert!(cache.read("claude").expect("read").is_none());
    }

    #[test]
    fn stale_check_uses_ttl() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = CookieHeaderCache::new(tmp.path()).with_ttl(Duration::from_secs(60));
        let fresh = CachedHeader {
            provider_id: "x".into(),
            header: "y=1".into(),
            written_unix_secs: now_unix_secs(),
            source: "test".to_string(),
        };
        assert!(!cache.is_stale(&fresh));
        let old = CachedHeader {
            provider_id: "x".into(),
            header: "y=1".into(),
            written_unix_secs: now_unix_secs() - 3600,
            source: "test".to_string(),
        };
        assert!(cache.is_stale(&old));
    }
}
