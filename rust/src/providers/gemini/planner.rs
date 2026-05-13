//! Consolidate the Gemini strategies. Currently only the OAuth path is
//! wired; CLI parsing exists in the Swift source as a fallback but is
//! deferred until the Tauri shell side ships a PTY runner.

use std::sync::Arc;

use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::gemini::oauth::strategy::{
    GeminiCredentialsResolver, GeminiOAuthStrategy, GoogleHttp,
};

#[derive(Clone)]
pub struct GeminiWiring {
    pub http: Arc<dyn GoogleHttp>,
    pub credentials: Arc<dyn GeminiCredentialsResolver>,
}

impl GeminiWiring {
    pub fn into_strategies(self) -> Vec<Arc<dyn Strategy>> {
        vec![
            Arc::new(GeminiOAuthStrategy::new(self.http, self.credentials)) as Arc<dyn Strategy>
        ]
    }
}
