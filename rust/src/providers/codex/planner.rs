//! Consolidate the Codex OAuth and CLI strategies into the ordered list
//! the framework runtime walks. The Tauri shell hands a `CodexWiring`
//! to `CodexProvider::install_wiring` once at boot; the same instances
//! are reused for every refresh tick.

use std::sync::Arc;

use crate::providers::codex::cli::strategy::{CodexCliStrategy, TransportFactory};
use crate::providers::fetch_plan_runtime::Strategy;

#[derive(Clone)]
pub struct CodexWiring {
    pub cli_transport_factory: Arc<dyn TransportFactory>,
    // OAuth strategy wiring lives behind a follow-up commit because the
    // refresh-aware UsageHttp adapter wants the live secrets path. For
    // now the planner exposes the CLI strategy only; that is enough to
    // serve a logged-in user with the codex binary on PATH.
}

impl CodexWiring {
    pub fn into_strategies(self) -> Vec<Arc<dyn Strategy>> {
        vec![Arc::new(CodexCliStrategy::new(self.cli_transport_factory))]
    }
}
