//! Per-hook context structs for `ProviderImplementation`. Spec 30
//! section 17 lists every hook and the context shape it receives.

use crate::core::ProviderId;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::models::UsageSnapshot;

#[derive(Clone, Debug)]
pub struct ProviderPresentationContext {
    pub provider_id: ProviderId,
    pub snapshot: Option<UsageSnapshot>,
}

#[derive(Clone, Debug)]
pub struct ProviderAvailabilityContext {
    pub provider_id: ProviderId,
    pub winning_strategy: Option<FetchStrategy>,
}

#[derive(Clone, Debug)]
pub struct ProviderSourceLabelContext {
    pub provider_id: ProviderId,
    pub winning_strategy: Option<FetchStrategy>,
}

#[derive(Clone, Debug)]
pub struct ProviderSourceModeContext {
    pub provider_id: ProviderId,
}

#[derive(Clone, Debug)]
pub struct ProviderVersionContext {
    pub provider_id: ProviderId,
}
