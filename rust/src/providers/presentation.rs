//! Computed view model for the popup card. `ProviderImplementation`
//! folds the latest `UsageSnapshot` into a `ProviderPresentation` so the
//! React layer can render without re-deriving paces or reset strings.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct ProviderPresentation {
    pub display_name: String,
    pub plan_name: Option<String>,
    pub email: Option<String>,
    pub subtitle: Option<String>,
    pub metrics: Vec<PresentationMetric>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PresentationMetric {
    pub title: String,
    pub percent: Option<f32>,
    pub detail_left: Option<String>,
    pub detail_right: Option<String>,
    pub reset_text: Option<String>,
}

impl ProviderPresentation {
    pub fn empty(display_name: impl Into<String>) -> Self {
        Self {
            display_name: display_name.into(),
            plan_name: None,
            email: None,
            subtitle: Some("Awaiting first refresh".into()),
            metrics: Vec::new(),
        }
    }
}
