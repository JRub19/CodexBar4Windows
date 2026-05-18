//! OpenAI API provider for Admin API usage/costs plus credit fallback.

use std::sync::Arc;

use async_trait::async_trait;

use crate::core::ProviderId;
use crate::providers::branding::ProviderBranding;
use crate::providers::common_api::{self, AuthHint, CommonProviderSpec, EndpointSpec};
use crate::providers::descriptor::{
    FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata, ProviderStatusMetadata,
};
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::implementation::ProviderImplementation;
use crate::providers::settings_descriptor::SettingsDescriptor;
use crate::providers::settings_snapshot::ProviderSettingsContribution;

pub const OPENAI_ID: ProviderId = ProviderId("openai");

const ADMIN_SPEC: CommonProviderSpec = CommonProviderSpec {
    id: "openai",
    display_name: "OpenAI API",
    env_vars: &["OPENAI_ADMIN_KEY"],
    auth_hint: AuthHint::Bearer,
    endpoint: EndpointSpec::OpenAiAdmin,
};

const CREDITS_SPEC: CommonProviderSpec = CommonProviderSpec {
    id: "openai",
    display_name: "OpenAI API",
    env_vars: &["OPENAI_API_KEY"],
    auth_hint: AuthHint::Bearer,
    endpoint: EndpointSpec::OpenAiCredits,
};

pub fn descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: OPENAI_ID,
        metadata: ProviderMetadata {
            display_name: "OpenAI API",
            homepage: "https://platform.openai.com",
            dashboard_url: Some("https://platform.openai.com/usage"),
            status: ProviderStatusMetadata::statuspage("https://status.openai.com"),
            session_label: "Spend",
            weekly_label: "Requests",
            supports_opus: false,
            supports_credits: true,
        },
        branding: ProviderBranding::solid("#10A37F", "openai"),
        cli: None,
        fetch_plan: ProviderFetchPlan {
            strategies: vec![FetchStrategy::ApiKey],
        },
    }
}

pub struct OpenAiProvider {
    descriptor: ProviderDescriptor,
}

impl OpenAiProvider {
    pub fn new() -> Self {
        Self {
            descriptor: descriptor(),
        }
    }
}

impl Default for OpenAiProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderImplementation for OpenAiProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        let mut strategies = common_api::strategy(ADMIN_SPEC);
        strategies.extend(common_api::strategy(CREDITS_SPEC));
        strategies
    }
}

pub mod settings {
    use super::*;

    pub fn contribution() -> ProviderSettingsContribution {
        ProviderSettingsContribution {
            provider_id: "openai".into(),
            section_title: "OpenAI API".into(),
            rows: vec![SettingsDescriptor::TokenAccounts {
                title: "OpenAI Admin/API key".into(),
                subtitle: Some(
                    "Use OPENAI_ADMIN_KEY for organization usage/cost graphs, or OPENAI_API_KEY for credit fallback. Stored DPAPI-wrapped on disk.".into(),
                ),
                provider_id: "openai".into(),
            }],
        }
    }
}

inventory::submit! {
    crate::providers::registry::ProviderRegistration {
        descriptor,
    }
}
