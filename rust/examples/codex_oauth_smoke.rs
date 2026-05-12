//! Live smoke test for the Codex OAuth path. Reads
//! `~/.codex/auth.json`, hits `chatgpt.com/backend-api/wham/usage`,
//! and prints the snapshot. Tokens are never echoed.

use std::sync::Arc;

use async_trait::async_trait;
use codexbar::core::ProviderId;
use codexbar::providers::codex::auth::credentials::{auth_path, CodexCredentials};
use codexbar::providers::codex::auth::errors::CodexOAuthError;
use codexbar::providers::codex::oauth::strategy::{CodexOAuthStrategy, OAuthCredentialsResolver};
use codexbar::providers::codex::oauth::transport::ReqwestUsageClient;
use codexbar::providers::fetch_context::{ProviderFetchContext, Runtime, SourceMode};
use codexbar::providers::Strategy;
use codexbar::secrets::token_account::TokenAccountStore;

struct FilesystemResolver;

#[async_trait]
impl OAuthCredentialsResolver for FilesystemResolver {
    async fn resolve(&self) -> Result<CodexCredentials, CodexOAuthError> {
        let path = auth_path().ok_or(CodexOAuthError::CredentialsNotFound)?;
        let bytes = std::fs::read(&path).map_err(|_| CodexOAuthError::CredentialsNotFound)?;
        CodexCredentials::parse(&bytes).map_err(|e| CodexOAuthError::DecodeFailed(e.to_string()))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let http = match ReqwestUsageClient::new() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("reqwest build failed: {e}");
            std::process::exit(1);
        }
    };
    let strategy = CodexOAuthStrategy::new(http, Arc::new(FilesystemResolver));

    let tokens = Arc::new(TokenAccountStore::new(std::env::temp_dir()));
    let context = ProviderFetchContext {
        provider_id: ProviderId("codex"),
        mode: SourceMode::Auto,
        runtime: Runtime { tokens },
    };

    match strategy.fetch(&context).await {
        Ok(snapshot) => {
            println!("=== Codex OAuth snapshot ===");
            println!("  account_token:   {}", snapshot.identity.account_token);
            println!("  email:           {:?}", snapshot.account_email);
            println!("  plan_name:       {:?}", snapshot.plan_name);
            println!("  windows:         {}", snapshot.windows.len());
            for window in &snapshot.windows {
                println!(
                    "    - {:<12} used={:<6.2}% reset_at={:?}",
                    window.window.label, window.window.used, window.window.reset_at_unix_secs,
                );
            }
        }
        Err(err) => {
            eprintln!("Codex OAuth fetch FAILED: {err}");
            std::process::exit(3);
        }
    }
}
