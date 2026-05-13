//! Cursor usage-summary fold. Translates the wire shape into the
//! framework's `RateWindow`/`ProviderCostSnapshot` types using the same
//! six-rule precedence ladder the macOS source enforces. The Swift
//! reference is `CursorStatusProbe.parseUsageSummary(_:)`.

use super::response::{LegacyUsage, UsageSummary};

#[derive(Debug, Clone, PartialEq)]
pub struct CursorHeadline {
    /// Headline "Total" percent (0..=100) per spec 42 §3.
    pub plan_percent: f64,
    /// Auto + Composer lane percent (0..=100) when reported. The popup
    /// renders this as a sub-bar below the primary.
    pub auto_percent: Option<f64>,
    /// API (named-model) lane percent (0..=100) when reported.
    pub api_percent: Option<f64>,
    /// Plan headline USD (used / limit). Cents are converted client side.
    pub plan_used_usd: f64,
    pub plan_limit_usd: f64,
    /// On-demand spend (used USD, optional limit USD).
    pub on_demand_used_usd: f64,
    pub on_demand_limit_usd: Option<f64>,
    /// Team on-demand: reported when an account is part of a team plan.
    pub team_on_demand_used_usd: Option<f64>,
    pub team_on_demand_limit_usd: Option<f64>,
    /// Legacy request-based plan: `Some(used, limit)` when `gpt-4`
    /// reports `maxRequestUsage`, `None` otherwise.
    pub legacy_requests: Option<(i64, i64)>,
    /// `billingCycleEnd` parsed to unix epoch seconds, when present.
    pub billing_cycle_end_unix_secs: Option<i64>,
    /// `membershipType`, unchanged from the wire.
    pub membership_type: Option<String>,
}

impl CursorHeadline {
    /// Final headline percent the icon should render. Legacy plans use
    /// the request ratio (used / limit) when present; otherwise the
    /// percent precedence ladder result.
    pub fn primary_used_percent(&self) -> f64 {
        if let Some((used, limit)) = self.legacy_requests {
            if limit > 0 {
                return (used as f64 / limit as f64) * 100.0;
            }
        }
        self.plan_percent
    }

    /// Whether the account is on a legacy request-based plan.
    pub fn is_legacy_request_plan(&self) -> bool {
        self.legacy_requests.is_some()
    }
}

