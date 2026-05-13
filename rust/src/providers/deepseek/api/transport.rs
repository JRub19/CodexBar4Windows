//! Reqwest-backed `DeepSeekHttp`.

use async_trait::async_trait;
use reqwest::Client;

use super::strategy::{DeepSeekHttp, DeepSeekResponse, PER_REQUEST_TIMEOUT};
use crate::providers::errors::ProviderFetchError;

pub struct ReqwestDeepSeekClient {
    client: Client,
}

impl ReqwestDeepSeekClient {
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
impl DeepSeekHttp for ReqwestDeepSeekClient {
    async fn get(&self, url: &str, bearer: &str) -> Result<DeepSeekResponse, ProviderFetchError> {
        let response = self
            .client
            .get(url)
            .header("Authorization", bearer)
            .header("Accept", "application/json")
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
        Ok(DeepSeekResponse {
            status,
            body: body.to_vec(),
        })
    }
}
