//! End-to-end smoke test for the Codex web strategy via the manual
//! paste path. Stores the env-var cookie value in a temp
//! `TokenAccountStore`, then runs the Web strategy against the live
//! chatgpt.com endpoints exactly as the production refresh loop would.
//!
//! Usage:
//!   $env:CODEX_PROBE_COOKIE = "__Secure-next-auth.session-token=<paste>"
//!   cargo run --example codex_manual_paste_smoke --manifest-path rust/Cargo.toml
//!
//! Tokens are never printed.

use std::sync::Arc;

use codexbar::cookies::{CookieAccessGate, CookieHeaderCache, CookieImporter};
use codexbar::core::ProviderId;
use codexbar::providers::claude::web::strategy::WebClient;
use codexbar::providers::claude::web::transport::ReqwestWebClient;
use codexbar::providers::codex::web::cookie_resolver::CodexCookieResolver;
use codexbar::providers::codex::web::strategy::CodexWebStrategy;
use codexbar::providers::fetch_context::{ProviderFetchContext, Runtime, SourceMode};
use codexbar::providers::Strategy;
use codexbar::secrets::token_account::{TokenAccountStore, TokenKind};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cookie = match std::env::var("CODEX_PROBE_COOKIE") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!(
                "Set CODEX_PROBE_COOKIE to a cookie header (e.g. \
                 `__Secure-next-auth.session-token=ey...`) and rerun."
            );
            std::process::exit(2);
        }
    };
    println!("Cookie length: {} bytes (value redacted)", cookie.len());

    let dir = tempfile::tempdir().expect("tempdir");
    let cache = Arc::new(CookieHeaderCache::new(dir.path().join("cache")));
    let gate = Arc::new(CookieAccessGate::new());
    let tokens = Arc::new(TokenAccountStore::new(dir.path().join("tokens")));

    tokens
        .add("codex", TokenKind::Cookie, "manual paste", &cookie)
        .expect("paste");
    println!("Stored manual paste in TokenAccountStore for provider id `codex`.");

    let importer = Arc::new(CookieImporter::new(cache, gate, tokens));
    let resolver = Arc::new(CodexCookieResolver::new(importer));

    let client: Arc<dyn WebClient> = match ReqwestWebClient::new() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("reqwest build failed: {e}");
            std::process::exit(1);
        }
    };
    let strategy = CodexWebStrategy::new(client, resolver);

    let tokens_for_ctx = Arc::new(TokenAccountStore::new(std::env::temp_dir()));
    let context = ProviderFetchContext {
        provider_id: ProviderId("codex"),
        mode: SourceMode::Auto,
        runtime: Runtime {
            tokens: tokens_for_ctx,
        },
    };

    match strategy.fetch(&context).await {
        Ok(snapshot) => {
            println!();
            println!("=== Codex snapshot (manual paste path) ===");
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
            eprintln!();
            eprintln!("Codex web fetch FAILED: {err}");
            std::process::exit(3);
        }
    }
}
