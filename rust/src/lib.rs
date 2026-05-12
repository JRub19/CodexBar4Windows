//! CodexBar4Windows shared core crate.
//!
//! Placeholder for Phase 0. Phase 1 grows this into the providers, settings,
//! secrets, refresh loop, and IPC layer.

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
