//! Secrets subsystem: DPAPI, secure files, blob stores, token accounts.
//!
//! Public surface is intentionally small. Phase 2 task 2.3 onward fills in
//! the modules listed below. Every provider in phase 4 and later goes
//! through `SecretBlobStore` and the (forthcoming) `TokenAccountStore`.

pub mod dpapi;
pub mod errors;
pub mod secure_file;

pub use dpapi::{dpapi_protect, dpapi_unprotect, unwrap_string, wrap_string};
pub use errors::SecretsError;
pub use secure_file::SecureFile;
