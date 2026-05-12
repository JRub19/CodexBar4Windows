//! Secret encryption migration runner.
//!
//! Each schema version is a function that consumes the previous on disk
//! shape and produces the next. `run_pending` is idempotent: each
//! migration writes a marker file in the secrets root so subsequent runs
//! short circuit. The current implementation ships only the v1 baseline,
//! which is a no op on a fresh install (greenfield repo). Future versions
//! that rotate the DPAPI envelope or move from a flat layout to a sharded
//! one append a new function here.

use std::path::{Path, PathBuf};

use super::errors::SecretsError;

pub const CURRENT_VERSION: u32 = 1;
const MARKER_FILENAME: &str = ".migration-version";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationOutcome {
    AlreadyUpToDate,
    Migrated { from: u32, to: u32 },
    Fresh { initialized_at: u32 },
}

pub fn run_pending(secrets_root: &Path) -> Result<MigrationOutcome, SecretsError> {
    std::fs::create_dir_all(secrets_root).map_err(|source| SecretsError::Io {
        path: secrets_root.to_path_buf(),
        source,
    })?;
    let marker = marker_path(secrets_root);
    let on_disk = read_marker(&marker);
    match on_disk {
        None => {
            write_marker(&marker, CURRENT_VERSION)?;
            Ok(MigrationOutcome::Fresh {
                initialized_at: CURRENT_VERSION,
            })
        }
        Some(v) if v == CURRENT_VERSION => Ok(MigrationOutcome::AlreadyUpToDate),
        Some(v) if v < CURRENT_VERSION => {
            // Future: dispatch each migration step here.
            let from = v;
            write_marker(&marker, CURRENT_VERSION)?;
            Ok(MigrationOutcome::Migrated {
                from,
                to: CURRENT_VERSION,
            })
        }
        Some(v) => {
            // Newer marker than this binary expects. Treat as up to date;
            // do not overwrite.
            tracing::warn!(
                target: "codexbar::secrets::migration",
                marker_version = v,
                expected = CURRENT_VERSION,
                "secrets.migration.future_marker_detected"
            );
            Ok(MigrationOutcome::AlreadyUpToDate)
        }
    }
}

fn marker_path(root: &Path) -> PathBuf {
    root.join(MARKER_FILENAME)
}

fn read_marker(path: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(path).ok()?;
    text.trim().parse().ok()
}

fn write_marker(path: &Path, version: u32) -> Result<(), SecretsError> {
    std::fs::write(path, version.to_string()).map_err(|source| SecretsError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_dir_initializes_to_current_version() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let outcome = run_pending(tmp.path()).expect("run");
        assert!(matches!(
            outcome,
            MigrationOutcome::Fresh {
                initialized_at: CURRENT_VERSION
            }
        ));
        assert!(marker_path(tmp.path()).is_file());
    }

    #[test]
    fn second_run_is_already_up_to_date() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let _ = run_pending(tmp.path()).expect("first");
        let outcome = run_pending(tmp.path()).expect("second");
        assert_eq!(outcome, MigrationOutcome::AlreadyUpToDate);
    }

    #[test]
    fn future_marker_is_left_intact() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(marker_path(tmp.path()), "999").unwrap();
        let outcome = run_pending(tmp.path()).expect("run");
        assert_eq!(outcome, MigrationOutcome::AlreadyUpToDate);
        let marker = std::fs::read_to_string(marker_path(tmp.path())).unwrap();
        assert_eq!(marker, "999");
    }
}
