//! `reqwest::Client`-backed implementation of the `HttpClient` trait
//! used by `ClaudeOAuthStrategy`. The 30 s per-request budget sits
//! under the framework's 45 s per-strategy budget so a slow endpoint
//! still leaves headroom for the runtime to fall through to the next
//! strategy.

use async_trait::async_trait;
use reqwest::Client;

use super::strategy::{HttpClient, HttpResponse, ANTHROPIC_BETA_HEADER, PER_REQUEST_TIMEOUT};
use crate::providers::errors::ProviderFetchError;

pub struct ReqwestClient {
    client: Client,
}

impl ReqwestClient {
    pub fn new() -> Result<Self, ProviderFetchError> {
        let client = Client::builder()
            .timeout(PER_REQUEST_TIMEOUT)
            .user_agent(concat!("codexbar4windows/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl HttpClient for ReqwestClient {
    async fn get_json(&self, url: &str, bearer: &str) -> Result<HttpResponse, ProviderFetchError> {
        let response = self
            .client
            .get(url)
            .bearer_auth(bearer)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("anthropic-beta", ANTHROPIC_BETA_HEADER)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderFetchError::Timeout {
                        budget_ms: PER_REQUEST_TIMEOUT.as_millis() as u64,
                    }
                } else {
                    ProviderFetchError::Network(e.to_string())
                }
            })?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        Ok(HttpResponse {
            status,
            body: body.to_vec(),
        })
    }
}
