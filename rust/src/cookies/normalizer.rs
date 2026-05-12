//! Normalize free form cookie header inputs into `name=value; name=value`.
//!
//! Accepts every shape we have seen real users paste:
//!
//! - `name=value; name2=value2` (bare)
//! - `Cookie: name=value; name2=value2` (with HTTP header prefix)
//! - `-H "Cookie: name=value"` (curl `-H` form)
//! - `-H 'Cookie: name=value'` (curl `-H` with single quotes)
//! - `--header "Cookie: name=value"`
//! - `--cookie "name=value"` (curl `--cookie`)
//! - `-b "name=value"` (curl `-b`)
//!
//! Values may contain `=` (e.g. base64 padding). Pairs are separated by
//! `;` only; commas are part of the value.

pub struct CookieHeaderNormalizer;

impl CookieHeaderNormalizer {
    /// Parse a raw cookie header string into `(name, value)` pairs.
    pub fn pairs(raw: &str) -> Vec<(String, String)> {
        let cleaned = strip_curl_and_prefix(raw);
        cleaned
            .split(';')
            .filter_map(|chunk| {
                let chunk = chunk.trim();
                if chunk.is_empty() {
                    return None;
                }
                let mut split = chunk.splitn(2, '=');
                let name = split.next()?.trim().to_string();
                let value = split.next().unwrap_or("").trim().to_string();
                if name.is_empty() {
                    None
                } else {
                    Some((name, value))
                }
            })
            .collect()
    }

    /// Return `name=value; name=value` for every parsed pair.
    pub fn header(raw: &str) -> String {
        Self::pairs(raw)
            .into_iter()
            .map(|(n, v)| format!("{n}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Return only the pairs whose name is in `allowed`, preserving the
    /// order of the input.
    pub fn filtered_header(raw: &str, allowed: &[&str]) -> String {
        Self::pairs(raw)
            .into_iter()
            .filter(|(n, _)| allowed.iter().any(|a| a.eq_ignore_ascii_case(n)))
            .map(|(n, v)| format!("{n}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Strip curl argument prefixes (`-H`, `--header`, `--cookie`, `-b`), outer
/// quotes (single or double), and a leading case insensitive `Cookie:` or
/// `cookie:` header.
fn strip_curl_and_prefix(input: &str) -> String {
    let mut s = input.trim().to_string();

    // Strip curl flag prefixes if present at the start.
    for prefix in ["-H ", "--header ", "--cookie ", "-b "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim().to_string();
            break;
        }
    }

    // Strip a single layer of matching outer quotes.
    if s.len() >= 2 {
        let first = s.chars().next();
        let last = s.chars().last();
        if first == last && (first == Some('"') || first == Some('\'')) {
            s = s[1..s.len() - 1].to_string();
        }
    }

    // Strip a leading `Cookie:` header (case insensitive). Tolerate
    // whitespace either side of the colon.
    let lowered = s.to_ascii_lowercase();
    if let Some(rest) = lowered.strip_prefix("cookie:") {
        let start = s.len() - rest.len();
        s = s[start..].trim_start().to_string();
    } else if lowered.trim_start().starts_with("cookie ") {
        // Rarely seen: `Cookie ` without the colon. Tolerate.
        if let Some(pos) = s.find(|c: char| c.is_whitespace()) {
            s = s[pos..].trim_start().to_string();
        }
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_header_round_trips() {
        let pairs = CookieHeaderNormalizer::pairs("a=1; b=2; c=3");
        assert_eq!(
            pairs,
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string()),
                ("c".to_string(), "3".to_string())
            ]
        );
    }

    #[test]
    fn cookie_prefix_stripped() {
        let header = CookieHeaderNormalizer::header("Cookie: session=abc; user=def");
        assert_eq!(header, "session=abc; user=def");
    }

    #[test]
    fn curl_h_with_double_quotes() {
        let pairs = CookieHeaderNormalizer::pairs(r#"-H "Cookie: a=1; b=2""#);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("a".to_string(), "1".to_string()));
    }

    #[test]
    fn curl_h_with_single_quotes() {
        let pairs = CookieHeaderNormalizer::pairs("-H 'Cookie: a=1; b=2'");
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn curl_long_header_form() {
        let pairs = CookieHeaderNormalizer::pairs(r#"--header "Cookie: token=xyz""#);
        assert_eq!(pairs, vec![("token".to_string(), "xyz".to_string())]);
    }

    #[test]
    fn curl_cookie_form() {
        let pairs = CookieHeaderNormalizer::pairs("--cookie \"name=value; name2=value2\"");
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn curl_b_form() {
        let pairs = CookieHeaderNormalizer::pairs(r#"-b "name=value""#);
        assert_eq!(pairs, vec![("name".to_string(), "value".to_string())]);
    }

    #[test]
    fn value_can_contain_equals_signs() {
        let pairs = CookieHeaderNormalizer::pairs("session=abc=def==");
        assert_eq!(
            pairs,
            vec![("session".to_string(), "abc=def==".to_string())]
        );
    }

    #[test]
    fn whitespace_tolerated_around_pairs() {
        let pairs = CookieHeaderNormalizer::pairs("   a = 1 ;   b = 2   ");
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("a".to_string(), "1".to_string()));
    }

    #[test]
    fn empty_pairs_dropped() {
        let pairs = CookieHeaderNormalizer::pairs("a=1; ; b=2;;");
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn filtered_header_keeps_only_allowed() {
        let raw = "Cookie: session=abc; tracking=xyz; user=def";
        let filtered = CookieHeaderNormalizer::filtered_header(raw, &["session", "user"]);
        assert_eq!(filtered, "session=abc; user=def");
    }

    #[test]
    fn filtered_header_is_case_insensitive() {
        let raw = "SESSION=abc; User=def";
        let filtered = CookieHeaderNormalizer::filtered_header(raw, &["session", "user"]);
        assert_eq!(filtered, "SESSION=abc; User=def");
    }

    #[test]
    fn leading_whitespace_in_curl_input_is_tolerated() {
        let pairs = CookieHeaderNormalizer::pairs("   -H 'Cookie: a=1'");
        assert_eq!(pairs, vec![("a".to_string(), "1".to_string())]);
    }
}
