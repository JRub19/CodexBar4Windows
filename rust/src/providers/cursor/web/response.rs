//! Wire types for the Cursor web responses. Field shapes are taken
//! verbatim from `CursorStatusProbe.swift` so the same JSON the macOS
//! app handles round-trips through the Windows fold.

use chrono::DateTime;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct UsageSummary {
    #[serde(default, rename = "billingCycleEnd")]
    pub billing_cycle_end: Option<String>,
    #[serde(default, rename = "membershipType")]
    pub membership_type: Option<String>,
    #[serde(default, rename = "individualUsage")]
    pub individual_usage: Option<IndividualUsage>,
    #[serde(default, rename = "teamUsage")]
    pub team_usage: Option<TeamUsage>,
}

#[derive(Debug, Default, Deserialize)]
pub struct IndividualUsage {
    #[serde(default)]
    pub plan: Option<PlanUsage>,
    #[serde(default, rename = "onDemand")]
    pub on_demand: Option<CentUsage>,
    /// Enterprise / team-member personal cap; cents.
    #[serde(default)]
    pub overall: Option<CentUsage>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PlanUsage {
    #[serde(default)]
    pub used: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default, rename = "autoPercentUsed")]
    pub auto_percent_used: Option<f64>,
    #[serde(default, rename = "apiPercentUsed")]
    pub api_percent_used: Option<f64>,
    #[serde(default, rename = "totalPercentUsed")]
    pub total_percent_used: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CentUsage {
    #[serde(default)]
    pub used: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TeamUsage {
    #[serde(default, rename = "onDemand")]
    pub on_demand: Option<CentUsage>,
    /// Shared team/enterprise pool counted across all members.
    #[serde(default)]
    pub pooled: Option<CentUsage>,
}

#[derive(Debug, Default, Deserialize)]
pub struct AuthMe {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub sub: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LegacyUsage {
    #[serde(default, rename = "gpt-4")]
    pub gpt4: Option<LegacyModelUsage>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LegacyModelUsage {
    #[serde(default, rename = "numRequests")]
    pub num_requests: Option<i64>,
    #[serde(default, rename = "numRequestsTotal")]
    pub num_requests_total: Option<i64>,
    #[serde(default, rename = "maxRequestUsage")]
    pub max_request_usage: Option<i64>,
}

impl UsageSummary {
    pub fn billing_cycle_end_unix_secs(&self) -> Option<i64> {
        self.billing_cycle_end
            .as_deref()
            .and_then(parse_iso8601_unix_secs)
    }
}

fn parse_iso8601_unix_secs(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value).ok().map(|d| d.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plan_total_percent() {
        let body = br#"{
            "billingCycleEnd": "2026-06-01T00:00:00Z",
            "membershipType": "pro",
            "individualUsage": {
                "plan": {
                    "used": 1500, "limit": 2000,
                    "autoPercentUsed": 12.5, "apiPercentUsed": 4.0,
                    "totalPercentUsed": 8.25
                },
                "onDemand": {"used": 384, "limit": 10000}
            }
        }"#;
        let summary: UsageSummary = serde_json::from_slice(body).unwrap();
        let plan = summary.individual_usage.unwrap().plan.unwrap();
        assert_eq!(plan.total_percent_used, Some(8.25));
        assert_eq!(plan.used, Some(1500));
    }

    #[test]
    fn parses_enterprise_overall_cap() {
        let body = br#"{
            "billingCycleEnd": "2026-06-01T00:00:00Z",
            "membershipType": "enterprise",
            "individualUsage": {
                "overall": {"used": 7384, "limit": 10000}
            }
        }"#;
        let summary: UsageSummary = serde_json::from_slice(body).unwrap();
        let overall = summary.individual_usage.unwrap().overall.unwrap();
        assert_eq!(overall.used, Some(7384));
        assert_eq!(overall.limit, Some(10000));
    }

    #[test]
    fn parses_team_pooled_pool() {
        let body = br#"{
            "teamUsage": {
                "pooled": {"used": 12345, "limit": 50000},
                "onDemand": {"used": 555, "limit": null}
            }
        }"#;
        let summary: UsageSummary = serde_json::from_slice(body).unwrap();
        let team = summary.team_usage.unwrap();
        assert_eq!(team.pooled.unwrap().used, Some(12345));
        assert!(team.on_demand.unwrap().limit.is_none());
    }

    #[test]
    fn parses_legacy_gpt4_usage() {
        let body = br#"{"gpt-4": {"numRequests": 12, "numRequestsTotal": 15, "maxRequestUsage": 500}}"#;
        let usage: LegacyUsage = serde_json::from_slice(body).unwrap();
        let gpt4 = usage.gpt4.unwrap();
        assert_eq!(gpt4.num_requests_total, Some(15));
        assert_eq!(gpt4.max_request_usage, Some(500));
    }

    #[test]
    fn billing_cycle_end_decodes_to_epoch() {
        let summary = UsageSummary {
            billing_cycle_end: Some("2026-06-01T00:00:00Z".into()),
            ..UsageSummary::default()
        };
        assert_eq!(summary.billing_cycle_end_unix_secs(), Some(1_780_272_000));
    }

    #[test]
    fn billing_cycle_end_returns_none_for_garbage() {
        let summary = UsageSummary {
            billing_cycle_end: Some("not-a-date".into()),
            ..UsageSummary::default()
        };
        assert!(summary.billing_cycle_end_unix_secs().is_none());
    }
}
