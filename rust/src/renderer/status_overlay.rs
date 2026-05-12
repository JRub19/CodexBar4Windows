//! Status incident overlay, painted on top of the bar layout.
//!
//! Per `docs/windows/spec/10-tray-icon-system.md` section 2.7:
//!
//! - `Minor` and `Maintenance`: filled circle 4 by 4 at `(w-6, 2)`.
//! - `Major`, `Critical`, `Unknown`: line 2 by 6 at `(w-6, 4)` plus a
//!   2 by 2 dot at `(w-6, 2)`. (Exclamation mark shape.)
//! - `Operational`: no overlay.
//!
//! Drawn with the *resolved* foreground color, never dimmed by the stale
//! alpha table.

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IncidentSeverity {
    Operational,
    Minor,
    Maintenance,
    Major,
    Critical,
    Unknown,
}

impl IncidentSeverity {
    /// True when an overlay should be drawn.
    pub fn has_overlay(self) -> bool {
        !matches!(self, Self::Operational)
    }

    fn is_dot_only(self) -> bool {
        matches!(self, Self::Minor | Self::Maintenance)
    }
}

/// Paint the overlay for `severity`. No op for `Operational`.
pub fn paint_overlay(pixmap: &mut Pixmap, severity: IncidentSeverity, fg: Color) {
    if !severity.has_overlay() {
        return;
    }
    let w = pixmap.width() as f32;
    let anchor_x = w - 6.0;

    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color(fg);

    if severity.is_dot_only() {
        // 4 by 4 filled circle (anchor 2 px above top-right corner).
        if let Some(rect) = Rect::from_xywh(anchor_x, 2.0, 4.0, 4.0) {
            let mut pb = PathBuilder::new();
            pb.push_oval(rect);
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
        return;
    }

    // Major / Critical / Unknown: 2 by 2 dot + 2 by 6 vertical line.
    let mut blocky = Paint {
        anti_alias: false,
        ..Default::default()
    };
    blocky.set_color(fg);
    if let Some(rect) = Rect::from_xywh(anchor_x, 2.0, 2.0, 2.0) {
        let mut pb = PathBuilder::new();
        pb.push_rect(rect);
        if let Some(path) = pb.finish() {
            pixmap.fill_path(
                &path,
                &blocky,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
    if let Some(rect) = Rect::from_xywh(anchor_x, 4.0, 2.0, 6.0) {
        let mut pb = PathBuilder::new();
        pb.push_rect(rect);
        if let Some(path) = pb.finish() {
            pixmap.fill_path(
                &path,
                &blocky,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Pixmap {
        Pixmap::new(36, 36).expect("36x36")
    }

    fn alpha_at(pixmap: &Pixmap, x: u32, y: u32) -> u8 {
        pixmap.data()[((y * pixmap.width() + x) * 4 + 3) as usize]
    }

    #[test]
    fn operational_paints_nothing() {
        let mut p = fresh();
        paint_overlay(&mut p, IncidentSeverity::Operational, Color::BLACK);
        assert!(p.data().chunks_exact(4).all(|c| c[3] == 0));
    }

    #[test]
    fn minor_paints_a_filled_circle_at_top_right() {
        let mut p = fresh();
        paint_overlay(&mut p, IncidentSeverity::Minor, Color::BLACK);
        // Center of the 4x4 circle: (30+2, 2+2) = (32, 4).
        assert!(alpha_at(&p, 32, 4) > 200, "expected filled circle center");
    }

    #[test]
    fn maintenance_uses_same_circle_as_minor() {
        let mut p1 = fresh();
        let mut p2 = fresh();
        paint_overlay(&mut p1, IncidentSeverity::Minor, Color::BLACK);
        paint_overlay(&mut p2, IncidentSeverity::Maintenance, Color::BLACK);
        assert_eq!(p1.data(), p2.data());
    }

    #[test]
    fn major_paints_dot_and_line() {
        let mut p = fresh();
        paint_overlay(&mut p, IncidentSeverity::Major, Color::BLACK);
        // Dot at (30, 2)..(32, 4)
        assert!(alpha_at(&p, 30, 2) > 200, "expected top dot");
        // Line at (30, 4)..(32, 10) — sample at y=7
        assert!(alpha_at(&p, 30, 7) > 200, "expected vertical line");
    }
}
