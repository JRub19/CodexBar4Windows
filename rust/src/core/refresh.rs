//! Refresh loop skeleton.
//!
//! Phase 1 ships the cadence machinery without any real fetch work. The
//! loop:
//!
//! 1. Sleeps for the configured cadence (or waits for a manual trigger when
//!    `Manual`).
//! 2. Bails if a tick is already in flight.
//! 3. Honors `pause_refresh = true` by skipping the tick.
//! 4. Iterates the (currently empty) provider registry, wrapping each
//!    strategy in a 45 second `tokio::time::timeout`.
//! 5. Folds results into `UsageStore`. Phase 1 writes nothing.
//! 6. Loops.
//!
//! Phase 4 (Claude) replaces the inner body with real dispatch.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::settings::SettingsHandle;

pub const PER_STRATEGY_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug, Error)]
pub enum RefreshError {
    #[error("refresh is paused")]
    Paused,
    #[error("tick already in flight")]
    Reentry,
}

pub struct RefreshLoop {
    settings: SettingsHandle,
    in_flight: AtomicBool,
    manual_trigger: Notify,
}

impl RefreshLoop {
    pub fn new(settings: SettingsHandle) -> Arc<Self> {
        Arc::new(Self {
            settings,
            in_flight: AtomicBool::new(false),
            manual_trigger: Notify::new(),
        })
    }

    /// Run a single tick. Returns:
    /// - `Err(Paused)` if `pause_refresh = true`.
    /// - `Err(Reentry)` if a tick is already in flight.
    /// - `Ok(())` once the tick has completed.
    pub async fn tick(self: &Arc<Self>) -> Result<(), RefreshError> {
        let snapshot = self.settings.snapshot();
        if snapshot.pause_refresh {
            return Err(RefreshError::Paused);
        }
        if self
            .in_flight
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(RefreshError::Reentry);
        }
        let tick_id = uuid_lite();
        info!(target: "codexbar::core::refresh", tick_id = %tick_id, "refresh.tick.start");

        // Phase 1: zero providers. Phase 4 fills this in.
        let providers: Vec<&'static str> = snapshot
            .providers
            .iter()
            .filter(|p| p.enabled)
            .map(|_| "")
            .collect();
        for _ in providers {
            // For each enabled provider we would dispatch strategies wrapped
            // in `tokio::time::timeout(PER_STRATEGY_TIMEOUT, ...)` and fold
            // the result into the usage store. There are zero providers in
            // phase 1, so this loop never runs.
        }

        info!(target: "codexbar::core::refresh", tick_id = %tick_id, "refresh.tick.end");
        self.in_flight.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Trigger a manual tick. Returns the same errors as `tick`.
    pub async fn refresh_now(self: &Arc<Self>) -> Result<(), RefreshError> {
        self.tick().await
    }

    /// Spawn the background interval loop. Returns the join handle; the
    /// loop terminates when the handle is aborted (the desktop shell does
    /// this on quit).
    pub fn spawn(self: Arc<Self>) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let snapshot = self.settings.snapshot();
                let cadence = snapshot.refresh_frequency.as_duration();
                match cadence {
                    Some(duration) => {
                        tokio::time::sleep(duration).await;
                    }
                    None => {
                        // Manual mode: wait indefinitely for a manual trigger.
                        self.manual_trigger.notified().await;
                    }
                }
                match self.tick().await {
                    Ok(()) => {}
                    Err(RefreshError::Paused) => {
                        info!(target: "codexbar::core::refresh", "refresh.skipped.paused")
                    }
                    Err(RefreshError::Reentry) => {
                        warn!(target: "codexbar::core::refresh", "refresh.skipped.reentry")
                    }
                }
            }
        })
    }

    pub fn nudge(&self) {
        self.manual_trigger.notify_one();
    }
}

fn uuid_lite() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{RefreshFrequency, SettingsPatch, SettingsStore};

    fn settings_with(freq: RefreshFrequency, paused: bool) -> SettingsHandle {
        let tmp = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(tmp.path().join("config.json"));
        store
            .update(SettingsPatch {
                refresh_frequency: Some(freq),
                pause_refresh: Some(paused),
                ..Default::default()
            })
            .unwrap();
        std::mem::forget(tmp); // keep dir alive for the test; small leak is fine
        std::sync::Arc::new(store)
    }

    #[test]
    fn tick_skips_when_paused() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let settings = settings_with(RefreshFrequency::OneMinute, true);
            let loop_ref = RefreshLoop::new(settings);
            let result = loop_ref.tick().await;
            assert!(matches!(result, Err(RefreshError::Paused)));
        });
    }

    #[test]
    fn tick_completes_when_unpaused() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let settings = settings_with(RefreshFrequency::OneMinute, false);
            let loop_ref = RefreshLoop::new(settings);
            let result = loop_ref.tick().await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn concurrent_tick_reports_reentry() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let settings = settings_with(RefreshFrequency::OneMinute, false);
            let loop_ref = RefreshLoop::new(settings);
            // Force in_flight true to simulate a concurrent tick.
            loop_ref.in_flight.store(true, Ordering::SeqCst);
            let result = loop_ref.tick().await;
            assert!(matches!(result, Err(RefreshError::Reentry)));
        });
    }
}
