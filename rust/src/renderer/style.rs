//! Provider style variants. Affects bar corner radius, twist overlay,
//! and brand color. Mirrors `IconStyle` in the Mac source.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum IconStyle {
    /// Default capsule bars, no twist overlay. Used for providers that do
    /// not have a distinctive brand glyph (most of them).
    #[default]
    Default,
    /// Codex face: 4 by 4 eye, 18 by 4 hat. Blocky, antialias off.
    Codex,
    /// Claude crab: 3 px wide arms running full height. Blocky, antialias off.
    Claude,
    /// Gemini 8 point sparkle. Organic, antialias on.
    Gemini,
    /// Factory 16 point asterisk. Organic, antialias on.
    Factory,
    /// Warp tilted ellipses. Organic, antialias on. 3 px bar corner radius.
    Warp,
}

impl IconStyle {
    /// Default corner radius in pixels for a bar of the given height. The
    /// Mac source uses `h / 2` (capsule) for Default, 0 for Claude
    /// (square), and 3 for Warp (rounded but not capsule).
    pub fn bar_corner_radius(self, bar_height: u32) -> f32 {
        match self {
            Self::Claude => 0.0,
            Self::Warp => 3.0,
            _ => (bar_height as f32) / 2.0,
        }
    }

    /// Returns true when the twist overlay (or bars themselves) should be
    /// drawn with antialiasing on. Blocky shapes off, organic shapes on.
    pub fn antialias(self) -> bool {
        match self {
            Self::Default | Self::Codex | Self::Claude => false,
            Self::Gemini | Self::Factory | Self::Warp => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capsule_default_radius_is_half_height() {
        assert_eq!(IconStyle::Default.bar_corner_radius(12), 6.0);
    }

    #[test]
    fn claude_has_zero_radius() {
        assert_eq!(IconStyle::Claude.bar_corner_radius(12), 0.0);
    }

    #[test]
    fn warp_has_fixed_three_pixel_radius() {
        assert_eq!(IconStyle::Warp.bar_corner_radius(12), 3.0);
        assert_eq!(IconStyle::Warp.bar_corner_radius(8), 3.0);
    }

    #[test]
    fn organic_styles_request_antialias() {
        for s in [IconStyle::Gemini, IconStyle::Factory, IconStyle::Warp] {
            assert!(s.antialias(), "{:?} should antialias", s);
        }
        for s in [IconStyle::Default, IconStyle::Codex, IconStyle::Claude] {
            assert!(!s.antialias(), "{:?} should not antialias", s);
        }
    }
}
