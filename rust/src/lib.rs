//! CodexBar4Windows shared core crate.
//!
//! Houses the provider framework, settings, secrets, refresh loop, usage
//! store, renderer helpers, and packaging-facing version seam shared by the
//! desktop shell and helper binaries.

pub mod cookies;
pub mod core;
pub mod cost;
pub mod locale;
pub mod logging;
pub mod notifications;
pub mod projection;
pub mod providers;
pub mod redact;
pub mod renderer;
pub mod secrets;
pub mod settings;
pub mod status;

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
