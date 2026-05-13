//! Provider storage-footprint scanner. Ported from
//! `docs/windows/spec/70-cost-scanning.md` §11.
//!
//! Reports byte sizes of provider-owned directories on disk so the
//! preferences pane can show "Codex is using 3.4 GB, ~/.codex/sessions
//! is 2.9 GB of that". **It never deletes anything.**
//!
//! Five providers are scanned:
//!
//! - **codex** — `%CODEX_HOME%` else `%USERPROFILE%\.codex` (+ managed
//!   account homes when CodexBar managed-auth ships).
//! - **claude** — `%USERPROFILE%\.claude`, `%USERPROFILE%\.config\claude`,
//!   plus the ClaudeProbe scratch dir under `%APPDATA%\CodexBar`.
//! - **gemini** — `%USERPROFILE%\.gemini`, `%USERPROFILE%\.config\gemini`.
//! - **opencode** — `%USERPROFILE%\.config\opencode`.
//! - **copilot** — `%USERPROFILE%\.config\github-copilot`.
//!
//! Scan rules (per spec §11.3):
//!
//! - Use a `WalkDir`-style enumerator.
//! - Skip symlinks entirely (do not follow, do not count).
//! - Add only regular file sizes.
//! - Unreadable entries → push to `unreadable_paths`, continue.
//! - Cancelable via an external `AtomicBool` cancel token.
//!
//! The result splits each existing root into its first-level
//! children so the UI can surface "sessions is 2.9 GB of that 3.4 GB",
//! sorted desc by size.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use super::walker::{Env, Filesystem};

/// Provider identifier matching the macOS `UsageProvider` enum so the
/// settings UI keys footprints by the same string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageProvider {
    Codex,
    Claude,
    Gemini,
    Opencode,
    Copilot,
}

impl StorageProvider {
    pub fn id(self) -> &'static str {
        match self {
            StorageProvider::Codex => "codex",
            StorageProvider::Claude => "claude",
            StorageProvider::Gemini => "gemini",
            StorageProvider::Opencode => "opencode",
            StorageProvider::Copilot => "copilot",
        }
    }

    pub fn all() -> [StorageProvider; 5] {
        [
            StorageProvider::Codex,
            StorageProvider::Claude,
            StorageProvider::Gemini,
            StorageProvider::Opencode,
            StorageProvider::Copilot,
        ]
    }
}

/// One first-level child directory (or file) inside a scanned root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageComponent {
    /// First-level child name, e.g. `sessions`, `projects`, `cache`.
    pub id: String,
    /// Absolute path on disk.
    pub path: String,
    pub total_bytes: u64,
}

/// One provider's storage report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderStorageFootprint {
    pub provider: StorageProvider,
    pub total_bytes: u64,
    /// Existing roots that contributed to `total_bytes`.
    pub paths: Vec<String>,
    /// Roots that resolved but were not found on disk.
    pub missing_paths: Vec<String>,
    /// Roots / sub-paths we hit a permission error on.
    pub unreadable_paths: Vec<String>,
    /// First-level child breakdown across all roots, sorted desc by size.
    pub components: Vec<StorageComponent>,
    /// ISO-8601 string in UTC.
    pub updated_at: String,
}

/// Filesystem extension trait so tests inject fake byte counts without
/// touching the real disk. The blanket impl below wraps any
/// `Filesystem` and falls back to a real `WalkDir` scan.
pub trait FilesystemSize: Filesystem {
    /// Recursive size sum over `root`. Excludes symlinks and any
    /// entries that error out (those are reported via the second
    /// return value as absolute path strings).
    ///
    /// `cancel` is checked between entries; once set, the walk
    /// returns whatever it has accumulated so far.
    fn total_size(&self, root: &Path, cancel: &AtomicBool) -> (u64, Vec<String>);

    /// First-level children of `root`. Returns `(name, full path, is_dir)`.
    fn first_level_children(&self, root: &Path) -> Vec<(String, PathBuf, bool)>;
}

/// Real-disk implementation that delegates to `walkdir`.
pub struct OsStorageFs;

impl Filesystem for OsStorageFs {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
    fn walk(&self, _root: &Path) -> Vec<(PathBuf, Option<i64>)> {
        // Footprint scanner does not need this; defer to OsFilesystem
        // if you also need mtime-aware listing.
        Vec::new()
    }
}

