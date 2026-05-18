//! Static descriptor for the Codex provider. Spec 41 section 2 fixes
//! every field. Strategy order: OAuth API → Web (chatgpt.com cookies)
//! → CLI binary fallback.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::cli_config::ProviderCLIConfig;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata, ProviderStatusMetadata,
};

pub const CODEX_ID: ProviderId = ProviderId("codex");

/// OpenAI brand teal, matched against the macOS reference asset.
pub const CODEX_ACCENT: &str = "#10A37F";

pub fn codex_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: CODEX_ID,
        metadata: ProviderMetadata {
            display_name: "Codex",
            homepage: "https://platform.openai.com",
            dashboard_url: Some("https://platform.openai.com/usage"),
            status: ProviderStatusMetadata::statuspage("https://status.openai.com"),
            session_label: "Session",
            weekly_label: "Week",
            supports_opus: false,
            supports_credits: true,
        },
        branding: ProviderBranding::solid(CODEX_ACCENT, "codex"),
        cli: Some(ProviderCLIConfig {
            binary_name: "codex",
            default_args: &[],
            extra_search_dirs: &[
                "%LOCALAPPDATA%\\Programs\\codex",
                "%USERPROFILE%\\.bun\\bin",
            ],
            min_version: Some("0.1.0"),
        }),
        fetch_plan: ProviderFetchPlan {
            // OAuth API → Web (chatgpt.com cookies) → CLI fallback.
            strategies: vec![FetchStrategy::OAuth, FetchStrategy::Web, FetchStrategy::CLI],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_advertises_oauth_web_cli_plan_in_order() {
        let d = codex_descriptor();
        assert_eq!(
            d.fetch_plan.strategies,
            vec![FetchStrategy::OAuth, FetchStrategy::Web, FetchStrategy::CLI]
        );
    }

    #[test]
    fn descriptor_supports_credits_not_opus() {
        let d = codex_descriptor();
        assert!(d.metadata.supports_credits);
        assert!(!d.metadata.supports_opus);
    }
}
