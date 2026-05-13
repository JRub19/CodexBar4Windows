//! OpenRouter provider. API-key path ported from
//! `Sources/CodexBarCore/Providers/OpenRouter/OpenRouterUsageStats.swift`.

pub mod api;
pub mod descriptor;
pub mod planner;
pub mod settings;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use self::descriptor::openrouter_descriptor;
use self::planner::OpenRouterWiring;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct OpenRouterProvider {
    descriptor: ProviderDescriptor,
    wiring: Mutex<Option<Vec<Arc<dyn Strategy>>>>,
}

impl Default for OpenRouterProvider {
    fn default() -> Self {
        Self {
            descriptor: openrouter_descriptor(),
            wiring: Mutex::new(None),
        }
    }
}

impl OpenRouterProvider {
    pub fn install_wiring(&self, wiring: OpenRouterWiring) {
        *self.wiring.lock() = Some(wiring.into_strategies());
    }
}

#[async_trait]
impl ProviderImplementation for OpenRouterProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        self.wiring.lock().clone().unwrap_or_default()
    }
}

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: openrouter_descriptor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::descriptor::FetchStrategy;
    use crate::providers::errors::ProviderFetchError;
    use crate::providers::openrouter::api::strategy::{
        OpenRouterCredentials, OpenRouterCredentialsResolver, OpenRouterHttp, OpenRouterResponse,
    };
    use std::time::Duration;

    struct NoopHttp;
    #[async_trait]
    impl OpenRouterHttp for NoopHttp {
        async fn get(
            &self,
            _: &str,
            _: &str,
            _: &[(&str, &str)],
            _: Duration,
        ) -> Result<OpenRouterResponse, ProviderFetchError> {
            Ok(OpenRouterResponse {
                status: 200,
                body: b"{}".to_vec(),
            })
        }
    }
    struct NoopCreds;
    #[async_trait]
    impl OpenRouterCredentialsResolver for NoopCreds {
        async fn resolve(&self) -> Result<Option<OpenRouterCredentials>, ProviderFetchError> {
            Ok(None)
        }
    }

    #[test]
    fn strategies_are_empty_until_wiring_is_installed() {
        let provider = OpenRouterProvider::default();
        assert!(provider.strategies().is_empty());
        assert_eq!(provider.descriptor().id.as_str(), "openrouter");
    }

    #[test]
    fn install_wiring_yields_single_api_key_strategy() {
        let provider = OpenRouterProvider::default();
        provider.install_wiring(OpenRouterWiring {
            http: Arc::new(NoopHttp),
            credentials: Arc::new(NoopCreds),
        });
        let strategies = provider.strategies();
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].strategy_id(), FetchStrategy::ApiKey);
    }
}
