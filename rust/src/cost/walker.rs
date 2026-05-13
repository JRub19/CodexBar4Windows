//! Filesystem walker for the cost-scanning subsystem. Ported from
//! `docs/windows/spec/70-cost-scanning.md` §1.
//!
//! Three families of JSONL trees, each with multiple candidate roots:
//!
//! - **Codex sessions**: `%CODEX_HOME%\sessions` else
//!   `%USERPROFILE%\.codex\sessions`, plus the sibling
//!   `%USERPROFILE%\.codex\archived_sessions`. Layout is one of
//!   date-partitioned `YYYY/MM/DD/*.jsonl`, flat `*.jsonl`, or
//!   any-depth `**/*.jsonl` (recursive cold-cache pass).
//! - **Claude Code**: roots from `%CLAUDE_CONFIG_DIR%` (comma-
//!   separated, each appended with `\projects` when not already
//!   ending in `projects`), plus
//!   `%USERPROFILE%\.config\claude\projects` and
//!   `%USERPROFILE%\.claude\projects`. Globbed `**/*.jsonl`.
//! - **pi (Practical Intelligence)**:
//!   `%USERPROFILE%\.pi\agent\sessions\**\*.jsonl`.
//!
//! All walkers skip hidden files (filename starts with `.`),
//! optionally prefilter by mtime, and never follow symlinks.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

/// One discovered JSONL file, with the source family that turned it
/// up so the caller routes it to the right parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredFile {
    pub family: JsonlFamily,
    pub path: PathBuf,
    /// File mtime as unix epoch seconds. None when unreadable.
    pub mtime_unix_secs: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JsonlFamily {
    ClaudeCode,
    Codex,
    Pi,
}

/// Filesystem abstraction so tests drive every code path without
/// writing real files.
pub trait Filesystem: Send + Sync {
    fn exists(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    /// Recursive listing rooted at `root`. Each result carries a
    /// best-effort mtime. Returned paths are absolute. The
    /// production implementation uses `walkdir` with
    /// `follow_links(false)`.
    fn walk(&self, root: &Path) -> Vec<(PathBuf, Option<i64>)>;
}

/// Environment lookup so tests inject paths without touching real
/// process env.
pub trait Env: Send + Sync {
    fn var(&self, key: &str) -> Option<String>;
}

pub struct OsEnv;
impl Env for OsEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

pub struct OsFilesystem;
impl Filesystem for OsFilesystem {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
    fn walk(&self, root: &Path) -> Vec<(PathBuf, Option<i64>)> {
        let mut out = Vec::new();
        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_hidden_component(e.file_name()))
        {
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_file() {
                continue;
            }
            let mtime = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| {
                    t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
                });
            out.push((entry.into_path(), mtime));
        }
        out
    }
}

fn is_hidden_component(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
        && name != std::ffi::OsStr::new(".")
        && name != std::ffi::OsStr::new("..")
}

/// Resolve every candidate root for one JSONL family on this host.
/// Roots that do not exist on disk are silently skipped.
pub fn resolve_roots(family: JsonlFamily, env: &dyn Env, fs: &dyn Filesystem) -> Vec<PathBuf> {
    match family {
        JsonlFamily::Codex => resolve_codex_roots(env, fs),
        JsonlFamily::ClaudeCode => resolve_claude_roots(env, fs),
        JsonlFamily::Pi => resolve_pi_roots(env, fs),
    }
}

fn resolve_codex_roots(env: &dyn Env, fs: &dyn Filesystem) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = env.var("CODEX_HOME") {
        push_if_dir(&mut out, fs, PathBuf::from(home).join("sessions"));
    }
    if let Some(profile) = env.var("USERPROFILE") {
        push_if_dir(&mut out, fs, PathBuf::from(&profile).join(".codex").join("sessions"));
        push_if_dir(
            &mut out,
            fs,
            PathBuf::from(&profile).join(".codex").join("archived_sessions"),
        );
    }
    out
}

fn resolve_claude_roots(env: &dyn Env, fs: &dyn Filesystem) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(raw) = env.var("CLAUDE_CONFIG_DIR") {
        for piece in raw.split([',', ';']).map(str::trim) {
            if piece.is_empty() {
                continue;
            }
            let candidate = PathBuf::from(piece);
            // If the path already ends in `projects`, accept it
            // verbatim; otherwise append `projects` per spec.
            let final_path = if candidate
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("projects"))
                .unwrap_or(false)
            {
                candidate
            } else {
                candidate.join("projects")
            };
            push_if_dir(&mut out, fs, final_path);
        }
    }
    if let Some(profile) = env.var("USERPROFILE") {
        push_if_dir(
            &mut out,
            fs,
            PathBuf::from(&profile)
                .join(".config")
                .join("claude")
                .join("projects"),
        );
        push_if_dir(
            &mut out,
            fs,
            PathBuf::from(&profile).join(".claude").join("projects"),
        );
    }
    out
}

