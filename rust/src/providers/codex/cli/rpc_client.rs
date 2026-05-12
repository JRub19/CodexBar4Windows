//! High-level JSON-RPC client. The client is generic over the transport
//! so production code can plug in a real ConPTY-backed stream while
//! tests inject an in-memory `mpsc` pipe.
//!
//! Spec 41 §4.2 defines the call sequence:
//! 1. `initialize` with our client info.
//! 2. `account/read` to discover the active Codex account.
//! 3. `rateLimits/read` for the live session and weekly windows.

use async_trait::async_trait;
use serde_json::Value;

use super::rpc_framer::{encode, Incoming, LineDecoder, Request, RpcError};

#[derive(Debug, thiserror::Error)]
pub enum RpcCallError {
    #[error("transport closed before response arrived")]
    Closed,
    #[error("transport error: {0}")]
    Transport(String),
    #[error("server returned error code {code}: {message}")]
    Server { code: i64, message: String },
    #[error("framer error: {0}")]
    Frame(String),
    #[error("result was missing required fields: {0}")]
    BadResult(String),
}

/// Pluggable transport. The production impl wraps a ConPTY stdin/stdout
/// pair; the test impl uses a `tokio::sync::mpsc` queue.
#[async_trait]
pub trait RpcTransport: Send + Sync {
    async fn send(&self, frame: Vec<u8>) -> Result<(), RpcCallError>;
    async fn recv(&self) -> Result<Vec<u8>, RpcCallError>;
}

pub struct RpcClient {
    transport: std::sync::Arc<dyn RpcTransport>,
    decoder: tokio::sync::Mutex<LineDecoder>,
    next_id: std::sync::atomic::AtomicU64,
}

impl RpcClient {
    pub fn new(transport: std::sync::Arc<dyn RpcTransport>) -> Self {
        Self {
            transport,
            decoder: tokio::sync::Mutex::new(LineDecoder::default()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    pub async fn call(&self, method: &str, params: Option<Value>) -> Result<Value, RpcCallError> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let request = Request::new(id, method, params);
        let frame = encode(&request);
        self.transport.send(frame).await?;
        loop {
            // Drain any complete responses already buffered before
            // reading more bytes from the transport.
            {
                let mut decoder = self.decoder.lock().await;
                while let Some(item) = decoder.poll() {
                    match item.map_err(|e| RpcCallError::Frame(e.to_string()))? {
                        Incoming::Response(resp) if resp.id == id => {
                            return resolve_response(resp);
                        }
                        // Notifications and out-of-order responses are
                        // intentionally discarded for now; spec 41 §4.2
                        // promises strict request/response pairing.
                        _ => continue,
                    }
                }
            }
            let chunk = self.transport.recv().await?;
            if chunk.is_empty() {
                return Err(RpcCallError::Closed);
            }
            self.decoder.lock().await.feed(&chunk);
        }
    }
}

fn resolve_response(response: super::rpc_framer::Response) -> Result<Value, RpcCallError> {
    if let Some(err) = response.error {
        return Err(map_rpc_error(err));
    }
    response
        .result
        .ok_or_else(|| RpcCallError::BadResult("missing result".into()))
}

fn map_rpc_error(err: RpcError) -> RpcCallError {
    RpcCallError::Server {
        code: err.code,
        message: err.message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    struct MpscTransport {
        inbox: tokio::sync::Mutex<mpsc::Receiver<Vec<u8>>>,
        outbox: mpsc::Sender<Vec<u8>>,
    }

    #[async_trait]
    impl RpcTransport for MpscTransport {
        async fn send(&self, frame: Vec<u8>) -> Result<(), RpcCallError> {
            self.outbox
                .send(frame)
                .await
                .map_err(|e| RpcCallError::Transport(e.to_string()))
        }
        async fn recv(&self) -> Result<Vec<u8>, RpcCallError> {
            self.inbox
                .lock()
                .await
                .recv()
                .await
                .ok_or(RpcCallError::Closed)
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn happy_path_returns_result_value() {
        let runtime = rt();
        let value = runtime
            .block_on(async {
                let (sender_to_server, mut receiver_from_client) = mpsc::channel::<Vec<u8>>(4);
                let (sender_to_client, receiver_from_server) = mpsc::channel::<Vec<u8>>(4);
                let transport = MpscTransport {
                    inbox: tokio::sync::Mutex::new(receiver_from_server),
                    outbox: sender_to_server,
                };
                let client = Arc::new(RpcClient::new(Arc::new(transport)));
                let server = tokio::spawn(async move {
                    let frame = receiver_from_client.recv().await.unwrap();
                    let request: serde_json::Value = serde_json::from_slice(
                        std::str::from_utf8(&frame).unwrap().trim_end().as_bytes(),
                    )
                    .unwrap();
                    let id = request["id"].as_u64().unwrap();
                    let response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {"hello": "world"},
                    });
                    let mut bytes = serde_json::to_vec(&response).unwrap();
                    bytes.push(b'\n');
                    sender_to_client.send(bytes).await.unwrap();
                });
                let result = client.call("test/echo", None).await;
                server.abort();
                result
            })
            .unwrap();
        assert_eq!(value, json!({"hello": "world"}));
    }

    #[test]
    fn server_error_maps_to_rpc_call_error_server() {
        let runtime = rt();
        let err = runtime
            .block_on(async {
                let (sender_to_server, mut receiver_from_client) = mpsc::channel::<Vec<u8>>(4);
                let (sender_to_client, receiver_from_server) = mpsc::channel::<Vec<u8>>(4);
                let transport = MpscTransport {
                    inbox: tokio::sync::Mutex::new(receiver_from_server),
                    outbox: sender_to_server,
                };
                let client = Arc::new(RpcClient::new(Arc::new(transport)));
                let server = tokio::spawn(async move {
                    let _ = receiver_from_client.recv().await.unwrap();
                    let response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "error": {"code": -32601, "message": "method not found"},
                    });
                    let mut bytes = serde_json::to_vec(&response).unwrap();
                    bytes.push(b'\n');
                    sender_to_client.send(bytes).await.unwrap();
                });
                let result = client.call("does/not_exist", None).await;
                server.abort();
                result
            })
            .unwrap_err();
        match err {
            RpcCallError::Server { code, message } => {
                assert_eq!(code, -32601);
                assert!(message.contains("method not found"));
            }
            other => panic!("expected Server error, got {other:?}"),
        }
    }
}
