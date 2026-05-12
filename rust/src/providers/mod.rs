//! Provider system: descriptors, branding, the inventory!-backed
//! catalog, and the shared error type. Phase 4 P4-01 splits the
//! descriptor file into one module per sub-struct so each can grow
//! independently. Phase 4 P4-02 onward adds the result models, the
//! Strategy trait, and the real provider implementations.

pub mod branding;
pub mod cli_config;
pub mod cookie_source;
pub mod descriptor;
pub mod errors;
pub mod identity;
pub mod registry;

pub use branding::ProviderBranding;
pub use cli_config::ProviderCLIConfig;
pub use cookie_source::{BrowserKind, CookieSource, DEFAULT_BROWSER_IMPORT_ORDER};
pub use descriptor::{FetchStrategy, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata};
pub use errors::ProviderError;
pub use identity::ProviderIdentitySnapshot;
pub use registry::{ProviderCatalog, ProviderRegistration, REGISTRY};
