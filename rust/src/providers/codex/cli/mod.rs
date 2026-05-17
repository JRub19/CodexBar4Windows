//! Codex CLI integration. The shipped path locates the real `codex` binary
//! and scrapes its interactive TUI. The JSON-RPC framing modules are kept for
//! a future protocol mode if Codex exposes one.

pub mod binary_locator;
pub mod conpty_transport;
pub mod rpc_client;
pub mod rpc_framer;
pub mod strategy;
pub mod tui_parser;
pub mod tui_strategy;
