//! reqwest-backed `WebClient` for the Claude web path. The 15 s budget
//! aligns with spec 40 section 3.2; the runtime's 45 s per-strategy
//! budget still wraps three of these requests with margin.

use async_trait::async_trait;
use reqwest::Client;

use super::strategy::{WebClient, WebResponse, PER_REQUEST_TIMEOUT};
use crate::providers::errors::ProviderFetchError;

pub struct ReqwestWebClient {
    client: Client,
}

impl ReqwestWebClient {
    pub fn new() -> Result<Self, ProviderFetchError> {
        let client = Client::builder()
            .timeout(PER_REQUEST_TIMEOUT)
            .user_agent(concat!("codexbar4windows/", env!("CARGO_PKG_VERSION")))
            .gzip(true)
            .build()
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl WebClient for ReqwestWebClient {
    async fn get_json(&self, url: &str, cookie: &str) -> Result<WebResponse, ProviderFetchError> {
        let response = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .header("Cookie", cookie)
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
        Ok(WebResponse {
            status,
            body: body.to_vec(),
        })
    }
}
