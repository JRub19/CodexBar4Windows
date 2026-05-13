//! Copilot provider. OAuth path ported from
//! `Sources/CodexBarCore/Providers/Copilot/CopilotUsageFetcher.swift`.

pub mod descriptor;
pub mod oauth;
pub mod planner;
pub mod settings;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use self::descriptor::copilot_descriptor;
use self::planner::CopilotWiring;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct CopilotProvider {
    descriptor: ProviderDescriptor,
    wiring: Mutex<Option<Vec<Arc<dyn Strategy>>>>,
}

impl Default for CopilotProvider {
    fn default() -> Self {
        Self {
            descriptor: copilot_descriptor(),
            wiring: Mutex::new(None),
        }
    }
}

impl CopilotProvider {
    pub fn install_wiring(&self, wiring: CopilotWiring) {
        *self.wiring.lock() = Some(wiring.into_strategies());
    }
}

#[async_trait]
impl ProviderImplementation for CopilotProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        self.wiring.lock().clone().unwrap_or_default()
    }
}

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: copilot_descriptor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::copilot::oauth::strategy::{
        CopilotCredentials, CopilotCredentialsResolver, GithubHttp, GithubResponse,
    };
    use crate::providers::descriptor::FetchStrategy;
    use crate::providers::errors::ProviderFetchError;

    struct NoopHttp;
    #[async_trait]
    impl GithubHttp for NoopHttp {
        async fn get(
            &self,
            _: &str,
            _: &[(&str, &str)],
        ) -> Result<GithubResponse, ProviderFetchError> {
            Ok(GithubResponse {
                status: 200,
                body: b"{}".to_vec(),
            })
        }
    }
    struct NoopCreds;
    #[async_trait]
    impl CopilotCredentialsResolver for NoopCreds {
        async fn resolve(&self) -> Result<Option<CopilotCredentials>, ProviderFetchError> {
            Ok(None)
        }
    }

    #[test]
    fn strategies_are_empty_until_wiring_is_installed() {
        let provider = CopilotProvider::default();
        assert!(provider.strategies().is_empty());
        assert_eq!(provider.descriptor().id.as_str(), "copilot");
    }

    #[test]
    fn install_wiring_yields_single_oauth_strategy() {
        let provider = CopilotProvider::default();
        provider.install_wiring(CopilotWiring {
            http: Arc::new(NoopHttp),
            credentials: Arc::new(NoopCreds),
        });
        let strategies = provider.strategies();
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].strategy_id(), FetchStrategy::OAuth);
    }
}
