//! Cursor web endpoint list. Ported verbatim from
//! `Sources/CodexBarCore/Providers/Cursor/CursorStatusProbe.swift` so the
//! Windows port speaks the same JSON contract macOS does.

pub const HOST: &str = "https://cursor.com";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorWebEndpoint {
    /// `/api/usage-summary` — the headline payload with plan/team/on-demand cents.
    UsageSummary,
    /// `/api/auth/me` — current user (email, name, sub used by /api/usage).
    AuthMe,
    /// `/api/usage?user=<sub>` — legacy request-based plan counter.
    LegacyUsage,
}

impl CursorWebEndpoint {
    pub fn path(self) -> &'static str {
        match self {
            CursorWebEndpoint::UsageSummary => "/api/usage-summary",
            CursorWebEndpoint::AuthMe => "/api/auth/me",
            CursorWebEndpoint::LegacyUsage => "/api/usage",
        }
    }
}

pub fn usage_summary_url() -> String {
    format!("{HOST}{}", CursorWebEndpoint::UsageSummary.path())
}

pub fn auth_me_url() -> String {
    format!("{HOST}{}", CursorWebEndpoint::AuthMe.path())
}

pub fn legacy_usage_url(user_sub: &str) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("user", user_sub)
        .finish();
    format!("{HOST}{}?{query}", CursorWebEndpoint::LegacyUsage.path())
}
