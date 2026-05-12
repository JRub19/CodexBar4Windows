//! Codex CLI strategy. The strategy speaks JSON-RPC to a long-lived
//! `codex` subprocess and pulls the live `rateLimits/read` snapshot.
//! When the user has multiple Codex accounts, the strategy looks up the
//! active one via `account/read` so the resulting snapshot lands in the
//! right `UsageStore` slot.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;

use super::rpc_client::{RpcCallError, RpcClient, RpcTransport};
use crate::core::ProviderId;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;

pub const CODEX_ID: ProviderId = ProviderId("codex");

/// Builds a fresh `RpcTransport` per refresh tick. Tests inject a stub
/// that talks to an in-memory channel; production code launches the
/// codex binary via ConPTY.
pub trait TransportFactory: Send + Sync {
    fn open(&self) -> Result<Arc<dyn RpcTransport>, ProviderFetchError>;
}

/// Decoded `account/read` result. The Codex CLI returns more fields
/// than we use; we ignore them to stay tolerant of schema additions.
#[derive(Debug, Deserialize)]
struct AccountInfo {
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    plan_type: Option<String>,
}

/// Decoded `rateLimits/read` result.
#[derive(Debug, Deserialize)]
struct RateLimitsResult {
    #[serde(default)]
    primary: Option<RateBucket>,
    #[serde(default)]
    secondary: Option<RateBucket>,
}

#[derive(Debug, Deserialize)]
struct RateBucket {
    #[serde(default)]
    used: f64,
    #[serde(default)]
    allotted: Option<f64>,
    #[serde(default)]
    resets_at_epoch: Option<i64>,
}

pub struct CodexCliStrategy {
    transport_factory: Arc<dyn TransportFactory>,
}

impl CodexCliStrategy {
    pub fn new(transport_factory: Arc<dyn TransportFactory>) -> Self {
        Self { transport_factory }
    }

    async fn collect_snapshot(&self) -> Result<UsageSnapshot, ProviderFetchError> {
        let transport = self.transport_factory.open()?;
        let client = RpcClient::new(transport);
        // 1. initialize. The CLI requires this before any other method.
        let _ = client
            .call(
                "initialize",
                Some(serde_json::json!({
                    "client": "codexbar4windows",
                    "version": env!("CARGO_PKG_VERSION"),
                })),
            )
            .await
            .map_err(rpc_to_fetch)?;
        // 2. account/read.
        let account_value = client
            .call("account/read", None)
            .await
            .map_err(rpc_to_fetch)?;
        let account: AccountInfo = serde_json::from_value(account_value)
            .map_err(|e| ProviderFetchError::ParseError(e.to_string()))?;
        // 3. rateLimits/read.
        let limits_value = client
            .call("rateLimits/read", None)
            .await
            .map_err(rpc_to_fetch)?;
        let limits: RateLimitsResult = serde_json::from_value(limits_value)
            .map_err(|e| ProviderFetchError::ParseError(e.to_string()))?;

        let mut windows = Vec::new();
        if let Some(b) = limits.primary {
            windows.push(NamedRateWindow {
                key: "session".into(),
                window: bucket_to_window("Session", &b),
            });
        }
        if let Some(b) = limits.secondary {
            windows.push(NamedRateWindow {
                key: "weekly".into(),
                window: bucket_to_window("Week", &b),
            });
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let account_token = account
            .account_id
            .as_deref()
            .map(|id| format!("codex:{}", id.to_ascii_lowercase()))
            .or_else(|| {
                account
                    .email
                    .as_deref()
                    .map(|e| format!("codex:{}", e.to_ascii_lowercase()))
            })
            .unwrap_or_else(|| "codex:anonymous".into());

        Ok(UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(CODEX_ID, account_token),
            windows,
            credits: None,
            cost: None,
            account_display_name: None,
            account_email: account.email,
            plan_name: account.plan_type,
            captured_at_unix_secs: now,
        })
    }
}

#[async_trait]
impl Strategy for CodexCliStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::CLI
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        self.collect_snapshot().await
    }
}

fn bucket_to_window(label: &str, bucket: &RateBucket) -> RateWindow {
    RateWindow {
        label: label.into(),
        used: bucket.used,
        allotted: bucket.allotted,
        reset_at_unix_secs: bucket.resets_at_epoch,
        pace_delta_percent: None,
    }
}

