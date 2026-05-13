//! Global-hotkey Tauri commands. Phase 8 task 14 + rebinding backfill.
//!
//! Default global shortcut: `Win+Shift+U` toggles the tray popup
//! visibility. The user can rebind it from the Shortcuts pane using
//! the `KeyShortcutRecorder` widget — that surface ships a chord
//! string like `"Ctrl+Shift+K"` to `hotkey_set_chord`, which parses
//! it, unregisters the previous binding, registers the new one, and
//! persists the chord in `Settings.popup_toggle_hotkey`.
//!
//! Chord-string grammar:
//!
//! - `+`-separated tokens, modifiers before the key.
//! - Modifier tokens (case-insensitive): `Ctrl`, `Control`, `Shift`,
//!   `Alt`, `Option`, `Win`, `Cmd`, `Meta`, `Super`.
//! - Key token: a letter `A`-`Z` (maps to `Code::KeyA`..`Code::KeyZ`)
//!   or a digit `0`-`9` (maps to `Code::Digit0`..`Code::Digit9`).
//!   Function keys `F1`..`F12` also accepted.
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

pub const DEFAULT_CHORD: &str = "Win+Shift+U";

#[derive(Default)]
pub struct HotkeyRegistry {
    /// The currently-registered shortcut. `None` means nothing is
    /// registered right now.
    active: Mutex<Option<Shortcut>>,
}

pub struct HotkeyHandle(pub Arc<HotkeyRegistry>);

#[tauri::command]
pub async fn hotkey_is_registered(handle: tauri::State<'_, HotkeyHandle>) -> Result<bool, String> {
    Ok(handle.0.active.lock().is_some())
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
    unregister_active(&app, &handle.0)
}

/// Parse a chord string + register it. If a chord is already active
/// it is unregistered first. Returns `Err` with a user-facing reason
/// when the chord is invalid or the OS refuses the binding.
#[tauri::command]
pub async fn hotkey_set_chord(
    app: AppHandle,
    handle: tauri::State<'_, HotkeyHandle>,
    chord: String,
) -> Result<(), String> {
    let parsed = parse_chord(&chord)?;
    let mut active = handle.0.active.lock();
    if let Some(prev) = active.take() {
        let _ = app.global_shortcut().unregister(prev);
    }
    app.global_shortcut()
        .register(parsed)
        .map_err(|e| format!("failed to register {chord}: {e}"))?;
    *active = Some(parsed);
    info!(
        target: "codexbar::hotkey",
        chord = %chord,
        "global_shortcut.bound",
    );
    Ok(())
}

/// Dry-run a chord string: returns Ok when it parses, Err with a
/// reason when it does not. The recorder widget calls this to give
/// inline feedback before the user commits the binding.
#[tauri::command]
pub async fn hotkey_test_chord(chord: String) -> Result<String, String> {
    parse_chord(&chord)?;
    Ok(chord)
}

pub fn register_default(app: &AppHandle, registry: &HotkeyRegistry) -> Result<(), String> {
    register_shortcut(app, registry, default_shortcut(), DEFAULT_CHORD)
}

pub fn register_shortcut(
    app: &AppHandle,
    registry: &HotkeyRegistry,
    shortcut: Shortcut,
    chord_for_log: &str,
) -> Result<(), String> {
    let mut active = registry.active.lock();
    if active.as_ref() == Some(&shortcut) {
        return Ok(());
    }
    if let Some(prev) = active.take() {
        let _ = app.global_shortcut().unregister(prev);
    }
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| e.to_string())?;
    *active = Some(shortcut);
    info!(
        target: "codexbar::hotkey",
        shortcut = chord_for_log,
        "global_shortcut.registered",
    );
    Ok(())
}

pub fn unregister_active(app: &AppHandle, registry: &HotkeyRegistry) -> Result<(), String> {
    let mut active = registry.active.lock();
    let Some(shortcut) = active.take() else {
        return Ok(());
    };
    app.global_shortcut()
        .unregister(shortcut)
        .map_err(|e| e.to_string())?;
    info!(
        target: "codexbar::hotkey",
        "global_shortcut.unregistered",
    );
    Ok(())
}

/// Parse a `+`-separated chord string into a `Shortcut`. Whitespace
/// around each token is ignored; the order of modifiers does not
/// matter. The key token must appear last and be the only non-modifier.
pub fn parse_chord(input: &str) -> Result<Shortcut, String> {
    let mut modifiers = Modifiers::empty();
    let mut key: Option<Code> = None;
    for raw in input.split('+') {
        let token = raw.trim();
        if token.is_empty() {
            return Err(format!("empty modifier in chord '{input}'"));
        }
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            "win" | "cmd" | "meta" | "super" => modifiers |= Modifiers::META,
            other => {
                if key.is_some() {
                    return Err(format!(
                        "chord '{input}' has more than one non-modifier key"
                    ));
                }
                key =
                    Some(parse_key(other).ok_or_else(|| {
                        format!("chord '{input}' has unknown key token '{token}'")
                    })?);
            }
        }
    }
    let Some(code) = key else {
        return Err(format!("chord '{input}' is missing a key"));
    };
    if modifiers.is_empty() {
        return Err(format!(
            "chord '{input}' has no modifier — bare keys are not allowed"
        ));
    }
    Ok(Shortcut::new(Some(modifiers), code))
}

