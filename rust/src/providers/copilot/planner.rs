//! Consolidate the Copilot strategies into the ordered list the
//! framework runtime walks. Phase 6.5 ships the OAuth path only; the
//! GitHub device-code login flow lands later with the Tauri shell.

use std::sync::Arc;

use crate::providers::copilot::oauth::strategy::{
    CopilotCredentialsResolver, CopilotOAuthStrategy, GithubHttp,
};
use crate::providers::fetch_plan_runtime::Strategy;

#[derive(Clone)]
pub struct CopilotWiring {
    pub http: Arc<dyn GithubHttp>,
    pub credentials: Arc<dyn CopilotCredentialsResolver>,
}

impl CopilotWiring {
    pub fn into_strategies(self) -> Vec<Arc<dyn Strategy>> {
        vec![Arc::new(CopilotOAuthStrategy::new(self.http, self.credentials)) as Arc<dyn Strategy>]
    }
}
