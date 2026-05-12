//! Promotion executor. **Safety contract**: this module never touches
//! the live `~/.codex/auth.json`. The live swap happens only in the
//! service module, after both planning and execution have succeeded.
//!
//! The executor accepts an `ExecutionPlan` (resolved from
//! `PromotionPlan` by the service) and performs writes against a
//! sandboxed managed-homes root. Every write goes through
//! `validate_managed_home_for_writes`; we refuse to write outside the
//! root so a malformed plan can never trash an unrelated directory.

use std::path::{Path, PathBuf};

use super::errors::CodexAccountPromotionError;
use super::types::AuthMaterial;

/// Resolved write target for the executor. The service builds this
/// from the planner output by combining the destination id with the
/// catalog's stored home path (or by minting a fresh UUID home for
/// `ImportNew`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub kind: ExecutionKind,
    /// Absolute path inside the managed-homes sandbox.
    pub destination_home: PathBuf,
    /// Bytes to write at `<destination_home>/auth.json`.
    pub auth_material: AuthMaterial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionKind {
    ImportNew,
    RefreshExisting,
    RepairExisting,
}

pub struct Executor {
    managed_root: PathBuf,
}

impl Executor {
    pub fn new(managed_root: impl Into<PathBuf>) -> Self {
        Self {
            managed_root: managed_root.into(),
        }
    }

    pub fn managed_root(&self) -> &Path {
        &self.managed_root
    }

    /// Validate that `path` lives inside the managed-homes sandbox. Any
    /// `..` segments that would escape the root are rejected.
    pub fn validate_managed_home_for_writes(
        &self,
        path: &Path,
    ) -> Result<(), CodexAccountPromotionError> {
        if !path.starts_with(&self.managed_root) {
            return Err(CodexAccountPromotionError::ManagedStoreCommitFailed);
        }
        for component in path.components() {
            if matches!(component, std::path::Component::ParentDir) {
                return Err(CodexAccountPromotionError::ManagedStoreCommitFailed);
            }
        }
        Ok(())
    }

    /// Same as `validate_managed_home_for_writes` but also requires the
    /// path to be a direct child of `managed_root` (no nested manage,
    /// no sibling dir). Used before delete operations.
    pub fn validate_managed_home_for_deletion(
        &self,
        path: &Path,
    ) -> Result<(), CodexAccountPromotionError> {
        self.validate_managed_home_for_writes(path)?;
        let parent = path.parent();
        if parent != Some(self.managed_root.as_path()) {
            return Err(CodexAccountPromotionError::ManagedStoreCommitFailed);
        }
        Ok(())
    }

    pub fn execute(&self, plan: &ExecutionPlan) -> Result<(), CodexAccountPromotionError> {
        self.validate_managed_home_for_writes(&plan.destination_home)?;
        std::fs::create_dir_all(&plan.destination_home)
            .map_err(|_| CodexAccountPromotionError::ManagedStoreCommitFailed)?;
        let auth_path = plan.destination_home.join("auth.json");
        atomic_write(&auth_path, &plan.auth_material.bytes)
            .map_err(|_| CodexAccountPromotionError::DisplacedLiveImportFailed)?;
        Ok(())
    }

    /// Best-effort cleanup. Refuses to delete outside the sandbox.
    pub fn cleanup_failed_import(&self, home: &Path) -> Result<(), CodexAccountPromotionError> {
        self.validate_managed_home_for_deletion(home)?;
        if home.exists() {
            std::fs::remove_dir_all(home)
                .map_err(|_| CodexAccountPromotionError::ManagedStoreCommitFailed)?;
        }
        Ok(())
    }
}

fn atomic_write(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = target.with_extension(format!("json.codexbar-staged-{nanos}"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn material() -> AuthMaterial {
        AuthMaterial {
            bytes: br#"{"access_token":"a","refresh_token":"r","id_token":"i"}"#.to_vec(),
        }
    }

    #[test]
    fn rejects_writes_outside_the_sandbox() {
        let dir = tempfile::tempdir().unwrap();
        let executor = Executor::new(dir.path());
        let outside = dir.path().parent().unwrap().join("outside");
        let err = executor
            .validate_managed_home_for_writes(&outside)
            .unwrap_err();
        assert_eq!(
            err,
            CodexAccountPromotionError::ManagedStoreCommitFailed,
            "writes outside the sandbox must be refused",
        );
    }

    #[test]
    fn rejects_writes_with_parent_dir_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let executor = Executor::new(dir.path());
        let traversal = dir.path().join("..").join("traverse").join("home");
        let err = executor
            .validate_managed_home_for_writes(&traversal)
            .unwrap_err();
        assert_eq!(err, CodexAccountPromotionError::ManagedStoreCommitFailed);
    }

    #[test]
    fn allows_writes_inside_sandbox() {
        let dir = tempfile::tempdir().unwrap();
        let executor = Executor::new(dir.path());
        let home = dir.path().join("uuid-1");
        executor.validate_managed_home_for_writes(&home).unwrap();
    }

    #[test]
    fn execute_writes_auth_json_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let executor = Executor::new(dir.path());
        let home = dir.path().join("uuid-1");
        let plan = ExecutionPlan {
            kind: ExecutionKind::ImportNew,
            destination_home: home.clone(),
            auth_material: material(),
        };
        executor.execute(&plan).unwrap();
        let actual = std::fs::read(home.join("auth.json")).unwrap();
        assert!(actual.starts_with(b"{\"access_token\""));
    }

    #[test]
    fn cleanup_refuses_deletion_outside_sandbox() {
        let dir = tempfile::tempdir().unwrap();
        let executor = Executor::new(dir.path());
        let outside = dir.path().parent().unwrap().join("sibling");
        let err = executor.cleanup_failed_import(&outside).unwrap_err();
        assert_eq!(err, CodexAccountPromotionError::ManagedStoreCommitFailed);
    }

    #[test]
    fn cleanup_requires_direct_child_of_root() {
        let dir = tempfile::tempdir().unwrap();
        let executor = Executor::new(dir.path());
        let nested = dir.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        let err = executor.cleanup_failed_import(&nested).unwrap_err();
        assert_eq!(err, CodexAccountPromotionError::ManagedStoreCommitFailed);
    }

    #[test]
    fn cleanup_removes_a_freshly_created_home() {
        let dir = tempfile::tempdir().unwrap();
        let executor = Executor::new(dir.path());
        let home = dir.path().join("uuid-1");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("auth.json"), b"x").unwrap();
        executor.cleanup_failed_import(&home).unwrap();
        assert!(!home.exists());
    }
}
