//! CodexBar4Windows desktop Tauri shell.
//!
//! Phase 0 baseline: builds a tray icon with a native context menu, toggles
//! the main window on left click, exits on Quit. Phase 1 onward replaces the
//! mock body with real config, settings, refresh loop, and IPC contract.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You have been greeted from Rust.", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
                        println!("[tray] menu: quit");
                        app.exit(0);
                    }
                    "show" => {
                        println!("[tray] menu: show");
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "refresh" => {
                        println!("[tray] menu: refresh (stub, phase 1)");
                    }
                    "about" => {
                        println!("[tray] menu: about (stub, phase 1)");
                    }
                    other => {
                        println!("[tray] menu: unknown id {}", other);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        println!("[tray] left click");
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

            println!("[tray] icon registered with id 'main'");
            println!("[core] codexbar version: {}", codexbar::version());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
