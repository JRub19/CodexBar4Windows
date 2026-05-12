//! Codex provider. Phase 5 lands the OAuth API path, the CLI JSON-RPC
//! integration, and the history ownership scheme. Web scraping
//! (chatgpt.com cookies + WebView2 fallback) and the multi-account
//! promotion flow ship in a follow-up because both require live OpenAI
//! sessions to verify safely.

pub mod auth;
pub mod cli;
pub mod descriptor;
pub mod history;
pub mod oauth;
pub mod planner;
pub mod settings;

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
    fn install_wiring_yields_cli_strategy() {
        use crate::providers::codex::cli::rpc_client::{RpcCallError, RpcTransport};
        use crate::providers::codex::cli::strategy::TransportFactory;
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

        let provider = CodexProvider::default();
        provider.install_wiring(CodexWiring {
            cli_transport_factory: Arc::new(StubFactory),
        });
        let strategies = provider.strategies();
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].strategy_id(), FetchStrategy::CLI);
    }
}
