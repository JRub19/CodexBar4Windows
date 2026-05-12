//! Cursor provider. Spec 42 §1.
//!
//! Phase 6 ships the descriptor + registry registration. Strategy
//! implementations (cookie import, /api/usage-summary fold, six-rule
//! precedence ladder for the headline percent) land in follow-ups.

pub mod descriptor;

use self::descriptor::cursor_descriptor;

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: cursor_descriptor,
    }
}
