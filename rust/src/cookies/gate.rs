//! Per (provider, browser) cooldown to avoid hammering DPAPI / SQLite when
//! we already know the path is failing (e.g. v20 cookies on Chrome).
//!
//! In memory only at this phase. Phase 2.15 wraps the file backed
//! persistence via the settings store so cooldowns survive restarts.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::BrowserId;

/// Default cooldown when a path failed: six hours. Aligns with the spec
/// recommendation in `docs/windows/spec/60-auth-cookies-secrets.md` §6.
pub const DEFAULT_COOLDOWN: Duration = Duration::from_secs(6 * 3600);

pub struct CookieAccessGate {
    cooldowns: Mutex<HashMap<GateKey, Instant>>,
    default_cooldown: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct GateKey {
    provider: String,
    browser: BrowserId,
}

impl Default for CookieAccessGate {
    fn default() -> Self {
        Self {
            cooldowns: Mutex::new(HashMap::new()),
            default_cooldown: DEFAULT_COOLDOWN,
        }
    }
}

impl CookieAccessGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cooldown(mut self, d: Duration) -> Self {
        self.default_cooldown = d;
        self
    }

    /// Record that the (provider, browser) path just failed; subsequent
    /// `is_open` calls return false until the cooldown expires.
    pub fn mark_failure(&self, provider: &str, browser: BrowserId) {
        let until = Instant::now() + self.default_cooldown;
        let mut guard = self.cooldowns.lock().expect("cookie gate poisoned");
        guard.insert(
            GateKey {
                provider: provider.to_string(),
                browser,
            },
            until,
        );
    }

    /// Returns true when the (provider, browser) path is allowed (no
    /// active cooldown), false otherwise.
    pub fn is_open(&self, provider: &str, browser: BrowserId) -> bool {
        let key = GateKey {
            provider: provider.to_string(),
            browser,
        };
        let mut guard = self.cooldowns.lock().expect("cookie gate poisoned");
        match guard.get(&key) {
            Some(until) if *until > Instant::now() => false,
            Some(_) => {
                guard.remove(&key);
                true
            }
            None => true,
        }
    }

    /// Force the cooldown for one (provider, browser) to expire now.
    pub fn clear(&self, provider: &str, browser: BrowserId) {
        let mut guard = self.cooldowns.lock().expect("cookie gate poisoned");
        guard.remove(&GateKey {
            provider: provider.to_string(),
            browser,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_gate_is_open_for_any_pair() {
        let gate = CookieAccessGate::new();
        assert!(gate.is_open("claude", BrowserId::Chrome));
        assert!(gate.is_open("codex", BrowserId::Firefox));
    }

    #[test]
    fn mark_failure_closes_only_the_named_pair() {
        let gate = CookieAccessGate::new();
        gate.mark_failure("claude", BrowserId::Chrome);
        assert!(!gate.is_open("claude", BrowserId::Chrome));
        assert!(gate.is_open("claude", BrowserId::Firefox));
        assert!(gate.is_open("codex", BrowserId::Chrome));
    }

    #[test]
    fn clear_re_opens_the_pair() {
        let gate = CookieAccessGate::new();
        gate.mark_failure("claude", BrowserId::Chrome);
        gate.clear("claude", BrowserId::Chrome);
        assert!(gate.is_open("claude", BrowserId::Chrome));
    }

    #[test]
    fn expired_cooldown_is_removed_on_query() {
        let gate = CookieAccessGate::new().with_cooldown(Duration::from_millis(1));
        gate.mark_failure("claude", BrowserId::Chrome);
        std::thread::sleep(Duration::from_millis(10));
        assert!(gate.is_open("claude", BrowserId::Chrome));
    }
}
