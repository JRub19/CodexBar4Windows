//! Phase 3 E4 manual QA harness. Gated behind the `dev` cargo feature
//! so production builds never include it. When enabled, a background
//! task rotates through the canonical icon states every 8 seconds so
//! a human reviewer can verify every state without wiring real auth.
//!
//! States cycled:
//! 1. normal (single bar, 50%)
//! 2. loading (animated pattern)
//! 3. stale (dim alpha)
//! 4. error (status overlay)
//! 5. reset celebration (morph)
//! 6. quota flash (highlighted card)

use std::time::Duration;

use codexbar::renderer::FrameDriver;
use tokio::time::sleep;
use tracing::info;

#[derive(Clone, Copy, Debug)]
pub enum Fixture {
    Normal,
    Loading,
    Stale,
    Error,
    ResetCelebration,
    QuotaFlash,
}

impl Fixture {
    pub fn label(self) -> &'static str {
        match self {
            Fixture::Normal => "normal",
            Fixture::Loading => "loading",
            Fixture::Stale => "stale",
            Fixture::Error => "error",
            Fixture::ResetCelebration => "reset_celebration",
            Fixture::QuotaFlash => "quota_flash",
        }
    }

    pub fn cycle() -> [Fixture; 6] {
        [
            Fixture::Normal,
            Fixture::Loading,
            Fixture::Stale,
            Fixture::Error,
            Fixture::ResetCelebration,
            Fixture::QuotaFlash,
        ]
    }
}

pub const FIXTURE_INTERVAL: Duration = Duration::from_secs(8);

/// Spawn a tokio task that rotates fixtures every `FIXTURE_INTERVAL`.
/// The actual icon repaint hook is passed in by the caller so we don't
/// take a hard dependency on the tray-icon plumbing here.
pub async fn run_fixture_loop<F>(_driver: FrameDriver, mut on_fixture: F)
where
    F: FnMut(Fixture) + Send + 'static,
{
    let cycle = Fixture::cycle();
    let mut idx = 0;
    loop {
        let fixture = cycle[idx % cycle.len()];
        info!(target: "codexbar::dev", fixture = fixture.label(), "fixture.tick");
        on_fixture(fixture);
        sleep(FIXTURE_INTERVAL).await;
        idx += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_lists_all_states() {
        let labels: Vec<_> = Fixture::cycle().iter().map(|f| f.label()).collect();
        assert_eq!(labels.len(), 6);
        assert!(labels.contains(&"normal"));
        assert!(labels.contains(&"loading"));
        assert!(labels.contains(&"stale"));
        assert!(labels.contains(&"error"));
        assert!(labels.contains(&"reset_celebration"));
        assert!(labels.contains(&"quota_flash"));
    }
}
