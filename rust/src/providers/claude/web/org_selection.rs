//! Pick the right Claude org from the `/api/organizations` response.
//!
//! Spec 40 section 3.4 lists the priority rules: prefer an org with the
//! `chat` capability, then by `name` lexicographically. Orgs that are
//! API-only (no `chat` capability) are usable only via API keys, so we
//! skip them in the Web path.

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Organization {
    pub uuid: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl Organization {
    pub fn has_chat(&self) -> bool {
        self.capabilities.iter().any(|c| c == "chat")
    }
}

/// Returns the chosen org or `None` when no org has the `chat`
/// capability. Caller treats `None` as "user must visit claude.ai once
/// in a browser before the Web path can work."
pub fn pick(orgs: &[Organization]) -> Option<&Organization> {
    let mut chat_orgs: Vec<&Organization> = orgs.iter().filter(|o| o.has_chat()).collect();
    chat_orgs.sort_by(|a, b| {
        a.name
            .as_deref()
            .unwrap_or("")
            .cmp(b.name.as_deref().unwrap_or(""))
    });
    chat_orgs.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn org(uuid: &str, name: Option<&str>, caps: &[&str]) -> Organization {
        Organization {
            uuid: uuid.into(),
            name: name.map(|s| s.to_string()),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn picks_chat_org_over_api_only() {
        let orgs = vec![
            org("api-only", Some("Api Co"), &["api"]),
            org("chat", Some("Chat Co"), &["chat", "api"]),
        ];
        assert_eq!(pick(&orgs).unwrap().uuid, "chat");
    }

    #[test]
    fn breaks_tie_by_name() {
        let orgs = vec![
            org("u1", Some("Beta"), &["chat"]),
            org("u2", Some("Alpha"), &["chat"]),
        ];
        assert_eq!(pick(&orgs).unwrap().uuid, "u2");
    }

    #[test]
    fn returns_none_when_no_chat_capability() {
        let orgs = vec![org("u1", None, &["api"])];
        assert!(pick(&orgs).is_none());
    }
}
