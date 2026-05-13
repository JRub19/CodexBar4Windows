//! CodexBar4Windows desktop Tauri shell.
//!
//! Phase 1 wires the path environment, file logging, settings store, usage
//! store, and the refresh loop. The tray icon plus native context menu from
//! phase 0 are updated to expose Pause/Resume refresh and Preferences entry
//! points. Phase 3 onward layers the popup window and dynamic icon on top.

pub mod commands;
#[cfg(feature = "dev")]
pub mod dev;
pub mod first_run;
pub mod login_commands;
pub mod perf;
pub mod secrets_commands;
pub mod tray_renderer;

use std::sync::Arc;

use codexbar::cookies::{CookieAccessGate, CookieHeaderCache, CookieImporter};
use codexbar::core::{PathEnvironment, RefreshLoop, UsageStore};
use codexbar::providers::claude::cli::pty_actor::{CliRunner, RecordedRunner};
use codexbar::providers::claude::oauth::credentials::{resolve, OAuthCredentials};
use codexbar::providers::claude::oauth::strategy::{CredentialsResolver, HttpClient, HttpResponse};
use codexbar::providers::claude::oauth::transport::ReqwestClient;
use codexbar::providers::claude::planner::ClaudeWiring;
use codexbar::providers::claude::web::strategy::{CookieResolver, WebClient, WebResponse};
use codexbar::providers::claude::web::transport::ReqwestWebClient;
use codexbar::providers::claude::ClaudeProvider;
use codexbar::providers::codex::cli::rpc_client::{
    RpcCallError as CodexRpcCallError, RpcTransport as CodexRpcTransport,
};
use codexbar::providers::codex::cli::strategy::TransportFactory as CodexTransportFactory;
use codexbar::providers::codex::planner::CodexWiring;
use codexbar::providers::codex::CodexProvider;
use codexbar::providers::copilot::oauth::strategy::{
    CopilotCredentials, CopilotCredentialsResolver, GithubHttp as CopilotGithubHttp,
    GithubResponse as CopilotGithubResponse,
};
use codexbar::providers::copilot::oauth::transport::ReqwestGithubClient;
use codexbar::providers::copilot::planner::CopilotWiring;
use codexbar::providers::copilot::CopilotProvider;
use codexbar::providers::cursor::planner::CursorWiring;
use codexbar::providers::cursor::CursorProvider;
use codexbar::providers::deepseek::api::strategy::{
    DeepSeekCredentialsResolver, DeepSeekHttp, DeepSeekResponse,
};
use codexbar::providers::deepseek::api::transport::ReqwestDeepSeekClient;
use codexbar::providers::deepseek::planner::DeepSeekWiring;
use codexbar::providers::deepseek::DeepSeekProvider;
use codexbar::providers::errors::ProviderFetchError;
use codexbar::providers::factory::api::strategy::{
    FactoryCredentials, FactoryCredentialsResolver, FactoryHttp, FactoryResponse,
};
use codexbar::providers::factory::api::transport::ReqwestFactoryClient;
use codexbar::providers::factory::planner::FactoryWiring;
use codexbar::providers::factory::FactoryProvider;
use codexbar::providers::gemini::oauth::client_locator::{
    locate as locate_gemini_client, OAuthClientCredentials as GeminiOAuthClient, OsEnv,
    OsFilesystem,
};
use codexbar::providers::gemini::oauth::credentials::{
    load_auth_type, load_credentials, GeminiAuthType,
};
use codexbar::providers::gemini::oauth::strategy::{
    ClientCredentialsProvider, GeminiCredentialsResolver, GeminiCredentialsState, GoogleHttp,
    GoogleResponse as GeminiGoogleResponse, HttpMethod as GeminiHttpMethod, RefreshHook,
};
use codexbar::providers::gemini::oauth::transport::ReqwestGoogleClient;
use codexbar::providers::gemini::planner::GeminiWiring;
use codexbar::providers::gemini::GeminiProvider;
use codexbar::providers::moonshot::api::strategy::{
    MoonshotCredentials, MoonshotCredentialsResolver, MoonshotHttp, MoonshotResponse,
};
use codexbar::providers::moonshot::api::transport::ReqwestMoonshotClient;
use codexbar::providers::moonshot::descriptor::MoonshotRegion;
use codexbar::providers::moonshot::planner::MoonshotWiring;
use codexbar::providers::moonshot::MoonshotProvider;
use codexbar::providers::openrouter::api::strategy::{
    OpenRouterCredentials, OpenRouterCredentialsResolver, OpenRouterHttp, OpenRouterResponse,
};
use codexbar::providers::openrouter::api::transport::ReqwestOpenRouterClient;
use codexbar::providers::openrouter::planner::OpenRouterWiring;
use codexbar::providers::openrouter::OpenRouterProvider;
use codexbar::providers::zai::api::strategy::{
    ZaiCredentials, ZaiCredentialsResolver, ZaiHttp, ZaiResponse,
};
use codexbar::providers::zai::api::transport::ReqwestZaiClient;
use codexbar::providers::zai::descriptor::ZaiRegion;
use codexbar::providers::zai::planner::ZaiWiring;
use codexbar::providers::zai::ZaiProvider;
use codexbar::providers::ProviderImplementation;
use codexbar::secrets::token_account::TokenAccountStore;
use codexbar::settings::SettingsHandle;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use tokio::runtime::Runtime;
use tracing::info;

use crate::commands::{FirstRunHandle, RefreshHandle, UsageHandle};
use crate::first_run::FirstRunStore;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You have been greeted from Rust.", name)
}

/// Resolver that walks the canonical credential chain on disk plus the
/// `CODEXBAR_CLAUDE_OAUTH_TOKEN` env var.
struct FilesystemCredentialsResolver;

