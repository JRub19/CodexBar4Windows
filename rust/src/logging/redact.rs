//! Newtype that hides its inner string from logs by default.
//!
//! Wrap every token, cookie header, API key, refresh token, OAuth code, or
//! identifying account email at the point of construction. The `Display` and
//! `Debug` impls render `<redacted: N chars>` so accidental
//! `info!(?secret)` calls do not leak data. The raw value is reachable via
//! [`SensitiveString::expose_secret`] for the rare call site (an HTTP
//! Authorization header, for example) that needs it.

use std::fmt;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_redacts() {
        let s = SensitiveString::new("sk-ant-oat0123456789");
        assert_eq!(format!("{}", s), "<redacted: 20 chars>");
        assert_eq!(format!("{:?}", s), "SensitiveString(<redacted: 20 chars>)");
    }

    #[test]
    fn expose_returns_raw() {
        let s = SensitiveString::new("hunter2");
        assert_eq!(s.expose_secret(), "hunter2");
    }

    #[test]
    fn len_counts_chars_not_bytes() {
        let s = SensitiveString::new("näive");
        assert_eq!(s.len(), 5);
    }
}
