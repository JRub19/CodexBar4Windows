//! Factory provider. Spec 42 §5.
//!
//! Phase 6 ships the descriptor. Factory uses an in-app session
//! cookie shared with the OpenAI cookie pipeline; the Web strategy
//! lands in a follow-up.

pub mod descriptor;

use self::descriptor::factory_descriptor;

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: factory_descriptor,
    }
}
