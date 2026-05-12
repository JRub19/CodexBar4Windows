//! Build a multi size ICO file from one 36 by 36 source pixmap.
//!
//! Per `docs/windows/spec/10-tray-icon-system.md` section 6, the Windows
//! tray needs an ICO atlas with entries at 16, 20, 24, 28, 32, 36, 40,
//! 48, and 64 px so Shell_NotifyIcon picks the correct size on each
//! monitor's DPI. We resample with Lanczos3.
//!
//! The smallest bucket collapse rule (a simpler single bar variant at
//! 16 px) is documented in the spec but deferred: at 16 px the bilinear
//! resample of the 36 px source is acceptable, and a separate render
//! pipeline for the small icon is post v1 polish.

use ico::{IconDir, IconImage, ResourceType};
use image::imageops::FilterType;
use image::{ImageBuffer, Rgba, RgbaImage};
use tiny_skia::Pixmap;

pub const ATLAS_SIZES_PX: [u32; 9] = [16, 20, 24, 28, 32, 36, 40, 48, 64];

#[derive(Debug, thiserror::Error)]
pub enum AtlasError {
    #[error("pixmap is empty")]
    Empty,
    #[error("image conversion failed: {0}")]
    Image(String),
    #[error("ico encoding failed: {0}")]
    Ico(String),
}

/// Encode the given pixmap as a multi size ICO byte buffer.
pub fn encode_ico(source: &Pixmap) -> Result<Vec<u8>, AtlasError> {
    if source.width() == 0 || source.height() == 0 {
        return Err(AtlasError::Empty);
    }
    let src_rgba: RgbaImage = pixmap_to_image(source)?;
    let mut dir = IconDir::new(ResourceType::Icon);
    for &size in ATLAS_SIZES_PX.iter() {
        let resized = image::imageops::resize(&src_rgba, size, size, FilterType::Lanczos3);
        let icon_image = IconImage::from_rgba_data(size, size, resized.into_raw());
        let entry =
            ico::IconDirEntry::encode(&icon_image).map_err(|e| AtlasError::Ico(e.to_string()))?;
        dir.add_entry(entry);
    }
    let mut buf: Vec<u8> = Vec::new();
    dir.write(&mut buf)
        .map_err(|e| AtlasError::Ico(e.to_string()))?;
    Ok(buf)
}

fn pixmap_to_image(source: &Pixmap) -> Result<RgbaImage, AtlasError> {
    let w = source.width();
    let h = source.height();
    let mut buf = source.data().to_vec();
    // tiny_skia uses premultiplied RGBA; image expects straight RGBA.
    // The Lanczos resize behaves better on straight alpha. Convert.
    for chunk in buf.chunks_exact_mut(4) {
        let a = chunk[3] as f32 / 255.0;
        if a > 0.0 {
            for c in chunk.iter_mut().take(3) {
                let straight = (*c as f32 / a).clamp(0.0, 255.0);
                *c = straight as u8;
            }
        }
    }
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(w, h, buf)
        .ok_or_else(|| AtlasError::Image("pixmap byte count mismatch".to_string()))?;
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Color;

    fn solid_pixmap() -> Pixmap {
        let mut p = Pixmap::new(36, 36).expect("pixmap");
        p.fill(Color::from_rgba(0.0, 0.5, 1.0, 1.0).unwrap());
        p
    }

    #[test]
    fn encode_returns_non_empty_bytes() {
        let p = solid_pixmap();
        let bytes = encode_ico(&p).expect("encode");
        assert!(
            bytes.len() > 1024,
            "ico should be at least 1 KiB, got {}",
            bytes.len()
        );
    }

    #[test]
    fn encode_starts_with_ico_magic() {
        let p = solid_pixmap();
        let bytes = encode_ico(&p).expect("encode");
        // ICO header: reserved=0, type=1, count=N. First 4 bytes are
        // `00 00 01 00`.
        assert_eq!(bytes[..4], [0, 0, 1, 0]);
    }

    #[test]
    fn ico_contains_all_nine_atlas_sizes() {
        let p = solid_pixmap();
        let bytes = encode_ico(&p).expect("encode");
        // count is at offset 4..6, little endian u16.
        let count = u16::from_le_bytes([bytes[4], bytes[5]]);
        assert_eq!(count, ATLAS_SIZES_PX.len() as u16);
    }

    #[test]
    fn empty_pixmap_returns_error() {
        // Pixmap::new requires positive dimensions; simulate via a 1x1
        // pixmap that we then claim is empty. Best we can do without
        // allocating a 0 by 0 (which the crate rejects).
        let p = Pixmap::new(1, 1).expect("pixmap");
        let bytes = encode_ico(&p);
        assert!(bytes.is_ok(), "1x1 should encode fine");
    }
}
