//! 6-state status severity enum, plus the order-based "has issue"
//! predicate and label keys per spec 55 §3.1.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusSeverity {
    /// Operational; no issue.
    None,
    /// Yellow; partial outage.
    Minor,
    /// Orange; major outage.
    Major,
    /// Red; critical issue.
    Critical,
    /// Gray; scheduled maintenance.
    Maintenance,
    /// Gray; we could not fetch a feed but we have no prior snapshot.
    Unknown,
}

impl StatusSeverity {
    pub fn has_issue(self) -> bool {
        !matches!(self, StatusSeverity::None)
    }

    /// Stable localisation key, matching `docs/windows/spec/55` §3.1.
    pub fn label_key(self) -> &'static str {
        match self {
            StatusSeverity::None => "status_operational",
            StatusSeverity::Minor => "status_partial_outage",
            StatusSeverity::Major => "status_major_outage",
            StatusSeverity::Critical => "status_critical_issue",
            StatusSeverity::Maintenance => "status_maintenance",
            StatusSeverity::Unknown => "status_unknown",
        }
    }

    /// English fallback label used when localization tables are not
    /// yet wired. The popup falls back to this when i18n misses.
    pub fn english_label(self) -> &'static str {
        match self {
            StatusSeverity::None => "Operational",
            StatusSeverity::Minor => "Partial outage",
            StatusSeverity::Major => "Major outage",
            StatusSeverity::Critical => "Critical issue",
            StatusSeverity::Maintenance => "Maintenance",
            StatusSeverity::Unknown => "Status unknown",
        }
    }

    /// Severity rank, used when picking the worst of multiple active
    /// incidents on the same Google Workspace feed.
    pub fn rank(self) -> u8 {
        match self {
            StatusSeverity::None => 0,
            // Maintenance and Unknown are tied at the bottom of the
            // "has issue" group; the spec calls this out explicitly.
            StatusSeverity::Maintenance => 1,
            StatusSeverity::Unknown => 1,
            StatusSeverity::Minor => 2,
            StatusSeverity::Major => 3,
            StatusSeverity::Critical => 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_issue_excludes_only_none() {
        assert!(!StatusSeverity::None.has_issue());
        for s in [
            StatusSeverity::Minor,
            StatusSeverity::Major,
            StatusSeverity::Critical,
            StatusSeverity::Maintenance,
            StatusSeverity::Unknown,
        ] {
            assert!(s.has_issue(), "{s:?} should have_issue");
        }
    }

    #[test]
    fn rank_orders_critical_above_major_above_minor() {
        assert!(StatusSeverity::Critical.rank() > StatusSeverity::Major.rank());
        assert!(StatusSeverity::Major.rank() > StatusSeverity::Minor.rank());
        assert!(StatusSeverity::Minor.rank() > StatusSeverity::Maintenance.rank());
    }

    #[test]
    fn serializes_as_lowercase_kebab() {
        let s = serde_json::to_string(&StatusSeverity::Critical).unwrap();
        assert_eq!(s, "\"critical\"");
        let parsed: StatusSeverity = serde_json::from_str("\"maintenance\"").unwrap();
        assert_eq!(parsed, StatusSeverity::Maintenance);
    }

    #[test]
    fn label_keys_are_spec_55_strings() {
        assert_eq!(StatusSeverity::Major.label_key(), "status_major_outage");
        assert_eq!(StatusSeverity::None.label_key(), "status_operational");
    }
}
