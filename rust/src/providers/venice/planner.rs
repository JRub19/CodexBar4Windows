use std::sync::Arc;

use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::venice::api::strategy::{
    VeniceApiStrategy, VeniceCredentialsResolver, VeniceHttp,
};

#[derive(Clone)]
pub struct VeniceWiring {
    pub http: Arc<dyn VeniceHttp>,
    pub credentials: Arc<dyn VeniceCredentialsResolver>,
}

impl VeniceWiring {
    pub fn into_strategies(self) -> Vec<Arc<dyn Strategy>> {
        vec![Arc::new(VeniceApiStrategy::new(self.http, self.credentials)) as Arc<dyn Strategy>]
    }
}
