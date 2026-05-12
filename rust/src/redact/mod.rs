//! Sensitive value redaction.
//!
//! Two responsibilities:
//!
//! 1. `SensitiveString` newtype: wraps every token, cookie header, API key,
//!    OAuth code, refresh token, or identifying account email at the point
//!    of construction. The `Debug` and `Display` impls render
//!    `<redacted: N chars>` so accidental `info!(?secret)` calls do not
//!    leak data. The raw value is reachable via `expose_secret` for the
//!    rare call site (an HTTP `Authorization` header, for example) that
//!    needs it.
//!
//! 2. `Redactor`: pattern based scrubber for free form strings, used when a
//!    third party crate writes a string we did not author (a CLI stdout
//!    line, for example). Phase 2 task 2.18 wires this into a
//!    `tracing::Layer` that scrubs `tracing` event fields globally.

pub mod tracing_layer;

use std::fmt;

use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Clone)]
pub struct SensitiveString(String);

impl SensitiveString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn len(&self) -> usize {
        self.0.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the raw inner value. Call sites must not log the result.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SensitiveString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SensitiveString(<redacted: {} chars>)", self.len())
    }
}

impl fmt::Display for SensitiveString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<redacted: {} chars>", self.len())
    }
}

impl From<String> for SensitiveString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for SensitiveString {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl PartialEq for SensitiveString {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for SensitiveString {}

/// Pattern based string scrubber. Use only for strings whose contents we
/// did not author and cannot wrap in [`SensitiveString`].
pub struct Redactor;

static EMAIL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
        .expect("static email regex must compile")
});

static BEARER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)Bearer\s+[A-Za-z0-9._\-+/=]+").expect("static bearer regex must compile")
});

static SK_ANT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"sk-(?:ant-|cp-|api-|live-|test-)?[A-Za-z0-9_\-]{20,}")
        .expect("static api-key regex must compile")
});

impl Redactor {
    /// Replace every email address with `<redacted-email>`.
    pub fn email(input: &str) -> String {
        EMAIL_RE.replace_all(input, "<redacted-email>").into_owned()
    }

    /// Replace every `Bearer <token>` with `Bearer <redacted>`.
    pub fn bearer(input: &str) -> String {
        BEARER_RE
            .replace_all(input, "Bearer <redacted>")
            .into_owned()
    }

    /// Replace `sk-ant-...`, `sk-cp-...`, `sk-api-...`, etc.
    pub fn api_key(input: &str) -> String {
        SK_ANT_RE
            .replace_all(input, "<redacted-api-key>")
            .into_owned()
    }

    /// Run every redactor in order. Use this when a string may contain any
    /// of the patterns and you do not know which.
    pub fn all(input: &str) -> String {
        let s = Self::email(input);
        let s = Self::bearer(&s);
        Self::api_key(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_display_redacts() {
        let s = SensitiveString::new("sk-ant-oat0123456789");
        assert_eq!(format!("{}", s), "<redacted: 20 chars>");
        assert_eq!(format!("{:?}", s), "SensitiveString(<redacted: 20 chars>)");
    }

    #[test]
    fn sensitive_expose_returns_raw() {
        let s = SensitiveString::new("hunter2");
        assert_eq!(s.expose_secret(), "hunter2");
    }

    #[test]
    fn sensitive_len_counts_chars_not_bytes() {
        let s = SensitiveString::new("näive");
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn redactor_email_scrubs() {
        let scrubbed = Redactor::email("contact alice@example.com today");
        assert_eq!(scrubbed, "contact <redacted-email> today");
    }

    #[test]
    fn redactor_bearer_scrubs_and_preserves_prefix() {
        let scrubbed = Redactor::bearer("Authorization: Bearer abc.def-ghi+jkl/mno=");
        assert_eq!(scrubbed, "Authorization: Bearer <redacted>");
    }

    #[test]
    fn redactor_api_key_handles_common_prefixes() {
        for raw in [
            "sk-ant-oat0123456789ABCDEF0123",
            "sk-cp-supersecretvaluexyzabc12345",
            "sk-api-supersecretvaluexyzabc12345",
            "sk-live-1234567890abcdef1234567890",
        ] {
            let scrubbed = Redactor::api_key(raw);
            assert_eq!(scrubbed, "<redacted-api-key>", "input {raw}");
        }
    }

    #[test]
    fn redactor_all_runs_every_layer() {
        let raw = "user a@b.co header Bearer xyz_token_value secret sk-ant-aaaaaaaaaaaaaaaaaaaaaa";
        let scrubbed = Redactor::all(raw);
        assert!(!scrubbed.contains("a@b.co"));
        assert!(!scrubbed.contains("xyz_token_value"));
        assert!(!scrubbed.contains("aaaaaaaaaaaaaaaaaaaaaa"));
    }
}
