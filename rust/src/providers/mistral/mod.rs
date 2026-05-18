//! Mistral provider.

crate::simple_windows_provider! {
    provider_struct: MistralProvider,
    provider_id_const: MISTRAL_ID,
    id: "mistral",
    display_name: "Mistral",
    homepage: "https://mistral.ai",
    dashboard_url: Some("https://admin.mistral.ai/usage"),
    status: crate::providers::descriptor::ProviderStatusMetadata::link("https://status.mistral.ai"),
    accent: "#FA520F",
    icon: "mistral",
    session_label: "Spend",
    weekly_label: "Usage",
    supports_credits: true,
    auth_hint: crate::providers::common_api::AuthHint::Cookie,
    env_vars: &["MISTRAL_COOKIE", "MISTRAL_COOKIE_HEADER"],
    endpoint: crate::providers::common_api::EndpointSpec::JsonGet("https://admin.mistral.ai/api/billing/v2/usage"),
    settings_title: "Mistral browser session",
    settings_help: "Auto-import or paste the Mistral admin cookie header. Chrome/Edge imports can fail when App-Bound Encryption blocks cookie decryption."
}
