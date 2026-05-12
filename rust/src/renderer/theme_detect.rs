//! Detect the current Windows taskbar theme by reading
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\SystemUsesLightTheme`.
//!
//! Returns `Theme::Light` when the registry value is non zero,
//! `Theme::Dark` when it is zero, and `Theme::Dark` (the default) when
//! the key is absent or unreadable. The Windows process must subscribe
//! to `WM_SETTINGCHANGE` and re call this on `ImmersiveColorSet` to
//! pick up runtime switches; that wiring lives in the desktop crate.

use super::cache::Theme;

#[cfg(windows)]
const KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

#[cfg(windows)]
const VALUE_NAME: &str = "SystemUsesLightTheme";

/// Read the current taskbar theme from the registry.
#[cfg(windows)]
pub fn detect_taskbar_theme() -> Theme {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey(KEY_PATH) else {
        return Theme::Dark;
    };
    match key.get_value::<u32, _>(VALUE_NAME) {
        Ok(0) => Theme::Dark,
        Ok(_) => Theme::Light,
        Err(_) => Theme::Dark,
    }
}

#[cfg(not(windows))]
pub fn detect_taskbar_theme() -> Theme {
    Theme::Dark
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_one_of_the_two_themes() {
        let theme = detect_taskbar_theme();
        assert!(matches!(theme, Theme::Light | Theme::Dark));
    }
}
