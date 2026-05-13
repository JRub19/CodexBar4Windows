//! Tauri command handlers. Phase 1 registers the settings, provider, and
//! refresh commands. Phase 4 onwards adds auth, log dump, and provider
//! action commands.

use std::sync::Arc;

use codexbar::core::{RefreshLoop, UsageStore};
use codexbar::providers::{ProviderCatalog, ProviderDescriptor, REGISTRY};
use codexbar::settings::{Settings, SettingsHandle, SettingsPatch};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::info;

use crate::first_run::{FirstRunState, FirstRunStore, WindowGeometry};

pub const EVENT_SETTINGS_CHANGED: &str = "settings:changed";

#[derive(serde::Serialize, Clone)]
pub struct SettingsChangedPayload {
    pub settings: Settings,
}

pub struct RefreshHandle(pub Arc<RefreshLoop>);
pub struct UsageHandle(pub Arc<UsageStore>);
pub struct StatusHandle(pub codexbar::status::StatusStore);
/// Shared cost-history store. The first call kicks off a scan; the
/// scan is also rerun on demand by the `refresh_cost_history`
/// command and as a side effect of every `cost_snapshots` invocation
/// that finds the cache older than 60s.
pub struct CostHandle(pub Arc<codexbar::cost::CostStore>);

#[derive(Serialize)]
pub struct StatusSnapshotDto {
    pub provider_id: String,
    pub severity: String,
    pub title: Option<String>,
    pub updated_at_unix_secs: Option<i64>,
    pub status_page_url: Option<String>,
    pub captured_at_unix_secs: i64,
}

#[derive(Serialize)]
pub struct StatusOverviewDto {
    pub snapshots: Vec<StatusSnapshotDto>,
    pub aggregate_provider_id: Option<String>,
    pub aggregate_severity: Option<String>,
}

#[tauri::command]
pub async fn status_snapshots(
    status: State<'_, StatusHandle>,
) -> Result<StatusOverviewDto, String> {
    let store = &status.0;
    let mut snapshots: Vec<StatusSnapshotDto> = store
        .all()
        .into_iter()
        .map(|s| StatusSnapshotDto {
            provider_id: s.provider_id,
            severity: serde_json::to_value(s.severity)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "unknown".into()),
            title: s.title,
            updated_at_unix_secs: s.updated_at_unix_secs,
            status_page_url: s.status_page_url,
            captured_at_unix_secs: s.captured_at_unix_secs,
        })
        .collect();
    snapshots.sort_by(|a, b| a.provider_id.cmp(&b.provider_id));

    // Tray aggregation honours user-configured provider order. Until
    // the Phase-8 reorder UI exists we walk the registry's canonical
    // order with every provider enabled.
    let mut order = codexbar::status::AggregationOrder::new();
    for id in codexbar::status::registry::all_status_capable_provider_ids() {
        order.push(*id, true);
    }
    let aggregated = codexbar::status::aggregate(store, &order);
    Ok(StatusOverviewDto {
        snapshots,
        aggregate_provider_id: aggregated.as_ref().map(|s| s.provider_id.clone()),
        aggregate_severity: aggregated.as_ref().map(|s| {
            serde_json::to_value(s.severity)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "unknown".into())
        }),
    })
}

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
pub async fn get_provider_kv(
    key: String,
    store: State<'_, SettingsHandle>,
) -> Result<Option<String>, String> {
    let snap = store.snapshot();
    Ok(snap.provider_kv.get(&key).cloned())
}

