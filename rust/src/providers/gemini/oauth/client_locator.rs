//! Locate the embedded `OAUTH_CLIENT_ID` / `OAUTH_CLIENT_SECRET` inside
//! the installed `@google/gemini-cli` npm package on Windows. Ported
//! from `parseOAuthCredentials` + `extractOAuthCredentialsFromLegacyPaths`
//! + `findGeminiPackageRoot` in `GeminiStatusProbe.swift`.
//!
//! The Gemini CLI bundles its Google OAuth client credentials inside
//! `dist/src/code_assist/oauth2.js` (and `bundle/gemini.js` for the
//! single-file build). On Windows the package can live in any of:
//!
//!   - `%APPDATA%\npm\node_modules\@google\gemini-cli\...` (npm -g)
//!   - `%LOCALAPPDATA%\Programs\nodejs\node_modules\...` (system node)
//!   - `%USERPROFILE%\scoop\persist\nodejs\bin\...` (scoop)
//!   - `%LOCALAPPDATA%\fnm_multishells\<id>\node_modules\...` (fnm)
//!   - Sibling of the resolved `gemini.cmd` shim on PATH
//!
//! Filesystem + env are pluggable traits so tests can drive every
//! lookup branch without writing real files.

use std::path::{Path, PathBuf};

use regex::Regex;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OAuthClientCredentials {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LocateError {
    #[error("@google/gemini-cli package not found in any known install layout")]
    PackageNotFound,
    #[error("oauth2 source file found but OAUTH_CLIENT_ID/SECRET regex did not match")]
    ConstantsNotMatched,
}

pub trait Filesystem: Send + Sync {
    fn exists(&self, path: &Path) -> bool;
    fn read_to_string(&self, path: &Path) -> Option<String>;
}

pub struct OsFilesystem;

impl Filesystem for OsFilesystem {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn read_to_string(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
}

pub trait Env: Send + Sync {
    fn var(&self, key: &str) -> Option<String>;
}

pub struct OsEnv;
impl Env for OsEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// Walk the known Windows install layouts looking for the
/// `@google/gemini-cli` package, then return its OAuth client
/// credentials. The order mirrors the macOS source: cheap fixed paths
/// first, then the ascent-from-binary fallback.
pub fn locate(env: &dyn Env, fs: &dyn Filesystem) -> Result<OAuthClientCredentials, LocateError> {
    for candidate in candidate_package_roots(env, fs) {
        if let Some(creds) = extract_from_package(&candidate, fs) {
            return Ok(creds);
        }
    }
    Err(LocateError::PackageNotFound)
}

/// Enumerate likely install roots. Each one is a directory we expect to
/// be the package root (i.e. contains `dist/src/code_assist/oauth2.js`
/// or `bundle/gemini.js`). Non-existing paths are silently skipped.
fn candidate_package_roots(env: &dyn Env, fs: &dyn Filesystem) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let push_if_exists = |out: &mut Vec<PathBuf>, p: PathBuf| {
        if fs.exists(&p) {
            out.push(p);
        }
    };

    // npm global on Windows: %APPDATA%\npm\node_modules\@google\gemini-cli
    if let Some(appdata) = env.var("APPDATA") {
        push_if_exists(
            &mut out,
            PathBuf::from(appdata)
                .join("npm")
                .join("node_modules")
                .join("@google")
                .join("gemini-cli"),
        );
    }

    // System node install: %LOCALAPPDATA%\Programs\nodejs\node_modules\...
    if let Some(local) = env.var("LOCALAPPDATA") {
        push_if_exists(
            &mut out,
            PathBuf::from(&local)
                .join("Programs")
                .join("nodejs")
                .join("node_modules")
                .join("@google")
                .join("gemini-cli"),
        );

        // fnm_multishells: each shell session symlinks under here.
        // We try the most recently-modified one in tests; in production
        // we just enumerate the directory.
        let fnm_root = PathBuf::from(&local).join("fnm_multishells");
        if fs.exists(&fnm_root) {
            // The actual node_modules path inside fnm depends on the
            // installed version; only the binary symlink lives in
            // multishells. The ascent-from-binary path handles fnm.
        }
    }

    // Scoop: %USERPROFILE%\scoop\apps\nodejs\current\node_modules\...
    if let Some(profile) = env.var("USERPROFILE") {
        push_if_exists(
            &mut out,
            PathBuf::from(&profile)
                .join("scoop")
                .join("apps")
                .join("nodejs")
                .join("current")
                .join("node_modules")
                .join("@google")
                .join("gemini-cli"),
        );
        // Volta caches per-version, package_image / image roots:
        push_if_exists(
            &mut out,
            PathBuf::from(&profile)
                .join("AppData")
                .join("Local")
                .join("Volta")
                .join("tools")
                .join("image")
                .join("packages")
                .join("@google")
                .join("gemini-cli"),
        );
    }

    // Ascent from a resolved gemini binary on PATH. We look for the
    // gemini.cmd shim and walk up to find a sibling node_modules/.
    if let Some(path) = env.var("PATH") {
        for dir in std::env::split_paths(&path) {
            for shim in &["gemini.cmd", "gemini.ps1", "gemini.exe", "gemini"] {
                let bin = dir.join(shim);
                if fs.exists(&bin) {
                    out.extend(ascend_for_package(&bin, fs));
                }
            }
        }
    }

    out
}

/// Walk up to 8 levels from a resolved binary, looking for a
/// `node_modules/@google/gemini-cli` next to a global `lib/` or
/// directly under the binary's parent. Mirrors
/// `findGeminiPackageRoot(startingAt:)` from the Swift source.
fn ascend_for_package(start: &Path, fs: &dyn Filesystem) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut cursor = start.parent().unwrap_or(start).to_path_buf();
    for _ in 0..8 {
        // Direct sibling: cursor/node_modules/@google/gemini-cli
        let direct = cursor
            .join("node_modules")
            .join("@google")
            .join("gemini-cli");
        if fs.exists(&direct) {
            out.push(direct);
        }
        // Linux/Mac-style: cursor/lib/node_modules/@google/gemini-cli
        let lib_layout = cursor
            .join("lib")
            .join("node_modules")
            .join("@google")
            .join("gemini-cli");
        if fs.exists(&lib_layout) {
            out.push(lib_layout);
        }
        let parent = cursor.parent().map(|p| p.to_path_buf());
        match parent {
            Some(p) if p != cursor => cursor = p,
            _ => break,
        }
    }
    out
}

