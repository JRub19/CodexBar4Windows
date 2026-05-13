//! Factory settings contributions.

use crate::providers::settings_descriptor::SettingsDescriptor;
use crate::providers::settings_snapshot::ProviderSettingsContribution;

pub const TOKEN_ACCOUNT_TITLE: &str = "Factory credentials";
pub const TOKEN_ACCOUNT_HELP: &str =
    "Paste a Factory bearer token (`Bearer ...`) or a Cookie header from app.factory.ai. \
     Bearer tokens take priority when both are stored.";

pub fn contribution() -> ProviderSettingsContribution {
    ProviderSettingsContribution {
        provider_id: "factory".into(),
        section_title: "Factory".into(),
        rows: vec![SettingsDescriptor::TokenAccounts {
            title: TOKEN_ACCOUNT_TITLE.into(),
            subtitle: Some(TOKEN_ACCOUNT_HELP.into()),
            provider_id: "factory".into(),
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
            SettingsDescriptor::TokenAccounts { provider_id, .. } if provider_id == "factory"
        )));
    }
}
