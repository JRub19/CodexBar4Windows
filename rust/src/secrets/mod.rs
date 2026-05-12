//! Secrets subsystem: DPAPI, secure files, blob stores, token accounts.
//!
//! Public surface is intentionally small. Phase 2 task 2.3 onward fills in
//! the modules listed below. Every provider in phase 4 and later goes
//! through `SecretBlobStore` and the (forthcoming) `TokenAccountStore`.

pub mod blob_store;
pub mod dpapi;
pub mod errors;
pub mod keyring_store;
pub mod secure_file;
pub mod token_account;

pub use blob_store::{FileSecretBlobStore, SecretBlobStore, SecretKey};
pub use dpapi::{dpapi_protect, dpapi_unprotect, unwrap_string, wrap_string};
pub use errors::SecretsError;
pub use keyring_store::CredentialManagerOAuthStore;
pub use secure_file::SecureFile;
pub use token_account::{ProviderTokenAccounts, TokenAccount, TokenAccountStore, TokenKind};
