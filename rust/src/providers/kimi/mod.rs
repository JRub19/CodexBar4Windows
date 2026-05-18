//! Kimi provider.

crate::simple_windows_provider! {
    provider_struct: KimiProvider,
    provider_id_const: KIMI_ID,
    id: "kimi",
    display_name: "Kimi",
    homepage: "https://www.kimi.com",
    dashboard_url: Some("https://www.kimi.com"),
    status: crate::providers::descriptor::ProviderStatusMetadata::none(),
    accent: "#111827",
    icon: "kimi",
    session_label: "Window",
    weekly_label: "Quota",
    supports_credits: true,
    auth_hint: crate::providers::common_api::AuthHint::Cookie,
    env_vars: &["KIMI_AUTH_TOKEN", "KIMI_COOKIE", "KIMI_COOKIE_HEADER"],
    endpoint: crate::providers::common_api::EndpointSpec::JsonPost {
        url: "https://www.kimi.com/apiv2/kimi.gateway.billing.v1.BillingService/GetUsages",
        body: "{}"
    },
    settings_title: "Kimi auth token or cookie",
    settings_help: "Auto-import or paste kimi-auth/session credentials. Chrome/Edge imports can fail when App-Bound Encryption blocks cookie decryption."
}
