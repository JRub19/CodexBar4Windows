//! Predicate that decides whether a `CodexOAuthError` should fall
//! through to the CLI strategy. Hard mode pins (`SourceMode::Forced`)
//! never fall back; only `SourceMode::Auto` does, and only for errors
//! the CLI can actually fix.
//!
//! Spec 41 §3.6 truth table. Tests below assert every variant so a new
//! enum case forces a deliberate decision.

use crate::providers::codex::auth::errors::CodexOAuthError;
use crate::providers::fetch_context::SourceMode;

pub fn should_fallback(err: &CodexOAuthError, mode: SourceMode) -> bool {
    if !matches!(mode, SourceMode::Auto) {
        return false;
    }
    matches!(
        err,
        CodexOAuthError::Unauthorized
            | CodexOAuthError::CredentialsNotFound
            | CodexOAuthError::CredentialsMissingTokens
            | CodexOAuthError::RefreshExpired(_)
            | CodexOAuthError::RefreshRevoked
            | CodexOAuthError::RefreshReused
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::descriptor::FetchStrategy;

    fn all_variants() -> Vec<CodexOAuthError> {
        vec![
            CodexOAuthError::Unauthorized,
            CodexOAuthError::CredentialsNotFound,
            CodexOAuthError::CredentialsMissingTokens,
            CodexOAuthError::RefreshExpired("e".into()),
            CodexOAuthError::RefreshRevoked,
            CodexOAuthError::RefreshReused,
            CodexOAuthError::InvalidResponse,
            CodexOAuthError::ServerError(500),
            CodexOAuthError::NetworkError("n".into()),
            CodexOAuthError::DecodeFailed("d".into()),
            CodexOAuthError::RefreshNetworkError("rn".into()),
            CodexOAuthError::RefreshInvalidResponse("ri".into()),
        ]
    }

    #[test]
    fn auto_mode_falls_back_only_for_recoverable_auth_errors() {
        for err in all_variants() {
            let expected = matches!(
                err,
                CodexOAuthError::Unauthorized
                    | CodexOAuthError::CredentialsNotFound
                    | CodexOAuthError::CredentialsMissingTokens
                    | CodexOAuthError::RefreshExpired(_)
                    | CodexOAuthError::RefreshRevoked
                    | CodexOAuthError::RefreshReused
            );
            assert_eq!(
                should_fallback(&err, SourceMode::Auto),
                expected,
                "wrong auto fallback for {err:?}"
            );
        }
    }

    #[test]
    fn forced_oauth_mode_never_falls_back() {
        for err in all_variants() {
            assert!(
                !should_fallback(&err, SourceMode::Forced(FetchStrategy::OAuth)),
                "forced mode should never fall back for {err:?}",
            );
        }
    }

    #[test]
    fn disabled_mode_never_falls_back() {
        for err in all_variants() {
            assert!(
                !should_fallback(&err, SourceMode::Disabled),
                "disabled mode should never fall back for {err:?}",
            );
        }
    }
}
