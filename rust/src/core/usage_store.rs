//! In memory store for per provider usage snapshots.
//!
//! Phase 1 ships the contract and the identity siloing invariant. The
//! `UsageState` struct is intentionally empty; later phases add per provider
//! slots, status snapshots, and cost summaries.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot_local::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;

use super::events::{UsageEvent, UsageUpdated};

/// Stable identifier for a provider. The string is `&'static str` so it
/// doubles as the persistence key without allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct ProviderId(pub &'static str);

impl ProviderId {
    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsageState {
    // Phase 1: empty. Phase 4 adds per provider snapshots.
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageSnapshot {
    pub identity: UsageIdentity,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageIdentity {
    pub provider_id: String,
    #[serde(default)]
    pub account_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("snapshot identity {got} does not match slot {expected}")]
    IdentityMismatch { expected: String, got: String },
}

/// Owned usage store with concurrent read access and broadcast updates.
pub struct UsageStore {
    state: Arc<RwLock<UsageState>>,
    menu_rev: AtomicU64,
    icon_rev: AtomicU64,
    tx: broadcast::Sender<UsageEvent>,
}

impl Default for UsageStore {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageStore {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(64);
        Self {
            state: Arc::new(RwLock::new(UsageState::default())),
            menu_rev: AtomicU64::new(0),
            icon_rev: AtomicU64::new(0),
            tx,
        }
    }

    /// Write a per provider snapshot. Enforces the identity siloing
    /// invariant: the snapshot must declare the same `provider_id` as the
    /// slot it is being filed under.
    pub fn write_snapshot(
        &self,
        provider: ProviderId,
        snapshot: UsageSnapshot,
    ) -> Result<UsageUpdated, StoreError> {
        if snapshot.identity.provider_id != provider.as_str() {
            return Err(StoreError::IdentityMismatch {
                expected: provider.as_str().to_string(),
                got: snapshot.identity.provider_id,
            });
        }
        // Phase 1: state mutation is a no-op. We still bump revisions so the
        // event channel exercises end to end.
        let _state_guard = self.state.write();
        let menu_rev = self.menu_rev.fetch_add(1, Ordering::SeqCst) + 1;
        let icon_rev = self.icon_rev.fetch_add(1, Ordering::SeqCst) + 1;
        let event = UsageUpdated {
            provider,
            menu_rev,
            icon_rev,
        };
        let _ = self.tx.send(UsageEvent::Updated(event.clone()));
        Ok(event)
    }

    pub fn menu_rev(&self) -> u64 {
        self.menu_rev.load(Ordering::SeqCst)
    }

    pub fn icon_rev(&self) -> u64 {
        self.icon_rev.load(Ordering::SeqCst)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<UsageEvent> {
        self.tx.subscribe()
    }
}

// Tiny shim so we do not need to pull in the external `parking_lot` crate
// just for an RwLock alias. Stdlib's lock is sufficient at phase 1 since
// reads are infrequent and short.
mod parking_lot_local {
    use std::sync::RwLock as StdRwLock;

    pub struct RwLock<T>(StdRwLock<T>);

    impl<T> RwLock<T> {
        pub fn new(value: T) -> Self {
            Self(StdRwLock::new(value))
        }
        #[allow(dead_code)] // phase 4 fills UsageState and adds read sites
        pub fn read(&self) -> std::sync::RwLockReadGuard<'_, T> {
            self.0.read().expect("usage store rwlock poisoned")
        }
        pub fn write(&self) -> std::sync::RwLockWriteGuard<'_, T> {
            self.0.write().expect("usage store rwlock poisoned")
        }
    }

    impl<T: std::fmt::Debug> std::fmt::Debug for RwLock<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "RwLock<{:?}>", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_with_matching_identity_succeeds_and_bumps_revs() {
        let store = UsageStore::new();
        let snap = UsageSnapshot {
            identity: UsageIdentity {
                provider_id: "claude".into(),
                ..Default::default()
            },
        };
        let event = store.write_snapshot(ProviderId("claude"), snap).unwrap();
        assert_eq!(event.menu_rev, 1);
        assert_eq!(event.icon_rev, 1);
        assert_eq!(store.menu_rev(), 1);
    }

    #[test]
    fn write_with_mismatched_identity_returns_error_and_does_not_bump() {
        let store = UsageStore::new();
        let snap = UsageSnapshot {
            identity: UsageIdentity {
                provider_id: "codex".into(),
                ..Default::default()
            },
        };
        let err = store
            .write_snapshot(ProviderId("claude"), snap)
            .expect_err("expected identity mismatch");
        assert!(matches!(err, StoreError::IdentityMismatch { .. }));
        assert_eq!(store.menu_rev(), 0);
        assert_eq!(store.icon_rev(), 0);
    }

    #[test]
    fn subscribers_receive_updates() {
        let store = Arc::new(UsageStore::new());
        let mut rx = store.subscribe();
        let producer = store.clone();
        let handle = std::thread::spawn(move || {
            let snap = UsageSnapshot {
                identity: UsageIdentity {
                    provider_id: "claude".into(),
                    ..Default::default()
                },
            };
            producer.write_snapshot(ProviderId("claude"), snap).unwrap()
        });
        let received = futures_lite_like_blocking_recv(&mut rx);
        match received {
            UsageEvent::Updated(u) => {
                assert_eq!(u.provider, ProviderId("claude"));
                assert!(u.menu_rev >= 1);
            }
        }
        handle.join().unwrap();
    }

    fn futures_lite_like_blocking_recv(rx: &mut broadcast::Receiver<UsageEvent>) -> UsageEvent {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .expect("timed out waiting for event")
                .expect("channel closed before event")
        })
    }
}