pub fn fold_summary(summary: &UsageSummary, legacy: Option<&LegacyUsage>) -> CursorHeadline {
    let plan = summary
        .individual_usage
        .as_ref()
        .and_then(|i| i.plan.as_ref());
    let overall = summary
        .individual_usage
        .as_ref()
        .and_then(|i| i.overall.as_ref());
    let pooled = summary.team_usage.as_ref().and_then(|t| t.pooled.as_ref());

    let plan_used_cents = plan.and_then(|p| p.used).unwrap_or(0);
    let plan_limit_cents = plan.and_then(|p| p.limit).unwrap_or(0);
    let auto_percent = plan.and_then(|p| p.auto_percent_used).map(clamp_percent);
    let api_percent = plan.and_then(|p| p.api_percent_used).map(clamp_percent);

    let overall_used = overall.and_then(|o| o.used);
    let overall_limit = overall.and_then(|o| o.limit);
    let pooled_used = pooled.and_then(|p| p.used);
    let pooled_limit = pooled.and_then(|p| p.limit);

    // Plan-percent precedence ladder per `parseUsageSummary`:
    //   1. plan.totalPercentUsed
    //   2. (auto + api) / 2 when both present
    //   3. either lane alone
    //   4. plan ratio (used / limit) — when limit > 0
    //   5. overall ratio — enterprise/team personal cap
    //   6. pooled ratio — shared team pool last resort
    let plan_percent = if let Some(total) = plan.and_then(|p| p.total_percent_used) {
        clamp_percent(total)
    } else if let (Some(auto), Some(api)) = (auto_percent, api_percent) {
        clamp_percent((auto + api) / 2.0)
    } else if let Some(api) = api_percent {
        clamp_percent(api)
    } else if let Some(auto) = auto_percent {
        clamp_percent(auto)
    } else if plan_limit_cents > 0 {
        clamp_percent((plan_used_cents as f64 / plan_limit_cents as f64) * 100.0)
    } else if let (Some(u), Some(l)) = (overall_used, overall_limit) {
        if l > 0 {
            clamp_percent((u as f64 / l as f64) * 100.0)
        } else {
            0.0
        }
    } else if let (Some(u), Some(l)) = (pooled_used, pooled_limit) {
        if l > 0 {
            clamp_percent((u as f64 / l as f64) * 100.0)
        } else {
            0.0
        }
    } else {
        0.0
    };

    // USD figures track the same fallback order so the popup never shows
    // a zeroed dollar amount when overall/pooled actually carry the cents.
    let (plan_used_usd, plan_limit_usd) = if plan_limit_cents > 0 || plan_used_cents > 0 {
        (
            cents_to_usd(plan_used_cents),
            cents_to_usd(plan_limit_cents),
        )
    } else if let (Some(u), Some(l)) = (overall_used, overall_limit) {
        (cents_to_usd(u), cents_to_usd(l))
    } else if let (Some(u), Some(l)) = (pooled_used, pooled_limit) {
        (cents_to_usd(u), cents_to_usd(l))
    } else {
        (0.0, 0.0)
    };

    let on_demand = summary
        .individual_usage
        .as_ref()
        .and_then(|i| i.on_demand.as_ref());
    let on_demand_used_usd = cents_to_usd(on_demand.and_then(|o| o.used).unwrap_or(0));
    let on_demand_limit_usd = on_demand.and_then(|o| o.limit).map(cents_to_usd);

    let team_on_demand = summary
        .team_usage
        .as_ref()
        .and_then(|t| t.on_demand.as_ref());
    let team_on_demand_used_usd = team_on_demand.and_then(|o| o.used).map(cents_to_usd);
    let team_on_demand_limit_usd = team_on_demand.and_then(|o| o.limit).map(cents_to_usd);

    let legacy_requests = legacy.and_then(|wire| {
        let gpt4 = wire.gpt4.as_ref()?;
        let limit = gpt4.max_request_usage?;
        let used = gpt4.num_requests_total.or(gpt4.num_requests).unwrap_or(0);
        Some((used, limit))
    });

    CursorHeadline {
        plan_percent,
        auto_percent,
        api_percent,
        plan_used_usd,
        plan_limit_usd,
        on_demand_used_usd,
        on_demand_limit_usd,
        team_on_demand_used_usd,
        team_on_demand_limit_usd,
        legacy_requests,
        billing_cycle_end_unix_secs: summary.billing_cycle_end_unix_secs(),
        membership_type: summary.membership_type.clone(),
    }
}

fn clamp_percent(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}

fn cents_to_usd(value: i64) -> f64 {
    value as f64 / 100.0
}

/// Membership type → human label. Mirrors `formatMembershipType` so the
/// popup header matches the macOS app exactly.
pub fn pretty_membership(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "enterprise" => "Cursor Enterprise".into(),
        "pro" => "Cursor Pro".into(),
        "hobby" => "Cursor Hobby".into(),
        "team" => "Cursor Team".into(),
        other => format!("Cursor {}", capitalize_first(other)),
    }
}

