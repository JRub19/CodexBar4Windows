//! Cursor web strategy. Reuses the shared `WebClient` + `CookieResolver`
//! traits introduced for Claude so the reqwest transport and cookie
//! cache code are shared. Calls `/api/usage-summary` and `/api/auth/me`
//! in parallel, then `/api/usage?user=<sub>` if the account has a sub
//! (legacy request-based plans). `/auth/me` and the legacy probe are
//! best-effort: if either fails the snapshot still produces a primary
//! window so the popup shows something useful.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use super::endpoints::{auth_me_url, legacy_usage_url, usage_summary_url};
use super::fold::{fold_summary, pretty_membership};
use super::response::{AuthMe, LegacyUsage, UsageSummary};
use crate::providers::claude::web::strategy::{CookieResolver, WebClient};
use crate::providers::cursor::descriptor::CURSOR_ID;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::provider_cost::ProviderCostSnapshot;
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;

pub struct CursorWebStrategy {
    client: Arc<dyn WebClient>,
    cookies: Arc<dyn CookieResolver>,
}

impl CursorWebStrategy {
    pub fn new(client: Arc<dyn WebClient>, cookies: Arc<dyn CookieResolver>) -> Self {
        Self { client, cookies }
    }

    async fn fetch_summary(&self, cookie: &str) -> Result<UsageSummary, ProviderFetchError> {
        let url = usage_summary_url();
        let response = self.client.get_json(&url, cookie).await?;
        match response.status {
            200..=299 => serde_json::from_slice::<UsageSummary>(&response.body)
                .map_err(|e| ProviderFetchError::ParseError(format!("usage-summary: {e}"))),
            401 | 403 => {
                let _ = self.cookies.invalidate().await;
                Err(ProviderFetchError::Unauthorized)
            }
            other => Err(ProviderFetchError::Network(format!(
                "cursor /api/usage-summary returned {other}"
            ))),
        }
    }

    async fn fetch_auth_me(&self, cookie: &str) -> Option<AuthMe> {
        let url = auth_me_url();
        let response = self.client.get_json(&url, cookie).await.ok()?;
        if !(200..=299).contains(&response.status) {
            return None;
        }
        serde_json::from_slice::<AuthMe>(&response.body).ok()
    }

    async fn fetch_legacy_usage(&self, cookie: &str, sub: &str) -> Option<LegacyUsage> {
        let url = legacy_usage_url(sub);
        let response = self.client.get_json(&url, cookie).await.ok()?;
        if !(200..=299).contains(&response.status) {
            return None;
        }
        serde_json::from_slice::<LegacyUsage>(&response.body).ok()
    }
}

