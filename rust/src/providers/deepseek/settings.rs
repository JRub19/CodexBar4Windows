use crate::providers::settings_descriptor::SettingsDescriptor;
use crate::providers::settings_snapshot::ProviderSettingsContribution;

pub const TOKEN_ACCOUNT_TITLE: &str = "DeepSeek API key";
pub const TOKEN_ACCOUNT_HELP: &str =
    "Paste a key from platform.deepseek.com/api_keys. Stored DPAPI-wrapped on disk.";

pub fn contribution() -> ProviderSettingsContribution {
    ProviderSettingsContribution {
        provider_id: "deepseek".into(),
        section_title: "DeepSeek".into(),
        rows: vec![SettingsDescriptor::TokenAccounts {
            title: TOKEN_ACCOUNT_TITLE.into(),
            subtitle: Some(TOKEN_ACCOUNT_HELP.into()),
            provider_id: "deepseek".into(),
        }],
    }
}
