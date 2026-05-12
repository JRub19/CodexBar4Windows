//! Probe canonical install paths for each supported browser.
//!
//! Phase 2.6 ships path probing only. A future revision can read
//! `HKLM\Software\Clients\StartMenuInternet` for non default install
//! locations; the canonical paths cover the vast majority of users.

use std::path::PathBuf;

use super::BrowserId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserPresence {
    pub browser: BrowserId,
    pub local_state_path: Option<PathBuf>,
    pub profile_root: Option<PathBuf>,
    pub cookie_db_path: Option<PathBuf>,
}

impl BrowserPresence {
    pub fn is_installed(&self) -> bool {
        self.cookie_db_path.is_some()
    }
}

pub struct BrowserDetection;

impl BrowserDetection {
    /// Probe every supported browser. Returns one `BrowserPresence` per
    /// `BrowserId`, with `is_installed() == false` when the canonical paths
    /// do not exist.
    pub fn probe_all() -> Vec<BrowserPresence> {
        vec![
            Self::probe(BrowserId::Chrome),
            Self::probe(BrowserId::Edge),
            Self::probe(BrowserId::Brave),
            Self::probe(BrowserId::Firefox),
        ]
    }

    pub fn probe(browser: BrowserId) -> BrowserPresence {
        match browser {
            BrowserId::Chrome => probe_chromium(
                browser,
                local_app_data().map(|p| p.join("Google").join("Chrome").join("User Data")),
            ),
            BrowserId::Edge => probe_chromium(
                browser,
                local_app_data().map(|p| p.join("Microsoft").join("Edge").join("User Data")),
            ),
            BrowserId::Brave => probe_chromium(
                browser,
                local_app_data().map(|p| {
                    p.join("BraveSoftware")
                        .join("Brave-Browser")
                        .join("User Data")
                }),
            ),
            BrowserId::Firefox => probe_firefox(browser),
        }
    }
}

fn probe_chromium(browser: BrowserId, user_data: Option<PathBuf>) -> BrowserPresence {
    let user_data = match user_data {
        Some(p) if p.is_dir() => p,
        _ => return absent(browser),
    };
    let local_state = user_data.join("Local State");
    if !local_state.is_file() {
        return absent(browser);
    }
    // Prefer "Default" profile; fall back to first matching "Profile *" dir.
    let candidates = [user_data.join("Default")]
        .into_iter()
        .chain(scan_profile_dirs(&user_data))
        .filter(|p| p.is_dir());
    for profile in candidates {
        let cookies = profile.join("Network").join("Cookies");
        if cookies.is_file() {
            return BrowserPresence {
                browser,
                local_state_path: Some(local_state),
                profile_root: Some(profile),
                cookie_db_path: Some(cookies),
            };
        }
    }
    absent(browser)
}

fn probe_firefox(browser: BrowserId) -> BrowserPresence {
    let Some(roaming) = roaming_app_data() else {
        return absent(browser);
    };
    let profiles_root = roaming.join("Mozilla").join("Firefox").join("Profiles");
    if !profiles_root.is_dir() {
        return absent(browser);
    }
    let mut latest: Option<(std::time::SystemTime, PathBuf)> = None;
    if let Ok(read) = std::fs::read_dir(&profiles_root) {
        for entry in read.flatten() {
            let cookies = entry.path().join("cookies.sqlite");
            if let Ok(meta) = std::fs::metadata(&cookies) {
                let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                if latest.as_ref().is_none_or(|(t, _)| *t < modified) {
                    latest = Some((modified, cookies));
                }
            }
        }
    }
    match latest {
        Some((_, cookies)) => BrowserPresence {
            browser,
            local_state_path: None,
            profile_root: cookies.parent().map(|p| p.to_path_buf()),
            cookie_db_path: Some(cookies),
        },
        None => absent(browser),
    }
}

fn scan_profile_dirs(user_data: &std::path::Path) -> impl Iterator<Item = PathBuf> {
    std::fs::read_dir(user_data)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|s| s.starts_with("Profile "))
        })
}

fn absent(browser: BrowserId) -> BrowserPresence {
    BrowserPresence {
        browser,
        local_state_path: None,
        profile_root: None,
        cookie_db_path: None,
    }
}

#[cfg(windows)]
fn local_app_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

#[cfg(windows)]
fn roaming_app_data() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

#[cfg(not(windows))]
fn local_app_data() -> Option<PathBuf> {
    None
}
#[cfg(not(windows))]
fn roaming_app_data() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_all_returns_four_results() {
        let results = BrowserDetection::probe_all();
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn absent_browser_reports_not_installed() {
        let presence = absent(BrowserId::Chrome);
        assert!(!presence.is_installed());
    }
}
