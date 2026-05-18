//! Cursor provider descriptor. Spec 42 §1 defines the brand metadata.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::cli_config::ProviderCLIConfig;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata, ProviderStatusMetadata,
};

pub const CURSOR_ID: ProviderId = ProviderId("cursor");
pub const CURSOR_ACCENT_DARK: &str = "#0F172A";
pub const CURSOR_ACCENT_LIGHT: &str = "#000000";

pub fn cursor_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: CURSOR_ID,
        metadata: ProviderMetadata {
            display_name: "Cursor",
            homepage: "https://cursor.com",
            dashboard_url: Some("https://cursor.com/dashboard"),
            status: ProviderStatusMetadata::statuspage("https://status.cursor.com"),
            session_label: "Total",
            weekly_label: "Auto",
            supports_opus: true,
            supports_credits: false,
        },
        branding: ProviderBranding {
            accent_hex: CURSOR_ACCENT_DARK,
            accent_dark_hex: Some(CURSOR_ACCENT_DARK),
            accent_light_hex: Some(CURSOR_ACCENT_LIGHT),
            icon_id: "cursor",
        },
        cli: Some(ProviderCLIConfig::simple("cursor")),
        fetch_plan: ProviderFetchPlan {
            strategies: vec![FetchStrategy::Web],
        },
    }
}
