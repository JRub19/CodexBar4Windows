//! Thin Claude-specific wrapper around the shared `CookieHeaderCache`.
//!
//! The wrapper exists so the web strategy can read and invalidate the
//! claude entry without knowing about the cache key shape. Spec 40
//! section 3.1 names the entry `cookie.claude`.

use crate::cookies::header_cache::CookieHeaderCache;
use crate::secrets::SecretsError;

pub const CLAUDE_COOKIE_KEY: &str = "claude";

pub struct ClaudeCookieCache<'a> {
    inner: &'a CookieHeaderCache,
}

impl<'a> ClaudeCookieCache<'a> {
    pub fn new(inner: &'a CookieHeaderCache) -> Self {
        Self { inner }
    }

    pub fn read(&self) -> Result<Option<String>, SecretsError> {
        Ok(self
            .inner
            .read(CLAUDE_COOKIE_KEY)?
            .map(|cached| cached.header))
    }

    pub fn write(&self, header: &str, source: impl Into<String>) -> Result<(), SecretsError> {
        self.inner
            .write(CLAUDE_COOKIE_KEY, header, source)
            .map(|_| ())
    }

    pub fn invalidate(&self) -> Result<(), SecretsError> {
        self.inner.invalidate(CLAUDE_COOKIE_KEY)
    }
}
