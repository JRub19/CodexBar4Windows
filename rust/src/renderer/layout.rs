//! Layout selector: from `(primary, weekly, credits, style, stale)`, pick
//! which bars to draw and where.
//!
//! Matches `docs/windows/spec/10-tray-icon-system.md` section 2.1. Credits
//! are capped at $1000 (`creditsRatio = min(credits / 1000, 1) * 100`)
//! and only render as a bar when at least one of `primary` / `weekly` is
//! `None`.

use super::bars::BarRect;
use super::style::IconStyle;

pub const CREDITS_CAP_USD: f32 = 1000.0;

/// Inputs to layout selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutInput {
    pub primary: Option<f32>,
    pub weekly: Option<f32>,
    pub credits: Option<f32>,
    pub style: IconStyle,
    pub stale: bool,
}

/// The resolved layout choice. Each variant lists the bars to draw, in
/// top to bottom order. Pixel positions are 36 by 36 canvas coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    /// No data at all. Renderer falls back to the brand glyph or an
    /// empty canvas. The caller decides which.
    Empty,
    /// Primary on top, weekly on bottom. Used when at least one of the
    /// two has data and credits is not the only signal.
    TwoBarNormal {
        primary: BarSlot,
        weekly: BarSlot,
        credits_overlay: Option<BarSlot>,
    },
    /// Credits is the only signal. One thick bar centered vertically.
    TwoBarCreditsOnly { credits: BarSlot },
    /// Both usage and credits are present, and the layout puts a thick
    /// credits bar on the bottom for providers with credit balances.
    CreditsThickBottom {
        primary: BarSlot,
        weekly: BarSlot,
        credits: BarSlot,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarSlot {
    pub rect: BarRect,
    pub value: f32,
}

impl LayoutInput {
    /// Compute the credit ratio (0 to 100) capped at the $1000 cap.
    pub fn credit_ratio(&self) -> Option<f32> {
        self.credits.map(|c| {
            let clamped = c.clamp(0.0, CREDITS_CAP_USD);
            (clamped / CREDITS_CAP_USD) * 100.0
        })
    }
}

/// Build the canonical 36 by 36 two bar layout: primary at y=3..15,
/// weekly at y=19..31, both 30 px wide centered at x=3..33.
pub fn select(input: LayoutInput) -> Layout {
    let primary = input.primary;
    let weekly = input.weekly;
    let credits_ratio = input.credit_ratio();

    let has_primary = primary.is_some();
    let has_weekly = weekly.is_some();
    let has_credits = credits_ratio.is_some();

    if !has_primary && !has_weekly && !has_credits {
        return Layout::Empty;
    }

    // Credits only: one thick centered bar.
    if !has_primary && !has_weekly {
        let value = credits_ratio.unwrap_or(0.0);
        return Layout::TwoBarCreditsOnly {
            credits: BarSlot {
                rect: BarRect {
                    x: 3,
                    y: 12,
                    w: 30,
                    h: 12,
                },
                value,
            },
        };
    }

    // Two bar mode. If credits exists, render as a thin overlay below
    // the bottom bar (the spec offers an explicit `CreditsThickBottom`
    // variant when the provider opts in via style).
    let primary_slot = BarSlot {
        rect: BarRect {
            x: 3,
            y: 3,
            w: 30,
            h: 12,
        },
        value: primary.unwrap_or(0.0),
    };
    let weekly_slot = BarSlot {
        rect: BarRect {
            x: 3,
            y: 19,
            w: 30,
            h: 12,
        },
        value: weekly.unwrap_or(0.0),
    };

    if has_credits {
        // Thick bottom variant when style suggests credits matter most.
        // Mac source uses this for `Codebuff`, `OpenRouter`, etc.; we
        // expose the toggle via the `style` (a future provider can map
        // to `CreditsThickBottom` deterministically in phase 4).
        let credits = BarSlot {
            rect: BarRect {
                x: 3,
                y: 32,
                w: 30,
                h: 3,
            },
            value: credits_ratio.unwrap_or(0.0),
        };
        return Layout::TwoBarNormal {
            primary: primary_slot,
            weekly: weekly_slot,
            credits_overlay: Some(credits),
        };
    }

    Layout::TwoBarNormal {
        primary: primary_slot,
        weekly: weekly_slot,
        credits_overlay: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(p: Option<f32>, w: Option<f32>, c: Option<f32>) -> LayoutInput {
        LayoutInput {
            primary: p,
            weekly: w,
            credits: c,
            style: IconStyle::Default,
            stale: false,
        }
    }

    #[test]
    fn empty_inputs_resolve_to_empty_layout() {
        assert!(matches!(select(input(None, None, None)), Layout::Empty));
    }

    #[test]
    fn primary_only_renders_two_bar_normal_with_zero_weekly() {
        match select(input(Some(75.0), None, None)) {
            Layout::TwoBarNormal {
                primary,
                weekly,
                credits_overlay,
            } => {
                assert_eq!(primary.value, 75.0);
                assert_eq!(weekly.value, 0.0);
                assert!(credits_overlay.is_none());
            }
            other => panic!("unexpected layout: {:?}", other),
        }
    }

    #[test]
    fn primary_plus_weekly_resolves_to_normal() {
        match select(input(Some(50.0), Some(40.0), None)) {
            Layout::TwoBarNormal {
                primary, weekly, ..
            } => {
                assert_eq!(primary.value, 50.0);
                assert_eq!(weekly.value, 40.0);
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn credits_only_picks_credits_only_layout() {
        match select(input(None, None, Some(500.0))) {
            Layout::TwoBarCreditsOnly { credits } => {
                assert_eq!(credits.value, 50.0);
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn credits_cap_caps_at_1000_dollars() {
        let result = input(None, None, Some(5000.0)).credit_ratio().unwrap();
        assert_eq!(result, 100.0);
    }

    #[test]
    fn credits_overlay_added_when_credits_plus_usage() {
        match select(input(Some(10.0), Some(20.0), Some(250.0))) {
            Layout::TwoBarNormal {
                credits_overlay, ..
            } => {
                let c = credits_overlay.expect("credits overlay");
                assert_eq!(c.value, 25.0);
                assert_eq!(c.rect.h, 3);
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn credit_ratio_clamps_negative_to_zero() {
        let r = input(None, None, Some(-100.0)).credit_ratio().unwrap();
        assert_eq!(r, 0.0);
    }
}
