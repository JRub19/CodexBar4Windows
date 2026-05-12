//! Static Claude provider descriptor. Spec 40 section 2 documents every
//! field. The descriptor only changes when a Claude.ai-facing surface
//! changes upstream; runtime state lives in the strategy modules.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::cli_config::ProviderCLIConfig;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata,
};

pub const CLAUDE_ID: ProviderId = ProviderId("claude");

/// The canonical accent hex matches Anthropic's brand sheet, dark mode
/// variant. Light mode uses a darker shade so the icon stays legible on
/// the white Windows 11 light taskbar.
pub const CLAUDE_ACCENT_DARK: &str = "#D97757";
pub const CLAUDE_ACCENT_LIGHT: &str = "#A65A3F";

pub fn claude_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: CLAUDE_ID,
        metadata: ProviderMetadata {
            display_name: "Claude",
            homepage: "https://claude.ai",
            dashboard_url: Some("https://claude.ai/settings/billing"),
            session_label: "Session",
            weekly_label: "Week",
            supports_opus: true,
            supports_credits: false,
        },
        branding: ProviderBranding {
            accent_hex: CLAUDE_ACCENT_DARK,
            accent_dark_hex: Some(CLAUDE_ACCENT_DARK),
            accent_light_hex: Some(CLAUDE_ACCENT_LIGHT),
            icon_id: "claude",
        },
        cli: Some(ProviderCLIConfig {
            binary_name: "claude",
            default_args: &[],
            extra_search_dirs: &[
                // The Claude CLI is shipped as an npm global; the default
                // install on Windows lives under %APPDATA%\npm.
                "%APPDATA%\\npm",
            ],
            min_version: Some("1.0.0"),
        }),
        fetch_plan: ProviderFetchPlan {
            strategies: vec![FetchStrategy::OAuth, FetchStrategy::Web, FetchStrategy::CLI],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_lists_oauth_web_cli_in_order() {
        let d = claude_descriptor();
        assert_eq!(
            d.fetch_plan.strategies,
            vec![FetchStrategy::OAuth, FetchStrategy::Web, FetchStrategy::CLI]
        );
    }

    #[test]
    fn theme_aware_accents_resolve_correctly() {
        let d = claude_descriptor();
        assert_eq!(d.branding.accent_for_theme(true), CLAUDE_ACCENT_DARK);
        assert_eq!(d.branding.accent_for_theme(false), CLAUDE_ACCENT_LIGHT);
    }

    #[test]
    fn supports_opus_window() {
        assert!(claude_descriptor().metadata.supports_opus);
    }
}
