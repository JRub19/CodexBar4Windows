//! Per-provider settings contribution and the merged snapshot the React
//! side reads. Phase 4 P4-06 keeps this in-memory; the persistent values
//! still live in `Settings`. The snapshot is rebuilt every time the
//! settings change.

use serde::Serialize;

use super::settings_descriptor::SettingsDescriptor;

#[derive(Clone, Debug, Serialize)]
pub struct ProviderSettingsContribution {
    pub provider_id: String,
    pub section_title: String,
    pub rows: Vec<SettingsDescriptor>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ProviderSettingsSnapshot {
    pub sections: Vec<ProviderSettingsContribution>,
}

impl ProviderSettingsSnapshot {
    pub fn builder() -> Builder {
        Builder::default()
    }
}

#[derive(Default)]
pub struct Builder {
    sections: Vec<ProviderSettingsContribution>,
}

impl Builder {
    pub fn add(mut self, contribution: ProviderSettingsContribution) -> Self {
        self.sections.push(contribution);
        self
    }

    pub fn build(self) -> ProviderSettingsSnapshot {
        ProviderSettingsSnapshot {
            sections: self.sections,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::settings_descriptor::{PickerOption, SettingsDescriptor};

    #[test]
    fn builder_collects_contributions_in_order() {
        let snap = ProviderSettingsSnapshot::builder()
            .add(ProviderSettingsContribution {
                provider_id: "claude".into(),
                section_title: "Claude".into(),
                rows: vec![SettingsDescriptor::Picker {
                    key: "claude.source".into(),
                    title: "Source".into(),
                    subtitle: None,
                    options: vec![PickerOption {
                        value: "auto".into(),
                        label: "Auto".into(),
                    }],
                    default: "auto".into(),
                }],
            })
            .add(ProviderSettingsContribution {
                provider_id: "codex".into(),
                section_title: "Codex".into(),
                rows: vec![],
            })
            .build();
        assert_eq!(snap.sections.len(), 2);
        assert_eq!(snap.sections[0].provider_id, "claude");
        assert_eq!(snap.sections[1].provider_id, "codex");
    }
}
