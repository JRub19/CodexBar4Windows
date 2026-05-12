//! Six loading patterns. Each maps a phase angle `phi` (radians) to a
//! primary bar value (0 to 100). The secondary bar uses the same pattern
//! at `phi + offset`. The desktop shell drives `phi` at 30 Hz with a
//! 30 s hard ceiling per spec 80.
//!
//! Pattern math (verbatim from `docs/windows/spec/10-tray-icon-system.md`
//! section 4):
//!
//! - `KnightRider`: `0.5 + 0.5 * sin(phi)`, offset `pi`.
//! - `Cylon`: sawtooth, offset `pi / 2`.
//! - `OutsideIn`: `abs(cos(phi))`, offset `pi`.
//! - `Race`: sawtooth at 1.2x, offset `pi / 3`.
//! - `Pulse`: `0.4 + 0.6 * (0.5 + 0.5 * sin(phi))`, offset `pi / 2`.
//! - `Unbraid`: drives the morph through the celebration pipeline,
//!   offset `pi / 2`. The pattern value is `0.5 + 0.5 * sin(phi)` so
//!   the morph progresses smoothly.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_3, PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LoadingPattern {
    KnightRider,
    Cylon,
    OutsideIn,
    Race,
    Pulse,
    Unbraid,
}

impl LoadingPattern {
    /// Phase offset between the primary and secondary bar.
    pub fn secondary_offset(self) -> f32 {
        match self {
            Self::KnightRider => PI,
            Self::Cylon => FRAC_PI_2,
            Self::OutsideIn => PI,
            Self::Race => FRAC_PI_3,
            Self::Pulse => FRAC_PI_2,
            Self::Unbraid => FRAC_PI_2,
        }
    }

    /// Compute the bar value (0 to 100) at phase `phi` (radians).
    pub fn sample(self, phi: f32) -> f32 {
        let raw = match self {
            Self::KnightRider => 0.5 + 0.5 * phi.sin(),
            Self::Cylon => sawtooth(phi, 1.0),
            Self::OutsideIn => phi.cos().abs(),
            Self::Race => sawtooth(phi, 1.2),
            Self::Pulse => 0.4 + 0.6 * (0.5 + 0.5 * phi.sin()),
            Self::Unbraid => 0.5 + 0.5 * phi.sin(),
        };
        (raw * 100.0).clamp(0.0, 100.0)
    }

    /// Primary plus secondary at one phase. Convenience for callers that
    /// always want both bars.
    pub fn pair(self, phi: f32) -> (f32, f32) {
        (self.sample(phi), self.sample(phi + self.secondary_offset()))
    }
}

fn sawtooth(phi: f32, speed: f32) -> f32 {
    // Map phi to [0, 1) via fract on phi / TAU * speed, producing a
    // linear ramp that wraps. The result is left in [0, 1].
    let t = (phi / TAU) * speed;
    let frac = t - t.floor();
    frac.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knight_rider_returns_zero_to_one_hundred() {
        assert!((LoadingPattern::KnightRider.sample(0.0) - 50.0).abs() < 0.1);
        assert!((LoadingPattern::KnightRider.sample(FRAC_PI_2) - 100.0).abs() < 0.1);
        assert!((LoadingPattern::KnightRider.sample(-FRAC_PI_2) - 0.0).abs() < 0.1);
    }

    #[test]
    fn cylon_is_sawtooth() {
        let lo = LoadingPattern::Cylon.sample(0.0);
        let hi = LoadingPattern::Cylon.sample(TAU * 0.99);
        assert!(lo < hi, "{} < {}", lo, hi);
    }

    #[test]
    fn outside_in_uses_absolute_cosine() {
        assert!((LoadingPattern::OutsideIn.sample(0.0) - 100.0).abs() < 0.1);
        assert!((LoadingPattern::OutsideIn.sample(PI) - 100.0).abs() < 0.1);
        assert!(LoadingPattern::OutsideIn.sample(FRAC_PI_2) < 1.0);
    }

    #[test]
    fn pulse_minimum_is_40_percent() {
        let mut min: f32 = f32::INFINITY;
        for i in 0..1000 {
            let phi = (i as f32) * TAU / 1000.0;
            min = min.min(LoadingPattern::Pulse.sample(phi));
        }
        assert!(min >= 39.9, "pulse minimum was {}", min);
    }

    #[test]
    fn race_runs_faster_than_cylon() {
        // Race uses 1.2x speed; over one cycle (TAU), race wraps faster.
        let cylon = LoadingPattern::Cylon.sample(TAU * 0.9);
        let race = LoadingPattern::Race.sample(TAU * 0.9);
        // Race wraps before reaching 0.9, so its value resets lower.
        assert_ne!(cylon, race);
    }

    #[test]
    fn all_patterns_stay_in_range_under_random_phases() {
        // Property: 10,000 random phases, all values in [0, 100], no NaN.
        let phases: Vec<f32> = (0..10_000).map(|i| (i as f32) * 0.001 - 5.0).collect();
        for pattern in [
            LoadingPattern::KnightRider,
            LoadingPattern::Cylon,
            LoadingPattern::OutsideIn,
            LoadingPattern::Race,
            LoadingPattern::Pulse,
            LoadingPattern::Unbraid,
        ] {
            for &phi in &phases {
                let v = pattern.sample(phi);
                assert!(
                    v.is_finite(),
                    "{:?} produced non finite at phi={}",
                    pattern,
                    phi
                );
                assert!(
                    (0.0..=100.0).contains(&v),
                    "{:?} produced {} out of range at phi={}",
                    pattern,
                    v,
                    phi
                );
            }
        }
    }

    #[test]
    fn pair_secondary_uses_offset() {
        let (p, s) = LoadingPattern::KnightRider.pair(0.0);
        // Primary at 0 is 50; secondary at PI is also 50 (sin(PI) = 0).
        assert!((p - 50.0).abs() < 0.1);
        assert!((s - 50.0).abs() < 0.1);
    }
}
