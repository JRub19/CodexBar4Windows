//! Codex provider. Phase 5 lands the OAuth API path, the CLI JSON-RPC
//! integration, and the history ownership scheme. Web scraping
//! (chatgpt.com cookies + WebView2 fallback) and the multi-account
//! promotion flow ship in a follow-up because both require live OpenAI
//! sessions to verify safely.

pub mod auth;
pub mod cli;
pub mod oauth;
