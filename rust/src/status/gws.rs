//! Google Workspace `appsstatus/incidents.json` parser. Used by
//! Gemini and Antigravity (which ride on the same GWS product id).
//!
//! Spec: docs/windows/spec/55-status-incidents.md §2.4.
//! For each incident we look at `currently_affected_products` first;
//! when absent we fall back to `affected_products`. We filter by the
//! provider's product id, then among the remaining active incidents
//! (`end == null`) we pick the most severe.

use chrono::DateTime;
use serde::Deserialize;

use super::severity::StatusSeverity;
use super::statuspage::ParsedStatus;

#[derive(Debug, Deserialize)]
pub struct IncidentsPayload(pub Vec<Incident>);

#[derive(Debug, Deserialize, Clone)]
pub struct Incident {
    #[serde(default)]
    pub begin: Option<String>,
    #[serde(default)]
    pub modified: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub external_desc: Option<String>,
    #[serde(default)]
    pub status_impact: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub affected_products: Vec<AffectedProduct>,
    #[serde(default)]
    pub currently_affected_products: Vec<AffectedProduct>,
    #[serde(default)]
    pub most_recent_update: Option<IncidentUpdate>,
    #[serde(default)]
    pub updates: Vec<IncidentUpdate>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AffectedProduct {
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IncidentUpdate {
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

pub fn parse(bytes: &[u8], product_id: &str) -> Result<ParsedStatus, String> {
    let payload: IncidentsPayload =
        serde_json::from_slice(bytes).map_err(|e| format!("gws decode: {e}"))?;
    let active: Vec<&Incident> = payload
        .0
        .iter()
        .filter(|incident| incident.end.is_none() && touches_product(incident, product_id))
        .collect();
    if active.is_empty() {
        return Ok(ParsedStatus {
            severity: StatusSeverity::None,
            title: None,
            updated_at_unix_secs: None,
        });
    }
    // Pick the most severe; on tie, the first one (Google sorts most
    // recent first in practice but we do not rely on that).
    let worst = active
        .iter()
        .max_by_key(|i| severity_for(i).rank())
        .copied()
        .expect("active is non-empty");
    let severity = severity_for(worst);
    let title = title_for(worst);
    let updated_at_unix_secs = updated_at_for(worst);
    Ok(ParsedStatus {
        severity,
        title,
        updated_at_unix_secs,
    })
}

fn touches_product(incident: &Incident, product_id: &str) -> bool {
    let in_current = incident
        .currently_affected_products
        .iter()
        .any(|p| p.id.as_deref() == Some(product_id));
    if !incident.currently_affected_products.is_empty() {
        return in_current;
    }
    incident
        .affected_products
        .iter()
        .any(|p| p.id.as_deref() == Some(product_id))
}

fn severity_for(incident: &Incident) -> StatusSeverity {
    match incident.status_impact.as_deref() {
        Some("AVAILABLE") => StatusSeverity::None,
        Some("SERVICE_INFORMATION") => StatusSeverity::Minor,
        Some("SERVICE_DISRUPTION") => StatusSeverity::Major,
        Some("SERVICE_OUTAGE") => StatusSeverity::Critical,
        Some("SERVICE_MAINTENANCE") | Some("SCHEDULED_MAINTENANCE") => StatusSeverity::Maintenance,
        _ => match incident.severity.as_deref() {
            Some("low") => StatusSeverity::Minor,
            Some("medium") => StatusSeverity::Major,
            Some("high") => StatusSeverity::Critical,
            _ => StatusSeverity::Unknown,
        },
    }
}

fn title_for(incident: &Incident) -> Option<String> {
    let candidates = [
        incident
            .most_recent_update
            .as_ref()
            .and_then(|u| u.text.as_deref()),
        incident.updates.last().and_then(|u| u.text.as_deref()),
        incident.external_desc.as_deref(),
    ];
    for text in candidates.into_iter().flatten() {
        let cleaned = strip_markdown(text).trim().to_string();
        if !cleaned.is_empty() {
            return Some(cleaned);
        }
    }
    None
}

fn updated_at_for(incident: &Incident) -> Option<i64> {
    let candidates = [
        incident
            .most_recent_update
            .as_ref()
            .and_then(|u| u.when.as_deref()),
        incident.updates.last().and_then(|u| u.when.as_deref()),
        incident.modified.as_deref(),
        incident.begin.as_deref(),
    ];
    for raw in candidates.into_iter().flatten() {
        if let Some(secs) = parse_iso(raw) {
            return Some(secs);
        }
    }
    None
}

fn parse_iso(raw: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|d| d.timestamp())
}

/// Light markdown stripper. GWS updates frequently embed `**bold**`,
/// markdown links `[text](url)`, and bullet points starting with `* `.
/// We render the popup as plain text so we collapse those to readable
/// content.
fn strip_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                // Skip a paired ** for bold; collapse single * bullets
                // by emitting a single space.
                if chars.peek() == Some(&'*') {
                    chars.next();
                } else {
                    out.push(' ');
                }
            }
            '[' => {
                // Markdown link: keep the text, drop the (url) tail.
                let mut text_part = String::new();
                while let Some(&peek) = chars.peek() {
                    if peek == ']' {
                        chars.next();
                        break;
                    }
                    text_part.push(peek);
                    chars.next();
                }
                if chars.peek() == Some(&'(') {
                    chars.next();
                    for peek in chars.by_ref() {
                        if peek == ')' {
                            break;
                        }
                    }
                }
                out.push_str(&text_part);
            }
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEMINI_PRODUCT_ID: &str = "npdyhgECDJ6tB66MxXyo";

