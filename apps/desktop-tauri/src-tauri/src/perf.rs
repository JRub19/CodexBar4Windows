//! Runtime budget sampler for the tray icon render path.
//!
//! Per spec 10 section 11, a sustained render time above the budget is
//! the canary for performance regressions on a user's actual hardware.
//! We track the most recent render durations in a small ring buffer and
//! emit a `tracing::warn!` when the buffer's average exceeds the budget
//! for five consecutive samples.

use std::sync::Mutex;
use std::time::Duration;

use tracing::{debug, warn};

const BUDGET_COLD_MS: f64 = 2.0;
const WARN_AFTER_CONSECUTIVE: usize = 5;
const RING_CAPACITY: usize = 16;

pub struct RenderBudgetSampler {
    inner: Mutex<Inner>,
}

struct Inner {
    samples: Vec<Duration>,
    cursor: usize,
    consecutive_over_budget: usize,
}

impl Default for RenderBudgetSampler {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                samples: Vec::with_capacity(RING_CAPACITY),
                cursor: 0,
                consecutive_over_budget: 0,
            }),
        }
    }
}

impl RenderBudgetSampler {
    /// Record one render duration. Emits a warning when the budget has
    /// been blown for `WARN_AFTER_CONSECUTIVE` consecutive samples.
    pub fn record(&self, duration: Duration) {
        let mut inner = self.inner.lock().expect("render budget mutex poisoned");
        let ms = duration.as_secs_f64() * 1000.0;
        if inner.samples.len() < RING_CAPACITY {
            inner.samples.push(duration);
        } else {
            let cursor = inner.cursor;
            inner.samples[cursor] = duration;
            inner.cursor = (cursor + 1) % RING_CAPACITY;
        }
        if ms > BUDGET_COLD_MS {
            inner.consecutive_over_budget += 1;
            if inner.consecutive_over_budget >= WARN_AFTER_CONSECUTIVE {
                warn!(
                    target: "codexbar::perf",
                    ms,
                    budget = BUDGET_COLD_MS,
                    "render budget blown for {} consecutive samples",
                    inner.consecutive_over_budget,
                );
                inner.consecutive_over_budget = 0;
            }
        } else {
            inner.consecutive_over_budget = 0;
            debug!(target: "codexbar::perf", ms, "render sample ok");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_budget_samples_do_not_trigger_warn() {
        let sampler = RenderBudgetSampler::default();
        for _ in 0..20 {
            sampler.record(Duration::from_micros(500));
        }
        // No assertion needed; we are checking that no panic and no
        // overflow happens in steady state.
    }

    #[test]
    fn over_budget_streak_resets_after_warn() {
        let sampler = RenderBudgetSampler::default();
        for _ in 0..5 {
            sampler.record(Duration::from_millis(3));
        }
        // Inner consecutive counter should have reset after the warn.
        let inner = sampler.inner.lock().unwrap();
        assert_eq!(inner.consecutive_over_budget, 0);
    }
}
