//! Spec 30 section 12: an immutable view of one quota window, used both
//! for session windows (5h cycles on Claude, weekly on Cursor) and for
//! credit windows where a provider tracks consumed vs allotted.

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RateWindow {
    /// Friendly window label for the UI: "Session", "Week", "Opus 5h", etc.
    pub label: String,
    /// Used count in the window's natural unit (tokens, requests, USD).
    pub used: f64,
    /// Total allotted in the same unit. None when the provider does not
    /// expose a hard cap (some plans report only used).
    pub allotted: Option<f64>,
    /// Absolute reset instant, as a unix epoch seconds value. None when
    /// the provider gives a relative "resets in X" hint without a clock.
    pub reset_at_unix_secs: Option<i64>,
    /// Hint for the popup pace text: ahead/behind the linear projection
    /// in percentage points. Positive means ahead (will run out early).
    pub pace_delta_percent: Option<f32>,
}

impl RateWindow {
    /// Percent of the window still available. Clamps to `[0, 100]` so
    /// the icon never paints a negative or over-100% bar.
    pub fn remaining_percent(&self) -> f32 {
        let Some(total) = self.allotted else {
            return 100.0;
        };
        if total <= 0.0 {
            return 100.0;
        }
        let remaining = (total - self.used).max(0.0);
        let pct = (remaining / total) * 100.0;
        pct.clamp(0.0, 100.0) as f32
    }

    /// Wall clock duration until reset. `None` when the provider does
    /// not surface an absolute reset and the cached value has been
    /// invalidated.
    pub fn time_until_reset(&self, now: SystemTime) -> Option<Duration> {
        let reset = self.reset_at_unix_secs?;
        let now_secs = now.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs() as i64;
        if reset <= now_secs {
            return Some(Duration::ZERO);
        }
        Some(Duration::from_secs((reset - now_secs) as u64))
    }

    /// Spec 30 section 12.4: when the provider returns a reset that
    /// already passed, treat it as backfilling (the reset already
    /// happened, we just don't have new data yet) and return the
    /// cached future reset. Returns None when no future reset is known.
    pub fn backfilling_reset_time(
        &self,
        cached_future_reset: Option<i64>,
        now: SystemTime,
    ) -> Option<i64> {
        let now_secs = now.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs() as i64;
        match self.reset_at_unix_secs {
            Some(r) if r > now_secs => Some(r),
            _ => cached_future_reset,
        }
    }
}

/// A `RateWindow` plus the canonical name it is keyed under. The
/// `UsageSnapshot` stores a list of these so the popup can iterate in
/// declared order.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NamedRateWindow {
    pub key: String,
    pub window: RateWindow,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64)
    }

    #[test]
    fn remaining_percent_clamps_at_zero() {
        let w = RateWindow {
            label: "x".into(),
            used: 200.0,
            allotted: Some(100.0),
            reset_at_unix_secs: None,
            pace_delta_percent: None,
        };
        assert_eq!(w.remaining_percent(), 0.0);
    }

    #[test]
    fn remaining_percent_is_full_without_allotted() {
        let w = RateWindow {
            label: "x".into(),
            used: 0.0,
            allotted: None,
            reset_at_unix_secs: None,
            pace_delta_percent: None,
        };
        assert_eq!(w.remaining_percent(), 100.0);
    }

    #[test]
    fn time_until_reset_handles_past_reset() {
        let w = RateWindow {
            label: "x".into(),
            used: 0.0,
            allotted: None,
            reset_at_unix_secs: Some(100),
            pace_delta_percent: None,
        };
        assert_eq!(w.time_until_reset(at(200)), Some(Duration::ZERO));
    }

    #[test]
    fn backfilling_uses_cached_future_reset_when_payload_is_stale() {
        let w = RateWindow {
            label: "x".into(),
            used: 0.0,
            allotted: None,
            reset_at_unix_secs: Some(100), // already past
            pace_delta_percent: None,
        };
        let cached_future = Some(500);
        assert_eq!(w.backfilling_reset_time(cached_future, at(200)), Some(500));
    }

    #[test]
    fn backfilling_keeps_future_payload_when_it_is_fresh() {
        let w = RateWindow {
            label: "x".into(),
            used: 0.0,
            allotted: None,
            reset_at_unix_secs: Some(600),
            pace_delta_percent: None,
        };
        let cached_future = Some(500);
        assert_eq!(w.backfilling_reset_time(cached_future, at(200)), Some(600));
    }
}