fn resolve_pi_roots(env: &dyn Env, fs: &dyn Filesystem) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(profile) = env.var("USERPROFILE") {
        push_if_dir(
            &mut out,
            fs,
            PathBuf::from(&profile)
                .join(".pi")
                .join("agent")
                .join("sessions"),
        );
    }
    out
}

fn push_if_dir(out: &mut Vec<PathBuf>, fs: &dyn Filesystem, path: PathBuf) {
    if fs.exists(&path) && fs.is_dir(&path) && !out.contains(&path) {
        out.push(path);
    }
}

/// Walk every resolved root for `family` and return JSONL files
/// modified at or after `since_unix_secs` (when supplied).
///
/// `since_unix_secs` is the lower bound (inclusive); use `0` or
/// `None` for a cold-cache full rescan.
pub fn discover(
    family: JsonlFamily,
    env: &dyn Env,
    fs: &dyn Filesystem,
    since_unix_secs: Option<i64>,
) -> Vec<DiscoveredFile> {
    let mut out = Vec::new();
    for root in resolve_roots(family, env, fs) {
        for (path, mtime) in fs.walk(&root) {
            if path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| !s.eq_ignore_ascii_case("jsonl"))
                .unwrap_or(true)
            {
                continue;
            }
            // Skip entries whose name (or any sub-directory between
            // the root and the file) starts with `.`. The root itself
            // may legitimately be a dotfolder like `.codex`, so we
            // only check the components AFTER the root.
            if path
                .strip_prefix(&root)
                .ok()
                .map(|rel| {
                    rel.components()
                        .filter_map(|c| c.as_os_str().to_str())
                        .any(|s| s.starts_with('.'))
                })
                .unwrap_or(false)
            {
                continue;
            }
            if let (Some(since), Some(actual)) = (since_unix_secs, mtime) {
                if actual < since {
                    continue;
                }
            }
            out.push(DiscoveredFile {
                family,
                path,
                mtime_unix_secs: mtime,
            });
        }
    }
    out
}

/// Local-midnight unix timestamp for a `days_back` window. Use this
/// to seed `since_unix_secs` when the caller wants "files modified
/// in the last N days". Computed in the host's local timezone.
pub fn local_midnight_n_days_ago(days_back: u32) -> i64 {
    use chrono::{Duration as ChDuration, Local, Timelike};
    let now = Local::now();
    let midnight = now
        .with_hour(0)
        .and_then(|t| t.with_minute(0))
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(now);
    let target = midnight - ChDuration::days(days_back as i64);
    target.timestamp()
}

