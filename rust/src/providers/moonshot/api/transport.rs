//! Reqwest-backed `MoonshotHttp`.

use async_trait::async_trait;
use reqwest::Client;

use super::strategy::{MoonshotHttp, MoonshotResponse, PER_REQUEST_TIMEOUT};
use crate::providers::errors::ProviderFetchError;

pub struct ReqwestMoonshotClient {
    client: Client,
}

impl ReqwestMoonshotClient {
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
impl MoonshotHttp for ReqwestMoonshotClient {
    async fn get(&self, url: &str, bearer: &str) -> Result<MoonshotResponse, ProviderFetchError> {
        let response = self
            .client
            .get(url)
            .header("Authorization", bearer)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|e| ProviderFetchError::Network(e.to_string()))?;
        Ok(MoonshotResponse {
            status,
            body: body.to_vec(),
        })
    }
}
