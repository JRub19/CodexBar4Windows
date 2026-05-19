use std::sync::Mutex;

use codexbar::core::usage_store::ProviderSlot;
use codexbar::core::UsageStore;
use codexbar::providers::models::rate_window::NamedRateWindow;
use codexbar::providers::{ProviderCatalog, ProviderDescriptor, REGISTRY};
use codexbar::renderer::{Color, IconStyle, IncidentSeverity, TooltipInputs};
use codexbar::settings::Settings;
use tauri::AppHandle;
use tracing::info;

use crate::tray_renderer::{self, TrayRenderInputs};

#[derive(Default)]
pub struct ActiveTrayProviderState {
    active: Mutex<Option<String>>,
}

impl ActiveTrayProviderState {
    pub fn set(&self, provider_id: String) {
        *self.active.lock().expect("active tray provider poisoned") = Some(provider_id);
    }

    pub fn get(&self) -> Option<String> {
        self.active
            .lock()
            .expect("active tray provider poisoned")
            .clone()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrayUsageSelection {
    pub provider_id: String,
    pub provider_name: String,
    pub inputs: TrayRenderInputs,
    pub tooltip: String,
}

pub fn update_tray_from_state(
    app: &AppHandle,
    active: &ActiveTrayProviderState,
    settings: &Settings,
    usage: &UsageStore,
) -> Result<(), String> {
    match select_tray_usage(&REGISTRY, settings, usage, active.get().as_deref()) {
        Some(selection) => {
            tray_renderer::update_tray_icon(app, selection.inputs)?;
            set_tray_tooltip(app, &selection.tooltip)?;
            info!(
                target: "codexbar::tray",
                provider = %selection.provider_id,
                primary = ?selection.inputs.primary,
                weekly = ?selection.inputs.weekly,
                "tray.usage_icon_updated",
            );
            Ok(())
        }
        None => {
            tray_renderer::update_tray_icon(app, TrayRenderInputs::default())?;
            set_tray_tooltip(
                app,
                &TooltipInputs::new("CodexBar4Windows")
                    .with_line("AI coding limits in your Windows tray")
                    .build(),
            )
        }
    }
}

pub fn select_tray_usage(
    catalog: &ProviderCatalog,
    settings: &Settings,
    usage: &UsageStore,
    active_provider: Option<&str>,
) -> Option<TrayUsageSelection> {
    let ordered = ordered_enabled_descriptors(catalog, settings);
    let mut candidates = Vec::new();
    if let Some(active) = active_provider {
        if let Some(descriptor) = ordered.iter().copied().find(|d| d.id.as_str() == active) {
            candidates.push(descriptor);
        }
    }
    for descriptor in ordered {
        if !candidates.iter().any(|d| d.id == descriptor.id) {
            candidates.push(descriptor);
        }
    }

    candidates.into_iter().find_map(|descriptor| {
        let slot = usage.slot(descriptor.id)?;
        let inputs = tray_inputs_from_slot(descriptor, &slot)?;
        let tooltip = tooltip_for_usage(descriptor, inputs);
        Some(TrayUsageSelection {
            provider_id: descriptor.id.as_str().to_string(),
            provider_name: descriptor.metadata.display_name.to_string(),
            inputs,
            tooltip,
        })
    })
}

pub fn tray_inputs_from_slot(
    descriptor: &ProviderDescriptor,
    slot: &ProviderSlot,
) -> Option<TrayRenderInputs> {
    let primary = slot
        .snapshot
        .primary()
        .and_then(remaining_percent_for_window);
    let weekly = slot
        .snapshot
        .secondary()
        .and_then(remaining_percent_for_window);
    if primary.is_none() && weekly.is_none() {
        return None;
    }
    Some(TrayRenderInputs {
        primary,
        weekly,
        credits_ratio: None,
        stale: false,
        style: icon_style_for_descriptor(descriptor),
        indicator: IncidentSeverity::Operational,
        fg: Color::WHITE,
    })
}

pub fn remaining_percent_for_window(window: &NamedRateWindow) -> Option<f32> {
    let total = window.window.allotted?;
    if total <= 0.0 || !total.is_finite() || !window.window.used.is_finite() {
        return None;
    }
    let remaining = ((total - window.window.used) / total) * 100.0;
    Some(remaining.clamp(0.0, 100.0) as f32)
}

pub fn provider_exists(provider_id: &str) -> bool {
    REGISTRY
        .descriptors()
        .any(|descriptor| descriptor.id.as_str() == provider_id)
}

pub fn first_enabled_provider_id(settings: &Settings) -> Option<String> {
    ordered_enabled_descriptors(&REGISTRY, settings)
        .first()
        .map(|descriptor| descriptor.id.as_str().to_string())
}

fn ordered_enabled_descriptors<'a>(
    catalog: &'a ProviderCatalog,
    settings: &Settings,
) -> Vec<&'a ProviderDescriptor> {
    if settings.providers.is_empty() {
        return catalog.descriptors().collect();
    }