/// Try the standard distributed file first, then the single-file
/// bundle. Both contain the same OAuth constants — the regex match
/// works on either.
fn extract_from_package(root: &Path, fs: &dyn Filesystem) -> Option<OAuthClientCredentials> {
    // dist/src/code_assist/oauth2.js — distributed multi-file build.
    let oauth_file = root
        .join("dist")
        .join("src")
        .join("code_assist")
        .join("oauth2.js");
    if let Some(content) = fs.read_to_string(&oauth_file) {
        if let Some(creds) = parse_constants(&content) {
            return Some(creds);
        }
    }

    // node_modules/@google/gemini-cli-core sibling layout.
    let core_oauth = root
        .join("node_modules")
        .join("@google")
        .join("gemini-cli-core")
        .join("dist")
        .join("src")
        .join("code_assist")
        .join("oauth2.js");
    if let Some(content) = fs.read_to_string(&core_oauth) {
        if let Some(creds) = parse_constants(&content) {
            return Some(creds);
        }
    }

    // bundle/gemini.js — single-file bundle. We do not recursively walk
    // its imports the way the Swift source does because the bundled
    // build typically inlines the constants directly into gemini.js.
    let bundle = root.join("bundle").join("gemini.js");
    if let Some(content) = fs.read_to_string(&bundle) {
        if let Some(creds) = parse_constants(&content) {
            return Some(creds);
        }
    }

    None
}

