//! Canonical Claude.ai endpoint list for the probe. Mirrors the
//! production Web strategy so the probe shows the same view the app
//! sees.

#[derive(Clone, Copy, Debug)]
pub struct Probe {
    pub label: &'static str,
    pub url: &'static str,
}

pub const ENDPOINTS: &[Probe] = &[
    Probe {
        label: "organizations",
        url: "https://claude.ai/api/organizations",
    },
    Probe {
        label: "settings",
        url: "https://claude.ai/api/account",
    },
    Probe {
        label: "subscription",
        url: "https://claude.ai/api/subscription",
    },
];
