//! Hello sample provider descriptor. Hello is gated behind the
//! `debug.debug_menu_enabled` setting; production installs never see it.

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata,
};

pub const HELLO_ID: ProviderId = ProviderId("hello");

pub fn hello_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: HELLO_ID,
        metadata: ProviderMetadata {
            display_name: "Hello",
            homepage: "https://example.com",
            dashboard_url: None,
            session_label: "Session",
            weekly_label: "Week",
            supports_opus: false,
            supports_credits: false,
        },
        branding: ProviderBranding::solid("#888888", "hello"),
        cli: None,
        fetch_plan: ProviderFetchPlan {
            strategies: vec![FetchStrategy::OAuth],
        },
    }
}
