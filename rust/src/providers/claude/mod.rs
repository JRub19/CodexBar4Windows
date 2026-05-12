//! Claude provider. Phase 4 commits land here in order: P4-10
//! descriptor only, P4-11 OAuth credential discovery, P4-12 OAuth fetch
//! strategy, P4-13 Web strategy, P4-14 multi-account routing, P4-16 CLI
//! PTY strategy, P4-18 source consolidation, P4-19 settings, P4-20
//! tray and popup wiring.

pub mod cli;
pub mod descriptor;
pub mod errors;
pub mod models;
pub mod oauth;
pub mod planner;
pub mod routing;
pub mod tokens;
pub mod web;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use self::descriptor::claude_descriptor;
use self::planner::ClaudeWiring;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct ClaudeProvider {
    descriptor: ProviderDescriptor,
    wiring: Mutex<Option<Vec<Arc<dyn Strategy>>>>,
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self {
            descriptor: claude_descriptor(),
            wiring: Mutex::new(None),
        }
    }
}

impl ClaudeProvider {
    /// Install the runtime strategies. The Tauri shell calls this once
    /// at boot with the real reqwest + cookie + PTY transports.
    pub fn install_wiring(&self, wiring: ClaudeWiring) {
        *self.wiring.lock() = Some(wiring.into_strategies());
    }
}

#[async_trait]
impl ProviderImplementation for ClaudeProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        self.wiring.lock().clone().unwrap_or_default()
    }
}

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: claude_descriptor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::descriptor::FetchStrategy;

    #[test]
    fn strategies_are_empty_until_wiring_is_installed() {
        let provider = ClaudeProvider::default();
        assert!(provider.strategies().is_empty());
    }

    #[test]
    fn install_wiring_yields_three_strategies_in_order() {
        use crate::providers::claude::cli::pty_actor::RecordedRunner;
        use crate::providers::claude::oauth::credentials::OAuthCredentials;
        use crate::providers::claude::oauth::strategy::{
            CredentialsResolver, HttpClient, HttpResponse,
        };
        use crate::providers::claude::web::strategy::{CookieResolver, WebClient, WebResponse};
        use crate::providers::errors::ProviderFetchError;

        struct NoopHttp;
        #[async_trait]
        impl HttpClient for NoopHttp {
            async fn get_json(&self, _: &str, _: &str) -> Result<HttpResponse, ProviderFetchError> {
                Ok(HttpResponse {
                    status: 200,
                    body: b"{}".to_vec(),
                })
            }
        }
        struct NoopCreds;
        #[async_trait]
        impl CredentialsResolver for NoopCreds {
            async fn resolve(
                &self,
            ) -> Result<OAuthCredentials, crate::providers::claude::errors::CredentialError>
            {
                Ok(OAuthCredentials {
                    access_token: "x".into(),
                    refresh_token: None,
                    expires_at_unix_secs: None,
                    scopes: vec!["user:profile".into()],
                })
            }
        }
        struct NoopWeb;
        #[async_trait]
        impl WebClient for NoopWeb {
            async fn get_json(&self, _: &str, _: &str) -> Result<WebResponse, ProviderFetchError> {
                Ok(WebResponse {
                    status: 200,
                    body: b"[]".to_vec(),
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

        let provider = ClaudeProvider::default();
        provider.install_wiring(ClaudeWiring {
            oauth_http: Arc::new(NoopHttp),
            oauth_credentials: Arc::new(NoopCreds),
            web_client: Arc::new(NoopWeb),
            web_cookies: Arc::new(NoopCookies),
            cli_runner: Arc::new(RecordedRunner { output: "".into() }),
            cli_binary: "claude".into(),
        });
        let strategies = provider.strategies();
        assert_eq!(strategies.len(), 3);
        assert_eq!(strategies[0].strategy_id(), FetchStrategy::OAuth);
        assert_eq!(strategies[1].strategy_id(), FetchStrategy::Web);
        assert_eq!(strategies[2].strategy_id(), FetchStrategy::CLI);
    }
}
