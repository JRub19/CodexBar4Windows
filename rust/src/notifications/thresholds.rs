//! Threshold-warning rules. A "fired" threshold does not fire again
//! until `remaining` climbs back above it (which clears the fired
//! set). The macOS default thresholds are 50 / 25 / 10.

use std::collections::BTreeSet;

/// Default warning thresholds in percent-remaining units, ported from
/// the macOS source.
pub const DEFAULT_THRESHOLDS: &[i64] = &[50, 25, 10];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThresholdEvent {
    pub threshold: i64,
}

/// Clean a user-supplied threshold list: drop duplicates, clamp to
/// `[1, 99]`, sort descending (so we fire the highest crossed first
/// — matches the macOS sort).
pub fn sanitize_thresholds(raw: &[i64]) -> Vec<i64> {
    let mut set: BTreeSet<i64> = BTreeSet::new();
    for v in raw {
        if (1..=99).contains(v) {
            set.insert(*v);
        }
    }
    let mut out: Vec<i64> = set.into_iter().collect();
    out.sort_by(|a, b| b.cmp(a));
    out
}

/// Return the *largest* threshold that has just been crossed,
/// excluding anything already fired. `None` when nothing crossed.
///
/// We pick the largest crossed (e.g. if user fell from 60 % straight
/// past 50 % and 25 % in one tick) so the notification text shows
/// the most informative number; subsequent ticks below 25 % will
/// fire its dedicated 25 % event because it remains unfired.
pub fn crossed_threshold(
    previous_remaining: Option<f64>,
    current_remaining: f64,
    thresholds: &[i64],
    already_fired: &BTreeSet<i64>,
) -> Option<ThresholdEvent> {
    let sanitized = sanitize_thresholds(thresholds);
    let eligible: Vec<i64> = sanitized
        .into_iter()
        .filter(|t| current_remaining <= *t as f64 && !already_fired.contains(t))
        .collect();
    if eligible.is_empty() {
        return None;
    }
    let chosen = if let Some(prev) = previous_remaining {
        // Strictly crossed: prev > t && current <= t.
        eligible.iter().copied().filter(|t| prev > *t as f64).min()
    } else {
        // First observation: deliberately do not fire. The macOS
        // behaviour fires on first observation, but it has a
        // persisted "previous" from the last session. We do not
        // persist notification state, so firing on first observation
        // would mean toasts on every app start. Safer to wait one tick.
        None
    };
    chosen.map(|t| ThresholdEvent { threshold: t })
}

/// Once we fire threshold `t`, every threshold at or above `t` is
/// considered fired too — the user is past those without needing a
/// stale "you crossed 50 %" toast after we already told them about 25 %.
pub fn fired_after(threshold: i64, thresholds: &[i64]) -> BTreeSet<i64> {
    sanitize_thresholds(thresholds)
        .into_iter()
        .filter(|t| *t >= threshold)
        .collect()
}

/// When `remaining` climbs above a previously-fired threshold, that
/// threshold is re-armed for future notifications.
pub fn thresholds_to_clear(current_remaining: f64, already_fired: &BTreeSet<i64>) -> BTreeSet<i64> {
    already_fired
        .iter()
        .copied()
        .filter(|t| current_remaining > *t as f64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_drops_out_of_range_and_dupes() {
        assert_eq!(sanitize_thresholds(&[50, 50, 25, 10]), vec![50, 25, 10]);
        assert_eq!(sanitize_thresholds(&[0, 100, 50]), vec![50]);
        assert_eq!(sanitize_thresholds(&[-5, 75, 25]), vec![75, 25]);
    }

    #[test]
    fn no_crossing_when_remaining_above_all_thresholds() {
        let fired = BTreeSet::new();
        assert!(crossed_threshold(Some(80.0), 60.0, DEFAULT_THRESHOLDS, &fired).is_none());
    }

    #[test]
    fn fires_the_smallest_just_crossed_threshold() {
        // Previous 60 → current 40: crossed 50 only.
        let fired = BTreeSet::new();
        let event = crossed_threshold(Some(60.0), 40.0, DEFAULT_THRESHOLDS, &fired).unwrap();
        assert_eq!(event.threshold, 50);
    }

    #[test]
    fn skips_already_fired_thresholds() {
        let mut fired = BTreeSet::new();
        fired.insert(50);
        // Previous 40 → current 30 → no eligible threshold (50 fired,
        // 25 not yet crossed).
        assert!(crossed_threshold(Some(40.0), 30.0, DEFAULT_THRESHOLDS, &fired).is_none());
        // Previous 30 → current 20: crosses 25.
        let event = crossed_threshold(Some(30.0), 20.0, DEFAULT_THRESHOLDS, &fired).unwrap();
        assert_eq!(event.threshold, 25);
    }

    #[test]
    fn first_observation_returns_none_to_avoid_startup_toasts() {
        let fired = BTreeSet::new();
        // No previous → we deliberately wait one tick before firing.
        assert!(crossed_threshold(None, 20.0, DEFAULT_THRESHOLDS, &fired).is_none());
        assert!(crossed_threshold(None, 5.0, DEFAULT_THRESHOLDS, &fired).is_none());
    }

    #[test]
    fn fired_after_includes_all_higher_thresholds() {
        // After firing 25 we should auto-mark 50 as fired too.
        let after = fired_after(25, DEFAULT_THRESHOLDS);
        assert!(after.contains(&50));
        assert!(after.contains(&25));
        assert!(!after.contains(&10));
    }

    #[test]
    fn thresholds_to_clear_when_remaining_climbs_back() {
        let mut fired = BTreeSet::new();
        fired.insert(50);
        fired.insert(25);
        // Remaining climbs back to 60 (well above both fired): both
        // 50 and 25 are re-armed.
        let to_clear = thresholds_to_clear(60.0, &fired);
        assert_eq!(to_clear, BTreeSet::from([50, 25]));
        // Remaining 30: user is back above 25 but still below 50.
        // Only 25 re-arms; 50 stays fired.
        let to_clear = thresholds_to_clear(30.0, &fired);
        assert_eq!(to_clear, BTreeSet::from([25]));
    }

    #[test]
    fn jumping_past_multiple_thresholds_fires_smallest_crossed() {
        // Falls from 80 to 5: every threshold in the default list
        // crossed in one tick. We fire the smallest (10) so the toast
        // text is the most informative.
        let fired = BTreeSet::new();
        let event = crossed_threshold(Some(80.0), 5.0, DEFAULT_THRESHOLDS, &fired).unwrap();
        assert_eq!(event.threshold, 10);
    }
}
