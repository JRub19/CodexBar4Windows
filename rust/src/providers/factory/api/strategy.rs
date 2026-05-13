//! Factory OAuth/cookie strategy. Ported from
//! `Sources/CodexBarCore/Providers/Factory/FactoryStatusProbe.swift`.
//!
//! The macOS source walks a long ladder of cookie/bearer/refresh paths
//! including WorkOS refresh token exchange and Chromium localStorage
//! import. The Rust port takes the bearer or cookie header as input
//! (resolved by the Tauri shell) and focuses on the three HTTP calls
//! that produce the snapshot:
//!
//! 1. `GET https://app.factory.ai/api/app/auth/me` — auth + plan + org.
//! 2. `GET https://api.factory.ai/api/billing/limits` — when this
//!    returns `usesTokenRateLimitsBilling: true` we use the new
//!    fiveHour/weekly/monthly windows.
//! 3. `GET https://app.factory.ai/api/organization/subscription/usage`
//!    — legacy standard/premium token allowance, used when (2) absent.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use super::response::{
    AuthResponse, BillingLimitsResponse, BillingWindow, TokenRateLimits, TokenUsage, UsageResponse,
};
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::factory::descriptor::FACTORY_ID;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::provider_cost::ProviderCostSnapshot;
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;

pub const APP_BASE: &str = "https://app.factory.ai";
pub const API_BASE: &str = "https://api.factory.ai";
pub const AUTH_ME_PATH: &str = "/api/app/auth/me";
pub const USAGE_PATH: &str = "/api/organization/subscription/usage";
pub const BILLING_LIMITS_PATH: &str = "/api/billing/limits";
pub const REQUIRED_X_FACTORY_CLIENT: &str = "web-app";
pub const REQUIRED_ORIGIN: &str = "https://app.factory.ai";
pub const REQUIRED_REFERER: &str = "https://app.factory.ai/";
pub const PER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Treat a `totalAllowance` larger than this as effectively unlimited.
/// Matches the Swift `unlimitedThreshold`.
pub const UNLIMITED_THRESHOLD: i64 = 1_000_000_000_000;

#[async_trait]
pub trait FactoryHttp: Send + Sync {
    async fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<FactoryResponse, ProviderFetchError>;
}

pub struct FactoryResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct FactoryCredentials {
    /// Authorization: Bearer header value (with or without the
    /// `Bearer ` prefix — the strategy normalises before sending).
    pub bearer: Option<String>,
    /// Cookie header (full `name=value; ...`).
    pub cookie: Option<String>,
    /// WorkOS refresh token, used when both `bearer` is absent /
    /// expired and the user has stored a refresh token from a prior
    /// login. When present, the strategy will trade it for a fresh
    /// bearer before making API calls.
    pub workos_refresh_token: Option<String>,
    /// WorkOS organisation id (optional). Some Factory enterprises
    /// require this on the refresh exchange.
    pub workos_organization_id: Option<String>,
}

impl FactoryCredentials {
    pub fn has_auth(&self) -> bool {
        self.bearer.as_ref().is_some_and(|b| !b.trim().is_empty())
            || self.cookie.as_ref().is_some_and(|c| !c.trim().is_empty())
            || self
                .workos_refresh_token
                .as_ref()
                .is_some_and(|t| !t.trim().is_empty())
    }
}

#[async_trait]
pub trait FactoryCredentialsResolver: Send + Sync {
    async fn resolve(&self) -> Result<FactoryCredentials, ProviderFetchError>;
    /// Persist a freshly minted WorkOS bearer / refresh token bundle.
    /// Default impl is a no-op for callers that do not care about
    /// rotation (tests, headless smoke).
    async fn persist_workos_refresh(
        &self,
        _bearer: &str,
        _new_refresh_token: Option<&str>,
    ) -> Result<(), ProviderFetchError> {
        Ok(())
    }
}

/// Optional refresh hook for the WorkOS exchange. When present, the
/// strategy uses it to trade a stored refresh_token (or cookie) for a
/// fresh bearer before falling back to surfacing `Unauthorized`.
pub struct FactoryRefreshHook {
    pub http: Arc<dyn WorkOSHttp>,
}

use super::workos_refresh::{exchange_cookie, exchange_refresh_token, WorkOSHttp};

