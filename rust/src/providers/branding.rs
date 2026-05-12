//! Provider visual branding. Split out from `descriptor.rs` in phase 4
//! so adding fields (theme variants, monogram glyphs) does not touch
//! the umbrella descriptor module.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ProviderBranding {
    pub accent_hex: &'static str,
    pub accent_dark_hex: Option<&'static str>,
    pub accent_light_hex: Option<&'static str>,
    pub icon_id: &'static str,
}

impl ProviderBranding {
    pub fn solid(accent_hex: &'static str, icon_id: &'static str) -> Self {
        Self {
            accent_hex,
            accent_dark_hex: None,
            accent_light_hex: None,
            icon_id,
        }
    }

    /// Pick the accent hex for the active theme, falling back to the
    /// default when the theme variant is unset.
    pub fn accent_for_theme(&self, dark: bool) -> &'static str {
        if dark {
            self.accent_dark_hex.unwrap_or(self.accent_hex)
        } else {
            self.accent_light_hex.unwrap_or(self.accent_hex)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_returns_same_color_for_both_themes() {
        let b = ProviderBranding::solid("#6E5AFF", "claude");
        assert_eq!(b.accent_for_theme(true), "#6E5AFF");
        assert_eq!(b.accent_for_theme(false), "#6E5AFF");
    }

    #[test]
    fn theme_variants_are_picked_when_set() {
        let b = ProviderBranding {
            accent_hex: "#000000",
            accent_dark_hex: Some("#FFFFFF"),
            accent_light_hex: Some("#222222"),
            icon_id: "x",
        };
        assert_eq!(b.accent_for_theme(true), "#FFFFFF");
        assert_eq!(b.accent_for_theme(false), "#222222");
    }
}