#[async_trait]
impl Strategy for CursorWebStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::Web
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let Some(cookie) = self.cookies.cookie().await? else {
            return Err(ProviderFetchError::NoCookies("cursor"));
        };

        // 1. Usage summary first — required. /auth/me is best-effort.
        //    Sequential rather than parallel: tokio's `join!` needs the
        //    "macros" feature which we don't pull in, and the savings
        //    here are bounded by the 15 s per-request budget anyway.
        let summary = self.fetch_summary(&cookie).await?;
        let me: Option<AuthMe> = self.fetch_auth_me(&cookie).await;

        // 2. Legacy request usage — only if a sub claim is available.
        let legacy = match me.as_ref().and_then(|m| m.sub.as_deref()) {
            Some(sub) => self.fetch_legacy_usage(&cookie, sub).await,
            None => None,
        };

        let headline = fold_summary(&summary, legacy.as_ref());

        let mut windows = Vec::new();
        windows.push(NamedRateWindow {
            key: "total".into(),
            window: RateWindow {
                label: "Total".into(),
                used: headline.primary_used_percent(),
                allotted: Some(100.0),
                reset_at_unix_secs: headline.billing_cycle_end_unix_secs,
                pace_delta_percent: None,
            },
        });
        if let Some(auto) = headline.auto_percent {
            windows.push(NamedRateWindow {
                key: "auto".into(),
                window: RateWindow {
                    label: "Auto".into(),
                    used: auto,
                    allotted: Some(100.0),
                    reset_at_unix_secs: headline.billing_cycle_end_unix_secs,
                    pace_delta_percent: None,
                },
            });
        }
        if let Some(api) = headline.api_percent {
            windows.push(NamedRateWindow {
                key: "api".into(),
                window: RateWindow {
                    label: "API".into(),
                    used: api,
                    allotted: Some(100.0),
                    reset_at_unix_secs: headline.billing_cycle_end_unix_secs,
                    pace_delta_percent: None,
                },
            });
        }

        // On-demand spend surfaces in the cost row. The framework's
        // `ProviderCostSnapshot` only carries `current_cycle_usd`; the
        // limit and team pool stay in the popup-side breakdown.
        let total_on_demand =
            headline.on_demand_used_usd + headline.team_on_demand_used_usd.unwrap_or(0.0);
        let cost = if total_on_demand > 0.0 || headline.on_demand_limit_usd.unwrap_or(0.0) > 0.0 {
            Some(ProviderCostSnapshot {
                current_cycle_usd: total_on_demand,
                previous_cycle_usd: None,
                last_30_days_usd: Vec::new(),
                daily: Vec::new(),
                total_window_usd: 0.0,
                updated_at_unix_secs: 0,
                breakdown_by_service: Vec::new(),
            })
        } else {
            None
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();

        let account_token = me
            .as_ref()
            .and_then(|m| m.email.clone())
            .map(|e: String| format!("cursor:{}", e.to_ascii_lowercase()))
            .or_else(|| {
                me.as_ref()
                    .and_then(|m| m.sub.clone())
                    .map(|s| format!("cursor:sub:{s}"))
            })
            .unwrap_or_else(|| "cursor:cookie".into());

        let plan_label = headline.membership_type.as_deref().map(pretty_membership);

        Ok(UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(CURSOR_ID, account_token),
            windows,
            credits: None,
            cost,
            account_display_name: me.as_ref().and_then(|m| m.name.clone()),
            account_email: me.as_ref().and_then(|m| m.email.clone()),
            plan_name: plan_label,
            captured_at_unix_secs: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::claude::web::strategy::WebResponse;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct ScriptedClient {
        replies: Mutex<HashMap<String, (u16, Vec<u8>)>>,
    }

    impl ScriptedClient {
        fn put(&self, url: &str, status: u16, body: &[u8]) {
            self.replies
                .lock()
                .unwrap()
                .insert(url.to_string(), (status, body.to_vec()));
        }
    }

    #[async_trait]
    impl WebClient for ScriptedClient {
        async fn get_json(&self, url: &str, _: &str) -> Result<WebResponse, ProviderFetchError> {
            let (status, body) = self
                .replies
                .lock()
                .unwrap()
                .get(url)
                .cloned()
                .unwrap_or((404, b"{}".to_vec()));
            Ok(WebResponse { status, body })
        }
    }

    struct StubCookies(Mutex<Option<String>>);

    #[async_trait]
    impl CookieResolver for StubCookies {
        async fn cookie(&self) -> Result<Option<String>, ProviderFetchError> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn invalidate(&self) -> Result<(), ProviderFetchError> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn ctx() -> ProviderFetchContext {
        use crate::providers::fetch_context::{Runtime, SourceMode};
        use crate::secrets::token_account::TokenAccountStore;
        let tokens = Arc::new(TokenAccountStore::new(std::env::temp_dir()));
        ProviderFetchContext {
            provider_id: CURSOR_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    #[test]
    fn happy_path_returns_total_auto_api_windows() {
        let client = Arc::new(ScriptedClient::default());
        client.put(
            "https://cursor.com/api/usage-summary",
            200,
            br#"{
                "billingCycleEnd": "2026-06-01T00:00:00Z",
                "membershipType": "pro",
                "individualUsage": {
                    "plan": {
                        "used": 800, "limit": 2000,
                        "autoPercentUsed": 40.0, "apiPercentUsed": 60.0,
                        "totalPercentUsed": 50.0
                    },
                    "onDemand": {"used": 1234, "limit": 5000}
                }
            }"#,
        );
        client.put(
            "https://cursor.com/api/auth/me",
            200,
            br#"{"email":"u@example.com","name":"User","sub":"user-1"}"#,
        );
        // Sub is present → legacy usage URL is hit; return 404 so the
        // fold falls back to the percent ladder (which is what should
        // happen on token-based plans).
        client.put("https://cursor.com/api/usage?user=user-1", 404, b"{}");
        let cookies = Arc::new(StubCookies(Mutex::new(Some("cookie=value".into()))));
        let strategy = CursorWebStrategy::new(client, cookies);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();

        assert_eq!(snap.windows.len(), 3);
        assert_eq!(snap.windows[0].key, "total");
        assert_eq!(snap.windows[0].window.used, 50.0);
        assert_eq!(snap.windows[1].key, "auto");
        assert_eq!(snap.windows[1].window.used, 40.0);
        assert_eq!(snap.windows[2].key, "api");
        assert_eq!(snap.windows[2].window.used, 60.0);
        assert_eq!(snap.plan_name.as_deref(), Some("Cursor Pro"));
        assert_eq!(snap.account_email.as_deref(), Some("u@example.com"));
        assert_eq!(snap.identity.account_token, "cursor:u@example.com");
        let cost = snap.cost.unwrap();
        assert!((cost.current_cycle_usd - 12.34).abs() < 1e-9);
    }

    #[test]
    fn legacy_request_plan_overrides_primary_with_request_ratio() {
        let client = Arc::new(ScriptedClient::default());
        client.put(
            "https://cursor.com/api/usage-summary",
            200,
            br#"{
                "membershipType": "hobby",
                "individualUsage": {
                    "plan": {"used": 0, "limit": 0}
                }
            }"#,
        );
        client.put(
            "https://cursor.com/api/auth/me",
            200,
            br#"{"email":"u@example.com","sub":"42"}"#,
        );
        client.put(
            "https://cursor.com/api/usage?user=42",
            200,
            br#"{"gpt-4": {"numRequestsTotal": 100, "maxRequestUsage": 500}}"#,
        );
        let cookies = Arc::new(StubCookies(Mutex::new(Some("cookie=value".into()))));
        let strategy = CursorWebStrategy::new(client, cookies);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows[0].window.used, 20.0);
    }

    #[test]
    fn http_401_clears_cookie_cache() {
        let client = Arc::new(ScriptedClient::default());
        client.put("https://cursor.com/api/usage-summary", 401, b"{}");
        let cookies = Arc::new(StubCookies(Mutex::new(Some("c=v".into()))));
        let cookies_clone = cookies.clone();
        let strategy = CursorWebStrategy::new(client, cookies);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
        assert!(cookies_clone.0.lock().unwrap().is_none());
    }

    #[test]
    fn missing_cookie_maps_to_no_cookies() {
        let client = Arc::new(ScriptedClient::default());
        let cookies = Arc::new(StubCookies(Mutex::new(None)));
        let strategy = CursorWebStrategy::new(client, cookies);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoCookies("cursor")));
    }

    #[test]
    fn auth_me_failure_is_tolerated_but_drops_legacy_probe() {
        let client = Arc::new(ScriptedClient::default());
        client.put(
            "https://cursor.com/api/usage-summary",
            200,
            br#"{
                "membershipType": "pro",
                "individualUsage": {
                    "plan": {"totalPercentUsed": 12.5}
                }
            }"#,
        );
        // /me intentionally absent → legacy probe never fires (no sub).
        let cookies = Arc::new(StubCookies(Mutex::new(Some("cookie=value".into()))));
        let strategy = CursorWebStrategy::new(client, cookies);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows[0].window.used, 12.5);
        assert!(snap.account_email.is_none());
        assert_eq!(snap.identity.account_token, "cursor:cookie");
    }
}
