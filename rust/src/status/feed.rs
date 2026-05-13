//! Status feed shape + per-snapshot transport. Provider-agnostic;
//! `StatusFeedKind` knows how to construct the per-provider URL +
//! parser pick.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::severity::StatusSeverity;

pub const FEED_TIMEOUT: Duration = Duration::from_secs(10);

/// One successful status pull. The store keeps the last one per
/// provider and is sticky on transient failures.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StatusSnapshot {
    pub provider_id: String,
    pub severity: StatusSeverity,
    pub title: Option<String>,
    /// Unix epoch seconds.
    pub updated_at_unix_secs: Option<i64>,
    /// Optional public URL the menu's "Status Page" action opens. This
    /// stays alongside the snapshot so the popup can render the link
    /// even when the feed has no current incident.
    pub status_page_url: Option<String>,
    /// Unix epoch seconds when this snapshot was minted client-side.
    /// Helpful for the "Last checked" caption in the popup.
    pub captured_at_unix_secs: i64,
}

impl StatusSnapshot {
    pub fn now(
        provider_id: impl Into<String>,
        severity: StatusSeverity,
        title: Option<String>,
        updated_at_unix_secs: Option<i64>,
        status_page_url: Option<String>,
    ) -> Self {
        let captured_at_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        Self {
            provider_id: provider_id.into(),
            severity,
            title,
            updated_at_unix_secs,
            status_page_url,
            captured_at_unix_secs,
        }
    }
}

/// Specification of a feed for one provider. Construction happens in
/// `registry.rs`; tests can build instances directly.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusFeed {
    pub provider_id: &'static str,
    pub kind: StatusFeedKind,
    /// Public URL the menu's "Status Page" action opens.
    pub public_url: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StatusFeedKind {
    /// Statuspage.io `${base}/api/v2/status.json`.
    Statuspage { base_url: &'static str },
    /// Google Workspace `https://www.google.com/appsstatus/dashboard/incidents.json`
    /// filtered by product id.
    GoogleWorkspace { product_id: &'static str },
}

impl StatusFeed {
    pub fn polled_url(&self) -> String {
        match self.kind {
            StatusFeedKind::Statuspage { base_url } => {
                let trimmed = base_url.trim_end_matches('/');
                format!("{trimmed}/api/v2/status.json")
            }
            StatusFeedKind::GoogleWorkspace { .. } => {
                "https://www.google.com/appsstatus/dashboard/incidents.json".to_string()
            }
        }
    }
}

#[derive(Debug)]
pub struct StatusResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// HTTP transport for status feeds. Decoupled from the per-provider
/// `WebClient` traits so the status subsystem can be tested without
/// dragging in provider crates.
#[async_trait]
pub trait StatusHttp: Send + Sync {
    async fn get(&self, url: &str) -> Result<StatusResponse, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuspage_url_handles_trailing_slash() {
        let feed = StatusFeed {
            provider_id: "claude",
            kind: StatusFeedKind::Statuspage {
                base_url: "https://status.claude.com/",
            },
            public_url: "https://status.claude.com/",
        };
        assert_eq!(
            feed.polled_url(),
            "https://status.claude.com/api/v2/status.json"
        );
    }

    #[test]
    fn statuspage_url_without_trailing_slash_works() {
        let feed = StatusFeed {
            provider_id: "claude",
            kind: StatusFeedKind::Statuspage {
                base_url: "https://status.claude.com",
            },
            public_url: "https://status.claude.com",
        };
        assert_eq!(
            feed.polled_url(),
            "https://status.claude.com/api/v2/status.json"
        );
    }

    #[test]
    fn gws_url_is_fixed_regardless_of_product_id() {
        let feed = StatusFeed {
            provider_id: "gemini",
            kind: StatusFeedKind::GoogleWorkspace {
                product_id: "npdyhgECDJ6tB66MxXyo",
            },
            public_url: "https://www.google.com/appsstatus/dashboard/products/x/history",
        };
        assert_eq!(
            feed.polled_url(),
            "https://www.google.com/appsstatus/dashboard/incidents.json"
        );
    }

    #[test]
    fn snapshot_now_stamps_captured_time() {
        let snap = StatusSnapshot::now(
            "claude",
            StatusSeverity::None,
            None,
            None,
            Some("https://status.claude.com/".into()),
        );
        assert!(snap.captured_at_unix_secs > 0);
    }
}