#[async_trait::async_trait]
impl CredentialsResolver for FilesystemCredentialsResolver {
    async fn resolve(
        &self,
    ) -> Result<OAuthCredentials, codexbar::providers::claude::errors::CredentialError> {
        let env_value =
            std::env::var(codexbar::providers::claude::oauth::credentials::ENV_TOKEN).ok();
        let file_path = codexbar::providers::claude::oauth::credentials::default_file_path();
        let resolved = resolve(env_value, None, file_path.as_deref())?;
        Ok(resolved.credentials)
    }
}

/// Tries the DPAPI-wrapped token store first, then falls back to the
/// shared cookie cache. Lets a user paste a Cookie header in the
/// Preferences pane without losing the auto-imported browser cookie
/// pathway.
struct StoredCookieResolver {
    tokens: Arc<TokenAccountStore>,
    cache: Arc<CookieHeaderCache>,
    provider_id: &'static str,
}

impl StoredCookieResolver {
    fn new(
        tokens: Arc<TokenAccountStore>,
        cache: Arc<CookieHeaderCache>,
        provider_id: &'static str,
    ) -> Self {
        Self {
            tokens,
            cache,
            provider_id,
        }
    }
}

#[async_trait::async_trait]
impl CookieResolver for StoredCookieResolver {
    async fn cookie(&self) -> Result<Option<String>, ProviderFetchError> {
        let store = self.tokens.clone();
        let provider_id = self.provider_id;
        let stored = tokio::task::spawn_blocking(move || store.active_for(provider_id))
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        if let Some(account) = stored {
            let value = account.value.trim();
            if !value.is_empty() {
                return Ok(Some(value.to_string()));
            }
        }
        let cache = self.cache.clone();
        let result = tokio::task::spawn_blocking(move || cache.read(provider_id))
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        match result {
            Ok(Some(cached)) => Ok(Some(cached.header)),
            Ok(None) => Ok(None),
            Err(e) => Err(ProviderFetchError::Network(e.to_string())),
        }
    }

    async fn invalidate(&self) -> Result<(), ProviderFetchError> {
        let cache = self.cache.clone();
        let provider_id = self.provider_id;
        tokio::task::spawn_blocking(move || cache.invalidate(provider_id))
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?
            .map_err(|e| ProviderFetchError::Network(e.to_string()))
    }
}

/// Last-resort HTTP transports used when reqwest fails to build (eg. a
/// missing TLS root store). They always report `Network` so the runtime
/// falls back to the next strategy in the plan.
struct NullHttpClient;

#[async_trait::async_trait]
impl HttpClient for NullHttpClient {
    async fn get_json(&self, _: &str, _: &str) -> Result<HttpResponse, ProviderFetchError> {
        Err(ProviderFetchError::Network(
            "reqwest unavailable; OAuth disabled".into(),
        ))
    }
}

struct NullWebClient;

#[async_trait::async_trait]
impl WebClient for NullWebClient {
    async fn get_json(&self, _: &str, _: &str) -> Result<WebResponse, ProviderFetchError> {
        Err(ProviderFetchError::Network(
            "reqwest unavailable; Web disabled".into(),
        ))
    }
}