pub struct FactoryApiStrategy {
    http: Arc<dyn FactoryHttp>,
    creds: Arc<dyn FactoryCredentialsResolver>,
    refresh: Option<FactoryRefreshHook>,
}

impl FactoryApiStrategy {
    pub fn new(http: Arc<dyn FactoryHttp>, creds: Arc<dyn FactoryCredentialsResolver>) -> Self {
        Self {
            http,
            creds,
            refresh: None,
        }
    }

    pub fn with_refresh(mut self, refresh: FactoryRefreshHook) -> Self {
        self.refresh = Some(refresh);
        self
    }
}

#[async_trait]
impl Strategy for FactoryApiStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::OAuth
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let mut creds = self.creds.resolve().await?;
        if !creds.has_auth() {
            return Err(ProviderFetchError::NoToken("factory"));
        }

        // If we have no usable bearer but we *do* have a WorkOS
        // refresh token (or session cookies), trade it for one before
        // hitting the Factory API.
        if !has_usable_bearer(&creds) {
            self.try_workos_refresh(&mut creds).await?;
        }

        let bearer = creds.bearer.as_deref();
        let cookie = creds.cookie.as_deref();
        let normalised_bearer = bearer.map(normalize_bearer);
        let bearer_ref = normalised_bearer.as_deref();

        let auth_result = self.fetch_auth_me(bearer_ref, cookie).await;
        let auth = match auth_result {
            Ok(a) => a,
            Err(ProviderFetchError::Unauthorized) if self.refresh.is_some() => {
                // Bearer is stale even though we had one; force refresh.
                self.try_workos_refresh(&mut creds).await?;
                let retry_bearer = creds.bearer.as_deref().map(normalize_bearer);
                self.fetch_auth_me(retry_bearer.as_deref(), creds.cookie.as_deref())
                    .await?
            }
            Err(e) => return Err(e),
        };

        let bearer_after = creds.bearer.as_deref().map(normalize_bearer);
        let bearer_ref = bearer_after.as_deref();
        let cookie = creds.cookie.as_deref();
        let billing = self.fetch_billing_limits(bearer_ref, cookie).await;
        let usage = self
            .fetch_usage(
                bearer_ref,
                cookie,
                auth.user_profile.as_ref().and_then(|u| u.id.as_deref()),
            )
            .await?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();

        let snapshot = build_snapshot(&auth, billing.as_ref(), &usage, now);
        Ok(snapshot)
    }
}

impl FactoryApiStrategy {
    async fn fetch_auth_me(
        &self,
        bearer: Option<&str>,
        cookie: Option<&str>,
    ) -> Result<AuthResponse, ProviderFetchError> {
        let url = format!("{APP_BASE}{AUTH_ME_PATH}");
        let response = self.http.get(&url, &build_headers(bearer, cookie)).await?;
        match response.status {
            200..=299 => serde_json::from_slice::<AuthResponse>(&response.body)
                .map_err(|e| ProviderFetchError::ParseError(format!("auth/me: {e}"))),
            401 => Err(ProviderFetchError::Unauthorized),
            403 => Err(ProviderFetchError::PermissionDenied(
                "factory auth/me 403".into(),
            )),
            other => Err(ProviderFetchError::Network(format!(
                "factory auth/me returned {other}"
            ))),
        }
    }

