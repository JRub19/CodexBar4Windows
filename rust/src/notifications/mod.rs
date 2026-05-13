//! Session-quota and threshold-warning notifications. Ported from
//! `Sources/CodexBar/SessionQuotaNotifications.swift`.
//!
//! Two distinct notification types ride on the same dedup machinery:
//!
//! 1. **Session transitions** — fired when the primary quota window
//!    flips between "available" and "depleted". Mirrors the macOS
//!    `.depleted` / `.restored` toast pair so the user is told once
//!    when they hit zero and once when the window reopens.
//! 2. **Threshold warnings** — fired when the remaining percentage
//!    crosses a configured threshold (e.g. 50 %, 25 %, 10 %). Each
//!    threshold fires at most once per window; thresholds are
//!    re-armed when remaining climbs back above them.
//!
//! The pure logic (transition + crossed-threshold detector + state
//! tracker) lives in this module so we can test every edge case
//! without an OS notification handle.

pub mod state;
pub mod thresholds;
pub mod toast;
pub mod transition;

pub use state::{NotificationKey, NotificationStateStore};
pub use thresholds::{crossed_threshold, sanitize_thresholds, ThresholdEvent, DEFAULT_THRESHOLDS};
pub use toast::{copy_for_threshold, copy_for_transition, NotificationToast};
pub use transition::{transition_for, SessionTransition};
