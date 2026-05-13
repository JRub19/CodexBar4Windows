//! Reset-celebration predicate. Phase 9 §B-7 + §B-8 +
//! `docs/windows/spec/80-feel-and-polish.md` §4.
//!
//! When a provider's weekly window resets, we want to celebrate —
//! but only when the user actually used the plan during the last
//! window. Surprising people who never opened the popup with confetti
//! "rewarding" a 0% usage week is anti-pattern.
//!
//! The gate: `past_24h_utilization_percent >= MIN_UTILIZATION_PCT`.
//! Default 1.0% — anyone who's done any meaningful work qualifies.
//! Below that, the reset is silent (the tray icon still morphs).
//!
//! This module is pure logic so the macOS source's tests can be
//! ported 1:1. The actual confetti rendering (canvas-confetti when
//! the popup is open at reset moment) lives in the React layer; this
//! file just answers "should we fire?".

use std::time::{Duration, SystemTime};

/// Minimum utilization (0–100) required to fire a celebration. Spec
/// 80 §4 calls this out explicitly so the value is locked behind a
/// named constant rather than a magic number.
pub const MIN_UTILIZATION_PCT: f32 = 1.0;

/// Coalescing window: the same provider can't fire two celebrations
/// within 7 days. Prevents repeated firings if the reset boundary
/// flaps (e.g. clock skew on a laptop coming out of sleep).
pub const MIN_REFRACTORY: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Inputs the predicate consumes. Kept as plain fields so the
/// React-shadowing side can pass a JSON blob 1:1 (commit `b9996d86`
/// + spec 15 §card model).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CelebrationInputs {
    /// Percentage of the prior weekly window the user actually
    /// consumed, clamped to `[0, 100]`. `0.0` always returns false.
    pub past_24h_utilization_pct: f32,
    /// Unix epoch seconds of the previous reset moment for this
    /// provider. The predicate uses this against `now` to honour
    /// the refractory window.
    pub last_celebrated_at_unix_secs: Option<i64>,
}

/// Decide whether to fire a celebration for a reset that just
/// happened. Returns true iff:
///
///   - Utilization in the past 24h is at or above
///     `MIN_UTILIZATION_PCT`, AND
///   - The previous celebration (if any) was at least
///     `MIN_REFRACTORY` ago.
pub fn should_celebrate(inputs: CelebrationInputs, now: SystemTime) -> bool {
    if !inputs.past_24h_utilization_pct.is_finite() {
        return false;
    }
    if inputs.past_24h_utilization_pct < MIN_UTILIZATION_PCT {
        return false;
    }
    let Ok(now_secs) = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
    else {
        return false;
    };
    if let Some(prev) = inputs.last_celebrated_at_unix_secs {
        if now_secs.saturating_sub(prev) < MIN_REFRACTORY.as_secs() as i64 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(unix_secs: i64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(unix_secs as u64)
    }

    #[test]
    fn fires_above_threshold_with_no_history() {
        let inputs = CelebrationInputs {
            past_24h_utilization_pct: 5.0,
            last_celebrated_at_unix_secs: None,
        };
        assert!(should_celebrate(inputs, at(1_700_000_000)));
    }

    #[test]
    fn does_not_fire_below_threshold() {
        let inputs = CelebrationInputs {
            past_24h_utilization_pct: 0.5,
            last_celebrated_at_unix_secs: None,
        };
        assert!(!should_celebrate(inputs, at(1_700_000_000)));
    }

    #[test]
    fn does_not_fire_at_zero_utilization() {
        let inputs = CelebrationInputs {
            past_24h_utilization_pct: 0.0,
            last_celebrated_at_unix_secs: None,
        };
        assert!(!should_celebrate(inputs, at(1_700_000_000)));
    }

    #[test]
    fn refractory_blocks_repeat_celebrations() {
        let now = 1_700_000_000;
        let just_now = now - 60 * 60; // 1 hour ago
        let inputs = CelebrationInputs {
            past_24h_utilization_pct: 50.0,
            last_celebrated_at_unix_secs: Some(just_now),
        };
        assert!(!should_celebrate(inputs, at(now)));
    }

    #[test]
    fn refractory_clears_after_seven_days() {
        let now = 1_700_000_000;
        let week_ago = now - 7 * 24 * 60 * 60 - 1;
        let inputs = CelebrationInputs {
            past_24h_utilization_pct: 50.0,
            last_celebrated_at_unix_secs: Some(week_ago),
        };
        assert!(should_celebrate(inputs, at(now)));
    }

    #[test]
    fn rejects_non_finite_utilization() {
        let inputs = CelebrationInputs {
            past_24h_utilization_pct: f32::NAN,
            last_celebrated_at_unix_secs: None,
        };
        assert!(!should_celebrate(inputs, at(1_700_000_000)));

        let inputs = CelebrationInputs {
            past_24h_utilization_pct: f32::INFINITY,
            last_celebrated_at_unix_secs: None,
        };
        assert!(!should_celebrate(inputs, at(1_700_000_000)));
    }

    #[test]
    fn boundary_value_at_exactly_one_percent_fires() {
        let inputs = CelebrationInputs {
            past_24h_utilization_pct: MIN_UTILIZATION_PCT,
            last_celebrated_at_unix_secs: None,
        };
        assert!(should_celebrate(inputs, at(1_700_000_000)));
    }
}
