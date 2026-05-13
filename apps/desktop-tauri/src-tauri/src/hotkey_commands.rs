//! Global-hotkey Tauri commands. Phase 8 task 14.
//!
//! Single global shortcut: `Win+Shift+U` toggles the tray popup
//! visibility. The shortcut is registered at app boot when the
//! user has it enabled (default: ON) and unregistered when they
//! disable it from the Shortcuts pane.
//!
//! The handler emits a `popup:toggle` event the popup React side
//! already listens for, so no additional plumbing is needed in
//! `PopupShell`.

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tracing::info;

/// META maps to the Windows logo key on Windows. We construct
/// lazily because `Shortcut::new` is not `const`.
fn default_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::META | Modifiers::SHIFT), Code::KeyU)
}

#[derive(Default)]
pub struct HotkeyRegistry {
    /// Whether the default shortcut is currently registered.
    registered: Mutex<bool>,
}

pub struct HotkeyHandle(pub Arc<HotkeyRegistry>);

#[tauri::command]
pub async fn hotkey_is_registered(
    handle: tauri::State<'_, HotkeyHandle>,
) -> Result<bool, String> {
    Ok(*handle.0.registered.lock())
}

#[tauri::command]
pub async fn hotkey_register(
    app: AppHandle,
    handle: tauri::State<'_, HotkeyHandle>,
) -> Result<(), String> {
    register_default(&app, &handle.0)
}

#[tauri::command]
pub async fn hotkey_unregister(
    app: AppHandle,
    handle: tauri::State<'_, HotkeyHandle>,
) -> Result<(), String> {
    unregister_default(&app, &handle.0)
}

pub fn register_default(app: &AppHandle, registry: &HotkeyRegistry) -> Result<(), String> {
    let mut state = registry.registered.lock();
    if *state {
        return Ok(());
    }
    app.global_shortcut()
        .register(default_shortcut())
        .map_err(|e| e.to_string())?;
    *state = true;
    info!(
        target: "codexbar::hotkey",
        shortcut = "Win+Shift+U",
        "global_shortcut.registered",
    );
    Ok(())
}

pub fn unregister_default(app: &AppHandle, registry: &HotkeyRegistry) -> Result<(), String> {
    let mut state = registry.registered.lock();
    if !*state {
        return Ok(());
    }
    app.global_shortcut()
        .unregister(default_shortcut())
        .map_err(|e| e.to_string())?;
    *state = false;
    info!(
        target: "codexbar::hotkey",
        "global_shortcut.unregistered",
    );
    Ok(())
}

/// Build the `tauri-plugin-global-shortcut` plugin with the toggle
/// handler. Called from the Tauri Builder in `lib.rs`.
pub fn build_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            if *shortcut != default_shortcut() {
                return;
            }
            info!(target: "codexbar::hotkey", "global_shortcut.toggle_pressed");
            toggle_popup(app);
        })
        .build()
}

fn toggle_popup<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        match window.is_visible() {
            Ok(true) => {
                let _ = window.hide();
            }
            _ => {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    }
}
