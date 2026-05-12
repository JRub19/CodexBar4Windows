//! Claude settings contributions surfaced to the React preferences pane.
//!
//! The shape mirrors spec 40 section 5. Today we expose:
//! - A Picker for the source mode (Auto/OAuth/Web/CLI/Disabled).
//! - A TokenAccounts row for paste-in OAuth bearers and session cookies.
//! - A boolean Toggle to suppress the CLI strategy when the user has
//!   no `claude` binary installed.

use crate::providers::settings_descriptor::{PickerOption, SettingsDescriptor};
use crate::providers::settings_snapshot::ProviderSettingsContribution;

pub const SOURCE_KEY: &str = "claude.source_mode";
pub const CLI_ENABLED_KEY: &str = "claude.cli_enabled";

pub fn contribution() -> ProviderSettingsContribution {
    ProviderSettingsContribution {
        provider_id: "claude".into(),
        section_title: "Claude".into(),
        rows: vec![
            SettingsDescriptor::Picker {
                key: SOURCE_KEY.into(),
                title: "Source".into(),
                subtitle: Some(
                    "Pick which fetch path the refresh loop tries first. \
                     Auto tries OAuth, Web, then CLI in turn."
                        .into(),
                ),
                options: vec![
                    PickerOption {
                        value: "auto".into(),
                        label: "Auto".into(),
                    },
                    PickerOption {
                        value: "oauth".into(),
                        label: "OAuth (Claude Code token)".into(),
                    },
                    PickerOption {
                        value: "web".into(),
                        label: "Web (browser cookie)".into(),
                    },
                    PickerOption {
                        value: "cli".into(),
                        label: "CLI (claude binary)".into(),
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
                title: "Enable Claude CLI fallback".into(),
                subtitle: Some(
                    "When on, the refresh loop falls back to the local \
                     `claude` binary if OAuth and Web both fail."
                        .into(),
                ),
                default: true,
            },
            SettingsDescriptor::TokenAccounts {
                title: super::tokens::TOKEN_ACCOUNT_TITLE.into(),
                subtitle: Some(super::tokens::TOKEN_ACCOUNT_HELP.into()),
                provider_id: "claude".into(),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contribution_has_picker_with_five_options() {
        let c = contribution();
        let picker = c
            .rows
            .iter()
            .find_map(|r| match r {
                SettingsDescriptor::Picker { options, .. } => Some(options),
                _ => None,
            })
            .expect("picker descriptor present");
        let labels: Vec<_> = picker.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(labels, vec!["auto", "oauth", "web", "cli", "disabled"]);
    }

    #[test]
    fn contribution_includes_token_accounts_row() {
        let c = contribution();
        assert!(c.rows.iter().any(|r| matches!(
            r,
            SettingsDescriptor::TokenAccounts { provider_id, .. } if provider_id == "claude"
        )));
    }
}
