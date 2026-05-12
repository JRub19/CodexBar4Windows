//! Canonical history key. Spec 41 §6.1 fixes the shape:
//! `codex::{account_id}::{plan_type}::{data_kind}`.
//!
//! The `account_id` is the same lowercased identifier the strategy
//! files snapshots under; `plan_type` reflects the JWT claim at the
//! time of the write (so a plan upgrade creates a fresh bucket). The
//! `data_kind` enumerates the named histories the popup chart cards
//! consume.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HistoryKind {
    /// Credit balance over the last 30 days.
    Credits,
    /// Cost in USD per day.
    Cost,
    /// Per-service breakdown for the current cycle.
    Breakdown,
    /// Plan utilization (used / allotted) per window.
    PlanUtilization,
}

impl HistoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            HistoryKind::Credits => "credits",
            HistoryKind::Cost => "cost",
            HistoryKind::Breakdown => "breakdown",
            HistoryKind::PlanUtilization => "plan_utilization",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct HistoryKey {
    pub account_id: String,
    pub plan_type: String,
    pub kind: HistoryKind,
}

impl HistoryKey {
    pub fn new(
        account_id: impl Into<String>,
        plan_type: impl Into<String>,
        kind: HistoryKind,
    ) -> Self {
        Self {
            account_id: normalize(account_id.into()),
            plan_type: normalize(plan_type.into()),
            kind,
        }
    }

    /// Compose a flat string representation suitable for use as a
    /// filesystem key, log field, or telemetry tag. The double-colon
    /// separator avoids collisions with any plausible email local part.
    pub fn as_path_key(&self) -> String {
        format!(
            "codex::{}::{}::{}",
            self.account_id,
            self.plan_type,
            self.kind.as_str()
        )
    }
}

fn normalize(raw: String) -> String {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        "anonymous".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_canonical_path_key() {
        let k = HistoryKey::new("Acct-1", "Plus", HistoryKind::Credits);
        assert_eq!(k.as_path_key(), "codex::acct-1::plus::credits");
    }

    #[test]
    fn empty_account_id_collapses_to_anonymous() {
        let k = HistoryKey::new("", "free", HistoryKind::Cost);
        assert_eq!(k.account_id, "anonymous");
    }

    #[test]
    fn plan_change_yields_a_different_key() {
        let plus = HistoryKey::new("a", "plus", HistoryKind::Breakdown);
        let pro = HistoryKey::new("a", "pro", HistoryKind::Breakdown);
        assert_ne!(plus.as_path_key(), pro.as_path_key());
    }

    #[test]
    fn data_kind_emits_stable_str() {
        assert_eq!(HistoryKind::Credits.as_str(), "credits");
        assert_eq!(HistoryKind::Cost.as_str(), "cost");
        assert_eq!(HistoryKind::Breakdown.as_str(), "breakdown");
        assert_eq!(HistoryKind::PlanUtilization.as_str(), "plan_utilization");
    }
}
