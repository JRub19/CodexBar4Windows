//! File backed settings store with atomic writes and corruption recovery.
//!
//! Writes go to `config.json.tmp` first, then are renamed over the live
//! file. A parse failure on read backs the bad file up as
//! `config.json.broken-<unix-ts>` and re-emits defaults.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use thiserror::Error;
use tracing::{info, warn};

use super::model::{Settings, SettingsPatch};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("serialize: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("rename {from} -> {to}: {source}")]
    Rename {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Owned settings store backed by an on disk JSON file.
#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    state: RwLock<Settings>,
}

/// Convenient `Arc<SettingsStore>` alias for sharing across IPC handlers.
pub type SettingsHandle = Arc<SettingsStore>;

impl SettingsStore {
    /// Load settings from disk, falling back to defaults on missing or
    /// corrupt files. The store remembers the path and rewrites to it on
    /// every successful mutation.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let initial = read_or_default(&path);
        Self {
            path,
            state: RwLock::new(initial),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn snapshot(&self) -> Settings {
        self.state.read().expect("settings lock poisoned").clone()
    }

    pub fn update(&self, patch: SettingsPatch) -> Result<Settings, StoreError> {
        let next = {
            let mut guard = self.state.write().expect("settings lock poisoned");
            *guard = guard.clone().apply_patch(patch);
            guard.clone()
        };
        atomic_write(&self.path, &next)?;
        info!(target: "codexbar::settings", "settings.updated");
        Ok(next)
    }

    pub fn reset(&self) -> Result<Settings, StoreError> {
        let defaults = Settings::default();
        {
            let mut guard = self.state.write().expect("settings lock poisoned");
            *guard = defaults.clone();
        }
        atomic_write(&self.path, &defaults)?;
        info!(target: "codexbar::settings", "settings.reset");
        Ok(defaults)
    }
}

fn read_or_default(path: &Path) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(parsed) => parsed,
            Err(err) => {
                warn!(
                    target: "codexbar::settings",
                    error = %err,
                    "settings.parse_failed",
                );
                back_up_broken(path);
                Settings::default()
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(err) => {
            warn!(
                target: "codexbar::settings",
                error = %err,
                "settings.read_failed",
            );
            Settings::default()
        }
    }
}

fn back_up_broken(path: &Path) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = path.with_file_name(format!(
        "{}.broken-{}",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("config"),
        ts
    ));
    if let Err(err) = std::fs::rename(path, &backup) {
        warn!(
            target: "codexbar::settings",
            error = %err,
            backup = %backup.display(),
            "settings.backup_failed",
        );
    }
}

fn atomic_write(path: &Path, settings: &Settings) -> Result<(), StoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| StoreError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(settings)?;
    std::fs::write(&tmp, &bytes).map_err(|source| StoreError::Write {
        path: tmp.clone(),
        source,
    })?;
    std::fs::rename(&tmp, path).map_err(|source| StoreError::Rename {
        from: tmp,
        to: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::model::RefreshFrequency;

    fn temp_config() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        (dir, path)
    }

    #[test]
    fn load_returns_defaults_for_missing_file() {
        let (_dir, path) = temp_config();
        let store = SettingsStore::load(path);
        let snap = store.snapshot();
        assert_eq!(snap.refresh_frequency, RefreshFrequency::FiveMinutes);
    }

    #[test]
    fn update_persists_and_round_trips() {
        let (_dir, path) = temp_config();
        let store = SettingsStore::load(&path);
        store
            .update(SettingsPatch {
                refresh_frequency: Some(RefreshFrequency::OneMinute),
                ..Default::default()
            })
            .expect("update");
        let on_disk = std::fs::read_to_string(&path).expect("read file");
        let reloaded: Settings = serde_json::from_str(&on_disk).expect("parse");
        assert_eq!(reloaded.refresh_frequency, RefreshFrequency::OneMinute);
    }

    #[test]
    fn reset_restores_defaults_on_disk() {
        let (_dir, path) = temp_config();
        let store = SettingsStore::load(&path);
        store
            .update(SettingsPatch {
                pause_refresh: Some(true),
                ..Default::default()
            })
            .expect("update");
        store.reset().expect("reset");
        let on_disk: Settings =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_disk, Settings::default());
    }

    #[test]
    fn corrupt_file_backs_up_and_returns_defaults() {
        let (_dir, path) = temp_config();
        std::fs::write(&path, b"{ this is not json").unwrap();
        let store = SettingsStore::load(&path);
        assert_eq!(store.snapshot(), Settings::default());
        let same_dir = path.parent().unwrap();
        let any_broken = std::fs::read_dir(same_dir)
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("config.json.broken-")
            });
        assert!(any_broken, "expected broken backup file");
    }
}
