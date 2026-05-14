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

// ---- Cost popover window control ----------------------------------
//
// The cost-popover is a separate Tauri window declared in
// tauri.conf.json (visible: false initially). The main popup's per-
// provider Cost row invokes `show_cost_popover` on hover to position
// the window beside the main popup and reveal it; mouseleave invokes
// `schedule_cost_popover_close` after a grace period. The popover
// window itself invokes `cancel_cost_popover_close` while the cursor
// is over its content so the close timer never fires while the user
// is interacting with it.

use parking_lot::Mutex;
use std::time::Duration;

/// Shared state tracking whether the cost popover is currently
/// shown. Read by the main-popup focus handler to suppress its
/// auto-hide-on-blur while the popover is up (otherwise the
/// popover gaining focus would close the main popup).
#[derive(Default)]
pub struct CostPopoverState {
    pub visible: std::sync::atomic::AtomicBool,
    /// Generation counter for the scheduled-close timer. Every
    /// `schedule_cost_popover_close` invocation increments this;
    /// the spawned task only acts when the generation it captured
    /// still matches at fire time. This is how we cancel pending
    /// closes from `cancel_cost_popover_close`.
    pub close_generation: std::sync::atomic::AtomicU64,
    /// Last provider id requested. The popover React side reads
    /// this via the `cost-popover:set-provider` event, but we keep
    /// the value so the Rust side can be authoritative when the
    /// popover is re-shown for a different provider.
    pub current_provider: Mutex<Option<String>>,
}

/// Show the cost popover for the given provider, positioned beside
/// the main popup. Prefers the LEFT side of the main popup; falls
/// back to the right edge when the screen has no room on the left.
#[tauri::command]
pub async fn show_cost_popover(
    app: AppHandle,
    state: State<'_, CostPopoverHandle>,
    provider_id: String,
) -> Result<(), String> {
    use tauri::{Manager, PhysicalPosition};

    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let popover = app
        .get_webview_window("cost-popover")
        .ok_or_else(|| "cost-popover window not found".to_string())?;

    // Cancel any pending close — the user is interacting again.
    state
        .0
        .close_generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    *state.0.current_provider.lock() = Some(provider_id.clone());
    state
        .0
        .visible
        .store(true, std::sync::atomic::Ordering::SeqCst);

    // Tell the popover which provider's data to show. Emit BEFORE
    // making the window visible so the React side has provider data
    // queued by the time the window paints — avoids a "no provider
    // selected" flash.
    let _ = app.emit("cost-popover:set-provider", serde_json::json!({
        "provider_id": provider_id,
    }));

    // Position. The main popup is anchored bottom-right of the
    // screen (above the tray icon). We prefer placing the popover
    // to its left, gap of 6 px; if that would land off-screen,
    // place to its right.
    let main_pos = main.outer_position().map_err(|e| e.to_string())?;
    let main_size = main.outer_size().map_err(|e| e.to_string())?;
    let scale = main.scale_factor().unwrap_or(1.0);

    // Logical popover size, matching tauri.conf.json.
    let pop_logical_w: f64 = 360.0;
    let pop_logical_h: f64 = 240.0;
    let pop_physical_w = (pop_logical_w * scale) as i32;
    let pop_physical_h = (pop_logical_h * scale) as i32;
    let gap = (6.0 * scale) as i32;
    // Vertical offset from the main popup's top: drops the popover
    // down by ~280 logical px so it visually attaches near the
    // Cost section inside the provider card (which sits below
    // switcher + card header + hero + week metric). The macOS
    // submenu opens at the row's screen Y; we approximate by
    // offsetting from the popup's top.
    let y_offset = (280.0 * scale) as i32;

    let monitor = main
        .current_monitor()
        .ok()
        .flatten()
        .ok_or_else(|| "no monitor for main window".to_string())?;
    let mon_pos = monitor.position();
    let mon_size = monitor.size();

    // Candidate: to the left of main.
    let left_x = main_pos.x - pop_physical_w - gap;
    let right_x = main_pos.x + main_size.width as i32 + gap;
    let prefer_left = left_x >= mon_pos.x + 4;
    let x = if prefer_left {
        left_x
    } else if right_x + pop_physical_w <= mon_pos.x + mon_size.width as i32 - 4 {
        right_x
    } else {
        // Neither side fits — pin to whichever side has more room.
        if left_x >= mon_pos.x { left_x } else { right_x }
    };

    // Vertical alignment: drop the popover down from the popup's
    // top edge by `y_offset` so it visually anchors near the
    // provider card body. Clamp to monitor.
    let mut y = main_pos.y + y_offset;
    let max_y = mon_pos.y + mon_size.height as i32 - pop_physical_h - 4;
    let min_y = mon_pos.y + 4;
    if y > max_y { y = max_y; }
    if y < min_y { y = min_y; }

    let _ = popover.set_position(PhysicalPosition::new(x, y));
    // Set size explicitly each show in case DPI changed between monitors.
    let _ = popover.set_size(tauri::LogicalSize::new(pop_logical_w, pop_logical_h));
    let _ = popover.show();
    // DEBUG: auto-open DevTools the first time we show the popover
    // so a blank-render bug is immediately inspectable. The atomic
    // generation is incremented before show, so we tie devtools to
    // the FIRST show this session via a simple OnceLock.
    static DEVTOOLS_OPENED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if DEVTOOLS_OPENED.get().is_none() {
        let _ = DEVTOOLS_OPENED.set(());
        popover.open_devtools();
    }
    Ok(())
}

