//! Status poller. Walks every registered polled provider, fires the
//! relevant HTTPS GET, parses, and applies the result (success or
//! sticky-failure) to the `StatusStore`.
//!
//! The poller is decoupled from the refresh loop's `JoinSet` so it can
//! be exercised in tests without touching that machinery. The Tauri
//! shell schedules it alongside the usage refresh tick.

use std::sync::Arc;

use super::feed::{StatusFeed, StatusFeedKind, StatusHttp, StatusSnapshot};
use super::gws;
use super::registry::all_status_capable_provider_ids;
use super::store::StatusStore;
use super::{feed_for_provider, statuspage};

pub struct StatusPoller {
    http: Arc<dyn StatusHttp>,
    store: StatusStore,
}

impl StatusPoller {
    pub fn new(http: Arc<dyn StatusHttp>, store: StatusStore) -> Self {
        Self { http, store }
    }

    /// Run one polling cycle. Fetches every provider with a registered
    /// feed in parallel — but bounded sequentially here to keep the
    /// test stub simple. Production may run them concurrently inside
    /// the Tauri shell's tokio runtime.
    pub async fn run_once(&self) {
        for provider_id in all_status_capable_provider_ids() {
            let Some(feed) = feed_for_provider(provider_id) else {
                continue;
            };
            self.poll_one(feed).await;
        }
    }

    pub async fn poll_one(&self, feed: StatusFeed) {
        let url = feed.polled_url();
        let provider_id = feed.provider_id.to_string();
        let public_url = feed.public_url.to_string();
        match self.http.get(&url).await {
            Ok(response) if (200..=299).contains(&response.status) => {
                match parse(&feed, &response.body) {
                    Ok(parsed) => {
                        let snap = StatusSnapshot::now(
                            provider_id,
                            parsed.severity,
                            parsed.title,
                            parsed.updated_at_unix_secs,
                            Some(public_url),
                        );
                        self.store.apply_success(snap);
                    }
                    Err(_) => self.store.apply_failure(
                        &provider_id,
                        Some("status feed unparseable".into()),
                        Some(public_url),
                    ),
                }
            }
            Ok(response) => self.store.apply_failure(
                &provider_id,
                Some(format!("status feed returned HTTP {}", response.status)),
                Some(public_url),
            ),
            Err(_) => self.store.apply_failure(
                &provider_id,
                Some("status feed network error".into()),
                Some(public_url),
            ),
        }
    }
}

fn parse(feed: &StatusFeed, body: &[u8]) -> Result<statuspage::ParsedStatus, String> {
    match feed.kind {
        StatusFeedKind::Statuspage { .. } => statuspage::parse(body),
        StatusFeedKind::GoogleWorkspace { product_id } => gws::parse(body, product_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::severity::StatusSeverity;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct ScriptedStatusHttp {
        replies: Mutex<HashMap<String, (u16, Vec<u8>)>>,
    }
    impl ScriptedStatusHttp {
        fn new() -> Self {
            Self {
                replies: Mutex::new(HashMap::new()),
            }
        }
        fn put(&self, url: &str, status: u16, body: &[u8]) {
            self.replies
                .lock()
                .unwrap()
                .insert(url.into(), (status, body.to_vec()));
        }
    }
    #[async_trait]
    impl StatusHttp for ScriptedStatusHttp {
        async fn get(&self, url: &str) -> Result<crate::status::feed::StatusResponse, String> {
            let (status, body) = self
                .replies
                .lock()
                .unwrap()
                .get(url)
                .cloned()
                .unwrap_or((599, b"no fixture".to_vec()));
            Ok(crate::status::feed::StatusResponse { status, body })
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn run_once_records_operational_on_each_polled_feed() {
        let http = Arc::new(ScriptedStatusHttp::new());
        let ok = br#"{"status": {"indicator": "none", "description": "ok"}}"#;
        http.put("https://status.openai.com/api/v2/status.json", 200, ok);
        http.put("https://status.claude.com/api/v2/status.json", 200, ok);
        http.put("https://status.cursor.com/api/v2/status.json", 200, ok);
        http.put("https://status.factory.ai/api/v2/status.json", 200, ok);
        http.put("https://www.githubstatus.com/api/v2/status.json", 200, ok);
        http.put(
            "https://www.google.com/appsstatus/dashboard/incidents.json",
            200,
            b"[]",
        );
        let store = StatusStore::new();
        let poller = StatusPoller::new(http, store.clone());
        rt().block_on(async { poller.run_once().await });
        assert_eq!(store.get("claude").unwrap().severity, StatusSeverity::None);
        assert_eq!(store.get("gemini").unwrap().severity, StatusSeverity::None);
    }

    #[test]
    fn http_500_with_no_prior_snapshot_installs_unknown() {
        let http = Arc::new(ScriptedStatusHttp::new());
        http.put(
            "https://status.claude.com/api/v2/status.json",
            500,
            b"internal",
        );
        let store = StatusStore::new();
        let poller = StatusPoller::new(http, store.clone());
        let feed = feed_for_provider("claude").unwrap();
        rt().block_on(async { poller.poll_one(feed).await });
        let snap = store.get("claude").unwrap();
        assert_eq!(snap.severity, StatusSeverity::Unknown);
        assert!(snap.title.as_deref().unwrap().contains("HTTP 500"));
    }

    #[test]
    fn http_failure_after_prior_success_keeps_prior_snapshot() {
        let http = Arc::new(ScriptedStatusHttp::new());
        http.put(
            "https://status.claude.com/api/v2/status.json",
            200,
            br#"{"status": {"indicator": "none", "description": "ok"}}"#,
        );
        let store = StatusStore::new();
        let poller = StatusPoller::new(http.clone(), store.clone());
        let feed = feed_for_provider("claude").unwrap();
        rt().block_on(async { poller.poll_one(feed.clone()).await });
        // Now poison the response.
        http.put("https://status.claude.com/api/v2/status.json", 500, b"err");
        rt().block_on(async { poller.poll_one(feed).await });
        assert_eq!(
            store.get("claude").unwrap().severity,
            StatusSeverity::None,
            "sticky on failure"
        );
    }

    #[test]
    fn malformed_payload_records_unparseable_unknown() {
        let http = Arc::new(ScriptedStatusHttp::new());
        http.put(
            "https://status.claude.com/api/v2/status.json",
            200,
            b"not json",
        );
        let store = StatusStore::new();
        let poller = StatusPoller::new(http, store.clone());
        let feed = feed_for_provider("claude").unwrap();
        rt().block_on(async { poller.poll_one(feed).await });
        let snap = store.get("claude").unwrap();
        assert_eq!(snap.severity, StatusSeverity::Unknown);
        assert!(snap.title.as_deref().unwrap().contains("unparseable"));
    }
}
