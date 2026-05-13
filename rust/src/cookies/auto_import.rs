//! Per-provider cookie auto-import. Wraps `CookieImporter` with the
//! domain + cookie-name list that each provider needs, and persists
//! the resulting `Cookie:` header into the DPAPI-wrapped
//! `TokenAccountStore` so it survives restarts and shows up in the
//! Preferences UI.
//!
//! Lists are ported verbatim from the macOS Swift importer constants
//! (`CursorCookieImporter`, `FactoryCookieImporter`) so we stay
//! aligned with what the upstream app harvests.

use std::sync::Arc;

use super::importer::{CookieImporter, ImportSuccess};
use super::CookieSource;
use crate::secrets::errors::SecretsError;
use crate::secrets::token_account::{TokenAccountStore, TokenKind};

#[derive(Debug, Clone, Copy)]
pub struct AutoImportConfig {
    pub provider_id: &'static str,
    pub domains: &'static [&'static str],
    pub allowed_names: &'static [&'static str],
}

pub const CURSOR_CONFIG: AutoImportConfig = AutoImportConfig {
    provider_id: "cursor",
    domains: &[
        "cursor.com",
        "www.cursor.com",
        "cursor.sh",
        "authenticator.cursor.sh",
    ],
    allowed_names: &[
        "WorkosCursorSessionToken",
        "__Secure-next-auth.session-token",
        "next-auth.session-token",
        "wos-session",
        "__Secure-wos-session",
        "authjs.session-token",
        "__Secure-authjs.session-token",
    ],
};

pub const FACTORY_CONFIG: AutoImportConfig = AutoImportConfig {
    provider_id: "factory",
    domains: &["app.factory.ai", "auth.factory.ai", "api.factory.ai"],
    allowed_names: &[
        "access-token",
        "authjs.session-token",
        "next-auth.session-token",
        "__Secure-authjs.session-token",
        "__Secure-next-auth.session-token",
        "session",
    ],
};

pub fn configs() -> &'static [AutoImportConfig] {
    &[CURSOR_CONFIG, FACTORY_CONFIG]
}

pub fn config_for(provider_id: &str) -> Option<AutoImportConfig> {
    configs().iter().copied().find(|c| c.provider_id == provider_id)
}

#[derive(Debug, thiserror::Error)]
pub enum AutoImportError {
    #[error("no auto-import config registered for provider `{0}`")]
    UnknownProvider(String),
    #[error("cookie import failed: {0}")]
    Import(#[from] super::errors::ImportError),
    #[error("token store error: {0}")]
    Secrets(#[from] SecretsError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutoImportOutcome {
    pub provider_id: String,
    pub account_id: String,
    pub label: String,
    pub source: CookieSource,
}

/// Run the cookie importer for `provider_id` and save the resulting
/// header as a new active `TokenAccount` of kind `Cookie`. If a
/// previous auto-imported account exists we replace its value rather
/// than appending a duplicate (matched by label prefix).
pub fn auto_import_and_save(
    provider_id: &str,
    importer: Arc<CookieImporter>,
    tokens: Arc<TokenAccountStore>,
) -> Result<AutoImportOutcome, AutoImportError> {
    let config = config_for(provider_id)
        .ok_or_else(|| AutoImportError::UnknownProvider(provider_id.to_string()))?;
    let ImportSuccess { header, source } =
        importer.import_for(config.provider_id, config.domains, config.allowed_names)?;
    let label = label_for(&source);

    // Reuse the existing auto-imported account when its label matches.
    // This keeps the account list tidy across repeated imports.
    let existing = tokens.load(config.provider_id)?;
    let existing_id = existing
        .accounts
        .iter()
        .find(|a| a.kind == TokenKind::Cookie && a.label.starts_with(AUTO_PREFIX))
        .map(|a| a.id.clone());

    let account = if let Some(id) = existing_id {
        tokens.edit(config.provider_id, &id, Some(label.clone()), Some(header))?
    } else {
        tokens.add(config.provider_id, TokenKind::Cookie, label.clone(), header)?
    };
    tokens.set_active(config.provider_id, &account.id)?;

    Ok(AutoImportOutcome {
        provider_id: config.provider_id.to_string(),
        account_id: account.id,
        label,
        source,
    })
}

const AUTO_PREFIX: &str = "Auto-imported";

fn label_for(source: &CookieSource) -> String {
    match source {
        CookieSource::Cache => format!("{AUTO_PREFIX} (cached)"),
        CookieSource::Manual => format!("{AUTO_PREFIX} (manual paste)"),
        CookieSource::Browser(b) => format!("{AUTO_PREFIX} from {}", b.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_config_lists_known_session_cookies() {
        let names = CURSOR_CONFIG.allowed_names;
        assert!(names.contains(&"WorkosCursorSessionToken"));
        assert!(names.contains(&"__Secure-next-auth.session-token"));
        assert!(CURSOR_CONFIG.domains.contains(&"cursor.com"));
    }

    #[test]
    fn factory_config_lists_workos_access_token() {
        let names = FACTORY_CONFIG.allowed_names;
        assert!(names.contains(&"access-token"));
        assert!(FACTORY_CONFIG.domains.contains(&"app.factory.ai"));
    }

    #[test]
    fn config_for_returns_none_for_unknown_provider() {
        assert!(config_for("nonexistent").is_none());
        assert!(config_for("cursor").is_some());
    }

    #[test]
    fn label_for_distinguishes_source() {
        use crate::cookies::BrowserId;
        assert_eq!(label_for(&CookieSource::Cache), "Auto-imported (cached)");
        assert_eq!(label_for(&CookieSource::Manual), "Auto-imported (manual paste)");
        assert_eq!(
            label_for(&CookieSource::Browser(BrowserId::Brave)),
            "Auto-imported from brave"
        );
    }
}
