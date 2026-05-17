//! Per-provider CLI configuration for discovering binaries across PATH and
//! known install locations (npm global, scoop, winget, etc.).

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ProviderCLIConfig {
    pub binary_name: &'static str,
    /// Arguments passed on every invocation, before user-supplied args.
    pub default_args: &'static [&'static str],
    /// Optional extra search paths checked in addition to PATH. Used by
    /// providers whose CLI is shipped under `%LOCALAPPDATA%` (Claude
    /// installs via npm global, Codex via scoop, etc.).
    pub extra_search_dirs: &'static [&'static str],
    /// Minimum CLI version required for the strategy to run. The
    /// dispatcher refuses to talk to older binaries.
    pub min_version: Option<&'static str>,
}

impl ProviderCLIConfig {
    pub const fn simple(binary_name: &'static str) -> Self {
        Self {
            binary_name,
            default_args: &[],
            extra_search_dirs: &[],
            min_version: None,
        }
    }
}
