//! Z.ai API-key strategy.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use super::response::{fold, LimitEntry, LimitType, QuotaLimitResponse};
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;
use crate::providers::zai::descriptor::{ZaiRegion, ZAI_ID};

pub const PER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[async_trait]
pub trait ZaiHttp: Send + Sync {
    async fn get(&self, url: &str, bearer: &str) -> Result<ZaiResponse, ProviderFetchError>;
}

pub struct ZaiResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ZaiCredentials {
    pub api_key: String,
    pub region: ZaiRegion,
    pub host_override: Option<String>,
    pub quota_url_override: Option<String>,
}

#[async_trait]
pub trait ZaiCredentialsResolver: Send + Sync {
    async fn resolve(&self) -> Result<Option<ZaiCredentials>, ProviderFetchError>;
}

pub struct ZaiApiStrategy {
    http: Arc<dyn ZaiHttp>,
    creds: Arc<dyn ZaiCredentialsResolver>,
}

impl ZaiApiStrategy {
    pub fn new(http: Arc<dyn ZaiHttp>, creds: Arc<dyn ZaiCredentialsResolver>) -> Self {
        Self { http, creds }
    }
}

#[async_trait]
impl Strategy for ZaiApiStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::ApiKey
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let creds = self
            .creds
            .resolve()
            .await?
            .filter(|c| !c.api_key.trim().is_empty())
            .ok_or(ProviderFetchError::NoToken("zai"))?;
        let url = resolve_quota_url(&creds);
        let bearer = format!("Bearer {}", creds.api_key);

        let response = self.http.get(&url, &bearer).await?;
        let parsed: QuotaLimitResponse = match response.status {
            200..=299 => {
                if response.body.is_empty() {
                    return Err(ProviderFetchError::ParseError(
                        "z.ai returned an empty 200 body — check region and API key".into(),
                    ));
                }
                serde_json::from_slice(&response.body)
                    .map_err(|e| ProviderFetchError::ParseError(format!("quota/limit: {e}")))?
            }
            401 | 403 => return Err(ProviderFetchError::Unauthorized),
            other => {
                return Err(ProviderFetchError::Network(format!(
                    "z.ai quota/limit returned {other}"
                )))
            }
        };

        if !parsed.is_success() {
            return Err(ProviderFetchError::Network(format!(
                "z.ai API error code {} ({})",
                parsed.code,
                parsed.msg.as_deref().unwrap_or("")
            )));
        }

        let folded = fold(&parsed);
        let mut windows = Vec::new();
        if let Some(entry) = folded.primary.as_ref() {
            windows.push(entry_to_window("primary", "Tokens", entry));
        }
        if let Some(entry) = folded.secondary.as_ref() {
            windows.push(entry_to_window("secondary", "Window", entry));
        }
        if let Some(entry) = folded.tertiary.as_ref() {
            windows.push(entry_to_window("session", "Session", entry));
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();

        Ok(UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(
                ZAI_ID,
                format!("zai:key:{}", short_key_token(&creds.api_key)),
            ),
            windows,
            credits: None,
            cost: None,
            account_display_name: None,
            account_email: None,
            plan_name: folded.plan_name,
            captured_at_unix_secs: now,
        })
    }
}

fn entry_to_window(key: &str, label: &str, entry: &LimitEntry) -> NamedRateWindow {
    let label = match (&entry.kind, label) {
        (LimitType::Time, _) => "Cycle",
        (LimitType::Tokens, other) => other,
    };
    NamedRateWindow {
        key: key.into(),
        window: RateWindow {
            label: label.into(),
            used: entry.used_percent,
            allotted: Some(100.0),
            reset_at_unix_secs: entry.reset_at_unix_secs,
            pace_delta_percent: None,
        },
    }
}

/// Resolution order: explicit URL override > host override + standard
/// path > region default. Mirrors `ZaiUsageFetcher.resolveQuotaURL`.
pub fn resolve_quota_url(creds: &ZaiCredentials) -> String {
    if let Some(url) = creds
        .quota_url_override
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
    {
        return url.to_string();
    }
    if let Some(host) = creds
        .host_override
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
    {
        let with_scheme = if host.contains("://") {
            host.to_string()
        } else {
            format!("https://{host}")
        };
        return format!(
            "{}/api/monitor/usage/quota/limit",
            with_scheme.trim_end_matches('/')
        );
    }
    creds.region.quota_url()
}

