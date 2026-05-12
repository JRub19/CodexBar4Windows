//! Tauri command handlers. Phase 1 registers the settings, provider, and
//! refresh commands. Phase 4 onwards adds auth, log dump, and provider
//! action commands.

use std::sync::Arc;

use codexbar::core::{RefreshLoop, UsageStore};
use codexbar::providers::{ProviderCatalog, ProviderDescriptor, REGISTRY};
use codexbar::settings::{Settings, SettingsHandle, SettingsPatch};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tracing::info;

pub const EVENT_SETTINGS_CHANGED: &str = "settings:changed";

#[derive(serde::Serialize, Clone)]
pub struct SettingsChangedPayload {
    pub settings: Settings,
}

pub struct RefreshHandle(pub Arc<RefreshLoop>);
pub struct UsageHandle(pub Arc<UsageStore>);

#[tauri::command]
pub async fn get_settings(store: State<'_, SettingsHandle>) -> Result<Settings, String> {
    Ok(store.snapshot())
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    store: State<'_, SettingsHandle>,
    patch: SettingsPatch,
) -> Result<Settings, String> {
    let next = store.update(patch).map_err(|e| e.to_string())?;
    let _ = app.emit(
        EVENT_SETTINGS_CHANGED,
        SettingsChangedPayload {
            settings: next.clone(),
        },
    );
    info!(target: "codexbar::commands", "settings.update_applied");
    Ok(next)
}

#[tauri::command]
pub async fn reset_settings(
    app: AppHandle,
    store: State<'_, SettingsHandle>,
) -> Result<Settings, String> {
    let next = store.reset().map_err(|e| e.to_string())?;
    let _ = app.emit(
        EVENT_SETTINGS_CHANGED,
        SettingsChangedPayload {
            settings: next.clone(),
        },
    );
    info!(target: "codexbar::commands", "settings.reset_applied");
    Ok(next)
}

#[derive(Serialize)]
pub struct ProviderDescriptorDto {
    pub id: String,
    pub display_name: String,
    pub accent_hex: String,
}

#[tauri::command]
pub async fn provider_descriptors() -> Result<Vec<ProviderDescriptorDto>, String> {
    Ok(catalog_to_dtos(&REGISTRY))
}

fn catalog_to_dtos(catalog: &ProviderCatalog) -> Vec<ProviderDescriptorDto> {
    catalog
        .descriptors()
        .map(|d: &ProviderDescriptor| ProviderDescriptorDto {
            id: d.id.as_str().to_string(),
            display_name: d.metadata.display_name.to_string(),
            accent_hex: d.branding.accent_hex.to_string(),
        })
        .collect()
}

#[tauri::command]
pub async fn provider_snapshots() -> Result<serde_json::Value, String> {
    // Phase 1: empty. Phase 4 fills this with real per provider snapshots.
    Ok(serde_json::json!({}))
}

#[tauri::command]
pub async fn refresh_now(refresh: State<'_, RefreshHandle>) -> Result<(), String> {
    refresh.0.refresh_now().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn toggle_pause(
    app: AppHandle,
    store: State<'_, SettingsHandle>,
    paused: bool,
) -> Result<Settings, String> {
    let next = store
        .update(SettingsPatch {
            pause_refresh: Some(paused),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
    let _ = app.emit(
        EVENT_SETTINGS_CHANGED,
        SettingsChangedPayload {
            settings: next.clone(),
        },
    );
    info!(target: "codexbar::commands", paused, "settings.toggle_pause");
    Ok(next)
}

#[tauri::command]
pub async fn open_preferences() -> Result<(), String> {
    // Phase 8 wires this to the preferences window.
    info!(target: "codexbar::commands", "open_preferences.invoked");
    Ok(())
}

#[tauri::command]
pub async fn quit_app(app: AppHandle) {
    info!(target: "codexbar::commands", "quit.invoked");
    app.exit(0);
}

/// Helper for the Tauri builder to register the State once paths are known.
pub fn build_settings_handle(config_path: std::path::PathBuf) -> SettingsHandle {
    Arc::new(codexbar::settings::SettingsStore::load(config_path))
}
