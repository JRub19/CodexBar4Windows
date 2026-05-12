//! Settings data model. Phase 1 ships a minimum subset of the macOS schema.
//! Later phases extend the structs in place. New fields must default safely
//! so older `config.json` files load without intervention.

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Settings {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub refresh_frequency: RefreshFrequency,
    #[serde(default)]
    pub pause_refresh: bool,
    #[serde(default)]
    pub providers: Vec<ProviderToggle>,
    #[serde(default)]
    pub display: DisplayPreferences,
    #[serde(default)]
    pub debug: DebugFlags,
    #[serde(default)]
    pub app_language: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            refresh_frequency: RefreshFrequency::default(),
            pause_refresh: false,
            providers: Vec::new(),
            display: DisplayPreferences::default(),
            debug: DebugFlags::default(),
            app_language: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshFrequency {
    Manual,
    OneMinute,
    TwoMinutes,
    FiveMinutes,
    FifteenMinutes,
    ThirtyMinutes,
}

impl Default for RefreshFrequency {
    fn default() -> Self {
        Self::FiveMinutes
    }
}

impl RefreshFrequency {
    pub fn as_duration(&self) -> Option<std::time::Duration> {
        let secs = match self {
            Self::Manual => return None,
            Self::OneMinute => 60,
            Self::TwoMinutes => 120,
            Self::FiveMinutes => 300,
            Self::FifteenMinutes => 900,
            Self::ThirtyMinutes => 1800,
        };
        Some(std::time::Duration::from_secs(secs))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProviderToggle {
    pub id: String,
    pub enabled: bool,
    #[serde(default)]
    pub order: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DisplayPreferences {
    #[serde(default)]
    pub merge_icons: bool,
    #[serde(default)]
    pub usage_bars_show_used: bool,
    #[serde(default)]
    pub hide_quota_warning_markers: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DebugFlags {
    #[serde(default)]
    pub debug_menu_enabled: bool,
    #[serde(default)]
    pub verbose_logging: bool,
}

/// A partial mirror of `Settings` where every field is optional. `update_settings`
/// applies only the fields explicitly present in the patch.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default)]
pub struct SettingsPatch {
    pub refresh_frequency: Option<RefreshFrequency>,
    pub pause_refresh: Option<bool>,
    pub providers: Option<Vec<ProviderToggle>>,
    pub display: Option<DisplayPreferences>,
    pub debug: Option<DebugFlags>,
    pub app_language: Option<Option<String>>,
}

impl Settings {
    pub fn apply_patch(mut self, patch: SettingsPatch) -> Self {
        if let Some(v) = patch.refresh_frequency {
            self.refresh_frequency = v;
        }
        if let Some(v) = patch.pause_refresh {
            self.pause_refresh = v;
        }
        if let Some(v) = patch.providers {
            self.providers = v;
        }
        if let Some(v) = patch.display {
            self.display = v;
        }
        if let Some(v) = patch.debug {
            self.debug = v;
        }
        if let Some(v) = patch.app_language {
            self.app_language = v;
        }
        self
    }
}

const fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_phase_1_baseline() {
        let s = Settings::default();
        assert_eq!(s.schema_version, 1);
        assert_eq!(s.refresh_frequency, RefreshFrequency::FiveMinutes);
        assert!(!s.pause_refresh);
        assert!(s.providers.is_empty());
        assert!(s.app_language.is_none());
    }

    #[test]
    fn refresh_frequency_durations() {
        assert!(RefreshFrequency::Manual.as_duration().is_none());
        assert_eq!(
            RefreshFrequency::FiveMinutes
                .as_duration()
                .unwrap()
                .as_secs(),
            300
        );
    }

    #[test]
    fn patch_applies_only_present_fields() {
        let original = Settings::default();
        let patched = original.clone().apply_patch(SettingsPatch {
            refresh_frequency: Some(RefreshFrequency::OneMinute),
            ..Default::default()
        });
        assert_eq!(patched.refresh_frequency, RefreshFrequency::OneMinute);
        assert_eq!(patched.pause_refresh, original.pause_refresh);
    }

    #[test]
    fn patch_can_clear_app_language() {
        let mut s = Settings::default();
        s.app_language = Some("pt-BR".into());
        let cleared = s.apply_patch(SettingsPatch {
            app_language: Some(None),
            ..Default::default()
        });
        assert_eq!(cleared.app_language, None);
    }

    #[test]
    fn round_trip_serde_default() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}
