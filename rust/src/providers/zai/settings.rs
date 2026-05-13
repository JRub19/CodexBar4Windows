use crate::providers::settings_descriptor::{PickerOption, SettingsDescriptor};
use crate::providers::settings_snapshot::ProviderSettingsContribution;

pub const REGION_KEY: &str = "zai.region";
pub const HOST_OVERRIDE_KEY: &str = "zai.api_host";
pub const TOKEN_ACCOUNT_TITLE: &str = "Z.ai API key";
pub const TOKEN_ACCOUNT_HELP: &str =
    "Paste a key from z.ai/manage-apikey or open.bigmodel.cn. Stored DPAPI-wrapped on disk.";

pub fn contribution() -> ProviderSettingsContribution {
    ProviderSettingsContribution {
        provider_id: "zai".into(),
        section_title: "Z.ai".into(),
        rows: vec![
            SettingsDescriptor::Picker {
                key: REGION_KEY.into(),
                title: "Region".into(),
                subtitle: Some(
                    "Global routes to api.z.ai; BigModel CN routes to open.bigmodel.cn.".into(),
                ),
                options: vec![
                    PickerOption {
                        value: "global".into(),
                        label: "Global".into(),
                    },
                    PickerOption {
                        value: "bigmodel-cn".into(),
                        label: "BigModel CN".into(),
                    },
                ],
                default: "global".into(),
            },
            SettingsDescriptor::Field {
                key: HOST_OVERRIDE_KEY.into(),
                title: "Custom API host".into(),
                subtitle: Some(
                    "Optional host or full URL. Overrides the region picker when set.".into(),
                ),
                placeholder: Some("api.z.ai".into()),
                secret: false,
            },
            SettingsDescriptor::TokenAccounts {
                title: TOKEN_ACCOUNT_TITLE.into(),
                subtitle: Some(TOKEN_ACCOUNT_HELP.into()),
                provider_id: "zai".into(),
            },
        ],
    }
}
