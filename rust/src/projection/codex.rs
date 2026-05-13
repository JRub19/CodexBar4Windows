//! Codex consumer-plan projection. See module-level doc on
//! `crate::projection` for the spec references.
//!
//! Three concerns live here:
//!
//! 1. `project_codex_consumer` — turn a live `UsageSnapshot` +
//!    optional credits + optional rolling cost into the lanes the
//!    UI surfaces.
//! 2. `monthly_projection` — extrapolate the next 30-day cost from a
//!    rolling daily-cost series, with the Phase 7 A12 smoothing
//!    clamp.
//! 3. `map_user_facing_error` — rewrite known raw error strings
//!    into messages the popup can show. Mirrors `CodexUIErrorMapper`
//!    from the macOS source.

use serde::{Deserialize, Serialize};

use crate::providers::models::credits::CreditsSnapshot;
use crate::providers::models::rate_window::NamedRateWindow;
use crate::providers::models::usage_snapshot::UsageSnapshot;

/// Semantic priority of a Codex consumer rate lane.
///
/// `Session` (5h) always ranks before `Weekly` (7d) — when both
/// are present the tray icon paints session as the primary bar and
/// weekly as the secondary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RateLane {
    Session,
    Weekly,
}

impl RateLane {
    pub fn key(self) -> &'static str {
        match self {
            RateLane::Session => "session",
            RateLane::Weekly => "weekly",
        }
    }
}

/// Credits-remaining fallback. Only surfaced when every rate lane is
/// fully exhausted (`remaining_percent <= 0`) or no rate lanes exist.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreditsProjection {
    pub balance: f64,
    pub unit: String,
    /// True when credits were elevated to the primary menu-bar lane
    /// because all rate lanes drained. Drives the tray icon's
    /// fallback paint.
    pub is_primary_fallback: bool,
}

/// 30-day extrapolated cost from a rolling daily-cost series.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MonthlyProjection {
    /// Sum of the rolling daily-cost samples (USD).
    pub rolling_sum_usd: f64,
    /// Number of samples that contributed.
    pub samples: u32,
    /// Smoothing constant applied — `1/n` clamped at `1/7`.
    pub smoothing: f64,
    /// Projected next-30d cost (USD). Lower-bounded at 0.
    pub projected_30d_usd: f64,
}

/// Output of the projection pass.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CodexConsumerProjection {
    /// Lanes to surface anywhere (tray icon, popup card). Order is
    /// semantic, session first.
    pub visible_rate_lanes: Vec<RateLane>,
    /// Lanes accepted by the plan-utilization sampler. Subset of
    /// `visible_rate_lanes` — empty when both lanes are unknown.
    pub plan_utilization_lanes: Vec<RateLane>,
    pub credits_projection: Option<CreditsProjection>,
    /// User-facing error messages already mapped through
    /// `map_user_facing_error`. Empty when none.
    pub user_facing_errors: Vec<String>,
    /// 30-day extrapolated cost — present only when a non-empty
    /// rolling series was supplied.
    pub monthly_projection: Option<MonthlyProjection>,
}

