//! `ClaudeCliStrategy`. Runs the Claude CLI via `pty_actor`, captures
//! the `/usage` panel, and folds it into a `UsageSnapshot`.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;

use super::parser::{parse_panel, snapshot_from_rows};
use super::pty_actor::SharedRunner;
use super::reset_parser::{fold_to_epoch, parse as parse_reset};
use crate::providers::descriptor::FetchStrategy;
use crate::providers::errors::ProviderFetchError;
use crate::providers::fetch_context::ProviderFetchContext;
use crate::providers::fetch_plan_runtime::Strategy;
use crate::providers::models::UsageSnapshot;

pub struct ClaudeCliStrategy {
    runner: SharedRunner,
    binary: String,
}

impl ClaudeCliStrategy {
    pub fn new(runner: SharedRunner, binary: impl Into<String>) -> Self {
        Self {
            runner,
            binary: binary.into(),
        }
    }
}

#[async_trait]
impl Strategy for ClaudeCliStrategy {
    fn strategy_id(&self) -> FetchStrategy {
        FetchStrategy::CLI
    }

    async fn fetch(&self, _: &ProviderFetchContext) -> Result<UsageSnapshot, ProviderFetchError> {
        let runner = self.runner.clone();
        let binary = self.binary.clone();
        let output = tokio::task::spawn_blocking(move || runner.run_usage(&binary))
            .await
            .map_err(|e| ProviderFetchError::PluginUnavailable(e.to_string()))??;
        let rows = parse_panel(&output);
        if rows.is_empty() {
            return Err(ProviderFetchError::ParseError(
                "no usage rows found in CLI output".into(),
            ));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let mut snapshot = snapshot_from_rows(&rows, "claude:cli", now);
        // Fold any reset hints into the window reset times.
        for (i, row) in rows.iter().enumerate() {
            if let Some(hint) = row.reset_hint.as_deref() {
                if let Some(parsed) = parse_reset(hint) {
                    let now_local = chrono::Local::now().naive_local();
                    if let Some(epoch) = fold_to_epoch(&parsed, now_local) {
                        if let Some(window) = snapshot.windows.get_mut(i) {
                            window.window.reset_at_unix_secs = Some(epoch);
                        }
                    }
                }
            }
        }
        Ok(snapshot)
    }
}

pub fn shared_runner(runner: Arc<dyn super::pty_actor::CliRunner>) -> SharedRunner {
    runner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ProviderId;
    use crate::providers::claude::cli::pty_actor::RecordedRunner;
    use crate::providers::fetch_context::{Runtime, SourceMode};
    use crate::secrets::token_account::TokenAccountStore;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn ctx() -> ProviderFetchContext {
        let tokens = Arc::new(TokenAccountStore::new(std::env::temp_dir()));
        ProviderFetchContext {
            provider_id: ProviderId("claude"),
            mode: SourceMode::Auto,
            runtime: Runtime { tokens },
        }
    }

    const FIXTURE: &str = "\
Settings:\n\
Session : 25% used. Resets 8pm\n\
Week    : 60% used. Resets May 14 at 11am\n\
Week (Opus) : 10% used. Resets May 14 at 11am\n\
";

    #[test]
    fn parses_fixture_into_three_windows() {
        let runner: SharedRunner = Arc::new(RecordedRunner {
            output: FIXTURE.into(),
        });
        let strategy = ClaudeCliStrategy::new(runner, "claude");
        let snap = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap();
        assert_eq!(snap.windows.len(), 3);
        assert_eq!(snap.windows[0].key, "five_hour");
        assert_eq!(snap.windows[1].key, "seven_day");
        assert_eq!(snap.windows[2].key, "seven_day_opus");
        assert!(snap.windows[0].window.reset_at_unix_secs.is_some());
    }

    #[test]
    fn empty_output_returns_parse_error() {
        let runner: SharedRunner = Arc::new(RecordedRunner { output: "".into() });
        let strategy = ClaudeCliStrategy::new(runner, "claude");
        let err = rt()
            .block_on(async { strategy.fetch(&ctx()).await })
            .unwrap_err();
        assert!(matches!(err, ProviderFetchError::ParseError(_)));
    }
}
