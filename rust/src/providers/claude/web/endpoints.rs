//! Claude.ai web endpoint list. Spec 40 section 6.2 names every URL we
//! poll plus the field each one contributes. The list is fixed at
//! compile time so a runtime config drift cannot ask us to call a
//! surprise URL.

/// Tagged endpoint identifier. The strategy iterates this list in order,
/// stopping early when the user's cookies cannot reach a given endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaudeWebEndpoint {
    Organizations,
    UsageRollup,
    UsageBreakdown,
    Subscription,
}

impl ClaudeWebEndpoint {
    pub fn path(self) -> &'static str {
        match self {
            ClaudeWebEndpoint::Organizations => "/api/organizations",
            ClaudeWebEndpoint::UsageRollup => "/api/organizations/{org}/usage/rollup",
            ClaudeWebEndpoint::UsageBreakdown => "/api/organizations/{org}/usage/breakdown",
            ClaudeWebEndpoint::Subscription => "/api/organizations/{org}/subscription",
        }
    }

    /// The order callers should walk through. The first endpoint is the
    /// only one that depends on no other state; later endpoints require
    /// an org id from `Organizations`.
    pub fn ordered() -> &'static [ClaudeWebEndpoint] {
        &[
            ClaudeWebEndpoint::Organizations,
            ClaudeWebEndpoint::UsageRollup,
            ClaudeWebEndpoint::UsageBreakdown,
            ClaudeWebEndpoint::Subscription,
        ]
    }
}

pub const HOST: &str = "https://claude.ai";
