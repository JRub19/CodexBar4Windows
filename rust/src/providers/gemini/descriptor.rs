//! Gemini provider descriptor. Spec 42 §3.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::cli_config::ProviderCLIConfig;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata,
};

pub const GEMINI_ID: ProviderId = ProviderId("gemini");
pub const GEMINI_ACCENT: &str = "#4796E3";

pub fn gemini_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: GEMINI_ID,
        metadata: ProviderMetadata {
            display_name: "Gemini",
            homepage: "https://aistudio.google.com",
            dashboard_url: Some("https://aistudio.google.com/usage"),
            session_label: "Daily",
            weekly_label: "Quota",
            supports_opus: false,
            supports_credits: false,
        },
        branding: ProviderBranding::solid(GEMINI_ACCENT, "gemini"),
        cli: Some(ProviderCLIConfig {
            binary_name: "gemini",
            default_args: &[],
            extra_search_dirs: &["%LOCALAPPDATA%\\Programs\\gemini", "%APPDATA%\\npm"],
            min_version: Some("0.1.0"),
        }),
        fetch_plan: ProviderFetchPlan {
            strategies: vec![FetchStrategy::CLI],
        },
    }
}
