//! Compute popup window position from the tray icon rect and the
//! taskbar work area.
//!
//! Algorithm (matches spec 80 popup behavior):
//!
//! 1. Identify the taskbar edge by comparing the monitor's full bounds
//!    to its work area. The edge with the largest gap is where the
//!    taskbar lives.
//! 2. Anchor the popup on the opposite side of the tray rect from the
//!    taskbar (e.g., bottom taskbar → popup above the tray, with the
//!    popup's bottom edge touching the tray's top edge plus a 4 px gap).
//! 3. Horizontally center the popup on the tray rect, then clamp to the
//!    monitor work area with a 4 px margin so the popup never overflows
//!    the screen.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn center_x(&self) -> i32 {
        self.x + self.w / 2
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarEdge {
    Bottom,
    Top,
    Left,
    Right,
}

/// Detect the taskbar edge given monitor full bounds plus work area.
pub fn detect_edge(monitor: Rect, work_area: Rect) -> TaskbarEdge {
    let bottom_gap = monitor.bottom() - work_area.bottom();
    let top_gap = work_area.y - monitor.y;
    let right_gap = monitor.right() - work_area.right();
    let left_gap = work_area.x - monitor.x;
    let max = bottom_gap.max(top_gap).max(right_gap).max(left_gap);
    if max == bottom_gap {
        TaskbarEdge::Bottom
    } else if max == top_gap {
        TaskbarEdge::Top
    } else if max == right_gap {
        TaskbarEdge::Right
    } else {
        TaskbarEdge::Left
    }
}

pub const POPUP_GAP_PX: i32 = 4;
pub const POPUP_MARGIN_PX: i32 = 4;

/// Compute the top-left position of the popup.
pub fn compute(
    tray: Rect,
    popup_w: i32,
    popup_h: i32,
    work_area: Rect,
    edge: TaskbarEdge,
) -> (i32, i32) {
    let (mut x, mut y) = match edge {
        TaskbarEdge::Bottom => (
            tray.center_x() - popup_w / 2,
            tray.y - popup_h - POPUP_GAP_PX,
        ),
        TaskbarEdge::Top => (tray.center_x() - popup_w / 2, tray.bottom() + POPUP_GAP_PX),
        TaskbarEdge::Right => (
            tray.x - popup_w - POPUP_GAP_PX,
            tray.y + tray.h / 2 - popup_h / 2,
        ),
        TaskbarEdge::Left => (
            tray.right() + POPUP_GAP_PX,
            tray.y + tray.h / 2 - popup_h / 2,
        ),
    };
    // Clamp into the work area with a margin.
    let min_x = work_area.x + POPUP_MARGIN_PX;
    let max_x = work_area.right() - popup_w - POPUP_MARGIN_PX;
    let min_y = work_area.y + POPUP_MARGIN_PX;
    let max_y = work_area.bottom() - popup_h - POPUP_MARGIN_PX;
    x = x.clamp(min_x.min(max_x), max_x.max(min_x));
    y = y.clamp(min_y.min(max_y), max_y.max(min_y));
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_taskbar_anchors_popup_above_tray() {
        let monitor = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let work_area = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        }; // 40 px taskbar at bottom
        let tray = Rect {
            x: 1800,
            y: 1050,
            w: 16,
            h: 16,
        };
        assert_eq!(detect_edge(monitor, work_area), TaskbarEdge::Bottom);
        let (x, y) = compute(tray, 360, 480, work_area, TaskbarEdge::Bottom);
        assert!(y < tray.y, "popup should be above the tray (y={})", y);
        // Popup horizontal center should be near the tray center, clamped.
        let popup_center = x + 360 / 2;
        assert!((100..=1900).contains(&popup_center));
    }

    #[test]
    fn top_taskbar_anchors_popup_below_tray() {
        let monitor = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let work_area = Rect {
            x: 0,
            y: 40,
            w: 1920,
            h: 1040,
        }; // 40 px at top
        let tray = Rect {
            x: 1800,
            y: 4,
            w: 16,
            h: 16,
        };
        assert_eq!(detect_edge(monitor, work_area), TaskbarEdge::Top);
        let (_, y) = compute(tray, 360, 480, work_area, TaskbarEdge::Top);
        assert!(y > tray.bottom(), "popup should be below tray");
    }

    #[test]
    fn left_taskbar_places_popup_to_the_right() {
        let monitor = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let work_area = Rect {
            x: 64,
            y: 0,
            w: 1856,
            h: 1080,
        };
        let tray = Rect {
            x: 20,
            y: 540,
            w: 16,
            h: 16,
        };
        assert_eq!(detect_edge(monitor, work_area), TaskbarEdge::Left);
        let (x, _) = compute(tray, 360, 480, work_area, TaskbarEdge::Left);
        assert!(x >= tray.right());
    }

    #[test]
    fn right_taskbar_places_popup_to_the_left() {
        let monitor = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let work_area = Rect {
            x: 0,
            y: 0,
            w: 1856,
            h: 1080,
        };
        let tray = Rect {
            x: 1880,
            y: 540,
            w: 16,
            h: 16,
        };
        assert_eq!(detect_edge(monitor, work_area), TaskbarEdge::Right);
        let (x, _) = compute(tray, 360, 480, work_area, TaskbarEdge::Right);
        assert!(x + 360 <= tray.x);
    }

    #[test]
    fn popup_is_clamped_to_work_area() {
        let _monitor = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let work_area = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        };
        let tray_near_edge = Rect {
            x: 1900,
            y: 1050,
            w: 16,
            h: 16,
        };
        let (x, _) = compute(tray_near_edge, 360, 480, work_area, TaskbarEdge::Bottom);
        assert!(
            x + 360 <= work_area.right(),
            "popup must not overflow right edge"
        );
        assert!(x >= work_area.x, "popup must not overflow left edge");
    }
}
