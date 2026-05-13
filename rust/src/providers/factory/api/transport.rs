//! Reqwest-backed `FactoryHttp`.

use async_trait::async_trait;
use reqwest::Client;

use super::strategy::{FactoryHttp, FactoryResponse, PER_REQUEST_TIMEOUT};
use super::workos_refresh::{WorkOSHttp, WorkOSResponse};
use crate::providers::errors::ProviderFetchError;

pub struct ReqwestFactoryClient {
    client: Client,
}

impl ReqwestFactoryClient {
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
impl FactoryHttp for ReqwestFactoryClient {
    async fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<FactoryResponse, ProviderFetchError> {
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
        Ok(FactoryResponse {
            status,
            body: body.to_vec(),
        })
    }
}

#[async_trait]
impl WorkOSHttp for ReqwestFactoryClient {
    async fn post_json(
        &self,
        url: &str,
        body: &str,
        cookie_header: Option<&str>,
    ) -> Result<WorkOSResponse, ProviderFetchError> {
        let mut req = self
            .client
            .post(url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .body(body.to_string());
        if let Some(cookie) = cookie_header {
            req = req.header("Cookie", cookie);
        }
        let response = req
            .send()
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        Ok(WorkOSResponse {
            status,
            body: body.to_vec(),
        })
    }
}