#[tauri::command]
pub async fn set_provider_kv(
    app: AppHandle,
    store: State<'_, SettingsHandle>,
    key: String,
    value: String,
) -> Result<(), String> {
    let mut entry = std::collections::BTreeMap::new();
    entry.insert(key, value);
    let next = store
        .update(SettingsPatch {
            provider_kv: Some(entry),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
    let _ = app.emit(
        EVENT_SETTINGS_CHANGED,
        SettingsChangedPayload { settings: next },
    );
    Ok(())
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
    // Phase 4 P4-20: read every slot from UsageStore and return as a
    // map keyed by provider id. The popup uses this to populate the
    // initial state on mount and as a fallback when an event was
    // missed.
    let mut out = serde_json::Map::new();
    for descriptor in REGISTRY.descriptors() {
        if let Some(slot) = usage.0.slot(descriptor.id) {
            let value = serde_json::json!({
                "snapshot": slot.snapshot,
                "attempts": slot.attempts,
            });
            out.insert(descriptor.id.as_str().to_string(), value);
        }
    }
    Ok(serde_json::Value::Object(out))
}

#[tauri::command]
pub async fn provider_settings_descriptors(
) -> Result<codexbar::providers::ProviderSettingsSnapshot, String> {
    // Phase 4 P4-19 plus Phase 5: assemble settings contributions from
    // each provider that exposes one. Claude exposes the source picker,
    // CLI toggle, and accounts list; Codex contributes the same trio.
    let snap = codexbar::providers::ProviderSettingsSnapshot::builder()
        .with_section(codexbar::providers::claude::settings::contribution())
        .with_section(codexbar::providers::codex::settings::contribution())
        .with_section(codexbar::providers::cursor::settings::contribution())
        .with_section(codexbar::providers::copilot::settings::contribution())
        .with_section(codexbar::providers::gemini::settings::contribution())
        .with_section(codexbar::providers::openrouter::settings::contribution())
        .with_section(codexbar::providers::factory::settings::contribution())
        .with_section(codexbar::providers::deepseek::settings::contribution())
        .with_section(codexbar::providers::moonshot::settings::contribution())
        .with_section(codexbar::providers::zai::settings::contribution())
        .with_section(codexbar::providers::venice::settings::contribution())
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

/// Tauri command. Shows the Preferences window, optionally focusing
/// the Providers pane on a specific provider (`provider_id`). When
/// supplied, the command emits a `preferences:focus_provider` event
/// with the id so the React side scrolls to the right row.
#[tauri::command]
pub async fn open_preferences(
    app: AppHandle,
    store: State<'_, FirstRunHandle>,
    provider_id: Option<String>,
) -> Result<(), String> {
    let persisted = store.0.read().settings_window;
    // Phase 8: show + focus the Mica-effect Settings window. Apply
    // the persisted geometry on every show so the window lands where
    // the user left it; clamp negative coordinates to (0, 0) so a
    // disconnected monitor doesn't strand the window off-screen.
    if let Some(window) = app.get_webview_window("settings") {
        if let Some(g) = persisted {
            apply_geometry(&window, g);
        }
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        window.unminimize().ok();
        emit_focus_provider(&app, provider_id.as_deref());
        info!(target: "codexbar::commands", "open_preferences.shown");
        return Ok(());
    }
    // The settings window is declared in `tauri.conf.json`. If it
    // disappeared for any reason (uncaught close), recreate it.
    let mut builder = tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("index.html#/settings".into()),
    )
    .title("CodexBar4Windows Preferences")
    .inner_size(880.0, 620.0)
    .min_inner_size(720.0, 480.0)
    .resizable(true)
    .visible(true);
    if let Some(g) = persisted {
        builder = builder
            .position(g.x.max(0) as f64, g.y.max(0) as f64)
            .inner_size(g.width as f64, g.height as f64);
    }
    let _ = builder.build().map_err(|e| e.to_string())?;
    emit_focus_provider(&app, provider_id.as_deref());
    info!(target: "codexbar::commands", "open_preferences.recreated");
    Ok(())
}

/// Emit `preferences:focus_provider` so the React side can route the
/// user to a specific provider row. No-op when `provider_id` is None.
///
/// We emit twice with a short delay so the event lands whether the
/// webview is already mounted (immediate listener) or still booting
/// (the second emit catches the listener after it mounts).
fn emit_focus_provider<R: tauri::Runtime>(app: &tauri::AppHandle<R>, provider_id: Option<&str>) {
    let Some(id) = provider_id else { return };
    let payload = serde_json::json!({ "provider_id": id });
    let _ = app.emit("preferences:focus_provider", payload.clone());
    let app_clone = app.clone();
    let payload_clone = payload.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let _ = app_clone.emit("preferences:focus_provider", payload_clone);
    });
}

