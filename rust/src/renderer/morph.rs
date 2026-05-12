//! Reset celebration morph: three ribbon segments that fade in and out
//! across a normalized progress `t` in `[0, 1]`.
//!
//! Per `docs/windows/spec/10-tray-icon-system.md` section 3.4:
//!
//! - Three horizontal ribbon segments cross fade during the 1.5 s
//!   celebration window.
//! - Cross fade-in starts at `t > 0.55`.
//! - The third ribbon uses `p = t * 1.1` and is allowed to go negative
//!   (clamped to 0 at the renderer level) so it fades out faster than
//!   the first two.
//! - A 512 entry morph cache keys by `styleKey * 1000 + bucket`, with
//!   200 progress buckets across `t in [0, 1]`.

use std::num::NonZeroUsize;

use lru::LruCache;
use parking_lot::Mutex;
use tiny_skia::Pixmap;

use super::style::IconStyle;

pub const BUCKET_COUNT: u32 = 200;
pub const MORPH_CACHE_CAPACITY: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RibbonAlphas {
    pub first: f32,
    pub second: f32,
    pub third: f32,
}

/// Compute the per ribbon alphas at progress `t`.
///
/// First two ribbons fade in over `[0, 0.55]` and fade out over
/// `[0.55, 1.0]`. The third uses `p = t * 1.1`, so it crosses zero
/// before `t = 1.0`. The renderer clamps negative alphas to 0.
pub fn ribbon_alphas(t: f32) -> RibbonAlphas {
    let t = t.clamp(0.0, 1.0);
    let first = if t < 0.55 {
        t / 0.55
    } else {
        ((1.0 - t) / 0.45).max(0.0)
    };
    let second = if t < 0.55 {
        (t / 0.55).powf(1.5)
    } else {
        ((1.0 - t) / 0.45).powf(0.75).max(0.0)
    };
    let p = t * 1.1;
    let third = if p < 0.55 { p / 0.55 } else { (1.0 - p) / 0.45 };
    RibbonAlphas {
        first: first.clamp(0.0, 1.0),
        second: second.clamp(0.0, 1.0),
        third: third.clamp(0.0, 1.0),
    }
}

/// Bucket id for the morph cache. Quantizes `t in [0, 1]` to one of
/// [`BUCKET_COUNT`] integer buckets. Pixmap output is identical within
/// a bucket so cache hits are perceptually transparent.
pub fn progress_bucket(t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let raw = (t * BUCKET_COUNT as f32) as u32;
    raw.min(BUCKET_COUNT - 1)
}

/// Composite cache key combining `IconStyle` and progress bucket.
pub fn cache_key(style: IconStyle, bucket: u32) -> u32 {
    style_index(style) * 1000 + bucket
}

fn style_index(style: IconStyle) -> u32 {
    match style {
        IconStyle::Default => 0,
        IconStyle::Codex => 1,
        IconStyle::Claude => 2,
        IconStyle::Gemini => 3,
        IconStyle::Factory => 4,
        IconStyle::Warp => 5,
    }
}

/// LRU cache of already rendered morph frames.
pub struct MorphCache {
    inner: Mutex<LruCache<u32, std::sync::Arc<Pixmap>>>,
}

impl Default for MorphCache {
    fn default() -> Self {
        Self::new(MORPH_CACHE_CAPACITY)
    }
}

impl MorphCache {
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity.max(1)).expect("non zero capacity");
        Self {
            inner: Mutex::new(LruCache::new(cap)),
        }
    }

    pub fn get(&self, key: u32) -> Option<std::sync::Arc<Pixmap>> {
        self.inner.lock().get(&key).cloned()
    }

    pub fn put(&self, key: u32, pixmap: std::sync::Arc<Pixmap>) {
        self.inner.lock().put(key, pixmap);
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphas_zero_at_t_zero() {
        let a = ribbon_alphas(0.0);
        assert_eq!(a.first, 0.0);
        assert_eq!(a.second, 0.0);
        assert_eq!(a.third, 0.0);
    }

    #[test]
    fn alphas_peak_near_cross_fade_inflection() {
        let a = ribbon_alphas(0.55);
        assert!(a.first > 0.99, "first peaks at t=0.55, got {}", a.first);
        assert!(a.second > 0.99, "second peaks too, got {}", a.second);
    }

    #[test]
    fn third_ribbon_goes_to_zero_before_t_one() {
        // p = t * 1.1; at t = 1/1.1 (~ 0.909), p = 1, so third = 0.
        let a = ribbon_alphas(0.95);
        assert!(a.third < 0.5, "third should fade out faster: {}", a.third);
    }

    #[test]
    fn alphas_always_in_unit_range() {
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            let a = ribbon_alphas(t);
            for v in [a.first, a.second, a.third] {
                assert!((0.0..=1.0).contains(&v), "t={t} produced {v}");
            }
        }
    }

    #[test]
    fn progress_bucket_quantizes_to_under_count() {
        for i in 0..1000 {
            let t = (i as f32) / 999.0;
            let bucket = progress_bucket(t);
            assert!(bucket < BUCKET_COUNT, "bucket {} out of range", bucket);
        }
        assert_eq!(progress_bucket(0.0), 0);
        assert_eq!(progress_bucket(1.0), BUCKET_COUNT - 1);
    }

    #[test]
    fn cache_key_separates_styles() {
        let a = cache_key(IconStyle::Default, 100);
        let b = cache_key(IconStyle::Codex, 100);
        assert_ne!(a, b);
    }

    #[test]
    fn morph_cache_evicts_at_capacity() {
        let cache = MorphCache::new(2);
        let pm = || std::sync::Arc::new(Pixmap::new(2, 2).expect("pixmap"));
        cache.put(1, pm());
        cache.put(2, pm());
        cache.put(3, pm());
        assert_eq!(cache.len(), 2);
        assert!(cache.get(1).is_none(), "oldest entry should be evicted");
    }
}
