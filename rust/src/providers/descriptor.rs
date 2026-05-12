//! Provider descriptor. Phase 1 ships the minimum surface needed to type the
//! registry and the IPC DTO. Later phases extend the sub structs in place;
//! every sub struct is `#[non_exhaustive]` so growth does not break callers.

use serde::Serialize;

use crate::core::ProviderId;

#[derive(Clone, Debug, Serialize)]
pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub metadata: ProviderMetadata,
    pub branding: ProviderBranding,
    pub cli: Option<ProviderCLIConfig>,
    pub fetch_plan: ProviderFetchPlan,
}

#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ProviderMetadata {
    pub display_name: &'static str,
    pub homepage: &'static str,
    pub dashboard_url: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ProviderBranding {
    pub accent_hex: &'static str,
    pub icon_id: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ProviderCLIConfig {
    pub binary_name: &'static str,
    pub default_args: &'static [&'static str],
}

/// The ordered list of strategies the refresh loop tries for a provider.
/// Phase 1 keeps the list empty; phase 4 adds the OAuth, Web, and CLI
/// strategies for Claude.
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct ProviderFetchPlan {
    pub strategies: Vec<FetchStrategy>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum FetchStrategy {
    OAuth,
    Web,
    CLI,
    ApiKey,
}