/// Rebroadcast `UsageStore` updates to the Tauri event bus. The popup
/// listens to `usage:updated` and re-fetches `provider_snapshots`.
async fn bridge_usage_events(
    mut rx: tokio::sync::broadcast::Receiver<codexbar::core::UsageEvent>,
    handle: Arc<parking_lot::Mutex<Option<tauri::AppHandle>>>,
) {
    loop {
        match rx.recv().await {
            Ok(codexbar::core::UsageEvent::Updated(update)) => {
                if let Some(app) = handle.lock().clone() {
                    let payload = serde_json::json!({
                        "provider": update.provider.as_str(),
                        "menu_rev": update.menu_rev,
                        "icon_rev": update.icon_rev,
                    });
                    let _ = app.emit("usage:updated", payload);
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                info!(
                    target: "codexbar::app",
                    skipped,
                    "usage event channel lagged",
                );
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// Placeholder Codex transport. Real ConPTY plumbing for the codex
/// binary lands in a follow-up; until then the strategy is wired but
/// reports `PluginUnavailable` so the framework falls through cleanly.
struct UnavailableCodexTransport;

#[async_trait::async_trait]
impl CodexRpcTransport for UnavailableCodexTransport {
    async fn send(&self, _: Vec<u8>) -> Result<(), CodexRpcCallError> {
        Err(CodexRpcCallError::Transport(
            "codex transport not yet wired".into(),
        ))
    }
    async fn recv(&self) -> Result<Vec<u8>, CodexRpcCallError> {
        Err(CodexRpcCallError::Closed)
    }
}

impl CodexTransportFactory for UnavailableCodexTransport {
    fn open(
        &self,
    ) -> Result<Arc<dyn CodexRpcTransport>, codexbar::providers::errors::ProviderFetchError> {
        Err(
            codexbar::providers::errors::ProviderFetchError::PluginUnavailable(
                "codex binary not configured".into(),
            ),
        )
    }
}

/// Last-resort Codex OAuth transport when reqwest cannot build.
struct NullCodexUsageHttp;

#[async_trait::async_trait]
impl codexbar::providers::codex::oauth::usage::UsageHttp for NullCodexUsageHttp {
    async fn get(
        &self,
        _: &str,
        _: &[(&str, &str)],
    ) -> Result<
        codexbar::providers::codex::oauth::usage::UsageResponse,
        codexbar::providers::codex::auth::errors::CodexOAuthError,
    > {
        Err(
            codexbar::providers::codex::auth::errors::CodexOAuthError::NetworkError(
                "reqwest unavailable; Codex OAuth disabled".into(),
            ),
        )
    }
}

/// Resolves Codex OAuth credentials from `~/.codex/auth.json` (or the
/// `CODEX_HOME` env override).
struct FilesystemCodexCredentials;

#[async_trait::async_trait]
impl codexbar::providers::codex::oauth::strategy::OAuthCredentialsResolver
    for FilesystemCodexCredentials
{
    async fn resolve(
        &self,
    ) -> Result<
        codexbar::providers::codex::auth::credentials::CodexCredentials,
        codexbar::providers::codex::auth::errors::CodexOAuthError,
    > {
        let path = codexbar::providers::codex::auth::credentials::auth_path().ok_or(
            codexbar::providers::codex::auth::errors::CodexOAuthError::CredentialsNotFound,
        )?;
        let bytes = std::fs::read(&path).map_err(|_| {
            codexbar::providers::codex::auth::errors::CodexOAuthError::CredentialsNotFound
        })?;
        codexbar::providers::codex::auth::credentials::CodexCredentials::parse(&bytes).map_err(
            |e| {
                codexbar::providers::codex::auth::errors::CodexOAuthError::DecodeFailed(
                    e.to_string(),
                )
            },
        )
    }
}

// ── Tier-1 provider transports + credential placeholders ─────────────
//
// These follow the same pattern as the Claude/Codex shims above: when
// reqwest cannot build we fall back to a null transport that always
// reports `Network`, and credential resolvers default to `NoToken`
// until the secret-storage UI lands. The strategies stay wired so the
// refresh loop walks each provider on every tick.

struct NullCopilotGithubHttp;
#[async_trait::async_trait]
impl CopilotGithubHttp for NullCopilotGithubHttp {
    async fn get(
        &self,
        _: &str,
        _: &[(&str, &str)],
    ) -> Result<CopilotGithubResponse, ProviderFetchError> {
        Err(ProviderFetchError::Network(
            "reqwest unavailable; Copilot OAuth disabled".into(),
        ))
    }
}

/// Reads the active Copilot/GitHub OAuth token from the DPAPI-wrapped
/// `TokenAccountStore`. The optional GHE host is read from the
/// `CODEXBAR_COPILOT_HOST` env var until the settings UI exposes it as
/// a stored field.
struct StoredCopilotCredentials {
    tokens: Arc<TokenAccountStore>,
    settings: SettingsHandle,
}
#[async_trait::async_trait]
impl CopilotCredentialsResolver for StoredCopilotCredentials {
    async fn resolve(&self) -> Result<Option<CopilotCredentials>, ProviderFetchError> {
        let store = self.tokens.clone();
        let active = tokio::task::spawn_blocking(move || store.active_for("copilot"))
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        let Some(account) = active else {
            return Ok(None);
        };
        let token = account.value.trim();
        if token.is_empty() {
            return Ok(None);
        }
        // Settings picker wins; env var stays as a power-user override.
        let enterprise_host = self
            .settings
            .snapshot()
            .provider_kv_get("copilot.enterprise_host")
            .map(|s| s.to_string())
            .or_else(|| std::env::var("CODEXBAR_COPILOT_HOST").ok())
            .filter(|s| !s.trim().is_empty());
        Ok(Some(CopilotCredentials {
            access_token: token.to_string(),
            enterprise_host,
        }))
    }
}

/// Caches the @google/gemini-cli OAuth client credentials so we do not
/// re-walk the filesystem on every refresh tick. The locator hits
/// 4-12 candidate paths; running it inline would tax the refresh
/// budget. Cache invalidates only on app restart, which is fine —
/// the embedded constants change when the user upgrades the CLI, and
/// the next launch will pick up the new ones.
#[derive(Default)]
struct CachedGeminiClientLocator {
    cached: parking_lot::Mutex<Option<Option<GeminiOAuthClient>>>,
}

#[async_trait::async_trait]
impl ClientCredentialsProvider for CachedGeminiClientLocator {
    async fn resolve(&self) -> Result<Option<GeminiOAuthClient>, ProviderFetchError> {
        if let Some(cached) = self.cached.lock().clone() {
            return Ok(cached);
        }
        // Run the FS walk on a blocking thread.
        let resolved = tokio::task::spawn_blocking(|| {
            let env = OsEnv;
            let fs = OsFilesystem;
            locate_gemini_client(&env, &fs).ok()
        })
        .await
        .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        *self.cached.lock() = Some(resolved.clone());
        Ok(resolved)
    }
}

struct NullGeminiHttp;
#[async_trait::async_trait]
impl GoogleHttp for NullGeminiHttp {
    async fn request(
        &self,
        _: GeminiHttpMethod,
        _: &str,
        _: &str,
        _: Option<&[u8]>,
    ) -> Result<GeminiGoogleResponse, ProviderFetchError> {
        Err(ProviderFetchError::Network(
            "reqwest unavailable; Gemini OAuth disabled".into(),
        ))
    }
}

/// Resolves Gemini OAuth credentials from `~/.gemini/oauth_creds.json`
/// (`%USERPROFILE%\.gemini` on Windows) and `~/.gemini/settings.json`.
struct FilesystemGeminiCredentials;
#[async_trait::async_trait]
impl GeminiCredentialsResolver for FilesystemGeminiCredentials {
    async fn resolve(&self) -> Result<GeminiCredentialsState, ProviderFetchError> {
        let Some(home) = dirs_home_dir() else {
            return Ok(GeminiCredentialsState {
                auth_type: GeminiAuthType::Unknown,
                credentials: None,
            });
        };
        let auth_type = load_auth_type(&home);
        let credentials = match load_credentials(&home) {
            Ok(c) => Some(c),
            Err(ProviderFetchError::NoToken(_)) => None,
            Err(other) => return Err(other),
        };
        Ok(GeminiCredentialsState {
            auth_type,
            credentials,
        })
    }
}

fn dirs_home_dir() -> Option<std::path::PathBuf> {
    if let Ok(home) = std::env::var("USERPROFILE") {
        return Some(std::path::PathBuf::from(home));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(std::path::PathBuf::from(home));
    }
    None
}

struct NullOpenRouterHttp;
#[async_trait::async_trait]
impl OpenRouterHttp for NullOpenRouterHttp {
    async fn get(
        &self,
        _: &str,
        _: &str,
        _: &[(&str, &str)],
        _: std::time::Duration,
    ) -> Result<OpenRouterResponse, ProviderFetchError> {
        Err(ProviderFetchError::Network(
            "reqwest unavailable; OpenRouter disabled".into(),
        ))
    }
}

/// Reads the active OpenRouter API key from the DPAPI-wrapped token
/// store. Falls back to `OPENROUTER_API_KEY` when no stored account
/// exists, so headless smoke tests still work.
struct StoredOpenRouterCredentials {
    tokens: Arc<TokenAccountStore>,
}
#[async_trait::async_trait]
impl OpenRouterCredentialsResolver for StoredOpenRouterCredentials {
    async fn resolve(&self) -> Result<Option<OpenRouterCredentials>, ProviderFetchError> {
        let store = self.tokens.clone();
        let active = tokio::task::spawn_blocking(move || store.active_for("openrouter"))
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        let api_key = match active.map(|a| a.value).filter(|v| !v.trim().is_empty()) {
            Some(value) => value,
            None => match std::env::var("OPENROUTER_API_KEY") {
                Ok(t) if !t.trim().is_empty() => t,
                _ => return Ok(None),
            },
        };
        Ok(Some(OpenRouterCredentials {
            api_key,
            base_url: std::env::var("OPENROUTER_BASE_URL").ok(),
            http_referer: std::env::var("OPENROUTER_HTTP_REFERER").ok(),
            client_title: std::env::var("OPENROUTER_X_TITLE").ok(),
        }))
    }
}

struct NullFactoryHttp;
#[async_trait::async_trait]
impl FactoryHttp for NullFactoryHttp {
    async fn get(
        &self,
        _: &str,
        _: &[(&str, &str)],
    ) -> Result<FactoryResponse, ProviderFetchError> {
        Err(ProviderFetchError::Network(
            "reqwest unavailable; Factory disabled".into(),
        ))
    }
}

/// Reads the active Factory credential from the token store. The
/// account `kind` decides whether the value goes into the
/// `Authorization: Bearer` header or the `Cookie` header. Falls back
/// to the legacy `CODEXBAR_FACTORY_BEARER` / `CODEXBAR_FACTORY_COOKIE`
/// env vars when no stored account exists.
struct StoredFactoryCredentials {
    tokens: Arc<TokenAccountStore>,
}

const FACTORY_WORKOS_REFRESH_LABEL_PREFIX: &str = "WorkOS refresh";

#[async_trait::async_trait]
impl FactoryCredentialsResolver for StoredFactoryCredentials {
    async fn resolve(&self) -> Result<FactoryCredentials, ProviderFetchError> {
        use codexbar::secrets::token_account::TokenKind;
        let store = self.tokens.clone();
        let list = tokio::task::spawn_blocking(move || store.load("factory"))
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;

        let active = list
            .active_id
            .as_deref()
            .and_then(|id| list.accounts.iter().find(|a| a.id == id));

        let mut bearer: Option<String> = None;
        let mut cookie: Option<String> = None;
        if let Some(account) = active {
            let value = account.value.trim().to_string();
            if !value.is_empty() {
                match account.kind {
                    TokenKind::Cookie => cookie = Some(value),
                    TokenKind::OauthToken | TokenKind::ApiKey => {
                        bearer = Some(value);
                    }
                }
            }
        }

        // A separate account labelled "WorkOS refresh" holds the
        // refresh token. We never expose it as the bearer directly —
        // the strategy trades it for an access_token via WorkOS first.
        let workos_refresh_token = list
            .accounts
            .iter()
            .find(|a| a.label.starts_with(FACTORY_WORKOS_REFRESH_LABEL_PREFIX))
            .and_then(|a| {
                let v = a.value.trim();
                if v.is_empty() { None } else { Some(v.to_string()) }
            });

        if bearer.is_none() {
            bearer = std::env::var("CODEXBAR_FACTORY_BEARER")
                .ok()
                .filter(|s| !s.trim().is_empty());
        }
        if cookie.is_none() {
            cookie = std::env::var("CODEXBAR_FACTORY_COOKIE")
                .ok()
                .filter(|s| !s.trim().is_empty());
        }
        let workos_organization_id = std::env::var("CODEXBAR_FACTORY_WORKOS_ORG")
            .ok()
            .filter(|s| !s.trim().is_empty());

        Ok(FactoryCredentials {
            bearer,
            cookie,
            workos_refresh_token,
            workos_organization_id,
        })
    }

    async fn persist_workos_refresh(
        &self,
        bearer: &str,
        new_refresh_token: Option<&str>,
    ) -> Result<(), ProviderFetchError> {
        use codexbar::secrets::token_account::TokenKind;
        let store = self.tokens.clone();
        let bearer = bearer.to_string();
        let new_rt = new_refresh_token.map(|s| s.to_string());
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let list = store.load("factory").map_err(|e| e.to_string())?;
            // Update / create the active bearer account.
            let active_bearer_id = list.active_id.as_deref().and_then(|id| {
                list.accounts
                    .iter()
                    .find(|a| a.id == id && a.kind != TokenKind::Cookie)
                    .map(|a| a.id.clone())
            });
            if let Some(id) = active_bearer_id {
                store
                    .edit("factory", &id, None, Some(bearer))
                    .map_err(|e| e.to_string())?;
            } else {
                let acct = store
                    .add("factory", TokenKind::OauthToken, "WorkOS bearer", bearer)
                    .map_err(|e| e.to_string())?;
                store
                    .set_active("factory", &acct.id)
                    .map_err(|e| e.to_string())?;
            }

            if let Some(rt) = new_rt {
                // Update or create the dedicated refresh-token slot.
                let refresh_id = list
                    .accounts
                    .iter()
                    .find(|a| a.label.starts_with(FACTORY_WORKOS_REFRESH_LABEL_PREFIX))
                    .map(|a| a.id.clone());
                if let Some(id) = refresh_id {
                    store
                        .edit("factory", &id, None, Some(rt))
                        .map_err(|e| e.to_string())?;
                } else {
                    store
                        .add(
                            "factory",
                            TokenKind::OauthToken,
                            FACTORY_WORKOS_REFRESH_LABEL_PREFIX,
                            rt,
                        )
                        .map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| ProviderFetchError::Network(e.to_string()))?
        .map_err(ProviderFetchError::Network)?;
        Ok(())
    }
}

// ── Tier-2 provider transports + credential placeholders ─────────────

struct NullDeepSeekHttp;
#[async_trait::async_trait]
impl DeepSeekHttp for NullDeepSeekHttp {
    async fn get(&self, _: &str, _: &str) -> Result<DeepSeekResponse, ProviderFetchError> {
        Err(ProviderFetchError::Network(
            "reqwest unavailable; DeepSeek disabled".into(),
        ))
    }
}

struct StoredDeepSeekCredentials {
    tokens: Arc<TokenAccountStore>,
}
#[async_trait::async_trait]
impl DeepSeekCredentialsResolver for StoredDeepSeekCredentials {
    async fn resolve(&self) -> Result<Option<String>, ProviderFetchError> {
        let store = self.tokens.clone();
        let active = tokio::task::spawn_blocking(move || store.active_for("deepseek"))
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        Ok(active.map(|a| a.value).filter(|v| !v.trim().is_empty()))
    }
}

struct NullMoonshotHttp;
#[async_trait::async_trait]
impl MoonshotHttp for NullMoonshotHttp {
    async fn get(&self, _: &str, _: &str) -> Result<MoonshotResponse, ProviderFetchError> {
        Err(ProviderFetchError::Network(
            "reqwest unavailable; Moonshot disabled".into(),
        ))
    }
}

struct StoredMoonshotCredentials {
    tokens: Arc<TokenAccountStore>,
    settings: SettingsHandle,
}
#[async_trait::async_trait]
impl MoonshotCredentialsResolver for StoredMoonshotCredentials {
    async fn resolve(&self) -> Result<Option<MoonshotCredentials>, ProviderFetchError> {
        let store = self.tokens.clone();
        let active = tokio::task::spawn_blocking(move || store.active_for("moonshot"))
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        let Some(account) = active else {
            return Ok(None);
        };
        let api_key = account.value.trim().to_string();
        if api_key.is_empty() {
            return Ok(None);
        }
        let region = self
            .settings
            .snapshot()
            .provider_kv_get("moonshot.region")
            .map(str::to_ascii_lowercase)
            .or_else(|| std::env::var("CODEXBAR_MOONSHOT_REGION").ok());
        let region = match region.as_deref() {
            Some("china") | Some("cn") => MoonshotRegion::China,
            _ => MoonshotRegion::International,
        };
        Ok(Some(MoonshotCredentials { api_key, region }))
    }
}

struct NullZaiHttp;
#[async_trait::async_trait]
impl ZaiHttp for NullZaiHttp {
    async fn get(&self, _: &str, _: &str) -> Result<ZaiResponse, ProviderFetchError> {
        Err(ProviderFetchError::Network(
            "reqwest unavailable; Z.ai disabled".into(),
        ))
    }
}

struct StoredZaiCredentials {
    tokens: Arc<TokenAccountStore>,
    settings: SettingsHandle,
}
#[async_trait::async_trait]
impl ZaiCredentialsResolver for StoredZaiCredentials {
    async fn resolve(&self) -> Result<Option<ZaiCredentials>, ProviderFetchError> {
        let store = self.tokens.clone();
        let active = tokio::task::spawn_blocking(move || store.active_for("zai"))
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        let Some(account) = active else {
            return Ok(None);
        };
        let api_key = account.value.trim().to_string();
        if api_key.is_empty() {
            return Ok(None);
        }
        let snap = self.settings.snapshot();
        let region_value = snap
            .provider_kv_get("zai.region")
            .map(str::to_ascii_lowercase)
            .or_else(|| std::env::var("CODEXBAR_ZAI_REGION").ok());
        let region = match region_value.as_deref() {
            Some("bigmodel-cn") | Some("cn") => ZaiRegion::BigmodelCN,
            _ => ZaiRegion::Global,
        };
        let host_override = snap
            .provider_kv_get("zai.api_host")
            .map(|s| s.to_string())
            .or_else(|| std::env::var("Z_AI_API_HOST").ok())
            .filter(|s| !s.trim().is_empty());
        let quota_url_override = snap
            .provider_kv_get("zai.quota_url")
            .map(|s| s.to_string())
            .or_else(|| std::env::var("Z_AI_QUOTA_URL").ok())
            .filter(|s| !s.trim().is_empty());
        Ok(Some(ZaiCredentials {
            api_key,
            region,
            host_override,
            quota_url_override,
        }))
    }
}

/// Locate the `claude` binary on PATH. Returns `None` when the user has
/// not installed the CLI.
fn claude_cli_binary() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    let exe = if cfg!(windows) {
        "claude.cmd"
    } else {
        "claude"
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(exe);
        if candidate.exists() {
            return candidate.to_str().map(|s| s.to_string());
        }
    }
    None
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let env = match PathEnvironment::discover() {
        Ok(env) => {
            if let Err(err) = env.ensure() {
                eprintln!("[codexbar] failed to ensure path environment: {err}");
            }
            env
        }
        Err(err) => {
            eprintln!("[codexbar] failed to discover path environment: {err}");
            return;
        }
    };

    let _log_guard = codexbar::logging::init(&env.logs_dir).ok();
    info!(target: "codexbar::app", version = codexbar::version(), "app.boot");

    let settings: SettingsHandle = commands::build_settings_handle(env.config_file.clone());
    let usage = Arc::new(UsageStore::new());
    let refresh = RefreshLoop::new(settings.clone());

    let token_store = Arc::new(TokenAccountStore::new(env.secrets_dir.clone()));
    let cookie_cache = Arc::new(CookieHeaderCache::new(env.cache_dir.join("cookie-cache")));
    let cookie_gate = Arc::new(CookieAccessGate::new());
    let cookie_importer = Arc::new(CookieImporter::new(
        cookie_cache.clone(),
        cookie_gate,
        token_store.clone(),
    ));

    // Spawn the refresh loop on a tokio runtime owned by the main thread.
    // We leak the runtime intentionally so it lives for the app lifetime;
    // the OS reclaims on exit and tokio handles will be cancelled.
    let runtime: &'static Runtime = Box::leak(Box::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("codexbar-refresh")
            .build()
            .expect("tokio runtime must build"),
    ));
    let refresh_for_spawn = refresh.clone();
    runtime.spawn(async move {
        refresh_for_spawn.spawn().await.ok();
    });

    // Phase 4 P4-20: pipe core UsageStore events to the Tauri event bus
    // so the popup can listen for `usage:updated`. The Tauri setup
    // closure later fills `app_handle_holder` with the live AppHandle.
    let app_handle_holder: Arc<parking_lot::Mutex<Option<tauri::AppHandle>>> =
        Arc::new(parking_lot::Mutex::new(None));
    let app_handle_for_setup = app_handle_holder.clone();
    let app_handle_for_bridge = app_handle_holder.clone();
    let usage_rx = usage.subscribe();
    runtime.spawn(async move {
        bridge_usage_events(usage_rx, app_handle_for_bridge).await;
    });

    let first_run_store = FirstRunStore::new(env.roaming.clone());

    // Phase 4 P4-20: build the Claude provider with real reqwest + cookie
    // wiring and install it into the refresh loop. The Claude CLI fetch
    // path falls back to a recorded runner when the CLI binary is not on
    // PATH; this keeps the popup populated even on a fresh install.
    let claude_provider = Arc::new(ClaudeProvider::default());
    let oauth_http: Arc<dyn HttpClient> = match ReqwestClient::new() {
        Ok(c) => Arc::new(c),
        Err(_) => Arc::new(NullHttpClient),
    };
    let oauth_credentials: Arc<dyn CredentialsResolver> = Arc::new(FilesystemCredentialsResolver);
    let web_client: Arc<dyn WebClient> = match ReqwestWebClient::new() {
        Ok(c) => Arc::new(c),
        Err(_) => Arc::new(NullWebClient),
    };
    let web_cookies: Arc<dyn CookieResolver> = Arc::new(StoredCookieResolver::new(
        token_store.clone(),
        cookie_cache.clone(),
        "claude",
    ));
    let cli_runner: Arc<dyn CliRunner> = match claude_cli_binary() {
        Some(_) => codexbar::providers::claude::planner::default_cli_runner(),
        None => Arc::new(RecordedRunner {
            output: String::new(),
        }),
    };
    claude_provider.install_wiring(ClaudeWiring {
        oauth_http,
        oauth_credentials,
        web_client,
        web_cookies,
        cli_runner,
        cli_binary: claude_cli_binary().unwrap_or_else(|| "claude".to_string()),
    });
    // Phase 5: Codex provider.
    //   - OAuth strategy: hits chatgpt.com/wham/usage with the bearer
    //     from `~/.codex/auth.json` and the codex_cli_rs/<version>
    //     User-Agent the API requires (verified live 2026-05-13).
    //   - Web strategy: reuses the shared CookieImporter through the
    //     CodexCookieResolver adapter. ChatGPT.com's Cloudflare layer
    //     blocks raw cookie requests, so this path almost always
    //     fails for end users — kept for future fallback work.
    //   - CLI strategy: still routed through the unavailable
    //     transport until the ConPTY launcher lands.
    let codex_provider = Arc::new(CodexProvider::default());
    let codex_oauth_http: Arc<dyn codexbar::providers::codex::oauth::usage::UsageHttp> =
        match codexbar::providers::codex::oauth::transport::ReqwestUsageClient::new() {
            Ok(c) => Arc::new(c),
            Err(_) => Arc::new(NullCodexUsageHttp),
        };
    let codex_oauth_credentials: Arc<
        dyn codexbar::providers::codex::oauth::strategy::OAuthCredentialsResolver,
    > = Arc::new(FilesystemCodexCredentials);
    let codex_web_client: Arc<dyn WebClient> = match ReqwestWebClient::new() {
        Ok(c) => Arc::new(c),
        Err(_) => Arc::new(NullWebClient),
    };
    let codex_cookie_resolver: Arc<dyn CookieResolver> = Arc::new(
        codexbar::providers::codex::web::cookie_resolver::CodexCookieResolver::new(
            cookie_importer.clone(),
        ),
    );
    // Codex CLI: when a `codex` binary is locatable on the host, wire
    // the real ConPTY-backed transport so the JSON-RPC strategy can
    // talk to it. Otherwise fall back to the unavailable stub so the
    // rest of the plan still runs.
    let codex_cli_factory: Arc<dyn CodexTransportFactory> =
        match codexbar::providers::codex::cli::binary_locator::locate() {
            Ok(path) => {
                let binary = path.to_string_lossy().to_string();
                info!(target: "codexbar::app", binary = %binary, "codex.cli.conpty.installed");
                Arc::new(
                    codexbar::providers::codex::cli::conpty_transport::ConPtyTransportFactory::new(
                        binary,
                    ),
                )
            }
            Err(_) => Arc::new(UnavailableCodexTransport),
        };
    codex_provider.install_wiring(CodexWiring {
        oauth_http: codex_oauth_http,
        oauth_credentials: codex_oauth_credentials,
        web_client: codex_web_client,
        web_cookies: codex_cookie_resolver,
        cli_transport_factory: codex_cli_factory,
    });

    // Phase 6.5: Tier-1 providers ported from the macOS Swift source.
    // Each gets its own reqwest transport plus an env-driven credential
    // resolver as a placeholder. The settings UI will swap these out
    // for keychain-backed resolvers in a follow-up.
    let cursor_provider = Arc::new(CursorProvider::default());
    let cursor_web_client: Arc<dyn WebClient> = match ReqwestWebClient::new() {
        Ok(c) => Arc::new(c),
        Err(_) => Arc::new(NullWebClient),
    };
    let cursor_cookie_resolver: Arc<dyn CookieResolver> = Arc::new(StoredCookieResolver::new(
        token_store.clone(),
        cookie_cache.clone(),
        "cursor",
    ));
    cursor_provider.install_wiring(CursorWiring {
        web_client: cursor_web_client,
        web_cookies: cursor_cookie_resolver,
    });

    let copilot_provider = Arc::new(CopilotProvider::default());
    let copilot_http: Arc<dyn CopilotGithubHttp> = match ReqwestGithubClient::new() {
        Ok(c) => Arc::new(c),
        Err(_) => Arc::new(NullCopilotGithubHttp),
    };
    copilot_provider.install_wiring(CopilotWiring {
        http: copilot_http,
        credentials: Arc::new(StoredCopilotCredentials {
            tokens: token_store.clone(),
            settings: settings.clone(),
        }),
    });

    let gemini_provider = Arc::new(GeminiProvider::default());
    let gemini_reqwest = ReqwestGoogleClient::new().ok().map(Arc::new);
    let gemini_http: Arc<dyn GoogleHttp> = match gemini_reqwest.clone() {
        Some(c) => c,
        None => Arc::new(NullGeminiHttp),
    };
    let gemini_wiring = GeminiWiring {
        http: gemini_http,
        credentials: Arc::new(FilesystemGeminiCredentials),
    };
    // Install the refresh hook when reqwest is available + we have a
    // home dir to write the refreshed token back to. The OAuth client
    // credentials are located lazily on first refresh; if @google/gemini-cli
    // is not installed the strategy gracefully falls back to Unauthorized.
    match (gemini_reqwest, dirs_home_dir()) {
        (Some(refresh_http), Some(home)) => {
            let refresh_hook = RefreshHook {
                http: refresh_http,
                client: Arc::new(CachedGeminiClientLocator::default()),
                home_dir: home,
            };
            gemini_provider.install_wiring_with_refresh(gemini_wiring, refresh_hook);
        }
        _ => gemini_provider.install_wiring(gemini_wiring),
    }

    let openrouter_provider = Arc::new(OpenRouterProvider::default());
    let openrouter_http: Arc<dyn OpenRouterHttp> = match ReqwestOpenRouterClient::new() {
        Ok(c) => Arc::new(c),
        Err(_) => Arc::new(NullOpenRouterHttp),
    };
    openrouter_provider.install_wiring(OpenRouterWiring {
        http: openrouter_http,
        credentials: Arc::new(StoredOpenRouterCredentials {
            tokens: token_store.clone(),
        }),
    });

    let factory_provider = Arc::new(FactoryProvider::default());
    let factory_reqwest = ReqwestFactoryClient::new().ok().map(Arc::new);
    let factory_http: Arc<dyn FactoryHttp> = match factory_reqwest.clone() {
        Some(c) => c,
        None => Arc::new(NullFactoryHttp),
    };
    let factory_wiring = FactoryWiring {
        http: factory_http,
        credentials: Arc::new(StoredFactoryCredentials {
            tokens: token_store.clone(),
        }),
    };
    match factory_reqwest {
        Some(workos_http) => {
            factory_provider.install_wiring_with_refresh(
                factory_wiring,
                codexbar::providers::factory::api::strategy::FactoryRefreshHook {
                    http: workos_http,
                },
            );
        }
        None => factory_provider.install_wiring(factory_wiring),
    }

    // Tier-2 providers. Each one follows the same template: a reqwest
    // transport (with a null fallback when reqwest cannot build) plus a
    // `TokenAccountStore`-backed credential resolver. Region/host
    // overrides come from env vars until the settings UI grows the
    // dedicated controls.
    let deepseek_provider = Arc::new(DeepSeekProvider::default());
    let deepseek_http: Arc<dyn DeepSeekHttp> = match ReqwestDeepSeekClient::new() {
        Ok(c) => Arc::new(c),
        Err(_) => Arc::new(NullDeepSeekHttp),
    };
    deepseek_provider.install_wiring(DeepSeekWiring {
        http: deepseek_http,
        credentials: Arc::new(StoredDeepSeekCredentials {
            tokens: token_store.clone(),
        }),
    });

    let moonshot_provider = Arc::new(MoonshotProvider::default());
    let moonshot_http: Arc<dyn MoonshotHttp> = match ReqwestMoonshotClient::new() {
        Ok(c) => Arc::new(c),
        Err(_) => Arc::new(NullMoonshotHttp),
    };
    moonshot_provider.install_wiring(MoonshotWiring {
        http: moonshot_http,
        credentials: Arc::new(StoredMoonshotCredentials {
            tokens: token_store.clone(),
            settings: settings.clone(),
        }),
    });

    let zai_provider = Arc::new(ZaiProvider::default());
    let zai_http: Arc<dyn ZaiHttp> = match ReqwestZaiClient::new() {
        Ok(c) => Arc::new(c),
        Err(_) => Arc::new(NullZaiHttp),
    };
    zai_provider.install_wiring(ZaiWiring {
        http: zai_http,
        credentials: Arc::new(StoredZaiCredentials {
            tokens: token_store.clone(),
            settings: settings.clone(),
        }),
    });

    let providers: Vec<Arc<dyn ProviderImplementation>> = vec![
        claude_provider.clone(),
        codex_provider.clone(),
        cursor_provider.clone(),
        copilot_provider.clone(),
        gemini_provider.clone(),
        openrouter_provider.clone(),
        factory_provider.clone(),
        deepseek_provider.clone(),
        moonshot_provider.clone(),
        zai_provider.clone(),
    ];
    refresh.install_providers(providers, usage.clone(), token_store.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(settings.clone())
        .manage(RefreshHandle(refresh))
        .manage(UsageHandle(usage))
        .manage(FirstRunHandle(first_run_store))
        .manage(secrets_commands::TokenAccountHandle(token_store))
        .manage(secrets_commands::CookieImporterHandle(cookie_importer))
        .manage(login_commands::CopilotLoginHandle(Arc::new(
            login_commands::CopilotLoginRegistry::default(),
        )))
        .setup(move |app| {
            // Hand the live AppHandle to the usage-event bridge.
            *app_handle_for_setup.lock() = Some(app.handle().clone());
            let refresh_i = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
            let pause_i = MenuItem::with_id(
                app,
                "pause",
                if settings.snapshot().pause_refresh {
                    "Resume refresh"
                } else {
                    "Pause refresh"
                },
                true,
                None::<&str>,
            )?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let prefs_i =
                MenuItem::with_id(app, "preferences", "Preferences...", true, None::<&str>)?;
            let about_i =
                MenuItem::with_id(app, "about", "About CodexBar4Windows", true, None::<&str>)?;
            let check_updates_i = MenuItem::with_id(
                app,
                "check_updates",
                "Check for updates",
                true,
                None::<&str>,
            )?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, Some("CmdOrCtrl+Q"))?;

            let menu = Menu::with_items(
                app,
                &[
                    &refresh_i,
                    &pause_i,
                    &sep1,
                    &prefs_i,
                    &about_i,
                    &check_updates_i,
                    &sep2,
                    &quit_i,
                ],
            )?;

            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("default window icon missing; bundle is misconfigured")?;
            let _tray = TrayIconBuilder::with_id("main")
                .icon(icon)
                .tooltip("CodexBar4Windows\nAI coding limits in your Windows tray")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        info!(target: "codexbar::tray", "menu.quit");
                        app.exit(0);
                    }
                    "preferences" => {
                        info!(target: "codexbar::tray", "menu.preferences");
                    }
                    "about" => {
                        info!(target: "codexbar::tray", "menu.about");
                    }
                    "check_updates" => {
                        info!(target: "codexbar::tray", "menu.check_updates");
                    }
                    "pause" => {
                        info!(target: "codexbar::tray", "menu.pause_toggle");
                        if let Some(handle) = app.try_state::<SettingsHandle>() {
                            let cur = handle.snapshot();
                            let _ = handle.update(codexbar::settings::SettingsPatch {
                                pause_refresh: Some(!cur.pause_refresh),
                                ..Default::default()
                            });
                        }
                    }
                    "refresh" => {
                        info!(target: "codexbar::tray", "menu.refresh");
                        if let Some(handle) = app.try_state::<RefreshHandle>() {
                            let loop_ref = handle.0.clone();
                            std::thread::spawn(move || {
                                let rt = tokio::runtime::Builder::new_current_thread()
                                    .enable_all()
                                    .build()
                                    .expect("oneshot runtime");
                                rt.block_on(async {
                                    let _ = loop_ref.refresh_now().await;
                                });
                            });
                        }
                    }
                    other => {
                        info!(target: "codexbar::tray", id = other, "menu.unknown");
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        info!(target: "codexbar::tray", "icon.left_click");
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            info!(target: "codexbar::tray", "icon.registered");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::get_settings,
            commands::update_settings,
            commands::reset_settings,
            commands::get_provider_kv,
            commands::set_provider_kv,
            commands::provider_descriptors,
            commands::provider_snapshots,
            commands::refresh_now,
            commands::toggle_pause,
            commands::open_preferences,
            commands::quit_app,
            commands::provider_settings_descriptors,
            commands::first_run_state,
            commands::first_run_mark_tray_hint_shown,
            commands::first_run_reset,
            secrets_commands::list_token_accounts,
            secrets_commands::add_token_account,
            secrets_commands::edit_token_account,
            secrets_commands::remove_token_account,
            secrets_commands::set_active_token_account,
            secrets_commands::set_manual_cookie,
            secrets_commands::import_cookies_for,
            secrets_commands::clear_cookie_cache,
            secrets_commands::auto_import_cookies,
            login_commands::start_copilot_device_login,
            login_commands::poll_copilot_device_login,
            login_commands::complete_factory_workos_login,
        ])
        .on_window_event(|window, event| {
            // Auto-dismiss the popup on focus loss to match the spec 80
            // behavior: the popover disappears whenever the user clicks
            // outside it or alt-tabs to another app.
            if window.label() == "main" {
                if let WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
