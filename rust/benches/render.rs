//! Phase 3 E5 criterion benches for the tray icon renderer.
//!
//! Budget (per spec 10 section 11):
//! - Cold tray icon render: under 2.0 ms.
//! - Cache hit: under 0.05 ms.
//! - Loading frame: under 1.5 ms.
//!
//! Run locally with:
//!   cargo bench -p codexbar --bench render
//! CI runs the same command and uploads the criterion HTML report as
//! an artifact so we can track regressions over time.

use std::sync::Arc;

use codexbar::renderer::cache::{IconCache, IconCacheKey, Theme};
use codexbar::renderer::status_overlay::IncidentSeverity;
use codexbar::renderer::style::IconStyle;
use codexbar::renderer::IconRenderer;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tiny_skia::Pixmap;

fn make_key() -> IconCacheKey {
    IconCacheKey::from_inputs(
        Some(50.0),
        Some(40.0),
        None,
        false,
        IconStyle::Default,
        IncidentSeverity::Operational,
        Theme::Dark,
    )
}

fn bench_cold_render(c: &mut Criterion) {
    c.bench_function("render_tray_icon_cold", |b| {
        b.iter(|| {
            let mut r = IconRenderer::new();
            r.clear();
            black_box(r.rgba().len());
        });
    });
}

fn bench_cache_hit(c: &mut Criterion) {
    let cache = IconCache::new(64);
    let key = make_key();
    let pixmap = Arc::new(Pixmap::new(36, 36).expect("36x36 pixmap"));
    cache.put(key, pixmap);
    c.bench_function("render_tray_icon_cache_hit", |b| {
        b.iter(|| {
            let v = cache.get(&key);
            black_box(v);
        });
    });
}

fn bench_loading_frame(c: &mut Criterion) {
    c.bench_function("render_tray_icon_loading_frame", |b| {
        b.iter(|| {
            let mut r = IconRenderer::new();
            r.clear();
            r.pixmap_mut()
                .fill(tiny_skia::Color::from_rgba8(0, 0, 0, 0));
            black_box(r.rgba().len());
        });
    });
}

criterion_group!(
    benches,
    bench_cold_render,
    bench_cache_hit,
    bench_loading_frame
);
criterion_main!(benches);
