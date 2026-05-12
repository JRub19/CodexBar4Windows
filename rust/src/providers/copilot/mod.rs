//! GitHub Copilot provider. Spec 42 §2.
//!
//! Phase 6 ships the descriptor + registration. The OAuth path is
//! based on the device-flow token returned by `gh auth login`; the Web
//! path scrapes the Copilot dashboard. Both ship in follow-up commits.

pub mod descriptor;

use self::descriptor::copilot_descriptor;

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: copilot_descriptor,
    }
}
