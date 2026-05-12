//! Consolidate the three Claude strategies into the ordered list the
//! framework runtime walks. The plan order matches descriptor.rs
//! (OAuth -> Web -> CLI). The planner exposes typed factories so the
//! Tauri shell can construct strategies once at boot, pass the
//! credential resolvers in, and reuse the same instances for every
//! refresh tick.

use std::sync::Arc;

use crate::providers::claude::cli::pty_actor::{CliRunner, RealCliRunner};
use crate::providers::claude::cli::strategy::ClaudeCliStrategy;
use crate::providers::claude::oauth::strategy::{
    ClaudeOAuthStrategy, CredentialsResolver, HttpClient,
};
use crate::providers::claude::web::strategy::{ClaudeWebStrategy, CookieResolver, WebClient};
use crate::providers::fetch_plan_runtime::Strategy;

#[derive(Clone)]
pub struct ClaudeWiring {
    pub oauth_http: Arc<dyn HttpClient>,
    pub oauth_credentials: Arc<dyn CredentialsResolver>,
    pub web_client: Arc<dyn WebClient>,
    pub web_cookies: Arc<dyn CookieResolver>,
    pub cli_runner: Arc<dyn CliRunner>,
    pub cli_binary: String,
}

impl ClaudeWiring {
    pub fn into_strategies(self) -> Vec<Arc<dyn Strategy>> {
        vec![
            Arc::new(ClaudeOAuthStrategy::new(
                self.oauth_http,
                self.oauth_credentials,
            )) as Arc<dyn Strategy>,
            Arc::new(ClaudeWebStrategy::new(self.web_client, self.web_cookies)),
            Arc::new(ClaudeCliStrategy::new(self.cli_runner, self.cli_binary)),
        ]
    }
}

/// Default CLI runner: spawn the real `claude` binary via ConPTY.
pub fn default_cli_runner() -> Arc<dyn CliRunner> {
    Arc::new(RealCliRunner)
}
