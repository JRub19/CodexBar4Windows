//! Verbatim error string table for the Codex account promotion flow.
//! Spec 41 §6.10 lists every string. The Windows variant of
//! `liveAccountUnreadable` uses "on this PC" instead of macOS's "on
//! this Mac".

use thiserror::Error;

pub const ALERT_TITLE: &str = "Could not switch system account";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CodexAccountPromotionError {
    #[error(
        "That account is no longer available in CodexBar. Refresh the account list and try again."
    )]
    TargetManagedAccountNotFound,
    #[error(
        "CodexBar could not find saved auth for that account. Re-authenticate it and try again."
    )]
    TargetManagedAccountAuthMissing,
    #[error(
        "CodexBar could not read saved auth for that account. Re-authenticate it and try again."
    )]
    TargetManagedAccountAuthUnreadable,
    #[error("CodexBar could not read the current system account on this PC.")]
    LiveAccountUnreadable,
    #[error("CodexBar could not safely preserve the current system account before switching.")]
    LiveAccountMissingIdentityForPreservation,
    #[error(
        "CodexBar can't replace a system account that is signed in with an API key only setup."
    )]
    LiveAccountAPIKeyOnlyUnsupported,
    #[error("CodexBar found another managed account that already uses the current system account. Resolve duplicate first.")]
    DisplacedLiveManagedAccountConflict,
    #[error("CodexBar could not save the current system account before switching.")]
    DisplacedLiveImportFailed,
    #[error("CodexBar could not update managed account storage.")]
    ManagedStoreCommitFailed,
    #[error("CodexBar could not replace the live Codex auth on this PC.")]
    LiveAuthSwapFailed,
    #[error("Finish the current managed account change before switching the system account.")]
    InteractionBlocked,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_strings_pin_to_spec_41_section_6_10() {
        let pairs: &[(CodexAccountPromotionError, &str)] = &[
            (
                CodexAccountPromotionError::TargetManagedAccountNotFound,
                "That account is no longer available in CodexBar. Refresh the account list and try again.",
            ),
            (
                CodexAccountPromotionError::TargetManagedAccountAuthMissing,
                "CodexBar could not find saved auth for that account. Re-authenticate it and try again.",
            ),
            (
                CodexAccountPromotionError::TargetManagedAccountAuthUnreadable,
                "CodexBar could not read saved auth for that account. Re-authenticate it and try again.",
            ),
            (
                CodexAccountPromotionError::LiveAccountUnreadable,
                "CodexBar could not read the current system account on this PC.",
            ),
            (
                CodexAccountPromotionError::LiveAccountMissingIdentityForPreservation,
                "CodexBar could not safely preserve the current system account before switching.",
            ),
            (
                CodexAccountPromotionError::LiveAccountAPIKeyOnlyUnsupported,
                "CodexBar can't replace a system account that is signed in with an API key only setup.",
            ),
            (
                CodexAccountPromotionError::DisplacedLiveManagedAccountConflict,
                "CodexBar found another managed account that already uses the current system account. Resolve duplicate first.",
            ),
            (
                CodexAccountPromotionError::DisplacedLiveImportFailed,
                "CodexBar could not save the current system account before switching.",
            ),
            (
                CodexAccountPromotionError::ManagedStoreCommitFailed,
                "CodexBar could not update managed account storage.",
            ),
            (
                CodexAccountPromotionError::LiveAuthSwapFailed,
                "CodexBar could not replace the live Codex auth on this PC.",
            ),
            (
                CodexAccountPromotionError::InteractionBlocked,
                "Finish the current managed account change before switching the system account.",
            ),
        ];
        for (err, expected) in pairs {
            assert_eq!(err.to_string(), *expected, "mismatch for {err:?}");
        }
    }

    #[test]
    fn alert_title_matches_spec() {
        assert_eq!(ALERT_TITLE, "Could not switch system account");
    }
}
