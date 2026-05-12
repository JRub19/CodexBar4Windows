//! Codex history ownership. Phase 5 Tasks 6.1 through 6.3 land here.
//!
//! Every history bucket (credits, cost, breakdown, plan utilization)
//! lives behind a `HistoryKey` so a user with two Codex accounts gets
//! two independent buckets. The key combines `(account_id, plan_type,
//! data_kind)` so a plan upgrade does not silently merge buckets that
//! used to belong to different plans.

pub mod key;
pub mod ownership;
