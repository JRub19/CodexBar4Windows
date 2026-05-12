//! Find the `codex` binary on a Windows host. Spec 41 §4.1 lookup
//! order:
//!
//! 1. `%PATH%`
//! 2. `%LOCALAPPDATA%\Programs\codex\codex.exe`
//! 3. `%USERPROFILE%\.bun\bin\codex.exe`
//!
//! The locator is filesystem-pluggable: tests inject a fake
//! `Filesystem` to drive every arm without writing real files.

use std::path::{Path, PathBuf};

pub const BINARY_NAME: &str = if cfg!(windows) { "codex.exe" } else { "codex" };

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BinaryNotFoundError {
    #[error(
        "codex binary not found on PATH, in %LOCALAPPDATA%\\Programs\\codex, or in %USERPROFILE%\\.bun\\bin"
    )]
    Missing,
}

pub trait Filesystem {
    fn exists(&self, path: &Path) -> bool;
}

pub struct OsFilesystem;

impl Filesystem for OsFilesystem {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

/// Environment hooks. Tests inject a stub so the locator can be driven
/// without manipulating real env vars.
pub trait Env {
    fn var(&self, key: &str) -> Option<String>;
}

pub struct OsEnv;

impl Env for OsEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

pub fn locate_with(env: &dyn Env, fs: &dyn Filesystem) -> Result<PathBuf, BinaryNotFoundError> {
    if let Some(path) = locate_on_path(env, fs) {
        return Ok(path);
    }
    if let Some(local) = env.var("LOCALAPPDATA") {
        let candidate = PathBuf::from(local)
            .join("Programs")
            .join("codex")
            .join(BINARY_NAME);
        if fs.exists(&candidate) {
            return Ok(candidate);
        }
    }
    if let Some(profile) = env.var("USERPROFILE") {
        let candidate = PathBuf::from(profile)
            .join(".bun")
            .join("bin")
            .join(BINARY_NAME);
        if fs.exists(&candidate) {
            return Ok(candidate);
        }
    }
    Err(BinaryNotFoundError::Missing)
}

fn locate_on_path(env: &dyn Env, fs: &dyn Filesystem) -> Option<PathBuf> {
    let path = env.var("PATH")?;
    let sep = if cfg!(windows) { ';' } else { ':' };
    for segment in path.split(sep) {
        if segment.is_empty() {
            continue;
        }
        let candidate = PathBuf::from(segment).join(BINARY_NAME);
        if fs.exists(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Convenience entry point for production callers.
pub fn locate() -> Result<PathBuf, BinaryNotFoundError> {
    locate_with(&OsEnv, &OsFilesystem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeFs(Vec<PathBuf>);
    impl Filesystem for FakeFs {
        fn exists(&self, path: &Path) -> bool {
            self.0.iter().any(|p| p == path)
        }
    }

    struct FakeEnv(HashMap<String, String>);
    impl Env for FakeEnv {
        fn var(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }

    fn binary_name() -> &'static str {
        BINARY_NAME
    }

    #[test]
    fn finds_codex_on_path() {
        let env = FakeEnv(HashMap::from([(
            "PATH".to_string(),
            "C:\\bin;C:\\tools".to_string(),
        )]));
        let on_disk = PathBuf::from("C:\\bin").join(binary_name());
        let fs = FakeFs(vec![on_disk.clone()]);
        let resolved = locate_with(&env, &fs).unwrap();
        assert_eq!(resolved, on_disk);
    }

    #[test]
    fn falls_back_to_localappdata_programs_codex() {
        let env = FakeEnv(HashMap::from([
            ("PATH".to_string(), "C:\\bin".to_string()),
            (
                "LOCALAPPDATA".to_string(),
                "C:\\Users\\u\\AppData\\Local".to_string(),
            ),
        ]));
        let on_disk =
            PathBuf::from("C:\\Users\\u\\AppData\\Local\\Programs\\codex").join(binary_name());
        let fs = FakeFs(vec![on_disk.clone()]);
        let resolved = locate_with(&env, &fs).unwrap();
        assert_eq!(resolved, on_disk);
    }

    #[test]
    fn falls_back_to_bun_bin() {
        let env = FakeEnv(HashMap::from([
            ("PATH".to_string(), String::new()),
            ("USERPROFILE".to_string(), "C:\\Users\\u".to_string()),
        ]));
        let on_disk = PathBuf::from("C:\\Users\\u\\.bun\\bin").join(binary_name());
        let fs = FakeFs(vec![on_disk.clone()]);
        let resolved = locate_with(&env, &fs).unwrap();
        assert_eq!(resolved, on_disk);
    }

    #[test]
    fn returns_missing_when_nothing_exists() {
        let env = FakeEnv(HashMap::from([
            ("PATH".to_string(), "C:\\bin".to_string()),
            ("LOCALAPPDATA".to_string(), "C:\\x".to_string()),
            ("USERPROFILE".to_string(), "C:\\Users\\u".to_string()),
        ]));
        let fs = FakeFs(Vec::new());
        let err = locate_with(&env, &fs).unwrap_err();
        assert_eq!(err, BinaryNotFoundError::Missing);
    }
}
