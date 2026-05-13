//! Copilot settings contributions.

use crate::providers::settings_descriptor::{PickerOption, SettingsDescriptor};
use crate::providers::settings_snapshot::ProviderSettingsContribution;

pub const SOURCE_KEY: &str = "copilot.source_mode";
pub const ENTERPRISE_HOST_KEY: &str = "copilot.enterprise_host";
pub const TOKEN_ACCOUNT_TITLE: &str = "Copilot GitHub token";
pub const TOKEN_ACCOUNT_HELP: &str =
    "Paste the GitHub OAuth access token used by your Copilot subscription. \
     The device-code login flow stores this for you on success.";

pub fn contribution() -> ProviderSettingsContribution {
    ProviderSettingsContribution {
        provider_id: "copilot".into(),
        section_title: "GitHub Copilot".into(),
        rows: vec![
            SettingsDescriptor::Picker {
                key: SOURCE_KEY.into(),
                title: "Source".into(),
                subtitle: Some("Copilot only uses the GitHub OAuth API path today.".into()),
                options: vec![
                    PickerOption {
                        value: "auto".into(),
                        label: "Auto".into(),
                    },
                    PickerOption {
                        value: "oauth".into(),
                        label: "OAuth (GitHub token)".into(),
                    },
                    PickerOption {
                        value: "disabled".into(),
                        label: "Disabled".into(),
                    },
                ],
                default: "auto".into(),
            },
            SettingsDescriptor::Field {
                key: ENTERPRISE_HOST_KEY.into(),
                title: "Enterprise host".into(),
                subtitle: Some(
                    "GHE host (e.g. github.example.com). Leave blank for github.com.".into(),
                ),
                placeholder: Some("github.com".into()),
                secret: false,
            },
            SettingsDescriptor::TokenAccounts {
                title: TOKEN_ACCOUNT_TITLE.into(),
                subtitle: Some(TOKEN_ACCOUNT_HELP.into()),
                provider_id: "copilot".into(),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contribution_includes_enterprise_host_and_tokens_row() {
        let c = contribution();
        assert!(c.rows.iter().any(|r| matches!(
            r,
            SettingsDescriptor::Field { key, .. } if key == ENTERPRISE_HOST_KEY
        )));
        assert!(c.rows.iter().any(|r| matches!(
            r,
            SettingsDescriptor::TokenAccounts { provider_id, .. } if provider_id == "copilot"
        )));
    }
}
