//! Codex settings contributions for the preferences pane.

use crate::providers::settings_descriptor::{PickerOption, SettingsDescriptor};
use crate::providers::settings_snapshot::ProviderSettingsContribution;

pub const SOURCE_KEY: &str = "codex.source_mode";
pub const CLI_ENABLED_KEY: &str = "codex.cli_enabled";

pub fn contribution() -> ProviderSettingsContribution {
    ProviderSettingsContribution {
        provider_id: "codex".into(),
        section_title: "Codex".into(),
        rows: vec![
            SettingsDescriptor::Picker {
                key: SOURCE_KEY.into(),
                title: "Source".into(),
                subtitle: Some(
                    "Pick the fetch path the refresh loop tries first. \
                     Auto tries OAuth API, then the local CLI."
                        .into(),
                ),
                options: vec![
                    PickerOption {
                        value: "auto".into(),
                        label: "Auto".into(),
                    },
                    PickerOption {
                        value: "oauth".into(),
                        label: "OAuth (Codex CLI token)".into(),
                    },
                    PickerOption {
                        value: "cli".into(),
                        label: "CLI (codex binary)".into(),
                    },
                    PickerOption {
                        value: "disabled".into(),
                        label: "Disabled".into(),
                    },
                ],
                default: "auto".into(),
            },
            SettingsDescriptor::Toggle {
                key: CLI_ENABLED_KEY.into(),
                title: "Enable Codex CLI fallback".into(),
                subtitle: Some(
                    "When on, the refresh loop falls back to the local \
                     `codex` binary if the OAuth API fails."
                        .into(),
                ),
                default: true,
            },
            SettingsDescriptor::TokenAccounts {
                title: "Codex accounts".into(),
                subtitle: Some(
                    "Multi-account promotion lands in a follow-up phase. \
                     Today, sign in via `codex login` and CodexBar will \
                     pick up your active account on the next refresh."
                        .into(),
                ),
                provider_id: "codex".into(),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contribution_lists_four_source_options() {
        let c = contribution();
        let picker = c
            .rows
            .iter()
            .find_map(|r| match r {
                SettingsDescriptor::Picker { options, .. } => Some(options),
                _ => None,
            })
            .expect("picker descriptor present");
        let values: Vec<_> = picker.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(values, vec!["auto", "oauth", "cli", "disabled"]);
    }

    #[test]
    fn contribution_section_title_is_codex() {
        let c = contribution();
        assert_eq!(c.provider_id, "codex");
        assert_eq!(c.section_title, "Codex");
    }
}