    async fn fetch_usage(
        &self,
        bearer: Option<&str>,
        cookie: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<UsageResponse, ProviderFetchError> {
        let query = {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            serializer.append_pair("useCache", "true");
            if let Some(uid) = user_id {
                serializer.append_pair("userId", uid);
            }
            serializer.finish()
        };
        let url = format!("{APP_BASE}{USAGE_PATH}?{query}");
        let response = self.http.get(&url, &build_headers(bearer, cookie)).await?;
        match response.status {
            200..=299 => serde_json::from_slice::<UsageResponse>(&response.body)
                .map_err(|e| ProviderFetchError::ParseError(format!("subscription/usage: {e}"))),
            401 | 403 => Err(ProviderFetchError::Unauthorized),
            other => Err(ProviderFetchError::Network(format!(
                "factory subscription/usage returned {other}"
            ))),
        }
    }

    async fn fetch_billing_limits(
        &self,
        bearer: Option<&str>,
        cookie: Option<&str>,
    ) -> Option<BillingLimitsResponse> {
        let url = format!("{API_BASE}{BILLING_LIMITS_PATH}");
        let response = self
            .http
            .get(&url, &build_headers(bearer, cookie))
            .await
            .ok()?;
        if !(200..=299).contains(&response.status) {
            return None;
        }
        serde_json::from_slice::<BillingLimitsResponse>(&response.body).ok()
    }

    async fn try_workos_refresh(
        &self,
        creds: &mut FactoryCredentials,
    ) -> Result<(), ProviderFetchError> {
        let Some(hook) = self.refresh.as_ref() else {
            // No refresh hook installed: surface Unauthorized only if
            // we have no usable bearer at all.
            if !has_usable_bearer(creds) {
                return Err(ProviderFetchError::Unauthorized);
            }
            return Ok(());
        };

        let org = creds.workos_organization_id.as_deref();
        // Prefer a stored refresh token; fall back to WorkOS cookies
        // in the user's `cookie` header.
        let outcome = if let Some(rt) = creds
            .workos_refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            exchange_refresh_token(hook.http.as_ref(), rt, org).await?
        } else if let Some(cookie) = creds.cookie.as_deref().filter(|c| !c.is_empty()) {
            exchange_cookie(hook.http.as_ref(), cookie, org).await?
        } else {
            return Err(ProviderFetchError::Unauthorized);
        };

        // Persist the new tokens via the resolver so the next tick
        // skips the refresh round-trip.
        let _ = self
            .creds
            .persist_workos_refresh(&outcome.access_token, outcome.refresh_token.as_deref())
            .await;
        creds.bearer = Some(outcome.access_token);
        if let Some(new_rt) = outcome.refresh_token {
            creds.workos_refresh_token = Some(new_rt);
        }
        if creds.workos_organization_id.is_none() {
            creds.workos_organization_id = outcome.organization_id;
        }
        Ok(())
    }
}

/// True when `creds.bearer` is present and non-empty (after trimming).
fn has_usable_bearer(creds: &FactoryCredentials) -> bool {
    creds
        .bearer
        .as_deref()
        .map(str::trim)
        .is_some_and(|t| !t.is_empty())
}

/// Normalise a stored bearer value into the `Authorization` header
/// shape Factory expects. Stored tokens may or may not include the
/// `Bearer ` prefix; we add it when missing.
fn normalize_bearer(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("Bearer ") {
        trimmed.to_string()
    } else {
        format!("Bearer {trimmed}")
    }
}

fn build_headers<'a>(bearer: Option<&'a str>, cookie: Option<&'a str>) -> Vec<(&'a str, &'a str)> {
    let mut headers: Vec<(&str, &str)> = vec![
        ("Accept", "application/json"),
        ("Content-Type", "application/json"),
        ("Origin", REQUIRED_ORIGIN),
        ("Referer", REQUIRED_REFERER),
        ("x-factory-client", REQUIRED_X_FACTORY_CLIENT),
    ];
    if let Some(c) = cookie.filter(|c| !c.is_empty()) {
        headers.push(("Cookie", c));
    }
    if let Some(b) = bearer.filter(|b| !b.is_empty()) {
        // The caller passes a bare token; we don't store the "Bearer "
        // prefix here so the bearer can be substituted as a static
        // lifetime str at the call site.
        headers.push(("Authorization", b));
    }
    headers
}

