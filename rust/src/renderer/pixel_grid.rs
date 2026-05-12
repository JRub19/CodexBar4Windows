//! Pixel grid snapping helpers.
//!
//! Geometry described in `docs/windows/spec/10-tray-icon-system.md` is
//! quoted in physical pixels at the 36 by 36 canvas. `PixelGrid` provides
//! a half pixel snap so 1 px strokes land on a sharp pixel boundary,
//! mirroring the Mac source's pixel grid helper.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelGrid;

impl PixelGrid {
    /// Snap a logical x or y to its nearest half pixel center.
    ///
    /// An odd integer (1 px stroke offset) returns 0.5. An even integer
    /// returns 0.0. Used by stroke functions so a 1 px outline does not
    /// blur over two pixel rows.
    pub fn snap_delta(coord: u32) -> f32 {
        if coord.is_multiple_of(2) {
            0.0
        } else {
            0.5
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn even_coords_snap_to_zero() {
        assert_eq!(PixelGrid::snap_delta(0), 0.0);
        assert_eq!(PixelGrid::snap_delta(2), 0.0);
        assert_eq!(PixelGrid::snap_delta(36), 0.0);
    }

    #[test]
    fn odd_coords_snap_to_half() {
        assert_eq!(PixelGrid::snap_delta(1), 0.5);
        assert_eq!(PixelGrid::snap_delta(3), 0.5);
        assert_eq!(PixelGrid::snap_delta(35), 0.5);
    }
}
