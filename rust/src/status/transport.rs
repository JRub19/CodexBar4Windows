//! Reqwest-backed `StatusHttp`. Pure HTTPS GET with a 10-second
//! timeout, no auth headers — status feeds are public.

use async_trait::async_trait;
use reqwest::Client;

use super::feed::{StatusHttp, StatusResponse, FEED_TIMEOUT};

pub struct ReqwestStatusClient {
    client: Client,
}

impl ReqwestStatusClient {
    pub fn new() -> Result<Self, String> {
        let client = Client::builder()
            .timeout(FEED_TIMEOUT)
            .user_agent(concat!("codexbar4windows-status/", env!("CARGO_PKG_VERSION")))
            .gzip(true)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { client })
    }
}

#[async_trait]
impl StatusHttp for ReqwestStatusClient {
    async fn get(&self, url: &str) -> Result<StatusResponse, String> {
        let response = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let body = response.bytes().await.map_err(|e| e.to_string())?;
        Ok(StatusResponse {
            status,
            body: body.to_vec(),
        })
    }
}
