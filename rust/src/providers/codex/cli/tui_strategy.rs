//! Codex CLI TUI strategy. Spawns the real `codex` binary inside a
//! ConPTY, sends `/status` followed by Enter, drains stdout until the
//! status panel finishes printing, and folds the parsed snapshot into
//! the framework's `UsageSnapshot`. Ported from
//! `CodexStatusProbe.runAndParse` in the macOS Swift source.
//!
//! Why a separate strategy from `CodexCliStrategy`: the existing one
//! assumes the codex binary speaks JSON-RPC, which it does not. This
//! strategy reads the raw TUI output and parses it with `tui_parser`.

use std::io::{Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

use super::tui_parser::{parse, CodexTuiSnapshot, ParseError};
use crate::providers::codex::descriptor::CODEX_ID;
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::identity::ProviderIdentitySnapshot;
use crate::providers::models::credits::{CreditUnit, CreditsSnapshot};
use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
use crate::providers::models::UsageSnapshot;

pub const PTY_ROWS: u16 = 60;
pub const PTY_COLS: u16 = 200;
/// Hard timeout per attempt. Matches the macOS `defaultTimeoutSeconds`.
pub const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(8);
/// Short retry budget — Swift uses 4 s for the second attempt.
pub const RETRY_TIMEOUT: Duration = Duration::from_secs(4);

/// Pluggable runner so tests can drive the parser with prerecorded TUI
/// transcripts instead of launching codex. The real impl spawns the
/// binary and drains the PTY master.
pub trait CodexTuiRunner: Send + Sync {
    fn capture_status(&self, binary: &str, timeout: Duration)
        -> Result<String, ProviderFetchError>;
}

pub struct RealCodexTuiRunner;

impl CodexTuiRunner for RealCodexTuiRunner {
    fn capture_status(
        &self,
        binary: &str,
        timeout: Duration,
    ) -> Result<String, ProviderFetchError> {
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
        for arg in &["-s", "read-only", "-a", "untrusted"] {
            cmd.arg(arg);
        }
        for key in scrub_env_keys() {
            cmd.env_remove(key);
        }
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

        // Send `/status` after a tiny delay so the TUI's start-up
        // banner does not race the prompt.
        std::thread::sleep(Duration::from_millis(250));
        let _ = writer.write_all(b"/status\r");
        let _ = writer.flush();

        let started = Instant::now();
        let mut buffer = Vec::with_capacity(64 * 1024);
        let mut tmp = [0u8; 4096];
        while started.elapsed() < timeout {
            match reader.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => {
                    buffer.extend_from_slice(&tmp[..n]);
                    // The /status panel renders 5h limit and Weekly
                    // limit lines back-to-back. Stop draining once both
                    // appear so we are not waiting on the cursor blink
                    // indefinitely.
                    if buffer
                        .windows(b"Weekly limit".len())
                        .any(|w| w == b"Weekly limit")
                    {
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
        // Try to exit cleanly so we don't leak a child process. The
        // wait is best-effort; the JobObject watchdog handles the
        // worst case.
        let _ = writer.write_all(b"/exit\r");
        let _ = writer.flush();
        let _ = child.wait();
        Ok(String::from_utf8_lossy(&buffer).to_string())
    }
}

/// Keys we scrub before launching codex so our OAuth state cannot leak
/// into the child. Mirrors the conpty_transport scrub list.
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

pub struct CodexTuiStrategy {
    runner: Arc<dyn CodexTuiRunner>,
    binary: String,
}

impl CodexTuiStrategy {
    pub fn new(runner: Arc<dyn CodexTuiRunner>, binary: impl Into<String>) -> Self {
        Self {
            runner,
            binary: binary.into(),
        }
    }
}

#[async_trait]
impl Strategy for CodexTuiStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::CLI
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        // Spawn the blocking PTY drain on a dedicated thread; we cannot
        // hold raw PTY handles across awaits.
        let runner = self.runner.clone();
        let binary = self.binary.clone();
        let text = tokio::task::spawn_blocking(move || -> Result<String, ProviderFetchError> {
            match runner.capture_status(&binary, ATTEMPT_TIMEOUT) {
                Ok(t) => Ok(t),
                Err(err) => Err(err),
            }
        })
        .await
        .map_err(|e| ProviderFetchError::PluginUnavailable(e.to_string()))??;

        let snapshot = match parse(&text) {
            Ok(s) => s,
            Err(ParseError::Empty) | Err(ParseError::NoUsageData) => {
                // Retry once with a tighter budget. Spec says transient
                // parse flakes are common when the TUI animation is
                // still drawing.
                let runner = self.runner.clone();
                let binary = self.binary.clone();
                let retry = tokio::task::spawn_blocking(move || {
                    runner.capture_status(&binary, RETRY_TIMEOUT)
                })
                .await
                .map_err(|e| ProviderFetchError::PluginUnavailable(e.to_string()))??;
                parse(&retry).map_err(parse_error_to_fetch)?
            }
            Err(e) => return Err(parse_error_to_fetch(e)),
        };

        Ok(snapshot_to_usage(snapshot))
    }
}

fn parse_error_to_fetch(err: ParseError) -> ProviderFetchError {
    match err {
        ParseError::Empty => ProviderFetchError::Timeout {
            budget_ms: (ATTEMPT_TIMEOUT.as_millis() as u64),
        },
        ParseError::DataNotAvailable => {
            ProviderFetchError::Network("codex reports no data yet; retry on next tick".into())
        }
        ParseError::UpdateRequired => ProviderFetchError::UserConfigInvalid(
            "codex CLI requires an update; run `bun install -g @openai/codex`".into(),
        ),
        ParseError::NoUsageData => {
            ProviderFetchError::ParseError("codex /status panel had no usage fields".into())
        }
    }
}

fn snapshot_to_usage(tui: CodexTuiSnapshot) -> UsageSnapshot {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default();

    let mut windows = Vec::new();
    if let Some(pct_left) = tui.five_hour_percent_left {
        windows.push(NamedRateWindow {
            key: "session".into(),
            window: RateWindow {
                label: "Session".into(),
                used: percent_used_from_left(pct_left),
                allotted: Some(100.0),
                reset_at_unix_secs: None,
                pace_delta_percent: None,
            },
        });
    }
    if let Some(pct_left) = tui.weekly_percent_left {
        windows.push(NamedRateWindow {
            key: "weekly".into(),
            window: RateWindow {
                label: "Week".into(),
                used: percent_used_from_left(pct_left),
                allotted: Some(100.0),
                reset_at_unix_secs: None,
                pace_delta_percent: None,
            },
        });
    }

    let credits = tui.credits.map(|balance| CreditsSnapshot {
        balance,
        unit: CreditUnit::UsdCents,
        recent_events: Vec::new(),
    });

    let plan_name = build_plan_label(&tui);

    UsageSnapshot {
        identity: ProviderIdentitySnapshot::new(CODEX_ID, "codex:cli".to_string()),
        windows,
        credits,
        cost: None,
        account_display_name: None,
        account_email: None,
        plan_name,
        captured_at_unix_secs: now,
    }
}

fn percent_used_from_left(percent_left: i64) -> f64 {
    (100.0 - percent_left as f64).clamp(0.0, 100.0)
}

fn build_plan_label(tui: &CodexTuiSnapshot) -> Option<String> {
    let parts: Vec<String> = [
        tui.five_hour_reset_hint
            .as_deref()
            .map(|hint| format!("5h resets {hint}")),
        tui.weekly_reset_hint
            .as_deref()
            .map(|hint| format!("Week resets {hint}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordedRunner {
        outputs: Mutex<Vec<String>>,
    }
    impl RecordedRunner {
        fn new(outputs: Vec<&str>) -> Self {
            Self {
                outputs: Mutex::new(outputs.into_iter().map(String::from).collect()),
            }
        }
    }
    impl CodexTuiRunner for RecordedRunner {
        fn capture_status(
            &self,
            _binary: &str,
            _timeout: Duration,
        ) -> Result<String, ProviderFetchError> {
            let mut outputs = self.outputs.lock().unwrap();
            if outputs.is_empty() {
                return Err(ProviderFetchError::Timeout { budget_ms: 0 });
            }
            Ok(outputs.remove(0))
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
    fn happy_path_produces_session_and_weekly_windows_plus_credits() {
        let runner = Arc::new(RecordedRunner::new(vec![
            "Credits: 7.50\n5h limit  78% left  resets 13:42 on 5 Jun\nWeekly limit  42% left  resets 09:00 on Sun 8 Jun\n",
        ]));
        let strategy = CodexTuiStrategy::new(runner, "codex");
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 2);
        assert_eq!(snap.windows[0].window.label, "Session");
        assert!((snap.windows[0].window.used - 22.0).abs() < 1e-9);
        assert_eq!(snap.windows[1].window.label, "Week");
        assert!((snap.windows[1].window.used - 58.0).abs() < 1e-9);
        let credits = snap.credits.unwrap();
        assert!((credits.balance - 7.5).abs() < 1e-9);
        let plan = snap.plan_name.unwrap();
        assert!(plan.contains("5h resets"));
        assert!(plan.contains("Week resets"));
    }

    #[test]
    fn data_not_available_propagates_as_network_error() {
        let runner = Arc::new(RecordedRunner::new(vec![
            "Data not available yet, please retry",
        ]));
        let strategy = CodexTuiStrategy::new(runner, "codex");
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Network(_)));
    }

    #[test]
    fn update_prompt_propagates_as_user_config_invalid() {
        let runner = Arc::new(RecordedRunner::new(vec![
            "Update available: codex 1.2.3 → 1.3.0\nRun `bun install -g @openai/codex` to continue.",
        ]));
        let strategy = CodexTuiStrategy::new(runner, "codex");
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::UserConfigInvalid(_)));
    }

    #[test]
    fn empty_first_attempt_retries_and_succeeds() {
        let runner = Arc::new(RecordedRunner::new(vec![
            "",
            "Credits: 1.00\n5h limit  10% left  resets 09:00",
        ]));
        let strategy = CodexTuiStrategy::new(runner, "codex");
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].window.label, "Session");
        assert!((snap.windows[0].window.used - 90.0).abs() < 1e-9);
    }

    #[test]
    fn two_empty_attempts_fail_as_timeout() {
        let runner = Arc::new(RecordedRunner::new(vec!["", ""]));
        let strategy = CodexTuiStrategy::new(runner, "codex");
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::Timeout { .. }));
    }

    #[test]
    fn snapshot_to_usage_omits_session_when_only_credits_present() {
        let tui = CodexTuiSnapshot {
            credits: Some(2.0),
            ..CodexTuiSnapshot::default()
        };
        let snap = snapshot_to_usage(tui);
        assert!(snap.windows.is_empty());
        assert!(snap.credits.is_some());
        assert!(snap.plan_name.is_none());
    }

    #[test]
    fn percent_used_clamps_at_zero_when_percent_left_above_100() {
        assert_eq!(percent_used_from_left(150), 0.0);
        assert_eq!(percent_used_from_left(-10), 100.0);
    }
}
