//! `loadCodeAssist` POST + plan-label mapping for Gemini. Ported from
//! `GeminiStatusProbe.swift`. The endpoint returns the user's tier
//! (`free-tier` / `standard-tier` / `legacy-tier`) and a
//! `cloudaicompanionProject` ID we use to scope the quota lookup.

use serde::Deserialize;

pub const LOAD_CODE_ASSIST_URL: &str = "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist";
pub const LOAD_CODE_ASSIST_BODY: &[u8] =
    br#"{"metadata":{"ideType":"GEMINI_CLI","pluginType":"GEMINI"}}"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeminiTier {
    Free,
    Legacy,
    Standard,
}

impl GeminiTier {
    pub fn from_raw(value: &str) -> Option<Self> {
        match value {
            "free-tier" => Some(GeminiTier::Free),
            "legacy-tier" => Some(GeminiTier::Legacy),
            "standard-tier" => Some(GeminiTier::Standard),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CodeAssistStatus {
    pub tier: Option<GeminiTier>,
    pub project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Wire {
    #[serde(default, rename = "cloudaicompanionProject")]
    cloudaicompanion_project: Option<serde_json::Value>,
    #[serde(default, rename = "currentTier")]
    current_tier: Option<TierWire>,
}

#[derive(Debug, Deserialize)]
struct TierWire {
    #[serde(default)]
    id: Option<String>,
}

pub fn parse_status(bytes: &[u8]) -> CodeAssistStatus {
    let Ok(wire) = serde_json::from_slice::<Wire>(bytes) else {
        return CodeAssistStatus::default();
    };
    let project_id = wire
        .cloudaicompanion_project
        .as_ref()
        .and_then(extract_project_id);
    let tier = wire
        .current_tier
        .as_ref()
        .and_then(|t| t.id.as_deref())
        .and_then(GeminiTier::from_raw);
    CodeAssistStatus { tier, project_id }
}

fn extract_project_id(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return non_empty_trim(s);
    }
    if let Some(obj) = value.as_object() {
        for key in &["id", "projectId"] {
            if let Some(id) = obj.get(*key).and_then(|v| v.as_str()).and_then(non_empty_trim) {
                return Some(id);
            }
        }
    }
    None
}

fn non_empty_trim(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Plan-label decision matrix per spec / Swift source:
/// - standard tier → "Paid"
/// - free tier + hosted domain → "Workspace"
/// - free tier without domain → "Free"
/// - legacy → "Legacy"
/// - unknown / loadCodeAssist failed → None
pub fn plan_label(tier: Option<&GeminiTier>, hosted_domain: Option<&str>) -> Option<String> {
    match (tier, hosted_domain) {
        (Some(GeminiTier::Standard), _) => Some("Paid".into()),
        (Some(GeminiTier::Free), Some(_)) => Some("Workspace".into()),
        (Some(GeminiTier::Free), None) => Some("Free".into()),
        (Some(GeminiTier::Legacy), _) => Some("Legacy".into()),
        (None, _) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_with_string_project_and_tier() {
        let body = br#"{
            "cloudaicompanionProject": "gen-lang-client-1234",
            "currentTier": {"id": "standard-tier"}
        }"#;
        let status = parse_status(body);
        assert_eq!(status.project_id.as_deref(), Some("gen-lang-client-1234"));
        assert_eq!(status.tier, Some(GeminiTier::Standard));
    }

    #[test]
    fn parses_status_with_object_project_using_id_field() {
        let body = br#"{
            "cloudaicompanionProject": {"id": "my-project"},
            "currentTier": {"id": "free-tier"}
        }"#;
        let status = parse_status(body);
        assert_eq!(status.project_id.as_deref(), Some("my-project"));
        assert_eq!(status.tier, Some(GeminiTier::Free));
    }

    #[test]
    fn parses_status_with_object_project_using_project_id_field() {
        let body = br#"{
            "cloudaicompanionProject": {"projectId": "other-project"},
            "currentTier": {"id": "legacy-tier"}
        }"#;
        let status = parse_status(body);
        assert_eq!(status.project_id.as_deref(), Some("other-project"));
        assert_eq!(status.tier, Some(GeminiTier::Legacy));
    }

    #[test]
    fn empty_project_string_treated_as_none() {
        let body = br#"{
            "cloudaicompanionProject": "   ",
            "currentTier": {"id": "free-tier"}
        }"#;
        let status = parse_status(body);
        assert!(status.project_id.is_none());
        assert_eq!(status.tier, Some(GeminiTier::Free));
    }

    #[test]
    fn invalid_json_is_empty_status() {
        let status = parse_status(b"not json");
        assert!(status.project_id.is_none());
        assert!(status.tier.is_none());
    }

    #[test]
    fn unknown_tier_id_drops_to_none() {
        let body = br#"{"currentTier": {"id": "future-tier"}}"#;
        let status = parse_status(body);
        assert!(status.tier.is_none());
    }

    #[test]
    fn plan_label_matrix_matches_macos_source() {
        assert_eq!(plan_label(Some(&GeminiTier::Standard), None), Some("Paid".into()));
        assert_eq!(
            plan_label(Some(&GeminiTier::Free), Some("example.com")),
            Some("Workspace".into())
        );
        assert_eq!(plan_label(Some(&GeminiTier::Free), None), Some("Free".into()));
        assert_eq!(plan_label(Some(&GeminiTier::Legacy), None), Some("Legacy".into()));
        assert_eq!(plan_label(None, None), None);
    }
}
