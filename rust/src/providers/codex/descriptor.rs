//! Static descriptor for the Codex provider. Spec 41 section 2 fixes
//! every field; the OAuth API path is the primary strategy, with the
//! local CLI as the fallback. The Web scraping path is reserved for a
//! follow-up phase.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::cli_config::ProviderCLIConfig;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata,
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
            // OAuth API is primary; CLI is the fallback. Web scraping
            // ships in a follow-up so we leave it out for now.
            strategies: vec![FetchStrategy::OAuth, FetchStrategy::CLI],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_advertises_oauth_and_cli_plan() {
        let d = codex_descriptor();
        assert_eq!(
            d.fetch_plan.strategies,
            vec![FetchStrategy::OAuth, FetchStrategy::CLI]
        );
    }

    #[test]
    fn descriptor_supports_credits_not_opus() {
        let d = codex_descriptor();
        assert!(d.metadata.supports_credits);
        assert!(!d.metadata.supports_opus);
    }
}
