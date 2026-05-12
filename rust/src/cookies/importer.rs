//! High level orchestrator that turns a provider id plus a set of domains
//! into a `Cookie:` header.
//!
//! Lookup order:
//!
//! 1. Persistent header cache (if entry is present and not stale).
//! 2. Manual paste from `TokenAccountStore` (active account, kind Cookie).
//! 3. Live browser import: try Chrome, then Edge, then Brave, then
//!    Firefox; skip browsers gated by the [`CookieAccessGate`]; on v20,
//!    mark the (provider, browser) gated and continue with the next.
//!
//! On success: write the result to the header cache. On failure: surface
//! the most informative error (V20 if any browser saw it; else
//! `BrowserNotInstalled` if none of the four had a path; else the last
//! sqlite or decrypt error).

use std::sync::Arc;

use super::{
    chromium::ChromiumCookieReader, detect::BrowserDetection, errors::ImportError,
    firefox::FirefoxCookieReader, header_cache::CookieHeaderCache,
    normalizer::CookieHeaderNormalizer, BrowserCookieImporter, BrowserId, HttpCookie,
};
use crate::secrets::token_account::{TokenAccountStore, TokenKind};

pub struct CookieImporter {
    pub cache: Arc<CookieHeaderCache>,
    pub gate: Arc<super::gate::CookieAccessGate>,
    pub tokens: Arc<TokenAccountStore>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieSource {
    Cache,
    Manual,
    Browser(BrowserId),
}

pub struct ImportSuccess {
    pub header: String,
    pub source: CookieSource,
}

impl CookieImporter {
    pub fn new(
        cache: Arc<CookieHeaderCache>,
        gate: Arc<super::gate::CookieAccessGate>,
        tokens: Arc<TokenAccountStore>,
    ) -> Self {
        Self {
            cache,
            gate,
            tokens,
        }
    }

    /// Import a `Cookie:` header for `provider_id` against `domains`,
    /// preferring cached values, then manual paste, then live browsers.
    pub fn import_for(
        &self,
        provider_id: &str,
        domains: &[&str],
        allowed_names: &[&str],
    ) -> Result<ImportSuccess, ImportError> {
        // Step 1: cache.
        if let Some(cached) = self.cache.read(provider_id)? {
            if !self.cache.is_stale(&cached) {
                return Ok(ImportSuccess {
                    header: cached.header,
                    source: CookieSource::Cache,
                });
            }
        }

        // Step 2: manual paste.
        if let Some(account) = self.tokens.active_for(provider_id)? {
            if account.kind == TokenKind::Cookie {
                let header = CookieHeaderNormalizer::filtered_header(&account.value, allowed_names);
                if !header.is_empty() {
                    self.cache.write(provider_id, &header, "manual:paste")?;
                    return Ok(ImportSuccess {
                        header,
                        source: CookieSource::Manual,
                    });
                }
            }
        }

        // Step 3: live browsers, in order of likelihood.
        let mut last_error: Option<ImportError> = None;
        let mut saw_v20 = false;

        for browser in [
            BrowserId::Chrome,
            BrowserId::Edge,
            BrowserId::Brave,
            BrowserId::Firefox,
        ] {
            if !self.gate.is_open(provider_id, browser) {
                continue;
            }
            let presence = BrowserDetection::probe(browser);
            if !presence.is_installed() {
                continue;
            }
            let result = match browser {
                BrowserId::Firefox => {
                    FirefoxCookieReader::new(presence.clone()).import_for(domains)
                }
                _ => ChromiumCookieReader::new(presence.clone()).import_for(domains),
            };
            match result {
                Ok(cookies) if !cookies.is_empty() => {
                    let header = format_cookies(&cookies, allowed_names);
                    if header.is_empty() {
                        continue;
                    }
                    let source_label = format!("browser:{}", browser.as_str());
                    self.cache.write(provider_id, &header, source_label)?;
                    return Ok(ImportSuccess {
                        header,
                        source: CookieSource::Browser(browser),
                    });
                }
                Ok(_) => {}
                Err(ImportError::V20OnlyForDomain { host }) => {
                    saw_v20 = true;
                    self.gate.mark_failure(provider_id, browser);
                    last_error = Some(ImportError::V20OnlyForDomain { host });
                }
                Err(other) => {
                    last_error = Some(other);
                }
            }
        }

        if saw_v20 {
            return Err(last_error.unwrap_or(ImportError::V20OnlyForDomain {
                host: domains.first().map(|s| s.to_string()).unwrap_or_default(),
            }));
        }
        Err(last_error.unwrap_or(ImportError::BrowserNotInstalled(BrowserId::Chrome)))
    }
}

fn format_cookies(cookies: &[HttpCookie], allowed: &[&str]) -> String {
    cookies
        .iter()
        .filter(|c| allowed.is_empty() || allowed.iter().any(|a| a.eq_ignore_ascii_case(&c.name)))
        .map(|c| format!("{}={}", c.name, c.value))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::cookies::gate::CookieAccessGate;

    #[test]
    fn manual_paste_takes_precedence_over_browsers_when_cache_is_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = Arc::new(CookieHeaderCache::new(tmp.path().join("cache")));
        let gate = Arc::new(CookieAccessGate::new());
        let tokens = Arc::new(TokenAccountStore::new(tmp.path().join("tokens")));

        tokens
            .add(
                "claude",
                TokenKind::Cookie,
                "personal",
                "sessionKey=sk-ant-abc; ignored=true",
            )
            .expect("add");

        let importer = CookieImporter::new(cache.clone(), gate, tokens);
        let result = importer
            .import_for("claude", &["claude.ai"], &["sessionKey"])
            .expect("import");
        assert_eq!(result.source, CookieSource::Manual);
        assert_eq!(result.header, "sessionKey=sk-ant-abc");
        // Cache write happened.
        assert!(cache.read("claude").expect("read").is_some());
    }

    #[test]
    fn cache_short_circuits_subsequent_imports() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache = Arc::new(CookieHeaderCache::new(tmp.path().join("cache")));
        cache
            .write("claude", "sessionKey=sk-ant-x", "manual:paste")
            .expect("seed");
        let gate = Arc::new(CookieAccessGate::new());
        let tokens = Arc::new(TokenAccountStore::new(tmp.path().join("tokens")));
        let importer = CookieImporter::new(cache, gate, tokens);
        let result = importer
            .import_for("claude", &["claude.ai"], &["sessionKey"])
            .expect("import");
        assert_eq!(result.source, CookieSource::Cache);
    }
}
