//! Interaction guards for the promotion flow. Spec 41 §6.11 requires
//! that we refuse to start a promotion while:
//!
//! - Another promotion is in flight.
//! - A managed-account add/remove is in flight.
//! - The live system account is in the middle of authentication.
//!
//! The block message is the verbatim string from spec 41 §6.10.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::errors::CodexAccountPromotionError;

#[derive(Debug, Default)]
pub struct PromotionCoordinator {
    is_authenticating_live: AtomicBool,
    is_promoting_system: AtomicBool,
    has_conflicting_managed_op: AtomicBool,
}

impl PromotionCoordinator {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set_authenticating_live(&self, value: bool) {
        self.is_authenticating_live.store(value, Ordering::SeqCst);
    }

    pub fn set_conflicting_managed_op(&self, value: bool) {
        self.has_conflicting_managed_op
            .store(value, Ordering::SeqCst);
    }

    /// Attempt to start a promotion. Returns the guard that resets
    /// `is_promoting_system` on drop. Returns an
    /// `InteractionBlocked` error when another operation is already in
    /// flight.
    pub fn begin_promotion(self: &Arc<Self>) -> Result<PromotionGuard, CodexAccountPromotionError> {
        if self.is_authenticating_live.load(Ordering::SeqCst)
            || self.has_conflicting_managed_op.load(Ordering::SeqCst)
        {
            return Err(CodexAccountPromotionError::InteractionBlocked);
        }
        if self
            .is_promoting_system
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(CodexAccountPromotionError::InteractionBlocked);
        }
        Ok(PromotionGuard {
            coordinator: self.clone(),
        })
    }
}

#[derive(Debug)]
pub struct PromotionGuard {
    coordinator: Arc<PromotionCoordinator>,
}

impl Drop for PromotionGuard {
    fn drop(&mut self) {
        self.coordinator
            .is_promoting_system
            .store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_promotion_succeeds() {
        let coordinator = PromotionCoordinator::new();
        let _guard = coordinator.begin_promotion().unwrap();
    }

    #[test]
    fn second_concurrent_promotion_is_blocked() {
        let coordinator = PromotionCoordinator::new();
        let guard = coordinator.begin_promotion().unwrap();
        let err = coordinator.begin_promotion().unwrap_err();
        assert_eq!(err, CodexAccountPromotionError::InteractionBlocked);
        drop(guard);
    }

    #[test]
    fn live_authentication_blocks_promotion() {
        let coordinator = PromotionCoordinator::new();
        coordinator.set_authenticating_live(true);
        let err = coordinator.begin_promotion().unwrap_err();
        assert_eq!(err, CodexAccountPromotionError::InteractionBlocked);
    }

    #[test]
    fn managed_op_in_flight_blocks_promotion() {
        let coordinator = PromotionCoordinator::new();
        coordinator.set_conflicting_managed_op(true);
        let err = coordinator.begin_promotion().unwrap_err();
        assert_eq!(err, CodexAccountPromotionError::InteractionBlocked);
    }

    #[test]
    fn dropping_guard_releases_promotion_slot() {
        let coordinator = PromotionCoordinator::new();
        {
            let _ = coordinator.begin_promotion().unwrap();
        }
        let _next = coordinator.begin_promotion().unwrap();
    }
}
