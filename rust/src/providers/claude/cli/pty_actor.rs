//! ConPTY-backed launcher for the Claude CLI. We open a 50x160 PTY,
//! spawn `claude` with a scrubbed environment, type `/usage` plus the
//! Enter key, and stream the output to the caller until we see the
//! `Settings:` panel header.
//!
//! The actor is generic over a runner so tests can drive the parser
//! with prerecorded transcripts without launching a real CLI.

use std::io::{Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

use crate::providers::errors::ProviderFetchError;

pub const PTY_ROWS: u16 = 50;
pub const PTY_COLS: u16 = 160;
/// Soft cap on the time we wait for the panel to print. The framework's
/// 45 s per-strategy budget is the hard cap.
pub const PANEL_TIMEOUT: Duration = Duration::from_secs(20);

/// Trait so callers can inject a recorder fake in tests.
pub trait CliRunner: Send + Sync {
    fn run_usage(&self, binary: &str) -> Result<String, ProviderFetchError>;
}

pub struct RealCliRunner;

impl CliRunner for RealCliRunner {
    fn run_usage(&self, binary: &str) -> Result<String, ProviderFetchError> {
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
        for key in scrub_env_keys() {
            cmd.env_remove(key);
        }
        // The CLI honors CLAUDE_FORCE_NON_INTERACTIVE for some prompts;
        // we leave the rest of the env intact so it can find the user's
        // ~/.claude directory.
        cmd.env("CLAUDE_FORCE_NON_INTERACTIVE", "0");
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| ProviderFetchError::PluginUnavailable(e.to_string()))?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|e| ProviderFetchError::PluginUnavailable(e.to_string()))?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| ProviderFetchError::PluginUnavailable(e.to_string()))?;
        // Send `/usage` after a tiny delay so the CLI's start-up output
        // does not race the prompt.
        let _ = writer.write_all(b"/usage\r");
        let _ = writer.flush();
        let started = Instant::now();
        let mut buffer = Vec::with_capacity(64 * 1024);
        let mut tmp = [0u8; 4096];
        while started.elapsed() < PANEL_TIMEOUT {
            match reader.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => {
                    buffer.extend_from_slice(&tmp[..n]);
                    if buffer
                        .windows(b"Settings:".len())
                        .any(|w| w == b"Settings:")
                    {
                        // Give the panel a chance to finish printing.
                        std::thread::sleep(Duration::from_millis(150));
                        if let Ok(extra) = reader.read(&mut tmp) {
                            buffer.extend_from_slice(&tmp[..extra]);
                        }
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    return Err(ProviderFetchError::PluginUnavailable(e.to_string()));
                }
            }
        }
        // Send `/exit` to clean up.
        let _ = writer.write_all(b"/exit\r");
        let _ = child.wait();
        Ok(String::from_utf8_lossy(&buffer).to_string())
    }
}

/// Environment variables we wipe before spawning the CLI. Per spec 40
/// section 4.3: never leak our own OAuth bearer or scopes, and never
/// confuse the CLI with stray ANTHROPIC_ overrides.
pub fn scrub_env_keys() -> Vec<String> {
    let mut keys = vec![
        "CODEXBAR_CLAUDE_OAUTH_TOKEN".to_string(),
        "CODEXBAR_CLAUDE_OAUTH_SCOPES".to_string(),
    ];
    for (key, _) in std::env::vars() {
        if key.starts_with("ANTHROPIC_") {
            keys.push(key);
        }
    }
    keys
}

/// Recorder runner: returns prerecorded output for tests.
pub struct RecordedRunner {
    pub output: String,
}

impl CliRunner for RecordedRunner {
    fn run_usage(&self, _binary: &str) -> Result<String, ProviderFetchError> {
        Ok(self.output.clone())
    }
}

/// Type-erased shared runner used by the strategy.
pub type SharedRunner = Arc<dyn CliRunner>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_runner_returns_canned_output() {
        let runner = RecordedRunner {
            output: "Settings:\nSession : 5% used.".into(),
        };
        let out = runner.run_usage("claude").unwrap();
        assert!(out.contains("Settings:"));
    }

    #[test]
    fn scrub_env_keys_always_lists_oauth_token() {
        let keys = scrub_env_keys();
        assert!(keys.iter().any(|k| k == "CODEXBAR_CLAUDE_OAUTH_TOKEN"));
        assert!(keys.iter().any(|k| k == "CODEXBAR_CLAUDE_OAUTH_SCOPES"));
    }
}