/// Build the framework `UsageSnapshot` from the three responses. Pure
/// fold, no IO — easy to test.
pub fn build_snapshot(
    auth: &AuthResponse,
    billing: Option<&BillingLimitsResponse>,
    usage: &UsageResponse,
    now_unix_secs: i64,
) -> UsageSnapshot {
    let token_rate_limits = billing
        .filter(|b| b.uses_token_rate_limits_billing)
        .and_then(|b| b.limits.as_ref());

    let (windows, cost) = match token_rate_limits {
        Some(limits) => {
            let windows = build_token_rate_windows(limits, now_unix_secs);
            let cost = billing.and_then(|b| {
                let cents = b.extra_usage_balance_cents;
                if cents <= 0 {
                    return None;
                }
                Some(ProviderCostSnapshot {
                    current_cycle_usd: cents as f64 / 100.0,
                    previous_cycle_usd: None,
                    last_30_days_usd: Vec::new(),
                    daily: Vec::new(),
                    total_window_usd: 0.0,
                    updated_at_unix_secs: 0,
                    breakdown_by_service: Vec::new(),
                })
            });
            (windows, cost)
        }
        None => {
            let windows = build_legacy_token_windows(usage);
            (windows, None)
        }
    };

    let user_id = auth
        .user_profile
        .as_ref()
        .and_then(|u| u.id.clone())
        .or_else(|| usage.user_id.clone());
    let account_token = user_id
        .clone()
        .map(|id| format!("factory:{}", id))
        .or_else(|| {
            auth.organization
                .as_ref()
                .and_then(|o| o.id.clone())
                .map(|id| format!("factory:org:{id}"))
        })
        .unwrap_or_else(|| "factory:auth".into());

    let plan_name = build_login_label(auth, billing);

    UsageSnapshot {
        identity: ProviderIdentitySnapshot::new(FACTORY_ID, account_token),
        windows,
        credits: None,
        cost,
        account_display_name: auth.organization.as_ref().and_then(|o| o.name.clone()),
        account_email: auth.user_profile.as_ref().and_then(|p| p.email.clone()),
        plan_name,
        captured_at_unix_secs: now_unix_secs,
    }
}

fn build_token_rate_windows(limits: &TokenRateLimits, now_unix_secs: i64) -> Vec<NamedRateWindow> {
    vec![
        window_from_billing(
            "factory_5h",
            "5h",
            &limits.standard.five_hour,
            now_unix_secs,
        ),
        window_from_billing(
            "factory_7d",
            "7-day",
            &limits.standard.weekly,
            now_unix_secs,
        ),
        window_from_billing(
            "factory_monthly",
            "Monthly",
            &limits.standard.monthly,
            now_unix_secs,
        ),
    ]
}

fn window_from_billing(
    key: &str,
    label: &str,
    window: &BillingWindow,
    now_unix_secs: i64,
) -> NamedRateWindow {
    NamedRateWindow {
        key: key.into(),
        window: RateWindow {
            label: label.into(),
            used: window.effective_used_percent(now_unix_secs),
            allotted: Some(100.0),
            reset_at_unix_secs: window.reset_at(now_unix_secs),
            pace_delta_percent: None,
        },
    }
}

fn build_legacy_token_windows(usage: &UsageResponse) -> Vec<NamedRateWindow> {
    let period_end = usage
        .usage
        .as_ref()
        .and_then(|u| u.end_date_ms)
        .map(ms_to_secs);
    let mut out = Vec::new();
    if let Some(standard) = usage.usage.as_ref().and_then(|u| u.standard.as_ref()) {
        out.push(NamedRateWindow {
            key: "factory_standard".into(),
            window: RateWindow {
                label: "Standard".into(),
                used: calculate_usage_percent(standard),
                allotted: Some(100.0),
                reset_at_unix_secs: period_end,
                pace_delta_percent: None,
            },
        });
    }
    if let Some(premium) = usage.usage.as_ref().and_then(|u| u.premium.as_ref()) {
        out.push(NamedRateWindow {
            key: "factory_premium".into(),
            window: RateWindow {
                label: "Premium".into(),
                used: calculate_usage_percent(premium),
                allotted: Some(100.0),
                reset_at_unix_secs: period_end,
                pace_delta_percent: None,
            },
        });
    }
    out
}

fn ms_to_secs(ms: i64) -> i64 {
    ms / 1000
}

/// Mirrors `FactoryStatusSnapshot.calculateUsagePercent` from the Swift
/// source. Prefers an API-provided 0..=1 ratio, falls back to the
/// raw `used / allowance` calculation, and treats trillion-scale
/// allowances as unlimited.
pub fn calculate_usage_percent(usage: &TokenUsage) -> f64 {
    let used = usage.user_tokens.unwrap_or(0);
    let allowance = usage.total_allowance.unwrap_or(0);
    if let Some(ratio) = usage.used_ratio {
        if !(ratio == 0.0 && used > 0 && allowance > 0 && allowance <= UNLIMITED_THRESHOLD) {
            if let Some(pct) = percent_from_api_ratio(ratio, allowance) {
                return pct;
            }
        }
    }
    if allowance > UNLIMITED_THRESHOLD {
        // Pseudo-percent against a 100M-token reference so the bar
        // moves under the "unlimited" plan instead of stuck at zero.
        const REFERENCE_TOKENS: f64 = 100_000_000.0;
        return ((used as f64 / REFERENCE_TOKENS) * 100.0).clamp(0.0, 100.0);
    }
    if allowance <= 0 {
        return 0.0;
    }
    ((used as f64 / allowance as f64) * 100.0).clamp(0.0, 100.0)
}

