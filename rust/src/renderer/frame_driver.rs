//! 30 Hz frame driver for tray icon animations.
//!
//! Per `docs/windows/spec/80-feel-and-polish.md`, the icon animation
//! cadence is 30 Hz with a hard ceiling of 30 seconds total runtime.
//! Battery saver mode drops to 5 Hz. The driver is implementation
//! agnostic: it computes the *next deadline* given a wall clock instant
//! and lets the caller (the desktop shell's tokio runtime) sleep to it.
//!
//! This module is intentionally side effect free so the tray host can
//! unit test the cadence logic without spinning a real runtime.

use std::time::{Duration, Instant};

/// Default frame interval: 30 Hz.
pub const NORMAL_FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// Low power frame interval: 5 Hz.
pub const LOW_POWER_FRAME_INTERVAL: Duration = Duration::from_millis(200);

/// Hard ceiling after which animation must stop, regardless of mode.
pub const ANIMATION_CEILING: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerMode {
    Normal,
    LowPower,
    /// Phase 9 §C-3: the OS reports `prefers-reduced-motion`. We stop
    /// animating the icon entirely (no morph, no critter blink) — the
    /// tray icon repaints only on state change. This is stricter than
    /// `LowPower` because the user has explicitly asked for reduced
    /// motion, not just reduced power consumption.
    ReducedMotion,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameDriver {
    started_at: Instant,
    mode: PowerMode,
    last_frame: Option<Instant>,
}

impl FrameDriver {
    pub fn new(mode: PowerMode) -> Self {
        Self {
            started_at: Instant::now(),
            mode,
            last_frame: None,
        }
    }

    pub fn set_mode(&mut self, mode: PowerMode) {
        self.mode = mode;
    }

    /// Interval between frames in the active power mode.
    pub fn interval(&self) -> Duration {
        match self.mode {
            PowerMode::Normal => NORMAL_FRAME_INTERVAL,
            PowerMode::LowPower => LOW_POWER_FRAME_INTERVAL,
            // Reduced-motion gets a huge interval so the loop still
            // advances cleanly to the ceiling (avoids special-casing
            // the caller) but never paints in practice.
            PowerMode::ReducedMotion => ANIMATION_CEILING,
        }
    }

    /// Compute the next frame instant. Returns `None` once the
    /// [`ANIMATION_CEILING`] has elapsed, or immediately in
    /// `ReducedMotion` mode (no animation frames at all).
    pub fn next_frame_at(&self, now: Instant) -> Option<Instant> {
        if matches!(self.mode, PowerMode::ReducedMotion) {
            return None;
        }
        if now.duration_since(self.started_at) >= ANIMATION_CEILING {
            return None;
        }
        let base = self.last_frame.unwrap_or(self.started_at);
        Some(base + self.interval())
    }

    /// Mark a frame as rendered at the given instant. Call this from the
    /// driving loop after each successful paint.
    pub fn note_frame(&mut self, at: Instant) {
        self.last_frame = Some(at);
    }

    /// Return how long the driver has been animating.
    pub fn elapsed(&self, now: Instant) -> Duration {
        now.duration_since(self.started_at)
    }

    /// True if the ceiling has been reached.
    pub fn is_done(&self, now: Instant) -> bool {
        self.elapsed(now) >= ANIMATION_CEILING
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_mode_uses_33_ms_interval() {
        let d = FrameDriver::new(PowerMode::Normal);
        assert_eq!(d.interval(), NORMAL_FRAME_INTERVAL);
    }

    #[test]
    fn low_power_mode_uses_200_ms_interval() {
        let d = FrameDriver::new(PowerMode::LowPower);
        assert_eq!(d.interval(), LOW_POWER_FRAME_INTERVAL);
    }

    #[test]
    fn next_frame_advances_from_start() {
        let d = FrameDriver::new(PowerMode::Normal);
        let now = d.started_at;
        let next = d.next_frame_at(now).expect("not at ceiling");
        assert_eq!(next - d.started_at, NORMAL_FRAME_INTERVAL);
    }

    #[test]
    fn next_frame_returns_none_after_ceiling() {
        let d = FrameDriver::new(PowerMode::Normal);
        let now = d.started_at + ANIMATION_CEILING;
        assert!(d.next_frame_at(now).is_none());
    }

    #[test]
    fn is_done_flips_at_ceiling() {
        let d = FrameDriver::new(PowerMode::Normal);
        assert!(!d.is_done(d.started_at));
        assert!(d.is_done(d.started_at + ANIMATION_CEILING));
    }

    #[test]
    fn mode_change_takes_effect_immediately() {
        let mut d = FrameDriver::new(PowerMode::Normal);
        assert_eq!(d.interval(), NORMAL_FRAME_INTERVAL);
        d.set_mode(PowerMode::LowPower);
        assert_eq!(d.interval(), LOW_POWER_FRAME_INTERVAL);
    }

    #[test]
    fn reduced_motion_yields_no_frames() {
        // Phase 9 §C-3: when the OS reports prefers-reduced-motion,
        // the driver returns None for next_frame_at unconditionally
        // so the caller loops directly to is_done() without ever
        // painting an in-between frame.
        let d = FrameDriver::new(PowerMode::ReducedMotion);
        assert!(d.next_frame_at(d.started_at).is_none());
        // Even at t=0, the answer is None — no animation start frame.
        assert!(d.next_frame_at(d.started_at + Duration::from_millis(1)).is_none());
    }

    #[test]
    fn switching_to_reduced_motion_stops_animation_mid_run() {
        let mut d = FrameDriver::new(PowerMode::Normal);
        let now = d.started_at + Duration::from_millis(500);
        assert!(d.next_frame_at(now).is_some());
        d.set_mode(PowerMode::ReducedMotion);
        assert!(d.next_frame_at(now).is_none());
    }
}