fn capitalize_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::cursor::web::response::{
        CentUsage, IndividualUsage, LegacyModelUsage, PlanUsage, TeamUsage,
    };

    fn plan_only(plan: PlanUsage, on_demand: Option<CentUsage>) -> UsageSummary {
        UsageSummary {
            individual_usage: Some(IndividualUsage {
                plan: Some(plan),
                on_demand,
                overall: None,
            }),
            ..UsageSummary::default()
        }
    }

    #[test]
    fn rule_1_uses_total_percent_used_when_present() {
        let summary = plan_only(
            PlanUsage {
                used: Some(1000),
                limit: Some(2000),
                total_percent_used: Some(73.4),
                auto_percent_used: Some(10.0),
                api_percent_used: Some(40.0),
            },
            None,
        );
        let h = fold_summary(&summary, None);
        assert_eq!(h.plan_percent, 73.4);
    }

    #[test]
    fn rule_2_averages_auto_and_api_when_total_missing() {
        let summary = plan_only(
            PlanUsage {
                used: Some(500),
                limit: Some(1000),
                total_percent_used: None,
                auto_percent_used: Some(40.0),
                api_percent_used: Some(60.0),
            },
            None,
        );
        let h = fold_summary(&summary, None);
        assert_eq!(h.plan_percent, 50.0);
        assert_eq!(h.auto_percent, Some(40.0));
        assert_eq!(h.api_percent, Some(60.0));
    }

    #[test]
    fn rule_3a_uses_api_lane_alone_when_only_api_reported() {
        let summary = plan_only(
            PlanUsage {
                used: Some(0),
                limit: Some(0),
                total_percent_used: None,
                auto_percent_used: None,
                api_percent_used: Some(33.3),
            },
            None,
        );
        let h = fold_summary(&summary, None);
        assert_eq!(h.plan_percent, 33.3);
    }

    #[test]
    fn rule_3b_uses_auto_lane_alone_when_only_auto_reported() {
        let summary = plan_only(
            PlanUsage {
                used: Some(0),
                limit: Some(0),
                total_percent_used: None,
                auto_percent_used: Some(77.0),
                api_percent_used: None,
            },
            None,
        );
        let h = fold_summary(&summary, None);
        assert_eq!(h.plan_percent, 77.0);
    }

    #[test]
    fn rule_4_uses_plan_ratio_when_no_percent_fields_present() {
        let summary = plan_only(
            PlanUsage {
                used: Some(1500),
                limit: Some(2000),
                total_percent_used: None,
                auto_percent_used: None,
                api_percent_used: None,
            },
            None,
        );
        let h = fold_summary(&summary, None);
        assert_eq!(h.plan_percent, 75.0);
        assert_eq!(h.plan_used_usd, 15.0);
        assert_eq!(h.plan_limit_usd, 20.0);
    }

    #[test]
    fn rule_5_uses_overall_ratio_for_enterprise_personal_cap() {
        let summary = UsageSummary {
            individual_usage: Some(IndividualUsage {
                plan: None,
                on_demand: None,
                overall: Some(CentUsage {
                    used: Some(7384),
                    limit: Some(10000),
                }),
            }),
            ..UsageSummary::default()
        };
        let h = fold_summary(&summary, None);
        assert!((h.plan_percent - 73.84).abs() < 1e-9);
        // USD figures fall back to overall when plan absent.
        assert_eq!(h.plan_used_usd, 73.84);
        assert_eq!(h.plan_limit_usd, 100.0);
    }

    #[test]
    fn rule_6_uses_pooled_ratio_as_last_resort() {
        let summary = UsageSummary {
            team_usage: Some(TeamUsage {
                on_demand: None,
                pooled: Some(CentUsage {
                    used: Some(12345),
                    limit: Some(50000),
                }),
            }),
            ..UsageSummary::default()
        };
        let h = fold_summary(&summary, None);
        assert!((h.plan_percent - 24.69).abs() < 1e-9);
        assert_eq!(h.plan_used_usd, 123.45);
    }

    #[test]
    fn legacy_plan_request_ratio_takes_priority_for_primary() {
        let summary = plan_only(
            PlanUsage {
                used: Some(0),
                limit: Some(0),
                total_percent_used: None,
                auto_percent_used: None,
                api_percent_used: None,
            },
            None,
        );
        let legacy = LegacyUsage {
            gpt4: Some(LegacyModelUsage {
                num_requests: Some(80),
                num_requests_total: Some(100),
                max_request_usage: Some(500),
            }),
        };
        let h = fold_summary(&summary, Some(&legacy));
        assert!(h.is_legacy_request_plan());
        // Legacy ratio = 100 / 500 = 20.0
        assert_eq!(h.primary_used_percent(), 20.0);
    }

    #[test]
    fn percent_above_100_clamps_to_100() {
        let summary = plan_only(
            PlanUsage {
                used: Some(0),
                limit: Some(0),
                total_percent_used: Some(150.0),
                auto_percent_used: None,
                api_percent_used: None,
            },
            None,
        );
        let h = fold_summary(&summary, None);
        assert_eq!(h.plan_percent, 100.0);
    }

    #[test]
    fn pretty_membership_handles_known_and_unknown() {
        assert_eq!(pretty_membership("pro"), "Cursor Pro");
        assert_eq!(pretty_membership("enterprise"), "Cursor Enterprise");
        assert_eq!(pretty_membership("Hobby"), "Cursor Hobby");
        assert_eq!(pretty_membership("ultra"), "Cursor Ultra");
    }
}
