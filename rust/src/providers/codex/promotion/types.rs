//! Promotion vocabulary. Spec 41 §6.1 / §6.4 define every type.
//!
//! The flow is: Preparation reads disk and produces a
//! `PreparedPromotionContext`. Planning is a pure function over that
//! context that decides what to do. Execution performs the writes
//! against a sandboxed managed-homes root. The Service orchestrates
//! and performs the final atomic swap of the live `auth.json`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Stable id for the active Codex source after a promotion. Either a
/// managed account (we own its `auth.json`) or the live system account
/// (Codex CLI owns `~/.codex/auth.json` directly).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id")]
pub enum CodexActiveSource {
    LiveSystem,
    ManagedAccount(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedCodexAccount {
    pub id: String,
    pub created_at_unix_secs: i64,
    pub display_name: Option<String>,
    pub home: PathBuf,
    pub provider_id: Option<String>,
    pub email: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthMaterial {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoredHomeState {
    Readable(AuthMaterial),
    Missing(PathBuf),
    Unreadable(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveHomeState {
    Missing,
    Unreadable,
    ApiKeyOnly(AuthMaterial),
    Readable(AuthMaterial),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedStoredManagedAccount {
    pub account: ManagedCodexAccount,
    pub home_state: StoredHomeState,
    pub identity: Option<PreparedIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedLiveAccount {
    pub home_state: LiveHomeState,
    pub auth_identity: Option<PreparedIdentity>,
    pub snapshot_account_identity: Option<PreparedIdentity>,
}

/// Identity claims a Codex account file produced. Match by
/// `provider_id` first, then by email.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedIdentity {
    pub provider_id: Option<String>,
    pub email: Option<String>,
}

impl PreparedIdentity {
    pub fn matches(&self, other: &PreparedIdentity) -> bool {
        match (self.provider_id.as_deref(), other.provider_id.as_deref()) {
            (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => a == b,
            _ => match (self.email.as_deref(), other.email.as_deref()) {
                (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => a.eq_ignore_ascii_case(b),
                _ => false,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedPromotionContext {
    /// The managed account the user is promoting to system-active.
    pub target: PreparedStoredManagedAccount,
    /// Every other managed account, in catalog order.
    pub other_managed: Vec<PreparedStoredManagedAccount>,
    /// The current live `~/.codex/auth.json` state.
    pub live: PreparedLiveAccount,
}

/// Output of the planner. Each variant carries a `reason` enum so the
/// service can log without exposing the user to debug strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromotionPlan {
    None {
        reason: NoneReason,
    },
    Reject {
        reason: RejectReason,
    },
    ImportNew {
        reason: ImportReason,
    },
    RefreshExisting {
        destination_id: String,
        reason: RefreshReason,
    },
    RepairExisting {
        destination_id: String,
        reason: RepairReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoneReason {
    LiveMissing,
    TargetMatchesLiveAuthIdentity,
    TargetMatchesSnapshotLiveAccount,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RejectReason {
    LiveUnreadable,
    LiveAPIKeyOnlyUnsupported,
    LiveIdentityMissingForPreservation,
    ConflictingReadableManagedHome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportReason {
    NoExistingManagedDestination,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefreshReason {
    ReadableHomeIdentityMatch,
    ReadableHomeIdentityMatchUsingPersistedEmailFallback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepairReason {
    PersistedProviderMatchWithMissingHome,
    PersistedProviderMatchWithUnreadableHome,
    PersistedLegacyEmailMatch,
}
