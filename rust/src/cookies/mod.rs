//! Browser cookie import subsystem.
//!
//! Public traits:
//!
//! - [`BrowserCookieImporter`]: per browser implementation that returns
//!   `HttpCookie` values for one or more domains.
//! - The high level orchestrator (`CookieImporter`) lands in task 2.13 and
//!   composes Chromium, Firefox, and the manual paste path through a
//!   single `import_for(provider_id, domains)` call.
//!
//! Phase 2.6 lands the trait, the error taxonomy, and the browser
//! detection probe. Phase 2.7 onwards fills in the implementations.

pub mod chromium;
pub mod detect;
pub mod errors;
pub mod firefox;
pub mod header_cache;
pub mod normalizer;

pub use chromium::ChromiumCookieReader;
pub use detect::{BrowserDetection, BrowserPresence};
pub use errors::ImportError;
pub use firefox::FirefoxCookieReader;
pub use header_cache::{CachedHeader, CookieHeaderCache};
pub use normalizer::CookieHeaderNormalizer;

/// One cookie value as it appears in an HTTP `Cookie:` header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpCookie {
    pub host: String,
    pub name: String,
    pub value: String,
    pub path: String,
    pub is_secure: bool,
    pub is_http_only: bool,
}

/// Stable identifier for a browser. The string is `&'static str` so it
/// matches the persistence key style used elsewhere in the crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BrowserId {
    Chrome,
    Edge,
    Brave,
    Firefox,
}

impl BrowserId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Edge => "edge",
            Self::Brave => "brave",
            Self::Firefox => "firefox",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Chrome => "Google Chrome",
            Self::Edge => "Microsoft Edge",
            Self::Brave => "Brave",
            Self::Firefox => "Firefox",
        }
    }

    pub fn is_chromium(&self) -> bool {
        matches!(self, Self::Chrome | Self::Edge | Self::Brave)
    }
}

pub trait BrowserCookieImporter: Send + Sync {
    fn browser(&self) -> BrowserId;
    fn import_for(&self, domains: &[&str]) -> Result<Vec<HttpCookie>, ImportError>;
}
