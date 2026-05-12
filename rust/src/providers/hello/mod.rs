//! Hello sample provider. Phase 4 P4-08 ships this so the framework has
//! at least one provider exercised end to end. Hello is debug-only; the
//! Tauri shell guards registration behind `Settings.debug.debug_menu_enabled`.

pub mod descriptor;
pub mod strategies;

use std::sync::Arc;

use async_trait::async_trait;

use self::descriptor::hello_descriptor;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct HelloProvider {
    descriptor: ProviderDescriptor,
}

impl Default for HelloProvider {
    fn default() -> Self {
        Self {
            descriptor: hello_descriptor(),
        }
    }
}

#[async_trait]
impl ProviderImplementation for HelloProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        strategies::strategies()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ProviderId;
    use crate::providers::fetch_context::{ProviderFetchContext, Runtime, SourceMode};
    use crate::secrets::token_account::TokenAccountStore;

    fn ctx() -> ProviderFetchContext {
        let tokens = Arc::new(TokenAccountStore::new(std::env::temp_dir()));
        ProviderFetchContext {
            provider_id: ProviderId("hello"),
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    #[test]
    fn hello_returns_static_snapshot() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let provider = HelloProvider::default();
        let outcome = rt.block_on(async { provider.refresh(&ctx()).await });
        let snap = outcome.snapshot.expect("hello must return a snapshot");
        assert_eq!(snap.identity.provider_id, "hello");
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].window.used, 25.0);
    }
}
