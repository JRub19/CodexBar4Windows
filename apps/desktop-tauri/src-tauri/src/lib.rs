//! CodexBar4Windows desktop Tauri shell.
//!
//! Phase 1 wires the path environment, file logging, and the settings store
//! Tauri command surface. The tray icon plus native context menu carry over
//! from phase 0. Phase 3 onward layers the popup window, dynamic icon, and
//! real provider data on top of these seams.

pub mod commands;

use codexbar::core::PathEnvironment;
use codexbar::settings::SettingsHandle;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tracing::info;

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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(settings)
        .setup(|app| {
            let refresh_i = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "Show window", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let about_i =
                MenuItem::with_id(app, "about", "About CodexBar4Windows", true, None::<&str>)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, Some("CmdOrCtrl+Q"))?;

            let menu =
                Menu::with_items(app, &[&refresh_i, &show_i, &sep1, &about_i, &sep2, &quit_i])?;

            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("CodexBar4Windows\nAI coding limits in your Windows tray")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        info!(target: "codexbar::tray", "menu.quit");
                        app.exit(0);
                    }
                    "show" => {
                        info!(target: "codexbar::tray", "menu.show");
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "refresh" => {
                        info!(target: "codexbar::tray", "menu.refresh");
                    }
                    "about" => {
                        info!(target: "codexbar::tray", "menu.about");
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
            commands::reset_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
