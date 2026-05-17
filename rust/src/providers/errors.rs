//! Shared provider error types.

use thiserror::Error;

/// Registry-level errors. These surface during catalog build, not during
/// the refresh loop.
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("duplicate provider id: {0}")]
    DuplicateId(&'static str),
    #[error("provider {0} is not registered")]
    Unknown(&'static str),
}

/// Per-strategy fetch error. Each variant maps to a default
/// `should_fallback` answer that the runtime consults when deciding
/// whether to advance to the next strategy in the plan.
#[derive(Debug, Error, Clone)]
pub enum ProviderFetchError {
    #[error("strategy timed out after {budget_ms} ms")]
    Timeout { budget_ms: u64 },
    #[error("network error: {0}")]
    Network(String),
    #[error("no usable cookies available for {0}")]
    NoCookies(&'static str),
    #[error("no usable token available for {0}")]
    NoToken(&'static str),
    #[error("plugin or external binary unavailable: {0}")]
    PluginUnavailable(String),
    #[error("unauthorized; user must reauthenticate")]
    Unauthorized,
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("response parse error: {0}")]
    ParseError(String),
    #[error("user configuration invalid: {0}")]
    UserConfigInvalid(String),
}

impl ProviderFetchError {
    /// Default fallback decision per spec 30 section 13.3. Strategy
    /// implementations may override via their own `should_fallback`.
    pub fn should_fallback(&self) -> bool {
        match self {
            ProviderFetchError::Timeout { .. }
            | ProviderFetchError::Network(_)
            | ProviderFetchError::NoCookies(_)
            | ProviderFetchError::NoToken(_)
            | ProviderFetchError::PluginUnavailable(_) => true,
            ProviderFetchError::Unauthorized
            | ProviderFetchError::PermissionDenied(_)
            | ProviderFetchError::ParseError(_)
            | ProviderFetchError::UserConfigInvalid(_) => false,
        }
    }

    /// Stable identifier for logs and IPC payloads. Independent of the
    /// human-readable display string so callers can match on it.
    pub fn kind(&self) -> &'static str {
        match self {
            ProviderFetchError::Timeout { .. } => "timeout",
            ProviderFetchError::Network(_) => "network",
            ProviderFetchError::NoCookies(_) => "no_cookies",
            ProviderFetchError::NoToken(_) => "no_token",
            ProviderFetchError::PluginUnavailable(_) => "plugin_unavailable",
            ProviderFetchError::Unauthorized => "unauthorized",
            ProviderFetchError::PermissionDenied(_) => "permission_denied",
            ProviderFetchError::ParseError(_) => "parse_error",
            ProviderFetchError::UserConfigInvalid(_) => "user_config_invalid",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_true_for_transient_errors() {
        assert!(ProviderFetchError::Timeout { budget_ms: 0 }.should_fallback());
        assert!(ProviderFetchError::Network("conn".into()).should_fallback());
        assert!(ProviderFetchError::NoCookies("claude").should_fallback());
        assert!(ProviderFetchError::NoToken("claude").should_fallback());
        assert!(ProviderFetchError::PluginUnavailable("cli".into()).should_fallback());
    }

    #[test]
    fn fallback_false_for_terminal_errors() {
        assert!(!ProviderFetchError::Unauthorized.should_fallback());
        assert!(!ProviderFetchError::PermissionDenied("no".into()).should_fallback());
        assert!(!ProviderFetchError::ParseError("bad json".into()).should_fallback());
        assert!(!ProviderFetchError::UserConfigInvalid("bad".into()).should_fallback());
    }

    #[test]
    fn kind_is_stable() {
        assert_eq!(ProviderFetchError::Unauthorized.kind(), "unauthorized");
        assert_eq!(
            ProviderFetchError::Timeout { budget_ms: 1 }.kind(),
            "timeout",
        );
    }
}
