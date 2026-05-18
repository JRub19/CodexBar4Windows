//! Provider descriptor.
//!
//! The descriptor is the immutable, statically-known shape of a provider.
//! All runtime state (tokens, cookies, last refresh, etc.) lives in
//! `ProviderImplementation` and downstream stores. Every field here is
//! data the React UI can render before any network call lands.

use serde::{Deserialize, Serialize};

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::cli_config::ProviderCLIConfig;

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
    pub status: ProviderStatusMetadata,
    /// Right-side caption shown above the session bar in the popup.
    pub session_label: &'static str,
    /// Right-side caption shown above the weekly bar in the popup.
    pub weekly_label: &'static str,
    /// Some providers expose a separate Opus quota window (Claude does).
    pub supports_opus: bool,
    /// Whether the provider has a credits balance display in addition to
    /// rate windows.
    pub supports_credits: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[non_exhaustive]
pub struct ProviderStatusMetadata {
    pub status_page_url: Option<&'static str>,
    pub feed: Option<ProviderStatusFeed>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProviderStatusFeed {
    Statuspage { base_url: &'static str },
    GoogleWorkspace { product_id: &'static str },
}

impl ProviderStatusMetadata {
    pub const fn none() -> Self {
        Self {
            status_page_url: None,
            feed: None,
        }
    }

    pub const fn link(status_page_url: &'static str) -> Self {
        Self {
            status_page_url: Some(status_page_url),
            feed: None,
        }
    }

    pub const fn statuspage(base_url: &'static str) -> Self {
        Self {
            status_page_url: Some(base_url),
            feed: Some(ProviderStatusFeed::Statuspage { base_url }),
        }
    }

    pub const fn google_workspace(product_id: &'static str, status_page_url: &'static str) -> Self {
        Self {
            status_page_url: Some(status_page_url),
            feed: Some(ProviderStatusFeed::GoogleWorkspace { product_id }),
        }
    }
}

impl ProviderMetadata {
    pub const fn minimal(display_name: &'static str, homepage: &'static str) -> Self {
        Self {
            display_name,
            homepage,
            dashboard_url: None,
            status: ProviderStatusMetadata::none(),
            session_label: "Session",
            weekly_label: "Week",
            supports_opus: false,
            supports_credits: false,
        }
    }
}

/// The ordered list of strategies the refresh loop tries for a provider.
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct ProviderFetchPlan {
    pub strategies: Vec<FetchStrategy>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FetchStrategy {
    OAuth,
    Web,
    CLI,
    ApiKey,
}
