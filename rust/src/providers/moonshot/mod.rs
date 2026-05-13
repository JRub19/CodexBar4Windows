//! Moonshot (Kimi) provider.

pub mod api;
pub mod descriptor;
pub mod planner;
pub mod settings;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use self::descriptor::moonshot_descriptor;
use self::planner::MoonshotWiring;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct MoonshotProvider {
    descriptor: ProviderDescriptor,
    wiring: Mutex<Option<Vec<Arc<dyn Strategy>>>>,
}

impl Default for MoonshotProvider {
    fn default() -> Self {
        Self {
            descriptor: moonshot_descriptor(),
            wiring: Mutex::new(None),
        }
    }
}

impl MoonshotProvider {
    pub fn install_wiring(&self, wiring: MoonshotWiring) {
        *self.wiring.lock() = Some(wiring.into_strategies());
    }
}

#[async_trait]
impl ProviderImplementation for MoonshotProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        self.wiring.lock().clone().unwrap_or_default()
    }
}

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: moonshot_descriptor,
    }
}
