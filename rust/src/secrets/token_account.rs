//! Per provider token account store.
//!
//! Each provider may have multiple credential entries: a personal OAuth
//! token, a team API key, a pasted manual cookie header, and so on. The
//! user picks one as `active`; providers read the active one through
//! `TokenAccountStore::active_for`.
//!
//! Storage layout: one DPAPI wrapped JSON blob per provider, stored via
//! [`FileSecretBlobStore`] in the `token-accounts` category. The whole
//! list (including raw token values) is wrapped at rest; values are never
//! serialized to disk in plaintext.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::blob_store::{FileSecretBlobStore, SecretBlobStore, SecretKey};
use super::errors::SecretsError;

const CATEGORY: &str = "token-accounts";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    Cookie,
    OauthToken,
    ApiKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenAccount {
    pub id: String,
    pub kind: TokenKind,
    pub label: String,
    pub value: String,
    pub created_at_unix_secs: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderTokenAccounts {
    pub provider_id: String,
    pub accounts: Vec<TokenAccount>,
    pub active_id: Option<String>,
}

pub struct TokenAccountStore {
    blob_store: FileSecretBlobStore,
}

impl TokenAccountStore {
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            blob_store: FileSecretBlobStore::new(root),
        }
    }

    pub fn load(&self, provider_id: &str) -> Result<ProviderTokenAccounts, SecretsError> {
        let key = SecretKey::new(CATEGORY, provider_id);
        match self.blob_store.read(&key)? {
            Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
            None => Ok(ProviderTokenAccounts {
                provider_id: provider_id.to_string(),
                accounts: Vec::new(),
                active_id: None,
            }),
        }
    }

    fn save(&self, list: &ProviderTokenAccounts) -> Result<(), SecretsError> {
        let key = SecretKey::new(CATEGORY, &list.provider_id);
        let bytes = serde_json::to_vec(list)?;
        self.blob_store.write(&key, &bytes)
    }

    pub fn add(
        &self,
        provider_id: &str,
        kind: TokenKind,
        label: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<TokenAccount, SecretsError> {
        let mut list = self.load(provider_id)?;
        let account = TokenAccount {
            id: new_id(),
            kind,
            label: label.into(),
            value: value.into(),
            created_at_unix_secs: now_unix_secs(),
        };
        // First account becomes active by default.
        if list.active_id.is_none() {
            list.active_id = Some(account.id.clone());
        }
        list.accounts.push(account.clone());
        self.save(&list)?;
        Ok(account)
    }

    pub fn edit(
        &self,
        provider_id: &str,
        account_id: &str,
        label: Option<String>,
        value: Option<String>,
    ) -> Result<TokenAccount, SecretsError> {
        let mut list = self.load(provider_id)?;
        let account = list
            .accounts
            .iter_mut()
            .find(|a| a.id == account_id)
            .ok_or(SecretsError::EmptyIdentifier)?;
        if let Some(l) = label {
            account.label = l;
        }
        if let Some(v) = value {
            account.value = v;
        }
        let updated = account.clone();
        self.save(&list)?;
        Ok(updated)
    }

    pub fn remove(&self, provider_id: &str, account_id: &str) -> Result<(), SecretsError> {
        let mut list = self.load(provider_id)?;
        list.accounts.retain(|a| a.id != account_id);
        if list.active_id.as_deref() == Some(account_id) {
            list.active_id = list.accounts.first().map(|a| a.id.clone());
        }
        self.save(&list)
    }

    pub fn set_active(&self, provider_id: &str, account_id: &str) -> Result<(), SecretsError> {
        let mut list = self.load(provider_id)?;
        if !list.accounts.iter().any(|a| a.id == account_id) {
            return Err(SecretsError::EmptyIdentifier);
        }
        list.active_id = Some(account_id.to_string());
        self.save(&list)
    }

    pub fn active_for(&self, provider_id: &str) -> Result<Option<TokenAccount>, SecretsError> {
        let list = self.load(provider_id)?;
        Ok(list
            .active_id
            .and_then(|id| list.accounts.into_iter().find(|a| a.id == id)))
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn new_id() -> String {
    // Phase 1 minimal ID generator: high-resolution timestamp plus a small
    // process counter. Phase 4 may switch to uuid v7 when we add the dep.
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{nanos:x}-{n:x}")
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn add_load_round_trips_a_cookie_account() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = TokenAccountStore::new(tmp.path());
        let acct = store
            .add(
                "claude",
                TokenKind::Cookie,
                "personal",
                "sessionKey=sk-ant-abc",
            )
            .expect("add");
        let list = store.load("claude").expect("load");
        assert_eq!(list.accounts.len(), 1);
        assert_eq!(list.accounts[0], acct);
        assert_eq!(list.active_id.as_deref(), Some(acct.id.as_str()));
    }

    #[test]
    fn second_add_does_not_change_active() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = TokenAccountStore::new(tmp.path());
        let first = store
            .add("claude", TokenKind::Cookie, "personal", "sk-ant-1")
            .expect("first");
        let _second = store
            .add("claude", TokenKind::OauthToken, "team", "sk-ant-oat-2")
            .expect("second");
        let list = store.load("claude").expect("load");
        assert_eq!(list.active_id.as_deref(), Some(first.id.as_str()));
    }

    #[test]
    fn remove_active_falls_back_to_first_remaining() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = TokenAccountStore::new(tmp.path());
        let first = store
            .add("claude", TokenKind::Cookie, "a", "1")
            .expect("first");
        let second = store
            .add("claude", TokenKind::Cookie, "b", "2")
            .expect("second");
        store.remove("claude", &first.id).expect("remove");
        let list = store.load("claude").expect("load");
        assert_eq!(list.active_id.as_deref(), Some(second.id.as_str()));
    }

    #[test]
    fn set_active_changes_the_pointer() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = TokenAccountStore::new(tmp.path());
        let first = store
            .add("claude", TokenKind::Cookie, "a", "1")
            .expect("first");
        let second = store
            .add("claude", TokenKind::Cookie, "b", "2")
            .expect("second");
        store.set_active("claude", &second.id).expect("set active");
        let active = store
            .active_for("claude")
            .expect("active for")
            .expect("present");
        assert_eq!(active.id, second.id);
        assert_ne!(active.id, first.id);
    }

    #[test]
    fn edit_partial_update_preserves_other_fields() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = TokenAccountStore::new(tmp.path());
        let acct = store
            .add("claude", TokenKind::Cookie, "old", "old-value")
            .expect("add");
        let updated = store
            .edit("claude", &acct.id, Some("new".into()), None)
            .expect("edit");
        assert_eq!(updated.label, "new");
        assert_eq!(updated.value, "old-value");
    }

    #[test]
    fn load_for_unknown_provider_returns_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = TokenAccountStore::new(tmp.path());
        let list = store.load("unknown").expect("load");
        assert!(list.accounts.is_empty());
        assert!(list.active_id.is_none());
    }
}
