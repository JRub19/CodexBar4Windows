//! Runtime for `ProviderFetchPlan`. Iterates the configured strategy
//! list, applying the 45 s per-strategy timeout from spec 30 section 5.1
//! and consulting each error's `should_fallback` to decide whether to
//! advance to the next strategy.
//!
//! This module is intentionally provider-agnostic. It does not know
//! about Claude or any specific HTTP endpoint; concrete strategies plug
//! in via the `Strategy` trait.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::time::timeout;

use super::descriptor::FetchStrategy;
use super::errors::ProviderFetchError;
use super::fetch_context::{ProviderFetchContext, SourceMode};
use super::fetch_outcome::{ProviderFetchAttempt, ProviderFetchOutcome};
use super::models::UsageSnapshot;

pub const PER_STRATEGY_TIMEOUT: Duration = Duration::from_secs(45);

/// Implemented by each concrete strategy (OAuth, Web, CLI, ApiKey).
/// Returns the snapshot on success; on failure, returns the typed
/// `ProviderFetchError` that the runtime uses to decide on fallback.
#[async_trait]
pub trait Strategy: Send + Sync {
    fn strategy_id(&self) -> FetchStrategy;

    async fn fetch(
        &self,
        context: &ProviderFetchContext,
    ) -> Result<UsageSnapshot, ProviderFetchError>;

    /// Override the default fallback decision when the strategy knows
    /// better. Default delegates to `ProviderFetchError::should_fallback`.
    fn should_fallback(&self, error: &ProviderFetchError) -> bool {
        error.should_fallback()
    }
}

/// Run the configured strategies in order, returning the first success
/// or the last error if every strategy failed.
pub async fn run_pipeline(
    strategies: &[Arc<dyn Strategy>],
    context: &ProviderFetchContext,
) -> ProviderFetchOutcome {
    let mut attempts: Vec<ProviderFetchAttempt> = Vec::new();
    for strategy in strategies {
        if !is_eligible(context.mode, strategy.strategy_id()) {
            continue;
        }
        let started = Instant::now();
        let result = timeout(PER_STRATEGY_TIMEOUT, strategy.fetch(context)).await;
        let duration_ms = started.elapsed().as_millis() as u64;
        match result {
            Ok(Ok(snapshot)) => {
                attempts.push(ProviderFetchAttempt {
                    strategy: strategy.strategy_id(),
                    duration_ms,
                    error_kind: None,
                    error_detail: None,
                });
                return ProviderFetchOutcome {
                    provider_id: context.provider_id.as_str().to_string(),
                    snapshot: Some(snapshot),
                    winning_strategy: Some(strategy.strategy_id()),
                    attempts,
                };
            }
            Ok(Err(error)) => {
                let should_fallback = strategy.should_fallback(&error);
                attempts.push(ProviderFetchAttempt {
                    strategy: strategy.strategy_id(),
                    duration_ms,
                    error_kind: Some(error.kind().to_string()),
                    error_detail: Some(error.to_string()),
                });
                if !should_fallback {
                    break;
                }
            }
            Err(_elapsed) => {
                attempts.push(ProviderFetchAttempt {
                    strategy: strategy.strategy_id(),
                    duration_ms,
                    error_kind: Some("timeout".into()),
                    error_detail: Some(format!(
                        "strategy timed out after {} ms",
                        PER_STRATEGY_TIMEOUT.as_millis(),
                    )),
                });
                // Timeout always advances per spec 30 section 5.1.
            }
        }
    }
    ProviderFetchOutcome {
        provider_id: context.provider_id.as_str().to_string(),
        snapshot: None,
        winning_strategy: None,
        attempts,
    }
}

