//! Settings data model. Phase 1 ships a minimum subset of the macOS schema.
//! Later phases extend the structs in place. New fields must default safely
//! so older `config.json` files load without intervention.

use std::collections::BTreeMap;

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
    #[serde(default = "default_allow_browser_cookie_import")]
    pub allow_browser_cookie_import: bool,
    #[serde(default)]
    pub app_language: Option<String>,
    /// Per-provider key/value bag for picker + text-field settings
    /// (e.g. `moonshot.region`, `zai.api_host`, `copilot.enterprise_host`).
    /// Stored as plain strings so the settings descriptors and the
    /// React side can stay agnostic about value types.
    #[serde(default)]
    pub provider_kv: BTreeMap<String, String>,
    /// Master toggle for desktop toasts (Phase 7C). Default ON; users
    /// can disable from the Notifications pane.
    #[serde(default = "default_notifications_enabled")]
    pub notifications_enabled: bool,
    /// Chord that toggles the popup window. Stored in a human-readable
    /// form like `"Win+Shift+U"` so the Shortcuts pane can render it
    /// verbatim. `None` means "use the platform default" (`Win+Shift+U`
    /// on Windows).
    #[serde(default)]
    pub popup_toggle_hotkey: Option<String>,
}

const fn default_notifications_enabled() -> bool {
    true
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
            allow_browser_cookie_import: true,
            app_language: None,
            provider_kv: BTreeMap::new(),
            notifications_enabled: true,
            popup_toggle_hotkey: None,
        }
    }
}

const fn default_allow_browser_cookie_import() -> bool {
    true
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshFrequency {
    Manual,
    OneMinute,
    TwoMinutes,
    #[default]
    FiveMinutes,
    FifteenMinutes,
    ThirtyMinutes,
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
    /// When `true`, the secrets subsystem refuses to persist new blobs.
    /// Existing blobs remain readable. Used by power users debugging
    /// credential storage issues without losing live state.
    #[serde(default)]
    pub disable_secret_storage: bool,
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
    pub allow_browser_cookie_import: Option<bool>,
    pub app_language: Option<Option<String>>,
    /// Merge entries into `Settings.provider_kv`. Map keys that map
    /// to an empty string are removed; non-empty values are inserted
    /// or overwritten. Use `Option<None>` instead of an empty map to
    /// signal "leave unchanged".
    pub provider_kv: Option<BTreeMap<String, String>>,
    pub notifications_enabled: Option<bool>,
    /// Outer `Option` = present-in-patch; inner `Option<String>` =
    /// chord string, where `None` clears the override and restores the
    /// platform default.
    pub popup_toggle_hotkey: Option<Option<String>>,
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
        if let Some(v) = patch.allow_browser_cookie_import {
            self.allow_browser_cookie_import = v;
        }
        if let Some(v) = patch.app_language {
            self.app_language = v;
        }
        if let Some(kv) = patch.provider_kv {
            for (key, value) in kv {
                if value.is_empty() {
                    self.provider_kv.remove(&key);
                } else {
                    self.provider_kv.insert(key, value);
                }
            }
        }
        if let Some(v) = patch.notifications_enabled {
            self.notifications_enabled = v;
        }
        if let Some(v) = patch.popup_toggle_hotkey {
            self.popup_toggle_hotkey = v;
        }
        self
    }

    /// Convenience: read a single provider-kv entry.
    pub fn provider_kv_get(&self, key: &str) -> Option<&str> {
        self.provider_kv.get(key).map(String::as_str)
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
        let s = Settings {
            app_language: Some("pt-BR".into()),
            ..Default::default()
        };
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

    #[test]
    fn provider_kv_patch_inserts_and_removes_entries() {
        let mut s = Settings::default();
        s.provider_kv.insert("moonshot.region".into(), "china".into());
        // Patch sets one key, clears another (empty value).
        let patched = s.apply_patch(SettingsPatch {
            provider_kv: Some(
                [
                    ("zai.api_host".to_string(), "zai.example.com".to_string()),
                    ("moonshot.region".to_string(), String::new()),
                ]
                .into_iter()
                .collect(),
            ),
            ..Default::default()
        });
        assert_eq!(
            patched.provider_kv_get("zai.api_host"),
            Some("zai.example.com")
        );
        assert!(patched.provider_kv_get("moonshot.region").is_none());
    }

    #[test]
    fn provider_kv_round_trips_through_serde() {
        let mut s = Settings::default();
        s.provider_kv.insert("zai.region".into(), "global".into());
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.provider_kv_get("zai.region"), Some("global"));
    }
}
