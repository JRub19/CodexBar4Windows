//! Manus provider.

crate::simple_windows_provider! {
    provider_struct: ManusProvider,
    provider_id_const: MANUS_ID,
    id: "manus",
    display_name: "Manus",
    homepage: "https://manus.im",
    dashboard_url: Some("https://manus.im"),
    status: crate::providers::descriptor::ProviderStatusMetadata::none(),
    accent: "#0F766E",
    icon: "manus",
    session_label: "Monthly",
    weekly_label: "Daily",
    supports_credits: true,
    auth_hint: crate::providers::common_api::AuthHint::Cookie,
    env_vars: &["MANUS_SESSION_TOKEN", "MANUS_COOKIE"],
    endpoint: crate::providers::common_api::EndpointSpec::JsonPost {
        url: "https://api.manus.im/user.v1.UserService/GetAvailableCredits",
        body: "{}"
    },
    settings_title: "Manus session token or cookie",
    settings_help: "Auto-import or paste a Manus session token/cookie. Chrome/Edge imports can fail when App-Bound Encryption blocks cookie decryption."
}
