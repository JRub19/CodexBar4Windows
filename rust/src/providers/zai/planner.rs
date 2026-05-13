use std::sync::Arc;

use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::zai::api::strategy::{ZaiApiStrategy, ZaiCredentialsResolver, ZaiHttp};

#[derive(Clone)]
pub struct ZaiWiring {
    pub http: Arc<dyn ZaiHttp>,
    pub credentials: Arc<dyn ZaiCredentialsResolver>,
}

impl ZaiWiring {
    pub fn into_strategies(self) -> Vec<Arc<dyn Strategy>> {
        vec![Arc::new(ZaiApiStrategy::new(self.http, self.credentials)) as Arc<dyn Strategy>]
    }
}
