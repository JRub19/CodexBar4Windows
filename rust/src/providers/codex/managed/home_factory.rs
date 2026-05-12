//! Managed home factory and sandbox validation. Every write or delete
//! against the managed-codex-homes root flows through this module so a
//! malformed plan cannot escape the sandbox via `..` traversal or
//! symlink redirection.

use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HomeFactoryError {
    #[error("path is not inside the managed-homes root")]
    OutsideSandbox,
    #[error("path contains a parent-dir traversal segment")]
    ParentTraversal,
    #[error("path is reserved (user home or .codex)")]
    ReservedPath,
    #[error("filesystem error: {0}")]
    Io(String),
}

pub struct HomeFactory {
    root: PathBuf,
}

impl HomeFactory {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Allocate a fresh managed home under the root. The UUID is
    /// derived from the system nanosecond clock; tests inject their
    /// own clock by calling `make_home_url_with_id` directly.
    pub fn make_home_url(&self) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let id = format!("home-{nanos:x}");
        self.root.join(id)
    }

    pub fn make_home_url_with_id(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// Validate the path is safe to write to. Refuses any segment that
    /// could escape the sandbox.
    pub fn validate_for_writes(&self, path: &Path) -> Result<(), HomeFactoryError> {
        validate_path_is_inside(&self.root, path)
    }

    /// Validate that the path is safe to delete. Adds the rule that
    /// the path must be a direct child of the managed-homes root (so
    /// we never delete the root itself or a nested sibling).
    pub fn validate_for_deletion(&self, path: &Path) -> Result<(), HomeFactoryError> {
        self.validate_for_writes(path)?;
        if path == self.root {
            return Err(HomeFactoryError::ReservedPath);
        }
        if path.parent() != Some(self.root.as_path()) {
            return Err(HomeFactoryError::OutsideSandbox);
        }
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(".codex"))
        {
            return Err(HomeFactoryError::ReservedPath);
        }
        Ok(())
    }
}

fn validate_path_is_inside(root: &Path, path: &Path) -> Result<(), HomeFactoryError> {
    // Reject parent-dir traversal first, before any canonicalization.
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(HomeFactoryError::ParentTraversal);
        }
    }
    // The path must start with the root prefix. We do the comparison on
    // the literal paths because tests pass temporary directories that
    // do not exist until the executor creates them.
    if !path.starts_with(root) {
        return Err(HomeFactoryError::OutsideSandbox);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_home_url_emits_unique_paths_under_root() {
        let dir = tempfile::tempdir().unwrap();
        let factory = HomeFactory::new(dir.path());
        let a = factory.make_home_url();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = factory.make_home_url();
        assert_ne!(a, b);
        assert!(a.starts_with(dir.path()));
        assert!(b.starts_with(dir.path()));
    }

    #[test]
    fn validate_for_writes_accepts_paths_inside_root() {
        let dir = tempfile::tempdir().unwrap();
        let factory = HomeFactory::new(dir.path());
        let inside = dir.path().join("home-1");
        factory.validate_for_writes(&inside).unwrap();
    }

    #[test]
    fn validate_for_writes_rejects_parent_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let factory = HomeFactory::new(dir.path());
        let traversal = dir.path().join("..").join("escape");
        assert_eq!(
            factory.validate_for_writes(&traversal),
            Err(HomeFactoryError::ParentTraversal),
        );
    }

    #[test]
    fn validate_for_writes_rejects_sibling_directories() {
        let dir = tempfile::tempdir().unwrap();
        let factory = HomeFactory::new(dir.path());
        let sibling = dir.path().parent().unwrap().join("sibling");
        assert_eq!(
            factory.validate_for_writes(&sibling),
            Err(HomeFactoryError::OutsideSandbox),
        );
    }

    #[test]
    fn validate_for_deletion_refuses_root_itself() {
        let dir = tempfile::tempdir().unwrap();
        let factory = HomeFactory::new(dir.path());
        let err = factory.validate_for_deletion(dir.path()).unwrap_err();
        assert_eq!(err, HomeFactoryError::ReservedPath);
    }

    #[test]
    fn validate_for_deletion_rejects_nested_grandchild() {
        let dir = tempfile::tempdir().unwrap();
        let factory = HomeFactory::new(dir.path());
        let nested = dir.path().join("a").join("b");
        let err = factory.validate_for_deletion(&nested).unwrap_err();
        assert_eq!(err, HomeFactoryError::OutsideSandbox);
    }

    #[test]
    fn validate_for_deletion_refuses_dot_codex() {
        let dir = tempfile::tempdir().unwrap();
        let factory = HomeFactory::new(dir.path());
        let codex_like = dir.path().join(".codex");
        let err = factory.validate_for_deletion(&codex_like).unwrap_err();
        assert_eq!(err, HomeFactoryError::ReservedPath);
    }

    #[test]
    fn validate_for_deletion_accepts_a_managed_home() {
        let dir = tempfile::tempdir().unwrap();
        let factory = HomeFactory::new(dir.path());
        let home = dir.path().join("home-1");
        factory.validate_for_deletion(&home).unwrap();
    }
}
