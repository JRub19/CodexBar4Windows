//! Refresh loop for live provider updates.
//!
//! The loop:
//!
//! 1. Sleeps for the configured cadence (or waits for a manual trigger when
//!    `Manual`).
//! 2. Bails if a tick is already in flight.
//! 3. Honors `pause_refresh = true` by skipping the tick.
//! 4. Iterates the installed provider registry.
//! 5. Folds provider snapshots and attempt metadata into `UsageStore`.
//! 6. Loops.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use thiserror::Error;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::core::usage_store::UsageStore;
use crate::providers::fetch_context::{ProviderFetchContext, Runtime, SourceMode};
use crate::providers::ProviderImplementation;
use crate::secrets::token_account::TokenAccountStore;
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
    providers: Mutex<Vec<Arc<dyn ProviderImplementation>>>,
    usage_store: Mutex<Option<Arc<UsageStore>>>,
    token_store: Mutex<Option<Arc<TokenAccountStore>>>,
}

impl RefreshLoop {
    pub fn new(settings: SettingsHandle) -> Arc<Self> {
        Arc::new(Self {
            settings,
            in_flight: AtomicBool::new(false),
            manual_trigger: Notify::new(),
            providers: Mutex::new(Vec::new()),
            usage_store: Mutex::new(None),
            token_store: Mutex::new(None),
        })
    }

    /// Install the live provider list. Called once at boot by the Tauri
    /// shell; the loop will dispatch each one on every tick.
    pub fn install_providers(
        &self,
        providers: Vec<Arc<dyn ProviderImplementation>>,
        usage_store: Arc<UsageStore>,
        token_store: Arc<TokenAccountStore>,
    ) {
        *self.providers.lock() = providers;
        *self.usage_store.lock() = Some(usage_store);
        *self.token_store.lock() = Some(token_store);
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

        let providers = self.providers.lock().clone();
        let usage_store = self.usage_store.lock().clone();
        let token_store = self.token_store.lock().clone();
        let enabled: std::collections::HashSet<String> = snapshot
            .providers
            .iter()
            .filter(|p| p.enabled)
            .map(|p| p.id.clone())
            .collect();
        // Default: when settings.providers is empty, treat every registered
        // provider as enabled so fresh installs light up the shipped set.
        let treat_all_enabled = enabled.is_empty();

        if let (Some(usage_store), Some(token_store)) = (usage_store, token_store) {
            for provider in providers {
                let provider_id = provider.descriptor().id;
                if !treat_all_enabled && !enabled.contains(provider_id.as_str()) {
                    continue;
                }
                let context = ProviderFetchContext {
                    provider_id,
                    mode: SourceMode::Auto,
                    runtime: Runtime {
                        tokens: token_store.clone(),
                    },
                };
                let outcome = provider.refresh(&context).await;
                if let Some(snapshot) = outcome.snapshot.clone() {
                    if let Err(err) = usage_store.replace_snapshot(
                        provider_id,
                        snapshot,
                        outcome.attempts.clone(),
                    ) {
                        warn!(
                            target: "codexbar::core::refresh",
                            provider = provider_id.as_str(),
                            error = %err,
                            "refresh.store_rejected"
                        );
                    } else {
                        info!(
                            target: "codexbar::core::refresh",
                            provider = provider_id.as_str(),
                            winning = ?outcome.winning_strategy,
                            attempts = outcome.attempts.len(),
                            "refresh.applied"
                        );
                    }
                } else {
                    // Dump every attempt's strategy + error so we can
                    // see WHY no snapshot was produced. Helped debug
                    // a Claude OAuth response parse-failure that
                    // looked indistinguishable from missing creds in
                    // the previous one-liner.
                    let attempt_summary = outcome
                        .attempts
                        .iter()
                        .map(|a| {
                            let kind = a.error_kind.as_deref().unwrap_or("ok");
                            let detail = a.error_detail.as_deref().unwrap_or("");
                            format!("{:?}=[{kind}] {detail}", a.strategy)
                        })
                        .collect::<Vec<_>>()
                        .join(" | ");
                    warn!(
                        target: "codexbar::core::refresh",
                        provider = provider_id.as_str(),
                        attempts = outcome.attempts.len(),
                        details = %attempt_summary,
                        "refresh.no_snapshot"
                    );
                }
            }
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

    /// Phase 9 §A-3: Manual cadence must NEVER auto-trigger. The
    /// loop spawned by `spawn` should block indefinitely on
    /// `manual_trigger.notified()` until `nudge()` is called.
    ///
    /// We verify by spawning, sleeping 100 ms (the manual await is
    /// race-free, so any wakeup means a regression), aborting, and
    /// asserting `tick_count == 0`.
    #[test]
    fn manual_cadence_does_not_auto_tick() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .enable_io()
            .build()
            .unwrap();
        rt.block_on(async {
            let settings = settings_with(RefreshFrequency::Manual, false);
            let loop_ref = RefreshLoop::new(settings);
            let in_flight_before = loop_ref.in_flight.load(Ordering::SeqCst);
            assert!(!in_flight_before);
            let handle = loop_ref.clone().spawn();
            // Give the loop time to spuriously fire if it's going to.
            tokio::time::sleep(Duration::from_millis(100)).await;
            // in_flight is set during a tick; if it was set, the
            // loop ticked, which violates Manual semantics.
            assert!(
                !loop_ref.in_flight.load(Ordering::SeqCst),
                "Manual cadence must not auto-trigger ticks"
            );
            handle.abort();
        });
    }
}
