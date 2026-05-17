//! Locale strings. English is the source of truth; other locales fall back
//! to English on a missing key. Keys use dot separated paths mirroring the
//! Mac `Localizable.xcstrings` convention so a future sync script can diff
//! against upstream.

pub mod loader;

pub use loader::{lookup, lookup_in, set_active_locale, ActiveLocale, LocaleError};

pub const DEFAULT_LOCALE: &str = "en";
