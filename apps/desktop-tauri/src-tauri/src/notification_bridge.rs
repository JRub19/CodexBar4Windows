//! Bridges `UsageStore` snapshot updates into desktop toasts via
//! `tauri-plugin-notification`. The pure logic — when to fire,
//! what copy to use — lives in `codexbar::notifications`. This file
//! is the OS-side glue that subscribes to the broadcast bus,
//! reads the latest snapshot from the store, and feeds it through
//! the state machine.
//!
//! Setting toggle: `Settings.notifications_enabled` (defaults to true).

use std::sync::Arc;
use std::time::Duration;

use codexbar::core::{UsageEvent, UsageStore};
use codexbar::notifications::state::NotificationDecision;
use codexbar::notifications::{
    copy_for_threshold, copy_for_transition, NotificationKey, NotificationStateStore,
    NotificationToast, DEFAULT_THRESHOLDS,
};
use codexbar::providers::REGISTRY;
use codexbar::settings::SettingsHandle;
use tauri_plugin_notification::NotificationExt;
use tracing::info;

pub struct NotificationBridge {
    state: Arc<NotificationStateStore>,
}

impl Default for NotificationBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationBridge {
    pub fn new() -> Self {
        Self {
            state: Arc::new(NotificationStateStore::new()),
        }
    }

    /// Spawn the bridge task. The task keeps a clone of the AppHandle
    /// so it can call into `tauri-plugin-notification`. Listens for
    /// `UsageEvent::Updated` and walks the latest snapshot through
    /// the notification state machine.
    pub fn spawn(
        self,
        runtime: &tokio::runtime::Runtime,
        usage_store: Arc<UsageStore>,
        settings: SettingsHandle,
        app_handle: Arc<parking_lot::Mutex<Option<tauri::AppHandle>>>,
    ) {
        let mut rx = usage_store.subscribe();
        let state = self.state.clone();
        runtime.spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(UsageEvent::Updated(update)) => {
                        if !settings.snapshot().notifications_enabled {
                            continue;
                        }
                        let provider_id = update.provider.as_str();
                        let Some(slot) = usage_store.slot(update.provider) else {
                            continue;
                        };
                        let snapshot = slot.snapshot;
                        let display = REGISTRY
                            .get(update.provider)
                            .map(|d| d.metadata.display_name.to_string())
                            .unwrap_or_else(|| provider_id.to_string());
                        let pending: Vec<NotificationToast> = snapshot
                            .windows
                            .iter()
                            .filter_map(|named| {
                                let remaining = named.window.remaining_percent() as f64;
                                let key = NotificationKey::new(provider_id, &named.key);
                                let decision = state.step(&key, remaining, DEFAULT_THRESHOLDS);
                                match decision {
                                    NotificationDecision::None => None,
                                    NotificationDecision::Transition(t) => {
                                        copy_for_transition(provider_id, &display, t)
                                    }
                                    NotificationDecision::Threshold(event) => {
                                        Some(copy_for_threshold(
                                            provider_id,
                                            &display,
                                            &named.window.label,
                                            &named.key,
                                            &event,
                                            remaining,
                                        ))
                                    }
                                }
                            })
                            .collect();
                        if pending.is_empty() {
                            continue;
                        }
                        let app = app_handle.lock().clone();
                        if let Some(app) = app {
                            for toast in pending {
                                dispatch(&app, &toast);
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        info!(
                            target: "codexbar::notifications",
                            skipped,
                            "notification bridge lagged",
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
    }

    pub fn state_store(&self) -> Arc<NotificationStateStore> {
        self.state.clone()
    }
}

fn dispatch(app: &tauri::AppHandle, toast: &NotificationToast) {
    let mut builder = app
        .notification()
        .builder()
        .title(&toast.title)
        .body(&toast.body);
    // The plugin's sound API takes the OS sound name; on Windows the
    // built-in "Default" plays the system notification chime.
    if toast.sound {
        builder = builder.sound("Default");
    }
    if let Err(err) = builder.show() {
        info!(
            target: "codexbar::notifications",
            id = %toast.id,
            error = %err,
            "notification dispatch failed",
        );
    }
}
