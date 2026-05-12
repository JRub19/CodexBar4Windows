//! Canvas wrapper around `tiny_skia::Pixmap`.
//!
//! The renderer reuses one allocation across redraws by `clear`ing the
//! existing pixmap. Group A tasks A3 onward fill in the per primitive draw
//! methods (bars, twists, overlays). A2 keeps the surface minimal: a
//! clear, an empty render, and an accessor for the underlying buffer so
//! ICO encoders can consume it.

use tiny_skia::Pixmap;

use super::{CANVAS_PX, OUTPUT_SCALE};

pub struct IconRenderer {
    pixmap: Pixmap,
}

impl Default for IconRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl IconRenderer {
    /// Allocate a fresh 36 by 36 RGBA pixmap.
    pub fn new() -> Self {
        Self {
            pixmap: Pixmap::new(CANVAS_PX, CANVAS_PX).expect("36x36 pixmap must allocate"),
        }
    }

    /// Discard the current frame.
    pub fn clear(&mut self) {
        self.pixmap.fill(tiny_skia::Color::TRANSPARENT);
    }

    /// Borrow the rendered RGBA buffer. Bytes are premultiplied RGBA at
    /// 36 by 36. Group A11 resamples this into the ICO atlas.
    pub fn rgba(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// Width in physical pixels.
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    /// Height in physical pixels.
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// Output scale, for callers that need to convert points to pixels.
    pub fn scale(&self) -> u32 {
        OUTPUT_SCALE
    }

    /// Mutable access to the underlying pixmap for the per primitive
    /// draw modules added by tasks A3 through A9.
    pub fn pixmap_mut(&mut self) -> &mut Pixmap {
        &mut self.pixmap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_canvas_is_36_by_36_and_fully_transparent() {
        let renderer = IconRenderer::new();
        assert_eq!(renderer.width(), 36);
        assert_eq!(renderer.height(), 36);
        for chunk in renderer.rgba().chunks_exact(4) {
            assert_eq!(chunk[3], 0, "expected alpha 0 on fresh canvas");
        }
    }

    #[test]
    fn clear_restores_transparent_canvas() {
        let mut renderer = IconRenderer::new();
        renderer.pixmap_mut().fill(tiny_skia::Color::WHITE);
        renderer.clear();
        for chunk in renderer.rgba().chunks_exact(4) {
            assert_eq!(chunk[3], 0);
        }
    }
}
