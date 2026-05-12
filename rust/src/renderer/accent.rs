//! Detect the current Windows accent color.
//!
//! The fastest path that does not pull in the WinRT runtime is to read
//! `HKCU\Software\Microsoft\Windows\DWM\AccentColor`, a `u32` packed as
//! `0xAABBGGRR` (DWM's preferred byte order). We unpack into a
//! `tiny_skia::Color`.
//!
//! Phase 8 (Preferences) layers a user override that lets people pick a
//! fixed color regardless of the OS setting; for Phase 3 we just read
//! whatever the OS reports and fall back to the Windows 10 default blue
//! (`#0078D4`) on any failure.

use tiny_skia::Color;

#[cfg(windows)]
const KEY_PATH: &str = r"Software\Microsoft\Windows\DWM";

#[cfg(windows)]
const VALUE_NAME: &str = "AccentColor";

/// Default accent: Windows 10 / 11 system blue. Used when the registry
/// read fails or the runtime is non Windows. Constructed lazily because
/// `tiny_skia::Color::from_rgba` is not `const`.
pub fn fallback_accent() -> Color {
    Color::from_rgba(0.0, 120.0 / 255.0, 212.0 / 255.0, 1.0).expect("fallback accent valid")
}

#[cfg(windows)]
pub fn detect_accent_color() -> Color {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey(KEY_PATH) else {
        return fallback_accent();
    };
    let Ok(raw): Result<u32, _> = key.get_value(VALUE_NAME) else {
        return fallback_accent();
    };
    color_from_dwm_u32(raw)
}

#[cfg(not(windows))]
pub fn detect_accent_color() -> Color {
    fallback_accent()
}

/// Convert a DWM `u32` accent color value into RGBA. DWM stores the
/// color as `0xAABBGGRR` so the byte at the lowest bit position is the
/// red channel.
pub fn color_from_dwm_u32(raw: u32) -> Color {
    let r = (raw & 0xFF) as u8;
    let g = ((raw >> 8) & 0xFF) as u8;
    let b = ((raw >> 16) & 0xFF) as u8;
    let a = ((raw >> 24) & 0xFF) as u8;
    let a = if a == 0 { 0xFF } else { a };
    rgba8(r, g, b, a).unwrap_or_else(fallback_accent)
}

fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Option<Color> {
    Color::from_rgba(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_a_color() {
        let c = detect_accent_color();
        assert!(c.alpha() > 0.0);
    }

    #[test]
    fn dwm_format_round_trips_a_known_value() {
        // 0xFFD47800 = ABGR, decoded as (R=0x00, G=0x78, B=0xD4, A=0xFF).
        let c = color_from_dwm_u32(0xFFD47800);
        // Round-trip back to u8.
        let r = (c.red() * 255.0).round() as u8;
        let g = (c.green() * 255.0).round() as u8;
        let b = (c.blue() * 255.0).round() as u8;
        assert_eq!((r, g, b), (0x00, 0x78, 0xD4));
    }

    #[test]
    fn dwm_zero_alpha_treated_as_opaque() {
        let c = color_from_dwm_u32(0x00FF0000);
        assert!(c.alpha() > 0.99, "expected opaque, got {}", c.alpha());
    }

    #[test]
    fn fallback_is_windows_default_blue() {
        let c = fallback_accent();
        let r = (c.red() * 255.0).round() as u8;
        let g = (c.green() * 255.0).round() as u8;
        let b = (c.blue() * 255.0).round() as u8;
        assert_eq!((r, g, b), (0x00, 0x78, 0xD4));
    }

    #[test]
    fn rgba8_returns_some_for_valid_input() {
        assert!(rgba8(0, 120, 212, 255).is_some());
    }
}
