//! Provider system: descriptors, branding, the inventory!-backed
//! catalog, result models, runtime strategy trait, settings descriptors,
//! and the shared error type.

pub mod augment;
pub mod branding;
pub mod claude;
pub mod cli_config;
pub mod codebuff;
pub mod codex;
pub mod common_api;
pub mod contexts;
pub mod cookie_source;
pub mod copilot;
pub mod cursor;
pub mod deepseek;
pub mod descriptor;
pub mod errors;
pub mod factory;
pub mod fetch_context;
pub mod fetch_outcome;
pub mod fetch_plan_runtime;
pub mod gemini;
pub mod hello;
pub mod identity;
pub mod implementation;
pub mod kimi;
pub mod kimi_k2;
pub mod manus;
pub mod minimax;
pub mod mistral;
pub mod models;
pub mod moonshot;
pub mod openai;
pub mod openrouter;
pub mod presentation;
pub mod registry;
pub mod settings_descriptor;
pub mod settings_snapshot;
pub mod venice;
pub mod zai;

pub use branding::ProviderBranding;
pub use cli_config::ProviderCLIConfig;
pub use cookie_source::{BrowserKind, CookieSource, DEFAULT_BROWSER_IMPORT_ORDER};
pub use descriptor::{FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata};
pub use errors::{ProviderError, ProviderFetchError};
pub use fetch_context::{ProviderFetchContext, Runtime, SourceMode};
pub use fetch_outcome::{ProviderFetchAttempt, ProviderFetchOutcome};
pub use fetch_plan_runtime::{run_pipeline, Strategy, PER_STRATEGY_TIMEOUT};
pub use identity::ProviderIdentitySnapshot;
pub use implementation::{Availability, ProviderImplementation};
pub use models::{
    CreditEvent, CreditUnit, CreditsSnapshot, NamedRateWindow, ProviderCostSnapshot,
    ProviderStorageFootprint, RateWindow, ServiceCost, UsageSnapshot,
};
pub use presentation::{PresentationMetric, ProviderPresentation};
pub use registry::{ProviderCatalog, ProviderRegistration, REGISTRY};
pub use settings_descriptor::{PickerOption, SettingsAction, SettingsDescriptor};
pub use settings_snapshot::{ProviderSettingsContribution, ProviderSettingsSnapshot};

#[macro_export]
macro_rules! simple_windows_provider {
    (
        provider_struct: $provider_struct:ident,
        provider_id_const: $provider_id_const:ident,
        id: $id:literal,
        display_name: $display_name:literal,
        homepage: $homepage:literal,
        dashboard_url: $dashboard_url:expr,
        status: $status:expr,
        accent: $accent:literal,
        icon: $icon:literal,
        session_label: $session_label:literal,
        weekly_label: $weekly_label:literal,
        supports_credits: $supports_credits:expr,
        auth_hint: $auth_hint:expr,
        env_vars: $env_vars:expr,
        endpoint: $endpoint:expr,
        settings_title: $settings_title:literal,
        settings_help: $settings_help:literal
    ) => {
        use std::sync::Arc;

        use async_trait::async_trait;

        use $crate::core::ProviderId;
        use $crate::providers::branding::ProviderBranding;
        use $crate::providers::common_api::{self, CommonProviderSpec};
        use $crate::providers::descriptor::{
            FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata,
        };
        use $crate::providers::fetch_plan_runtime::Strategy;
        use $crate::providers::implementation::ProviderImplementation;
        use $crate::providers::settings_descriptor::SettingsDescriptor;
        use $crate::providers::settings_snapshot::ProviderSettingsContribution;

        pub const $provider_id_const: ProviderId = ProviderId($id);

        const SPEC: CommonProviderSpec = CommonProviderSpec {
            id: $id,
            display_name: $display_name,
            env_vars: $env_vars,
            auth_hint: $auth_hint,
            endpoint: $endpoint,
        };

        pub fn descriptor() -> ProviderDescriptor {
            ProviderDescriptor {
                id: $provider_id_const,
                metadata: ProviderMetadata {
                    display_name: $display_name,
                    homepage: $homepage,
                    dashboard_url: $dashboard_url,
                    status: $status,
                    session_label: $session_label,
                    weekly_label: $weekly_label,
                    supports_opus: false,
                    supports_credits: $supports_credits,
                },
                branding: ProviderBranding::solid($accent, $icon),
                cli: None,
                fetch_plan: ProviderFetchPlan {
                    strategies: vec![match SPEC.endpoint {
                        $crate::providers::common_api::EndpointSpec::AugmentCli => {
                            FetchStrategy::CLI
                        }
                        _ => FetchStrategy::ApiKey,
                    }],
                },
            }
        }

        pub struct $provider_struct {
            descriptor: ProviderDescriptor,
        }

        impl $provider_struct {
            pub fn new() -> Self {
                Self {
                    descriptor: descriptor(),
                }
            }
        }

        impl Default for $provider_struct {
            fn default() -> Self {
                Self::new()
            }
        }

        #[async_trait]
        impl ProviderImplementation for $provider_struct {
            fn descriptor(&self) -> &ProviderDescriptor {
                &self.descriptor
            }

            fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
                common_api::strategy(SPEC)
            }
        }

        pub mod settings {
            use super::*;

            pub fn contribution() -> ProviderSettingsContribution {
                ProviderSettingsContribution {
                    provider_id: $id.into(),
                    section_title: $display_name.into(),
                    rows: vec![SettingsDescriptor::TokenAccounts {
                        title: $settings_title.into(),
                        subtitle: Some($settings_help.into()),
                        provider_id: $id.into(),
                    }],
                }
            }
        }

        inventory::submit! {
            $crate::providers::registry::ProviderRegistration {
                descriptor,
            }
        }
    };
}