fn short_key_token(api_key: &str) -> String {
    let trimmed = api_key.trim();
    trimmed
        .strip_prefix("sk-")
        .unwrap_or(trimmed)
        .chars()
        .take(4)
        .collect::<String>()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct ScriptedHttp {
        reply: Mutex<Option<(u16, Vec<u8>)>>,
        captured_url: Mutex<Option<String>>,
    }

    #[async_trait]
    impl ZaiHttp for ScriptedHttp {
        async fn get(&self, url: &str, _: &str) -> Result<ZaiResponse, ProviderFetchError> {
            *self.captured_url.lock().unwrap() = Some(url.into());
            let (status, body) = self
                .reply
                .lock()
                .unwrap()
                .clone()
                .unwrap_or((404, b"{}".to_vec()));
            Ok(ZaiResponse { status, body })
        }
    }

    struct StubResolver(Option<ZaiCredentials>);
    #[async_trait]
    impl ZaiCredentialsResolver for StubResolver {
        async fn resolve(&self) -> Result<Option<ZaiCredentials>, ProviderFetchError> {
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
            provider_id: ZAI_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    #[test]
    fn happy_path_returns_primary_secondary_windows_with_plan_name() {
        let http = Arc::new(ScriptedHttp::default());
        *http.reply.lock().unwrap() = Some((
            200,
            br#"{
                "code": 200, "msg": "ok", "success": true,
                "data": {
                    "planName": "Coding Plan Pro",
                    "limits": [
                        {"type":"TOKENS_LIMIT","unit":1,"number":1,"usage":1000,"remaining":600,"percentage":40},
                        {"type":"TIME_LIMIT","unit":1,"number":30,"usage":30,"currentValue":12,"percentage":40}
                    ]
                }
            }"#
            .to_vec(),
        ));
        let resolver = Arc::new(StubResolver(Some(ZaiCredentials {
            api_key: "sk-abcd".into(),
            region: ZaiRegion::Global,
            host_override: None,
            quota_url_override: None,
        })));
        let strategy = ZaiApiStrategy::new(http.clone(), resolver);
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 2);
        assert_eq!(snap.windows[0].window.label, "Tokens");
        assert_eq!(snap.windows[0].window.used, 40.0);
        assert_eq!(snap.windows[1].window.label, "Cycle");
        assert_eq!(snap.plan_name.as_deref(), Some("Coding Plan Pro"));
        assert_eq!(
            http.captured_url.lock().unwrap().as_deref(),
            Some("https://api.z.ai/api/monitor/usage/quota/limit")
        );
    }

    #[test]
    fn bigmodel_cn_region_hits_open_bigmodel_cn() {
        let http = Arc::new(ScriptedHttp::default());
        *http.reply.lock().unwrap() = Some((
            200,
            br#"{"code":200,"msg":"ok","success":true,"data":{"limits":[]}}"#.to_vec(),
        ));
        let resolver = Arc::new(StubResolver(Some(ZaiCredentials {
            api_key: "sk-x".into(),
            region: ZaiRegion::BigmodelCN,
            host_override: None,
            quota_url_override: None,
        })));
        let strategy = ZaiApiStrategy::new(http.clone(), resolver);
        rt().block_on(async { strategy.fetch(&ctx()).await }).unwrap();
        assert_eq!(
            http.captured_url.lock().unwrap().as_deref(),
            Some("https://open.bigmodel.cn/api/monitor/usage/quota/limit")
        );
    }

    #[test]
    fn host_override_replaces_region_default() {
        let creds = ZaiCredentials {
            api_key: "sk-x".into(),
            region: ZaiRegion::Global,
            host_override: Some("zai.example.com".into()),
            quota_url_override: None,
        };
        assert_eq!(
            resolve_quota_url(&creds),
            "https://zai.example.com/api/monitor/usage/quota/limit"
        );
    }

    #[test]
    fn quota_url_override_wins_over_host_and_region() {
        let creds = ZaiCredentials {
            api_key: "sk-x".into(),
            region: ZaiRegion::Global,
            host_override: Some("zai.example.com".into()),
            quota_url_override: Some("https://internal.example.com/custom/path".into()),
        };
        assert_eq!(
            resolve_quota_url(&creds),
            "https://internal.example.com/custom/path"
        );
    }

    #[test]
    fn empty_200_body_maps_to_parse_error_with_region_hint() {
        let http = Arc::new(ScriptedHttp::default());
        *http.reply.lock().unwrap() = Some((200, Vec::new()));
        let resolver = Arc::new(StubResolver(Some(ZaiCredentials {
            api_key: "sk-x".into(),
            region: ZaiRegion::Global,
            host_override: None,
            quota_url_override: None,
        })));
        let strategy = ZaiApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::ParseError(_)));
    }

    #[test]
    fn http_401_maps_to_unauthorized() {
        let http = Arc::new(ScriptedHttp::default());
        *http.reply.lock().unwrap() = Some((401, b"{}".to_vec()));
        let resolver = Arc::new(StubResolver(Some(ZaiCredentials {
            api_key: "sk-x".into(),
            region: ZaiRegion::Global,
            host_override: None,
            quota_url_override: None,
        })));
        let strategy = ZaiApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Unauthorized));
    }

    #[test]
    fn missing_credentials_map_to_no_token() {
        let http = Arc::new(ScriptedHttp::default());
        let resolver = Arc::new(StubResolver(None));
        let strategy = ZaiApiStrategy::new(http, resolver);
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::NoToken("zai")));
    }
}
