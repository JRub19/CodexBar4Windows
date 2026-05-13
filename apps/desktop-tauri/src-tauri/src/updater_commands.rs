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
use tracing::{info, warn};

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
        warn!(
            target: "codexbar::updater",
            "update.check_skipped placeholder_pubkey present in tauri.conf.json",
        );
        return Ok(UpdateInfoDto {
            current_version: current,
            available_version: None,
            release_notes: None,
            release_date: None,
        });
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

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    if updater_misconfigured() {
        return Err(
            "Updater is disabled: this build was compiled with a placeholder \
             minisign public key. Run scripts/generate-minisign-keypair.ps1 -Apply \
             and rebuild before enabling auto-update."
                .to_string(),
        );
    }
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
