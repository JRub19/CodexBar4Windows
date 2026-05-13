use std::sync::Arc;

use crate::providers::deepseek::api::strategy::{
    DeepSeekApiStrategy, DeepSeekCredentialsResolver, DeepSeekHttp,
};
use crate::providers::fetch_plan_runtime::Strategy;

#[derive(Clone)]
pub struct DeepSeekWiring {
    pub http: Arc<dyn DeepSeekHttp>,
    pub credentials: Arc<dyn DeepSeekCredentialsResolver>,
}

impl DeepSeekWiring {
    pub fn into_strategies(self) -> Vec<Arc<dyn Strategy>> {
        vec![Arc::new(DeepSeekApiStrategy::new(self.http, self.credentials)) as Arc<dyn Strategy>]
    }
}
