//! Z.ai provider. Ported from
//! `Sources/CodexBarCore/Providers/Zai/ZaiUsageStats.swift`.

pub mod api;
pub mod descriptor;
pub mod planner;
pub mod settings;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use self::descriptor::zai_descriptor;
use self::planner::ZaiWiring;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct ZaiProvider {
    descriptor: ProviderDescriptor,
    wiring: Mutex<Option<Vec<Arc<dyn Strategy>>>>,
}

impl Default for ZaiProvider {
    fn default() -> Self {
        Self {
            descriptor: zai_descriptor(),
            wiring: Mutex::new(None),
        }
    }
}

impl ZaiProvider {
    pub fn install_wiring(&self, wiring: ZaiWiring) {
        *self.wiring.lock() = Some(wiring.into_strategies());
    }
}

#[async_trait]
impl ProviderImplementation for ZaiProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        self.wiring.lock().clone().unwrap_or_default()
    }
}

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: zai_descriptor,
    }
}
