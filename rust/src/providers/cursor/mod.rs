//! Cursor provider. Web path ported from
//! `Sources/CodexBarCore/Providers/Cursor/CursorStatusProbe.swift`.

pub mod descriptor;
pub mod planner;
pub mod settings;
pub mod web;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use self::descriptor::cursor_descriptor;
use self::planner::CursorWiring;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct CursorProvider {
    descriptor: ProviderDescriptor,
    wiring: Mutex<Option<Vec<Arc<dyn Strategy>>>>,
}

impl Default for CursorProvider {
    fn default() -> Self {
        Self {
            descriptor: cursor_descriptor(),
            wiring: Mutex::new(None),
        }
    }
}

impl CursorProvider {
    pub fn install_wiring(&self, wiring: CursorWiring) {
        *self.wiring.lock() = Some(wiring.into_strategies());
    }
}

#[async_trait]
impl ProviderImplementation for CursorProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        self.wiring.lock().clone().unwrap_or_default()
    }
}

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: cursor_descriptor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::claude::web::strategy::{CookieResolver, WebClient, WebResponse};
    use crate::providers::descriptor::FetchStrategy;
    use crate::providers::errors::ProviderFetchError;

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

    #[test]
    fn strategies_are_empty_until_wiring_is_installed() {
        let provider = CursorProvider::default();
        assert!(provider.strategies().is_empty());
        assert_eq!(provider.descriptor().id.as_str(), "cursor");
    }

    #[test]
    fn install_wiring_yields_single_web_strategy() {
        let provider = CursorProvider::default();
        provider.install_wiring(CursorWiring {
            web_client: Arc::new(NoopWeb),
            web_cookies: Arc::new(NoopCookies),
        });
        let strategies = provider.strategies();
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].strategy_id(), FetchStrategy::Web);
    }
}
