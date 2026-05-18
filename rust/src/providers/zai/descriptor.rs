//! Z.ai provider descriptor.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata, ProviderStatusMetadata,
};

pub const ZAI_ID: ProviderId = ProviderId("zai");
pub const ZAI_ACCENT: &str = "#1F2937";

pub fn zai_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: ZAI_ID,
        metadata: ProviderMetadata {
            display_name: "Z.ai",
            homepage: "https://z.ai",
            dashboard_url: Some("https://z.ai/manage-apikey/usage"),
            status: ProviderStatusMetadata::none(),
            session_label: "Tokens",
            weekly_label: "Window",
            supports_opus: false,
            supports_credits: false,
        },
        branding: ProviderBranding::solid(ZAI_ACCENT, "zai"),
        cli: None,
        fetch_plan: ProviderFetchPlan {
            strategies: vec![FetchStrategy::ApiKey],
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZaiRegion {
    Global,
    BigmodelCN,
}

impl ZaiRegion {
    pub fn base_url(self) -> &'static str {
        match self {
            ZaiRegion::Global => "https://api.z.ai",
            ZaiRegion::BigmodelCN => "https://open.bigmodel.cn",
        }
    }

    pub fn quota_url(self) -> String {
        format!("{}/api/monitor/usage/quota/limit", self.base_url())
    }
}
