//! Codebuff provider.

crate::simple_windows_provider! {
    provider_struct: CodebuffProvider,
    provider_id_const: CODEBUFF_ID,
    id: "codebuff",
    display_name: "Codebuff",
    homepage: "https://www.codebuff.com",
    dashboard_url: Some("https://www.codebuff.com/account"),
    status: crate::providers::descriptor::ProviderStatusMetadata::none(),
    accent: "#7C3AED",
    icon: "codebuff",
    session_label: "Credits",
    weekly_label: "Subscription",
    supports_credits: true,
    auth_hint: crate::providers::common_api::AuthHint::Bearer,
    env_vars: &["CODEBUFF_API_KEY"],
    endpoint: crate::providers::common_api::EndpointSpec::Codebuff,
    settings_title: "Codebuff API key",
    settings_help: "Uses CODEBUFF_API_KEY, a pasted token, or the official CLI credentials at %USERPROFILE%\\.config\\manicode\\credentials.json."
}
