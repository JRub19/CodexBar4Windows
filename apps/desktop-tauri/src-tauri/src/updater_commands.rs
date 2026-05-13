//! Tauri auto-update commands. Phase 9 §F.
//!
//! Three commands surface the `tauri-plugin-updater` lifecycle to the
//! popup banner and About-pane "Check now" button:
//!
//! - `check_for_update` — fetches the signed manifest from the
//!   configured endpoint and reports the newest available version (or
//!   None when the installed version is current).
//! - `install_update` — downloads + verifies the minisign signature
//!   over the installer bytes, then runs the installer silently. On
//!   success the installer kills + relaunches the app.
//! - `current_version` — exposes the embedded `CARGO_PKG_VERSION` so
//!   the React banner can render "x.y.z is available; you are on
//!   a.b.c".
//!
//! All three are best-effort: a network failure or signature
//! mismatch is reported as a String error the React side surfaces in
//! the banner, never panics.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;
use tracing::info;

#[derive(Debug, Serialize)]
pub struct UpdateInfoDto {
    pub current_version: String,
    pub available_version: Option<String>,
    pub release_notes: Option<String>,
    pub release_date: Option<String>,
}

#[tauri::command]
pub async fn current_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<UpdateInfoDto, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            info!(
                target: "codexbar::updater",
                installed = %current,
                available = %update.version,
                "update.available",
            );
            Ok(UpdateInfoDto {
                current_version: current,
                available_version: Some(update.version.clone()),
                release_notes: update.body.clone(),
                release_date: update.date.map(|d| d.to_string()),
            })
        }
        Ok(None) => {
            info!(target: "codexbar::updater", installed = %current, "update.none");
            Ok(UpdateInfoDto {
                current_version: current,
                available_version: None,
                release_notes: None,
                release_date: None,
            })
        }
        Err(err) => {
            info!(
                target: "codexbar::updater",
                error = %err,
                "update.check_failed",
            );
            Err(format!("update check failed: {err}"))
        }
    }
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("update check failed: {e}"))?
        .ok_or_else(|| "no update available".to_string())?;

    // The plugin downloads + verifies the minisign signature against
    // the pubkey baked into tauri.conf.json before invoking the
    // installer. A tampered installer fails here, never reaches the
    // user's machine.
    update
        .download_and_install(
            |chunk_length, content_length| {
                info!(
                    target: "codexbar::updater",
                    chunk = chunk_length,
                    total = content_length.unwrap_or(0),
                    "update.download_progress",
                );
            },
            || {
                info!(target: "codexbar::updater", "update.download_finished");
            },
        )
        .await
        .map_err(|e| format!("install failed: {e}"))?;

    info!(target: "codexbar::updater", "update.installed");
    Ok(())
}
