//! Order-based tray-overlay aggregator. Walks user-configured provider
//! order; surfaces the first provider with `has_issue`. This matches
//! the macOS behavior (deliberately not severity-based) — see spec 55
//! §1 / §5.
//!
//! When no enabled provider is reporting an issue, returns `None` so
//! the tray icon paints with no overlay.

use super::feed::StatusSnapshot;
use super::store::StatusStore;

/// User-configurable provider order. Each entry is a provider id and
/// whether the user has the provider enabled in the popup. Disabled
/// providers are skipped during aggregation.
#[derive(Clone, Debug, Default)]
pub struct AggregationOrder {
    pub entries: Vec<(String, bool)>,
}

impl AggregationOrder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, provider_id: impl Into<String>, enabled: bool) {
        self.entries.push((provider_id.into(), enabled));
    }
}

/// Returns the first enabled provider's snapshot that `has_issue`,
/// walking `order` in declared order. None when no issue is active.
pub fn aggregate(store: &StatusStore, order: &AggregationOrder) -> Option<StatusSnapshot> {
    for (provider_id, enabled) in &order.entries {
        if !*enabled {
            continue;
        }
        if let Some(snap) = store.get(provider_id) {
            if snap.severity.has_issue() {
                return Some(snap);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::severity::StatusSeverity;

    fn snap(p: &str, sev: StatusSeverity) -> StatusSnapshot {
        StatusSnapshot::now(p, sev, None, None, None)
    }

    #[test]
    fn returns_none_when_all_operational() {
        let store = StatusStore::new();
        store.apply_success(snap("claude", StatusSeverity::None));
        store.apply_success(snap("codex", StatusSeverity::None));
        let mut order = AggregationOrder::new();
        order.push("claude", true);
        order.push("codex", true);
        assert!(aggregate(&store, &order).is_none());
    }

    #[test]
    fn picks_first_provider_with_issue_in_declared_order() {
        let store = StatusStore::new();
        store.apply_success(snap("claude", StatusSeverity::None));
        // Higher severity but later in order.
        store.apply_success(snap("codex", StatusSeverity::Critical));
        store.apply_success(snap("cursor", StatusSeverity::Minor));
        let mut order = AggregationOrder::new();
        order.push("claude", true);
        order.push("cursor", true);
        order.push("codex", true);
        let agg = aggregate(&store, &order).unwrap();
        // Cursor's Minor wins over Codex's Critical because Cursor is
        // earlier in declared order.
        assert_eq!(agg.provider_id, "cursor");
        assert_eq!(agg.severity, StatusSeverity::Minor);
    }

    #[test]
    fn skips_disabled_providers() {
        let store = StatusStore::new();
        store.apply_success(snap("cursor", StatusSeverity::Critical));
        store.apply_success(snap("codex", StatusSeverity::Minor));
        let mut order = AggregationOrder::new();
        order.push("cursor", false); // disabled
        order.push("codex", true);
        let agg = aggregate(&store, &order).unwrap();
        assert_eq!(agg.provider_id, "codex");
    }

    #[test]
    fn missing_snapshot_is_treated_as_operational() {
        let store = StatusStore::new();
        store.apply_success(snap("codex", StatusSeverity::Critical));
        let mut order = AggregationOrder::new();
        order.push("never-fetched", true);
        order.push("codex", true);
        let agg = aggregate(&store, &order).unwrap();
        assert_eq!(agg.provider_id, "codex");
    }

    #[test]
    fn unknown_severity_counts_as_an_issue() {
        let store = StatusStore::new();
        store.apply_failure("claude", Some("offline".into()), None);
        let mut order = AggregationOrder::new();
        order.push("claude", true);
        let agg = aggregate(&store, &order).unwrap();
        assert_eq!(agg.severity, StatusSeverity::Unknown);
    }
}
