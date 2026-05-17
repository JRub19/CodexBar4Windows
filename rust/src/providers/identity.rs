//! Provider identity siloing. Spec 30 section 2.2 requires every
//! `UsageSnapshot` to carry the provider id plus an opaque account
//! identifier so the store can refuse cross-provider writes.

use serde::{Deserialize, Serialize};

use crate::core::ProviderId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderIdentitySnapshot {
    pub provider_id: String,
    /// Stable per-account identifier (account uuid, email hash, etc.).
    /// Opaque to the store; the provider implementation decides what
    /// to use, as long as the same account always produces the same
    /// value across refreshes.
    pub account_token: String,
}

impl ProviderIdentitySnapshot {
    pub fn new(provider_id: ProviderId, account_token: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.as_str().to_string(),
            account_token: account_token.into(),
        }
    }

    pub fn scope_matches(&self, provider_id: ProviderId) -> bool {
        self.provider_id.as_str() == provider_id.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_matches_when_provider_id_matches() {
        let id = ProviderIdentitySnapshot::new(ProviderId("claude"), "acct-1");
        assert!(id.scope_matches(ProviderId("claude")));
        assert!(!id.scope_matches(ProviderId("codex")));
    }
}