fn apply_geometry<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>, g: WindowGeometry) {
    use tauri::{LogicalPosition, LogicalSize};
    let _ = window.set_position(LogicalPosition::new(g.x.max(0) as f64, g.y.max(0) as f64));
    let _ = window.set_size(LogicalSize::new(g.width as f64, g.height as f64));
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

#[tauri::command]
pub async fn onboarding_advance(
    store: State<'_, FirstRunHandle>,
    app: AppHandle,
) -> Result<FirstRunState, String> {
    let state = store.0.advance_onboarding().map_err(|e| e.to_string())?;
    let _ = app.emit("onboarding:state", state.clone());
    info!(
        target: "codexbar::onboarding",
        step = ?state.onboarding_step,
        completed = state.onboarding_completed,
        "onboarding.advance",
    );
    Ok(state)
}

#[tauri::command]
pub async fn onboarding_rewind(
    store: State<'_, FirstRunHandle>,
    app: AppHandle,
) -> Result<FirstRunState, String> {
    let state = store.0.rewind_onboarding().map_err(|e| e.to_string())?;
    let _ = app.emit("onboarding:state", state.clone());
    info!(
        target: "codexbar::onboarding",
        step = ?state.onboarding_step,
        "onboarding.rewind",
    );
    Ok(state)
}

#[tauri::command]
pub async fn onboarding_complete(
    store: State<'_, FirstRunHandle>,
    app: AppHandle,
) -> Result<FirstRunState, String> {
    let state = store.0.complete_onboarding().map_err(|e| e.to_string())?;
    let _ = app.emit("onboarding:state", state.clone());
    info!(target: "codexbar::onboarding", "onboarding.complete");
    Ok(state)
}

#[tauri::command]
pub async fn onboarding_reset(
    store: State<'_, FirstRunHandle>,
    app: AppHandle,
) -> Result<FirstRunState, String> {
    let state = store.0.reset_onboarding().map_err(|e| e.to_string())?;
    let _ = app.emit("onboarding:state", state.clone());
    info!(target: "codexbar::onboarding", "onboarding.reset");
    Ok(state)
}

#[tauri::command]
pub async fn save_settings_window_geometry(
    store: State<'_, FirstRunHandle>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    store
        .0
        .save_settings_window(WindowGeometry {
            x,
            y,
            width,
            height,
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_last_settings_pane(
    store: State<'_, FirstRunHandle>,
    pane: String,
) -> Result<(), String> {
    store
        .0
        .save_last_settings_pane(pane)
        .map_err(|e| e.to_string())
}

/// Polish A3: surface the provider storage footprint scanner to the
/// Cost pane. Runs on a blocking task because `walkdir` is sync; the
/// AtomicBool cancel token is unused for now (the UI doesn't expose
/// a cancel button yet — 5-minute throttle in `UsageStore` will
/// eventually carry that responsibility).
#[tauri::command]
pub async fn storage_footprint_scan(
) -> Result<Vec<codexbar::cost::storage::ProviderStorageFootprint>, String> {
    tokio::task::spawn_blocking(|| {
        use codexbar::cost::storage::{scan_all, OsStorageFs};
        use codexbar::cost::walker::OsEnv;
        use std::sync::atomic::AtomicBool;
        let cancel = AtomicBool::new(false);
        scan_all(&OsEnv, &OsStorageFs, &cancel)
    })
    .await
    .map_err(|e| format!("scan join error: {e}"))
}

/// Open a filesystem path in Windows Explorer. Used by the Cost pane
/// "Open folder" buttons — never deletes anything, just navigates.
///
/// Implemented by spawning `explorer.exe <path>` directly: Explorer
/// treats the path argument as "open this folder" when it's a
/// directory, and "select this item in its parent folder" when it's
/// a file. Both are safe surfaces for the Cost pane.
#[tauri::command]
pub async fn open_in_explorer(path: String) -> Result<(), String> {
    use std::path::PathBuf;
    use std::process::Command;
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("path does not exist: {path}"));
    }
    Command::new("explorer.exe")
        .arg(&p)
        .spawn()
        .map_err(|e| format!("explorer.exe spawn failed: {e}"))?;
    info!(target: "codexbar::commands", path = %path, "open_in_explorer");
    Ok(())
}

/// Append a line to %APPDATA%\CodexBar4Windows\popup.log so the React
/// popup can leave breadcrumbs even when DevTools isn't open. Used
/// while diagnosing the blank-popup regression — every component
/// boot, store update, and error funnels through here.
///
/// File is opened in append mode on every call; we don't keep an open
/// handle so concurrent invokes from multiple components stay safe.
/// Failure to write is swallowed (returning OK) — we never want a
/// logger error to take down the UI further.
#[tauri::command]
pub async fn log_from_ui(level: String, scope: String, message: String) -> Result<(), String> {
    use std::io::Write;

    // Mirror to tracing so it also lands in the rolling log file the
    // Rust side already maintains.
    info!(target: "codexbar::ui", level = %level, scope = %scope, "{}", message);

    let appdata = match std::env::var_os("APPDATA") {
        Some(v) => std::path::PathBuf::from(v),
        None => return Ok(()),
    };
    let dir = appdata.join("CodexBar4Windows");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("log_from_ui: mkdir failed: {e}");
        return Ok(());
    }
    let path = dir.join("popup.log");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let line = format!("[{now}] [{level}] [{scope}] {message}\n");
    let mut f = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("log_from_ui: open failed: {e}");
            return Ok(());
        }
    };
    let _ = f.write_all(line.as_bytes());
    Ok(())
}

