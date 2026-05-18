//! MiniMax provider.

crate::simple_windows_provider! {
    provider_struct: MiniMaxProvider,
    provider_id_const: MINIMAX_ID,
    id: "minimax",
    display_name: "MiniMax",
    homepage: "https://www.minimax.io",
    dashboard_url: Some("https://www.minimax.io/platform/user-center/basic-information"),
    status: crate::providers::descriptor::ProviderStatusMetadata::none(),
    accent: "#2454FF",
    icon: "minimax",
    session_label: "Credits",
    weekly_label: "Plan",
    supports_credits: true,
    auth_hint: crate::providers::common_api::AuthHint::Cookie,
    env_vars: &["MINIMAX_COOKIE", "MINIMAX_COOKIE_HEADER", "MINIMAX_API_KEY", "MINIMAX_CODING_API_KEY"],
    endpoint: crate::providers::common_api::EndpointSpec::JsonGet("https://www.minimax.io/api/user/remains"),
    settings_title: "MiniMax cookie or API key",
    settings_help: "Auto-import or paste a MiniMax browser cookie/API token. Chrome/Edge imports can fail when App-Bound Encryption blocks cookie decryption."
}
