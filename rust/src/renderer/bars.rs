//! Capsule bar rendering: track plus stroke plus fill, left anchored.
//!
//! Per `docs/windows/spec/10-tray-icon-system.md` section 2:
//!
//! - Track at `labelColor * track_alpha` (default 0.28).
//! - Stroke inset 1 px on every side at `* stroke_alpha` (default 0.44).
//! - Fill, left anchored, width `round(rect.w * value / 100.0)`, alpha
//!   `fill_alpha` (default 1.0). Right edge hard, no anti alias.
//! - Corner radius determined by [`IconStyle`] (capsule, 0 for Claude,
//!   3 for Warp).
//!
//! `alpha_table` lets `stale` and other modes swap the three alphas in
//! lockstep without touching this module.

use tiny_skia::{Color, FillRule, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform};

use super::style::IconStyle;

/// Default alpha values for the three layers of a bar. The stale mode
/// (task A7) supplies a different table to dim them in lockstep.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarAlphas {
    pub track: f32,
    pub stroke: f32,
    pub fill: f32,
}

impl Default for BarAlphas {
    fn default() -> Self {
        Self {
            track: 0.28,
            stroke: 0.44,
            fill: 1.0,
        }
    }
}

/// Where the bar sits in the 36 by 36 canvas, in pixel coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl BarRect {
    /// Fill width for the given value (0 to 100 inclusive). Mac uses
    /// round half away from zero; Rust's `round()` matches.
    pub fn fill_width(&self, value: f32) -> u32 {
        let clamped = value.clamp(0.0, 100.0);
        let w = (self.w as f32) * clamped / 100.0;
        w.round() as u32
    }
}

