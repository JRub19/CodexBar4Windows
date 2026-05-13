//! Statuspage.io `/api/v2/status.json` parser. Common across
//! OpenAI / Anthropic / Cursor / Factory / GitHub. The schema is
//! stable across Atlassian Statuspage tenants.

use chrono::DateTime;
use serde::Deserialize;

use super::severity::StatusSeverity;

#[derive(Debug, Deserialize)]
pub struct StatusPayload {
    #[serde(default)]
    pub page: Option<PagePayload>,
    pub status: StatusBlock,
}

#[derive(Debug, Deserialize)]
pub struct PagePayload {
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StatusBlock {
    #[serde(default)]
    pub indicator: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedStatus {
    pub severity: StatusSeverity,
    pub title: Option<String>,
    pub updated_at_unix_secs: Option<i64>,
}

pub fn parse(bytes: &[u8]) -> Result<ParsedStatus, String> {
    let payload: StatusPayload =
        serde_json::from_slice(bytes).map_err(|e| format!("statuspage decode: {e}"))?;
    let severity = match payload.status.indicator.as_deref() {
        Some("none") => StatusSeverity::None,
        Some("minor") => StatusSeverity::Minor,
        Some("major") => StatusSeverity::Major,
        Some("critical") => StatusSeverity::Critical,
        Some("maintenance") => StatusSeverity::Maintenance,
        _ => StatusSeverity::Unknown,
    };
    let title = payload
        .status
        .description
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let updated_at_unix_secs = payload
        .page
        .as_ref()
        .and_then(|p| p.updated_at.as_deref())
        .and_then(parse_iso_to_unix_secs);
    Ok(ParsedStatus {
        severity,
        title,
        updated_at_unix_secs,
    })
}

fn parse_iso_to_unix_secs(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value).ok().map(|d| d.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_operational_indicator() {
        let body = br#"{
            "page": {"updated_at": "2026-05-13T10:00:00Z"},
            "status": {"indicator": "none", "description": "All Systems Operational"}
        }"#;
        let parsed = parse(body).unwrap();
        assert_eq!(parsed.severity, StatusSeverity::None);
        assert_eq!(parsed.title.as_deref(), Some("All Systems Operational"));
        assert!(parsed.updated_at_unix_secs.is_some());
    }

    #[test]
    fn parses_each_indicator_variant() {
        for (raw, expected) in [
            ("minor", StatusSeverity::Minor),
            ("major", StatusSeverity::Major),
            ("critical", StatusSeverity::Critical),
            ("maintenance", StatusSeverity::Maintenance),
        ] {
            let body =
                format!("{{\"status\": {{\"indicator\": \"{raw}\", \"description\": \"x\"}}}}");
            assert_eq!(parse(body.as_bytes()).unwrap().severity, expected);
        }
    }

    #[test]
    fn unknown_indicator_maps_to_unknown_variant() {
        let body = br#"{"status": {"indicator": "novel-future-state"}}"#;
        assert_eq!(parse(body).unwrap().severity, StatusSeverity::Unknown);
    }

    #[test]
    fn empty_description_is_dropped() {
        let body = br#"{"status": {"indicator": "none", "description": "   "}}"#;
        assert!(parse(body).unwrap().title.is_none());
    }

    #[test]
    fn malformed_payload_yields_decode_error() {
        assert!(parse(b"not json").is_err());
    }

    #[test]
    fn page_updated_at_with_fractional_seconds_decodes() {
        let body = br#"{
            "page": {"updated_at": "2026-05-13T10:00:00.123Z"},
            "status": {"indicator": "none"}
        }"#;
        assert!(parse(body).unwrap().updated_at_unix_secs.is_some());
    }
}
