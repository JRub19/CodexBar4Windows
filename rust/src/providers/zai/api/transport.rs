//! Reqwest-backed `ZaiHttp`.

use async_trait::async_trait;
use reqwest::Client;

use super::strategy::{PER_REQUEST_TIMEOUT, ZaiHttp, ZaiResponse};
use crate::providers::errors::ProviderFetchError;

pub struct ReqwestZaiClient {
    client: Client,
}

impl ReqwestZaiClient {
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
impl ZaiHttp for ReqwestZaiClient {
    async fn get(&self, url: &str, bearer: &str) -> Result<ZaiResponse, ProviderFetchError> {
        let response = self
            .client
            .get(url)
            .header("authorization", bearer)
            .header("accept", "application/json")
            .send()
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        Ok(ZaiResponse {
            status,
            body: body.to_vec(),
        })
    }
}