/// Hide the cost popover immediately. Used internally and exposed
/// for completeness; React callers usually go through
/// `schedule_cost_popover_close` so the hover bridge can cancel it.
#[tauri::command]
pub async fn hide_cost_popover(
    app: AppHandle,
    state: State<'_, CostPopoverHandle>,
) -> Result<(), String> {
    use tauri::Manager;
    state
        .0
        .visible
        .store(false, std::sync::atomic::Ordering::SeqCst);
    *state.0.current_provider.lock() = None;
    if let Some(popover) = app.get_webview_window("cost-popover") {
        let _ = popover.hide();
    }
    Ok(())
}

/// Schedule a close after a grace period. If
/// `cancel_cost_popover_close` is invoked before the timer fires,
/// this close is cancelled by generation-counter mismatch.
#[tauri::command]
pub async fn schedule_cost_popover_close(
    app: AppHandle,
    state: State<'_, CostPopoverHandle>,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    use tauri::Manager;

    let gen = state.0.close_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let state_for_task = state.0.clone();
    let app_clone = app.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(360)).await;
        // Only act if generation is still current (no cancel happened).
        if state_for_task.close_generation.load(Ordering::SeqCst) != gen {
            return;
        }
        state_for_task.visible.store(false, Ordering::SeqCst);
        *state_for_task.current_provider.lock() = None;
        if let Some(popover) = app_clone.get_webview_window("cost-popover") {
            let _ = popover.hide();
        }
    });
    Ok(())
}

/// Cancel any pending scheduled close. Invoked by both the trigger
/// row (on re-hover) and the popover content (on mouseenter).
#[tauri::command]
pub async fn cancel_cost_popover_close(
    state: State<'_, CostPopoverHandle>,
) -> Result<(), String> {
    state
        .0
        .close_generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// Read the currently-displayed provider id from the popover state.
/// Used by the popover's React entry on mount so we don't depend on
/// catching the one-shot `cost-popover:set-provider` event (which the
/// listener may miss if the WebView hasn't booted yet at emit time).
#[tauri::command]
pub async fn get_active_cost_popover_provider(
    state: State<'_, CostPopoverHandle>,
) -> Result<Option<String>, String> {
    Ok(state.0.current_provider.lock().clone())
}

/// State wrapper, registered with `app.manage` in lib.rs setup().
pub struct CostPopoverHandle(pub std::sync::Arc<CostPopoverState>);

/// Helper for the Tauri builder to register the State once paths are known.
pub fn build_settings_handle(config_path: std::path::PathBuf) -> SettingsHandle {
    Arc::new(codexbar::settings::SettingsStore::load(config_path))
}
