//! Reqwest-backed `GithubHttp` for the Copilot OAuth path.

use async_trait::async_trait;
use reqwest::Client;

use super::strategy::{GithubHttp, GithubResponse, PER_REQUEST_TIMEOUT};
use crate::providers::errors::ProviderFetchError;

pub struct ReqwestGithubClient {
    client: Client,
}

impl ReqwestGithubClient {
    pub fn new() -> Result<Self, ProviderFetchError> {
        let client = Client::builder()
            .timeout(PER_REQUEST_TIMEOUT)
            // User-Agent is applied per-request so the GitHub API's
            // editor-fingerprint check sees the value we set there.
            .gzip(true)
            .build()
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl GithubHttp for ReqwestGithubClient {
    async fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<GithubResponse, ProviderFetchError> {
        let mut req = self.client.get(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let response = req.send().await.map_err(|e| {
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
        Ok(GithubResponse {
            status,
            body: body.to_vec(),
        })
    }
}
