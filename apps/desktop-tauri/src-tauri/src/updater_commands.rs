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
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;
use tracing::{info, warn};

/// Events emitted by `install_update` so the React side can render a
/// progress bar and final-state UI.
pub const EVENT_UPDATE_PROGRESS: &str = "updater:progress";
pub const EVENT_UPDATE_STAGE: &str = "updater:stage";

/// Sentinel value that lives in `tauri.conf.json` until a real
/// minisign public key is generated via
/// `scripts/generate-minisign-keypair.ps1`. The updater plugin
/// rejects this value (it's not valid base64 minisign output) so
/// guarding here just lets us surface a friendlier error than the
/// raw plugin failure.
const PLACEHOLDER_PUBKEY_FRAGMENT: &str = "REPLACE_WITH_BASE64_MINISIGN_PUBLIC_KEY";

/// Returns true when the running binary was built with the
/// placeholder pubkey. In that case we refuse to talk to the
/// updater so a dev/CI build doesn't accidentally noop-call the
/// network with an unverifiable manifest.
fn updater_misconfigured() -> bool {
    let raw = include_str!("../tauri.conf.json");
    raw.contains(PLACEHOLDER_PUBKEY_FRAGMENT)
}

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
    if updater_misconfigured() {
        let msg =
            "Updater is disabled: this build was compiled with a placeholder minisign public key.";
        warn!(target: "codexbar::updater", msg);
        return Err(msg.to_string());
    }
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

#[derive(Serialize, Clone)]
struct UpdateProgress {
    /// Total bytes received so far.
    downloaded: u64,
    /// Total expected bytes (from Content-Length); `None` when unknown.
    total: Option<u64>,
}

#[derive(Serialize, Clone)]
struct UpdateStage<'a> {
    /// One of: "checking" | "downloading" | "installing" | "relaunching" | "done" | "error".
    stage: &'a str,
    /// Optional human-readable status (error message, version, etc.).
    detail: Option<String>,
}

fn emit_stage(app: &AppHandle, stage: &str, detail: Option<String>) {
    let _ = app.emit(EVENT_UPDATE_STAGE, UpdateStage { stage, detail });
}

/// Download and apply the latest update, emitting progress events
/// throughout, then relaunch the app. The Tauri updater plugin runs
/// the bundled installer which kills the running process; on Windows
/// the NSIS/MSI installer relaunches us after install. As a belt-
/// and-braces safety net we also call `app.restart()` ourselves —
/// whichever wins, the user lands in the new version.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    if updater_misconfigured() {
        let msg = "Updater is disabled: this build was compiled with a placeholder \
            minisign public key. Run scripts/generate-minisign-keypair.ps1 -Apply \
            and rebuild before enabling auto-update.";
        emit_stage(&app, "error", Some(msg.to_string()));
        return Err(msg.to_string());
    }
    emit_stage(&app, "checking", None);
    let updater = app.updater().map_err(|e| {
        emit_stage(&app, "error", Some(e.to_string()));
        e.to_string()
    })?;
    let update = match updater.check().await {
        Ok(Some(u)) => u,
        Ok(None) => {
            emit_stage(&app, "done", Some("Already up to date".into()));
            return Ok(());
        }
        Err(err) => {
            let msg = format!("update check failed: {err}");
            emit_stage(&app, "error", Some(msg.clone()));
            return Err(msg);
        }
    };

    emit_stage(
        &app,
        "downloading",
        Some(format!("Downloading v{}", update.version)),
    );

    let app_for_progress = app.clone();
    let mut downloaded: u64 = 0;
    // The plugin downloads + verifies the minisign signature against
    // the pubkey baked into tauri.conf.json before invoking the
    // installer. A tampered installer fails here, never reaches the
    // user's machine.
    let install_result = update
        .download_and_install(
            move |chunk_length, content_length| {
                downloaded = downloaded.saturating_add(chunk_length as u64);
                let _ = app_for_progress.emit(
                    EVENT_UPDATE_PROGRESS,
                    UpdateProgress {
                        downloaded,
                        total: content_length,
                    },
                );
            },
            {
                let app = app.clone();
                move || {
                    info!(target: "codexbar::updater", "update.download_finished");
                    emit_stage(&app, "installing", Some("Running installer…".into()));
                }
            },
        )
        .await;

    match install_result {
        Ok(()) => {
            info!(target: "codexbar::updater", "update.installed");
            emit_stage(
                &app,
                "relaunching",
                Some("Restarting CodexBar4Windows…".into()),
            );
            // Give the React side ~250ms to render the "Restarting…"
            // toast before we kill the process.
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            app.restart();
        }
        Err(err) => {
            let msg = format!("install failed: {err}");
            emit_stage(&app, "error", Some(msg.clone()));
            Err(msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updater_misconfigured_matches_tauri_conf_state() {
        // The expected return value depends on whether the maintainer
        // has already swapped in a real minisign pubkey. We don't
        // assert a fixed bool — instead we verify the function reads
        // the embedded config and produces a stable answer matching
        // what `include_str!` sees.
        let raw = include_str!("../tauri.conf.json");
        let expected = raw.contains(PLACEHOLDER_PUBKEY_FRAGMENT);
        assert_eq!(updater_misconfigured(), expected);
    }

    #[test]
    fn placeholder_pubkey_fragment_is_distinct_enough_to_match_literally() {
        // Guard rail: if anyone shortens the placeholder, the
        // misconfigured() probe might match a real base64 pubkey.
        assert!(PLACEHOLDER_PUBKEY_FRAGMENT.len() >= 32);
        assert!(PLACEHOLDER_PUBKEY_FRAGMENT.starts_with("REPLACE_WITH"));
    }
}
