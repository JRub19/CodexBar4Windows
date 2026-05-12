//! Live smoke test for the Codex web strategy.
//!
//! Usage:
//!   1. In Brave/Chrome DevTools (F12) open Application → Cookies →
//!      https://chatgpt.com and copy the value of
//!      `__Secure-next-auth.session-token`.
//!   2. Set the env var:
//!        $env:CODEX_PROBE_COOKIE = "__Secure-next-auth.session-token=<paste>"
//!      You can include multiple cookies separated by `; `.
//!   3. Run:
//!        cargo run --example codex_web_smoke --manifest-path rust/Cargo.toml
//!
//! The test calls /backend-api/me and /backend-api/wham/usage and
//! prints the decoded snapshot. Tokens are never echoed verbatim.

use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use codexbar::core::ProviderId;
use codexbar::providers::claude::web::strategy::{CookieResolver, WebClient};
use codexbar::providers::claude::web::transport::ReqwestWebClient;
use codexbar::providers::codex::web::strategy::CodexWebStrategy;
use codexbar::providers::errors::ProviderFetchError;
use codexbar::providers::fetch_context::{ProviderFetchContext, Runtime, SourceMode};
use codexbar::secrets::token_account::TokenAccountStore;

struct StaticCookie(Mutex<Option<String>>);

#[async_trait]
impl CookieResolver for StaticCookie {
    async fn cookie(&self) -> Result<Option<String>, ProviderFetchError> {
        Ok(self.0.lock().unwrap().clone())
    }
    async fn invalidate(&self) -> Result<(), ProviderFetchError> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cookie = match std::env::var("CODEX_PROBE_COOKIE") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "Set CODEX_PROBE_COOKIE to a cookie header value (e.g. \
                 `__Secure-next-auth.session-token=ey...`) and rerun."
            );
            std::process::exit(2);
        }
    };
    println!("Cookie length: {} bytes (value redacted)", cookie.len());

    let client: Arc<dyn WebClient> = match ReqwestWebClient::new() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("reqwest build failed: {e}");
            std::process::exit(1);
        }
    };
    let cookies: Arc<dyn CookieResolver> = Arc::new(StaticCookie(Mutex::new(Some(cookie))));
    let strategy = CodexWebStrategy::new(client, cookies);

    let tokens = Arc::new(TokenAccountStore::new(std::env::temp_dir()));
    let context = ProviderFetchContext {
        provider_id: ProviderId("codex"),
        mode: SourceMode::Auto,
        runtime: Runtime { tokens },
    };

    use codexbar::providers::Strategy;
    match strategy.fetch(&context).await {
        Ok(snapshot) => {
            println!();
            println!("=== Codex snapshot ===");
            println!("  account_token:   {}", snapshot.identity.account_token);
            println!("  email:           {:?}", snapshot.account_email);
            println!("  display_name:    {:?}", snapshot.account_display_name);
            println!("  plan_name:       {:?}", snapshot.plan_name);
            println!("  captured_at:     {}", snapshot.captured_at_unix_secs);
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
            eprintln!();
            eprintln!("Codex web fetch FAILED: {err}");
            std::process::exit(3);
        }
    }
}
