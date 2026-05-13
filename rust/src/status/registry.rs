//! Per-provider feed registry. Maps a provider id to its public status
//! page URL plus the parser/transport shape we should use. Mirrors
//! `docs/windows/spec/55-status-incidents.md` §2.1 / §2.2 verbatim.

use super::feed::{StatusFeed, StatusFeedKind};

pub const GEMINI_GWS_PRODUCT_ID: &str = "npdyhgECDJ6tB66MxXyo";

/// Returns the polling feed for `provider_id` when one exists, else
/// `None`. Provider ids match the framework's `ProviderId` strings.
pub fn feed_for_provider(provider_id: &str) -> Option<StatusFeed> {
    Some(match provider_id {
        "codex" => StatusFeed {
            provider_id: "codex",
            kind: StatusFeedKind::Statuspage {
                base_url: "https://status.openai.com",
            },
            public_url: "https://status.openai.com",
        },
        "openai" => StatusFeed {
            provider_id: "openai",
            kind: StatusFeedKind::Statuspage {
                base_url: "https://status.openai.com",
            },
            public_url: "https://status.openai.com",
        },
        "claude" => StatusFeed {
            provider_id: "claude",
            kind: StatusFeedKind::Statuspage {
                base_url: "https://status.claude.com",
            },
            public_url: "https://status.claude.com",
        },
        "cursor" => StatusFeed {
            provider_id: "cursor",
            kind: StatusFeedKind::Statuspage {
                base_url: "https://status.cursor.com",
            },
            public_url: "https://status.cursor.com",
        },
        "factory" => StatusFeed {
            provider_id: "factory",
            kind: StatusFeedKind::Statuspage {
                base_url: "https://status.factory.ai",
            },
            public_url: "https://status.factory.ai",
        },
        "copilot" => StatusFeed {
            provider_id: "copilot",
            kind: StatusFeedKind::Statuspage {
                base_url: "https://www.githubstatus.com",
            },
            public_url: "https://www.githubstatus.com",
        },
        "gemini" => StatusFeed {
            provider_id: "gemini",
            kind: StatusFeedKind::GoogleWorkspace {
                product_id: GEMINI_GWS_PRODUCT_ID,
            },
            public_url:
                "https://www.google.com/appsstatus/dashboard/products/npdyhgECDJ6tB66MxXyo/history",
        },
        "antigravity" => StatusFeed {
            provider_id: "antigravity",
            kind: StatusFeedKind::GoogleWorkspace {
                product_id: GEMINI_GWS_PRODUCT_ID,
            },
            public_url:
                "https://www.google.com/appsstatus/dashboard/products/npdyhgECDJ6tB66MxXyo/history",
        },
        _ => return None,
    })
}

/// Public status URL the menu's "Status Page" action opens for
/// providers that expose a link but no machine-readable feed (and
/// therefore no polling). Spec 55 §2.2.
pub fn link_only_for_provider(provider_id: &str) -> Option<&'static str> {
    Some(match provider_id {
        "alibaba" => "https://status.aliyun.com",
        "deepseek" => "https://status.deepseek.com",
        "kiro" => "https://health.aws.amazon.com/health/status",
        "mistral" => "https://status.mistral.ai",
        "openrouter" => "https://status.openrouter.ai",
        "perplexity" => "https://status.perplexity.com/",
        "vertexai" => "https://status.cloud.google.com",
        _ => return None,
    })
}

/// Every provider id that has either a polled feed or a link-only page.
/// Used by the IPC layer to populate menu entries on the React side.
pub fn all_status_capable_provider_ids() -> &'static [&'static str] {
    &[
        "codex",
        "openai",
        "claude",
        "cursor",
        "factory",
        "copilot",
        "gemini",
        "antigravity",
        "alibaba",
        "deepseek",
        "kiro",
        "mistral",
        "openrouter",
        "perplexity",
        "vertexai",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polled_feeds_cover_the_spec_set() {
        for id in [
            "codex",
            "openai",
            "claude",
            "cursor",
            "factory",
            "copilot",
            "gemini",
            "antigravity",
        ] {
            assert!(
                feed_for_provider(id).is_some(),
                "{id} should have a polled feed",
            );
        }
    }

    #[test]
    fn link_only_providers_are_recognised() {
        assert!(link_only_for_provider("deepseek").is_some());
        assert!(link_only_for_provider("mistral").is_some());
        assert!(link_only_for_provider("perplexity").is_some());
    }

    #[test]
    fn no_status_providers_get_neither() {
        for id in ["abacus", "warp", "ollama", "venice", "synthetic"] {
            assert!(feed_for_provider(id).is_none(), "{id} should not poll");
            assert!(link_only_for_provider(id).is_none(), "{id} no link-only");
        }
    }

    #[test]
    fn gemini_and_antigravity_share_a_product_id() {
        let g = feed_for_provider("gemini").unwrap();
        let a = feed_for_provider("antigravity").unwrap();
        match (g.kind, a.kind) {
            (
                StatusFeedKind::GoogleWorkspace { product_id: g_id },
                StatusFeedKind::GoogleWorkspace { product_id: a_id },
            ) => {
                assert_eq!(g_id, a_id);
                assert_eq!(g_id, GEMINI_GWS_PRODUCT_ID);
            }
            _ => panic!("Gemini + Antigravity should be GWS feeds"),
        }
    }
}