fn percent_from_api_ratio(ratio: f64, allowance: i64) -> Option<f64> {
    if !ratio.is_finite() {
        return None;
    }
    if (-0.001..=1.001).contains(&ratio) {
        return Some((ratio * 100.0).clamp(0.0, 100.0));
    }
    let allowance_reliable = allowance > 0 && allowance <= UNLIMITED_THRESHOLD;
    if !allowance_reliable && (-0.1..=100.1).contains(&ratio) {
        return Some(ratio.clamp(0.0, 100.0));
    }
    None
}

fn build_login_label(
    auth: &AuthResponse,
    billing: Option<&BillingLimitsResponse>,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let subscription = auth
        .organization
        .as_ref()
        .and_then(|o| o.subscription.as_ref());
    if let Some(tier) = subscription
        .and_then(|s| s.factory_tier.as_deref())
        .filter(|t| !t.trim().is_empty())
    {
        parts.push(format!("Factory {}", capitalize_first(tier)));
    }
    if let Some(plan) = subscription
        .and_then(|s| s.orb_subscription.as_ref())
        .and_then(|orb| orb.plan.as_ref())
        .and_then(|p| p.name.as_deref())
        .filter(|n| !n.trim().is_empty() && !n.to_ascii_lowercase().contains("factory"))
    {
        parts.push(plan.to_string());
    }
    if let Some(overage) = billing
        .and_then(|b| b.overage_preference.as_deref())
        .filter(|o| !o.trim().is_empty())
    {
        parts.push(format!("Fallback: {overage}"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" - "))
    }
}