    let mut toggles = settings.providers.clone();
    toggles.sort_by_key(|toggle| toggle.order);
    toggles
        .iter()
        .filter(|toggle| toggle.enabled)
        .filter_map(|toggle| {
            catalog
                .descriptors()
                .find(|descriptor| descriptor.id.as_str() == toggle.id)
        })
        .collect()
}

fn tooltip_for_usage(descriptor: &ProviderDescriptor, inputs: TrayRenderInputs) -> String {
    let mut builder = TooltipInputs::new(format!(
        "CodexBar4Windows - {}",
        descriptor.metadata.display_name
    ));
    if let Some(primary) = inputs.primary {
        builder = builder.with_line(format!(
            "{} remaining: {}%",
            descriptor.metadata.session_label,
            primary.round() as u32
        ));
    }
    if let Some(weekly) = inputs.weekly {
        builder = builder.with_line(format!(
            "{} remaining: {}%",
            descriptor.metadata.weekly_label,
            weekly.round() as u32
        ));
    }
    builder.build()
}

fn set_tray_tooltip(app: &AppHandle, tooltip: &str) -> Result<(), String> {
    let tray = app
        .tray_by_id("main")
        .ok_or_else(|| "tray 'main' not registered".to_string())?;
    tray.set_tooltip(Some(tooltip)).map_err(|e| e.to_string())
}

