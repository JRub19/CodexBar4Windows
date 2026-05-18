//! Venice provider descriptor.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata, ProviderStatusMetadata,
};

pub const VENICE_ID: ProviderId = ProviderId("venice");
pub const VENICE_ACCENT: &str = "#0F172A";

pub fn venice_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: VENICE_ID,
        metadata: ProviderMetadata {
            display_name: "Venice",
            homepage: "https://venice.ai",
            dashboard_url: Some("https://venice.ai/account"),
            status: ProviderStatusMetadata::none(),
            session_label: "Balance",
            weekly_label: "Credits",
            supports_opus: false,
            supports_credits: true,
        },
        branding: ProviderBranding::solid(VENICE_ACCENT, "venice"),
        cli: None,
        fetch_plan: ProviderFetchPlan {
            strategies: vec![FetchStrategy::ApiKey],
        },
    }
}
