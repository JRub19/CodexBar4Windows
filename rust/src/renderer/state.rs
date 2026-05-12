//! Render state knobs that apply across the whole frame: stale dim,
//! reduced motion, low power mode. Phase 3 task A7 wires the stale alpha
//! table; later tasks add the motion and power flags.
//!
//! Stale mode dims track, stroke, and fill in lockstep. Without the
//! lockstep, only the fill would dim and a 0% bar would look identical
//! to a fresh 0% bar. With the lockstep, a stale icon looks washed out
//! across all three layers, which is the correct cue.

use super::bars::BarAlphas;

/// Stale dim alpha table. Values from `docs/windows/spec/10-tray-icon-system.md`
/// section 2.8.
pub const STALE_ALPHAS: BarAlphas = BarAlphas {
    track: 0.18,
    stroke: 0.28,
    fill: 0.55,
};

/// Return the alphas to use for `stale = true`, otherwise the default.
pub fn bar_alphas_for(stale: bool) -> BarAlphas {
    if stale {
        STALE_ALPHAS
    } else {
        BarAlphas::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_returns_full_brightness_alphas() {
        let a = bar_alphas_for(false);
        assert_eq!(a.track, 0.28);
        assert_eq!(a.stroke, 0.44);
        assert_eq!(a.fill, 1.0);
    }

    #[test]
    fn stale_dims_all_three_layers_in_lockstep() {
        let a = bar_alphas_for(true);
        assert_eq!(a.track, 0.18);
        assert_eq!(a.stroke, 0.28);
        assert_eq!(a.fill, 0.55);
    }

    #[test]
    fn stale_alphas_are_strictly_dimmer_than_default() {
        let fresh = bar_alphas_for(false);
        let stale = bar_alphas_for(true);
        assert!(stale.track < fresh.track);
        assert!(stale.stroke < fresh.stroke);
        assert!(stale.fill < fresh.fill);
    }
}
