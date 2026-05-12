//! Credit balance per spec 30 section 12.5. Providers that expose a
//! credit purse (Codex, Factory, OpenRouter) populate this; everything
//! else returns `None`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CreditsSnapshot {
    /// Current balance in the provider's natural unit.
    pub balance: f64,
    /// Whether `balance` is denominated in credits, tokens, or USD.
    pub unit: CreditUnit,
    /// Recent history. Phase 4 stores 30 days of events for the chart.
    pub recent_events: Vec<CreditEvent>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CreditUnit {
    Credits,
    Tokens,
    UsdCents,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CreditEvent {
    pub timestamp_unix_secs: i64,
    /// Positive for purchases, negative for usage.
    pub delta: f64,
    pub note: Option<String>,
}
