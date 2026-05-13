//! Gemini provider. OAuth path ported from
//! `Sources/CodexBarCore/Providers/Gemini/GeminiStatusProbe.swift`.

pub mod descriptor;
pub mod oauth;
pub mod planner;
pub mod settings;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use self::descriptor::gemini_descriptor;
use self::planner::GeminiWiring;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct GeminiProvider {
    descriptor: ProviderDescriptor,
    wiring: Mutex<Option<Vec<Arc<dyn Strategy>>>>,
}

impl Default for GeminiProvider {
    fn default() -> Self {
        Self {
            descriptor: gemini_descriptor(),
            wiring: Mutex::new(None),
        }
    }
}

impl GeminiProvider {
    pub fn install_wiring(&self, wiring: GeminiWiring) {
        *self.wiring.lock() = Some(wiring.into_strategies());
    }

    pub fn install_wiring_with_refresh(
        &self,
        wiring: GeminiWiring,
        refresh: self::oauth::strategy::RefreshHook,
    ) {
        *self.wiring.lock() = Some(wiring.into_strategies_with_refresh(refresh));
    }
}

#[async_trait]
impl ProviderImplementation for GeminiProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        self.wiring.lock().clone().unwrap_or_default()
    }
}

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: gemini_descriptor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::descriptor::FetchStrategy;
    use crate::providers::errors::ProviderFetchError;
    use crate::providers::gemini::oauth::credentials::GeminiAuthType;
    use crate::providers::gemini::oauth::strategy::{
        GeminiCredentialsResolver, GeminiCredentialsState, GoogleHttp, GoogleResponse, HttpMethod,
    };

    struct NoopHttp;
    #[async_trait]
    impl GoogleHttp for NoopHttp {
        async fn request(
            &self,
            _: HttpMethod,
            _: &str,
            _: &str,
            _: Option<&[u8]>,
        ) -> Result<GoogleResponse, ProviderFetchError> {
            Ok(GoogleResponse {
                status: 404,
                body: b"{}".to_vec(),
            })
        }
    }
    struct NoopResolver;
    #[async_trait]
    impl GeminiCredentialsResolver for NoopResolver {
        async fn resolve(&self) -> Result<GeminiCredentialsState, ProviderFetchError> {
            Ok(GeminiCredentialsState {
                auth_type: GeminiAuthType::Unknown,
                credentials: None,
            })
        }
    }

    #[test]
    fn strategies_are_empty_until_wiring_is_installed() {
        let provider = GeminiProvider::default();
        assert!(provider.strategies().is_empty());
        assert_eq!(provider.descriptor().id.as_str(), "gemini");
    }

    #[test]
    fn install_wiring_yields_single_oauth_strategy() {
        let provider = GeminiProvider::default();
        provider.install_wiring(GeminiWiring {
            http: Arc::new(NoopHttp),
            credentials: Arc::new(NoopResolver),
        });
        let strategies = provider.strategies();
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].strategy_id(), FetchStrategy::OAuth);
    }
}