/// Build the projection from live inputs.
///
/// - `snapshot.windows` is searched for keys `session` and `weekly`
///   (case-insensitive). A lane is visible when its window has an
///   `allotted` cap; otherwise it's hidden. Both lanes hidden →
///   `visible_rate_lanes` is empty.
/// - `credits` becomes a fallback when every visible lane has
///   `remaining_percent <= 0`. When no lanes exist at all, credits
///   takes the primary slot if present.
/// - `raw_errors` is mapped through `map_user_facing_error`.
/// - `rolling_daily_cost_usd` populates `monthly_projection` via
///   `monthly_projection()`.
pub fn project_codex_consumer(
    snapshot: &UsageSnapshot,
    credits: Option<&CreditsSnapshot>,
    raw_errors: &[String],
    rolling_daily_cost_usd: &[f64],
) -> CodexConsumerProjection {
    let session = find_lane(&snapshot.windows, "session");
    let weekly = find_lane(&snapshot.windows, "weekly");

    let mut visible: Vec<RateLane> = Vec::with_capacity(2);
    if let Some(s) = session {
        if s.window.allotted.is_some() {
            visible.push(RateLane::Session);
        }
    }
    if let Some(w) = weekly {
        if w.window.allotted.is_some() {
            visible.push(RateLane::Weekly);
        }
    }

    let mut plan_utilization: Vec<RateLane> = Vec::with_capacity(visible.len());
    if visible.contains(&RateLane::Session) {
        plan_utilization.push(RateLane::Session);
    }
    if visible.contains(&RateLane::Weekly) {
        plan_utilization.push(RateLane::Weekly);
    }

    // Credits fallback: only when *every* visible lane is fully
    // exhausted, or no lanes exist at all. Per spec §10 last bullet.
    let all_exhausted = !visible.is_empty()
        && visible.iter().all(|lane| match lane {
            RateLane::Session => session
                .map(|s| s.window.remaining_percent() <= 0.0)
                .unwrap_or(false),
            RateLane::Weekly => weekly
                .map(|w| w.window.remaining_percent() <= 0.0)
                .unwrap_or(false),
        });
    let no_lanes = visible.is_empty();
    let credits_projection = credits.map(|c| CreditsProjection {
        balance: c.balance,
        unit: match c.unit {
            crate::providers::models::credits::CreditUnit::Credits => "credits".into(),
            crate::providers::models::credits::CreditUnit::Tokens => "tokens".into(),
            crate::providers::models::credits::CreditUnit::UsdCents => "usd_cents".into(),
        },
        is_primary_fallback: all_exhausted || no_lanes,
    });

    let user_facing_errors: Vec<String> = raw_errors
        .iter()
        .map(|raw| map_user_facing_error(raw))
        .collect();

    let monthly = if rolling_daily_cost_usd.is_empty() {
        None
    } else {
        Some(monthly_projection(rolling_daily_cost_usd))
    };

    CodexConsumerProjection {
        visible_rate_lanes: visible,
        plan_utilization_lanes: plan_utilization,
        credits_projection,
        user_facing_errors,
        monthly_projection: monthly,
    }
}

fn find_lane<'a>(windows: &'a [NamedRateWindow], key: &str) -> Option<&'a NamedRateWindow> {
    windows.iter().find(|w| w.key.eq_ignore_ascii_case(key))
}

/// Extrapolate the next 30-day cost from a rolling daily-cost series.
///
/// Smoothing constant = `max(1/n, 1/7)`. Without the clamp a fresh
/// install with `n=0` would divide by zero; with `n<7` the projection
/// would overshoot because a single noisy day would scale 30×. The
/// clamp pins the per-day rate at the 7-day average until enough
/// history accumulates.
///
/// Output is lower-bounded at 0 so a future negative refund event
/// can't drag the projected monthly cost negative.
pub fn monthly_projection(rolling_daily_cost_usd: &[f64]) -> MonthlyProjection {
    let n = rolling_daily_cost_usd.len() as u32;
    let sum: f64 = rolling_daily_cost_usd.iter().copied().sum();
    let smoothing = (1.0_f64 / n.max(1) as f64).max(1.0 / 7.0);
    let projected = (sum * smoothing * 30.0).max(0.0);
    MonthlyProjection {
        rolling_sum_usd: sum,
        samples: n,
        smoothing,
        projected_30d_usd: projected,
    }
}

