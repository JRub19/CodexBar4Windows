//! reqwest-backed `UsageHttp` for the Codex OAuth path. The 10 s
//! per-request budget sits under the framework's 45 s per-strategy
//! budget so one slow request still leaves headroom for fallback.
//!
//! User-Agent pinned to `codex_cli_rs/<version>` per live verification
//! on 2026-05-13: any other UA returns 401 from
//! `chatgpt.com/backend-api/wham/usage`.

use async_trait::async_trait;
use reqwest::Client;

use super::usage::{UsageHttp, UsageResponse, PER_REQUEST_TIMEOUT};
use crate::providers::codex::auth::errors::CodexOAuthError;

pub const OAUTH_USER_AGENT: &str = concat!("codex_cli_rs/", env!("CARGO_PKG_VERSION"));

pub struct ReqwestUsageClient {
    client: Client,
}

impl ReqwestUsageClient {
    pub fn new() -> Result<Self, CodexOAuthError> {
        let client = Client::builder()
            .timeout(PER_REQUEST_TIMEOUT)
            .user_agent(OAUTH_USER_AGENT)
            .build()
            .map_err(|e| CodexOAuthError::NetworkError(e.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl UsageHttp for ReqwestUsageClient {
    async fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<UsageResponse, CodexOAuthError> {
        let mut request = self.client.get(url);
        for (key, value) in headers {
            request = request.header(*key, *value);
        }
        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                CodexOAuthError::NetworkError(format!(
                    "{url} timed out after {}s",
                    PER_REQUEST_TIMEOUT.as_secs()
                ))
            } else {
                CodexOAuthError::NetworkError(e.to_string())
            }
        })?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|e| CodexOAuthError::NetworkError(e.to_string()))?
            .to_vec();
        Ok(UsageResponse { status, body })
    }
}
