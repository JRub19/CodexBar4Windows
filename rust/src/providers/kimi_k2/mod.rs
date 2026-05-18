//! Kimi K2 provider.

crate::simple_windows_provider! {
    provider_struct: KimiK2Provider,
    provider_id_const: KIMI_K2_ID,
    id: "kimi-k2",
    display_name: "Kimi K2",
    homepage: "https://kimi-k2.ai",
    dashboard_url: Some("https://kimi-k2.ai"),
    status: crate::providers::descriptor::ProviderStatusMetadata::none(),
    accent: "#374151",
    icon: "kimi-k2",
    session_label: "Credits",
    weekly_label: "Balance",
    supports_credits: true,
    auth_hint: crate::providers::common_api::AuthHint::Bearer,
    env_vars: &["KIMI_K2_API_KEY", "KIMI_API_KEY", "KIMI_KEY"],
    endpoint: crate::providers::common_api::EndpointSpec::JsonGet("https://kimi-k2.ai/api/user/credits"),
    settings_title: "Kimi K2 API key",
    settings_help: "Paste a Kimi K2 bearer/API key. Stored DPAPI-wrapped on disk."
}