/// `CodexUIErrorMapper` port. Rewrites a known set of raw error
/// codes into user-facing copy. Unknown errors pass through
/// unchanged so we don't accidentally swallow novel failures.
pub fn map_user_facing_error(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    // Order matters: refresh_token_expired is more specific than
    // token_expired so it must be checked first.
    if lower.contains("refresh_token_expired") {
        return "Codex session expired. Sign in again.".to_string();
    }
    if lower.contains("token_expired") {
        return "Codex session expired. Sign in again.".to_string();
    }
    if lower.contains("rate_limit") {
        return "Codex rate-limited. Try again in a few minutes.".to_string();
    }
    if lower.contains("network")
        || lower.contains("dns")
        || lower.contains("timeout")
        || lower.contains("timed_out")
        || lower.contains("etimedout")
    {
        return "Codex unreachable. Check your network connection.".to_string();
    }
    if lower.contains("forbidden") || lower.contains("403") {
        return "Codex denied the request. Sign in again.".to_string();
    }
    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ProviderId;
    use crate::providers::identity::ProviderIdentitySnapshot;
    use crate::providers::models::credits::{CreditEvent, CreditUnit};
    use crate::providers::models::rate_window::RateWindow;

    fn lane(key: &str, used: f64, allotted: Option<f64>) -> NamedRateWindow {
        NamedRateWindow {
            key: key.to_string(),
            window: RateWindow {
                label: key.to_string(),
                used,
                allotted,
                reset_at_unix_secs: None,
                pace_delta_percent: None,
            },
        }
    }

    fn snapshot(windows: Vec<NamedRateWindow>) -> UsageSnapshot {
        UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(ProviderId("codex"), "acct"),
            windows,
            credits: None,
            cost: None,
            account_display_name: None,
            account_email: None,
            plan_name: Some("Plus".into()),
            captured_at_unix_secs: 0,
        }
    }

    #[test]
    fn both_lanes_visible_when_session_and_weekly_have_allotted_caps() {
        let snap = snapshot(vec![
            lane("session", 30.0, Some(100.0)),
            lane("weekly", 200.0, Some(1000.0)),
        ]);
        let proj = project_codex_consumer(&snap, None, &[], &[]);
        assert_eq!(
            proj.visible_rate_lanes,
            vec![RateLane::Session, RateLane::Weekly]
        );
        assert_eq!(proj.plan_utilization_lanes, proj.visible_rate_lanes);
        assert!(proj.credits_projection.is_none());
        assert!(proj.user_facing_errors.is_empty());
        assert!(proj.monthly_projection.is_none());
    }

    #[test]
    fn lane_hidden_when_allotted_is_none() {
        let snap = snapshot(vec![
            lane("session", 30.0, None),
            lane("weekly", 200.0, Some(1000.0)),
        ]);
        let proj = project_codex_consumer(&snap, None, &[], &[]);
        assert_eq!(proj.visible_rate_lanes, vec![RateLane::Weekly]);
        assert_eq!(proj.plan_utilization_lanes, vec![RateLane::Weekly]);
    }

    #[test]
    fn credits_fallback_kicks_in_when_all_visible_lanes_exhausted() {
        let snap = snapshot(vec![
            lane("session", 100.0, Some(100.0)),
            lane("weekly", 1000.0, Some(1000.0)),
        ]);
        let credits = CreditsSnapshot {
            balance: 12.34,
            unit: CreditUnit::UsdCents,
            recent_events: vec![CreditEvent {
                timestamp_unix_secs: 0,
                delta: -1.0,
                note: None,
            }],
        };
        let proj = project_codex_consumer(&snap, Some(&credits), &[], &[]);
        let cp = proj.credits_projection.expect("credits projection");
        assert!(cp.is_primary_fallback);
        assert_eq!(cp.balance, 12.34);
        assert_eq!(cp.unit, "usd_cents");
    }

    #[test]
    fn credits_not_primary_when_any_lane_still_has_quota() {
        let snap = snapshot(vec![
            lane("session", 100.0, Some(100.0)),
            lane("weekly", 100.0, Some(1000.0)), // 90% remaining
        ]);
        let credits = CreditsSnapshot {
            balance: 50.0,
            unit: CreditUnit::Credits,
            recent_events: vec![],
        };
        let proj = project_codex_consumer(&snap, Some(&credits), &[], &[]);
        let cp = proj.credits_projection.unwrap();
        assert!(!cp.is_primary_fallback);
    }

    #[test]
    fn credits_take_primary_when_no_rate_lanes_are_visible() {
        let snap = snapshot(vec![]);
        let credits = CreditsSnapshot {
            balance: 1.0,
            unit: CreditUnit::Tokens,
            recent_events: vec![],
        };
        let proj = project_codex_consumer(&snap, Some(&credits), &[], &[]);
        assert!(proj.visible_rate_lanes.is_empty());
        assert!(proj.plan_utilization_lanes.is_empty());
        assert!(proj.credits_projection.unwrap().is_primary_fallback);
    }

    #[test]
    fn errors_pass_through_the_mapper() {
        let snap = snapshot(vec![]);
        let raw = vec![
            "openai.token_expired: please re-login".into(),
            "Some novel weirdness".into(),
        ];
        let proj = project_codex_consumer(&snap, None, &raw, &[]);
        assert_eq!(proj.user_facing_errors.len(), 2);
        assert_eq!(
            proj.user_facing_errors[0],
            "Codex session expired. Sign in again."
        );
        // Unknown errors pass through verbatim.
        assert_eq!(proj.user_facing_errors[1], "Some novel weirdness");
    }

    #[test]
    fn monthly_projection_does_not_panic_on_fresh_install() {
        // n = 0 → smoothing pinned at max(1/1, 1/7) = 1.0, never
        // divides by zero. Sum is also 0, so projected is 0.
        let m = monthly_projection(&[]);
        assert_eq!(m.samples, 0);
        assert!((m.smoothing - 1.0).abs() < 1e-9);
        assert_eq!(m.projected_30d_usd, 0.0);
    }

    #[test]
    fn monthly_projection_uses_one_over_n_when_above_one_seventh() {
        // n = 3 → smoothing = max(1/3, 1/7) = 1/3 (n-based wins).
        let m = monthly_projection(&[10.0, 5.0, 2.0]);
        assert!((m.smoothing - (1.0 / 3.0)).abs() < 1e-9);
        // sum = 17, projected = 17 * 1/3 * 30 = 170
        assert!((m.projected_30d_usd - (17.0 / 3.0 * 30.0)).abs() < 1e-6);
    }

    #[test]
    fn monthly_projection_floors_smoothing_at_one_seventh_for_long_windows() {
        // n = 14 → max(1/14, 1/7) = 1/7. The clamp is a FLOOR, not a
        // ceiling — long windows use 1/7 so the 7-day average drives
        // the projection regardless of how much history accumulated.
        let series: Vec<f64> = (0..14).map(|_| 1.0).collect();
        let m = monthly_projection(&series);
        assert!((m.smoothing - (1.0 / 7.0)).abs() < 1e-9);
        // sum = 14, projected = 14 * 1/7 * 30 = 60
        assert!((m.projected_30d_usd - 60.0).abs() < 1e-6);
    }

    #[test]
    fn monthly_projection_lower_bounds_at_zero() {
        let m = monthly_projection(&[5.0, -10.0, 2.0]);
        assert!(m.projected_30d_usd >= 0.0);
    }

    #[test]
    fn error_mapper_recognizes_token_expired() {
        assert_eq!(
            map_user_facing_error("token_expired"),
            "Codex session expired. Sign in again."
        );
        assert_eq!(
            map_user_facing_error("openai.refresh_token_expired: nope"),
            "Codex session expired. Sign in again."
        );
    }

    #[test]
    fn error_mapper_recognizes_rate_limit_and_network() {
        assert!(map_user_facing_error("rate_limit_exceeded").contains("Try again in a few minutes"));
        assert!(map_user_facing_error("ETIMEDOUT").contains("network connection"));
        assert!(map_user_facing_error("DNS lookup failed").contains("network connection"));
    }

    #[test]
    fn error_mapper_passes_unknown_strings_through() {
        let unknown = "some_unexpected_error_xyzzy";
        assert_eq!(map_user_facing_error(unknown), unknown);
    }
}
