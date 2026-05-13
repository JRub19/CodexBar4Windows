//! Tauri commands exposing the secrets and cookies subsystems to React.
//!
//! Phase 2 wires the full surface. Phase 4 (Claude) and Phase 8
//! (Preferences UI) are the first real consumers.

use std::sync::Arc;

use codexbar::cookies::auto_import::{auto_import_and_save, AutoImportError};
use codexbar::cookies::{CookieImporter, CookieSource, ImportSuccess};
use codexbar::secrets::token_account::{TokenAccount, TokenAccountStore, TokenKind};
use serde::Serialize;
use tauri::State;
use tracing::info;

pub struct TokenAccountHandle(pub Arc<TokenAccountStore>);
pub struct CookieImporterHandle(pub Arc<CookieImporter>);

#[derive(Serialize)]
pub struct TokenAccountDto {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub created_at_unix_secs: u64,
}

impl From<&TokenAccount> for TokenAccountDto {
    fn from(a: &TokenAccount) -> Self {
        Self {
            id: a.id.clone(),
            kind: match a.kind {
                TokenKind::Cookie => "cookie",
                TokenKind::OauthToken => "oauth_token",
                TokenKind::ApiKey => "api_key",
            }
            .to_string(),
            label: a.label.clone(),
            created_at_unix_secs: a.created_at_unix_secs,
        }
    }
}

#[derive(Serialize)]
pub struct ListedAccounts {
    pub accounts: Vec<TokenAccountDto>,
    pub active_id: Option<String>,
}

#[tauri::command]
pub async fn list_token_accounts(
    provider_id: String,
    store: State<'_, TokenAccountHandle>,
) -> Result<ListedAccounts, String> {
    let list = store.0.load(&provider_id).map_err(|e| e.to_string())?;
    Ok(ListedAccounts {
        accounts: list.accounts.iter().map(TokenAccountDto::from).collect(),
        active_id: list.active_id,
    })
}

#[tauri::command]
pub async fn add_token_account(
    provider_id: String,
    kind: String,
    label: String,
    value: String,
    store: State<'_, TokenAccountHandle>,
) -> Result<TokenAccountDto, String> {
    let parsed_kind = parse_kind(&kind)?;
    let account = store
        .0
        .add(&provider_id, parsed_kind, label, value)
        .map_err(|e| e.to_string())?;
    info!(target: "codexbar::secrets_commands", provider = %provider_id, "token_account.added");
    Ok((&account).into())
}

#[tauri::command]
pub async fn edit_token_account(
    provider_id: String,
    account_id: String,
    label: Option<String>,
    value: Option<String>,
    store: State<'_, TokenAccountHandle>,
) -> Result<TokenAccountDto, String> {
    let account = store
        .0
        .edit(&provider_id, &account_id, label, value)
        .map_err(|e| e.to_string())?;
    info!(target: "codexbar::secrets_commands", provider = %provider_id, "token_account.edited");
    Ok((&account).into())
}

#[tauri::command]
pub async fn remove_token_account(
    provider_id: String,
    account_id: String,
    store: State<'_, TokenAccountHandle>,
) -> Result<(), String> {
    store
        .0
        .remove(&provider_id, &account_id)
        .map_err(|e| e.to_string())?;
    info!(target: "codexbar::secrets_commands", provider = %provider_id, "token_account.removed");
    Ok(())
}

#[tauri::command]
pub async fn set_active_token_account(
    provider_id: String,
    account_id: String,
    store: State<'_, TokenAccountHandle>,
) -> Result<(), String> {
    store
        .0
        .set_active(&provider_id, &account_id)
        .map_err(|e| e.to_string())?;
    info!(target: "codexbar::secrets_commands", provider = %provider_id, "token_account.active_set");
    Ok(())
}

#[tauri::command]
pub async fn set_manual_cookie(
    provider_id: String,
    raw: String,
    store: State<'_, TokenAccountHandle>,
) -> Result<TokenAccountDto, String> {
    let account = store
        .0
        .add(&provider_id, TokenKind::Cookie, "manual paste", raw)
        .map_err(|e| e.to_string())?;
    info!(target: "codexbar::secrets_commands", provider = %provider_id, "manual_cookie.added");
    Ok((&account).into())
}

