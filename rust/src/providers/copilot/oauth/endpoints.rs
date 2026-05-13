//! Copilot endpoint resolution. Ported from
//! `Sources/CodexBarCore/Providers/Copilot/CopilotUsageFetcher.swift` and
//! `CopilotDeviceFlow.swift` so the GHE host normalisation matches the
//! macOS app exactly.

pub const DEFAULT_HOST: &str = "github.com";
pub const DEFAULT_API_HOST: &str = "api.github.com";

/// Strip protocol/path/whitespace from a user-supplied GHE host.
pub fn normalize_host(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return DEFAULT_HOST.into();
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_HOST.into();
    }
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let without_path = without_scheme.split('/').next().unwrap_or(without_scheme);
    if without_path.is_empty() {
        DEFAULT_HOST.into()
    } else {
        without_path.to_ascii_lowercase()
    }
}

/// Pick the API host for a normalized GHE host. `github.com` →
/// `api.github.com`; enterprise hosts get an `api.` prefix unless they
/// already have one.
pub fn api_host(enterprise_host: Option<&str>) -> String {
    let host = normalize_host(enterprise_host);
    if host == DEFAULT_HOST {
        return DEFAULT_API_HOST.into();
    }
    if host.starts_with("api.") {
        return host;
    }
    format!("api.{host}")
}

pub fn usage_url(enterprise_host: Option<&str>) -> String {
    format!(
        "https://{}/copilot_internal/user",
        api_host(enterprise_host)
    )
}

pub fn user_identity_url() -> &'static str {
    "https://api.github.com/user"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_host_when_unset() {
        assert_eq!(normalize_host(None), "github.com");
        assert_eq!(api_host(None), "api.github.com");
        assert_eq!(
            usage_url(None),
            "https://api.github.com/copilot_internal/user"
        );
    }

    #[test]
    fn normalizes_scheme_and_path() {
        assert_eq!(
            normalize_host(Some("https://corp.example.com/")),
            "corp.example.com"
        );
        assert_eq!(
            api_host(Some("https://corp.example.com/")),
            "api.corp.example.com"
        );
    }

    #[test]
    fn passes_through_api_prefix_when_already_present() {
        assert_eq!(
            api_host(Some("api.corp.example.com")),
            "api.corp.example.com"
        );
    }

    #[test]
    fn whitespace_only_falls_back_to_default() {
        assert_eq!(normalize_host(Some("   ")), "github.com");
        assert_eq!(normalize_host(Some("")), "github.com");
    }
}
