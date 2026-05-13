use std::sync::Arc;

use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::moonshot::api::strategy::{
    MoonshotApiStrategy, MoonshotCredentialsResolver, MoonshotHttp,
};

#[derive(Clone)]
pub struct MoonshotWiring {
    pub http: Arc<dyn MoonshotHttp>,
    pub credentials: Arc<dyn MoonshotCredentialsResolver>,
}

impl MoonshotWiring {
    pub fn into_strategies(self) -> Vec<Arc<dyn Strategy>> {
        vec![Arc::new(MoonshotApiStrategy::new(self.http, self.credentials)) as Arc<dyn Strategy>]
    }
}
