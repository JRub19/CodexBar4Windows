//! OpenRouter provider. Spec 42 §4.
//!
//! Phase 6 ships the descriptor. OpenRouter is API-key only; the user
//! pastes a `sk-or-v1-...` token in Settings and the strategy hits
//! `https://openrouter.ai/api/v1/credits` and `/auth/key`.

pub mod descriptor;

use self::descriptor::openrouter_descriptor;

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: openrouter_descriptor,
    }
}
