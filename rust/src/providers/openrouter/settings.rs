//! OpenRouter settings contributions. API-key only.

use crate::providers::settings_descriptor::SettingsDescriptor;
use crate::providers::settings_snapshot::ProviderSettingsContribution;

pub const TOKEN_ACCOUNT_TITLE: &str = "OpenRouter API key";
pub const TOKEN_ACCOUNT_HELP: &str =
    "Paste your `sk-or-v1-...` API key from openrouter.ai/keys. \
     Stored DPAPI-wrapped on disk.";

pub fn contribution() -> ProviderSettingsContribution {
    ProviderSettingsContribution {
        provider_id: "openrouter".into(),
        section_title: "OpenRouter".into(),
        rows: vec![SettingsDescriptor::TokenAccounts {
            title: TOKEN_ACCOUNT_TITLE.into(),
            subtitle: Some(TOKEN_ACCOUNT_HELP.into()),
            provider_id: "openrouter".into(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contribution_includes_token_accounts_row() {
        let c = contribution();
        assert!(c.rows.iter().any(|r| matches!(
            r,
            SettingsDescriptor::TokenAccounts { provider_id, .. } if provider_id == "openrouter"
        )));
    }
}
