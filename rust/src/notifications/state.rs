//! Notification state store. Tracks the per-window previous remaining
//! percent + the set of fired thresholds so the threshold logic
//! knows which alerts have already gone out.
//!
//! This is the layer that converts a UsageSnapshot into a list of
//! pending toasts on each refresh tick.

use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

use super::thresholds::{
    crossed_threshold, fired_after, sanitize_thresholds, thresholds_to_clear, ThresholdEvent,
};
use super::transition::{transition_for, SessionTransition};

/// `(provider_id, window_key)`. Threshold state is tracked per
/// window so the user can be warned on session and weekly
/// independently.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NotificationKey {
    pub provider_id: String,
    pub window_key: String,
}

impl NotificationKey {
    pub fn new(provider_id: impl Into<String>, window_key: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            window_key: window_key.into(),
        }
    }
}

#[derive(Default)]
struct PerWindow {
    previous_remaining: Option<f64>,
    fired_thresholds: BTreeSet<i64>,
    previously_depleted: bool,
}

pub struct NotificationStateStore {
    inner: Mutex<HashMap<NotificationKey, PerWindow>>,
}

impl Default for NotificationStateStore {
    fn default() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NotificationDecision {
    /// Nothing to do.
    None,
    /// Session-transition toast.
    Transition(SessionTransition),
    /// Threshold crossed.
    Threshold(ThresholdEvent),
}

impl NotificationStateStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Step the state machine for one window. Returns the decision the
    /// caller should act on (post a toast, etc.) and updates internal
    /// state for the next tick.
    ///
    /// Transitions take priority over thresholds when both fire on
    /// the same tick — the user just hit zero so the "depleted" toast
    /// is more informative than "10 % left".
    pub fn step(
        &self,
        key: &NotificationKey,
        current_remaining: f64,
        thresholds: &[i64],
    ) -> NotificationDecision {
        let mut inner = self.inner.lock().expect("notification store mutex");
        let entry = inner.entry(key.clone()).or_default();
        let previous = entry.previous_remaining;

        // 1. Session transition.
        let transition = transition_for(previous, current_remaining);
        // 2. Threshold check uses the *pre-step* fired set so a fresh
        //    fire is recorded against the current observation.
        let fired = entry.fired_thresholds.clone();
        let crossed = crossed_threshold(previous, current_remaining, thresholds, &fired);

        // Re-arm thresholds whose remaining is now back above them.
        let to_clear = thresholds_to_clear(current_remaining, &entry.fired_thresholds);
        for t in &to_clear {
            entry.fired_thresholds.remove(t);
        }

        // Record post-step state.
        entry.previous_remaining = Some(current_remaining);

        match (transition, crossed) {
            (SessionTransition::Depleted, _) => {
                entry.previously_depleted = true;
                // Depletion implies every threshold is past — mark them
                // all fired so we do not flap notifications on the way
                // back up.
                for t in sanitize_thresholds(thresholds) {
                    entry.fired_thresholds.insert(t);
                }
                NotificationDecision::Transition(SessionTransition::Depleted)
            }
            (SessionTransition::Restored, _) => {
                entry.previously_depleted = false;
                // Restoration re-arms every threshold.
                entry.fired_thresholds.clear();
                NotificationDecision::Transition(SessionTransition::Restored)
            }
            (SessionTransition::None, Some(event)) => {
                let extra = fired_after(event.threshold, thresholds);
                for t in extra {
                    entry.fired_thresholds.insert(t);
                }
                entry.fired_thresholds.insert(event.threshold);
                NotificationDecision::Threshold(event)
            }
            _ => NotificationDecision::None,
        }
    }