fn is_eligible(mode: SourceMode, strategy: FetchStrategy) -> bool {
    match mode {
        SourceMode::Auto => true,
        SourceMode::Forced(forced) => forced == strategy,
        SourceMode::Disabled => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ProviderId;
    use crate::providers::identity::ProviderIdentitySnapshot;
    use crate::providers::models::rate_window::{NamedRateWindow, RateWindow};
    use crate::secrets::token_account::TokenAccountStore;

    fn make_ctx() -> ProviderFetchContext {
        let tokens = Arc::new(TokenAccountStore::new(std::env::temp_dir()));
        ProviderFetchContext {
            provider_id: ProviderId("test"),
            mode: SourceMode::Auto,
            runtime: super::super::fetch_context::Runtime { tokens },
        }
    }

    fn snap(id: &str) -> UsageSnapshot {
        UsageSnapshot {
            identity: ProviderIdentitySnapshot::new(ProviderId("test"), id),
            windows: vec![NamedRateWindow {
                key: "session".into(),
                window: RateWindow {
                    label: "Session".into(),
                    used: 1.0,
                    allotted: Some(2.0),
                    reset_at_unix_secs: None,
                    pace_delta_percent: None,
                },
            }],
            credits: None,
            cost: None,
            account_display_name: None,
            account_email: None,
            plan_name: None,
            captured_at_unix_secs: 0,
        }
    }

    struct OkStrategy(FetchStrategy);
    #[async_trait]
    impl Strategy for OkStrategy {
        fn strategy_id(&self) -> FetchStrategy {
            self.0
        }
        async fn fetch(
            &self,
            _ctx: &ProviderFetchContext,
        ) -> Result<UsageSnapshot, ProviderFetchError> {
            Ok(snap("ok"))
        }
    }

    struct FailStrategy(FetchStrategy, bool); // (id, should_fallback)
    #[async_trait]
    impl Strategy for FailStrategy {
        fn strategy_id(&self) -> FetchStrategy {
            self.0
        }
        async fn fetch(
            &self,
            _ctx: &ProviderFetchContext,
        ) -> Result<UsageSnapshot, ProviderFetchError> {
            if self.1 {
                Err(ProviderFetchError::Network("boom".into()))
            } else {
                Err(ProviderFetchError::Unauthorized)
            }
        }
    }

    struct SlowStrategy(FetchStrategy);
    #[async_trait]
    impl Strategy for SlowStrategy {
        fn strategy_id(&self) -> FetchStrategy {
            self.0
        }
        async fn fetch(
            &self,
            _ctx: &ProviderFetchContext,
        ) -> Result<UsageSnapshot, ProviderFetchError> {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(snap("slow"))
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap()
    }

    #[test]
    fn first_success_short_circuits_the_rest() {
        rt().block_on(async {
            let strategies: Vec<Arc<dyn Strategy>> = vec![
                Arc::new(OkStrategy(FetchStrategy::OAuth)),
                Arc::new(FailStrategy(FetchStrategy::Web, true)),
            ];
            let outcome = run_pipeline(&strategies, &make_ctx()).await;
            assert_eq!(outcome.winning_strategy, Some(FetchStrategy::OAuth));
            assert_eq!(outcome.attempts.len(), 1);
        });
    }

    #[test]
    fn fallback_true_advances_to_next_strategy() {
        rt().block_on(async {
            let strategies: Vec<Arc<dyn Strategy>> = vec![
                Arc::new(FailStrategy(FetchStrategy::OAuth, true)),
                Arc::new(OkStrategy(FetchStrategy::Web)),
            ];
            let outcome = run_pipeline(&strategies, &make_ctx()).await;
            assert_eq!(outcome.winning_strategy, Some(FetchStrategy::Web));
            assert_eq!(outcome.attempts.len(), 2);
        });
    }

    #[test]
    fn fallback_false_stops_the_pipeline() {
        rt().block_on(async {
            let strategies: Vec<Arc<dyn Strategy>> = vec![
                Arc::new(FailStrategy(FetchStrategy::OAuth, false)),
                Arc::new(OkStrategy(FetchStrategy::Web)),
            ];
            let outcome = run_pipeline(&strategies, &make_ctx()).await;
            assert!(outcome.snapshot.is_none());
            assert_eq!(outcome.attempts.len(), 1);
        });
    }

    #[test]
    fn slow_strategy_times_out_per_budget() {
        rt().block_on(async {
            let strategies: Vec<Arc<dyn Strategy>> = vec![
                Arc::new(SlowStrategy(FetchStrategy::OAuth)),
                Arc::new(OkStrategy(FetchStrategy::Web)),
            ];
            let outcome = run_pipeline(&strategies, &make_ctx()).await;
            assert_eq!(outcome.winning_strategy, Some(FetchStrategy::Web));
            assert_eq!(outcome.attempts[0].error_kind.as_deref(), Some("timeout"));
        });
    }

    #[test]
    fn forced_mode_only_runs_matching_strategy() {
        rt().block_on(async {
            let mut ctx = make_ctx();
            ctx.mode = SourceMode::Forced(FetchStrategy::Web);
            let strategies: Vec<Arc<dyn Strategy>> = vec![
                Arc::new(FailStrategy(FetchStrategy::OAuth, true)),
                Arc::new(OkStrategy(FetchStrategy::Web)),
            ];
            let outcome = run_pipeline(&strategies, &ctx).await;
            assert_eq!(outcome.winning_strategy, Some(FetchStrategy::Web));
            assert_eq!(outcome.attempts.len(), 1);
            assert_eq!(outcome.attempts[0].strategy, FetchStrategy::Web);
        });
    }
}
