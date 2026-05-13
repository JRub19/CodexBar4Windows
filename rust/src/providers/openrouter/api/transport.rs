//! Reqwest-backed `OpenRouterHttp`.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;

use super::strategy::{OpenRouterHttp, OpenRouterResponse, CREDITS_TIMEOUT};
use crate::providers::errors::ProviderFetchError;

pub struct ReqwestOpenRouterClient {
    client: Client,
}

impl ReqwestOpenRouterClient {
    pub fn new() -> Result<Self, ProviderFetchError> {
        // Per-request timeouts are applied by the strategy via the
        // `timeout` arg, but reqwest also needs a connect-side cap.
        let client = Client::builder()
            .connect_timeout(CREDITS_TIMEOUT)
            .user_agent(concat!("codexbar4windows/", env!("CARGO_PKG_VERSION")))
            .gzip(true)
            .build()
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl OpenRouterHttp for ReqwestOpenRouterClient {
    async fn get(
        &self,
        url: &str,
        bearer: &str,
        headers: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<OpenRouterResponse, ProviderFetchError> {
        let mut req = self
            .client
            .get(url)
            .header("Authorization", bearer)
            .timeout(timeout);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let response = req.send().await.map_err(|e| {
            if e.is_timeout() {
                ProviderFetchError::Timeout {
                    budget_ms: timeout.as_millis() as u64,
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
        Ok(OpenRouterResponse {
            status,
            body: body.to_vec(),
        })
    }
}
