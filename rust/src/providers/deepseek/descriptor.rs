//! DeepSeek provider descriptor.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata, ProviderStatusMetadata,
};

pub const DEEPSEEK_ID: ProviderId = ProviderId("deepseek");
pub const DEEPSEEK_ACCENT: &str = "#4D6BFE";

pub fn deepseek_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: DEEPSEEK_ID,
        metadata: ProviderMetadata {
            display_name: "DeepSeek",
            homepage: "https://platform.deepseek.com",
            dashboard_url: Some("https://platform.deepseek.com/usage"),
            status: ProviderStatusMetadata::link("https://status.deepseek.com"),
            session_label: "Balance",
            weekly_label: "Credits",
            supports_opus: false,
            supports_credits: true,
        },
        branding: ProviderBranding::solid(DEEPSEEK_ACCENT, "deepseek"),
        cli: None,
        fetch_plan: ProviderFetchPlan {
            strategies: vec![FetchStrategy::ApiKey],
        },
    }
}