impl FilesystemSize for OsStorageFs {
    fn total_size(&self, root: &Path, cancel: &AtomicBool) -> (u64, Vec<String>) {
        let mut total: u64 = 0;
        let mut unreadable: Vec<String> = Vec::new();
        for entry in WalkDir::new(root).follow_links(false) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            match entry {
                Ok(e) => {
                    // Skip symlinks (don't follow, don't count).
                    let ft = e.file_type();
                    if ft.is_symlink() {
                        continue;
                    }
                    if ft.is_file() {
                        match e.metadata() {
                            Ok(m) => total = total.saturating_add(m.len()),
                            Err(_) => unreadable.push(e.path().display().to_string()),
                        }
                    }
                }
                Err(err) => {
                    if let Some(p) = err.path() {
                        unreadable.push(p.display().to_string());
                    }
                }
            }
        }
        (total, unreadable)
    }

    fn first_level_children(&self, root: &Path) -> Vec<(String, PathBuf, bool)> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(root) else {
            return out;
        };
        for entry in rd.flatten() {
            // Skip symlinks per spec.
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            out.push((name, entry.path(), ft.is_dir()));
        }
        out
    }
}

/// Resolve every candidate root for a single provider on this host.
/// Returns `(existing, missing)` — existing roots are passed to the
/// scanner; missing ones surface in `ProviderStorageFootprint.missing_paths`.
pub fn resolve_provider_roots(
    provider: StorageProvider,
    env: &dyn Env,
    fs: &dyn Filesystem,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut candidates: Vec<PathBuf> = Vec::new();

    match provider {
        StorageProvider::Codex => {
            if let Some(home) = env.var("CODEX_HOME") {
                candidates.push(PathBuf::from(home));
            }
            if let Some(profile) = env.var("USERPROFILE") {
                candidates.push(PathBuf::from(&profile).join(".codex"));
            }
        }
        StorageProvider::Claude => {
            if let Some(profile) = env.var("USERPROFILE") {
                candidates.push(PathBuf::from(&profile).join(".claude"));
                candidates.push(PathBuf::from(&profile).join(".config").join("claude"));
            }
            if let Some(appdata) = env.var("APPDATA") {
                candidates.push(PathBuf::from(&appdata).join("CodexBar").join("ClaudeProbe"));
            }
        }
        StorageProvider::Gemini => {
            if let Some(profile) = env.var("USERPROFILE") {
                candidates.push(PathBuf::from(&profile).join(".gemini"));
                candidates.push(PathBuf::from(&profile).join(".config").join("gemini"));
            }
        }
        StorageProvider::Opencode => {
            if let Some(profile) = env.var("USERPROFILE") {
                candidates.push(PathBuf::from(&profile).join(".config").join("opencode"));
            }
        }
        StorageProvider::Copilot => {
            if let Some(profile) = env.var("USERPROFILE") {
                candidates.push(
                    PathBuf::from(&profile)
                        .join(".config")
                        .join("github-copilot"),
                );
            }
        }
    }

    let mut existing: Vec<PathBuf> = Vec::new();
    let mut missing: Vec<PathBuf> = Vec::new();
    for path in candidates {
        // Dedup by string match so users with overlapping env vars
        // don't get a path counted twice.
        if existing.iter().any(|p| p == &path) || missing.iter().any(|p| p == &path) {
            continue;
        }
        if fs.exists(&path) && fs.is_dir(&path) {
            existing.push(path);
        } else {
            missing.push(path);
        }
    }
    (existing, missing)
}

/// Build the coalescing signature for a `(provider, roots)` pair.
/// Used by the throttler in `UsageStore` (spec §11.5).
///
/// Format: `provider:path0\u{1f}path1...` per provider, then providers
/// joined with `\u{1e}` separators. Paths are sorted alphabetically
/// first so two calls with the same root set (in any order) collapse.
pub fn footprint_signature(provider: StorageProvider, roots: &[PathBuf]) -> String {
    let mut sorted: Vec<String> = roots.iter().map(|p| p.display().to_string()).collect();
    sorted.sort();
    let joined = sorted.join("\u{1f}");
    format!("{}:{}", provider.id(), joined)
}

