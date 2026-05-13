//! Best-effort credential detection for each registered provider.
//!
//! Used by the popup on first boot to auto-disable providers the user
//! hasn't signed in to yet — so a fresh install doesn't show 11
//! provider tabs the user has to manually disable.
//!
//! Detection is per-provider:
//!
//! - **Claude**: file `%USERPROFILE%\.claude\.credentials.json`.
//!   Created by Claude Code on Windows when the user signs in.
//! - **Codex**: file `%USERPROFILE%\.codex\auth.json`. Created by
//!   the Codex CLI on Windows after `codex login`.
//! - **Gemini**: file
//!   `%APPDATA%\gcloud\application_default_credentials.json` (the
//!   gcloud ADC path) OR `%USERPROFILE%\.gemini\settings.json`
//!   (Gemini CLI).
//! - **Cursor / Copilot / Factory / OpenRouter / DeepSeek / Moonshot
//!   / Z.ai / Venice**: token entries in the DPAPI-wrapped
//!   `TokenAccountStore`. Auto-import populates this for cookie
//!   providers on first boot; the rest require manual sign-in.
//!
//! We never *return* the credentials — just whether they're present.
//! No secret values cross the Tauri boundary.

use serde::Serialize;
use tauri::State;

use crate::secrets_commands::TokenAccountHandle;

#[derive(Debug, Clone, Serialize)]
pub struct CredentialPresence {
    pub provider_id: String,
    pub present: bool,
}

#[tauri::command]
pub async fn detect_provider_credentials(
    tokens: State<'_, TokenAccountHandle>,
) -> Result<Vec<CredentialPresence>, String> {
    let store = tokens.0.clone();
    let result = tokio::task::spawn_blocking(move || detect_all(&store))
        .await
        .map_err(|e| format!("detect join failed: {e}"))?;
    Ok(result)
}

fn detect_all(
    tokens: &codexbar::secrets::token_account::TokenAccountStore,
) -> Vec<CredentialPresence> {
    let mut out = Vec::new();

    // OAuth file-based providers — check the canonical credential
    // file locations on Windows. File existence is sufficient; we
    // don't validate the JSON shape (the resolver does that at fetch
    // time and reports errors via the regular refresh outcome path).
    out.push(check_file_path(
        "claude",
        userprofile_join(&[".claude", ".credentials.json"]),
    ));
    out.push(check_file_path(
        "codex",
        userprofile_join(&[".codex", "auth.json"]),
    ));

    // Gemini accepts either gcloud ADC (in APPDATA) or the Gemini CLI
    // settings file in USERPROFILE.
    let gemini_paths = [
        appdata_join(&["gcloud", "application_default_credentials.json"]),
        userprofile_join(&[".gemini", "settings.json"]),
    ];
    let gemini_present = gemini_paths
        .iter()
        .filter_map(|p| p.as_ref())
        .any(|p| p.exists());
    out.push(CredentialPresence {
        provider_id: "gemini".into(),
        present: gemini_present,
    });

    // Token-store providers — these all use DPAPI-wrapped entries
    // populated by auto-import (cursor/factory cookies) or manual
    // paste (API key providers).
    for id in [
        "cursor",
        "copilot",
        "factory",
        "openrouter",
        "deepseek",
        "moonshot",
        "zai",
        "venice",
    ] {
        let present = tokens
            .active_for(id)
            .ok()
            .flatten()
            .map(|a| !a.value.trim().is_empty())
            .unwrap_or(false);
        out.push(CredentialPresence {
            provider_id: id.into(),
            present,
        });
    }

    out
}

fn check_file_path(provider_id: &str, path: Option<std::path::PathBuf>) -> CredentialPresence {
    let present = path.as_ref().map(|p| p.exists()).unwrap_or(false);
    CredentialPresence {
        provider_id: provider_id.into(),
        present,
    }
}

fn userprofile_join(parts: &[&str]) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let mut p = std::path::PathBuf::from(home);
    for part in parts {
        p.push(part);
    }
    Some(p)
}

fn appdata_join(parts: &[&str]) -> Option<std::path::PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let mut p = std::path::PathBuf::from(appdata);
    for part in parts {
        p.push(part);
    }
    Some(p)
}
