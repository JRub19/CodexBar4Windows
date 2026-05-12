//! Detect whether a user-supplied token should flow through the OAuth
//! path or the Web cookie path. Spec 40 section 3.10 documents the
//! three input shapes users paste:
//!
//! - `sk-ant-oat...`            -> OAuth bearer token.
//! - `sessionKey=...`           -> Web cookie value (raw).
//! - `Cookie: sessionKey=...`   -> Web cookie header (full).
//!
//! A `Bearer ` prefix on either OAuth or web inputs is stripped, case
//! insensitive, so users can paste from curl examples without surprise.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutedToken {
    /// An OAuth bearer token to send as `Authorization: Bearer ...`.
    OAuth { access_token: String },
    /// A `Cookie:` header to send verbatim.
    WebCookieHeader { header: String },
}

const OAUTH_PREFIX: &str = "sk-ant-oat";

pub fn route(raw: &str) -> Option<RoutedToken> {
    let cleaned = strip_bearer_prefix(raw.trim()).trim();
    if cleaned.is_empty() {
        return None;
    }
    if cleaned.starts_with(OAUTH_PREFIX) {
        return Some(RoutedToken::OAuth {
            access_token: cleaned.to_string(),
        });
    }
    // Accept the full `Cookie:` header or a bare `sessionKey=...` value.
    let normalized = if let Some(rest) = cleaned.strip_prefix("Cookie:") {
        rest.trim().to_string()
    } else if let Some(rest) = cleaned.strip_prefix("cookie:") {
        rest.trim().to_string()
    } else {
        cleaned.to_string()
    };
    if !normalized.contains("sessionKey=") {
        return None;
    }
    Some(RoutedToken::WebCookieHeader { header: normalized })
}

fn strip_bearer_prefix(value: &str) -> &str {
    let lower_prefix = "bearer ";
    let lower = value.to_ascii_lowercase();
    if lower.starts_with(lower_prefix) {
        &value[lower_prefix.len()..]
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_sk_ant_oat_as_oauth() {
        match route("sk-ant-oat01-abc123").unwrap() {
            RoutedToken::OAuth { access_token } => {
                assert_eq!(access_token, "sk-ant-oat01-abc123");
            }
            other => panic!("expected oauth, got {other:?}"),
        }
    }

    #[test]
    fn strips_bearer_prefix_case_insensitive() {
        for variant in [
            "Bearer sk-ant-oat-x",
            "bearer sk-ant-oat-x",
            "BEARER sk-ant-oat-x",
        ] {
            match route(variant).unwrap() {
                RoutedToken::OAuth { access_token } => assert_eq!(access_token, "sk-ant-oat-x"),
                other => panic!("expected oauth for '{variant}', got {other:?}"),
            }
        }
    }

    #[test]
    fn bare_session_key_routes_as_web_cookie() {
        match route("sessionKey=abcdef").unwrap() {
            RoutedToken::WebCookieHeader { header } => {
                assert_eq!(header, "sessionKey=abcdef");
            }
            other => panic!("expected web cookie, got {other:?}"),
        }
    }

    #[test]
    fn full_cookie_header_strips_the_prefix() {
        match route("Cookie: sessionKey=abc; other=1").unwrap() {
            RoutedToken::WebCookieHeader { header } => {
                assert_eq!(header, "sessionKey=abc; other=1");
            }
            other => panic!("expected web cookie, got {other:?}"),
        }
    }

    #[test]
    fn empty_input_returns_none() {
        assert!(route("").is_none());
        assert!(route("   ").is_none());
    }

    #[test]
    fn unrecognized_input_returns_none() {
        assert!(route("randomstring").is_none());
        assert!(route("Bearer randomstring").is_none());
    }
}
