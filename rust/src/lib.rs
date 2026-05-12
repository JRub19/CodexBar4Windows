//! CodexBar4Windows shared core crate.
//!
//! Phase 1 onward grows this crate into the providers, settings, secrets,
//! refresh loop, and IPC layer. Phase 0 left a single `version()` function
//! here; we keep it as a sanity-check seam for the desktop shell.

pub mod cookies;
pub mod core;
pub mod locale;
pub mod logging;
pub mod providers;
pub mod redact;
pub mod secrets;
pub mod settings;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }
}
