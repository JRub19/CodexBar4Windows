//! `ProviderImplementation` lifecycle trait. Spec 30 section 17 lists
//! every hook; the trait is object safe so we can store providers as
//! `Box<dyn ProviderImplementation>` in the registry.
//!
//! Most hooks have a sensible default behavior; concrete providers
//! override only what they need to customize. The framework calls the
//! trait at three points:
//! 1. Boot, to fetch descriptors and settings descriptors.
//! 2. Every refresh, to run the fetch plan and fold the result into
//!    presentation data.
//! 3. On user action (refresh now, source mode change), to invalidate
//!    cached state.

use std::sync::Arc;

use async_trait::async_trait;

use super::contexts::{
    ProviderAvailabilityContext, ProviderPresentationContext, ProviderSourceLabelContext,
    ProviderSourceModeContext, ProviderVersionContext,
};
use super::descriptor::ProviderDescriptor;
use super::fetch_context::ProviderFetchContext;
use super::fetch_outcome::ProviderFetchOutcome;
use super::fetch_plan_runtime::{run_pipeline, Strategy};
use super::presentation::ProviderPresentation;

#[async_trait]
pub trait ProviderImplementation: Send + Sync {
    /// The static descriptor this provider registered with. The
    /// framework uses this to fill the registry and the IPC bridge.
    fn descriptor(&self) -> &ProviderDescriptor;

    /// The runtime strategies this provider exposes, in plan order.
    /// The default impl returns an empty list, which makes the provider
    /// unavailable; concrete providers override.
    fn strategies(&self) -> Vec<Arc<dyn Strategy>> {
        Vec::new()
    }

    /// Run one full refresh tick. The default folds together the
    /// strategy list and the framework's pipeline runtime.
    async fn refresh(&self, context: &ProviderFetchContext) -> ProviderFetchOutcome {
        let strategies = self.strategies();
        run_pipeline(&strategies, context).await
    }

    /// Fold the most recent fetch outcome into a presentation view
    /// model for the popup card. Default reads the snapshot directly.
    fn presentation(&self, context: &ProviderPresentationContext) -> ProviderPresentation {
        let display_name = self.descriptor().metadata.display_name.to_string();
        let Some(snapshot) = &context.snapshot else {
            return ProviderPresentation::empty(display_name);
        };
        ProviderPresentation {
            display_name,
            plan_name: snapshot.plan_name.clone(),
            email: snapshot.account_email.clone(),
            subtitle: snapshot.account_display_name.clone(),
            metrics: snapshot
                .windows
                .iter()
                .map(|w| super::presentation::PresentationMetric {
                    title: w.window.label.clone(),
                    percent: Some(100.0 - w.window.remaining_percent()),
                    detail_left: None,
                    detail_right: None,
                    reset_text: None,
                })
                .collect(),
        }
    }

    /// Whether the provider is currently usable. Defaults to "yes when
    /// at least one strategy was registered."
    fn availability(&self, _: &ProviderAvailabilityContext) -> Availability {
        if self.strategies().is_empty() {
            Availability::Unavailable("no strategies registered".to_string())
        } else {
            Availability::Available
        }
    }

    /// Short label for the debug source pill. Defaults to the winning
    /// strategy's name; concrete providers can return brand-specific
    /// labels ("OAuth (Claude.ai)", "Web (Cookie)").
    fn source_label(&self, ctx: &ProviderSourceLabelContext) -> String {
        match ctx.winning_strategy {
            Some(super::descriptor::FetchStrategy::OAuth) => "OAuth".into(),
            Some(super::descriptor::FetchStrategy::Web) => "Web".into(),
            Some(super::descriptor::FetchStrategy::CLI) => "CLI".into(),
            Some(super::descriptor::FetchStrategy::ApiKey) => "API key".into(),
            None => "—".into(),
        }
    }

    /// Source mode actions the settings pane should expose. Defaults to
    /// the full Auto/OAuth/Web/CLI/Disabled set; concrete providers can
    /// narrow this list to match the strategies they actually have.
    fn source_mode_options(&self, _ctx: &ProviderSourceModeContext) -> Vec<&'static str> {
        vec!["Auto", "OAuth", "Web", "CLI", "Disabled"]
    }

    /// Version string for the provider implementation. Currently used
    /// only by the debug pane and the log dump.
    fn version(&self, _ctx: &ProviderVersionContext) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Available,
    Unavailable(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ProviderId;
    use crate::providers::branding::ProviderBranding;
    use crate::providers::descriptor::{ProviderFetchPlan, ProviderMetadata};

    struct NullProvider {
        descriptor: ProviderDescriptor,
    }

    impl NullProvider {
        fn new() -> Self {
            Self {
                descriptor: ProviderDescriptor {
                    id: ProviderId("null"),
                    metadata: ProviderMetadata::minimal("Null", "https://null.example"),
                    branding: ProviderBranding::solid("#000000", "null"),
                    cli: None,
                    fetch_plan: ProviderFetchPlan::default(),
                },
            }
        }
    }

    impl ProviderImplementation for NullProvider {
        fn descriptor(&self) -> &ProviderDescriptor {
            &self.descriptor
        }
    }

    #[test]
    fn default_impl_compiles_and_advertises_unavailable() {
        let p = NullProvider::new();
        let availability = p.availability(&ProviderAvailabilityContext {
            provider_id: p.descriptor().id,
            winning_strategy: None,
        });
        assert!(matches!(availability, Availability::Unavailable(_)));
    }

    #[test]
    fn trait_is_object_safe() {
        let p: Box<dyn ProviderImplementation> = Box::new(NullProvider::new());
        assert_eq!(p.descriptor().id.as_str(), "null");
    }

    #[test]
    fn empty_presentation_returns_awaiting_subtitle() {
        let p = NullProvider::new();
        let pr = p.presentation(&ProviderPresentationContext {
            provider_id: p.descriptor().id,
            snapshot: None,
        });
        assert_eq!(pr.display_name, "Null");
        assert_eq!(pr.subtitle.as_deref(), Some("Awaiting first refresh"));
        assert!(pr.metrics.is_empty());
    }
}
