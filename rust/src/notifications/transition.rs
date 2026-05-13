//! Session-window transition detector. A window is "depleted" when
//! `remaining_percent <= DEPLETED_EPSILON`. The transition is the
//! diff between the previous and current snapshot per provider.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionTransition {
    /// No change worth notifying about (still available, still
    /// depleted, or we are seeing this provider for the first time).
    None,
    /// Crossed from available to depleted.
    Depleted,
    /// Crossed from depleted back to available.
    Restored,
}

/// Mirrors the macOS `depletedThreshold = 0.0001`. Anything at or
/// below this is treated as a hard zero so floating-point dust does
/// not bounce notifications.
pub const DEPLETED_EPSILON: f64 = 0.0001;

pub fn is_depleted(remaining_percent: f64) -> bool {
    remaining_percent <= DEPLETED_EPSILON
}

/// Compute the transition for a single window.
/// `previous` is `None` on the first observation; we never fire a
/// notification in that case.
pub fn transition_for(previous: Option<f64>, current: f64) -> SessionTransition {
    let Some(previous) = previous else {
        return SessionTransition::None;
    };
    let was_depleted = is_depleted(previous);
    let is_depleted = is_depleted(current);
    match (was_depleted, is_depleted) {
        (false, true) => SessionTransition::Depleted,
        (true, false) => SessionTransition::Restored,
        _ => SessionTransition::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_observation_never_fires() {
        assert_eq!(transition_for(None, 50.0), SessionTransition::None);
        assert_eq!(transition_for(None, 0.0), SessionTransition::None);
    }

    #[test]
    fn detects_depletion_only_when_crossing_zero() {
        // Was 5 %, now 0 % → depleted.
        assert_eq!(
            transition_for(Some(5.0), 0.0),
            SessionTransition::Depleted
        );
        // Was 5 %, now 4 % → no transition (still available).
        assert_eq!(transition_for(Some(5.0), 4.0), SessionTransition::None);
    }

    #[test]
    fn detects_restoration_when_crossing_back() {
        assert_eq!(
            transition_for(Some(0.0), 1.0),
            SessionTransition::Restored
        );
    }

    #[test]
    fn already_depleted_does_not_re_fire() {
        assert_eq!(transition_for(Some(0.0), 0.0), SessionTransition::None);
    }

    #[test]
    fn epsilon_dust_is_treated_as_zero() {
        // Floating point noise just below epsilon: still depleted.
        assert_eq!(
            transition_for(Some(0.00005), 0.0),
            SessionTransition::None
        );
        // Crossing above epsilon counts as restoration.
        assert_eq!(
            transition_for(Some(0.00005), 0.5),
            SessionTransition::Restored
        );
    }

    #[test]
    fn is_depleted_returns_false_for_low_but_nonzero_remaining() {
        assert!(!is_depleted(0.5));
        assert!(is_depleted(0.00001));
        assert!(is_depleted(0.0));
    }
}
