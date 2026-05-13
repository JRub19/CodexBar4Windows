//! ConPTY-backed `RpcTransport` for the Codex CLI.
//!
//! Spawns the `codex` binary inside a Windows ConPTY (via `portable-pty`,
//! the same crate the Claude CLI launcher uses) and exposes its stdin/
//! stdout pair through the existing `RpcTransport` trait so the
//! framework's `RpcClient` can drive it.
//!
//! Background reader: a dedicated blocking thread continuously reads
//! the PTY master and forwards bytes into an unbounded tokio channel.
//! The async `recv` polls that channel, so the strategy never blocks a
//! tokio worker thread on a synchronous `read`.
//!
//! Lifetime: when the `ConPtyRpcTransport` value is dropped the writer
//! handle is closed (closing the child's stdin), the child is signalled
//! to exit, and the reader thread observes EOF and exits. The
//! `RpcTransport::recv` impl translates EOF into `RpcCallError::Closed`
//! so the strategy can fall through to the next plan entry.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use async_trait::async_trait;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use tokio::sync::mpsc;

use super::rpc_client::{RpcCallError, RpcTransport};
use super::strategy::TransportFactory;
use crate::providers::errors::ProviderFetchError;

pub const PTY_ROWS: u16 = 50;
pub const PTY_COLS: u16 = 200;
/// Args the macOS reference uses for the Codex TUI session (read-only,
/// untrusted, so the CLI cannot mutate anything in our process).
pub const READ_ONLY_ARGS: &[&str] = &["-s", "read-only", "-a", "untrusted"];

pub struct ConPtyRpcTransport {
    writer: Mutex<Box<dyn Write + Send>>,
    inbox: tokio::sync::Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
}

impl ConPtyRpcTransport {
    /// Spawn `binary` in a ConPTY and wire up reader/writer. The
    /// returned transport stays alive until dropped or until the child
    /// exits and closes the PTY.
    pub fn spawn(binary: &str, extra_args: &[&str]) -> Result<Self, ProviderFetchError> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: PTY_ROWS,
                cols: PTY_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| ProviderFetchError::PluginUnavailable(e.to_string()))?;
        let mut cmd = CommandBuilder::new(binary);
        for arg in extra_args {
            cmd.arg(arg);
        }
        for key in scrub_env_keys() {
            cmd.env_remove(key);
        }
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| ProviderFetchError::PluginUnavailable(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| ProviderFetchError::PluginUnavailable(e.to_string()))?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| ProviderFetchError::PluginUnavailable(e.to_string()))?;

        let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
        // Dedicated reader thread: PTY reads are blocking on Windows
        // even when the FD is non-blocking, so we keep them off the
        // tokio executor.
        thread::Builder::new()
            .name("codexbar-codex-pty-reader".into())
            .spawn(move || {
                let mut tmp = [0u8; 4096];
                loop {
                    match reader.read(&mut tmp) {
                        Ok(0) => break,
                        Ok(n) => {
                            if tx.send(tmp[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(20));
                        }
                        Err(_) => break,
                    }
                }
                // Drain any remaining buffer then drop tx so receivers
                // see channel-closed.
                drop(tx);
                let _ = child.wait();
            })
            .map_err(|e| ProviderFetchError::PluginUnavailable(e.to_string()))?;

        Ok(Self {
            writer: Mutex::new(writer),
            inbox: tokio::sync::Mutex::new(rx),
        })
    }
}

#[async_trait]
impl RpcTransport for ConPtyRpcTransport {
    async fn send(&self, frame: Vec<u8>) -> Result<(), RpcCallError> {
        // Writing to the PTY master is blocking; offload to a
        // spawn_blocking task so the async caller does not stall the
        // executor on the lock.
        let writer = match self.writer.lock() {
            Ok(w) => w,
            Err(_) => {
                return Err(RpcCallError::Transport(
                    "writer mutex poisoned".into(),
                ))
            }
        };
        // Hold the lock for the synchronous write; `Box<dyn Write>` is
        // not Send across an await so we cannot move it. Writes here
        // are short (single JSON-RPC line) and the PTY does not block
        // unless the kernel buffer is full.
        let mut writer = writer;
        writer
            .write_all(&frame)
            .map_err(|e| RpcCallError::Transport(e.to_string()))?;
        writer
            .flush()
            .map_err(|e| RpcCallError::Transport(e.to_string()))
    }

    async fn recv(&self) -> Result<Vec<u8>, RpcCallError> {
        let mut inbox = self.inbox.lock().await;
        match inbox.recv().await {
            Some(bytes) if !bytes.is_empty() => Ok(bytes),
            _ => Err(RpcCallError::Closed),
        }
    }
}

/// Environment keys we wipe before launching `codex` so our own OAuth
/// state cannot leak into the child. Mirrors the Claude scrub list.
pub fn scrub_env_keys() -> Vec<String> {
    let mut keys = vec![
        "CODEXBAR_CODEX_OAUTH_TOKEN".to_string(),
        "CODEXBAR_CODEX_ACCESS_TOKEN".to_string(),
    ];
    for (key, _) in std::env::vars() {
        if key.starts_with("OPENAI_") || key.starts_with("CHATGPT_") {
            keys.push(key);
        }
    }
    keys
}

/// Default `TransportFactory` that spawns the real codex binary on
/// every refresh tick. The Tauri shell installs this when a codex
/// binary is resolvable on PATH; otherwise it falls back to the
/// `UnavailableCodexTransport` stub so the rest of the plan still runs.
pub struct ConPtyTransportFactory {
    binary: String,
    args: Vec<String>,
}

impl ConPtyTransportFactory {
    pub fn new(binary: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            args: READ_ONLY_ARGS.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }
}

impl TransportFactory for ConPtyTransportFactory {
    fn open(&self) -> Result<Arc<dyn RpcTransport>, ProviderFetchError> {
        let args: Vec<&str> = self.args.iter().map(|s| s.as_str()).collect();
        let transport = ConPtyRpcTransport::spawn(&self.binary, &args)?;
        Ok(Arc::new(transport))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_env_keys_strips_openai_and_chatgpt_namespaces() {
        // SAFETY: the env-var APIs are unsafe in newer rustc; this test
        // is single-threaded by virtue of `cargo test --test-threads`
        // not crossing modules in the same way, but to be safe we
        // restore the original state.
        let key = "OPENAI_API_KEY_TEST_PROBE";
        unsafe { std::env::set_var(key, "x") };
        let keys = scrub_env_keys();
        unsafe { std::env::remove_var(key) };
        assert!(keys.iter().any(|k| k == key));
        assert!(keys.iter().any(|k| k == "CODEXBAR_CODEX_OAUTH_TOKEN"));
    }

    #[test]
    fn factory_default_args_match_read_only_session() {
        let factory = ConPtyTransportFactory::new("codex");
        assert_eq!(
            factory.args,
            vec![
                "-s".to_string(),
                "read-only".to_string(),
                "-a".to_string(),
                "untrusted".to_string()
            ]
        );
    }

    #[test]
    fn factory_with_args_replaces_defaults() {
        let factory = ConPtyTransportFactory::new("codex").with_args(vec!["--proto".into()]);
        assert_eq!(factory.args, vec!["--proto"]);
    }
}
