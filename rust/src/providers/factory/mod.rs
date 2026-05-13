//! Factory provider. API/cookie path ported from
//! `Sources/CodexBarCore/Providers/Factory/FactoryStatusProbe.swift`.

pub mod api;
pub mod descriptor;
pub mod planner;
pub mod settings;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use self::descriptor::factory_descriptor;
use self::planner::FactoryWiring;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct FactoryProvider {
    descriptor: ProviderDescriptor,
    wiring: Mutex<Option<Vec<Arc<dyn Strategy>>>>,
}

impl Default for FactoryProvider {
    fn default() -> Self {
        Self {
            descriptor: factory_descriptor(),
            wiring: Mutex::new(None),
        }
    }
}

impl FactoryProvider {
    pub fn install_wiring(&self, wiring: FactoryWiring) {
        *self.wiring.lock() = Some(wiring.into_strategies());
    }

    pub fn install_wiring_with_refresh(
        &self,
        wiring: FactoryWiring,
        refresh: self::api::strategy::FactoryRefreshHook,
    ) {
        *self.wiring.lock() = Some(wiring.into_strategies_with_refresh(refresh));
    }
}

#[async_trait]
impl ProviderImplementation for FactoryProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        self.wiring.lock().clone().unwrap_or_default()
    }
}

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: factory_descriptor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::descriptor::FetchStrategy;
    use crate::providers::errors::ProviderFetchError;
    use crate::providers::factory::api::strategy::{
        FactoryCredentials, FactoryCredentialsResolver, FactoryHttp, FactoryResponse,
    };

    struct NoopHttp;
    #[async_trait]
    impl FactoryHttp for NoopHttp {
        async fn get(
            &self,
            _: &str,
            _: &[(&str, &str)],
        ) -> Result<FactoryResponse, ProviderFetchError> {
            Ok(FactoryResponse {
                status: 200,
                body: b"{}".to_vec(),
            })
        }
    }
    struct NoopCreds;
    #[async_trait]
    impl FactoryCredentialsResolver for NoopCreds {
        async fn resolve(&self) -> Result<FactoryCredentials, ProviderFetchError> {
            Ok(FactoryCredentials::default())
        }
    }

    #[test]
    fn strategies_are_empty_until_wiring_is_installed() {
        let provider = FactoryProvider::default();
        assert!(provider.strategies().is_empty());
        assert_eq!(provider.descriptor().id.as_str(), "factory");
    }

    #[test]
    fn install_wiring_yields_single_oauth_strategy() {
        let provider = FactoryProvider::default();
        provider.install_wiring(FactoryWiring {
            http: Arc::new(NoopHttp),
            credentials: Arc::new(NoopCreds),
        });
        let strategies = provider.strategies();
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].strategy_id(), FetchStrategy::OAuth);
    }
}
