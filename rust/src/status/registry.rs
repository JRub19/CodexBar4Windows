//! Per-provider status registry backed by provider metadata.

use super::feed::{StatusFeed, StatusFeedKind};
use crate::providers::descriptor::ProviderStatusFeed;
use crate::providers::REGISTRY;

pub const GEMINI_GWS_PRODUCT_ID: &str = "npdyhgECDJ6tB66MxXyo";

/// Returns the polling feed for `provider_id` when one exists, else
/// `None`. Provider ids match the framework's `ProviderId` strings.
pub fn feed_for_provider(provider_id: &str) -> Option<StatusFeed> {
    let descriptor = REGISTRY
        .descriptors()
        .find(|descriptor| descriptor.id.as_str() == provider_id)?;
    let public_url = descriptor.metadata.status.status_page_url?;
    let kind = match descriptor.metadata.status.feed? {
        ProviderStatusFeed::Statuspage { base_url } => StatusFeedKind::Statuspage { base_url },
        ProviderStatusFeed::GoogleWorkspace { product_id } => {
            StatusFeedKind::GoogleWorkspace { product_id }
        }
    };
    Some(StatusFeed {
        provider_id: descriptor.id.as_str(),
        kind,
        public_url,
    })
}

/// Public status URL for providers that expose a link but no
/// machine-readable feed.
pub fn link_only_for_provider(provider_id: &str) -> Option<&'static str> {
    let descriptor = REGISTRY
        .descriptors()
        .find(|descriptor| descriptor.id.as_str() == provider_id)?;
    if descriptor.metadata.status.feed.is_some() {
        return None;
    }
    descriptor.metadata.status.status_page_url
}

/// Every registered provider id that has either a polled feed or a
/// link-only status page.
pub fn all_status_capable_provider_ids() -> Vec<&'static str> {
    REGISTRY
        .descriptors()
        .filter(|descriptor| descriptor.metadata.status.status_page_url.is_some())
        .map(|descriptor| descriptor.id.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polled_feeds_come_from_registered_provider_metadata() {
        for id in ["codex", "claude", "cursor", "factory", "copilot", "gemini"] {
            assert!(
                feed_for_provider(id).is_some(),
                "{id} should have a polled feed",
            );
        }
    }

    #[test]
    fn link_only_providers_are_registered_shipped_providers() {
        assert!(link_only_for_provider("deepseek").is_some());
        assert!(link_only_for_provider("openrouter").is_some());
        assert!(link_only_for_provider("mistral").is_some());
    }

    #[test]
    fn no_status_providers_get_neither() {
        for id in ["hello", "venice", "zai"] {
            assert!(feed_for_provider(id).is_none(), "{id} should not poll");
            assert!(link_only_for_provider(id).is_none(), "{id} no link-only");
        }
    }

    #[test]
    fn gemini_product_id_matches_descriptor_metadata() {
        let g = feed_for_provider("gemini").unwrap();
        match g.kind {
            StatusFeedKind::GoogleWorkspace { product_id } => {
                assert_eq!(product_id, GEMINI_GWS_PRODUCT_ID);
            }
            _ => panic!("Gemini should be a GWS feed"),
        }
    }
}