fn rpc_to_fetch(err: RpcCallError) -> ProviderFetchError {
    match err {
        RpcCallError::Closed => {
            ProviderFetchError::PluginUnavailable("rpc transport closed".into())
        }
        RpcCallError::Transport(msg) => ProviderFetchError::Network(msg),
        RpcCallError::Server { code, message } => {
            ProviderFetchError::ParseError(format!("codex rpc error {code}: {message}"))
        }
        RpcCallError::Frame(msg) => ProviderFetchError::ParseError(msg),
        RpcCallError::BadResult(msg) => ProviderFetchError::ParseError(msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::sync::mpsc;

    struct ScriptedTransport {
        replies: Mutex<Vec<serde_json::Value>>,
        sent: Mutex<Vec<serde_json::Value>>,
    }

    #[async_trait]
    impl RpcTransport for ScriptedTransport {
        async fn send(&self, frame: Vec<u8>) -> Result<(), RpcCallError> {
            let text = std::str::from_utf8(&frame).unwrap().trim_end_matches('\n');
            let value: serde_json::Value = serde_json::from_str(text).unwrap();
            self.sent.lock().unwrap().push(value);
            Ok(())
        }
        async fn recv(&self) -> Result<Vec<u8>, RpcCallError> {
            let next = self
                .replies
                .lock()
                .unwrap()
                .pop()
                .ok_or(RpcCallError::Closed)?;
            // Match the response id to the last-sent request id so the
            // client's loop accepts it.
            let sent = self.sent.lock().unwrap();
            let id = sent.last().and_then(|v| v["id"].as_u64()).unwrap_or(0);
            drop(sent);
            let mut reply = next;
            reply["id"] = serde_json::Value::from(id);
            reply["jsonrpc"] = serde_json::Value::from("2.0");
            let mut bytes = serde_json::to_vec(&reply).unwrap();
            bytes.push(b'\n');
            Ok(bytes)
        }
    }

    struct ScriptedFactory(Arc<ScriptedTransport>);

    impl TransportFactory for ScriptedFactory {
        fn open(&self) -> Result<Arc<dyn RpcTransport>, ProviderFetchError> {
            Ok(self.0.clone() as Arc<dyn RpcTransport>)
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn ctx() -> ProviderFetchContext {
        use crate::providers::fetch_context::{Runtime, SourceMode};
        use crate::secrets::token_account::TokenAccountStore;
        let tokens = Arc::new(TokenAccountStore::new(std::env::temp_dir()));
        ProviderFetchContext {
            provider_id: CODEX_ID,
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    #[test]
    fn happy_path_folds_three_calls_into_a_snapshot() {
        // Replies are popped in reverse order, so push them in reverse.
        let transport = Arc::new(ScriptedTransport {
            replies: Mutex::new(vec![
                serde_json::json!({"result":{
                    "primary":{"used":12.0,"allotted":100.0,"resets_at_epoch":1},
                    "secondary":{"used":200.0,"allotted":1000.0}
                }}),
                serde_json::json!({"result":{
                    "email":"user@example.com",
                    "account_id":"acct-1",
                    "plan_type":"plus"
                }}),
                serde_json::json!({"result":{"ok":true}}),
            ]),
            sent: Mutex::new(Vec::new()),
        });
        let strategy = CodexCliStrategy::new(Arc::new(ScriptedFactory(transport.clone())));
        let snapshot = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.account_email.as_deref(), Some("user@example.com"));
        assert_eq!(snapshot.plan_name.as_deref(), Some("plus"));
        assert_eq!(snapshot.identity.account_token, "codex:acct-1");

        // Sanity-check the three RPC method names hit in order.
        let sent = transport.sent.lock().unwrap();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[0]["method"], "initialize");
        assert_eq!(sent[1]["method"], "account/read");
        assert_eq!(sent[2]["method"], "rateLimits/read");
    }

    #[test]
    fn closed_transport_maps_to_plugin_unavailable() {
        struct AlwaysClosed;

        #[async_trait]
        impl RpcTransport for AlwaysClosed {
            async fn send(&self, _: Vec<u8>) -> Result<(), RpcCallError> {
                Ok(())
            }
            async fn recv(&self) -> Result<Vec<u8>, RpcCallError> {
                Err(RpcCallError::Closed)
            }
        }
        struct Factory;
        impl TransportFactory for Factory {
            fn open(&self) -> Result<Arc<dyn RpcTransport>, ProviderFetchError> {
                Ok(Arc::new(AlwaysClosed) as Arc<dyn RpcTransport>)
            }
        }

        let strategy = CodexCliStrategy::new(Arc::new(Factory));
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::PluginUnavailable(_)));
    }

    // Unused channel imports help compile-time confirm we depend on the
    // same mpsc types the production transport will rely on.
    #[allow(dead_code)]
    fn unused() -> mpsc::Sender<Vec<u8>> {
        let (tx, _rx) = mpsc::channel(1);
        tx
    }
}
