//! GitHub Copilot provider descriptor. Spec 42 §2.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::cli_config::ProviderCLIConfig;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata,
};

pub const COPILOT_ID: ProviderId = ProviderId("copilot");
pub const COPILOT_ACCENT: &str = "#24292F";

pub fn copilot_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: COPILOT_ID,
        metadata: ProviderMetadata {
            display_name: "GitHub Copilot",
            homepage: "https://github.com/features/copilot",
            dashboard_url: Some("https://github.com/settings/copilot"),
            session_label: "Premium",
            weekly_label: "Monthly",
            supports_opus: false,
            supports_credits: false,
        },
        branding: ProviderBranding::solid(COPILOT_ACCENT, "copilot"),
        cli: Some(ProviderCLIConfig::simple("gh")),
        fetch_plan: ProviderFetchPlan {
            strategies: vec![FetchStrategy::OAuth, FetchStrategy::Web],
        },
    }
}
