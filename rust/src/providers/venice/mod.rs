//! Venice provider. Ported from
//! `Sources/CodexBarCore/Providers/Venice/VeniceUsageFetcher.swift`.

pub mod api;
pub mod descriptor;
pub mod planner;
pub mod settings;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use self::descriptor::venice_descriptor;
use self::planner::VeniceWiring;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct VeniceProvider {
    descriptor: ProviderDescriptor,
    wiring: Mutex<Option<Vec<Arc<dyn Strategy>>>>,
}

impl Default for VeniceProvider {
    fn default() -> Self {
        Self {
            descriptor: venice_descriptor(),
            wiring: Mutex::new(None),
        }
    }
}

impl VeniceProvider {
    pub fn install_wiring(&self, wiring: VeniceWiring) {
        *self.wiring.lock() = Some(wiring.into_strategies());
    }
}

#[async_trait]
impl ProviderImplementation for VeniceProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        self.wiring.lock().clone().unwrap_or_default()
    }
}

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: venice_descriptor,
    }
}
