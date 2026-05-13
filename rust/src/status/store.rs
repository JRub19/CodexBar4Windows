//! In-memory `StatusStore`. Holds the most recent successful
//! `StatusSnapshot` per provider and is sticky on transient failures —
//! callers ask the store to apply a fetch result; the store decides
//! whether to overwrite (success) or keep the old value (failure with
//! prior data) or surface `Unknown` (failure with no prior data).

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::broadcast;

use super::feed::StatusSnapshot;
use super::severity::StatusSeverity;

#[derive(Clone, Debug)]
pub enum StatusEvent {
    Updated { provider_id: String },
    Cleared { provider_id: String },
}

#[derive(Clone)]
pub struct StatusStore {
    inner: Arc<Inner>,
}

struct Inner {
    snapshots: RwLock<HashMap<String, StatusSnapshot>>,
    sender: broadcast::Sender<StatusEvent>,
}

impl Default for StatusStore {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(64);
        Self {
            inner: Arc::new(Inner {
                snapshots: RwLock::new(HashMap::new()),
                sender,
            }),
        }
    }
}

impl StatusStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a successful fetch result. Overwrites the snapshot and
    /// emits an `Updated` event. Returns the previous snapshot for
    /// diff-aware callers.
    pub fn apply_success(&self, snapshot: StatusSnapshot) -> Option<StatusSnapshot> {
        let provider_id = snapshot.provider_id.clone();
        let previous = {
            let mut guard = self.inner.snapshots.write();
            guard.insert(provider_id.clone(), snapshot)
        };
        let _ = self.inner.sender.send(StatusEvent::Updated { provider_id });
        previous
    }

    /// Apply a fetch failure. If we already have a prior snapshot we
    /// keep it (sticky). If not, install a synthetic `Unknown`
    /// snapshot so the popup can still render the row.
    pub fn apply_failure(
        &self,
        provider_id: &str,
        title: Option<String>,
        status_page_url: Option<String>,
    ) {
        let already_present = { self.inner.snapshots.read().contains_key(provider_id) };
        if already_present {
            return;
        }
        let snap = StatusSnapshot::now(
            provider_id.to_string(),
            StatusSeverity::Unknown,
            title,
            None,
            status_page_url,
        );
        self.inner
            .snapshots
            .write()
            .insert(provider_id.to_string(), snap);
        let _ = self.inner.sender.send(StatusEvent::Updated {
            provider_id: provider_id.to_string(),
        });
    }

    pub fn get(&self, provider_id: &str) -> Option<StatusSnapshot> {
        self.inner.snapshots.read().get(provider_id).cloned()
    }

    pub fn all(&self) -> Vec<StatusSnapshot> {
        let mut out: Vec<StatusSnapshot> = self.inner.snapshots.read().values().cloned().collect();
        out.sort_by(|a, b| a.provider_id.cmp(&b.provider_id));
        out
    }

    pub fn clear(&self, provider_id: &str) {
        let removed = self.inner.snapshots.write().remove(provider_id);
        if removed.is_some() {
            let _ = self.inner.sender.send(StatusEvent::Cleared {
                provider_id: provider_id.to_string(),
            });
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StatusEvent> {
        self.inner.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(provider: &str, severity: StatusSeverity) -> StatusSnapshot {
        StatusSnapshot::now(provider.to_string(), severity, None, None, None)
    }

    #[test]
    fn apply_success_inserts_and_overwrites() {
        let store = StatusStore::new();
        assert!(store
            .apply_success(snap("claude", StatusSeverity::None))
            .is_none());
        let prev = store
            .apply_success(snap("claude", StatusSeverity::Major))
            .unwrap();
        assert_eq!(prev.severity, StatusSeverity::None);
        assert_eq!(store.get("claude").unwrap().severity, StatusSeverity::Major);
    }

    #[test]
    fn apply_failure_with_prior_snapshot_keeps_it() {
        let store = StatusStore::new();
        store.apply_success(snap("cursor", StatusSeverity::None));
        store.apply_failure("cursor", Some("offline".into()), None);
        // Still operational (sticky).
        assert_eq!(store.get("cursor").unwrap().severity, StatusSeverity::None);
    }

    #[test]
    fn apply_failure_without_prior_snapshot_installs_unknown() {
        let store = StatusStore::new();
        store.apply_failure("gemini", Some("offline".into()), None);
        let stored = store.get("gemini").unwrap();
        assert_eq!(stored.severity, StatusSeverity::Unknown);
        assert_eq!(stored.title.as_deref(), Some("offline"));
    }

    #[test]
    fn all_returns_snapshots_sorted_by_provider_id() {
        let store = StatusStore::new();
        store.apply_success(snap("claude", StatusSeverity::None));
        store.apply_success(snap("codex", StatusSeverity::Major));
        let all = store.all();
        let ids: Vec<&str> = all.iter().map(|s| s.provider_id.as_str()).collect();
        assert_eq!(ids, vec!["claude", "codex"]);
    }

    #[test]
    fn subscribe_emits_updated_event_on_success() {
        let store = StatusStore::new();
        let mut rx = store.subscribe();
        store.apply_success(snap("claude", StatusSeverity::None));
        // Drain at least one event.
        let event = rx.try_recv().expect("update event");
        match event {
            StatusEvent::Updated { provider_id } => assert_eq!(provider_id, "claude"),
            other => panic!("expected Updated, got {other:?}"),
        }
    }

    #[test]
    fn clear_drops_the_snapshot_and_emits_event() {
        let store = StatusStore::new();
        store.apply_success(snap("claude", StatusSeverity::None));
        let mut rx = store.subscribe();
        store.clear("claude");
        assert!(store.get("claude").is_none());
        let event = rx.try_recv().expect("event after clear");
        assert!(matches!(event, StatusEvent::Cleared { .. }));
    }
}
