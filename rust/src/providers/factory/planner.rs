//! Consolidate the Factory strategies. The macOS source has a long
//! ladder of cookie/bearer/refresh-token attempts; the Tauri shell
//! resolves a `FactoryCredentials` upstream and we pass it to a single
//! strategy here.

use std::sync::Arc;

use crate::providers::factory::api::strategy::{
    FactoryApiStrategy, FactoryCredentialsResolver, FactoryHttp, FactoryRefreshHook,
};
use crate::providers::fetch_plan_runtime::Strategy;

#[derive(Clone)]
pub struct FactoryWiring {
    pub http: Arc<dyn FactoryHttp>,
    pub credentials: Arc<dyn FactoryCredentialsResolver>,
}

impl FactoryWiring {
    pub fn into_strategies(self) -> Vec<Arc<dyn Strategy>> {
        vec![Arc::new(FactoryApiStrategy::new(self.http, self.credentials)) as Arc<dyn Strategy>]
    }

    /// Install a WorkOS refresh hook so the strategy can trade a
    /// stored refresh_token (or session cookies) for a fresh bearer
    /// instead of surfacing `Unauthorized` when the saved bearer has
    /// expired.
    pub fn into_strategies_with_refresh(
        self,
        refresh: FactoryRefreshHook,
    ) -> Vec<Arc<dyn Strategy>> {
        vec![
            Arc::new(FactoryApiStrategy::new(self.http, self.credentials).with_refresh(refresh))
                as Arc<dyn Strategy>,
        ]
    }
}
