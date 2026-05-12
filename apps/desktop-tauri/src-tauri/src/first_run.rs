//! Phase 3 D12: persist a single "first run hint shown" flag so we only
//! surface the tray pin balloon once per install. State lives next to
//! `settings.json` in `%APPDATA%\CodexBar4Windows\state.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FirstRunState {
    #[serde(default)]
    pub tray_pinned_hint_shown: bool,
}

#[derive(Clone)]
pub struct FirstRunStore {
    path: PathBuf,
}

impl FirstRunStore {
    pub fn new(state_dir: impl AsRef<Path>) -> Self {
        Self {
            path: state_dir.as_ref().join("state.json"),
        }
    }

    pub fn read(&self) -> FirstRunState {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => FirstRunState::default(),
        }
    }

    /// Atomic write: write to a temp file in the same directory, then
    /// rename over the target so partial writes never corrupt state.
    pub fn write(&self, state: &FirstRunState) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(state).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(tmp, &self.path)?;
        Ok(())
    }

    pub fn mark_tray_pinned_hint_shown(&self) -> std::io::Result<()> {
        let mut s = self.read();
        s.tray_pinned_hint_shown = true;
        self.write(&s)
    }

    pub fn clear(&self) -> std::io::Result<()> {
        self.write(&FirstRunState::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_state_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let store = FirstRunStore::new(dir.path());
        assert!(!store.read().tray_pinned_hint_shown);
        store.mark_tray_pinned_hint_shown().unwrap();
        assert!(store.read().tray_pinned_hint_shown);
        store.clear().unwrap();
        assert!(!store.read().tray_pinned_hint_shown);
    }
}