fn parse_key(lower: &str) -> Option<Code> {
    // Single letter a..z
    if lower.len() == 1 {
        let c = lower.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            return letter_to_code(c.to_ascii_lowercase());
        }
        if c.is_ascii_digit() {
            return digit_to_code(c);
        }
    }
    // F1..F12
    if let Some(rest) = lower.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u32>() {
            return match n {
                1 => Some(Code::F1),
                2 => Some(Code::F2),
                3 => Some(Code::F3),
                4 => Some(Code::F4),
                5 => Some(Code::F5),
                6 => Some(Code::F6),
                7 => Some(Code::F7),
                8 => Some(Code::F8),
                9 => Some(Code::F9),
                10 => Some(Code::F10),
                11 => Some(Code::F11),
                12 => Some(Code::F12),
                _ => None,
            };
        }
    }
    None
}

fn letter_to_code(c: char) -> Option<Code> {
    Some(match c {
        'a' => Code::KeyA,
        'b' => Code::KeyB,
        'c' => Code::KeyC,
        'd' => Code::KeyD,
        'e' => Code::KeyE,
        'f' => Code::KeyF,
        'g' => Code::KeyG,
        'h' => Code::KeyH,
        'i' => Code::KeyI,
        'j' => Code::KeyJ,
        'k' => Code::KeyK,
        'l' => Code::KeyL,
        'm' => Code::KeyM,
        'n' => Code::KeyN,
        'o' => Code::KeyO,
        'p' => Code::KeyP,
        'q' => Code::KeyQ,
        'r' => Code::KeyR,
        's' => Code::KeyS,
        't' => Code::KeyT,
        'u' => Code::KeyU,
        'v' => Code::KeyV,
        'w' => Code::KeyW,
        'x' => Code::KeyX,
        'y' => Code::KeyY,
        'z' => Code::KeyZ,
        _ => return None,
    })
}

fn digit_to_code(c: char) -> Option<Code> {
    Some(match c {
        '0' => Code::Digit0,
        '1' => Code::Digit1,
        '2' => Code::Digit2,
        '3' => Code::Digit3,
        '4' => Code::Digit4,
        '5' => Code::Digit5,
        '6' => Code::Digit6,
        '7' => Code::Digit7,
        '8' => Code::Digit8,
        '9' => Code::Digit9,
        _ => return None,
    })
}

/// Build the `tauri-plugin-global-shortcut` plugin with the toggle
/// handler. Called from the Tauri Builder in `lib.rs`.
///
/// The plugin only emits events for shortcuts we explicitly
/// registered, so this handler simply toggles the popup on every
/// keydown event without filtering by shortcut value. That makes
/// rebinding a no-op for the handler — `hotkey_set_chord` swaps the
/// registration, the closure here keeps working.
pub fn build_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() != ShortcutState::Pressed {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_chord() {
        let s = parse_chord("Win+Shift+U").unwrap();
        assert_eq!(s, default_shortcut());
    }

    #[test]
    fn chord_parsing_is_case_and_order_insensitive() {
        let a = parse_chord("shift+ctrl+K").unwrap();
        let b = parse_chord("Ctrl+Shift+k").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn accepts_digits_and_function_keys() {
        let d = parse_chord("Alt+5").unwrap();
        assert_eq!(d, Shortcut::new(Some(Modifiers::ALT), Code::Digit5));
        let f = parse_chord("Ctrl+Alt+F12").unwrap();
        assert_eq!(
            f,
            Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::F12)
        );
    }

    #[test]
    fn rejects_bare_keys_without_modifier() {
        assert!(parse_chord("K").is_err());
    }

    #[test]
    fn rejects_chord_with_two_non_modifier_tokens() {
        assert!(parse_chord("Ctrl+K+L").is_err());
    }

    #[test]
    fn rejects_unknown_key_token() {
        let err = parse_chord("Ctrl+SpaceBar").unwrap_err();
        assert!(err.contains("SpaceBar"));
    }

    #[test]
    fn rejects_empty_tokens() {
        assert!(parse_chord("Ctrl++K").is_err());
    }

    #[test]
    fn aliases_are_recognized() {
        let cmd = parse_chord("Cmd+Shift+P").unwrap();
        let meta = parse_chord("Meta+Shift+P").unwrap();
        let win = parse_chord("Win+Shift+P").unwrap();
        assert_eq!(cmd, meta);
        assert_eq!(meta, win);
    }

    #[test]
    fn parse_chord_tolerates_whitespace_in_tokens() {
        let a = parse_chord(" Ctrl + Shift + K ").unwrap();
        let b = parse_chord("Ctrl+Shift+K").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn default_chord_round_trips_through_parser() {
        // The DEFAULT_CHORD constant must always be parseable by the
        // same `parse_chord` the recorder + Tauri command call. If
        // someone changes the default to "Ctrl+Space" but forgets the
        // parser, this catches it.
        let parsed = parse_chord(DEFAULT_CHORD).expect("DEFAULT_CHORD must parse");
        assert_eq!(parsed, default_shortcut());
    }

    #[test]
    fn parse_chord_rejects_modifier_only_chords() {
        // Modifier-only chords are useless (no key to press) and the
        // OS would never deliver an event for them.
        assert!(parse_chord("Ctrl+Shift").is_err());
        assert!(parse_chord("Win").is_err());
    }
}
