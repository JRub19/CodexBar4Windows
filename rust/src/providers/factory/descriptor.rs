//! Factory provider descriptor. Spec 42 §5.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata,
};

pub const FACTORY_ID: ProviderId = ProviderId("factory");
pub const FACTORY_ACCENT: &str = "#FF8A50";

pub fn factory_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: FACTORY_ID,
        metadata: ProviderMetadata {
            display_name: "Factory",
            homepage: "https://factory.ai",
            dashboard_url: Some("https://app.factory.ai/usage"),
            session_label: "Daily",
            weekly_label: "Monthly",
            supports_opus: false,
            supports_credits: true,
        },
        branding: ProviderBranding::solid(FACTORY_ACCENT, "factory"),
        cli: None,
        fetch_plan: ProviderFetchPlan {
            strategies: vec![FetchStrategy::Web],
        },
    }
}
