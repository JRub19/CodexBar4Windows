//! Codex OAuth API path. Wraps wham/usage with tolerant decode and
//! per-window decode-failure flags so a single mistyped field never
//! drops the whole snapshot.

pub mod usage;
pub mod wham_response;
