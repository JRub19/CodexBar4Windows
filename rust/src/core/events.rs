//! Internal event types broadcast from the core to subscribers (the desktop
//! shell bridges these to Tauri events). DTOs that cross IPC live in the
//! `host` module (added in phase 1 task 8 via ts-rs).

use serde::Serialize;

use super::usage_store::ProviderId;

/// Internal event emitted whenever [`UsageStore`](super::UsageStore) writes
/// a snapshot. The desktop shell rebroadcasts as `usage:updated`. Phase 8
/// adds a `Deserialize`-friendly DTO mirror under `host::dto` for IPC.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct UsageUpdated {
    pub provider: ProviderId,
    pub menu_rev: u64,
    pub icon_rev: u64,
}

/// High level usage event channel payload.
#[derive(Clone, Debug)]
pub enum UsageEvent {
    Updated(UsageUpdated),
}