/// Convenience: convert a `Duration` into "unix seconds N days ago"
/// without the chrono dependency. Useful in places that already
/// have a `SystemTime`.
pub fn duration_to_unix_secs_offset(now: SystemTime, ago: Duration) -> i64 {
    now.checked_sub(ago)
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct FakeFs {
        files: HashMap<PathBuf, Option<i64>>,
        dirs: Mutex<Vec<PathBuf>>,
    }
    impl FakeFs {
        fn new() -> Self {
            Self {
                files: HashMap::new(),
                dirs: Mutex::new(Vec::new()),
            }
        }
        fn put_file(&mut self, path: &str, mtime: Option<i64>) {
            let p = PathBuf::from(path);
            let mut cursor = p.parent().map(|c| c.to_path_buf());
            while let Some(c) = cursor {
                self.dirs.lock().unwrap().push(c.clone());
                cursor = c.parent().map(|p| p.to_path_buf());
            }
            self.files.insert(p, mtime);
        }
    }
    impl Filesystem for FakeFs {
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path) || self.dirs.lock().unwrap().iter().any(|d| d == path)
        }
        fn is_dir(&self, path: &Path) -> bool {
            self.dirs.lock().unwrap().iter().any(|d| d == path)
        }
        fn walk(&self, root: &Path) -> Vec<(PathBuf, Option<i64>)> {
            self.files
                .iter()
                .filter(|(p, _)| p.starts_with(root))
                .map(|(p, m)| (p.clone(), *m))
                .collect()
        }
    }

    struct FakeEnv(HashMap<String, String>);
    impl Env for FakeEnv {
        fn var(&self, k: &str) -> Option<String> {
            self.0.get(k).cloned()
        }
    }
    fn env(pairs: &[(&str, &str)]) -> FakeEnv {
        FakeEnv(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    #[test]
    fn codex_resolve_uses_codex_home_when_set_plus_user_profile_fallbacks() {
        let mut fs = FakeFs::new();
        fs.put_file(r"D:\codex\sessions\foo.jsonl", Some(100));
        fs.put_file(r"C:\Users\u\.codex\sessions\bar.jsonl", Some(100));
        fs.put_file(r"C:\Users\u\.codex\archived_sessions\baz.jsonl", Some(100));
        let e = env(&[("CODEX_HOME", r"D:\codex"), ("USERPROFILE", r"C:\Users\u")]);
        let roots = resolve_codex_roots(&e, &fs);
        assert!(roots.contains(&PathBuf::from(r"D:\codex\sessions")));
        assert!(roots.contains(&PathBuf::from(r"C:\Users\u\.codex\sessions")));
        assert!(roots.contains(&PathBuf::from(r"C:\Users\u\.codex\archived_sessions")));
    }

    #[test]
    fn claude_resolve_appends_projects_unless_already_present() {
        let mut fs = FakeFs::new();
        fs.put_file(r"D:\custom\projects\x.jsonl", Some(100));
        fs.put_file(r"E:\alt\already-projects\projects\y.jsonl", Some(100));
        let e = env(&[
            ("CLAUDE_CONFIG_DIR", r"D:\custom;E:\alt\already-projects\projects"),
            ("USERPROFILE", r"C:\Users\u"),
        ]);
        let roots = resolve_claude_roots(&e, &fs);
        assert!(roots.contains(&PathBuf::from(r"D:\custom\projects")));
        assert!(roots
            .contains(&PathBuf::from(r"E:\alt\already-projects\projects")));
    }

    #[test]
    fn claude_resolve_falls_back_to_dotfolders() {
        let mut fs = FakeFs::new();
        fs.put_file(r"C:\Users\u\.config\claude\projects\session.jsonl", Some(100));
        fs.put_file(r"C:\Users\u\.claude\projects\another.jsonl", Some(100));
        let e = env(&[("USERPROFILE", r"C:\Users\u")]);
        let roots = resolve_claude_roots(&e, &fs);
        assert!(
            roots.contains(&PathBuf::from(r"C:\Users\u\.config\claude\projects"))
        );
        assert!(roots.contains(&PathBuf::from(r"C:\Users\u\.claude\projects")));
    }

    #[test]
    fn pi_resolve_uses_userprofile_dot_pi_agent_sessions() {
        let mut fs = FakeFs::new();
        fs.put_file(r"C:\Users\u\.pi\agent\sessions\a.jsonl", Some(100));
        let e = env(&[("USERPROFILE", r"C:\Users\u")]);
        let roots = resolve_pi_roots(&e, &fs);
        assert_eq!(roots, vec![PathBuf::from(r"C:\Users\u\.pi\agent\sessions")]);
    }

    #[test]
    fn discover_filters_by_extension_and_skips_hidden_components() {
        let mut fs = FakeFs::new();
        fs.put_file(r"C:\Users\u\.claude\projects\valid.jsonl", Some(1000));
        // Hidden component in the path → skipped.
        fs.put_file(
            r"C:\Users\u\.claude\projects\.archive\old.jsonl",
            Some(2000),
        );
        // Non-jsonl extension → skipped.
        fs.put_file(r"C:\Users\u\.claude\projects\index.json", Some(3000));
        let e = env(&[("USERPROFILE", r"C:\Users\u")]);
        let found = discover(JsonlFamily::ClaudeCode, &e, &fs, None);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].path,
            PathBuf::from(r"C:\Users\u\.claude\projects\valid.jsonl")
        );
    }

    #[test]
    fn discover_filters_by_mtime_threshold() {
        let mut fs = FakeFs::new();
        fs.put_file(r"C:\Users\u\.claude\projects\fresh.jsonl", Some(1_700_000_500));
        fs.put_file(r"C:\Users\u\.claude\projects\stale.jsonl", Some(1_699_999_900));
        let e = env(&[("USERPROFILE", r"C:\Users\u")]);
        let found = discover(
            JsonlFamily::ClaudeCode,
            &e,
            &fs,
            Some(1_700_000_000),
        );
        assert_eq!(found.len(), 1);
        assert!(found[0]
            .path
            .to_string_lossy()
            .ends_with("fresh.jsonl"));
    }

    #[test]
    fn discover_returns_empty_when_no_roots_exist() {
        let fs = FakeFs::new();
        let e = env(&[]);
        assert!(discover(JsonlFamily::ClaudeCode, &e, &fs, None).is_empty());
        assert!(discover(JsonlFamily::Codex, &e, &fs, None).is_empty());
        assert!(discover(JsonlFamily::Pi, &e, &fs, None).is_empty());
    }

    #[test]
    fn discover_codex_walks_date_partitioned_layout() {
        let mut fs = FakeFs::new();
        fs.put_file(r"C:\Users\u\.codex\sessions\2026\05\13\a.jsonl", Some(100));
        fs.put_file(r"C:\Users\u\.codex\sessions\2026\05\13\b.jsonl", Some(200));
        fs.put_file(r"C:\Users\u\.codex\sessions\flat.jsonl", Some(300));
        let e = env(&[("USERPROFILE", r"C:\Users\u")]);
        let found = discover(JsonlFamily::Codex, &e, &fs, None);
        assert_eq!(found.len(), 3);
    }

    #[test]
    fn local_midnight_n_days_ago_returns_a_unix_second_in_the_past() {
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let yesterday = local_midnight_n_days_ago(1);
        assert!(yesterday <= now_secs);
        assert!(yesterday > now_secs - (3 * 86400));
    }
}
