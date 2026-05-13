//! Tiny JWT payload decoder for the Gemini id_token. We only care about
//! `email` and `hd` (hosted-domain) so the popup can show the right
//! plan-label. Matches `extractClaimsFromToken` in
//! `GeminiStatusProbe.swift`.

use base64::Engine;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GoogleTokenClaims {
    pub email: Option<String>,
    pub hosted_domain: Option<String>,
}

pub fn extract_claims(id_token: Option<&str>) -> GoogleTokenClaims {
    let Some(token) = id_token else {
        return GoogleTokenClaims::default();
    };
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return GoogleTokenClaims::default();
    }
    let padded = pad_base64(parts[1]);
    let Ok(bytes) = base64::engine::general_purpose::URL_SAFE
        .decode(padded.as_bytes())
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(padded.as_bytes()))
    else {
        return GoogleTokenClaims::default();
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return GoogleTokenClaims::default();
    };
    GoogleTokenClaims {
        email: value
            .get("email")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        hosted_domain: value
            .get("hd")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

fn pad_base64(input: &str) -> String {
    let mut s: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    match s.len() % 4 {
        0 => s,
        n => {
            s.push_str(&"=".repeat(4 - n));
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn jwt_with_payload(payload: &str) -> String {
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.as_bytes());
        format!("header.{encoded}.sig")
    }

    #[test]
    fn extracts_email_and_hosted_domain() {
        let token = jwt_with_payload(r#"{"email": "u@example.com", "hd": "example.com"}"#);
        let claims = extract_claims(Some(&token));
        assert_eq!(claims.email.as_deref(), Some("u@example.com"));
        assert_eq!(claims.hosted_domain.as_deref(), Some("example.com"));
    }

    #[test]
    fn missing_token_returns_empty_claims() {
        assert_eq!(extract_claims(None), GoogleTokenClaims::default());
    }

    #[test]
    fn malformed_token_returns_empty_claims() {
        assert_eq!(
            extract_claims(Some("no-dots")),
            GoogleTokenClaims::default()
        );
        assert_eq!(extract_claims(Some("a.b")), {
            // Two parts is fine for our purposes, but garbage base64 → empty.
            GoogleTokenClaims::default()
        });
    }

    #[test]
    fn personal_account_has_no_hosted_domain() {
        let token = jwt_with_payload(r#"{"email": "personal@gmail.com"}"#);
        let claims = extract_claims(Some(&token));
        assert_eq!(claims.email.as_deref(), Some("personal@gmail.com"));
        assert!(claims.hosted_domain.is_none());
    }
}
