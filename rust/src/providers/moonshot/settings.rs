use crate::providers::settings_descriptor::{PickerOption, SettingsDescriptor};
use crate::providers::settings_snapshot::ProviderSettingsContribution;

pub const REGION_KEY: &str = "moonshot.region";
pub const TOKEN_ACCOUNT_TITLE: &str = "Moonshot API key";
pub const TOKEN_ACCOUNT_HELP: &str =
    "Paste a key from platform.moonshot.ai/console. Stored DPAPI-wrapped on disk.";

pub fn contribution() -> ProviderSettingsContribution {
    ProviderSettingsContribution {
        provider_id: "moonshot".into(),
        section_title: "Moonshot".into(),
        rows: vec![
            SettingsDescriptor::Picker {
                key: REGION_KEY.into(),
                title: "Region".into(),
                subtitle: Some(
                    "International routes to api.moonshot.ai; China routes to api.moonshot.cn."
                        .into(),
                ),
                options: vec![
                    PickerOption {
                        value: "international".into(),
                        label: "International".into(),
                    },
                    PickerOption {
                        value: "china".into(),
                        label: "China".into(),
                    },
                ],
                default: "international".into(),
            },
            SettingsDescriptor::TokenAccounts {
                title: TOKEN_ACCOUNT_TITLE.into(),
                subtitle: Some(TOKEN_ACCOUNT_HELP.into()),
                provider_id: "moonshot".into(),
            },
        ],
    }
}
