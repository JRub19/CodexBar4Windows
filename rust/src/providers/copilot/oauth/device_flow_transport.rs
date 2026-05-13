//! Reqwest-backed `DeviceFlowHttp` for the Copilot login flow.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;

use super::device_flow::{DeviceFlowError, DeviceFlowHttp, DeviceFlowResponse};

pub const PER_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub struct ReqwestDeviceFlowClient {
    client: Client,
}

impl ReqwestDeviceFlowClient {
    pub fn new() -> Result<Self, DeviceFlowError> {
        let client = Client::builder()
            .timeout(PER_REQUEST_TIMEOUT)
            .user_agent(concat!("codexbar4windows/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| DeviceFlowError::Transport(e.to_string()))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl DeviceFlowHttp for ReqwestDeviceFlowClient {
    async fn post_form(
        &self,
        url: &str,
        body: &str,
        headers: &[(&str, &str)],
    ) -> Result<DeviceFlowResponse, DeviceFlowError> {
        let mut req = self.client.post(url).body(body.to_string());
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let response = req
            .send()
            .await
            .map_err(|e| DeviceFlowError::Transport(e.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|e| DeviceFlowError::Transport(e.to_string()))?;
        Ok(DeviceFlowResponse {
            status,
            body: body.to_vec(),
        })
    }
}