fn icon_style_for_descriptor(descriptor: &ProviderDescriptor) -> IconStyle {
    match descriptor.id.as_str() {
        "claude" => IconStyle::Claude,
        "codex" => IconStyle::Codex,
        "gemini" => IconStyle::Gemini,
        "factory" => IconStyle::Factory,
        _ => IconStyle::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codexbar::core::ProviderId;
    use codexbar::core::UsageStore;
    use codexbar::providers::identity::ProviderIdentitySnapshot;
    use codexbar::providers::models::rate_window::{NamedRateWindow, RateWindow};
    use codexbar::providers::models::UsageSnapshot;
    use codexbar::settings::{ProviderToggle, Settings};

    fn window(key: &str, used: f64, allotted: Option<f64>) -> NamedRateWindow {
        NamedRateWindow {
            key: key.into(),
            window: RateWindow {
                label: key.into(),
                used,
                allotted,
                reset_at_unix_secs: None,
                pace_delta_percent: None,
            },
        }
    }

    fn insert_snapshot(store: &UsageStore, provider: ProviderId, windows: Vec<NamedRateWindow>) {
        let snap = UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(provider, "acct"),
            windows,
            credits: None,
            cost: None,
            account_display_name: None,
            account_email: None,
            plan_name: None,
            captured_at_unix_secs: 1,
        };
        store.replace_snapshot(provider, snap, vec![]).unwrap();
    }

    #[test]
    fn snapshot_windows_become_remaining_percent_inputs() {
        let store = UsageStore::new();
        insert_snapshot(
            &store,
            ProviderId("codex"),
            vec![
                window("session", 20.0, Some(100.0)),
                window("week", 10.0, Some(100.0)),
            ],
        );
        let descriptor = REGISTRY.get(ProviderId("codex")).unwrap();
        let slot = store.slot(ProviderId("codex")).unwrap();
        let inputs = tray_inputs_from_slot(descriptor, &slot).unwrap();
        assert_eq!(inputs.primary, Some(80.0));
        assert_eq!(inputs.weekly, Some(90.0));
    }

    #[test]
    fn missing_weekly_window_produces_primary_only() {
        let store = UsageStore::new();
        insert_snapshot(
            &store,
            ProviderId("codex"),
            vec![window("session", 20.0, Some(100.0))],
        );
        let descriptor = REGISTRY.get(ProviderId("codex")).unwrap();
        let slot = store.slot(ProviderId("codex")).unwrap();
        let inputs = tray_inputs_from_slot(descriptor, &slot).unwrap();
        assert_eq!(inputs.primary, Some(80.0));
        assert_eq!(inputs.weekly, None);
    }

    #[test]
    fn missing_or_zero_allotted_omits_bars() {
        assert_eq!(
            remaining_percent_for_window(&window("session", 1.0, None)),
            None
        );
        assert_eq!(
            remaining_percent_for_window(&window("session", 1.0, Some(0.0))),
            None
        );
    }

    #[test]
    fn remaining_values_are_clamped() {
        assert_eq!(
            remaining_percent_for_window(&window("session", 120.0, Some(100.0))),
            Some(0.0)
        );
        assert_eq!(
            remaining_percent_for_window(&window("session", -20.0, Some(100.0))),
            Some(100.0)
        );
    }

    #[test]
    fn fallback_prefers_active_then_first_enabled_with_data() {
        let store = UsageStore::new();
        insert_snapshot(
            &store,
            ProviderId("claude"),
            vec![window("session", 50.0, Some(100.0))],
        );
        insert_snapshot(
            &store,
            ProviderId("codex"),
            vec![window("session", 20.0, Some(100.0))],
        );
        let selection =
            select_tray_usage(&REGISTRY, &Settings::default(), &store, Some("codex")).unwrap();
        assert_eq!(selection.provider_id, "codex");

        let settings = Settings {
            providers: vec![
                ProviderToggle {
                    id: "claude".into(),
                    enabled: true,
                    order: 0,
                },
                ProviderToggle {
                    id: "codex".into(),
                    enabled: true,
                    order: 1,
                },
            ],
            ..Default::default()
        };
        let selection = select_tray_usage(&REGISTRY, &settings, &store, Some("missing")).unwrap();
        assert_eq!(selection.provider_id, "claude");
    }

    #[test]
    fn disabled_active_provider_falls_back_to_first_enabled_with_data() {
        let store = UsageStore::new();
        insert_snapshot(
            &store,
            ProviderId("claude"),
            vec![window("session", 50.0, Some(100.0))],
        );
        insert_snapshot(
            &store,
            ProviderId("codex"),
            vec![window("session", 20.0, Some(100.0))],
        );
        let settings = Settings {
            providers: vec![
                ProviderToggle {
                    id: "claude".into(),
                    enabled: true,
                    order: 0,
                },
                ProviderToggle {
                    id: "codex".into(),
                    enabled: false,
                    order: 1,
                },
            ],
            ..Default::default()
        };
        let selection = select_tray_usage(&REGISTRY, &settings, &store, Some("codex")).unwrap();
        assert_eq!(selection.provider_id, "claude");
    }

    #[test]
    fn provider_validation_rejects_unknown_ids() {
        assert!(provider_exists("codex"));
        assert!(!provider_exists("not-a-provider"));
    }
}
