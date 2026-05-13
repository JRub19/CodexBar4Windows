//! DeepSeek provider. Ported from
//! `Sources/CodexBarCore/Providers/DeepSeek/DeepSeekUsageFetcher.swift`.

pub mod api;
pub mod descriptor;
pub mod planner;
pub mod settings;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use self::descriptor::deepseek_descriptor;
use self::planner::DeepSeekWiring;
use super::descriptor::ProviderDescriptor;
use super::fetch_plan_runtime::Strategy;
use super::implementation::ProviderImplementation;

pub struct DeepSeekProvider {
    descriptor: ProviderDescriptor,
    wiring: Mutex<Option<Vec<Arc<dyn Strategy>>>>,
}

impl Default for DeepSeekProvider {
    fn default() -> Self {
        Self {
            descriptor: deepseek_descriptor(),
            wiring: Mutex::new(None),
        }
    }
}

impl DeepSeekProvider {
    pub fn install_wiring(&self, wiring: DeepSeekWiring) {
        *self.wiring.lock() = Some(wiring.into_strategies());
    }
}

#[async_trait]
impl ProviderImplementation for DeepSeekProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        self.wiring.lock().clone().unwrap_or_default()
    }
}

inventory::submit! {
    super::registry::ProviderRegistration {
        descriptor: deepseek_descriptor,
    }
}
