//! CodexBar4Windows desktop Tauri shell.
//!
//! Phase 1 wires the path environment, file logging, settings store, usage
//! store, and the refresh loop. The tray icon plus native context menu from
//! phase 0 are updated to expose Pause/Resume refresh and Preferences entry
//! points. Phase 3 onward layers the popup window and dynamic icon on top.

pub mod commands;
#[cfg(feature = "dev")]
pub mod dev;
pub mod first_run;
pub mod secrets_commands;
pub mod tray_renderer;

use std::sync::Arc;

use codexbar::cookies::{CookieAccessGate, CookieHeaderCache, CookieImporter};
use codexbar::core::{PathEnvironment, RefreshLoop, UsageStore};
use codexbar::secrets::token_account::TokenAccountStore;
use codexbar::settings::SettingsHandle;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
use tokio::runtime::Runtime;
use tracing::info;

use crate::commands::{FirstRunHandle, RefreshHandle, UsageHandle};
use crate::first_run::FirstRunStore;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You have been greeted from Rust.", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let env = match PathEnvironment::discover() {
        Ok(env) => {
            if let Err(err) = env.ensure() {
                eprintln!("[codexbar] failed to ensure path environment: {err}");
            }
            env
        }
        Err(err) => {
            eprintln!("[codexbar] failed to discover path environment: {err}");
            return;
        }
    };

    let _log_guard = codexbar::logging::init(&env.logs_dir).ok();
    info!(target: "codexbar::app", version = codexbar::version(), "app.boot");

    let settings: SettingsHandle = commands::build_settings_handle(env.config_file.clone());
    let usage = Arc::new(UsageStore::new());
    let refresh = RefreshLoop::new(settings.clone());

    let token_store = Arc::new(TokenAccountStore::new(env.secrets_dir.clone()));
    let cookie_cache = Arc::new(CookieHeaderCache::new(env.cache_dir.join("cookie-cache")));
    let cookie_gate = Arc::new(CookieAccessGate::new());
    let cookie_importer = Arc::new(CookieImporter::new(
        cookie_cache.clone(),
        cookie_gate,
        token_store.clone(),
    ));

    // Spawn the refresh loop on a tokio runtime owned by the main thread.
    // We leak the runtime intentionally so it lives for the app lifetime;
    // the OS reclaims on exit and tokio handles will be cancelled.
    let runtime: &'static Runtime = Box::leak(Box::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("codexbar-refresh")
            .build()
            .expect("tokio runtime must build"),
    ));
    let refresh_for_spawn = refresh.clone();
    runtime.spawn(async move {
        refresh_for_spawn.spawn().await.ok();
    });

    let first_run_store = FirstRunStore::new(env.roaming.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(settings.clone())
        .manage(RefreshHandle(refresh))
        .manage(UsageHandle(usage))
        .manage(FirstRunHandle(first_run_store))
        .manage(secrets_commands::TokenAccountHandle(token_store))
        .manage(secrets_commands::CookieImporterHandle(cookie_importer))
        .setup(move |app| {
            let refresh_i = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
            let pause_i = MenuItem::with_id(
                app,
                "pause",
                if settings.snapshot().pause_refresh {
                    "Resume refresh"
                } else {
                    "Pause refresh"
                },
                true,
                None::<&str>,
            )?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let prefs_i =
                MenuItem::with_id(app, "preferences", "Preferences...", true, None::<&str>)?;
            let about_i =
                MenuItem::with_id(app, "about", "About CodexBar4Windows", true, None::<&str>)?;
            let check_updates_i = MenuItem::with_id(
                app,
                "check_updates",
                "Check for updates",
                true,
                None::<&str>,
            )?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, Some("CmdOrCtrl+Q"))?;

            let menu = Menu::with_items(
                app,
                &[
                    &refresh_i,
                    &pause_i,
                    &sep1,
                    &prefs_i,
                    &about_i,
                    &check_updates_i,
                    &sep2,
                    &quit_i,
                ],
            )?;

            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("default window icon missing; bundle is misconfigured")?;
            let _tray = TrayIconBuilder::with_id("main")
                .icon(icon)
                .tooltip("CodexBar4Windows\nAI coding limits in your Windows tray")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        info!(target: "codexbar::tray", "menu.quit");
                        app.exit(0);
                    }
                    "preferences" => {
                        info!(target: "codexbar::tray", "menu.preferences");
                    }
                    "about" => {
                        info!(target: "codexbar::tray", "menu.about");
                    }
                    "check_updates" => {
                        info!(target: "codexbar::tray", "menu.check_updates");
                    }
                    "pause" => {
                        info!(target: "codexbar::tray", "menu.pause_toggle");
                        if let Some(handle) = app.try_state::<SettingsHandle>() {
                            let cur = handle.snapshot();
                            let _ = handle.update(codexbar::settings::SettingsPatch {
                                pause_refresh: Some(!cur.pause_refresh),
                                ..Default::default()
                            });
                        }
                    }
                    "refresh" => {
                        info!(target: "codexbar::tray", "menu.refresh");
                        if let Some(handle) = app.try_state::<RefreshHandle>() {
                            let loop_ref = handle.0.clone();
                            std::thread::spawn(move || {
                                let rt = tokio::runtime::Builder::new_current_thread()
                                    .enable_all()
                                    .build()
                                    .expect("oneshot runtime");
                                rt.block_on(async {
                                    let _ = loop_ref.refresh_now().await;
                                });
                            });
                        }
                    }
                    other => {
                        info!(target: "codexbar::tray", id = other, "menu.unknown");
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        info!(target: "codexbar::tray", "icon.left_click");
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            info!(target: "codexbar::tray", "icon.registered");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::get_settings,
            commands::update_settings,
            commands::reset_settings,
            commands::provider_descriptors,
            commands::provider_snapshots,
            commands::refresh_now,
            commands::toggle_pause,
            commands::open_preferences,
            commands::quit_app,
            commands::first_run_state,
            commands::first_run_mark_tray_hint_shown,
            commands::first_run_reset,
            secrets_commands::list_token_accounts,
            secrets_commands::add_token_account,
            secrets_commands::edit_token_account,
            secrets_commands::remove_token_account,
            secrets_commands::set_active_token_account,
            secrets_commands::set_manual_cookie,
            secrets_commands::import_cookies_for,
            secrets_commands::clear_cookie_cache,
        ])
        .on_window_event(|window, event| {
            // Auto-dismiss the popup on focus loss to match the spec 80
            // behavior: the popover disappears whenever the user clicks
            // outside it or alt-tabs to another app.
            if window.label() == "main" {
                if let WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
