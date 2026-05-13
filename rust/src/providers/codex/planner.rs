//! Consolidate the Codex OAuth, Web, and CLI strategies into the
//! ordered list the framework runtime walks. The Tauri shell hands a
//! `CodexWiring` to `CodexProvider::install_wiring` once at boot.

use std::sync::Arc;

use crate::providers::claude::web::strategy::{CookieResolver, WebClient};
use crate::providers::codex::cli::strategy::{CodexCliStrategy, TransportFactory};
use crate::providers::codex::cli::tui_strategy::{CodexTuiRunner, CodexTuiStrategy};
use crate::providers::codex::oauth::strategy::{CodexOAuthStrategy, OAuthCredentialsResolver};
use crate::providers::codex::oauth::usage::UsageHttp;
use crate::providers::codex::web::strategy::CodexWebStrategy;
use crate::providers::fetch_plan_runtime::Strategy;

#[derive(Clone)]
pub struct CodexWiring {
    /// HTTP transport for the OAuth strategy. Requires the
    /// `codex_cli_rs/<version>` User-Agent — see `oauth/transport.rs`.
    pub oauth_http: Arc<dyn UsageHttp>,
    /// Resolves the on-disk `~/.codex/auth.json` (or env override).
    pub oauth_credentials: Arc<dyn OAuthCredentialsResolver>,
    /// HTTP transport for the Web strategy (chatgpt.com cookies).
    pub web_client: Arc<dyn WebClient>,
    /// Cookie resolver: cookie cache → manual paste → browser import.
    pub web_cookies: Arc<dyn CookieResolver>,
    /// JSON-RPC transport factory for the CLI strategy.
    pub cli_transport_factory: Arc<dyn TransportFactory>,
}

impl CodexWiring {
    pub fn into_strategies(self) -> Vec<Arc<dyn Strategy>> {
        vec![
            Arc::new(CodexOAuthStrategy::new(
                self.oauth_http,
                self.oauth_credentials,
            )) as Arc<dyn Strategy>,
            Arc::new(CodexWebStrategy::new(self.web_client, self.web_cookies)),
            Arc::new(CodexCliStrategy::new(self.cli_transport_factory)),
        ]
    }

    /// Same as `into_strategies` but replaces the JSON-RPC CLI
    /// strategy with the TUI scraper. Use this when a real `codex`
    /// binary is on disk — real codex does not speak JSON-RPC.
    pub fn into_strategies_with_tui(
        self,
        tui_runner: Arc<dyn CodexTuiRunner>,
        binary: String,
    ) -> Vec<Arc<dyn Strategy>> {
        vec![
            Arc::new(CodexOAuthStrategy::new(
                self.oauth_http,
                self.oauth_credentials,
            )) as Arc<dyn Strategy>,
            Arc::new(CodexWebStrategy::new(self.web_client, self.web_cookies)),
            Arc::new(CodexTuiStrategy::new(tui_runner, binary)),
        ]
    }
}
