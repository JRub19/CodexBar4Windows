//! Claude provider. Phase 4 commits land here in order: P4-10
//! descriptor only, P4-11 OAuth credential discovery, P4-12 OAuth fetch
//! strategy, P4-13 Web strategy, P4-14 multi-account routing, P4-16 CLI
//! PTY strategy, P4-18 source consolidation, P4-19 settings, P4-20
//! tray and popup wiring.

pub mod descriptor;
pub mod errors;
pub mod models;
pub mod oauth;
pub mod routing;
pub mod tokens;
pub mod web;

use std::sync::Arc;

use async_trait::async_trait;

use self::descriptor::claude_descriptor;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct ClaudeProvider {
    descriptor: ProviderDescriptor,
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self {
            descriptor: claude_descriptor(),
        }
    }
}

#[async_trait]
impl ProviderImplementation for ClaudeProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        // Phase 4 P4-12 onward registers concrete strategies here. For
        // P4-10 we ship descriptor-only, so the provider compiles but
        // reports as unavailable until subsequent commits land.
        Vec::new()
    }
}

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: claude_descriptor,
    }
}
