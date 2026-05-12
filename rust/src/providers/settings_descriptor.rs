//! Generic settings descriptor enum. Providers contribute one of these
//! per row they want shown in the preferences pane. The React side
//! renders each variant generically so adding a new provider does not
//! require any React work.
//!
//! Spec 30 section 9.1 enumerates the canonical variants.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingsDescriptor {
    /// A boolean toggle row. Stored in `Settings` under `key`.
    Toggle {
        key: String,
        title: String,
        subtitle: Option<String>,
        default: bool,
    },
    /// A free-form text field. `secret = true` switches the input to
    /// password mode and routes storage through the Credential Manager.
    Field {
        key: String,
        title: String,
        subtitle: Option<String>,
        placeholder: Option<String>,
        secret: bool,
    },
    /// A dropdown with a fixed option list. Used by Source pickers.
    Picker {
        key: String,
        title: String,
        subtitle: Option<String>,
        options: Vec<PickerOption>,
        default: String,
    },
    /// A row of inline action buttons. Each button maps to an
    /// `invoke(action_id)` Tauri command on click.
    ActionsRow {
        title: String,
        actions: Vec<SettingsAction>,
    },
    /// Multi-account list with add/remove/edit/set-active controls.
    TokenAccounts {
        title: String,
        subtitle: Option<String>,
        provider_id: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PickerOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SettingsAction {
    pub id: String,
    pub label: String,
    pub destructive: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_round_trips_through_serde() {
        let descriptor = SettingsDescriptor::Picker {
            key: "claude.source".into(),
            title: "Source".into(),
            subtitle: Some("Pick the fetch strategy".into()),
            options: vec![
                PickerOption {
                    value: "auto".into(),
                    label: "Auto".into(),
                },
                PickerOption {
                    value: "oauth".into(),
                    label: "OAuth".into(),
                },
            ],
            default: "auto".into(),
        };
        let json = serde_json::to_string(&descriptor).unwrap();
        // Sanity-check the discriminator is present so the React side
        // can branch on it.
        assert!(json.contains("\"kind\":\"picker\""));
        let back: SettingsDescriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(descriptor, back);
    }

    #[test]
    fn toggle_variant_carries_default_value() {
        let toggle = SettingsDescriptor::Toggle {
            key: "claude.cli_enabled".into(),
            title: "Enable Claude CLI".into(),
            subtitle: None,
            default: true,
        };
        let json = serde_json::to_string(&toggle).unwrap();
        assert!(json.contains("\"default\":true"));
    }
}
