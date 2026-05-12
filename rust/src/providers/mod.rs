//! Provider system: descriptors, the `inventory!`-backed catalog, and the
//! shared error type. Phase 1 ships zero provider implementations; the
//! catalog is iterable, addressable by id, and serializable across IPC.

pub mod descriptor;
pub mod errors;
pub mod registry;

pub use descriptor::{
    ProviderBranding, ProviderCLIConfig, ProviderDescriptor, ProviderFetchPlan, ProviderMetadata,
};
pub use errors::ProviderError;
pub use registry::{ProviderCatalog, ProviderRegistration, REGISTRY};
