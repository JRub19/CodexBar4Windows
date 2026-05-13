//! Codex consumer-plan projection. Ported from
//! `docs/windows/spec/70-cost-scanning.md` §10 +
//! `docs/windows/plan/phase-7-cost-status-notifications.md` §A12.
//!
//! The Codex *consumer* plan (paid ChatGPT plan; not cost-usage) has
//! its own projection separate from the JSONL cost scanner. It does
//! **not** smooth-or-extrapolate from local JSONL data. Inputs are
//! live snapshots:
//!
//! - `UsageSnapshot` with two known rate windows: `session` (5h
//!   cycle) and `weekly` (7-day cycle).
//! - Optional `CreditsSnapshot` for the credits-remaining fallback.
//! - Optional `dashboard.rolling_30d_cost` series for the smoothed
//!   monthly projection.
//!
//! Outputs feed:
//!
//! - the tray-icon double-bar (session ▸ weekly),
//! - the popup card's "Plan utilization" section (session → weekly
//!   in semantic priority order),
//! - the menu-bar credits fallback when every rate lane is fully
//!   exhausted,
//! - the user-facing error string mapped through
//!   `map_user_facing_error` (e.g. `token_expired` → "Codex session
//!   expired. Sign in again.").
//!
//! Smoothing for the monthly projection clamps the rolling-window
//! divisor at `1/7`, matching the Phase 7 A12 bug-fix
//! (`max(1.0 / max(1, n), 1.0 / 7.0)`). This avoids the divide-by-zero
//! that affected fresh installs with zero days of dashboard history.

pub mod codex;

pub use codex::{
    map_user_facing_error, monthly_projection, project_codex_consumer, CodexConsumerProjection,
    CreditsProjection, MonthlyProjection, RateLane,
};