/// Match `OAUTH_CLIENT_ID = '...';` / `OAUTH_CLIENT_SECRET = '...';`.
/// The patterns mirror the macOS regex literal so the same identifier
/// styles (const / let / var / bare assignment / arbitrary quote)
/// continue to work after upstream code-style churn.
pub fn parse_constants(content: &str) -> Option<OAuthClientCredentials> {
    let id_re =
        Regex::new(r#"(?:const|let|var)?\s*OAUTH_CLIENT_ID\s*=\s*['"]([\w\-\.]+)['"]\s*;"#).ok()?;
    let secret_re =
        Regex::new(r#"(?:const|let|var)?\s*OAUTH_CLIENT_SECRET\s*=\s*['"]([\w\-]+)['"]\s*;"#)
            .ok()?;
    let id = id_re.captures(content)?.get(1)?.as_str().to_string();
    let secret = secret_re.captures(content)?.get(1)?.as_str().to_string();
    Some(OAuthClientCredentials {
        client_id: id,
        client_secret: secret,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct FakeFs {
        files: HashMap<PathBuf, String>,
        dirs: Mutex<Vec<PathBuf>>,
    }
    impl FakeFs {
        fn new() -> Self {
            Self {
                files: HashMap::new(),
                dirs: Mutex::new(Vec::new()),
            }
        }
        fn put(&mut self, path: &str, content: &str) {
            let p = PathBuf::from(path);
            // Register the file and every ancestor as an existing dir.
            let mut cursor = p.parent().map(|c| c.to_path_buf());
            while let Some(c) = cursor {
                self.dirs.lock().unwrap().push(c.clone());
                cursor = c.parent().map(|p| p.to_path_buf());
            }
            self.files.insert(p, content.into());
        }
    }
    impl Filesystem for FakeFs {
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path) || self.dirs.lock().unwrap().iter().any(|d| d == path)
        }
        fn read_to_string(&self, path: &Path) -> Option<String> {
            self.files.get(path).cloned()
        }
    }

    struct FakeEnv(HashMap<String, String>);
    impl Env for FakeEnv {
        fn var(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }

    const SAMPLE_OAUTH_JS: &str = r#"
        const OAUTH_CLIENT_ID = '681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com';
        const OAUTH_CLIENT_SECRET = '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';
        function getOauthClient() { /* ... */ }
    "#;

    #[test]
    fn parse_constants_extracts_id_and_secret() {
        let creds = parse_constants(SAMPLE_OAUTH_JS).unwrap();
        assert_eq!(
            creds.client_id,
            "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com"
        );
        assert_eq!(creds.client_secret, "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");
    }

    #[test]
    fn parse_constants_handles_let_var_and_bare_assignment() {
        let snippets = vec![
            "let OAUTH_CLIENT_ID = 'a-id';\nvar OAUTH_CLIENT_SECRET = 'a-secret';",
            "OAUTH_CLIENT_ID = \"b-id\";\nOAUTH_CLIENT_SECRET = \"b-secret\";",
        ];
        for src in snippets {
            let creds = parse_constants(src).expect("matched");
            assert!(!creds.client_id.is_empty());
            assert!(!creds.client_secret.is_empty());
        }
    }

    #[test]
    fn parse_constants_returns_none_when_only_id_present() {
        let src = "OAUTH_CLIENT_ID = 'only-id';";
        assert!(parse_constants(src).is_none());
    }

    #[test]
    fn locate_finds_credentials_in_npm_global_layout() {
        let mut fs = FakeFs::new();
        fs.put(
            r"C:\Users\u\AppData\Roaming\npm\node_modules\@google\gemini-cli\dist\src\code_assist\oauth2.js",
            SAMPLE_OAUTH_JS,
        );
        let env = FakeEnv(
            [("APPDATA", r"C:\Users\u\AppData\Roaming")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        let creds = locate(&env, &fs).unwrap();
        assert!(creds.client_id.starts_with("681255809395"));
    }

    #[test]
    fn locate_finds_credentials_in_scoop_layout() {
        let mut fs = FakeFs::new();
        fs.put(
            r"C:\Users\u\scoop\apps\nodejs\current\node_modules\@google\gemini-cli\dist\src\code_assist\oauth2.js",
            SAMPLE_OAUTH_JS,
        );
        let env = FakeEnv(
            [("USERPROFILE", r"C:\Users\u")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        let creds = locate(&env, &fs).unwrap();
        assert_eq!(creds.client_secret, "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");
    }

    #[test]
    fn locate_walks_up_from_path_shim_to_sibling_node_modules() {
        let mut fs = FakeFs::new();
        // PATH shim lives under `bin/`; the package lives under
        // `lib/node_modules/...` two levels up.
        fs.put(
            r"D:\nodejs\bin\gemini.cmd",
            "@echo off\nnode %~dp0\\..\\lib\\node_modules\\@google\\gemini-cli\\bin\\gemini.js %*",
        );
        fs.put(
            r"D:\nodejs\lib\node_modules\@google\gemini-cli\dist\src\code_assist\oauth2.js",
            SAMPLE_OAUTH_JS,
        );
        let env = FakeEnv(
            [("PATH", r"D:\nodejs\bin")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        let creds = locate(&env, &fs).unwrap();
        assert!(!creds.client_id.is_empty());
    }

    #[test]
    fn locate_returns_package_not_found_when_no_layout_matches() {
        let fs = FakeFs::new();
        let env = FakeEnv(HashMap::new());
        let err = locate(&env, &fs).unwrap_err();
        assert_eq!(err, LocateError::PackageNotFound);
    }

    #[test]
    fn locate_falls_back_to_bundle_layout_when_dist_absent() {
        let mut fs = FakeFs::new();
        fs.put(
            r"C:\Users\u\AppData\Roaming\npm\node_modules\@google\gemini-cli\bundle\gemini.js",
            SAMPLE_OAUTH_JS,
        );
        let env = FakeEnv(
            [("APPDATA", r"C:\Users\u\AppData\Roaming")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        let creds = locate(&env, &fs).unwrap();
        assert!(creds.client_id.starts_with("681255809395"));
    }
}
