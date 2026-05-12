//! Codex web endpoints. Spec 41 §3.7 lists every URL the chatgpt.com
//! session uses; we restrict ourselves to the usage rollup and the
//! account read because that is all the popup card needs today.

pub const HOST: &str = "https://chatgpt.com";

/// Same WHAM endpoint as the OAuth path. Backend accepts either the
/// `Authorization: Bearer` header or a chatgpt.com session cookie.
pub const USAGE_PATH: &str = "/backend-api/wham/usage";

/// Account info — used to surface email + plan in the popup card.
pub const ACCOUNT_PATH: &str = "/backend-api/me";

pub fn url(path: &str) -> String {
    format!("{HOST}{path}")
}
