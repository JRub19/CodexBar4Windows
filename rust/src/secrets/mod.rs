//! Secrets subsystem: DPAPI, secure files, blob stores, token accounts.
//!
//! Public surface is intentionally small. Phase 2 task 2.3 onward fills in
//! the modules listed below. Every provider in phase 4 and later goes
//! through `SecretBlobStore` and the (forthcoming) `TokenAccountStore`.

pub mod errors;

pub use errors::SecretsError;
