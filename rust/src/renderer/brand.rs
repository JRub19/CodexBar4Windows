//! Brand icon mode: provider logo plus a percent label baked into the
//! tray bitmap at sizes greater than or equal to 32 px.
//!
//! Phase 3.A12 scaffolds the API surface and ships a minimal
//! implementation that fills a colored rectangle (the brand color) plus
//! a percent line drawn as a simple horizontal bar. Phase 4 swaps the
//! rectangle for a real provider SVG (rendered via `resvg`) and the
//! percent line for a numeric label (rendered via a future font crate).
//!
//! The user opts into brand mode through `Settings.display.brand_icon_mode`
//! (added in phase 3 group D). Until then this module is only consumed
//! by tests.

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrandIconParams {
    /// Provider brand color in RGBA. Used as the rectangle fill.
    pub brand: Color,
    /// Percent value 0 to 100 used for the label bar at the bottom.
    pub percent: f32,
    /// When true, draw the percent indicator. False produces just the
    /// brand rectangle.
    pub show_percent: bool,
}

/// Paint the brand icon into `pixmap`. Returns true when any pixels
/// were drawn. Phase 4 will pass through the resvg pipeline; this stub
/// is here so tray host wiring can call into the renderer today.
pub fn paint_brand(pixmap: &mut Pixmap, params: BrandIconParams) -> bool {
    let w = pixmap.width() as f32;
    let h = pixmap.height() as f32;

    let mut brand_paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    brand_paint.set_color(params.brand);

    // Brand rounded rectangle filling roughly the top three quarters of
    // the canvas. Phase 4 substitutes the provider SVG.
    let brand_h = (h * 0.7).round();
    if let Some(rect) = Rect::from_xywh(2.0, 2.0, w - 4.0, brand_h - 2.0) {
        let mut pb = PathBuilder::new();
        let r = 4.0_f32.min(brand_h / 2.0);
        push_rounded_rect(&mut pb, rect, r);
        if let Some(path) = pb.finish() {
            pixmap.fill_path(
                &path,
                &brand_paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    if params.show_percent {
        // Percent label bar across the bottom. Fill proportional to
        // value. Phase 4 replaces with rasterized text.
        let bar_y = brand_h + 1.0;
        let bar_h = h - bar_y - 2.0;
        let percent = params.percent.clamp(0.0, 100.0);
        let fill_w = ((w - 4.0) * percent / 100.0).round();
        let mut label_paint = Paint {
            anti_alias: false,
            ..Default::default()
        };
        label_paint.set_color(params.brand);
        if fill_w > 0.0 {
            if let Some(rect) = Rect::from_xywh(2.0, bar_y, fill_w, bar_h) {
                let mut pb = PathBuilder::new();
                pb.push_rect(rect);
                if let Some(path) = pb.finish() {
                    pixmap.fill_path(
                        &path,
                        &label_paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }
    }
    true
}

fn push_rounded_rect(pb: &mut PathBuilder, rect: Rect, radius: f32) {
    let x = rect.x();
    let y = rect.y();
    let w = rect.width();
    let h = rect.height();
    let r = radius.min(w / 2.0).min(h / 2.0);
    if r <= 0.0 {
        pb.push_rect(rect);
        return;
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixmap() -> Pixmap {
        Pixmap::new(36, 36).expect("36x36")
    }

    fn brand_red() -> Color {
        Color::from_rgba(1.0, 0.0, 0.0, 1.0).expect("color")
    }

    fn alpha_at(p: &Pixmap, x: u32, y: u32) -> u8 {
        p.data()[((y * p.width() + x) * 4 + 3) as usize]
    }

    #[test]
    fn paints_brand_rectangle_in_upper_half() {
        let mut p = pixmap();
        let drawn = paint_brand(
            &mut p,
            BrandIconParams {
                brand: brand_red(),
                percent: 0.0,
                show_percent: false,
            },
        );
        assert!(drawn);
        // The brand fills roughly the top 70% so a pixel at y=10 should be opaque.
        assert!(alpha_at(&p, 18, 10) > 200);
    }

    #[test]
    fn percent_bar_only_drawn_when_show_percent_is_true() {
        let mut p_off = pixmap();
        paint_brand(
            &mut p_off,
            BrandIconParams {
                brand: brand_red(),
                percent: 80.0,
                show_percent: false,
            },
        );
        // Bottom region should be transparent.
        let bottom_off = alpha_at(&p_off, 4, 32);

        let mut p_on = pixmap();
        paint_brand(
            &mut p_on,
            BrandIconParams {
                brand: brand_red(),
                percent: 80.0,
                show_percent: true,
            },
        );
        let bottom_on = alpha_at(&p_on, 4, 32);
        assert!(bottom_on > bottom_off, "show_percent should add fill");
    }

    #[test]
    fn fifty_percent_label_paints_left_half_only() {
        let mut p = pixmap();
        paint_brand(
            &mut p,
            BrandIconParams {
                brand: brand_red(),
                percent: 50.0,
                show_percent: true,
            },
        );
        // Bar fills width = (w-4) * 0.5 = 16; bar starts at x=2.
        let left = alpha_at(&p, 5, 32);
        let right = alpha_at(&p, 30, 32);
        assert!(left > 200, "left half should be filled");
        assert!(right < 200, "right half should be empty");
    }

    #[test]
    fn percent_out_of_range_is_clamped() {
        let mut p = pixmap();
        paint_brand(
            &mut p,
            BrandIconParams {
                brand: brand_red(),
                percent: 200.0,
                show_percent: true,
            },
        );
        // 100% should fill the entire bottom strip.
        let right = alpha_at(&p, 30, 32);
        assert!(right > 200);
    }
}
