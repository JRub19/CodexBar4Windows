//! Cursor settings contributions. Surfaces the source-mode picker
//! plus a token-accounts row so the user can paste a cookie header
//! captured from `cursor.com`.

use crate::providers::settings_descriptor::{PickerOption, SettingsDescriptor};
use crate::providers::settings_snapshot::ProviderSettingsContribution;

pub const SOURCE_KEY: &str = "cursor.source_mode";
pub const TOKEN_ACCOUNT_TITLE: &str = "Cursor cookie";
pub const TOKEN_ACCOUNT_HELP: &str =
    "Paste a Cookie header captured from cursor.com (Workos session token + supporting cookies). \
     Stored DPAPI-wrapped on disk.";

pub fn contribution() -> ProviderSettingsContribution {
    ProviderSettingsContribution {
        provider_id: "cursor".into(),
        section_title: "Cursor".into(),
        rows: vec![
            SettingsDescriptor::Picker {
                key: SOURCE_KEY.into(),
                title: "Source".into(),
                subtitle: Some("Cursor only supports the web cookie path today.".into()),
                options: vec![
                    PickerOption {
                        value: "auto".into(),
                        label: "Auto".into(),
                    },
                    PickerOption {
                        value: "web".into(),
                        label: "Web (browser cookie)".into(),
                    },
                    PickerOption {
                        value: "disabled".into(),
                        label: "Disabled".into(),
                    },
                ],
                default: "auto".into(),
            },
            SettingsDescriptor::TokenAccounts {
                title: TOKEN_ACCOUNT_TITLE.into(),
                subtitle: Some(TOKEN_ACCOUNT_HELP.into()),
                provider_id: "cursor".into(),
            },
        ],
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
            SettingsDescriptor::TokenAccounts { provider_id, .. } if provider_id == "cursor"
        )));
    }
}