#[derive(Serialize)]
pub struct ImportResultDto {
    pub provider_id: String,
    pub source: String,
    /// True when at least one cookie was imported. The header value itself
    /// is never returned over IPC; only the metadata.
    pub has_header: bool,
}

#[tauri::command]
pub async fn import_cookies_for(
    provider_id: String,
    domains: Vec<String>,
    allowed: Vec<String>,
    importer: State<'_, CookieImporterHandle>,
) -> Result<ImportResultDto, String> {
    let domain_refs: Vec<&str> = domains.iter().map(String::as_str).collect();
    let allowed_refs: Vec<&str> = allowed.iter().map(String::as_str).collect();
    let ImportSuccess { header, source } = importer
        .0
        .import_for(&provider_id, &domain_refs, &allowed_refs)
        .map_err(|e| e.to_string())?;
    Ok(ImportResultDto {
        provider_id,
        source: match source {
            CookieSource::Cache => "cache".to_string(),
            CookieSource::Manual => "manual".to_string(),
            CookieSource::Browser(b) => format!("browser:{}", b.as_str()),
        },
        has_header: !header.is_empty(),
    })
}

#[tauri::command]
pub async fn clear_cookie_cache(
    provider_id: String,
    importer: State<'_, CookieImporterHandle>,
) -> Result<(), String> {
    importer
        .0
        .cache
        .invalidate(&provider_id)
        .map_err(|e| e.to_string())?;
    info!(target: "codexbar::secrets_commands", provider = %provider_id, "cookie_cache.cleared");
    Ok(())
}

fn parse_kind(raw: &str) -> Result<TokenKind, String> {
    match raw {
        "cookie" => Ok(TokenKind::Cookie),
        "oauth_token" => Ok(TokenKind::OauthToken),
        "api_key" => Ok(TokenKind::ApiKey),
        other => Err(format!(
            "unknown token kind: {other} (expected cookie, oauth_token, api_key)"
        )),
    }
}

#[derive(Serialize)]
pub struct AutoImportOutcomeDto {
    pub provider_id: String,
    pub account_id: String,
    pub label: String,
    pub source: String,
}

/// Run the per-provider auto-import config (Cursor / Factory currently)
/// and persist the resulting `Cookie:` header into the DPAPI-wrapped
/// `TokenAccountStore` so the next refresh tick uses it. The returned
/// label distinguishes browser-sourced auto-imports from cache hits
/// so the popup can give the user useful provenance.
#[tauri::command]
pub async fn auto_import_cookies(
    provider_id: String,
    importer: State<'_, CookieImporterHandle>,
    tokens: State<'_, TokenAccountHandle>,
) -> Result<AutoImportOutcomeDto, String> {
    let importer = importer.0.clone();
    let store = tokens.0.clone();
    let provider_for_thread = provider_id.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        auto_import_and_save(&provider_for_thread, importer, store)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(map_auto_import_error)?;
    info!(
        target: "codexbar::secrets_commands",
        provider = %outcome.provider_id,
        label = %outcome.label,
        "cookie_auto_import.saved",
    );
    Ok(AutoImportOutcomeDto {
        provider_id: outcome.provider_id,
        account_id: outcome.account_id,
        label: outcome.label,
        source: match outcome.source {
            CookieSource::Cache => "cache".into(),
            CookieSource::Manual => "manual".into(),
            CookieSource::Browser(b) => format!("browser:{}", b.as_str()),
        },
    })
}

fn map_auto_import_error(err: AutoImportError) -> String {
    match err {
        AutoImportError::UnknownProvider(p) => {
            format!("No auto-import is configured for provider `{p}`.")
        }
        AutoImportError::Import(inner) => inner.to_string(),
        AutoImportError::Secrets(inner) => inner.to_string(),
    }
}
