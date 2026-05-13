//! Provider system: descriptors, branding, the inventory!-backed
//! catalog, and the shared error type. Phase 4 P4-01 splits the
//! descriptor file into one module per sub-struct so each can grow
//! independently. Phase 4 P4-02 onward adds the result models, the
//! Strategy trait, and the real provider implementations.

pub mod branding;
pub mod claude;
pub mod cli_config;
pub mod codex;
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
pub mod models;
pub mod moonshot;
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
