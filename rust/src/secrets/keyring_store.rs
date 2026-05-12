//! Thin wrapper around the Windows Credential Manager via the `keyring`
//! crate. Used as a *convenience* mirror for OAuth refresh tokens so other
//! tools (`gh auth`, `claude` CLI) can see the same credential when sharing
//! is desired. The file backed blob store remains canonical; this layer is
//! best effort and is allowed to fail silently in production.

use super::errors::SecretsError;

const DEFAULT_SERVICE: &str = "CodexBar4Windows";

pub struct CredentialManagerOAuthStore {
    service: String,
}

impl Default for CredentialManagerOAuthStore {
    fn default() -> Self {
        Self::new(DEFAULT_SERVICE)
    }
}

impl CredentialManagerOAuthStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, provider: &str) -> Result<keyring::Entry, SecretsError> {
        let target = format!("oauth/{provider}/refresh_token");
        keyring::Entry::new(&self.service, &target)
            .map_err(|e| SecretsError::CredentialManager(e.to_string()))
    }

    pub fn read_refresh_token(&self, provider: &str) -> Result<Option<String>, SecretsError> {
        let entry = self.entry(provider)?;
        match entry.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretsError::CredentialManager(e.to_string())),
        }
    }

    pub fn write_refresh_token(&self, provider: &str, token: &str) -> Result<(), SecretsError> {
        let entry = self.entry(provider)?;
        entry
            .set_password(token)
            .map_err(|e| SecretsError::CredentialManager(e.to_string()))
    }

    pub fn delete_refresh_token(&self, provider: &str) -> Result<(), SecretsError> {
        let entry = self.entry(provider)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretsError::CredentialManager(e.to_string())),
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn write_read_delete_round_trip() {
        let store = CredentialManagerOAuthStore::new("CodexBar4Windows-test");
        let provider = format!("test-{}", std::process::id());
        let token = "dpapi:v1:not-a-real-token";
        store
            .write_refresh_token(&provider, token)
            .expect("write to credential manager");
        let read = store
            .read_refresh_token(&provider)
            .expect("read from credential manager")
            .expect("present");
        assert_eq!(read, token);
        store
            .delete_refresh_token(&provider)
            .expect("delete from credential manager");
        assert!(store.read_refresh_token(&provider).unwrap().is_none());
    }
}
