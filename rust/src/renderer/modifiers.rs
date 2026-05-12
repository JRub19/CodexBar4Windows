//! Read keyboard modifier state for tray click handling.
//!
//! Tauri's `TrayIconEvent::Click` does not include modifier state, so we
//! query the OS at handler entry via `GetAsyncKeyState`. Modifiers
//! affect the click semantics per spec 80:
//!
//! - No modifier: toggle popup.
//! - Shift: force a manual refresh without opening the popup.
//! - Ctrl: open Preferences instead of the popup.
//! - Alt: cycle the loading pattern (debug only, gated by
//!   `Settings.debug.debug_menu_enabled`).

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModifierState {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickIntent {
    TogglePopup,
    ForceRefresh,
    OpenPreferences,
    CyclePattern,
}

impl ModifierState {
    /// Resolve the user intent given the current modifier state. Higher
    /// priority modifiers win: Ctrl > Alt > Shift > none.
    pub fn intent(self) -> ClickIntent {
        if self.ctrl {
            ClickIntent::OpenPreferences
        } else if self.alt {
            ClickIntent::CyclePattern
        } else if self.shift {
            ClickIntent::ForceRefresh
        } else {
            ClickIntent::TogglePopup
        }
    }
}

/// Query the current modifier state from the OS.
#[cfg(windows)]
pub fn current() -> ModifierState {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT,
    };
    fn is_down(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
        unsafe { GetAsyncKeyState(vk.0 as i32) as u32 & 0x8000 != 0 }
    }
    ModifierState {
        shift: is_down(VK_SHIFT),
        ctrl: is_down(VK_CONTROL),
        alt: is_down(VK_MENU),
    }
}

#[cfg(not(windows))]
pub fn current() -> ModifierState {
    ModifierState::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_modifier_toggles_popup() {
        let m = ModifierState::default();
        assert_eq!(m.intent(), ClickIntent::TogglePopup);
    }

    #[test]
    fn shift_forces_refresh() {
        let m = ModifierState {
            shift: true,
            ..Default::default()
        };
        assert_eq!(m.intent(), ClickIntent::ForceRefresh);
    }

    #[test]
    fn ctrl_opens_preferences() {
        let m = ModifierState {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(m.intent(), ClickIntent::OpenPreferences);
    }

    #[test]
    fn alt_cycles_pattern() {
        let m = ModifierState {
            alt: true,
            ..Default::default()
        };
        assert_eq!(m.intent(), ClickIntent::CyclePattern);
    }

    #[test]
    fn ctrl_wins_over_alt_and_shift() {
        let m = ModifierState {
            shift: true,
            ctrl: true,
            alt: true,
        };
        assert_eq!(m.intent(), ClickIntent::OpenPreferences);
    }

    #[test]
    fn current_returns_a_modifier_state() {
        let _ = current();
    }
}