/// Return the cached per-provider cost snapshots. Runs a fresh scan
/// when the cache is empty so the first call always returns useful
/// data (or empty maps if the user has no JSONL on disk).
///
/// Subsequent calls are O(1) until `refresh_cost_history` is
/// invoked or the in-memory cache is rotated.
#[tauri::command]
pub async fn cost_snapshots(
    cost: State<'_, CostHandle>,
) -> Result<std::collections::HashMap<String, codexbar::providers::ProviderCostSnapshot>, String> {
    let store = cost.0.clone();
    // Scan synchronously the first time so callers don't get an
    // empty map back. The scan is cheap when directories are empty
    // and bounded by file count when they aren't.
    if store.snapshots().is_empty() {
        let store_for_scan = store.clone();
        tokio::task::spawn_blocking(move || store_for_scan.scan_once())
            .await
            .map_err(|e| format!("cost scan join error: {e}"))?;
    }
    Ok(store.snapshots())
}

/// Force a re-scan of the cost JSONL roots. The popup surfaces this
/// via a "Refresh" affordance in the cost section, and the refresh
/// loop calls it periodically.
#[tauri::command]
pub async fn refresh_cost_history(cost: State<'_, CostHandle>) -> Result<(), String> {
    let store = cost.0.clone();
    tokio::task::spawn_blocking(move || store.scan_once())
        .await
        .map_err(|e| format!("cost refresh join error: {e}"))?;
    Ok(())
}

/// Helper for the Tauri builder to register the State once paths are known.
pub fn build_settings_handle(config_path: std::path::PathBuf) -> SettingsHandle {
    Arc::new(codexbar::settings::SettingsStore::load(config_path))
}
