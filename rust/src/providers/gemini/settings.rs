//! Gemini settings contributions. The credential file lives at
//! `~/.gemini/oauth_creds.json`; the popup mostly shows status here
//! and lets the user pick whether to surface the provider.

use crate::providers::settings_descriptor::{PickerOption, SettingsDescriptor};
use crate::providers::settings_snapshot::ProviderSettingsContribution;

pub const SOURCE_KEY: &str = "gemini.source_mode";

pub fn contribution() -> ProviderSettingsContribution {
    ProviderSettingsContribution {
        provider_id: "gemini".into(),
        section_title: "Gemini".into(),
        rows: vec![SettingsDescriptor::Picker {
            key: SOURCE_KEY.into(),
            title: "Source".into(),
            subtitle: Some(
                "Reads `~/.gemini/oauth_creds.json` produced by the gemini CLI. \
                 API-key and Vertex AI auth modes are not supported."
                    .into(),
            ),
            options: vec![
                PickerOption {
                    value: "auto".into(),
                    label: "Auto".into(),
                },
                PickerOption {
                    value: "oauth".into(),
                    label: "OAuth (Google account)".into(),
                },
                PickerOption {
                    value: "disabled".into(),
                    label: "Disabled".into(),
                },
            ],
            default: "auto".into(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contribution_exposes_source_picker() {
        let c = contribution();
        assert!(c.rows.iter().any(|r| matches!(
            r,
            SettingsDescriptor::Picker { key, .. } if key == SOURCE_KEY
        )));
    }
}
