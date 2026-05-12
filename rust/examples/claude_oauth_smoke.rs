//! Live smoke test for the Claude OAuth path. Reads
//! `~/.claude/.credentials.json`, hits `api.anthropic.com/api/oauth/usage`,
//! and prints the snapshot. Tokens are never echoed.

use std::sync::Arc;

use async_trait::async_trait;
use codexbar::core::ProviderId;
use codexbar::providers::claude::errors::CredentialError;
use codexbar::providers::claude::oauth::credentials::{
    default_file_path, resolve, OAuthCredentials, ENV_TOKEN,
};
use codexbar::providers::claude::oauth::strategy::{ClaudeOAuthStrategy, CredentialsResolver};
use codexbar::providers::claude::oauth::transport::ReqwestClient;
use codexbar::providers::fetch_context::{ProviderFetchContext, Runtime, SourceMode};
use codexbar::providers::Strategy;
use codexbar::secrets::token_account::TokenAccountStore;

struct FilesystemResolver;

#[async_trait]
impl CredentialsResolver for FilesystemResolver {
    async fn resolve(&self) -> Result<OAuthCredentials, CredentialError> {
        let env_value = std::env::var(ENV_TOKEN).ok();
        let file_path = default_file_path();
        let resolved = resolve(env_value, None, file_path.as_deref())?;
        Ok(resolved.credentials)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let client = match ReqwestClient::new() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("reqwest build failed: {e}");
            std::process::exit(1);
        }
    };
    let resolver = Arc::new(FilesystemResolver);
    let strategy = ClaudeOAuthStrategy::new(client, resolver);

    let tokens = Arc::new(TokenAccountStore::new(std::env::temp_dir()));
    let context = ProviderFetchContext {
        provider_id: ProviderId("claude"),
        mode: SourceMode::Auto,
        runtime: Runtime { tokens },
    };

    match strategy.fetch(&context).await {
        Ok(snapshot) => {
            println!("=== Claude OAuth snapshot ===");
            println!("  account_token:   {}", snapshot.identity.account_token);
            println!("  email:           {:?}", snapshot.account_email);
            println!("  display_name:    {:?}", snapshot.account_display_name);
            println!("  plan_name:       {:?}", snapshot.plan_name);
            println!("  windows:         {}", snapshot.windows.len());
            for window in &snapshot.windows {
                println!(
                    "    - {:<12} used={:<8.2} allotted={:<8} reset_at={:?}",
                    window.window.label,
                    window.window.used,
                    match window.window.allotted {
                        Some(v) => format!("{v:.0}"),
                        None => "?".into(),
                    },
                    window.window.reset_at_unix_secs,
                );
            }
        }
        Err(err) => {
            eprintln!("Claude OAuth fetch FAILED: {err}");
            std::process::exit(3);
        }
    }
}
