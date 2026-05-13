//! Reqwest-backed `GoogleHttp` for the Gemini Cloud Code endpoints.

use async_trait::async_trait;
use reqwest::{Client, Method};

use super::strategy::{GoogleHttp, GoogleResponse, HttpMethod, PER_REQUEST_TIMEOUT};
use crate::providers::errors::ProviderFetchError;

pub struct ReqwestGoogleClient {
    client: Client,
}

impl ReqwestGoogleClient {
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
impl GoogleHttp for ReqwestGoogleClient {
    async fn request(
        &self,
        method: HttpMethod,
        url: &str,
        bearer: &str,
        body: Option<&[u8]>,
    ) -> Result<GoogleResponse, ProviderFetchError> {
        let mut req = self
            .client
            .request(
                match method {
                    HttpMethod::Get => Method::GET,
                    HttpMethod::Post => Method::POST,
                },
                url,
            )
            .header("Authorization", bearer)
            .header("Accept", "application/json");
        if let Some(b) = body {
            req = req
                .header("Content-Type", "application/json")
                .body(b.to_vec());
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
        Ok(GoogleResponse {
            status,
            body: body.to_vec(),
        })
    }
}
