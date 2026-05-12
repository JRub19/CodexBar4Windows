//! `CodexIdentity` is the identity-only view of a Codex account. The
//! refresh loop scopes every snapshot to one of these so a user with
//! two Codex logins gets two cards, two history buckets, and two
//! cookie jars.

use super::credentials::{CodexCredentials, CodexCredentialsFull};
use super::jwt::{extract_all, ExtractedClaims};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodexIdentity {
    /// Full identity resolved from a Codex CLI auth.json with an id_token.
    Resolved {
        email: Option<String>,
        plan: Option<String>,
        account_id: Option<String>,
    },
    /// API-key-only files have no JWT; we still surface them as
    /// degraded identities so the popup can hint the user that quotas
    /// are unavailable until they `codex login`.
    ApiKeyOnly,
    /// JWT decode failed or no usable claims; the account exists but
    /// we cannot label it.
    Unresolved,
}

impl CodexIdentity {
    pub fn from_credentials(creds: &CodexCredentials) -> Self {
        match creds {
            CodexCredentials::Full(full) => Self::from_full(full, None),
            CodexCredentials::ApiKeyOnly(_) => CodexIdentity::ApiKeyOnly,
        }
    }

    pub fn from_full(creds: &CodexCredentialsFull, cli_account_id: Option<&str>) -> Self {
        let ExtractedClaims {
            email,
            plan,
            account_id,
        } = extract_all(&creds.id_token, cli_account_id);
        if email.is_none() && plan.is_none() && account_id.is_none() {
            return CodexIdentity::Unresolved;
        }
        CodexIdentity::Resolved {
            email,
            plan,
            account_id,
        }
    }

    /// Stable per-account key for the UsageStore. Prefers `account_id`
    /// then email; everything else collapses to the literal "anonymous"
    /// so we still file the snapshot somewhere without leaking the
    /// untyped JWT payload through.
    pub fn account_token(&self) -> String {
        match self {
            CodexIdentity::Resolved {
                account_id: Some(id),
                ..
            } => format!("codex:{}", id),
            CodexIdentity::Resolved {
                email: Some(email), ..
            } => format!("codex:{}", email),
            CodexIdentity::Resolved { .. } => "codex:unknown".into(),
            CodexIdentity::ApiKeyOnly => "codex:api-key".into(),
            CodexIdentity::Unresolved => "codex:anonymous".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::credentials::CodexCredentialsFull;
    use super::super::jwt::make_token;
    use super::*;
    use serde_json::json;

    fn full_with_payload(payload: serde_json::Value) -> CodexCredentialsFull {
        CodexCredentialsFull {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            id_token: make_token(&payload),
            last_refresh: None,
            openai_api_key: None,
        }
    }

    #[test]
    fn resolves_email_and_plan_from_payload() {
        let creds = full_with_payload(json!({
            "email": "user@example.com",
            "chatgpt_plan_type": "plus",
        }));
        let id = CodexIdentity::from_full(&creds, None);
        assert!(matches!(id, CodexIdentity::Resolved { .. }));
        assert_eq!(id.account_token(), "codex:user@example.com");
    }

    #[test]
    fn prefers_cli_account_id_over_jwt_claims() {
        let creds = full_with_payload(json!({
            "chatgpt_account_id": "jwt-id",
        }));
        let id = CodexIdentity::from_full(&creds, Some("CLI-id"));
        match id {
            CodexIdentity::Resolved {
                account_id: Some(id),
                ..
            } => assert_eq!(id, "cli-id"),
            other => panic!("expected resolved with cli id, got {other:?}"),
        }
    }

    #[test]
    fn malformed_payload_is_unresolved_not_panic() {
        let creds = CodexCredentialsFull {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            id_token: "not.a.jwt".into(),
            last_refresh: None,
            openai_api_key: None,
        };
        let id = CodexIdentity::from_full(&creds, None);
        assert_eq!(id, CodexIdentity::Unresolved);
    }

    #[test]
    fn api_key_only_credentials_yield_degraded_identity() {
        let creds = CodexCredentials::ApiKeyOnly("sk-abc".into());
        assert_eq!(
            CodexIdentity::from_credentials(&creds),
            CodexIdentity::ApiKeyOnly
        );
    }
}
