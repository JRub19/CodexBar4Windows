//! Locale bundle loader and lookup helpers.
//!
//! Bundles are embedded at compile time (`include_str!`) so the binary has
//! no external dependency at runtime. `lookup` resolves a dot path against
//! the active locale and falls back to English on a missing key.

use std::collections::HashMap;
use std::sync::RwLock;

use once_cell::sync::Lazy;
use serde_json::Value;

use super::DEFAULT_LOCALE;

#[derive(Debug, thiserror::Error)]
pub enum LocaleError {
    #[error("locale bundle for {0} is malformed: {1}")]
    Malformed(&'static str, String),
    #[error("locale {0} is not registered")]
    Unknown(String),
}

const EN_BUNDLE: &str = include_str!("en.json");

static BUNDLES: Lazy<HashMap<&'static str, Value>> = Lazy::new(|| {
    let mut map = HashMap::new();
    let en: Value = serde_json::from_str(EN_BUNDLE)
        .expect("the english locale bundle must parse; ship a valid en.json");
    map.insert(DEFAULT_LOCALE, en);
    map
});

static ACTIVE: Lazy<RwLock<String>> = Lazy::new(|| RwLock::new(DEFAULT_LOCALE.to_string()));

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveLocale(pub String);

/// Set the locale that subsequent [`lookup`] calls resolve against.
///
/// If `locale` is not registered the active locale stays at its previous
/// value and the call returns [`LocaleError::Unknown`].
pub fn set_active_locale(locale: &str) -> Result<(), LocaleError> {
    if !BUNDLES.contains_key(locale) {
        return Err(LocaleError::Unknown(locale.to_string()));
    }
    let mut guard = ACTIVE.write().expect("locale lock poisoned");
    *guard = locale.to_string();
    Ok(())
}

/// Look up a key in the active locale, falling back to English.
pub fn lookup(key: &str) -> String {
    let active = ACTIVE.read().expect("locale lock poisoned").clone();
    lookup_in(&active, key)
}

/// Look up a key in a specific locale, falling back to English.
pub fn lookup_in(locale: &str, key: &str) -> String {
    if let Some(found) = BUNDLES.get(locale).and_then(|v| traverse(v, key)) {
        return found;
    }
    if locale != DEFAULT_LOCALE {
        if let Some(found) = BUNDLES.get(DEFAULT_LOCALE).and_then(|v| traverse(v, key)) {
            return found;
        }
    }
    format!("[missing:{key}]")
}

fn traverse(root: &Value, key: &str) -> Option<String> {
    let mut current = root;
    for segment in key.split('.') {
        current = current.get(segment)?;
    }
    current.as_str().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_bundle_parses() {
        let val = BUNDLES.get(DEFAULT_LOCALE).expect("en bundle present");
        assert!(val.is_object());
    }

    #[test]
    fn lookup_returns_known_string() {
        assert_eq!(
            lookup("popup.empty_state"),
            "No providers configured. Open Preferences to enable."
        );
    }

    #[test]
    fn lookup_returns_placeholder_for_missing_key() {
        assert_eq!(lookup("does.not.exist"), "[missing:does.not.exist]");
    }

    #[test]
    fn tray_menu_keys_are_present() {
        for key in [
            "tray.menu.refresh_now",
            "tray.menu.pause_refresh",
            "tray.menu.resume_refresh",
            "tray.menu.preferences",
            "tray.menu.quit",
        ] {
            assert!(
                !lookup(key).starts_with("[missing"),
                "missing tray key: {key}"
            );
        }
    }
}
