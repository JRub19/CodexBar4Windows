//! Codex provider. Phase 5 lands the OAuth API path, the CLI JSON-RPC
//! integration, and the history ownership scheme. Web scraping
//! (chatgpt.com cookies + WebView2 fallback) and the multi-account
//! promotion flow ship in a follow-up because both require live OpenAI
//! sessions to verify safely.

pub mod auth;
pub mod cli;
pub mod descriptor;
pub mod history;
pub mod managed;
pub mod oauth;
pub mod planner;
pub mod promotion;
pub mod settings;
pub mod web;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use self::descriptor::codex_descriptor;
use self::planner::CodexWiring;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct CodexProvider {
    descriptor: ProviderDescriptor,
    wiring: Mutex<Option<Vec<Arc<dyn Strategy>>>>,
}

impl Default for CodexProvider {
    fn default() -> Self {
        Self {
            descriptor: codex_descriptor(),
            wiring: Mutex::new(None),
        }
    }
}

impl CodexProvider {
    /// Install the runtime strategies. The Tauri shell calls this once
    /// at boot with the real ConPTY transport factory.
    pub fn install_wiring(&self, wiring: CodexWiring) {
        *self.wiring.lock() = Some(wiring.into_strategies());
    }

    /// Install strategies with a TUI scraper instead of the JSON-RPC
    /// CLI strategy. Use this when a real `codex` binary is on disk —
    /// real codex emits an interactive TUI, not JSON-RPC.
    pub fn install_wiring_with_tui(
        &self,
        wiring: CodexWiring,
        tui_runner: std::sync::Arc<dyn self::cli::tui_strategy::CodexTuiRunner>,
        binary: String,
    ) {
        *self.wiring.lock() = Some(wiring.into_strategies_with_tui(tui_runner, binary));
    }
}

#[async_trait]
impl ProviderImplementation for CodexProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        self.wiring.lock().clone().unwrap_or_default()
    }
}

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: codex_descriptor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategies_are_empty_until_wiring_is_installed() {
        let provider = CodexProvider::default();
        assert!(provider.strategies().is_empty());
        assert_eq!(provider.descriptor().id.as_str(), "codex");
    }

    #[test]
    fn install_wiring_yields_oauth_web_and_cli_strategies_in_order() {
        use crate::providers::claude::web::strategy::{CookieResolver, WebClient, WebResponse};
        use crate::providers::codex::auth::credentials::CodexCredentials;
        use crate::providers::codex::auth::errors::CodexOAuthError;
        use crate::providers::codex::cli::rpc_client::{RpcCallError, RpcTransport};
        use crate::providers::codex::cli::strategy::TransportFactory;
        use crate::providers::codex::oauth::strategy::OAuthCredentialsResolver;
        use crate::providers::codex::oauth::usage::{UsageHttp, UsageResponse};
        use crate::providers::descriptor::FetchStrategy;
        use crate::providers::errors::ProviderFetchError;

        struct StaleTransport;
        #[async_trait]
        impl RpcTransport for StaleTransport {
            async fn send(&self, _: Vec<u8>) -> Result<(), RpcCallError> {
                Ok(())
            }
            async fn recv(&self) -> Result<Vec<u8>, RpcCallError> {
                Err(RpcCallError::Closed)
            }
        }
        struct StubFactory;
        impl TransportFactory for StubFactory {
            fn open(&self) -> Result<Arc<dyn RpcTransport>, ProviderFetchError> {
                Ok(Arc::new(StaleTransport) as Arc<dyn RpcTransport>)
            }
        }
        struct NoopWeb;
        #[async_trait]
        impl WebClient for NoopWeb {
            async fn get_json(&self, _: &str, _: &str) -> Result<WebResponse, ProviderFetchError> {
                Ok(WebResponse {
                    status: 200,
                    body: b"{}".to_vec(),
                })
            }
        }
        struct NoopCookies;
        #[async_trait]
        impl CookieResolver for NoopCookies {
            async fn cookie(&self) -> Result<Option<String>, ProviderFetchError> {
                Ok(None)
            }
            async fn invalidate(&self) -> Result<(), ProviderFetchError> {
                Ok(())
            }
        }
        struct NoopUsage;
        #[async_trait]
        impl UsageHttp for NoopUsage {
            async fn get(
                &self,
                _: &str,
                _: &[(&str, &str)],
            ) -> Result<UsageResponse, CodexOAuthError> {
                Ok(UsageResponse {
                    status: 404,
                    body: b"{}".to_vec(),
                })
            }
        }
        struct NoopCreds;
        #[async_trait]
        impl OAuthCredentialsResolver for NoopCreds {
            async fn resolve(&self) -> Result<CodexCredentials, CodexOAuthError> {
                Err(CodexOAuthError::CredentialsNotFound)
            }
        }

        let provider = CodexProvider::default();
        provider.install_wiring(CodexWiring {
            oauth_http: Arc::new(NoopUsage),
            oauth_credentials: Arc::new(NoopCreds),
            web_client: Arc::new(NoopWeb),
            web_cookies: Arc::new(NoopCookies),
            cli_transport_factory: Arc::new(StubFactory),
        });
        let strategies = provider.strategies();
        assert_eq!(strategies.len(), 3);
        assert_eq!(strategies[0].strategy_id(), FetchStrategy::OAuth);
        assert_eq!(strategies[1].strategy_id(), FetchStrategy::Web);
        assert_eq!(strategies[2].strategy_id(), FetchStrategy::CLI);
    }
}
