//! Shared provider error type.
//!
//! Phase 1 keeps the variants minimal. Phase 4 adds typed retries, auth
//! repair hints, and per strategy detail.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("duplicate provider id: {0}")]
    DuplicateId(&'static str),
    #[error("provider {0} is not registered")]
    Unknown(&'static str),
}