    #[test]
    fn empty_incident_list_yields_operational() {
        let parsed = parse(b"[]", GEMINI_PRODUCT_ID).unwrap();
        assert_eq!(parsed.severity, StatusSeverity::None);
        assert!(parsed.title.is_none());
    }

    #[test]
    fn filters_out_incidents_for_other_products() {
        let body = br#"[
            {
                "begin": "2026-05-13T09:00:00Z",
                "end": null,
                "status_impact": "SERVICE_DISRUPTION",
                "external_desc": "Search outage",
                "affected_products": [{"id": "other-product"}]
            }
        ]"#;
        let parsed = parse(body, GEMINI_PRODUCT_ID).unwrap();
        assert_eq!(parsed.severity, StatusSeverity::None);
    }

    #[test]
    fn ignores_resolved_incidents_with_non_null_end() {
        let body = format!(
            r#"[
                {{"begin": "2026-05-13T09:00:00Z",
                  "end": "2026-05-13T10:00:00Z",
                  "status_impact": "SERVICE_OUTAGE",
                  "currently_affected_products": [{{"id": "{GEMINI_PRODUCT_ID}"}}]
                }}
            ]"#
        );
        let parsed = parse(body.as_bytes(), GEMINI_PRODUCT_ID).unwrap();
        assert_eq!(parsed.severity, StatusSeverity::None);
    }

    #[test]
    fn picks_most_severe_active_incident() {
        let body = format!(
            r#"[
                {{"begin":"2026-05-13T09:00:00Z","end":null,
                  "status_impact":"SERVICE_INFORMATION",
                  "currently_affected_products":[{{"id":"{GEMINI_PRODUCT_ID}"}}],
                  "external_desc":"minor"}},
                {{"begin":"2026-05-13T09:30:00Z","end":null,
                  "status_impact":"SERVICE_OUTAGE",
                  "currently_affected_products":[{{"id":"{GEMINI_PRODUCT_ID}"}}],
                  "external_desc":"outage"}}
            ]"#
        );
        let parsed = parse(body.as_bytes(), GEMINI_PRODUCT_ID).unwrap();
        assert_eq!(parsed.severity, StatusSeverity::Critical);
        assert_eq!(parsed.title.as_deref(), Some("outage"));
    }

    #[test]
    fn falls_back_to_affected_products_when_currently_empty() {
        let body = format!(
            r#"[
                {{"begin":"2026-05-13T09:00:00Z","end":null,
                  "status_impact":"SERVICE_MAINTENANCE",
                  "affected_products":[{{"id":"{GEMINI_PRODUCT_ID}"}}],
                  "external_desc":"window"}}
            ]"#
        );
        let parsed = parse(body.as_bytes(), GEMINI_PRODUCT_ID).unwrap();
        assert_eq!(parsed.severity, StatusSeverity::Maintenance);
    }

    #[test]
    fn falls_back_to_severity_field_when_status_impact_missing() {
        let body = format!(
            r#"[
                {{"begin":"2026-05-13T09:00:00Z","end":null,
                  "severity":"medium",
                  "currently_affected_products":[{{"id":"{GEMINI_PRODUCT_ID}"}}],
                  "external_desc":"derp"}}
            ]"#
        );
        let parsed = parse(body.as_bytes(), GEMINI_PRODUCT_ID).unwrap();
        assert_eq!(parsed.severity, StatusSeverity::Major);
    }

    #[test]
    fn prefers_most_recent_update_text_for_title() {
        let body = format!(
            r#"[
                {{"begin":"2026-05-13T09:00:00Z","end":null,
                  "status_impact":"SERVICE_OUTAGE",
                  "currently_affected_products":[{{"id":"{GEMINI_PRODUCT_ID}"}}],
                  "external_desc":"old text",
                  "most_recent_update":{{"text":"**Fresh** update [details](https://x)","when":"2026-05-13T10:00:00Z"}}
                }}
            ]"#
        );
        let parsed = parse(body.as_bytes(), GEMINI_PRODUCT_ID).unwrap();
        assert_eq!(parsed.title.as_deref(), Some("Fresh update details"));
    }

    #[test]
    fn strips_markdown_link_and_bold_markers() {
        let raw = "**Issue:** see [our blog](https://example.com) for details.";
        assert_eq!(strip_markdown(raw), "Issue: see our blog for details.");
    }
}
