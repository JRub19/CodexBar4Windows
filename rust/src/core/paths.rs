//! Canonical Windows path layout for CodexBar4Windows.
//!
//! - `%APPDATA%\CodexBar4Windows\config.json`
//! - `%APPDATA%\CodexBar4Windows\secrets\`   (Phase 2 tightens DACL further)
//! - `%LOCALAPPDATA%\CodexBar4Windows\cache\`
//! - `%LOCALAPPDATA%\CodexBar4Windows\logs\`
//!
//! On non Windows builds (CI Linux runs of the shared crate) the layout maps
//! to `$HOME/.config/CodexBar4Windows` and `$HOME/.cache/CodexBar4Windows`
//! so unit tests stay portable. Per phase 1 plan §Risks, ACL hardening on
//! `secrets\` beyond the default `%APPDATA%` user scope is deferred to
//! phase 2 (auth subsystem) where DPAPI and credential storage land.

use std::path::{Path, PathBuf};

use thiserror::Error;

pub const APP_DIR_NAME: &str = "CodexBar4Windows";

#[derive(Debug, Error)]
pub enum PathError {
    #[error("environment variable {0} is unset or empty")]
    MissingEnv(&'static str),
    #[error("could not create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathEnvironment {
    pub roaming: PathBuf,
    pub local: PathBuf,
    pub config_file: PathBuf,
    pub secrets_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl PathEnvironment {
    pub fn discover() -> Result<Self, PathError> {
        let roaming = roaming_app_dir()?;
        let local = local_app_dir()?;
        Ok(Self {
            config_file: roaming.join("config.json"),
            secrets_dir: roaming.join("secrets"),
            cache_dir: local.join("cache"),
            logs_dir: local.join("logs"),
            roaming,
            local,
        })
    }

    pub fn ensure(&self) -> Result<(), PathError> {
        create_dir(&self.roaming)?;
        create_dir(&self.local)?;
        create_dir(&self.secrets_dir)?;
        create_dir(&self.cache_dir)?;
        create_dir(&self.logs_dir)?;
        Ok(())
    }
}

#[cfg(windows)]
fn roaming_app_dir() -> Result<PathBuf, PathError> {
    Ok(std::env::var_os("APPDATA")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .ok_or(PathError::MissingEnv("APPDATA"))?
        .join(APP_DIR_NAME))
}

#[cfg(windows)]
fn local_app_dir() -> Result<PathBuf, PathError> {
    Ok(std::env::var_os("LOCALAPPDATA")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .ok_or(PathError::MissingEnv("LOCALAPPDATA"))?
        .join(APP_DIR_NAME))
}

#[cfg(not(windows))]
fn roaming_app_dir() -> Result<PathBuf, PathError> {
    Ok(std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .ok_or(PathError::MissingEnv("HOME"))?
        .join(".config")
        .join(APP_DIR_NAME))
}

#[cfg(not(windows))]
fn local_app_dir() -> Result<PathBuf, PathError> {
    Ok(std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .ok_or(PathError::MissingEnv("HOME"))?
        .join(".cache")
        .join(APP_DIR_NAME))
}

fn create_dir(path: &Path) -> Result<(), PathError> {
    std::fs::create_dir_all(path).map_err(|source| PathError::CreateDir {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_uses_app_dir_name() {
        let env = PathEnvironment::discover().expect("discover should succeed in this env");
        assert!(env.roaming.ends_with(APP_DIR_NAME));
        assert!(env.local.ends_with(APP_DIR_NAME));
        assert!(env.config_file.ends_with("config.json"));
        assert_eq!(env.secrets_dir.file_name().unwrap(), "secrets");
        assert_eq!(env.cache_dir.file_name().unwrap(), "cache");
        assert_eq!(env.logs_dir.file_name().unwrap(), "logs");
    }

    #[test]
    fn ensure_creates_directories_in_a_sandbox() {
        let temp = tempfile::tempdir().expect("tempdir");
        let env = PathEnvironment {
            roaming: temp.path().join("roaming"),
            local: temp.path().join("local"),
            config_file: temp.path().join("roaming/config.json"),
            secrets_dir: temp.path().join("roaming/secrets"),
            cache_dir: temp.path().join("local/cache"),
            logs_dir: temp.path().join("local/logs"),
        };
        env.ensure().expect("ensure should create directories");
        assert!(env.roaming.is_dir());
        assert!(env.local.is_dir());
        assert!(env.secrets_dir.is_dir());
        assert!(env.cache_dir.is_dir());
        assert!(env.logs_dir.is_dir());
    }
}
