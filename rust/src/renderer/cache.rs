//! 64 entry LRU cache for fully rendered tray icons.
//!
//! Per `docs/windows/spec/10-tray-icon-system.md` section 5.1, the key
//! is `(primary, weekly, credits, stale, style, indicator, theme)`. The
//! renderer skips work when the same key was rendered recently. With
//! the 5 minute default cadence, idle workloads hit the cache after the
//! first paint and stay quiet.

use std::num::NonZeroUsize;
use std::sync::Arc;

use lru::LruCache;
use parking_lot::Mutex;
use tiny_skia::Pixmap;

use super::status_overlay::IncidentSeverity;
use super::style::IconStyle;

pub const ICON_CACHE_CAPACITY: usize = 64;

/// Quantize `primary` / `weekly` / `credits` to integer buckets so close
/// values share a cache slot.
pub const VALUE_QUANTIZATION_STEPS: u32 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IconCacheKey {
    pub primary: Option<u32>,
    pub weekly: Option<u32>,
    pub credits: Option<u32>,
    pub stale: bool,
    pub style: IconStyle,
    pub indicator: IncidentSeverity,
    pub theme: Theme,
}

impl IconCacheKey {
    /// Build a cache key from raw input values, quantizing each value to
    /// one of [`VALUE_QUANTIZATION_STEPS`] integer buckets in `[0, 100]`.
    /// `None` inputs stay `None` in the key.
    pub fn from_inputs(
        primary: Option<f32>,
        weekly: Option<f32>,
        credits_ratio: Option<f32>,
        stale: bool,
        style: IconStyle,
        indicator: IncidentSeverity,
        theme: Theme,
    ) -> Self {
        Self {
            primary: primary.map(quantize),
            weekly: weekly.map(quantize),
            credits: credits_ratio.map(quantize),
            stale,
            style,
            indicator,
            theme,
        }
    }
}

fn quantize(value: f32) -> u32 {
    let clamped = value.clamp(0.0, 100.0);
    let scaled = clamped * (VALUE_QUANTIZATION_STEPS as f32) / 100.0;
    (scaled.round() as u32).min(VALUE_QUANTIZATION_STEPS)
}

pub struct IconCache {
    inner: Mutex<LruCache<IconCacheKey, Arc<Pixmap>>>,
}

impl Default for IconCache {
    fn default() -> Self {
        Self::new(ICON_CACHE_CAPACITY)
    }
}

impl IconCache {
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity.max(1)).expect("non zero capacity");
        Self {
            inner: Mutex::new(LruCache::new(cap)),
        }
    }

    pub fn get(&self, key: &IconCacheKey) -> Option<Arc<Pixmap>> {
        self.inner.lock().get(key).cloned()
    }

    pub fn put(&self, key: IconCacheKey, pixmap: Arc<Pixmap>) {
        self.inner.lock().put(key, pixmap);
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.inner.lock().cap().get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pm() -> Arc<Pixmap> {
        Arc::new(Pixmap::new(2, 2).expect("pixmap"))
    }

    fn key(primary: f32) -> IconCacheKey {
        IconCacheKey::from_inputs(
            Some(primary),
            None,
            None,
            false,
            IconStyle::Default,
            IncidentSeverity::Operational,
            Theme::Dark,
        )
    }

    #[test]
    fn quantize_collapses_close_values() {
        // 200 buckets across [0, 100] means each step is 0.5. Values in
        // the same half-percent window round to the same bucket.
        let a = IconCacheKey::from_inputs(
            Some(49.55),
            None,
            None,
            false,
            IconStyle::Default,
            IncidentSeverity::Operational,
            Theme::Dark,
        );
        let b = IconCacheKey::from_inputs(
            Some(49.70),
            None,
            None,
            false,
            IconStyle::Default,
            IncidentSeverity::Operational,
            Theme::Dark,
        );
        assert_eq!(a.primary, b.primary);
    }

    #[test]
    fn distinct_values_at_far_buckets_disagree() {
        let a = IconCacheKey::from_inputs(
            Some(50.0),
            None,
            None,
            false,
            IconStyle::Default,
            IncidentSeverity::Operational,
            Theme::Dark,
        );
        let b = IconCacheKey::from_inputs(
            Some(80.0),
            None,
            None,
            false,
            IconStyle::Default,
            IncidentSeverity::Operational,
            Theme::Dark,
        );
        assert_ne!(a.primary, b.primary);
    }

    #[test]
    fn cache_evicts_oldest_at_capacity() {
        let cache = IconCache::new(2);
        cache.put(key(10.0), pm());
        cache.put(key(20.0), pm());
        cache.put(key(30.0), pm());
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&key(10.0)).is_none(), "oldest evicted");
        assert!(cache.get(&key(30.0)).is_some());
    }

    #[test]
    fn default_capacity_is_64() {
        let cache = IconCache::default();
        assert_eq!(cache.capacity(), 64);
    }

    #[test]
    fn theme_change_misses_cache() {
        let dark = IconCacheKey::from_inputs(
            Some(50.0),
            None,
            None,
            false,
            IconStyle::Default,
            IncidentSeverity::Operational,
            Theme::Dark,
        );
        let light = IconCacheKey::from_inputs(
            Some(50.0),
            None,
            None,
            false,
            IconStyle::Default,
            IncidentSeverity::Operational,
            Theme::Light,
        );
        let cache = IconCache::default();
        cache.put(dark, pm());
        assert!(cache.get(&light).is_none(), "theme is part of the key");
    }
}
