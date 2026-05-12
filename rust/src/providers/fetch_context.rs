//! Per-attempt context passed to every `Strategy::fetch` call.
//!
//! Strategies pull the `SourceMode` to know whether the user picked Auto
//! (fall through), forced a specific path (OAuth/Web/CLI), or disabled
//! the provider. They consult `Runtime` for the http client and the
//! secret store handles; everything they need to make a network or
//! filesystem call is reachable from this struct.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::ProviderId;
use crate::secrets::token_account::TokenAccountStore;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SourceMode {
    /// Try every strategy in plan order until one succeeds.
    Auto,
    /// Force a single strategy and skip the rest.
    Forced(super::descriptor::FetchStrategy),
    /// Skip this provider this tick.
    Disabled,
}

#[derive(Clone)]
pub struct ProviderFetchContext {
    pub provider_id: ProviderId,
    pub mode: SourceMode,
    pub runtime: Runtime,
}

#[derive(Clone)]
pub struct Runtime {
    pub tokens: Arc<TokenAccountStore>,
}