/// Render one bar. `fg` is the resolved foreground color (dark on light
/// taskbars, light on dark taskbars). `style` selects the corner radius.
pub fn draw_bar(
    pixmap: &mut Pixmap,
    rect: BarRect,
    value: f32,
    style: IconStyle,
    alphas: BarAlphas,
    fg: Color,
) {
    let r = style.bar_corner_radius(rect.h);
    let rect_path = capsule_path(rect, r);

    // Track.
    let mut paint = Paint {
        anti_alias: style.antialias(),
        ..Default::default()
    };
    paint.set_color(scale_alpha(fg, alphas.track));
    pixmap.fill_path(
        &rect_path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    // Stroke inset 1 px.
    if rect.w >= 2 && rect.h >= 2 {
        let inner = BarRect {
            x: rect.x + 1,
            y: rect.y + 1,
            w: rect.w - 2,
            h: rect.h - 2,
        };
        let stroke_path = capsule_path(inner, (r - 1.0).max(0.0));
        let mut stroke_paint = Paint {
            anti_alias: style.antialias(),
            ..Default::default()
        };
        stroke_paint.set_color(scale_alpha(fg, alphas.stroke));
        let stroke = Stroke {
            width: 1.0,
            ..Stroke::default()
        };
        pixmap.stroke_path(
            &stroke_path,
            &stroke_paint,
            &stroke,
            Transform::identity(),
            None,
        );
    }

    // Fill, left anchored, hard right edge (anti alias off so the right
    // edge never bleeds into the track).
    let fill_w = rect.fill_width(value);
    if fill_w > 0 {
        let mut fill_paint = Paint {
            anti_alias: false,
            ..Default::default()
        };
        fill_paint.set_color(scale_alpha(fg, alphas.fill));
        let fill_rect = BarRect {
            x: rect.x,
            y: rect.y,
            w: fill_w,
            h: rect.h,
        };
        // Clip the fill to the capsule so it does not poke past the
        // rounded ends.
        let fill_path = clipped_fill_path(fill_rect, rect, r);
        pixmap.fill_path(
            &fill_path,
            &fill_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn capsule_path(rect: BarRect, radius: f32) -> Path {
    let x = rect.x as f32;
    let y = rect.y as f32;
    let w = rect.w as f32;
    let h = rect.h as f32;
    if radius <= 0.0 {
        let mut pb = PathBuilder::new();
        pb.push_rect(Rect::from_xywh(x, y, w, h).expect("valid rect"));
        return pb.finish().expect("valid path");
    }
    let r = radius.min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish().expect("valid capsule path")
}

fn clipped_fill_path(fill: BarRect, parent: BarRect, parent_radius: f32) -> Path {
    // Intersect the fill rect with the parent capsule by clipping the
    // fill to a capsule shape. Easiest is to draw a capsule sized
    // (fill.w, parent.h) anchored at fill origin; if the fill ends inside
    // the parent, the right edge stays square (no aa), which matches the
    // Mac source.
    if fill.w >= parent.w {
        return capsule_path(parent, parent_radius);
    }
    let r = parent_radius.min(parent.h as f32 / 2.0);
    let x = fill.x as f32;
    let y = fill.y as f32;
    let w = fill.w as f32;
    let h = fill.h as f32;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w, y);
    pb.line_to(x + w, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish().expect("valid clipped fill path")
}

fn scale_alpha(c: Color, mul: f32) -> Color {
    let a = (c.alpha() * mul).clamp(0.0, 1.0);
    Color::from_rgba(c.red(), c.green(), c.blue(), a).expect("valid color")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn black() -> Color {
        Color::BLACK
    }

    fn sample_alpha(pixmap: &Pixmap, x: u32, y: u32) -> u8 {
        let i = ((y * pixmap.width() + x) * 4 + 3) as usize;
        pixmap.data()[i]
    }

    #[test]
    fn fill_width_rounds_to_nearest_pixel() {
        let rect = BarRect {
            x: 3,
            y: 19,
            w: 30,
            h: 12,
        };
        assert_eq!(rect.fill_width(0.0), 0);
        assert_eq!(rect.fill_width(50.0), 15);
        assert_eq!(rect.fill_width(100.0), 30);
        assert_eq!(rect.fill_width(33.0), 10); // 9.9 rounds to 10
    }

    #[test]
    fn fifty_percent_fill_paints_left_half_dark() {
        let mut pixmap = Pixmap::new(36, 36).expect("pixmap");
        draw_bar(
            &mut pixmap,
            BarRect {
                x: 3,
                y: 19,
                w: 30,
                h: 12,
            },
            50.0,
            IconStyle::Default,
            BarAlphas::default(),
            black(),
        );
        // Pixel inside the fill region (well inside the left half).
        let left_a = sample_alpha(&pixmap, 8, 24);
        assert!(
            left_a > 200,
            "expected near opaque fill at (8,24), got {}",
            left_a
        );
        // Pixel just past the fill, inside the track but not fill.
        let mid_a = sample_alpha(&pixmap, 22, 24);
        assert!(
            mid_a < 200,
            "expected track alpha at (22,24), got {}",
            mid_a
        );
    }

    #[test]
    fn zero_value_renders_only_track_and_stroke() {
        let mut pixmap = Pixmap::new(36, 36).expect("pixmap");
        draw_bar(
            &mut pixmap,
            BarRect {
                x: 3,
                y: 19,
                w: 30,
                h: 12,
            },
            0.0,
            IconStyle::Default,
            BarAlphas::default(),
            black(),
        );
        for x in 4..32 {
            let a = sample_alpha(&pixmap, x, 24);
            assert!(a < 200, "no fill expected at value=0 (x={}, a={})", x, a);
        }
    }

    #[test]
    fn one_hundred_fills_the_entire_bar() {
        let mut pixmap = Pixmap::new(36, 36).expect("pixmap");
        draw_bar(
            &mut pixmap,
            BarRect {
                x: 3,
                y: 19,
                w: 30,
                h: 12,
            },
            100.0,
            IconStyle::Default,
            BarAlphas::default(),
            black(),
        );
        // Pixel near the right edge of the bar should be filled.
        let right_a = sample_alpha(&pixmap, 30, 24);
        assert!(
            right_a > 180,
            "expected fill at right edge for value=100, got {}",
            right_a
        );
    }

    #[test]
    fn claude_style_has_zero_radius_and_hits_corner() {
        let mut pixmap = Pixmap::new(36, 36).expect("pixmap");
        draw_bar(
            &mut pixmap,
            BarRect {
                x: 0,
                y: 0,
                w: 36,
                h: 12,
            },
            100.0,
            IconStyle::Claude,
            BarAlphas::default(),
            black(),
        );
        // Square corner: (0,0) should be inside the bar.
        let corner = sample_alpha(&pixmap, 0, 0);
        assert!(corner > 200, "claude corner should be inside the bar");
    }
}
