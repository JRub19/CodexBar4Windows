//! Gemini provider. Spec 42 §3.
//!
//! Phase 6 ships the descriptor. The Gemini CLI reports quota via its
//! own `gemini status --json` command; the ConPTY launcher reuses the
//! Claude-side actor with a different command line.

pub mod descriptor;

use self::descriptor::gemini_descriptor;

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: gemini_descriptor,
    }
}
