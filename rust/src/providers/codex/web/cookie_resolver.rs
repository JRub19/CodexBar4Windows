//! `CookieResolver` adapter that wraps the shared `CookieImporter` for
//! the Codex provider. Spec 41 §3.7 lists the cookie names we need; we
//! pass them through `allowed_names` so the importer drops anything
//! else.

use std::sync::Arc;

use async_trait::async_trait;

use crate::cookies::CookieImporter;
use crate::providers::claude::web::strategy::CookieResolver;
use crate::providers::errors::ProviderFetchError;

pub const CODEX_PROVIDER_ID: &str = "codex";

pub const CODEX_COOKIE_DOMAINS: &[&str] =
    &["chatgpt.com", ".chatgpt.com", "openai.com", ".openai.com"];

pub const CODEX_COOKIE_NAMES: &[&str] = &[
    "__Secure-next-auth.session-token",
    "__Secure-next-auth.session-token.0",
    "__Secure-next-auth.session-token.1",
    "__Host-next-auth.csrf-token",
    "_account",
    "_puid",
    "oai-did",
    "oai-sc",
    "intercom-device-id-dgkjq2bp",
];

pub struct CodexCookieResolver {
    importer: Arc<CookieImporter>,
}

impl CodexCookieResolver {
    pub fn new(importer: Arc<CookieImporter>) -> Self {
        Self { importer }
    }
}

#[async_trait]
impl CookieResolver for CodexCookieResolver {
    async fn cookie(&self) -> Result<Option<String>, ProviderFetchError> {
        let importer = self.importer.clone();
        let result = tokio::task::spawn_blocking(move || {
            importer.import_for(CODEX_PROVIDER_ID, CODEX_COOKIE_DOMAINS, CODEX_COOKIE_NAMES)
        })
        .await
        .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        match result {
            Ok(success) => Ok(Some(success.header)),
            Err(crate::cookies::ImportError::BrowserNotInstalled(_)) => Ok(None),
            Err(crate::cookies::ImportError::DbLocked(browser)) => {
                Err(ProviderFetchError::PluginUnavailable(format!(
                    "{browser:?} is running and locks its cookie database; \
                     close it or paste your session cookie in Settings",
                )))
            }
            Err(crate::cookies::ImportError::V20OnlyForDomain { host }) => {
                Err(ProviderFetchError::PluginUnavailable(format!(
                    "{host} cookies are protected by Chromium App-Bound Encryption (v20). \
                     Paste your `__Secure-next-auth.session-token` value in Settings.",
                )))
            }
            Err(other) => Err(ProviderFetchError::Network(other.to_string())),
        }
    }

    async fn invalidate(&self) -> Result<(), ProviderFetchError> {
        let importer = self.importer.clone();
        tokio::task::spawn_blocking(move || importer.cache.invalidate(CODEX_PROVIDER_ID))
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?
            .map_err(|e| ProviderFetchError::Network(e.to_string()))
    }
}
