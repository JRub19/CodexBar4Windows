//! Settings: serde-backed user preferences persisted to `config.json`.
//!
//! The shared crate owns the data model and the on disk format. The desktop
//! shell wraps the store in Tauri commands and emits the `settings:changed`
//! event after every successful write.

pub mod model;
pub mod store;

pub use model::{
    DebugFlags, DisplayPreferences, ProviderToggle, RefreshFrequency, Settings, SettingsPatch,
    SCHEMA_VERSION,
};
pub use store::{SettingsHandle, SettingsStore, StoreError};
