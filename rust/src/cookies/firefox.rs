//! Firefox cookie reader.
//!
//! Firefox stores cookies in plaintext SQLite at
//! `%APPDATA%\Mozilla\Firefox\Profiles\<profile>\cookies.sqlite`. No
//! decryption needed; the only complication is that the DB is locked when
//! Firefox is open, so we copy to a temp dir before opening read only.

use std::path::{Path, PathBuf};

use super::detect::BrowserPresence;
use super::errors::ImportError;
use super::{BrowserCookieImporter, BrowserId, HttpCookie};

pub struct FirefoxCookieReader {
    presence: BrowserPresence,
}

impl FirefoxCookieReader {
    pub fn new(presence: BrowserPresence) -> Self {
        Self { presence }
    }

    fn copy_db(&self, source: &Path) -> Result<(tempfile::TempDir, PathBuf), ImportError> {
        let dir = tempfile::tempdir().map_err(|src| ImportError::Io {
            path: source.to_path_buf(),
            source: src,
        })?;
        let dest = dir.path().join("cookies.sqlite");
        std::fs::copy(source, &dest).map_err(|src| match src.kind() {
            std::io::ErrorKind::PermissionDenied => ImportError::DbLocked(BrowserId::Firefox),
            _ => ImportError::Io {
                path: source.to_path_buf(),
                source: src,
            },
        })?;
        Ok((dir, dest))
    }
}

impl BrowserCookieImporter for FirefoxCookieReader {
    fn browser(&self) -> BrowserId {
        BrowserId::Firefox
    }

    fn import_for(&self, domains: &[&str]) -> Result<Vec<HttpCookie>, ImportError> {
        let cookie_db = self
            .presence
            .cookie_db_path
            .as_deref()
            .ok_or(ImportError::BrowserNotInstalled(BrowserId::Firefox))?;
        let (_temp, temp_db) = self.copy_db(cookie_db)?;

        let conn = rusqlite::Connection::open_with_flags(
            &temp_db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| {
            let m = e.to_string().to_lowercase();
            if m.contains("busy") || m.contains("locked") {
                ImportError::DbLocked(BrowserId::Firefox)
            } else {
                ImportError::Sqlite(e.to_string())
            }
        })?;

        let mut out = Vec::new();
        for host_pattern in domains {
            let mut stmt = conn
                .prepare(
                    "SELECT host, name, value, path, isSecure, isHttpOnly \
                     FROM moz_cookies WHERE host LIKE ?1",
                )
                .map_err(|e| ImportError::Sqlite(e.to_string()))?;
            let pattern = if host_pattern.starts_with('.') || host_pattern.contains('%') {
                host_pattern.to_string()
            } else {
                format!("%{host_pattern}")
            };
            let rows = stmt
                .query_map([&pattern], |row| {
                    Ok(HttpCookie {
                        host: row.get::<_, String>(0)?,
                        name: row.get::<_, String>(1)?,
                        value: row.get::<_, String>(2)?,
                        path: row.get::<_, String>(3)?,
                        is_secure: row.get::<_, i64>(4)? != 0,
                        is_http_only: row.get::<_, i64>(5)? != 0,
                    })
                })
                .map_err(|e| ImportError::Sqlite(e.to_string()))?;
            for cookie in rows {
                out.push(cookie.map_err(|e| ImportError::Sqlite(e.to_string()))?);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_db(dir: &Path) -> PathBuf {
        let path = dir.join("cookies.sqlite");
        let conn = rusqlite::Connection::open(&path).expect("open test db");
        conn.execute(
            "CREATE TABLE moz_cookies (
                host TEXT, name TEXT, value TEXT, path TEXT,
                isSecure INTEGER, isHttpOnly INTEGER
            )",
            [],
        )
        .expect("create");
        conn.execute(
            "INSERT INTO moz_cookies VALUES ('.example.com', 'session', 'abc', '/', 1, 1)",
            [],
        )
        .expect("insert");
        conn.execute(
            "INSERT INTO moz_cookies VALUES ('.other.com', 'tracker', 'xyz', '/', 0, 0)",
            [],
        )
        .expect("insert");
        path
    }

    #[test]
    fn reads_matching_rows_only() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = make_test_db(tmp.path());
        let reader = FirefoxCookieReader {
            presence: BrowserPresence {
                browser: BrowserId::Firefox,
                local_state_path: None,
                profile_root: Some(tmp.path().to_path_buf()),
                cookie_db_path: Some(db),
            },
        };
        let cookies = reader.import_for(&["example.com"]).expect("import");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "session");
        assert_eq!(cookies[0].value, "abc");
        assert!(cookies[0].is_secure);
        assert!(cookies[0].is_http_only);
    }

    #[test]
    fn missing_browser_returns_not_installed() {
        let reader = FirefoxCookieReader {
            presence: BrowserPresence {
                browser: BrowserId::Firefox,
                local_state_path: None,
                profile_root: None,
                cookie_db_path: None,
            },
        };
        let err = reader.import_for(&["example.com"]).unwrap_err();
        assert!(matches!(err, ImportError::BrowserNotInstalled(_)));
    }
}