    /// Forget all state for a provider id. Called when the user
    /// disables the provider so we do not fire stale toasts after
    /// they re-enable it later.
    pub fn forget_provider(&self, provider_id: &str) {
        let mut inner = self.inner.lock().expect("notification store mutex");
        inner.retain(|k, _| k.provider_id != provider_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::thresholds::DEFAULT_THRESHOLDS;

    fn key() -> NotificationKey {
        NotificationKey::new("claude", "session")
    }

    #[test]
    fn first_observation_returns_none() {
        let store = NotificationStateStore::new();
        let dec = store.step(&key(), 70.0, DEFAULT_THRESHOLDS);
        assert_eq!(dec, NotificationDecision::None);
    }

    #[test]
    fn crossing_50_fires_once_per_descent() {
        let store = NotificationStateStore::new();
        // Seed.
        store.step(&key(), 70.0, DEFAULT_THRESHOLDS);
        // Drop to 40 → cross 50.
        let dec = store.step(&key(), 40.0, DEFAULT_THRESHOLDS);
        match dec {
            NotificationDecision::Threshold(e) => assert_eq!(e.threshold, 50),
            other => panic!("expected Threshold(50), got {other:?}"),
        }
        // Stays at 40 → no re-fire.
        assert_eq!(
            store.step(&key(), 40.0, DEFAULT_THRESHOLDS),
            NotificationDecision::None
        );
        // Drops further to 20 → fires 25 (50 already fired).
        match store.step(&key(), 20.0, DEFAULT_THRESHOLDS) {
            NotificationDecision::Threshold(e) => assert_eq!(e.threshold, 25),
            other => panic!("expected Threshold(25), got {other:?}"),
        }
    }

    #[test]
    fn restoration_clears_fired_thresholds() {
        let store = NotificationStateStore::new();
        store.step(&key(), 60.0, DEFAULT_THRESHOLDS);
        store.step(&key(), 40.0, DEFAULT_THRESHOLDS); // fires 50
        store.step(&key(), 0.0, DEFAULT_THRESHOLDS); // depleted
        // Climbing back up: restored toast fires, fired set is reset.
        let dec = store.step(&key(), 30.0, DEFAULT_THRESHOLDS);
        assert_eq!(
            dec,
            NotificationDecision::Transition(SessionTransition::Restored)
        );
        // Subsequent drop should be able to fire 50 again.
        store.step(&key(), 70.0, DEFAULT_THRESHOLDS);
        let dec = store.step(&key(), 40.0, DEFAULT_THRESHOLDS);
        match dec {
            NotificationDecision::Threshold(e) => assert_eq!(e.threshold, 50),
            other => panic!("expected Threshold(50) post-restoration, got {other:?}"),
        }
    }

    #[test]
    fn depletion_suppresses_threshold_on_same_tick() {
        let store = NotificationStateStore::new();
        store.step(&key(), 60.0, DEFAULT_THRESHOLDS);
        // Drops to 0 — should fire Depleted, not Threshold(50).
        let dec = store.step(&key(), 0.0, DEFAULT_THRESHOLDS);
        assert_eq!(
            dec,
            NotificationDecision::Transition(SessionTransition::Depleted)
        );
    }

    #[test]
    fn climbing_above_a_fired_threshold_re_arms_it() {
        let store = NotificationStateStore::new();
        store.step(&key(), 60.0, DEFAULT_THRESHOLDS);
        store.step(&key(), 40.0, DEFAULT_THRESHOLDS); // 50 fired
        store.step(&key(), 60.0, DEFAULT_THRESHOLDS); // re-arm 50
        // Now drop again past 50 — should fire again.
        let dec = store.step(&key(), 30.0, DEFAULT_THRESHOLDS);
        match dec {
            NotificationDecision::Threshold(e) => assert_eq!(e.threshold, 50),
            other => panic!("expected re-armed Threshold(50), got {other:?}"),
        }
    }

    #[test]
    fn forget_provider_drops_all_window_state() {
        let store = NotificationStateStore::new();
        store.step(&key(), 40.0, DEFAULT_THRESHOLDS);
        store.forget_provider("claude");
        // Subsequent ticks behave as if first observation.
        assert_eq!(
            store.step(&key(), 30.0, DEFAULT_THRESHOLDS),
            NotificationDecision::None,
            "first observation after forget returns None"
        );
    }

    #[test]
    fn provider_isolation_in_state_store() {
        let store = NotificationStateStore::new();
        let claude = NotificationKey::new("claude", "session");
        let codex = NotificationKey::new("codex", "session");
        store.step(&claude, 60.0, DEFAULT_THRESHOLDS);
        store.step(&codex, 60.0, DEFAULT_THRESHOLDS);
        // Claude crosses 50, Codex does not.
        let claude_dec = store.step(&claude, 40.0, DEFAULT_THRESHOLDS);
        assert!(matches!(
            claude_dec,
            NotificationDecision::Threshold(_)
        ));
        // Codex stays at 60; no toast.
        let codex_dec = store.step(&codex, 60.0, DEFAULT_THRESHOLDS);
        assert_eq!(codex_dec, NotificationDecision::None);
    }
}
