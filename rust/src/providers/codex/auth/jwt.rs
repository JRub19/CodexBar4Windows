//! Minimal JWT payload decoder. We do not verify the signature; the
//! token is used only as a passive source of identity hints (email,
//! plan, account id). The OpenAI auth server signed it; if anyone has
//! tampered with the local file, the resulting bad payload simply
//! degrades to `CodexIdentity::Unresolved` because we apply tolerant
//! fallbacks per field.
//!
//! Tolerant decode contract: no panics on malformed input. Per-field
//! decode failures are isolated.

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    #[error("token does not have three dot-separated segments")]
    Shape,
    #[error("payload segment is not valid base64url: {0}")]
    Base64(String),
    #[error("payload is not valid JSON: {0}")]
    Json(String),
}

/// Decode the payload (middle segment) of a JWT into a generic JSON
/// value. The signature segment is dropped on purpose.
pub fn decode_payload(token: &str) -> Result<serde_json::Value, JwtError> {
    let mut parts = token.split('.');
    let _header = parts.next().ok_or(JwtError::Shape)?;
    let payload = parts.next().ok_or(JwtError::Shape)?;
    let _signature = parts.next().ok_or(JwtError::Shape)?;
    if parts.next().is_some() {
        return Err(JwtError::Shape);
    }
    let bytes = decode_base64url(payload).map_err(|e| JwtError::Base64(e.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|e| JwtError::Json(e.to_string()))
}

/// Pull the `email` claim out of a payload using the three-step ladder:
/// 1. `payload.email`
/// 2. `payload["https://api.openai.com/profile"].email`
/// 3. `None`
pub fn extract_email(payload: &serde_json::Value) -> Option<String> {
    if let Some(email) = payload.get("email").and_then(|v| v.as_str()) {
        return Some(normalize_email(email));
    }
    let profile = payload.get("https://api.openai.com/profile")?;
    let email = profile.get("email")?.as_str()?;
    Some(normalize_email(email))
}

/// `chatgpt_plan_type` ladder per spec 41 §3.2.
pub fn extract_plan(payload: &serde_json::Value) -> Option<String> {
    if let Some(auth) = payload.get("https://api.openai.com/auth") {
        if let Some(plan) = auth.get("chatgpt_plan_type").and_then(|v| v.as_str()) {
            return Some(plan.to_string());
        }
    }
    payload
        .get("chatgpt_plan_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Account id ladder. The Codex CLI prefers `tokens.account_id`
/// (read from the credential file), so the caller passes it in as the
/// `cli_account_id` parameter and we only fall through to JWT claims
/// when it is `None`.
pub fn extract_account_id(
    cli_account_id: Option<&str>,
    payload: &serde_json::Value,
) -> Option<String> {
    if let Some(id) = cli_account_id {
        return Some(normalize_account_id(id));
    }
    if let Some(auth) = payload.get("https://api.openai.com/auth") {
        if let Some(id) = auth.get("chatgpt_account_id").and_then(|v| v.as_str()) {
            return Some(normalize_account_id(id));
        }
    }
    payload
        .get("chatgpt_account_id")
        .and_then(|v| v.as_str())
        .map(normalize_account_id)
}

fn normalize_email(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn normalize_account_id(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

/// Base64url decoder with optional padding. Mirrors the loose decoder
/// real-world JWT producers expect.
fn decode_base64url(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::engine::general_purpose;
    use base64::Engine;
    // Strip ASCII whitespace then pad with `=` to a multiple of 4.
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let padded = match cleaned.len() % 4 {
        0 => cleaned,
        n => {
            let mut buf = cleaned;
            buf.push_str(&"=".repeat(4 - n));
            buf
        }
    };
    general_purpose::URL_SAFE.decode(padded)
}

/// Make a fake JWT for tests. Header is fixed; the test passes in the
/// payload JSON. Signature is a placeholder we never verify.
#[cfg(test)]
pub fn make_token(payload: &serde_json::Value) -> String {
    use base64::engine::general_purpose;
    use base64::Engine;
    let header = general_purpose::URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let body = general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(payload).unwrap());
    let sig = general_purpose::URL_SAFE_NO_PAD.encode(b"signature");
    format!("{header}.{body}.{sig}")
}

/// Convenience extractor used by `CodexIdentity::from_id_token` so the
/// caller can pull all three fields without re-decoding the payload.
pub fn extract_all(id_token: &str, cli_account_id: Option<&str>) -> ExtractedClaims {
    let Ok(payload) = decode_payload(id_token) else {
        return ExtractedClaims::default();
    };
    ExtractedClaims {
        email: extract_email(&payload),
        plan: extract_plan(&payload),
        account_id: extract_account_id(cli_account_id, &payload),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ExtractedClaims {
    pub email: Option<String>,
    pub plan: Option<String>,
    pub account_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_email_uses_top_level_first() {
        let payload = json!({
            "email": "  Jonas@Skrylabs.COM  ",
            "https://api.openai.com/profile": { "email": "other@x.com" }
        });
        assert_eq!(
            extract_email(&payload).as_deref(),
            Some("jonas@skrylabs.com")
        );
    }

    #[test]
    fn extract_email_falls_back_to_profile_claim() {
        let payload = json!({
            "https://api.openai.com/profile": { "email": "user@example.com" }
        });
        assert_eq!(extract_email(&payload).as_deref(), Some("user@example.com"));
    }

    #[test]
    fn extract_email_returns_none_when_absent() {
        let payload = json!({});
        assert!(extract_email(&payload).is_none());
    }

    #[test]
    fn extract_plan_uses_namespaced_claim_first() {
        let payload = json!({
            "https://api.openai.com/auth": { "chatgpt_plan_type": "plus" },
            "chatgpt_plan_type": "free"
        });
        assert_eq!(extract_plan(&payload).as_deref(), Some("plus"));
    }

    #[test]
    fn extract_plan_falls_back_to_root() {
        let payload = json!({ "chatgpt_plan_type": "pro" });
        assert_eq!(extract_plan(&payload).as_deref(), Some("pro"));
    }

    #[test]
    fn extract_account_id_prefers_cli_value() {
        let payload = json!({ "chatgpt_account_id": "abc" });
        assert_eq!(
            extract_account_id(Some("CLI-ID"), &payload).as_deref(),
            Some("cli-id")
        );
    }

    #[test]
    fn extract_account_id_falls_back_through_ladder() {
        let payload = json!({
            "https://api.openai.com/auth": { "chatgpt_account_id": "NS-ID" }
        });
        assert_eq!(extract_account_id(None, &payload).as_deref(), Some("ns-id"));
    }

    #[test]
    fn decode_payload_round_trips_a_made_token() {
        let payload = json!({ "email": "u@v.com" });
        let token = make_token(&payload);
        let decoded = decode_payload(&token).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn malformed_token_returns_shape_error() {
        assert!(matches!(
            decode_payload("only-two.parts").unwrap_err(),
            JwtError::Shape
        ));
        assert!(matches!(
            decode_payload("a.b.c.d").unwrap_err(),
            JwtError::Shape
        ));
    }

    #[test]
    fn malformed_base64_payload_returns_base64_error() {
        let err = decode_payload("h.!!!.s").unwrap_err();
        assert!(matches!(err, JwtError::Base64(_)));
    }
}
