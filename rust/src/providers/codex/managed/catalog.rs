//! Managed Codex account catalog. Spec 41 §6.4 fixes the schema.
//! v1 rows are migrated to v2 on read; the on-disk file is rewritten
//! on the next save. Both versions live under
//! `%LOCALAPPDATA%\CodexBar4Windows\managed-codex-accounts.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION_V1: u8 = 1;
pub const SCHEMA_VERSION_V2: u8 = 2;
pub const CURRENT_SCHEMA: u8 = SCHEMA_VERSION_V2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedCatalogV2 {
    pub schema_version: u8,
    #[serde(default)]
    pub accounts: Vec<ManagedAccountRow>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedAccountRow {
    pub id: String,
    pub email: Option<String>,
    /// Stable provider-side identifier (JWT `chatgpt_account_id` or the
    /// account_id field from the local `auth.json`). The v1 schema did
    /// not store this; the migration hydrates it from the local JWT.
    pub provider_account_id: Option<String>,
    pub workspace_label: Option<String>,
    pub workspace_account_id: Option<String>,
    pub managed_home_path: PathBuf,
    pub created_at_unix_secs: i64,
    pub updated_at_unix_secs: i64,
    pub last_authenticated_at_unix_secs: Option<i64>,
}

impl ManagedAccountRow {
    fn merge_key(&self) -> MergeKey {
        if let Some(provider_id) = self.provider_account_id.as_ref().filter(|s| !s.is_empty()) {
            MergeKey::ProviderId {
                email: normalize_email(&self.email),
                provider_account_id: provider_id.clone(),
            }
        } else if let Some(email) = self.email.as_ref().filter(|s| !s.is_empty()) {
            MergeKey::LegacyEmail {
                email: normalize_email(&Some(email.clone())),
            }
        } else {
            MergeKey::Id(self.id.clone())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum MergeKey {
    ProviderId {
        email: Option<String>,
        provider_account_id: String,
    },
    LegacyEmail {
        email: Option<String>,
    },
    Id(String),
}

fn normalize_email(email: &Option<String>) -> Option<String> {
    email
        .as_ref()
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| !e.is_empty())
}

/// v1 row shape. The schema kept only `email` + `managedHomePath` +
/// timestamps; the provider account id is hydrated from disk during
/// migration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedAccountRowV1 {
    pub id: String,
    #[serde(default)]
    pub email: Option<String>,
    pub managed_home_path: PathBuf,
    pub created_at_unix_secs: i64,
    #[serde(default)]
    pub updated_at_unix_secs: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UntypedWrapper {
    #[serde(default, alias = "schemaVersion")]
    schema_version: Option<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("catalog read failed at {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("catalog write failed at {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("catalog payload not valid JSON: {0}")]
    Decode(String),
    #[error("unsupported schema version {0}")]
    UnsupportedSchema(u8),
}

pub struct CatalogStore {
    path: PathBuf,
}

impl CatalogStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read(&self) -> Result<ManagedCatalogV2, CatalogError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => decode_dual_version(&bytes),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(ManagedCatalogV2 {
                schema_version: CURRENT_SCHEMA,
                accounts: Vec::new(),
            }),
            Err(source) => Err(CatalogError::Read {
                path: self.path.clone(),
                source,
            }),
        }
    }

    pub fn write(&self, catalog: &ManagedCatalogV2) -> Result<(), CatalogError> {
        let bytes =
            serde_json::to_vec_pretty(catalog).map_err(|e| CatalogError::Decode(e.to_string()))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| CatalogError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        atomic_write(&self.path, &bytes).map_err(|source| CatalogError::Write {
            path: self.path.clone(),
            source,
        })
    }
}

pub fn decode_dual_version(bytes: &[u8]) -> Result<ManagedCatalogV2, CatalogError> {
    let wrapper: UntypedWrapper =
        serde_json::from_slice(bytes).map_err(|e| CatalogError::Decode(e.to_string()))?;
    match wrapper.schema_version.unwrap_or(SCHEMA_VERSION_V1) {
        SCHEMA_VERSION_V2 => {
            serde_json::from_slice(bytes).map_err(|e| CatalogError::Decode(e.to_string()))
        }
        SCHEMA_VERSION_V1 => {
            #[derive(Deserialize)]
            struct V1Wrapper {
                #[serde(default)]
                accounts: Vec<ManagedAccountRowV1>,
            }
            let v1: V1Wrapper =
                serde_json::from_slice(bytes).map_err(|e| CatalogError::Decode(e.to_string()))?;
            Ok(migrate_v1(v1.accounts))
        }
        other => Err(CatalogError::UnsupportedSchema(other)),
    }
}

/// Migrate a list of v1 rows into v2. `provider_account_id` stays
/// `None` until the caller hydrates it from each row's `auth.json`.
pub fn migrate_v1(rows: Vec<ManagedAccountRowV1>) -> ManagedCatalogV2 {
    let accounts = rows
        .into_iter()
        .map(|v1| ManagedAccountRow {
            id: v1.id,
            email: v1.email,
            provider_account_id: None,
            workspace_label: None,
            workspace_account_id: None,
            managed_home_path: v1.managed_home_path,
            created_at_unix_secs: v1.created_at_unix_secs,
            updated_at_unix_secs: v1.updated_at_unix_secs.unwrap_or(v1.created_at_unix_secs),
            last_authenticated_at_unix_secs: None,
        })
        .collect();
    sanitize(ManagedCatalogV2 {
        schema_version: CURRENT_SCHEMA,
        accounts,
    })
}

/// Drop duplicates by merge key. First-seen wins, mirroring spec 41
/// §6.4 ordering rules.
pub fn sanitize(catalog: ManagedCatalogV2) -> ManagedCatalogV2 {
    let mut seen: std::collections::HashSet<MergeKey> = std::collections::HashSet::new();
    let mut deduped: Vec<ManagedAccountRow> = Vec::new();
    for row in catalog.accounts {
        let key = row.merge_key();
        if seen.insert(key) {
            deduped.push(row);
        }
    }
    ManagedCatalogV2 {
        schema_version: CURRENT_SCHEMA,
        accounts: deduped,
    }
}

fn atomic_write(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = target.with_extension(format!("json.tmp.{nanos}"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_v2(id: &str, email: Option<&str>, provider_id: Option<&str>) -> ManagedAccountRow {
        ManagedAccountRow {
            id: id.into(),
            email: email.map(|s| s.to_string()),
            provider_account_id: provider_id.map(|s| s.to_string()),
            workspace_label: None,
            workspace_account_id: None,
            managed_home_path: PathBuf::from(format!("/managed/{id}")),
            created_at_unix_secs: 1,
            updated_at_unix_secs: 1,
            last_authenticated_at_unix_secs: None,
        }
    }

    #[test]
    fn decodes_v2_payload_directly() {
        let payload = br#"{
            "schemaVersion": 2,
            "accounts": [
                {
                    "id": "acct-1",
                    "email": "u@example.com",
                    "providerAccountId": "pa-1",
                    "managedHomePath": "/managed/acct-1",
                    "createdAtUnixSecs": 1,
                    "updatedAtUnixSecs": 2
                }
            ]
        }"#;
        let catalog = decode_dual_version(payload).unwrap();
        assert_eq!(catalog.schema_version, 2);
        assert_eq!(catalog.accounts.len(), 1);
        assert_eq!(
            catalog.accounts[0].provider_account_id.as_deref(),
            Some("pa-1")
        );
    }

    #[test]
    fn decodes_v1_payload_and_promotes_to_v2() {
        let payload = br#"{
            "schemaVersion": 1,
            "accounts": [
                {
                    "id": "acct-1",
                    "email": "u@example.com",
                    "managedHomePath": "/managed/acct-1",
                    "createdAtUnixSecs": 100
                }
            ]
        }"#;
        let catalog = decode_dual_version(payload).unwrap();
        assert_eq!(catalog.schema_version, 2);
        assert_eq!(catalog.accounts.len(), 1);
        assert!(catalog.accounts[0].provider_account_id.is_none());
        assert_eq!(catalog.accounts[0].updated_at_unix_secs, 100);
    }

    #[test]
    fn schema_version_missing_treats_payload_as_v1() {
        let payload = br#"{
            "accounts": [
                {
                    "id": "acct-1",
                    "email": "u@example.com",
                    "managedHomePath": "/managed/acct-1",
                    "createdAtUnixSecs": 100,
                    "updatedAtUnixSecs": 200
                }
            ]
        }"#;
        let catalog = decode_dual_version(payload).unwrap();
        assert_eq!(catalog.accounts[0].updated_at_unix_secs, 200);
    }

    #[test]
    fn duplicate_provider_ids_collapse_to_first_seen() {
        let catalog = ManagedCatalogV2 {
            schema_version: CURRENT_SCHEMA,
            accounts: vec![
                row_v2("a", Some("u@x.com"), Some("pa-1")),
                row_v2("b", Some("u@x.com"), Some("pa-1")),
                row_v2("c", Some("v@x.com"), Some("pa-2")),
            ],
        };
        let deduped = sanitize(catalog);
        assert_eq!(deduped.accounts.len(), 2);
        assert_eq!(deduped.accounts[0].id, "a");
        assert_eq!(deduped.accounts[1].id, "c");
    }

    #[test]
    fn legacy_email_duplicates_collapse() {
        let catalog = ManagedCatalogV2 {
            schema_version: CURRENT_SCHEMA,
            accounts: vec![
                row_v2("a", Some("Shared@x.com"), None),
                row_v2("b", Some("shared@x.com"), None),
            ],
        };
        let deduped = sanitize(catalog);
        assert_eq!(deduped.accounts.len(), 1);
        assert_eq!(deduped.accounts[0].id, "a");
    }

    #[test]
    fn store_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(dir.path().join("managed.json"));
        let catalog = ManagedCatalogV2 {
            schema_version: CURRENT_SCHEMA,
            accounts: vec![row_v2("a", Some("u@x.com"), Some("pa-1"))],
        };
        store.write(&catalog).unwrap();
        let back = store.read().unwrap();
        assert_eq!(back, catalog);
    }

    #[test]
    fn missing_file_returns_empty_v2_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(dir.path().join("missing.json"));
        let catalog = store.read().unwrap();
        assert_eq!(catalog.schema_version, CURRENT_SCHEMA);
        assert!(catalog.accounts.is_empty());
    }

    #[test]
    fn migration_is_idempotent() {
        let v1 = br#"{
            "schemaVersion": 1,
            "accounts": [
                {
                    "id": "a",
                    "email": "u@x.com",
                    "managedHomePath": "/managed/a",
                    "createdAtUnixSecs": 1
                }
            ]
        }"#;
        let first = decode_dual_version(v1).unwrap();
        let bytes = serde_json::to_vec_pretty(&first).unwrap();
        let second = decode_dual_version(&bytes).unwrap();
        let bytes2 = serde_json::to_vec_pretty(&second).unwrap();
        assert_eq!(
            bytes, bytes2,
            "second migration must yield byte-identical output"
        );
    }
}
