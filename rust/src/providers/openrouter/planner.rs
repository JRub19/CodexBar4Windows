//! Consolidate the OpenRouter strategies. API-key only.

use std::sync::Arc;

use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::openrouter::api::strategy::{
    OpenRouterApiStrategy, OpenRouterCredentialsResolver, OpenRouterHttp,
};

#[derive(Clone)]
pub struct OpenRouterWiring {
    pub http: Arc<dyn OpenRouterHttp>,
    pub credentials: Arc<dyn OpenRouterCredentialsResolver>,
}

impl OpenRouterWiring {
    pub fn into_strategies(self) -> Vec<Arc<dyn Strategy>> {
        vec![Arc::new(OpenRouterApiStrategy::new(self.http, self.credentials))
            as Arc<dyn Strategy>]
    }
}
