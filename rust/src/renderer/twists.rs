//! Provider twist overlays.
//!
//! Per `docs/windows/spec/10-tray-icon-system.md` section 2.6, five styles
//! ship a brand glyph overlay on top of the bar layout:
//!
//! - Codex: 4 by 4 eye and 18 by 4 hat. Blocky, antialias off.
//! - Claude: 3 px wide arms running the full bar height. Blocky, AA off.
//! - Gemini: 8 point star, outer radius 4, inner radius 1. Organic, AA on.
//! - Factory: 16 point asterisk, outer 3.5, inner 1.05. Organic, AA on.
//! - Warp: a pair of 5 by 8 ellipses tilted +/- pi/3. Organic, AA on.
//!
//! Each function paints over the bar layer with the resolved foreground
//! color. Codex's eye is cleared via `BlendMode::Clear` so the bar fill
//! cannot z fight the eye.

use tiny_skia::{BlendMode, Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

use super::style::IconStyle;

/// Top level dispatcher. Calls the per style routine and returns true if
/// any pixels were drawn.
pub fn paint_twist(pixmap: &mut Pixmap, style: IconStyle, fg: Color) -> bool {
    match style {
        IconStyle::Codex => {
            codex(pixmap, fg);
            true
        }
        IconStyle::Claude => {
            claude(pixmap, fg);
            true
        }
        IconStyle::Gemini => {
            gemini(pixmap, fg);
            true
        }
        IconStyle::Factory => {
            factory(pixmap, fg);
            true
        }
        IconStyle::Warp => {
            warp(pixmap, fg);
            true
        }
        IconStyle::Default => false,
    }
}

fn codex(pixmap: &mut Pixmap, fg: Color) {
    // 18 px wide hat centered on x=9 (so x=0..18, y=2..6, h=4).
    let mut hat = Paint {
        anti_alias: false,
        ..Default::default()
    };
    hat.set_color(fg);
    if let Some(rect) = Rect::from_xywh(9.0, 2.0, 18.0, 4.0) {
        let mut pb = PathBuilder::new();
        pb.push_rect(rect);
        if let Some(path) = pb.finish() {
            pixmap.fill_path(&path, &hat, FillRule::Winding, Transform::identity(), None);
        }
    }
    // 4 by 4 eye, blended out via Clear so the bar fill underneath does
    // not show through.
    let mut eye = Paint {
        anti_alias: false,
        blend_mode: BlendMode::Clear,
        ..Default::default()
    };
    eye.set_color(Color::WHITE);
    if let Some(rect) = Rect::from_xywh(15.0, 9.0, 4.0, 4.0) {
        let mut pb = PathBuilder::new();
        pb.push_rect(rect);
        if let Some(path) = pb.finish() {
            pixmap.fill_path(&path, &eye, FillRule::Winding, Transform::identity(), None);
        }
    }
}

fn claude(pixmap: &mut Pixmap, fg: Color) {
    // Two 3 px wide arms at x=2..5 and x=31..34, y=3..33 (h-6 of 36).
    let mut paint = Paint {
        anti_alias: false,
        ..Default::default()
    };
    paint.set_color(fg);
    for x in [2.0_f32, 31.0_f32] {
        if let Some(rect) = Rect::from_xywh(x, 3.0, 3.0, 30.0) {
            let mut pb = PathBuilder::new();
            pb.push_rect(rect);
            if let Some(path) = pb.finish() {
                pixmap.fill_path(
                    &path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    }
}

fn gemini(pixmap: &mut Pixmap, fg: Color) {
    // 8 point star centered at (18, 18). Outer radius 4, inner 1.
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color(fg);
    let path = star_path(18.0, 18.0, 4.0, 1.0, 8);
    if let Some(path) = path {
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn factory(pixmap: &mut Pixmap, fg: Color) {
    // 16 point asterisk centered at (18, 18). Outer radius 3.5, inner 1.05.
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color(fg);
    if let Some(path) = star_path(18.0, 18.0, 3.5, 1.05, 16) {
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn warp(pixmap: &mut Pixmap, fg: Color) {
    // Two 5 by 8 ellipses centered at (12, 18) and (24, 18), rotated +/- pi/3.
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color(fg);
    for (cx, sign) in [(12.0_f32, -1.0_f32), (24.0_f32, 1.0_f32)] {
        let transform = Transform::from_rotate_at(sign * 60.0, cx, 18.0);
        let path = ellipse_path(cx, 18.0, 5.0, 8.0);
        if let Some(path) = path {
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }
    }
}

fn star_path(cx: f32, cy: f32, outer: f32, inner: f32, points: u32) -> Option<tiny_skia::Path> {
    if points < 2 {
        return None;
    }
    let count = points * 2;
    let step = std::f32::consts::TAU / count as f32;
    let mut pb = PathBuilder::new();
    for i in 0..count {
        let theta = i as f32 * step - std::f32::consts::FRAC_PI_2;
        let r = if i.is_multiple_of(2) { outer } else { inner };
        let x = cx + r * theta.cos();
        let y = cy + r * theta.sin();
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    pb.close();
    pb.finish()
}

fn ellipse_path(cx: f32, cy: f32, rx: f32, ry: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.push_oval(Rect::from_xywh(cx - rx, cy - ry, rx * 2.0, ry * 2.0)?);
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_pixmap() -> Pixmap {
        Pixmap::new(36, 36).expect("36x36 pixmap")
    }

    fn any_pixel_painted(pixmap: &Pixmap) -> bool {
        pixmap.data().chunks_exact(4).any(|c| c[3] > 0)
    }

    #[test]
    fn default_style_paints_nothing() {
        let mut pixmap = fresh_pixmap();
        assert!(!paint_twist(&mut pixmap, IconStyle::Default, Color::BLACK));
        assert!(!any_pixel_painted(&pixmap));
    }

    #[test]
    fn codex_paints_a_hat_at_top() {
        let mut pixmap = fresh_pixmap();
        assert!(paint_twist(&mut pixmap, IconStyle::Codex, Color::BLACK));
        // Hat row should have pixels at y=3.
        let row3_alpha = pixmap.data()[((3 * 36 + 12) * 4 + 3) as usize];
        assert!(row3_alpha > 200, "expected hat fill at (12,3)");
    }

    #[test]
    fn claude_paints_both_arms() {
        let mut pixmap = fresh_pixmap();
        assert!(paint_twist(&mut pixmap, IconStyle::Claude, Color::BLACK));
        let left = pixmap.data()[((10 * 36 + 3) * 4 + 3) as usize];
        let right = pixmap.data()[((10 * 36 + 32) * 4 + 3) as usize];
        assert!(left > 200, "left arm");
        assert!(right > 200, "right arm");
    }

    #[test]
    fn gemini_paints_a_centered_star() {
        let mut pixmap = fresh_pixmap();
        assert!(paint_twist(&mut pixmap, IconStyle::Gemini, Color::BLACK));
        // Center pixel of the star should be painted.
        let center = pixmap.data()[((18 * 36 + 18) * 4 + 3) as usize];
        assert!(center > 0);
    }

    #[test]
    fn factory_paints_around_center() {
        let mut pixmap = fresh_pixmap();
        assert!(paint_twist(&mut pixmap, IconStyle::Factory, Color::BLACK));
        // Some pixel near center should be lit.
        let near_center = pixmap.data()[((18 * 36 + 18) * 4 + 3) as usize];
        assert!(near_center > 0);
    }

    #[test]
    fn warp_paints_two_offset_ellipses() {
        let mut pixmap = fresh_pixmap();
        assert!(paint_twist(&mut pixmap, IconStyle::Warp, Color::BLACK));
        let any_left = (10..15).any(|x| pixmap.data()[((18 * 36 + x) * 4 + 3) as usize] > 0);
        let any_right = (22..28).any(|x| pixmap.data()[((18 * 36 + x) * 4 + 3) as usize] > 0);
        assert!(any_left, "expected left ellipse");
        assert!(any_right, "expected right ellipse");
    }
}