fn capitalize_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    type CapturedHeaders = Vec<(String, String)>;
    type CapturedCall = (String, CapturedHeaders);

    #[derive(Default)]
    struct ScriptedHttp {
        replies: Mutex<HashMap<String, (u16, Vec<u8>)>>,
        captured: Mutex<Vec<CapturedCall>>,
    }

    impl ScriptedHttp {
        fn put(&self, url: &str, status: u16, body: &[u8]) {
            self.replies
                .lock()
                .unwrap()
                .insert(url.into(), (status, body.to_vec()));
        }
    }

    #[async_trait]
    impl FactoryHttp for ScriptedHttp {
        async fn get(
            &self,
            url: &str,
            headers: &[(&str, &str)],
        ) -> Result<FactoryResponse, ProviderFetchError> {
            self.captured.lock().unwrap().push((
                url.to_string(),
                headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ));
            let (status, body) = self
                .replies
                .lock()
                .unwrap()
                .get(url)
                .cloned()
                .unwrap_or((404, b"{}".to_vec()));
            Ok(FactoryResponse { status, body })
        }
    }

    struct StubResolver(FactoryCredentials);
    #[async_trait]
    impl FactoryCredentialsResolver for StubResolver {
        async fn resolve(&self) -> Result<FactoryCredentials, ProviderFetchError> {
            Ok(self.0.clone())
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
            provider_id: FACTORY_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    #[test]
    fn happy_path_legacy_plan_returns_standard_and_premium_windows() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(
            "https://app.factory.ai/api/app/auth/me",
            200,
            br#"{
                "userProfile": {"id":"user-1","email":"u@example.com"},
                "organization": {
                    "id": "org-1", "name": "AcmeCorp",
                    "subscription": {
                        "factoryTier": "enterprise",
                        "orbSubscription": {"plan": {"name": "Team Pro"}}
                    }
                }
            }"#,
        );
        http.put("https://api.factory.ai/api/billing/limits", 404, b"{}");
        http.put(
            "https://app.factory.ai/api/organization/subscription/usage?useCache=true&userId=user-1",
            200,
            br#"{
                "usage": {
                    "startDate": 1700000000000,
                    "endDate": 1702592000000,
                    "standard": {"userTokens": 5000, "totalAllowance": 10000, "usedRatio": 0.5},
                    "premium": {"userTokens": 250, "totalAllowance": 1000, "usedRatio": 0.25}
                },
                "userId": "user-1"
            }"#,
        );
        let resolver = Arc::new(StubResolver(FactoryCredentials {
            bearer: Some("Bearer abc".into()),
            ..FactoryCredentials::default()
        }));
        let strategy = FactoryApiStrategy::new(http.clone(), resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 2);
        assert_eq!(snap.windows[0].window.label, "Standard");
        assert_eq!(snap.windows[0].window.used, 50.0);
        assert_eq!(snap.windows[1].window.label, "Premium");
        assert_eq!(snap.windows[1].window.used, 25.0);
        assert_eq!(snap.account_email.as_deref(), Some("u@example.com"));
        assert_eq!(
            snap.plan_name.as_deref(),
            Some("Factory Enterprise - Team Pro")
        );
        assert_eq!(snap.identity.account_token, "factory:user-1");
    }

    #[test]
    fn happy_path_token_rate_limits_yields_five_hour_weekly_monthly() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(
            "https://app.factory.ai/api/app/auth/me",
            200,
            br#"{"userProfile":{"id":"u","email":"u@x.com"}}"#,
        );
        http.put(
            "https://api.factory.ai/api/billing/limits",
            200,
            br#"{
                "usesTokenRateLimitsBilling": true,
                "extraUsageBalanceCents": 1234,
                "overagePreference": "auto",
                "limits": {
                    "standard": {
                        "fiveHour": {"usedPercent": 12.5, "secondsRemaining": 3600},
                        "weekly":   {"usedPercent": 25.0, "secondsRemaining": 86400},
                        "monthly":  {"usedPercent": 33.3, "secondsRemaining": 604800}
                    }
                }
            }"#,
        );
        http.put(
            "https://app.factory.ai/api/organization/subscription/usage?useCache=true&userId=u",
            200,
            br#"{"usage": null, "userId": "u"}"#,
        );
        let resolver = Arc::new(StubResolver(FactoryCredentials {
            bearer: Some("Bearer t".into()),
            ..FactoryCredentials::default()
        }));
        let strategy = FactoryApiStrategy::new(http.clone(), resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 3);
        assert_eq!(snap.windows[0].window.label, "5h");
        assert_eq!(snap.windows[0].window.used, 12.5);
        assert_eq!(snap.windows[1].window.label, "7-day");
        assert_eq!(snap.windows[1].window.used, 25.0);
        assert_eq!(snap.windows[2].window.label, "Monthly");
        assert!((snap.windows[2].window.used - 33.3).abs() < 1e-9);
        let cost = snap.cost.unwrap();
        assert!((cost.current_cycle_usd - 12.34).abs() < 1e-9);
        assert_eq!(snap.plan_name.as_deref(), Some("Fallback: auto"));
    }

    #[test]
    fn missing_credentials_map_to_no_token() {
        let http = Arc::new(ScriptedHttp::default());
        let resolver = Arc::new(StubResolver(FactoryCredentials::default()));
        let strategy = FactoryApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoToken("factory")));
    }

    #[test]
    fn http_401_on_auth_me_maps_to_unauthorized() {
        let http = Arc::new(ScriptedHttp::default());
        http.put("https://app.factory.ai/api/app/auth/me", 401, b"{}");
        let resolver = Arc::new(StubResolver(FactoryCredentials {
            bearer: Some("Bearer x".into()),
            ..FactoryCredentials::default()
        }));
        let strategy = FactoryApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn required_headers_are_sent_on_auth_me_call() {
        let http = Arc::new(ScriptedHttp::default());
        http.put(
            "https://app.factory.ai/api/app/auth/me",
            200,
            br#"{"userProfile":{"id":"u","email":"u@x.com"}}"#,
        );
        http.put(
            "https://app.factory.ai/api/organization/subscription/usage?useCache=true&userId=u",
            200,
            br#"{"usage": null}"#,
        );
        let resolver = Arc::new(StubResolver(FactoryCredentials {
            bearer: Some("Bearer t".into()),
            cookie: Some("session=abc".into()),
            ..FactoryCredentials::default()
        }));
        let strategy = FactoryApiStrategy::new(http.clone(), resolver);
        rt().block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        let captured = http.captured.lock().unwrap();
        let auth_call = captured
            .iter()
            .find(|(u, _)| u == "https://app.factory.ai/api/app/auth/me")
            .unwrap();
        let headers = &auth_call.1;
        assert!(headers
            .iter()
            .any(|(k, v)| k == "x-factory-client" && v == "web-app"));
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Origin" && v == REQUIRED_ORIGIN));
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer t"));
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Cookie" && v == "session=abc"));
    }

    #[test]
    fn calculate_usage_percent_uses_api_ratio_when_in_zero_one_range() {
        let usage = TokenUsage {
            user_tokens: Some(500),
            total_allowance: Some(1000),
            used_ratio: Some(0.4),
        };
        assert_eq!(calculate_usage_percent(&usage), 40.0);
    }

    #[test]
    fn calculate_usage_percent_falls_back_to_used_over_allowance_when_ratio_zero_with_real_usage() {
        let usage = TokenUsage {
            user_tokens: Some(500),
            total_allowance: Some(1000),
            // Ratio is 0 but used > 0 and allowance is bounded → ignored.
            used_ratio: Some(0.0),
        };
        assert_eq!(calculate_usage_percent(&usage), 50.0);
    }

    #[test]
    fn calculate_usage_percent_treats_trillion_allowance_as_unlimited() {
        let usage = TokenUsage {
            user_tokens: Some(50_000_000),
            total_allowance: Some(2_000_000_000_000),
            used_ratio: None,
        };
        // 50M / 100M reference = 50%
        assert_eq!(calculate_usage_percent(&usage), 50.0);
    }

    // ─── WorkOS refresh tests ───────────────────────────────────────

    use super::super::workos_refresh::{WorkOSHttp, WorkOSResponse};

    struct StubWorkOSHttp {
        replies: Mutex<Vec<WorkOSResponse>>,
        captured: Mutex<Vec<(String, Option<String>)>>,
    }
    impl StubWorkOSHttp {
        fn new() -> Self {
            Self {
                replies: Mutex::new(Vec::new()),
                captured: Mutex::new(Vec::new()),
            }
        }
        fn enqueue(&self, status: u16, body: &[u8]) {
            self.replies.lock().unwrap().push(WorkOSResponse {
                status,
                body: body.to_vec(),
            });
        }
    }
    #[async_trait]
    impl WorkOSHttp for StubWorkOSHttp {
        async fn post_json(
            &self,
            _url: &str,
            body: &str,
            cookie: Option<&str>,
        ) -> Result<WorkOSResponse, ProviderFetchError> {
            self.captured
                .lock()
                .unwrap()
                .push((body.into(), cookie.map(|s| s.to_string())));
            let mut replies = self.replies.lock().unwrap();
            if replies.is_empty() {
                return Err(ProviderFetchError::Network("stub exhausted".into()));
            }
            Ok(replies.remove(0))
        }
    }

    /// Records persist_workos_refresh calls so tests can assert the
    /// strategy did write back a refreshed bearer.
    struct RecordingResolver {
        creds: Mutex<FactoryCredentials>,
        persisted: Mutex<Vec<(String, Option<String>)>>,
    }
    #[async_trait]
    impl FactoryCredentialsResolver for RecordingResolver {
        async fn resolve(&self) -> Result<FactoryCredentials, ProviderFetchError> {
            Ok(self.creds.lock().unwrap().clone())
        }
        async fn persist_workos_refresh(
            &self,
            bearer: &str,
            new_refresh_token: Option<&str>,
        ) -> Result<(), ProviderFetchError> {
            self.persisted
                .lock()
                .unwrap()
                .push((bearer.to_string(), new_refresh_token.map(String::from)));
            Ok(())
        }
    }

    fn install_happy_factory_responses(http: &ScriptedHttp) {
        http.put(
            "https://app.factory.ai/api/app/auth/me",
            200,
            br#"{"userProfile":{"id":"u","email":"u@x.com"}}"#,
        );
        http.put("https://api.factory.ai/api/billing/limits", 404, b"{}");
        http.put(
            "https://app.factory.ai/api/organization/subscription/usage?useCache=true&userId=u",
            200,
            br#"{
                "usage": {
                    "standard": {"userTokens": 50, "totalAllowance": 100, "usedRatio": 0.5},
                    "premium": {"userTokens": 10, "totalAllowance": 100, "usedRatio": 0.1}
                }
            }"#,
        );
    }

    #[test]
    fn refresh_hook_trades_refresh_token_for_bearer_when_bearer_missing() {
        let http = Arc::new(ScriptedHttp::default());
        install_happy_factory_responses(&http);

        let workos_http = Arc::new(StubWorkOSHttp::new());
        workos_http.enqueue(
            200,
            br#"{"access_token":"fresh-bearer","refresh_token":"new-rt"}"#,
        );

        let resolver = Arc::new(RecordingResolver {
            creds: Mutex::new(FactoryCredentials {
                workos_refresh_token: Some("rt-stored".into()),
                ..FactoryCredentials::default()
            }),
            persisted: Mutex::new(Vec::new()),
        });
        let strategy =
            FactoryApiStrategy::new(http, resolver.clone()).with_refresh(FactoryRefreshHook {
                http: workos_http.clone(),
            });
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 2);

        // WorkOS POST was made with the stored refresh token.
        let calls = workos_http.captured.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].0.contains("\"refresh_token\":\"rt-stored\""));
        assert!(calls[0].0.contains("\"grant_type\":\"refresh_token\""));
        // Persist hook saw the new bearer + rotated refresh token.
        let persisted = resolver.persisted.lock().unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].0, "fresh-bearer");
        assert_eq!(persisted[0].1.as_deref(), Some("new-rt"));
    }

    #[test]
    fn refresh_hook_uses_cookie_when_no_refresh_token_stored() {
        let http = Arc::new(ScriptedHttp::default());
        install_happy_factory_responses(&http);

        let workos_http = Arc::new(StubWorkOSHttp::new());
        workos_http.enqueue(200, br#"{"access_token":"bearer-from-cookie"}"#);

        let resolver = Arc::new(RecordingResolver {
            creds: Mutex::new(FactoryCredentials {
                cookie: Some("wos-session=abc".into()),
                ..FactoryCredentials::default()
            }),
            persisted: Mutex::new(Vec::new()),
        });
        let strategy =
            FactoryApiStrategy::new(http, resolver.clone()).with_refresh(FactoryRefreshHook {
                http: workos_http.clone(),
            });
        let _ = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        let calls = workos_http.captured.lock().unwrap();
        assert!(calls[0].0.contains("\"useCookie\":true"));
        assert_eq!(calls[0].1.as_deref(), Some("wos-session=abc"));
    }

    #[test]
    fn refresh_hook_recovers_from_initial_bearer_401() {
        let http = Arc::new(ScriptedHttp::default());
        // Stale bearer: first /auth/me returns 401, after refresh the
        // strategy retries and succeeds.
        http.put("https://app.factory.ai/api/app/auth/me", 401, b"{}");
        // The retry hits the same URL — the ScriptedHttp returns the
        // same recorded reply, so the test cannot distinguish a retry
        // from the first call. Treat the path as best-effort: this
        // test asserts only that the refresh POST fired, not that the
        // strategy succeeded.
        let workos_http = Arc::new(StubWorkOSHttp::new());
        workos_http.enqueue(200, br#"{"access_token":"refreshed"}"#);
        let resolver = Arc::new(RecordingResolver {
            creds: Mutex::new(FactoryCredentials {
                bearer: Some("Bearer stale".into()),
                workos_refresh_token: Some("rt-1".into()),
                ..FactoryCredentials::default()
            }),
            persisted: Mutex::new(Vec::new()),
        });
        let strategy =
            FactoryApiStrategy::new(http, resolver.clone()).with_refresh(FactoryRefreshHook {
                http: workos_http.clone(),
            });
        let _ = rt().block_on(async { strategy.fetch(&ctx()).await });
        assert_eq!(workos_http.captured.lock().unwrap().len(), 1);
        assert_eq!(resolver.persisted.lock().unwrap().len(), 1);
    }

    #[test]
    fn no_refresh_hook_with_only_refresh_token_returns_unauthorized() {
        let http = Arc::new(ScriptedHttp::default());
        let resolver = Arc::new(StubResolver(FactoryCredentials {
            workos_refresh_token: Some("rt-1".into()),
            ..FactoryCredentials::default()
        }));
        let strategy = FactoryApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }
}