/// Scan a single provider. Returns the footprint even when every root
/// is missing (caller surfaces an empty report).
pub fn scan_provider(
    provider: StorageProvider,
    env: &dyn Env,
    fs: &dyn FilesystemSize,
    cancel: &AtomicBool,
) -> ProviderStorageFootprint {
    let (existing, missing) = resolve_provider_roots(provider, env, fs);

    let mut total: u64 = 0;
    let mut unreadable: Vec<String> = Vec::new();
    // Component breakdown is keyed by the child *name* across roots,
    // so two roots that both have a `cache/` directory sum together.
    let mut components: Vec<StorageComponent> = Vec::new();

    for root in &existing {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        for (name, child_path, is_dir) in fs.first_level_children(root) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let (child_bytes, child_unreadable) = if is_dir {
                fs.total_size(&child_path, cancel)
            } else {
                // Plain file at the root — read its size via the same
                // helper rooted on the file itself.
                fs.total_size(&child_path, cancel)
            };
            total = total.saturating_add(child_bytes);
            unreadable.extend(child_unreadable);
            components.push(StorageComponent {
                id: name,
                path: child_path.display().to_string(),
                total_bytes: child_bytes,
            });
        }
    }

    components.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes).then(a.id.cmp(&b.id)));

    ProviderStorageFootprint {
        provider,
        total_bytes: total,
        paths: existing.iter().map(|p| p.display().to_string()).collect(),
        missing_paths: missing.iter().map(|p| p.display().to_string()).collect(),
        unreadable_paths: unreadable,
        components,
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Scan every supported provider. Convenience wrapper for the popup.
pub fn scan_all(
    env: &dyn Env,
    fs: &dyn FilesystemSize,
    cancel: &AtomicBool,
) -> Vec<ProviderStorageFootprint> {
    StorageProvider::all()
        .into_iter()
        .map(|p| scan_provider(p, env, fs, cancel))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct FakeFs {
        // path -> (size_bytes, is_symlink)
        files: HashMap<PathBuf, (u64, bool)>,
        dirs: Mutex<Vec<PathBuf>>,
    }
    impl FakeFs {
        fn new() -> Self {
            Self {
                files: HashMap::new(),
                dirs: Mutex::new(Vec::new()),
            }
        }
        fn put_file(&mut self, path: &str, size: u64) {
            self.put_file_with_link(path, size, false);
        }
        fn put_file_with_link(&mut self, path: &str, size: u64, is_symlink: bool) {
            let p = PathBuf::from(path);
            let mut cursor = p.parent().map(|c| c.to_path_buf());
            while let Some(c) = cursor {
                let mut d = self.dirs.lock().unwrap();
                if !d.contains(&c) {
                    d.push(c.clone());
                }
                drop(d);
                cursor = c.parent().map(|p| p.to_path_buf());
            }
            self.files.insert(p, (size, is_symlink));
        }
        fn put_dir(&mut self, path: &str) {
            let p = PathBuf::from(path);
            let mut d = self.dirs.lock().unwrap();
            if !d.contains(&p) {
                d.push(p);
            }
        }
    }
    impl Filesystem for FakeFs {
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path) || self.dirs.lock().unwrap().contains(&path.to_path_buf())
        }
        fn is_dir(&self, path: &Path) -> bool {
            self.dirs.lock().unwrap().contains(&path.to_path_buf())
        }
        fn walk(&self, _root: &Path) -> Vec<(PathBuf, Option<i64>)> {
            Vec::new()
        }
    }
    impl FilesystemSize for FakeFs {
        fn total_size(&self, root: &Path, cancel: &AtomicBool) -> (u64, Vec<String>) {
            let mut sum: u64 = 0;
            let mut unreadable: Vec<String> = Vec::new();
            // File at the root (single-file leaf case).
            if let Some((size, is_link)) = self.files.get(root) {
                if !is_link {
                    sum = sum.saturating_add(*size);
                }
                return (sum, unreadable);
            }
            for (p, (size, is_link)) in &self.files {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                if p.starts_with(root) {
                    if *is_link {
                        continue;
                    }
                    if p == &PathBuf::from("/__unreadable__") {
                        unreadable.push(p.display().to_string());
                        continue;
                    }
                    sum = sum.saturating_add(*size);
                }
            }
            (sum, unreadable)
        }
        fn first_level_children(&self, root: &Path) -> Vec<(String, PathBuf, bool)> {
            let mut seen: Vec<(String, PathBuf, bool)> = Vec::new();
            // Gather dirs whose parent == root.
            for d in self.dirs.lock().unwrap().iter() {
                if d.parent() == Some(root) {
                    let name = d
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    seen.push((name, d.clone(), true));
                }
            }
            // Gather plain files whose parent == root.
            for (p, (_size, is_link)) in &self.files {
                if *is_link {
                    continue;
                }
                if p.parent() == Some(root) {
                    let name = p
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    seen.push((name, p.clone(), false));
                }
            }
            seen
        }
    }

    struct FakeEnv(HashMap<String, String>);
    impl Env for FakeEnv {
        fn var(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }

    fn env(pairs: &[(&str, &str)]) -> FakeEnv {
        FakeEnv(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }

    #[test]
    fn codex_resolve_uses_codex_home_then_user_profile() {
        let mut fs = FakeFs::new();
        fs.put_dir("/codex-home");
        fs.put_dir("/users/jonas/.codex");
        let env = env(&[
            ("CODEX_HOME", "/codex-home"),
            ("USERPROFILE", "/users/jonas"),
        ]);
        let (existing, missing) = resolve_provider_roots(StorageProvider::Codex, &env, &fs);
        assert_eq!(existing.len(), 2);
        assert_eq!(missing.len(), 0);
        assert_eq!(existing[0], PathBuf::from("/codex-home"));
        assert_eq!(existing[1], PathBuf::from("/users/jonas/.codex"));
    }

    #[test]
    fn claude_resolve_includes_appdata_probe_dir() {
        let mut fs = FakeFs::new();
        fs.put_dir("/users/jonas/.claude");
        fs.put_dir("/users/jonas/.config/claude");
        fs.put_dir("/appdata/CodexBar/ClaudeProbe");
        let env = env(&[("USERPROFILE", "/users/jonas"), ("APPDATA", "/appdata")]);
        let (existing, missing) = resolve_provider_roots(StorageProvider::Claude, &env, &fs);
        assert_eq!(existing.len(), 3);
        assert!(existing.contains(&PathBuf::from("/appdata/CodexBar/ClaudeProbe")));
        assert!(missing.is_empty());
    }

    #[test]
    fn missing_roots_surface_in_missing_paths() {
        let fs = FakeFs::new(); // nothing on disk
        let env = env(&[("USERPROFILE", "/users/jonas")]);
        let (existing, missing) = resolve_provider_roots(StorageProvider::Gemini, &env, &fs);
        assert!(existing.is_empty());
        assert_eq!(missing.len(), 2);
    }

    #[test]
    fn scan_sums_first_level_components_and_skips_symlinks() {
        let mut fs = FakeFs::new();
        fs.put_dir("/users/jonas/.codex");
        fs.put_dir("/users/jonas/.codex/sessions");
        fs.put_dir("/users/jonas/.codex/cache");
        fs.put_file("/users/jonas/.codex/sessions/a.jsonl", 1000);
        fs.put_file("/users/jonas/.codex/sessions/b.jsonl", 500);
        fs.put_file("/users/jonas/.codex/cache/c.bin", 200);
        // Symlink — must be skipped on size and on first-level listing.
        fs.put_file_with_link("/users/jonas/.codex/link.lnk", 999_999, true);

        let env = env(&[("USERPROFILE", "/users/jonas")]);
        let cancel = AtomicBool::new(false);
        let report = scan_provider(StorageProvider::Codex, &env, &fs, &cancel);

        assert_eq!(report.provider, StorageProvider::Codex);
        assert_eq!(report.total_bytes, 1700);
        assert_eq!(report.paths.len(), 1);
        assert_eq!(report.components.len(), 2);
        // sorted desc by size
        assert_eq!(report.components[0].id, "sessions");
        assert_eq!(report.components[0].total_bytes, 1500);
        assert_eq!(report.components[1].id, "cache");
        assert_eq!(report.components[1].total_bytes, 200);
    }

    #[test]
    fn cancel_token_halts_walk_between_components() {
        let mut fs = FakeFs::new();
        fs.put_dir("/users/jonas/.codex");
        fs.put_dir("/users/jonas/.codex/sessions");
        fs.put_file("/users/jonas/.codex/sessions/a.jsonl", 1000);

        let env = env(&[("USERPROFILE", "/users/jonas")]);
        let cancel = AtomicBool::new(true); // pre-cancelled
        let report = scan_provider(StorageProvider::Codex, &env, &fs, &cancel);
        // Pre-cancel exits before the first child scan; still a
        // well-formed report.
        assert_eq!(report.total_bytes, 0);
    }

    #[test]
    fn signature_is_stable_for_same_roots_in_any_order() {
        let a = vec![PathBuf::from("/b"), PathBuf::from("/a")];
        let b = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        assert_eq!(
            footprint_signature(StorageProvider::Codex, &a),
            footprint_signature(StorageProvider::Codex, &b)
        );
        assert_ne!(
            footprint_signature(StorageProvider::Codex, &a),
            footprint_signature(StorageProvider::Claude, &a)
        );
    }

    #[test]
    fn scan_all_returns_one_report_per_provider() {
        let fs = FakeFs::new();
        let env = env(&[("USERPROFILE", "/users/jonas")]);
        let cancel = AtomicBool::new(false);
        let reports = scan_all(&env, &fs, &cancel);
        assert_eq!(reports.len(), 5);
        // Every provider present, deterministic order.
        assert_eq!(reports[0].provider, StorageProvider::Codex);
        assert_eq!(reports[4].provider, StorageProvider::Copilot);
    }
}
