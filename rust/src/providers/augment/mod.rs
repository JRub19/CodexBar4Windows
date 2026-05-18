//! Augment provider.

crate::simple_windows_provider! {
    provider_struct: AugmentProvider,
    provider_id_const: AUGMENT_ID,
    id: "augment",
    display_name: "Augment",
    homepage: "https://www.augmentcode.com",
    dashboard_url: Some("https://app.augmentcode.com"),
    status: crate::providers::descriptor::ProviderStatusMetadata::none(),
    accent: "#16A34A",
    icon: "augment",
    session_label: "Credits",
    weekly_label: "Cycle",
    supports_credits: true,
    auth_hint: crate::providers::common_api::AuthHint::None,
    env_vars: &["AUGMENT_COOKIE", "AUGMENT_COOKIE_HEADER"],
    endpoint: crate::providers::common_api::EndpointSpec::AugmentCli,
    settings_title: "Augment CLI or cookie",
    settings_help: "Uses `auggie account status` first. Cookie fallback remains available through token accounts for a later web strategy."
}
