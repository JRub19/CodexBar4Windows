//! Compute the popover width from the longest content line.
//!
//! Spec 15 (popover menu card UI) targets a 360 px popover but allows
//! the card to expand to 420 px when an unusually long provider summary
//! would otherwise wrap awkwardly. The minimum is 320 px so the card
//! never feels narrower than a Windows context menu.
//!
//! Inputs are in CSS pixels at 1x DPI. The caller is responsible for
//! measuring line widths with the actual fonts already loaded; this
//! module only does the arithmetic and the clamp so the policy is
//! verifiable by unit tests.

pub const POPUP_WIDTH_MIN_PX: u32 = 320;
pub const POPUP_WIDTH_TARGET_PX: u32 = 360;
pub const POPUP_WIDTH_MAX_PX: u32 = 420;

/// Horizontal padding inside the card. The longest measured line is
/// added to twice this value to compute the required outer width.
pub const POPUP_HORIZONTAL_PADDING_PX: u32 = 18;

/// Pick the popup width given the widest measured content line.
///
/// Returns a value in `[POPUP_WIDTH_MIN_PX, POPUP_WIDTH_MAX_PX]`. When
/// the content fits in the target width, the target is returned so the
/// popup stays a consistent shape across providers.
pub fn pick(longest_line_px: u32) -> u32 {
    let needed = longest_line_px.saturating_add(POPUP_HORIZONTAL_PADDING_PX * 2);
    needed
        .max(POPUP_WIDTH_TARGET_PX)
        .clamp(POPUP_WIDTH_MIN_PX, POPUP_WIDTH_MAX_PX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_lines_use_the_target_width() {
        // Even a 0-width line should keep the popup at the target so
        // the empty state does not collapse to the minimum.
        assert_eq!(pick(0), POPUP_WIDTH_TARGET_PX);
        assert_eq!(pick(100), POPUP_WIDTH_TARGET_PX);
    }

    #[test]
    fn medium_lines_expand_past_the_target() {
        // 340 px line + 2 * 18 padding = 376 px, which is above target
        // and below max, so it should be returned verbatim.
        let w = pick(340);
        assert!(w > POPUP_WIDTH_TARGET_PX);
        assert!(w < POPUP_WIDTH_MAX_PX);
        assert_eq!(w, 376);
    }

    #[test]
    fn very_long_lines_are_clamped_to_max() {
        assert_eq!(pick(1000), POPUP_WIDTH_MAX_PX);
        assert_eq!(pick(u32::MAX), POPUP_WIDTH_MAX_PX);
    }

    #[test]
    fn target_width_is_within_bounds() {
        const _: () = assert!(POPUP_WIDTH_TARGET_PX >= POPUP_WIDTH_MIN_PX);
        const _: () = assert!(POPUP_WIDTH_TARGET_PX <= POPUP_WIDTH_MAX_PX);
    }
}
