//! Codex CLI integration. Phase 5 Group 2 tasks:
//! - 2.1 binary_locator: find the codex binary.
//! - 2.2 rpc_framer: line-delimited JSON-RPC framing.
//! - 2.4 strategy: account/read + rateLimits/read RPCs.
//!
//! ConPTY /status diagnostic (2.5) and the JobObject lifetime (2.3)
//! reuse the Claude watchdog binary so we do not duplicate process
//! supervision here.

pub mod binary_locator;
pub mod rpc_framer;
