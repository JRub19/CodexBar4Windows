//! How a provider expects its cookies to be sourced when the Web
//! strategy needs them. Phase 4 reads this enum from `ProviderDescriptor`
//! to drive the cookie importer.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CookieSource {
    /// Cookies are not used by this provider's Web strategy.
    None,
    /// Try every supported browser in import order.
    AnyBrowser,
    /// Restrict to a single browser. Used by providers that ship a
    /// dedicated extension only available in one browser family.
    Browser(BrowserKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum BrowserKind {
    Chrome,
    Edge,
    Brave,
    Firefox,
}

/// Default import order matching spec 60 section 4.3: Chrome, Edge,
/// Brave, then Firefox. Providers may override per descriptor.
pub const DEFAULT_BROWSER_IMPORT_ORDER: &[BrowserKind] = &[
    BrowserKind::Chrome,
    BrowserKind::Edge,
    BrowserKind::Brave,
    BrowserKind::Firefox,
];
