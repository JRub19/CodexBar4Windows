//! Decide whether a historic data point belongs to the user we are
//! currently viewing. Spec 41 §6.2 names the function
//! `belongs_to_target_continuity` because the rule is "this point came
//! from the same continuous account-and-plan stream as the target."

use super::key::HistoryKey;

/// Returns true when `point_key` belongs to the same continuous stream
/// as `target_key`. Two streams are continuous when both fields are
/// equal; we do not coalesce across plan changes because the underlying
/// quotas reset.
pub fn belongs_to_target_continuity(point_key: &HistoryKey, target_key: &HistoryKey) -> bool {
    point_key.account_id == target_key.account_id
        && point_key.plan_type == target_key.plan_type
        && point_key.kind == target_key.kind
}

#[cfg(test)]
mod tests {
    use super::super::key::HistoryKind;
    use super::*;

    #[test]
    fn same_account_same_plan_same_kind_belongs() {
        let a = HistoryKey::new("acct", "plus", HistoryKind::Credits);
        let b = HistoryKey::new("acct", "plus", HistoryKind::Credits);
        assert!(belongs_to_target_continuity(&a, &b));
    }

    #[test]
    fn different_account_does_not_belong() {
        let a = HistoryKey::new("acct-1", "plus", HistoryKind::Credits);
        let b = HistoryKey::new("acct-2", "plus", HistoryKind::Credits);
        assert!(!belongs_to_target_continuity(&a, &b));
    }

    #[test]
    fn plan_change_does_not_belong() {
        let a = HistoryKey::new("acct", "plus", HistoryKind::Credits);
        let b = HistoryKey::new("acct", "pro", HistoryKind::Credits);
        assert!(!belongs_to_target_continuity(&a, &b));
    }

    #[test]
    fn different_kind_does_not_belong() {
        let a = HistoryKey::new("acct", "plus", HistoryKind::Credits);
        let b = HistoryKey::new("acct", "plus", HistoryKind::Cost);
        assert!(!belongs_to_target_continuity(&a, &b));
    }
}
