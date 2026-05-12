//! Codex account promotion flow. Spec 41 §6.
//!
//! The flow is split into pure-logic and side-effect modules so the
//! decision matrix (`planning.rs`) and the interaction guards
//! (`coordinator.rs`) are fully testable without the filesystem.
//! `execution.rs` performs the writes against a sandboxed managed-
//! homes root and refuses anything that would escape the sandbox.
//!
//! The final live swap of `~/.codex/auth.json` is the service's job
//! (deferred to a follow-up commit) — execution intentionally never
//! touches the live file so a broken plan cannot corrupt the Codex CLI
//! login.

pub mod coordinator;
pub mod errors;
pub mod execution;
pub mod planning;
pub mod types;

pub use coordinator::{PromotionCoordinator, PromotionGuard};
pub use errors::{CodexAccountPromotionError, ALERT_TITLE};
pub use execution::{ExecutionKind, ExecutionPlan, Executor};
pub use planning::plan;
pub use types::{
    AuthMaterial, CodexActiveSource, ImportReason, LiveHomeState, ManagedCodexAccount, NoneReason,
    PreparedIdentity, PreparedLiveAccount, PreparedPromotionContext, PreparedStoredManagedAccount,
    PromotionPlan, RefreshReason, RejectReason, RepairReason, StoredHomeState,
};
