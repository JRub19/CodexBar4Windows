//! Consolidate the Gemini strategies. Currently only the OAuth path is
//! wired; CLI parsing exists in the Swift source as a fallback but is
//! deferred until the Tauri shell side ships a PTY runner.

use std::sync::Arc;

use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::gemini::oauth::strategy::{
    GeminiCredentialsResolver, GeminiOAuthStrategy, GoogleHttp, RefreshHook,
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

    /// Same as `into_strategies` but installs a token-refresh hook so
    /// expired access_tokens are refreshed inline instead of surfacing
    /// `Unauthorized`.
    pub fn into_strategies_with_refresh(
        self,
        refresh: RefreshHook,
    ) -> Vec<Arc<dyn Strategy>> {
        vec![Arc::new(
            GeminiOAuthStrategy::new(self.http, self.credentials).with_refresh(refresh),
        ) as Arc<dyn Strategy>]
    }
}
