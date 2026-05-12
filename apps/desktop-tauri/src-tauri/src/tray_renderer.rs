//! Bridges the [`codexbar::renderer`] module to the Tauri tray icon.
//!
//! `render_initial_icon` produces a placeholder 50 percent bar so the
//! tray shows something coherent before the first real refresh.
//! `update_tray_icon` accepts a `TrayRenderInputs` struct and updates
//! `tray_by_id("main")` via `Shell_NotifyIcon` (Tauri 2's tray-icon
//! crate handles the `NIM_MODIFY` syscall internally).
//!
//! Phase 4 (Claude) calls `update_tray_icon` whenever the refresh loop
//! folds a new `UsageState` snapshot.

use codexbar::renderer::{
    bar_alphas_for, draw_bar, paint_overlay, paint_twist, BarAlphas, BarRect, Color, IconRenderer,
    IconStyle, IncidentSeverity,
};
use tauri::image::Image;
use tauri::AppHandle;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrayRenderInputs {
    pub primary: Option<f32>,
    pub weekly: Option<f32>,
    pub credits_ratio: Option<f32>,
    pub stale: bool,
    pub style: IconStyle,
    pub indicator: IncidentSeverity,
    pub fg: Color,
}

impl Default for TrayRenderInputs {
    fn default() -> Self {
        Self {
            primary: Some(50.0),
            weekly: Some(40.0),
            credits_ratio: None,
            stale: false,
            style: IconStyle::Default,
            indicator: IncidentSeverity::Operational,
            fg: Color::WHITE,
        }
    }
}

/// Build a fresh tray icon image (.ico bytes wrapped in `tauri::Image`).
pub fn render_icon(inputs: TrayRenderInputs) -> Result<Image<'static>, String> {
    let mut renderer = IconRenderer::new();
    renderer.clear();
    let alphas: BarAlphas = bar_alphas_for(inputs.stale);

    if let Some(primary) = inputs.primary {
        draw_bar(
            renderer.pixmap_mut(),
            BarRect {
                x: 3,
                y: 3,
                w: 30,
                h: 12,
            },
            primary,
            inputs.style,
            alphas,
            inputs.fg,
        );
    }
    if let Some(weekly) = inputs.weekly {
        draw_bar(
            renderer.pixmap_mut(),
            BarRect {
                x: 3,
                y: 19,
                w: 30,
                h: 12,
            },
            weekly,
            inputs.style,
            alphas,
            inputs.fg,
        );
    }
    paint_twist(renderer.pixmap_mut(), inputs.style, inputs.fg);
    paint_overlay(renderer.pixmap_mut(), inputs.indicator, inputs.fg);

    let rgba: Vec<u8> = renderer.rgba().to_vec();
    Ok(Image::new_owned(rgba, renderer.width(), renderer.height()))
}

/// Update the tray icon image registered as `main`. Returns `Err` when
/// the tray is not yet built or the ICO encoding fails.
pub fn update_tray_icon(app: &AppHandle, inputs: TrayRenderInputs) -> Result<(), String> {
    let tray = app
        .tray_by_id("main")
        .ok_or_else(|| "tray 'main' not registered".to_string())?;
    let image = render_icon(inputs)?;
    tray.set_icon(Some(image)).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_icon_returns_valid_image() {
        let img = render_icon(TrayRenderInputs::default()).expect("render");
        assert!(img.width() > 0);
        assert!(img.height() > 0);
    }

    #[test]
    fn stale_inputs_still_render() {
        let img = render_icon(TrayRenderInputs {
            stale: true,
            ..Default::default()
        })
        .expect("render");
        assert!(img.width() > 0);
    }

    #[test]
    fn brand_style_renders_with_twist() {
        let img = render_icon(TrayRenderInputs {
            style: IconStyle::Codex,
            ..Default::default()
        })
        .expect("render");
        assert!(img.width() > 0);
    }
}
