//! OpenRouter provider descriptor. Spec 42 §4.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata, ProviderStatusMetadata,
};

pub const OPENROUTER_ID: ProviderId = ProviderId("openrouter");
pub const OPENROUTER_ACCENT: &str = "#7C3AED";

pub fn openrouter_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: OPENROUTER_ID,
        metadata: ProviderMetadata {
            display_name: "OpenRouter",
            homepage: "https://openrouter.ai",
            dashboard_url: Some("https://openrouter.ai/activity"),
            status: ProviderStatusMetadata::link("https://status.openrouter.ai"),
            session_label: "Day",
            weekly_label: "Credits",
            supports_opus: false,
            supports_credits: true,
        },
        branding: ProviderBranding::solid(OPENROUTER_ACCENT, "openrouter"),
        cli: None,
        fetch_plan: ProviderFetchPlan {
            // OpenRouter uses API-key auth exclusively; no OAuth, no web cookies.
            strategies: vec![FetchStrategy::ApiKey],
        },
    }
}
