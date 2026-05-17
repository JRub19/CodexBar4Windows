//! Managed Codex accounts: v2 catalog and home factory. The live-codex-login
//! subprocess runner is deferred because it needs a working `codex.exe`
//! binary plus WebView2 for the workspace picker.

pub mod catalog;
pub mod home_factory;

pub use catalog::{
    decode_dual_version, migrate_v1, sanitize, CatalogError, CatalogStore, ManagedAccountRow,
    ManagedAccountRowV1, ManagedCatalogV2, CURRENT_SCHEMA, SCHEMA_VERSION_V1, SCHEMA_VERSION_V2,
};
pub use home_factory::{HomeFactory, HomeFactoryError};
