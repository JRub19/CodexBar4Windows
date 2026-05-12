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

use crate::first_run::{FirstRunState, FirstRunStore};

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
pub struct ProviderMetadataDto {
    pub display_name: String,
    pub homepage: String,
    pub dashboard_url: Option<String>,
    pub session_label: String,
    pub weekly_label: String,
    pub supports_opus: bool,
    pub supports_credits: bool,
}

#[derive(Serialize)]
pub struct ProviderBrandingDto {
    pub accent_hex: String,
    pub icon_id: String,
}

#[derive(Serialize)]
pub struct ProviderCliConfigDto {
    pub binary_name: String,
    pub default_args: Vec<String>,
}

#[derive(Serialize)]
pub struct ProviderFetchPlanDto {
    pub strategies: Vec<String>,
}

#[derive(Serialize)]
pub struct ProviderDescriptorDto {
    pub id: String,
    pub metadata: ProviderMetadataDto,
    pub branding: ProviderBrandingDto,
    pub cli: Option<ProviderCliConfigDto>,
    pub fetch_plan: ProviderFetchPlanDto,
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
            metadata: ProviderMetadataDto {
                display_name: d.metadata.display_name.to_string(),
                homepage: d.metadata.homepage.to_string(),
                dashboard_url: d.metadata.dashboard_url.map(|s| s.to_string()),
                session_label: d.metadata.session_label.to_string(),
                weekly_label: d.metadata.weekly_label.to_string(),
                supports_opus: d.metadata.supports_opus,
                supports_credits: d.metadata.supports_credits,
            },
            branding: ProviderBrandingDto {
                accent_hex: d.branding.accent_hex.to_string(),
                icon_id: d.branding.icon_id.to_string(),
            },
            cli: d.cli.as_ref().map(|c| ProviderCliConfigDto {
                binary_name: c.binary_name.to_string(),
                default_args: c.default_args.iter().map(|s| s.to_string()).collect(),
            }),
            fetch_plan: ProviderFetchPlanDto {
                strategies: d
                    .fetch_plan
                    .strategies
                    .iter()
                    .map(|s| format!("{s:?}"))
                    .collect(),
            },
        })
        .collect()
}

#[tauri::command]
pub async fn provider_snapshots(
    usage: State<'_, UsageHandle>,
) -> Result<serde_json::Value, String> {
    // Phase 4 P4-09: read every slot from UsageStore. Phase 4 P4-20 wires
    // the refresh loop to actually populate slots; until then the map is
    // empty.
    let _ = usage;
    Ok(serde_json::json!({}))
}

#[tauri::command]
pub async fn provider_settings_descriptors(
) -> Result<codexbar::providers::ProviderSettingsSnapshot, String> {
    // Phase 4 P4-19: assemble settings contributions from each provider.
    // Hello and Codex contribute nothing today; Claude exposes the
    // source picker, the CLI toggle, and the multi-account list.
    let snap = codexbar::providers::ProviderSettingsSnapshot::builder()
        .with_section(codexbar::providers::claude::settings::contribution())
        .build();
    Ok(snap)
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

pub struct FirstRunHandle(pub FirstRunStore);

#[tauri::command]
pub async fn first_run_state(store: State<'_, FirstRunHandle>) -> Result<FirstRunState, String> {
    Ok(store.0.read())
}

#[tauri::command]
pub async fn first_run_mark_tray_hint_shown(
    store: State<'_, FirstRunHandle>,
) -> Result<(), String> {
    store
        .0
        .mark_tray_pinned_hint_shown()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn first_run_reset(store: State<'_, FirstRunHandle>) -> Result<(), String> {
    store.0.clear().map_err(|e| e.to_string())
}

/// Helper for the Tauri builder to register the State once paths are known.
pub fn build_settings_handle(config_path: std::path::PathBuf) -> SettingsHandle {
    Arc::new(codexbar::settings::SettingsStore::load(config_path))
}
