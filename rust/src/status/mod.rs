//! Provider status feed subsystem. Polls public vendor status pages
//! (Statuspage.io for OpenAI/Anthropic/Cursor/Factory/GitHub, Google
//! Workspace for Gemini/Antigravity) on the same cadence as usage
//! refreshes and aggregates the result into a tray-icon overlay plus a
//! per-provider status line in the popup.
//!
//! Ported from `Sources/CodexBar/UsageStore+Status.swift` and
//! `Sources/CodexBarCore/Providers/Providers.swift`. The design choices
//! follow `docs/windows/spec/55-status-incidents.md` verbatim:
//!
//! - 6-state severity enum.
//! - Order-based aggregation (not severity-based) so the tray icon
//!   surfaces the first provider with an issue in user-configured order.
//! - In-memory only; the last successful snapshot is sticky on
//!   transient failures.
//! - No auth, no cookies, no platform APIs. Pure HTTPS JSON.

pub mod aggregator;
pub mod feed;
pub mod gws;
pub mod poller;
pub mod registry;
pub mod severity;
pub mod statuspage;
pub mod store;
pub mod transport;

pub use aggregator::{aggregate, AggregationOrder};
pub use feed::{StatusFeed, StatusHttp, StatusResponse, StatusSnapshot};
pub use poller::StatusPoller;
pub use registry::{feed_for_provider, link_only_for_provider};
pub use severity::StatusSeverity;
pub use store::{StatusEvent, StatusStore};
pub use transport::ReqwestStatusClient;
