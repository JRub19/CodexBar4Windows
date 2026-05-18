//! Moonshot (Kimi) provider descriptor.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata, ProviderStatusMetadata,
};

pub const MOONSHOT_ID: ProviderId = ProviderId("moonshot");
pub const MOONSHOT_ACCENT: &str = "#222831";

pub fn moonshot_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: MOONSHOT_ID,
        metadata: ProviderMetadata {
            display_name: "Moonshot",
            homepage: "https://platform.moonshot.ai",
            dashboard_url: Some("https://platform.moonshot.ai/console"),
            status: ProviderStatusMetadata::none(),
            session_label: "Balance",
            weekly_label: "Credits",
            supports_opus: false,
            supports_credits: true,
        },
        branding: ProviderBranding::solid(MOONSHOT_ACCENT, "moonshot"),
        cli: None,
        fetch_plan: ProviderFetchPlan {
            strategies: vec![FetchStrategy::ApiKey],
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoonshotRegion {
    International,
    China,
}

impl MoonshotRegion {
    pub fn api_base(self) -> &'static str {
        match self {
            MoonshotRegion::International => "https://api.moonshot.ai",
            MoonshotRegion::China => "https://api.moonshot.cn",
        }
    }

    pub fn balance_url(self) -> String {
        format!("{}/v1/users/me/balance", self.api_base())
    }
}
