//! Dynamic tray icon renderer.
//!
//! Phase 3 Group A. The renderer composes a per refresh RGBA buffer at the
//! requested pixel size; later groups (B) wrap it in the Windows tray host,
//! and the popup UI (groups C, D) renders the React popover on top.
//!
//! Canvas math: the Mac source renders at logical 18 by 18 pt with a 2x
//! output scale, producing a 36 by 36 px buffer that is then resampled
//! into an ICO atlas. We mirror that contract exactly so the geometry
//! tables in `docs/windows/spec/10-tray-icon-system.md` apply unchanged.

pub mod atlas;
pub mod bars;
pub mod brand;
pub mod cache;
pub mod canvas;
pub mod layout;
pub mod morph;
pub mod patterns;
pub mod pixel_grid;
pub mod state;
pub mod status_overlay;
pub mod style;
pub mod twists;

pub use atlas::{encode_ico, AtlasError, ATLAS_SIZES_PX};
pub use bars::{draw_bar, BarAlphas, BarRect};
pub use brand::{paint_brand, BrandIconParams};
pub use cache::{IconCache, IconCacheKey, Theme};
pub use canvas::IconRenderer;
pub use layout::{select as select_layout, BarSlot, Layout, LayoutInput};
pub use morph::{cache_key as morph_cache_key, progress_bucket, ribbon_alphas, MorphCache};
pub use patterns::LoadingPattern;
pub use pixel_grid::PixelGrid;
pub use state::{bar_alphas_for, STALE_ALPHAS};
pub use status_overlay::{paint_overlay, IncidentSeverity};
pub use style::IconStyle;
pub use twists::paint_twist;

/// Logical canvas size in points.
pub const CANVAS_PT: u32 = 18;

/// Output scale factor. The Mac source renders at 2x; we keep that for
/// per pixel parity with the reference geometry tables.
pub const OUTPUT_SCALE: u32 = 2;

/// Output buffer size in physical pixels. `CANVAS_PT * OUTPUT_SCALE`.
pub const CANVAS_PX: u32 = CANVAS_PT * OUTPUT_SCALE;
