---
phase: 4
title: "Provider framework plus first real provider (Claude) end to end"
status: "ready to execute"
depends_on:
  - phase-0-bootstrap
  - phase-1-tray-icon
  - phase-2-cache-and-secret-store
  - phase-3-refresh-loop-mock-data
unblocks:
  - phase-5-codex
  - phase-6-tier1-cohort
spec_refs:
  - docs/windows/spec/30-provider-system-architecture.md
  - docs/windows/spec/40-provider-claude.md
  - docs/windows/spec/50-refresh-state-pace.md
  - docs/windows/spec/60-auth-cookies-secrets.md
---

# Phase 4 plan, provider framework plus Claude end to end

## 1. Why this phase exists

Phase 3 left the app with a working tray icon, a working popup, a working refresh tick, and a registry that is empty. Mock data is rendered behind the scenes. The user sees bars, the bars do not move with reality.

Phase 4 closes that gap for the most important provider on the platform, Claude. The work splits cleanly into two halves. Half one is the provider framework, the shared contract every future provider will plug into. Half two is the first real implementation, the Claude provider end to end, with all three runtime data paths wired up, the watchdog binary, the web probe binary, and the settings pane needed to give the user manual overrides.

After this phase the tray icon shows live Claude session and weekly bars on every refresh tick. The popup card shows the real account email, real reset timers, and real pace text derived from Anthropic data. Switching the Source picker between Auto, OAuth, Web, and CLI changes which path was used for the most recent fetch and the change shows up in a debug source label visible inside Settings.

The provider framework is the load bearing piece. Everything in Phase 5 and Phase 6 is just one more provider folder. If the framework is wrong, every future provider will pay for it. We pay the cost once, in this phase, against a real fully featured provider with three runtime paths, so the framework actually has to flex in all directions before any other code touches it.

## 2. Dependencies (concrete contracts inherited)

Phase 4 assumes the following are already merged and stable.

From Phase 0, repository scaffolding.

- Cargo workspace at `rust/`, three binary crates declared, `codexbar4windows`, `codexbar4windows-claude-watchdog`, `codexbar4windows-claude-webprobe`, plus the `codexbar` shared library crate.
- Tauri 2 desktop shell at `apps/desktop-tauri/`, with React, Vite, TypeScript, `specta` plus `ts-rs` bridge wired in.
- `inventory = "0.3"` dependency available in the shared crate.

From Phase 1, tray icon.

- A `TrayController` that consumes `UsageStore` and rerenders the icon when the store changes. The icon currently renders against mock data.
- Per monitor DPI awareness V2 already opted into in the manifest.

From Phase 2, cache and secret store.

- `SecretStore` trait backed by Windows Credential Manager, with helpers `read("codexbar4windows", target)` and `write` plus `delete`.
- `DpapiBlobStore` for file backed DPAPI wrapped JSON at `%LOCALAPPDATA%\CodexBar4Windows\cache\<name>.bin`.
- `cookie-headers.json` cache file path already reserved at `%LOCALAPPDATA%\CodexBar4Windows\cache\cookie-headers.json` per spec 60.
- `HttpClient` factory returning a `reqwest::Client` pre configured with TLS, redirect policy off, and a 30 second default timeout.

From Phase 3, refresh loop and mock data.

- `UsageStore` exists as a single tokio mutex protected struct, with a `replace_snapshot(provider_id, snapshot, attempts)` writer. The writer currently accepts any snapshot, identity siloing is not yet enforced.
- A `RefreshScheduler` ticks at the configured cadence (default five minutes), single flight guarded, fans out one task per enabled provider, then publishes a `usage:updated` Tauri event.
- The provider registry is a placeholder, `static REGISTRY: Lazy<Vec<()>>`. Phase 4 replaces this with the real `inventory!` powered catalog.
- A `SettingsStore` trait exists with typed `SettingsKey<T>` accessors, backed by `%APPDATA%\CodexBar4Windows\config.json`.

If any of these inherited surfaces drifted, fix forward in Phase 4 commits and note the deviation in the relevant commit message.

## 3. Deliverables in this phase

### 3.0 Module layout target

By the end of this phase the following Rust source tree exists under `rust/src/providers/`. Empty files are placeholders for Phase 5 and beyond.

```
rust/src/providers/
  mod.rs                        // re exports plus registry bootstrap
  registry.rs                   // inventory! driven catalog plus ProviderCatalog::build
  descriptor.rs                 // ProviderDescriptor, ProviderMetadata, sub structs
  branding.rs                   // ProviderBranding, ProviderColor, IconStyle enum
  cli_config.rs                 // ProviderCLIConfig
  cookie_source.rs              // CookieSource enum, BrowserCookieImportOrder
  fetch_plan.rs                 // ProviderFetchPlan, ProviderFetchPipeline, Strategy trait
  fetch_context.rs              // ProviderFetchContext, Runtime enum, SourceMode enum
  fetch_outcome.rs              // ProviderFetchOutcome, ProviderFetchAttempt
  implementation.rs             // ProviderImplementation async trait
  contexts.rs                   // smaller ProviderXxxContext structs per hook
  presentation.rs               // ProviderPresentation, detail line helper
  settings_descriptor.rs        // Toggle, Field, Picker, ActionsRow, TokenAccounts
  settings_snapshot.rs          // ProviderSettingsSnapshot plus Builder plus Contribution
  candidate_retry.rs            // ProviderCandidateRetryRunner
  errors.rs                     // ProviderError plus ProviderFetchError
  identity.rs                   // ProviderIdentitySnapshot plus scoped helper
  models/
    rate_window.rs              // RateWindow, NamedRateWindow
    usage_snapshot.rs           // UsageSnapshot
    credits.rs                  // CreditsSnapshot, CreditEvent
    provider_cost.rs            // ProviderCostSnapshot
    storage_footprint.rs        // ProviderStorageFootprint
  hello/                        // sample provider for tests
    mod.rs
    descriptor.rs
    strategies.rs
    ui.rs
  claude/                       // first real provider
    mod.rs
    descriptor.rs
    ui.rs
    routing.rs
    tokens.rs
    planner.rs
    settings.rs
    errors.rs
    models.rs
    oauth/
      credentials.rs
      cache.rs
      response.rs
      strategy.rs
    web/
      strategy.rs
      endpoints.rs
      cookie_cache.rs
      org_selection.rs
    cli/
      strategy.rs
      pty_actor.rs
      auto_responder.rs
      parser.rs
      reset_parser.rs
```

The TypeScript side mirrors the descriptor only side. `apps/desktop-tauri/src/providers/shared/` holds the generic card and panel, `apps/desktop-tauri/src/providers/claude/index.tsx` adds bespoke UI when the descriptor cannot drive a particular surface.

### 3.1 Framework deliverables

F1. `ProviderDescriptor` struct, composed of the five sub structs called out in spec 30, with every field from the canonical field table populated or defaulted. The thirty mandatory fields per spec 30 are present and serializable through `specta` for the TypeScript bridge. The canonical Rust shape is.

```rust
pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub metadata: ProviderMetadata,
    pub branding: ProviderBranding,
    pub token_cost: ProviderTokenCostConfig,
    pub fetch_plan: ProviderFetchPlan,
    pub cli: ProviderCLIConfig,
}

pub struct ProviderMetadata {
    pub id: ProviderId,
    pub display_name: &'static str,
    pub session_label: &'static str,
    pub weekly_label: &'static str,
    pub opus_label: Option<&'static str>,
    pub supports_opus: bool,
    pub supports_credits: bool,
    pub credits_hint: &'static str,
    pub toggle_title: &'static str,
    pub cli_name: &'static str,
    pub default_enabled: bool,
    pub is_primary_provider: bool,
    pub uses_account_fallback: bool,
    pub browser_cookie_order: Option<BrowserCookieImportOrder>,
    pub dashboard_url: Option<&'static str>,
    pub subscription_dashboard_url: Option<&'static str>,
    pub status_page_url: Option<&'static str>,
    pub status_link_url: Option<&'static str>,
    pub status_workspace_product_id: Option<&'static str>,
}
```

F2. `ProviderImplementation` async trait with the eighteen hooks listed in spec 30 section 17. Default impls match the spec defaults. The trait is `Send + Sync` and object safe. Sketch.

```rust
#[async_trait]
pub trait ProviderImplementation: Send + Sync {
    fn id(&self) -> ProviderId;
    fn supports_login_flow(&self) -> bool { false }
    fn presentation(&self, ctx: &ProviderPresentationContext) -> ProviderPresentation;
    fn observe_settings(&self, settings: &dyn SettingsStore);
    fn is_available(&self, ctx: &ProviderAvailabilityContext) -> bool { true }
    fn default_source_label(&self, ctx: &ProviderSourceLabelContext) -> Option<String> { None }
    fn decorate_source_label(&self, ctx: &ProviderSourceLabelContext, base: &str) -> String { base.to_string() }
    fn source_mode(&self, ctx: &ProviderSourceModeContext) -> SourceMode { SourceMode::Auto }
    async fn detect_version(&self, ctx: &ProviderVersionContext) -> Option<String>;
    fn make_runtime(&self) -> Option<Arc<dyn ProviderRuntime>> { None }
    fn settings_toggles(&self, ctx: &SettingsContext) -> Vec<ToggleDescriptor> { vec![] }
    fn settings_fields(&self, ctx: &SettingsContext) -> Vec<FieldDescriptor> { vec![] }
    fn settings_actions(&self, ctx: &SettingsContext) -> Vec<ActionsRowDescriptor> { vec![] }
    fn settings_pickers(&self, ctx: &SettingsContext) -> Vec<PickerDescriptor> { vec![] }
    fn token_accounts_visibility(&self, ctx: &SettingsContext, support: &TokenAccountSupport) -> bool;
    fn settings_snapshot(&self, ctx: &SettingsSnapshotContext) -> Option<ProviderSettingsSnapshotContribution> { None }
    fn apply_token_account_cookie_source(&self, settings: &dyn SettingsStore) {}
    fn append_usage_menu_entries(&self, ctx: &MenuUsageContext, entries: &mut Vec<MenuEntry>) {}
    fn append_action_menu_entries(&self, ctx: &MenuActionContext, entries: &mut Vec<MenuEntry>) {}
    fn login_menu_action(&self, ctx: &MenuLoginContext) -> Option<(String, MenuAction)> { None }
    async fn run_login_flow(&self, ctx: &LoginContext) -> bool { false }
}
```

F3. `Strategy` async trait with `id`, `kind`, `is_available`, `fetch`, and `should_fallback`. The trait is object safe. Sketch.

```rust
#[async_trait]
pub trait Strategy: Send + Sync {
    fn id(&self) -> &'static str;
    fn kind(&self) -> FetchKind;
    async fn is_available(&self, ctx: &ProviderFetchContext<'_>) -> bool;
    async fn fetch(&self, ctx: &ProviderFetchContext<'_>) -> Result<ProviderFetchResult, ProviderError>;
    fn should_fallback(&self, err: &ProviderError, ctx: &ProviderFetchContext<'_>) -> bool {
        matches!(err,
            ProviderError::Timeout
            | ProviderError::Network(_)
            | ProviderError::NoCookies { .. }
            | ProviderError::NoToken { .. }
            | ProviderError::PluginUnavailable { .. })
    }
}

pub enum FetchKind { Cli, Web, OAuth, ApiToken, LocalProbe, WebDashboard }
```

F4. `inventory!` based registry. A single `ProviderCatalog::build()` materializes the registry at startup, validates uniqueness of `ProviderId`, validates icon resource references, panics with a clear message on any registration defect.

F5. `ProviderFetchPlan` runtime, an `async fn run(ctx) -> ProviderFetchOutcome` that resolves the candidate ordering by source mode, wraps each strategy invocation in `tokio::time::timeout(Duration::from_secs(45))`, records every attempt into the outcome regardless of result, and respects per strategy `should_fallback` decisions. Pseudocode mirroring spec 30 section 5.1.

```text
fn run(plan, ctx) -> ProviderFetchOutcome:
    strategies = plan.pipeline.resolve(ctx)
    attempts = []
    last_err = None
    for s in strategies:
        if not s.is_available(ctx).await:
            attempts.push(Attempt{ id, kind, available: false, error: None })
            continue
        let outcome = tokio::time::timeout(45s, s.fetch(ctx)).await
        match outcome:
            Ok(Ok(result)):
                attempts.push(Attempt{ id, kind, available: true, error: None })
                return Outcome{ result: Ok(result), attempts }
            Ok(Err(e)):
                attempts.push(Attempt{ id, kind, available: true, error: Some(e.clone()) })
                last_err = Some(e)
                if s.should_fallback(&e, ctx): continue
                else: return Outcome{ result: Err(e), attempts }
            Err(_timeout):
                let e = ProviderError::Timeout
                attempts.push(Attempt{ id, kind, available: true, error: Some(e.clone()) })
                last_err = Some(e)
                if s.should_fallback(&e, ctx): continue
                else: return Outcome{ result: Err(e), attempts }
    return Outcome{ result: Err(last_err.unwrap_or(NoAvailableStrategy(id))), attempts }
```

F6. `ProviderCandidateRetryRunner`, the within strategy retry helper at `rust/src/providers/candidate_retry.rs`, signature exactly as spelled in spec 30 section 5.3.

F7. `ProviderRuntime` context, the long lived helper trait, plus a `RuntimeContext` injection bundle containing `Arc<dyn HttpClient>`, `Arc<dyn SecretStore>`, `Arc<dyn CookieApi>`, `Arc<dyn SettingsStore>`, `tracing::Span` logger, `Arc<RefreshScheduler>` handle, plus a `RuntimeServices` view exposing `pty: Arc<dyn PtyHost>` for providers that need a ConPTY.

F8. Result models, `RateWindow`, `NamedRateWindow`, `UsageSnapshot`, `CreditsSnapshot`, `ProviderCostSnapshot`, `ProviderFetchResult`, `StatusSnapshot`, `ProviderStorageFootprint`, plus `ProviderIdentitySnapshot`. Identity siloing is enforced in `UsageStore::replace_snapshot`, mismatched provider ids panic in debug and log plus skip in release. Canonical Rust shapes.

```rust
pub struct RateWindow {
    pub used_percent: f64,
    pub window_minutes: Option<u32>,
    pub resets_at: Option<DateTime<Utc>>,
    pub reset_description: Option<String>,
    pub next_regen_percent: Option<f64>,
}

pub struct UsageSnapshot {
    pub primary: Option<RateWindow>,
    pub secondary: Option<RateWindow>,
    pub tertiary: Option<RateWindow>,
    pub extra_rate_windows: Option<Vec<NamedRateWindow>>,
    pub provider_cost: Option<ProviderCostSnapshot>,
    pub updated_at: DateTime<Utc>,
    pub identity: Option<ProviderIdentitySnapshot>,
}

pub struct ProviderFetchResult {
    pub usage: UsageSnapshot,
    pub credits: Option<CreditsSnapshot>,
    pub dashboard: Option<OpenAIDashboardSnapshot>,
    pub source_label: String,
    pub strategy_id: String,
    pub strategy_kind: FetchKind,
}

pub struct ProviderIdentitySnapshot {
    pub provider_id: ProviderId,
    pub account_email: Option<String>,
    pub account_organization: Option<String>,
    pub login_method: Option<String>,
}
```

F9. Typed `ProviderError` and `ProviderFetchError` per spec 30 section 13.1, including the variants `Timeout`, `Network`, `Unauthorized`, `PermissionDenied`, `NoCookies`, `NoToken`, `ParseError`, `UpstreamError`, `PluginUnavailable`, `UserConfigInvalid`, `Cancelled`. Retry decisions encoded as default trait methods on `Strategy`.

F10. Manual provider registration sample, a `HelloProvider` under `rust/src/providers/hello/`, default disabled, used in unit tests and integration tests to exercise the framework without touching real network or PTY paths.

F11. Tauri commands for the new boundary, `provider_descriptors`, `provider_snapshots`, `provider_refresh`, `provider_login`, plus the existing `usage:updated` event now carrying real attempts. DTOs codegenned to TypeScript via `specta`.

### 3.2 Claude provider deliverables

The Claude implementation is the load test for the framework. Every flex point in F1 through F11 is exercised here. The deliverables below are ordered as they ship through commits P4 10 through P4 20.

C1. `ClaudeProviderDescriptor` wired into the `inventory!` registry. Brand color RGB 204 124 94, icon resource name `ProviderIcon-claude`, display name Claude, session label Session, weekly label Weekly, opus label Sonnet (legacy name preserved per spec 40), `supports_opus = true`, `supports_credits = false`, `is_primary_provider = true`, dashboard URL `https://console.anthropic.com/settings/billing`, subscription dashboard URL `https://claude.ai/settings/usage`, status page URL `https://status.claude.com/`, source modes flagging Auto, OAuth, Web, Cli.

C2. OAuth strategy, `ClaudeOAuthStrategy`. Discovers credentials in priority order, env var `CODEXBAR_CLAUDE_OAUTH_TOKEN`, in memory thirty minute cache, DPAPI wrapped cache at `%LOCALAPPDATA%\CodexBar4Windows\cache\claude-oauth.bin`, then `%USERPROFILE%\.claude\.credentials.json`. Credential Manager fallback under target `claude-credentials` is wired but only used when the file path is absent, this mirrors how the CLI on some installs stores there. Reads the file, parses the `claudeAiOauth` root key, validates the scopes array contains `user:profile`, refuses with a clear error when the token only has `user:inference`.

C3. OAuth fetch path. `GET https://api.anthropic.com/api/oauth/usage` with headers `Authorization: Bearer <accessToken>`, `Accept: application/json`, `Content-Type: application/json`, `anthropic-beta: oauth-2025-04-20`, `User-Agent: claude-code/<detected-version>` falling back to `claude-code/2.1.0`. Thirty second timeout, wrapped by the forty five second strategy timeout from F5.

C4. OAuth response mapping per spec 40 section 2.6. Maps `five_hour` to `primary` (session, window 300 minutes), `seven_day` to `secondary` (weekly, window 10080 minutes), `seven_day_sonnet` to `tertiary` falling back to `seven_day_opus` when sonnet is absent, `extra_usage` to `ProviderCostSnapshot` with cents divided by one hundred. Plan inference reads `subscriptionType` first then `rate_limit_tier`, case insensitive substring match for Max, Pro, Team, Enterprise, Ultra.

C5. OAuth refresh handling. Reads `expiresAt` in milliseconds since epoch. If the token expires within sixty seconds, surface a typed `NeedsReauth` error that the strategy reports as `Unauthorized`, the planner falls through to CLI or Web in Auto mode, otherwise surfaces directly. Direct refresh against `https://platform.claude.com/v1/oauth/token` is gated behind the credential owner tag and only fires when the owner is `codexbar4windows` (we minted them). Delegated refresh against the CLI is deferred to a follow up commit in Phase 4 since it requires the PTY runtime; ship the OAuth strategy first with a clean re auth error, add the delegated refresh in C13.

C6. Web strategy, `ClaudeWebStrategy`. Cookie source order is Edge, Chrome, Brave, Vivaldi, Arc, Opera, Chromium, Firefox. Reads cookies through the Phase 2 `CookieApi`. Required cookie name is `sessionKey` starting with `sk-ant-`. Falls back to manual paste from `SettingsKey::ClaudeManualCookieHeader`.

C7. Cookie header cache at `%LOCALAPPDATA%\CodexBar4Windows\cache\cookie-headers.json`, DPAPI wrapped per Phase 2. Entry key `cookie.claude`. On success the strategy writes back the header plus the source label, for example `Chrome Profile 1`. On 401 or 403 the cache entry is cleared, on other errors the cache is preserved.

C8. Web API endpoints called in order. `GET https://claude.ai/api/organizations` to discover the org UUID, `GET https://claude.ai/api/organizations/{orgId}/usage` for the rate windows, `GET https://claude.ai/api/organizations/{orgId}/overage_spend_limit` for spend limit (best effort), `GET https://claude.ai/api/account` for email and plan billing fields (best effort). All four sent with header `Cookie: sessionKey=<value>`, `Accept: application/json`, fifteen second timeout each.

C9. Multi account routing. `ClaudeCredentialRouting` classifies each token account in `tokenAccounts` by prefix. Tokens beginning with `sk-ant-oat` route as OAuth bearers, env override `CODEXBAR_CLAUDE_OAUTH_TOKEN`. Bare values without `=` or `cookie:` are wrapped as `sessionKey=<value>` and forced into manual cookie source. Full headers containing `=` or `cookie:` are normalized as a manual header. The routing rule is surfaced as a static helper plus a settings hint string visible under the token accounts section.

C10. CLI PTY strategy, `ClaudeCliStrategy`. Uses `portable-pty` to launch `claude` with arguments `--allowed-tools ""`, fifty rows by one hundred sixty columns, working directory `%APPDATA%\CodexBar4Windows\ClaudeProbe` auto created. Env scrubbing strips `CODEXBAR_CLAUDE_OAUTH_TOKEN`, `CODEXBAR_CLAUDE_OAUTH_SCOPES`, and all `ANTHROPIC_` prefixed keys. Auto responder handles the trust prompt, the quick safety check, the workspace ready prompt, the press enter to continue prompt, and the CPR escape sequence `ESC[6n` answered with `ESC[1;1R`.

C11. CLI parser. ANSI strip via a compiled regex, trim to the last `Settings:` occurrence, label match the three windows by their alphanumeric only collapsed form, extract percent then determine direction (used vs left vs remaining) from adjacent words, extract reset string from the same twelve line lookahead window, skip the status bar lines containing `|` plus a model token. Optional `/status` round trip parsed for email, org, login method using the regex catalog from spec 40 section 4.7.

C12. Watchdog binary, `codexbar4windows-claude-watchdog`. Implements the Win32 Job Object dance, `CreateJobObject` plus `SetInformationJobObject` with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, `CreateProcess` suspended, `AssignProcessToJobObject`, `ResumeThread`. Polls the parent handle every two hundred milliseconds, closes the job on parent death, kernel takes the child down. Disable via env `CODEXBAR_DISABLE_CLAUDE_WATCHDOG=1`. Installed alongside the main binary in the release package under `helpers\codexbar4windows-claude-watchdog.exe`.

C13. Web probe binary, `codexbar4windows-claude-webprobe`. CLI utility that reads cookies through the same `CookieApi` as the main app, calls the default endpoint list from spec 40 section 6.2, prints status, content type, top level JSON keys, plus email and plan hints. Honors env `CLAUDE_WEB_PROBE_PREVIEW=1`. Truncates response bodies to two hundred kilobytes. Used for diagnostic.

C14. Source selection. Single `claude_resolve_plan` function takes runtime (App or Cli), selected source (Auto, OAuth, Web, Cli), plus the three availability flags. Returns the ordered strategy list. Auto in App returns OAuth then CLI then Web. Auto in Cli returns Web then CLI. Explicit picks produce single element plans and propagate the concrete error without fallback. The dual `.auto` sites called out in spec 40 section 1.5 are consolidated into this single function.

C15. Tray and popup integration. The tray icon renders Claude session and weekly bars from the live `UsageSnapshot` on every refresh tick. The popup card shows the Claude brand row, account email pulled from `identity.account_email`, the session bar with reset countdown, the weekly bar with reset countdown, the model specific weekly line labeled Sonnet by default (per spec 40 the legacy label is preserved), pace text, and the reset text formatted per user preference. Per preference the reset shows as a relative countdown like `Resets in 3h 12m` or an absolute clock time.

C16. Settings pane, basic. A new Claude section is rendered through the `SettingsDescriptor` machinery. Contains a Picker for Source (Auto, OAuth, Web, CLI), a Field for the manual cookie header, an Action labeled `Open Claude credentials file` opening `%USERPROFILE%\.claude\.credentials.json` in the default editor, plus a status row showing the most recent fetch source label.

Cost usage scanning is explicitly out of scope for this phase, it lands in Phase 7 per spec 70.

## 4. Atomic commit plan

Each entry is one commit, conventional commit format, no em dashes, push after each commit per branch policy. Files listed are illustrative and may grow during implementation, the constraint is that each commit must compile, pass `cargo test`, and pass `pnpm test`.

### 4.1 Commit P4 01, framework, descriptor and registry skeleton

Title. `feat(providers): introduce ProviderDescriptor, ProviderId, inventory registry`

Files.
- `rust/src/providers/mod.rs`
- `rust/src/providers/descriptor.rs`
- `rust/src/providers/branding.rs`
- `rust/src/providers/cli_config.rs`
- `rust/src/providers/cookie_source.rs`
- `rust/src/providers/registry.rs`
- `rust/src/providers/identity.rs`
- `rust/src/lib.rs` (re export the module)

Acceptance.
- `cargo test -p codexbar providers::registry::tests::registry_is_empty_at_startup` passes.
- `cargo build --release` succeeds.

Draft commit message body. `Adds the canonical ProviderDescriptor, the nested ProviderMetadata, ProviderBranding, ProviderCLIConfig and ProviderTokenCostConfig structs, plus the empty inventory powered registry. No providers are registered yet, the catalog asserts a zero count at startup. Sets up the surface every later commit will plug into.`

### 4.2 Commit P4 02, framework, result models

Title. `feat(providers): add canonical result models (RateWindow, UsageSnapshot, ProviderFetchResult)`

Files.
- `rust/src/providers/models/rate_window.rs`
- `rust/src/providers/models/usage_snapshot.rs`
- `rust/src/providers/models/credits.rs`
- `rust/src/providers/models/provider_cost.rs`
- `rust/src/providers/models/storage_footprint.rs`
- `rust/src/providers/fetch_outcome.rs`

Acceptance.
- `RateWindow::remaining_percent` is tested for clamping at zero.
- `RateWindow::backfilling_reset_time` is tested for the cached future reset case.
- `UsageSnapshot` round trips through serde JSON.

Draft commit message body. `Adds the immutable Sendable equivalents in Rust, every field from spec 30 section 12. Defines NamedRateWindow, ProviderFetchResult, ProviderFetchOutcome. Each model derives Clone, Serialize, Deserialize, Debug. No behavior wired up yet, this commit is types only.`

### 4.3 Commit P4 03, framework, error taxonomy

Title. `feat(providers): typed ProviderError and ProviderFetchError with retry decisions`

Files.
- `rust/src/providers/errors.rs`
- `rust/src/providers/fetch_outcome.rs` (extend with error fold)

Acceptance.
- Unit tests verify each variant maps to the expected default `should_fallback` answer per spec 30 section 13.3.

Draft commit message body. `Adds the typed error enums and the default retry decision matrix. Timeout, Network, NoCookies, NoToken, PluginUnavailable default to true, Unauthorized, PermissionDenied, ParseError, UserConfigInvalid default to false. Strategy implementations may override via should_fallback.`

### 4.4 Commit P4 04, framework, Strategy trait plus ProviderFetchPlan

Title. `feat(providers): Strategy trait and ProviderFetchPlan with 45s per strategy timeout`

Files.
- `rust/src/providers/fetch_plan.rs`
- `rust/src/providers/fetch_context.rs`
- `rust/src/providers/candidate_retry.rs`

Acceptance.
- Tests cover the four pipeline paths from spec 30 section 5.1, strategy returns Ok, strategy returns Err with fallback, strategy returns Err without fallback, every strategy unavailable.
- A test forces a strategy to sleep fifty seconds, the pipeline returns `ProviderError::Timeout` within one second after the forty five second budget.

Draft commit message body. `Adds the Strategy async trait plus ProviderFetchPlan runtime. Every strategy invocation is wrapped in tokio::time::timeout(Duration::from_secs(45)). Adds the run_candidates helper from spec 30 section 5.3. The plan records every attempt for verbose debug output and the future debug pane.`

### 4.5 Commit P4 05, framework, ProviderImplementation trait

Title. `feat(providers): ProviderImplementation async trait with eighteen hooks`

Files.
- `rust/src/providers/implementation.rs`
- `rust/src/providers/contexts.rs`
- `rust/src/providers/presentation.rs`

Acceptance.
- A no op default `ProviderImplementation` impl compiles and is exercised by the framework unit tests.
- Trait is object safe, `Box<dyn ProviderImplementation>` compiles.

Draft commit message body. `Adds the lifecycle trait with every hook from spec 30 section 17, plus the smaller context structs ProviderPresentationContext, ProviderAvailabilityContext, ProviderSourceLabelContext, ProviderSourceModeContext, ProviderVersionContext. Default implementations mirror the Mac defaults.`

### 4.6 Commit P4 06, framework, settings descriptors

Title. `feat(providers): SettingsDescriptor enum and SettingsSnapshot builder`

Files.
- `rust/src/providers/settings_descriptor.rs`
- `rust/src/providers/settings_snapshot.rs`
- `apps/desktop-tauri/src/providers/shared/types.ts` (codegen)
- `apps/desktop-tauri/src/providers/shared/ProviderSettingsPanel.tsx`

Acceptance.
- A unit test confirms that a Picker descriptor with a known options list round trips through `specta` to a TypeScript discriminated union.
- The `ProviderSettingsPanel` component renders Toggle, Field, Picker, ActionsRow and TokenAccounts variants. Storybook stories exist for each.

Draft commit message body. `Adds the descriptor enum, every variant from spec 30 section 9.1, plus the snapshot builder that folds per provider contributions. ProviderSettingsPanel renders descriptors generically with no provider specific React code.`

### 4.7 Commit P4 07, framework, identity siloing in UsageStore

Title. `fix(usage-store): enforce provider identity siloing on writes`

Files.
- `rust/src/usage_store.rs`
- `rust/src/providers/identity.rs`

Acceptance.
- `cargo test usage_store::tests::rejects_cross_provider_identity` passes.
- Writing a snapshot with a mismatched `identity.provider_id` panics in debug and logs plus drops in release.

Draft commit message body. `Closes the documented but unenforced invariant from spec 30 section 2.2. UsageStore::replace_snapshot now scopes the identity through the matching ProviderId before storing.`

### 4.8 Commit P4 08, framework, registry validation plus Hello sample provider

Title. `feat(providers): ProviderCatalog::build validation and Hello sample provider`

Files.
- `rust/src/providers/registry.rs`
- `rust/src/providers/hello/mod.rs`
- `rust/src/providers/hello/descriptor.rs`
- `rust/src/providers/hello/strategies.rs`
- `rust/src/providers/hello/ui.rs`

Acceptance.
- `cargo test providers::hello::tests::hello_returns_static_snapshot` passes.
- Catalog validation rejects a duplicate id at startup with a clear panic message.
- The hello provider appears in `provider_descriptors` invoke output during a manual smoke run.

Draft commit message body. `Materializes the inventory registry into ProviderCatalog at startup with full validation, then adds a minimal Hello provider that returns a deterministic snapshot. Hello is default disabled and gated behind a debug feature flag.`

### 4.9 Commit P4 09, framework, Tauri commands and event payloads

Title. `feat(ipc): provider_descriptors, provider_snapshots, provider_refresh commands`

Files.
- `apps/desktop-tauri/src-tauri/src/commands/providers.rs`
- `apps/desktop-tauri/src/providers/index.ts`
- `apps/desktop-tauri/src/providers/shared/ProviderCard.tsx`

Acceptance.
- `pnpm test` covers the IPC contract via a Tauri mock.
- Manual smoke, the popup mount calls `provider_descriptors` and receives the Hello descriptor.

Draft commit message body. `Adds the IPC boundary commands and the usage:updated event carrying real attempts. DTOs codegenned to TypeScript via specta, hand rolling the TS types is forbidden by a lint rule.`

### 4.10 Commit P4 10, Claude, descriptor only

Title. `feat(claude): register ClaudeProviderDescriptor in the inventory catalog`

Files.
- `rust/src/providers/claude/mod.rs`
- `rust/src/providers/claude/descriptor.rs`
- `rust/src/providers/claude/ui.rs`
- `apps/desktop-tauri/src/assets/icons/ProviderIcon-claude.svg`
- `apps/desktop-tauri/src/assets/icons/ProviderIcon-claude-mono.svg`

Acceptance.
- The popup card now lists Claude with mock zero state bars.
- `cargo test providers::claude::tests::descriptor_is_well_formed` passes, validates session label, weekly label, opus label, dashboard URL, status page URL, brand color, every required field.

Draft commit message body. `Adds the Claude descriptor with brand color 204 124 94, the dashboard and status URLs, the source mode flags Auto OAuth Web Cli, plus a no op ProviderImplementation. Card renders against zero state until a strategy lands.`

### 4.11 Commit P4 11, Claude, OAuth credential discovery

Title. `feat(claude): OAuth credential discovery, env and file with DPAPI cache`

Files.
- `rust/src/providers/claude/oauth/credentials.rs`
- `rust/src/providers/claude/oauth/cache.rs`
- `rust/src/providers/claude/errors.rs`

Acceptance.
- Test with a fixture `.credentials.json` returns parsed credentials.
- Test with a malformed file returns `decodeFailed`.
- Test with missing `user:profile` scope raises the scope error string verbatim from spec 40 section 2.3.
- DPAPI cache round trips a credential bundle.

Draft commit message body. `Implements the credential resolution chain, env CODEXBAR_CLAUDE_OAUTH_TOKEN, then DPAPI wrapped cache at %LOCALAPPDATA%\\CodexBar4Windows\\cache\\claude-oauth.bin, then file at %USERPROFILE%\\.claude\\.credentials.json. The mtime plus size fingerprint invalidates the cache when the file changes. No network calls in this commit.`

### 4.12 Commit P4 12, Claude, OAuth fetch strategy

Title. `feat(claude): OAuth usage strategy hitting api.anthropic.com/api/oauth/usage`

Files.
- `rust/src/providers/claude/oauth/strategy.rs`
- `rust/src/providers/claude/oauth/response.rs`
- `rust/src/providers/claude/models.rs`

Acceptance.
- Integration test against a mock server returns a fully populated `UsageSnapshot` with primary, secondary and tertiary windows.
- A 401 response invalidates the cached credentials.
- A 403 with `user:profile` in the body raises the scope error.
- `extra_usage` with cents values divides by one hundred for display.

Draft commit message body. `Sends GET https://api.anthropic.com/api/oauth/usage with Authorization Bearer, Accept application/json, Content-Type application/json, anthropic-beta oauth-2025-04-20, User-Agent claude-code/<version>. Thirty second per request timeout under the framework forty five second per strategy budget. Maps five_hour, seven_day, seven_day_sonnet, seven_day_opus, extra_usage per spec 40 section 2.6.`

### 4.13 Commit P4 13, Claude, Web strategy and cookie cache

Title. `feat(claude): web API strategy via browser cookies with DPAPI cookie cache`

Files.
- `rust/src/providers/claude/web/strategy.rs`
- `rust/src/providers/claude/web/endpoints.rs`
- `rust/src/providers/claude/web/cookie_cache.rs`
- `rust/src/providers/claude/web/org_selection.rs`

Acceptance.
- Test, cookie present in Edge first, used directly.
- Test, cookie absent everywhere, falls back to manual paste then errors `noSessionKeyFound`.
- Test, four endpoints called in order, partial failure on `overage_spend_limit` does not fail the strategy.
- Test, org selection picks the `chat` capable org over the API only org.

Draft commit message body. `Implements ClaudeWebStrategy. Cookie source order Edge, Chrome, Brave, Vivaldi, Arc, Opera, Chromium, Firefox per spec 40 section 3.1. All requests carry Cookie sessionKey=<value>, Accept application/json, fifteen second timeout. Cookie cache at %LOCALAPPDATA%\\CodexBar4Windows\\cache\\cookie-headers.json, key cookie.claude, cleared on 401 and 403 only.`

### 4.14 Commit P4 14, Claude, multi account routing

Title. `feat(claude): ClaudeCredentialRouting for sessionKey vs sk-ant-oat tokens`

Files.
- `rust/src/providers/claude/routing.rs`
- `rust/src/providers/claude/tokens.rs`

Acceptance.
- Unit tests cover the three input shapes from spec 40 section 3.10, sk-ant-oat bearer, bare value, full cookie header.
- A Bearer prefix is stripped, case insensitive.
- Token routed as OAuth flips cookie source to off and injects env `CODEXBAR_CLAUDE_OAUTH_TOKEN`.

Draft commit message body. `Adds the routing rule from spec 40 section 3.10 and the TokenAccountSupport entry. Surfaces a help string under the token accounts section explaining the two formats, matches the inconsistency 15 fix in the spec 00 index.`

### 4.15 Commit P4 15, watchdog binary

Title. `feat(claude): codexbar4windows-claude-watchdog binary with Job Object lifetime`

Files.
- `rust/crates/codexbar4windows-claude-watchdog/Cargo.toml`
- `rust/crates/codexbar4windows-claude-watchdog/src/main.rs`
- `rust/crates/codexbar4windows-claude-watchdog/src/job.rs`
- `apps/desktop-tauri/src-tauri/tauri.conf.json` (bundle inclusion)

Acceptance.
- Manual smoke, launching the watchdog with `-- cmd /c timeout 100` then killing the watchdog terminates the child within one second.
- `cargo test -p codexbar4windows-claude-watchdog` passes for the argument parsing.

Draft commit message body. `Adds the third workspace binary. CreateJobObject with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, CreateProcess CREATE_SUSPENDED, AssignProcessToJobObject, ResumeThread. Polls the parent every two hundred milliseconds. Honors env CODEXBAR_DISABLE_CLAUDE_WATCHDOG=1 for debugging. Installed to helpers\\codexbar4windows-claude-watchdog.exe.`

### 4.16 Commit P4 16, Claude, CLI PTY strategy

Title. `feat(claude): ConPTY-backed CLI strategy with auto-responder and parser`

Files.
- `rust/src/providers/claude/cli/strategy.rs`
- `rust/src/providers/claude/cli/pty_actor.rs`
- `rust/src/providers/claude/cli/auto_responder.rs`
- `rust/src/providers/claude/cli/parser.rs`
- `rust/src/providers/claude/cli/reset_parser.rs`

Acceptance.
- Test fixture, recorded `/usage` panel parses into a complete `UsageSnapshot`.
- Test, CPR escape `0x1B 0x5B 0x36 0x6E` is answered with `0x1B 0x5B 0x31 0x3B 0x31 0x52`.
- Test, the trust prompt substring auto answers `y\r`.
- Multi format reset parser tests cover `Resets 8pm`, `Resets at 3:00pm (America/New_York)`, `Resets May 14 at 11am`.

Draft commit message body. `Implements ClaudeCliStrategy using portable-pty for ConPTY. Window size fifty rows by one hundred sixty columns. Env scrubbing strips CODEXBAR_CLAUDE_OAUTH_TOKEN, CODEXBAR_CLAUDE_OAUTH_SCOPES, every ANTHROPIC_ prefixed key. The auto responder watches lowercased whitespace stripped output for the substrings in spec 40 section 4.4. Parser strips ANSI, trims to the last Settings:, label matches the three windows by alphanumeric only collapsed form, determines direction from adjacent words.`

### 4.17 Commit P4 17, web probe binary

Title. `feat(claude): codexbar4windows-claude-webprobe diagnostic binary`

Files.
- `rust/crates/codexbar4windows-claude-webprobe/Cargo.toml`
- `rust/crates/codexbar4windows-claude-webprobe/src/main.rs`
- `rust/crates/codexbar4windows-claude-webprobe/src/endpoints.rs`
- `rust/crates/codexbar4windows-claude-webprobe/src/report.rs`

Acceptance.
- Manual smoke against a logged in browser prints status, content type, key list, and email plus plan hints per endpoint.
- `CLAUDE_WEB_PROBE_PREVIEW=1` includes a five hundred character preview.
- Response bodies truncate at two hundred kilobytes.

Draft commit message body. `Adds the fourth workspace binary, used to diagnose web path failures and discover field renames. Shares the CookieApi with the main app, never echoes tokens. Endpoint list mirrors spec 40 section 6.2.`

### 4.18 Commit P4 18, Claude, source selection consolidation

Title. `refactor(claude): consolidate dual auto sites into claude_resolve_plan`

Files.
- `rust/src/providers/claude/planner.rs`
- `rust/src/providers/claude/descriptor.rs` (wire the planner into the fetch pipeline)

Acceptance.
- Property tests over the matrix in spec 40 section 1.3 confirm the resolved order.
- A debug subcommand `codexbar4windows providers --resolve-plan claude --runtime app --source auto` prints the resolved strategy list.

Draft commit message body. `Removes the dual auto decision site flagged in spec 40 section 1.5 and the inconsistency list in spec 00 index. Single planner takes runtime, selected source, has_oauth, has_cookie, has_cli and returns the ordered strategy list with reasons. Auto in App is OAuth, CLI, Web. Auto in CLI is Web, CLI.`

### 4.19 Commit P4 19, settings pane and source picker

Title. `feat(claude): settings pane with Source picker, manual cookie field, credentials link`

Files.
- `rust/src/providers/claude/ui.rs` (extend with `settings_pickers`, `settings_fields`, `settings_actions`)
- `rust/src/providers/claude/settings.rs`
- `apps/desktop-tauri/src/providers/claude/index.tsx` (optional override for the dashboard chip)
- `apps/desktop-tauri/src/screens/SettingsScreen.tsx`

Acceptance.
- The Settings screen renders a Claude section, the Source picker shows Auto, OAuth, Web, CLI.
- Changing the source persists to `config.json` under `providers.claude.source`.
- The manual cookie field accepts a header, validates that it contains `sessionKey=sk-ant-`, otherwise renders an inline error.
- The credentials link opens `%USERPROFILE%\.claude\.credentials.json` via `shell::open`.

Draft commit message body. `Adds the basic Claude settings pane. Descriptor driven, no provider specific React, just the configuration of SettingsDescriptor entries from the Rust side. The source picker is the manual override that consolidates the dual auto sites.`

### 4.20 Commit P4 20, tray and popup wiring

Title. `feat(claude): live tray bars and popup card driven by real UsageStore data`

Files.
- `apps/desktop-tauri/src/providers/claude/Card.tsx`
- `apps/desktop-tauri/src/components/Tray/iconRenderer.ts`
- `apps/desktop-tauri/src/components/Popup/ProviderCard.tsx`

Acceptance.
- Manual smoke, with a valid OAuth token in `.credentials.json`, the tray icon refreshes at the configured cadence and shows the live session percent.
- The popup card shows the real account email pulled from `identity.account_email`, session bar with reset countdown, weekly bar with reset countdown, opus model line, pace text.
- Toggling the reset display preference between relative and absolute updates the card without a refresh.

Draft commit message body. `Wires the live UsageStore into the tray icon and the popup card. The mock data path is removed. Closes Phase 4 acceptance, the tray now reflects reality on every refresh tick.`

### 4.21 Branch and push discipline

All twenty commits land on `main`. Per branch policy each commit is pushed immediately after it is created so the team can pull in lock step. Conventional commit format is enforced. No em dashes anywhere in commit messages or in this plan. Each commit's body explains the why, not the what. Reference the spec section that drove the change inside the body, for example `Implements spec 40 section 2.6 mapping for the OAuth response.`

If a commit fails CI, fix forward in the next commit. Do not rewrite history. The branch is `main`, hard rewrites are forbidden.

### 4.22 Definition of done per commit

Every commit must satisfy.

- `cargo fmt -- --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes locally.
- `pnpm typecheck` plus `pnpm test` pass when the commit touches the TS side.
- No new TODO without a tracking note in the commit body.
- The commit message body references the spec section that drove the change.

## 5. Phase acceptance tests

The phase is complete only when every check below passes on a clean Windows 11 install with the Claude CLI configured and a logged in Edge or Chrome session.

A1. Cold start. App launches in under two seconds. Tray icon appears with the merged or solo Claude icon per the user's setting. No error toast on first launch.

A2. OAuth path. With a valid `~/.claude/.credentials.json`, set Source to OAuth. Within one refresh tick the popup card shows session and weekly percent matching `https://claude.ai/settings/usage`, plus or minus a tolerance of one percent. The source label in Settings reads `oauth`.

A3. Web path. Without OAuth credentials but with a logged in Edge session, set Source to Web. Within one refresh tick the popup card shows session and weekly percent. The source label reads `web` with the originating browser name in parentheses.

A4. CLI path. Set Source to CLI. The watchdog plus claude PTY launch within five seconds, the parser extracts the three windows, the source label reads `cli`. The watchdog process exits within one second of the main app exit, no orphaned `claude.exe` remains.

A5. Auto fallback. Remove `.credentials.json`, set Source to Auto, refresh. The planner attempts OAuth, surfaces `unauthorized`, falls through to CLI. If the CLI is not on PATH, falls through to Web. The attempts list shown in Settings details exposes all three.

A6. Manual cookie override. Paste a `sessionKey=sk-ant-...` value into the manual cookie field, set Source to Web. Refresh succeeds and the source label reads `web (manual)`.

A7. Reset countdown. The popup shows `Resets in Xh Ym` per default user preference. Switching the preference to absolute renders `Resets at 8:00 PM`.

A8. Scope refusal. Inject a token with only `user:inference` scope into `.credentials.json`. Set Source to OAuth. The card surfaces the exact spec 40 section 2.3 message under the bar with a Fix action linking to the credentials file.

A9. Multi account routing. Add two token accounts to config, one `sk-ant-oat...` token and one bare session key. Switching the active account updates the env or cookie path correctly and the card refreshes within one tick.

A10. Web probe diagnostic. Running `helpers\codexbar4windows-claude-webprobe.exe` from a logged in machine prints a structured report including the account email. Tokens never appear in the output.

A11. Identity siloing. A unit test inserts a snapshot with mismatched `identity.provider_id` into `UsageStore`. In a debug build the test panics, in a release build the snapshot is dropped and a warning is logged.

A12. Refresh cadence. With cadence set to five minutes, the refresh loop ticks at five minute intervals plus or minus two hundred milliseconds. No tick fires while another is in flight.

A13. Forty five second timeout. A test strategy that sleeps fifty seconds completes with `ProviderError::Timeout` and the pipeline returns within one second after the budget.

A14. Identity not leaked. The popup card for Claude shows only the Claude account email. Even if a Codex placeholder snapshot carries an email, it never appears on the Claude card.

A15. Watchdog under crash. Killing the main `codexbar4windows.exe` process via Task Manager during a CLI fetch terminates the watchdog plus the `claude` child within one second. Verified by polling the process tree with `Get-Process` from PowerShell.

A16. DPAPI cache survives a restart. With a valid OAuth token, refresh once, then close and reopen the app. The next refresh reads from the DPAPI cache, not from the file. Verified by deleting `%USERPROFILE%\.claude\.credentials.json` after the first refresh, the second refresh still succeeds.

A17. Cookie source label provenance. With Brave logged in but Edge logged out, the source label reads `web (Brave)` not `web (Edge)`.

A18. Pace text. The popup card shows a pace text such as `Ahead of pace, 2.4 days of headroom`, derived from the Phase 3 pace engine reading the live `UsageSnapshot`. The string never contains the word `undefined` even when only one window is populated.

A19. Settings persistence. Set Source to Web, restart the app. The Source picker still reads Web. Persisted at `%APPDATA%\CodexBar4Windows\config.json` under the key `providers.claude.source`.

A20. Refresh button. Clicking Refresh now in the popup triggers a fetch within fifty milliseconds, ignoring the cadence timer. The button enters a spinning state until the fetch completes, then settles within two hundred milliseconds.

### 5.1 Manual smoke checklist

Before merging the final P4 20 commit, an engineer runs the smoke list on a clean Windows 11 26200 install. The smoke list is the human readable companion to A1 through A20.

1. Install. Run the unsigned MSI from the latest CI artifact. Confirm SmartScreen does not block the install with the bundled certificate.
2. First launch. Confirm onboarding finishes within five seconds, the tray icon appears, the popup opens within one hundred milliseconds.
3. Authenticated path. With `claude login` already run, confirm A2 passes.
4. Cookie path. Open Edge, log into `https://claude.ai`. Switch Source to Web, refresh. Confirm A3 passes.
5. CLI path. Switch Source to CLI. Confirm A4 passes, the watchdog and claude.exe both appear in Task Manager, both exit on app quit.
6. Multi account. Add a second token in Settings under the token accounts section. Switch active account. Confirm A9 passes.
7. Reset display. Toggle the absolute vs relative reset preference. Confirm A7 passes.
8. Diagnostic. Run the webprobe binary. Confirm A10 passes.
9. Clean uninstall. Uninstall the app. Confirm `%APPDATA%\CodexBar4Windows` plus `%LOCALAPPDATA%\CodexBar4Windows` are removed.

## 6. CI gates

The CI pipeline must run on every push to `main` and every pull request, even though the branch policy says everything lands on main. The PR template stays so contributors can iterate locally.

G1. `cargo fmt -- --check` plus `cargo clippy --workspace --all-targets -- -D warnings`.

G2. `cargo test --workspace`. Includes the framework tests, the Hello provider, every Claude strategy with fixtures.

G3. Mock fixtures. The Claude OAuth strategy ships fixtures at `rust/src/providers/claude/oauth/fixtures/`, the Web strategy ships fixtures under `web/fixtures/`, the CLI strategy ships ANSI recordings under `cli/fixtures/`. Every fixture is loaded by a `#[test]` and asserts a stable `UsageSnapshot`. New fixture goes in with every parser change.

G4. `pnpm typecheck`, `pnpm test`, `pnpm lint`. The `specta` generated TypeScript bridge is verified by a snapshot test, drift triggers a CI failure.

G5. `pnpm tauri build` against a Windows runner produces three signed binaries, `codexbar4windows.exe`, `codexbar4windows-claude-watchdog.exe`, `codexbar4windows-claude-webprobe.exe`. Authenticode signing is gated behind a CI secret, missing secret skips signing but does not fail.

G6. Secret hygiene. A pre commit hook plus a CI lint scans for accidental tokens. `sk-ant-`, `sk-ant-oat`, `Bearer sk-`, `sessionKey=sk-ant-` are forbidden in committed files outside `*.fixture` paths.

G7. A smoke job runs the `codexbar4windows providers --resolve-plan claude --runtime app --source auto` command and asserts the output ordering.

G8. Fixture freshness. The fixtures under `oauth/fixtures/`, `web/fixtures/`, `cli/fixtures/` carry a generation timestamp inside a sibling `README.md`. Any fixture older than six months emits a CI warning. Stale fixtures do not fail the build, they remind us to re record.

### 6.1 Mock fixture inventory

The fixture set ships with the Phase 4 commits. Each fixture is a verbatim recorded response with secrets scrubbed. Verbatim means the exact bytes the server sent, modulo a fixed redaction of bearer tokens and session keys.

OAuth fixtures.
- `oauth_usage_max_plan.json`. Full response with five_hour, seven_day, seven_day_sonnet, seven_day_opus, extra_usage all present, max plan.
- `oauth_usage_pro_plan.json`. Pro plan, no extra_usage, no seven_day_opus.
- `oauth_usage_enterprise_no_weekly.json`. Enterprise account, five_hour only, weekly absent.
- `oauth_usage_scope_error.json`. The 403 response body containing the literal substring `user:profile`.
- `oauth_usage_rate_limited.json`. The 429 body, used to verify the strategy surfaces as `Network` retryable.

Web fixtures.
- `web_organizations_chat_first.json`. Two orgs, chat capable first.
- `web_organizations_api_only.json`. One org with only the `api` capability, must be skipped.
- `web_usage_typical.json`. Full usage panel response.
- `web_overage_disabled.json`. is_enabled false, must be skipped.
- `web_overage_enabled.json`. With monthly_credit_limit and used_credits, both in cents.
- `web_account_email.json`. Account endpoint with memberships and rate_limit_tier.

CLI fixtures.
- `cli_usage_panel_max.ansi`. Recorded `/usage` panel for a max account, full ANSI sequences preserved.
- `cli_usage_panel_pro.ansi`. Pro account, no opus line.
- `cli_status_panel.ansi`. Recorded `/status` panel.
- `cli_rate_limit_error.ansi`. The JSON wrapped error case from spec 40 section 4.8.

Every fixture has a sibling `.expected.json` capturing the expected `UsageSnapshot` after parsing. The parser tests diff the actual against the expected, byte for byte.

## 7. Phase exit checklist

The Phase 4 ticket is closed only when every item below is checked, signed by the implementing engineer and the reviewer.

- [ ] Twenty commits landed on `main`, each compiles, each passes CI.
- [ ] `cargo doc --workspace --no-deps` builds without warnings, every public item has a doc comment.
- [ ] The acceptance tests A1 through A20 all pass on a clean Windows 11 26200 install.
- [ ] The smoke checklist 1 through 9 has been run by an engineer who did not write the code.
- [ ] The fixture set in section 6.1 is present under the listed paths.
- [ ] The Phase 4 release notes draft is opened against `docs/windows/RELEASE_NOTES.md`, listing the breaking changes (the dual auto consolidation, the identity siloing enforcement).
- [ ] Open question Q1 through Q10 in this document have been answered, or explicitly punted to a follow up phase.
- [ ] The Phase 5 plan is unblocked, the framework surface is stable, no API signature in `rust/src/providers/` is marked `TODO`.

## 8. Risks and mitigations

R1. Chromium v20 App Bound Encryption. `claude.ai` is an early adopter of App Bound Encryption, the cookie encryption format prefix changed from `v10` and `v11` to `APPB`. Decrypting `APPB` outside Chrome's process is fragile and may break on minor Chrome releases. Mitigation, attempt DPAPI plus AES GCM first, on `APPB` prefix surface the one time toast described in spec 40 section 3.2 and auto switch the user to the manual cookie source. Document the policy in the Phase 4 release notes. Owner of the toast copy, the user facing copy is locked to the verbatim spec text.

R2. OAuth scope drift. The OAuth tokens issued by `claude login` historically defaulted to `user:profile user:inference`, recent CLI releases have shipped tokens with `user:inference` only. Mitigation, the strategy refuses such tokens with a precise message that points at `claude setup-token`, plus an inline Fix action in the popup. The watchdog binary is unaffected.

R3. ConPTY differences across Windows builds. ConPTY is available on Windows 10 1809 plus, the behavior is stable on Windows 11 26200 but has known quirks around resize on older builds. Mitigation, pin window size at launch to fifty by one hundred sixty, do not call `ResizePseudoConsole`, document the minimum supported Windows version as 1809.

R4. PATH resolution for the `claude` binary. Claude CLI on Windows installs via npm typically resolves to `claude.cmd` first then `claude.exe`. Mitigation, the binary resolver honors `CLAUDE_CLI_PATH` env override, otherwise walks PATH preferring `.cmd` then `.exe`. Surface the resolved path in the Settings status row for debugging.

R5. First run prompts that the auto responder does not know about. The Claude CLI ships new prompts occasionally. Mitigation, the parser logs every unrecognized stop substring at `tracing::warn` with the lowercased whitespace stripped form, the user copies the log line into a bug report and we add the entry in a patch release.

R6. DPAPI cache corruption. A DPAPI master key rotation can invalidate cached blobs. Mitigation, on decryption failure the cache file is deleted and the next fetch falls through to the live source. No user facing prompt, the recovery is silent.

R7. Identity leak via shared `UsageStore`. Spec 30 section 2.2 calls this out, it is the only non negotiable security boundary in the framework. Mitigation, the test in commit P4 07 plus G2 prevents regression. A panic in debug builds plus a tracing warn in release fail loudly.

R8. CLI parser fragility. The `/usage` panel layout is stable enough but Anthropic could change it. Mitigation, fixture driven tests under `cli/fixtures/` plus the positional fallback in the parser when label match fails. Document the parser version in the Settings status row.

R9. Watchdog disabled in debug. The env `CODEXBAR_DISABLE_CLAUDE_WATCHDOG=1` skips the watchdog entirely, useful for debugging but dangerous in production. Mitigation, the env is read only in debug builds, the release build ignores it.

R10. Job Object plus inherited handles. The PTY end handles must be marked inheritable when passed to the watchdog. Mitigation, set `bInheritHandles` correctly in the `STARTUPINFOEX` extended attributes and the parent handles closed in the parent after `CreateProcess` succeeds, a regression test against the helpers crate covers this.

## 9. Time estimate

Twenty atomic commits. Estimated five to seven engineering days for a single experienced engineer working full time, ten to fourteen days at a half time pace.

Breakdown.

- Day one. Commits P4 01 through P4 03, framework types, errors, registry skeleton. About one day, mostly type definitions, low risk.
- Day two. Commits P4 04 through P4 06, fetch plan, implementation trait, settings descriptors. About one day, the timeout test for P4 04 is the trickiest part.
- Day three. Commits P4 07 through P4 09, identity siloing, registry validation, Tauri commands. Hello provider plus the descriptor IPC unlocks visible progress.
- Day four. Commits P4 10 through P4 12, Claude descriptor, OAuth discovery, OAuth fetch. Real network calls and the credential cache, expect one half day of integration work.
- Day five. Commits P4 13 through P4 14, Web strategy plus multi account routing. The cookie cache lifecycle is finicky, expect debugging time.
- Day six. Commits P4 15 through P4 17, watchdog binary, CLI PTY strategy, web probe binary. The CLI parser plus the auto responder are the highest risk pieces, allocate buffer.
- Day seven. Commits P4 18 through P4 20, planner consolidation, settings pane, tray plus popup wiring. Polish day, also the day acceptance tests A1 through A14 are run on a clean machine.

Buffer. Two additional days of slip allowed before re scoping. Suggested re scope, defer the web probe binary to Phase 4 follow up and the delegated CLI refresh in C5 to a Phase 4.1 patch release.

## 10. Open questions

Q1. Direct OAuth refresh client id. Spec 40 section 2.10 lists the client id `9d1c250a-e61b-44d9-88ed-5944d1962f5e` for the public PKCE refresh flow. We default to this id, override via env `CODEXBAR_CLAUDE_OAUTH_CLIENT_ID`. The question, do we want to mint our own client id before shipping, or do we ship with the published id and revisit. Default assumption, ship with the published id, the env override exists.

Q2. Delegated refresh in this phase. The plan currently ships the OAuth strategy with a clean re auth error and defers the delegated refresh state machine to a follow up. Should we attempt to land delegated refresh inside Phase 4 or push to Phase 4.1. Recommendation, push to Phase 4.1, the cooldown state plus the PTY actor reuse warrant their own commits.

Q3. Manual cookie validation strictness. The current settings field validator only checks that the pasted header contains `sessionKey=sk-ant-`. Should we additionally validate that it parses as a proper RFC 6265 cookie header. Default, the loose check, the parser further down the stack catches malformed input.

Q4. The Sonnet legacy label. Spec 40 section 0 calls out the legacy `Sonnet` label even when the data is `seven_day_opus`. The spec 00 index inconsistency 11 lists this for the fix during port pile. Default, we preserve the legacy label in this phase to match spec 40, then change to a dynamic label in a Phase 4 follow up commit once the popup card UX has settled.

Q5. The Web probe shipping channel. Should the diagnostic web probe ship in every release or be gated behind a debug build. Default, ship in every release, the env gates already prevent accidental token output.

Q6. Cookie source name ordering. Edge appears first per spec 40 section 3.1, but Brazilian Portuguese localization (recent commit `22c44848`) does not yet localize browser names. The settings UI renders browser names verbatim. Open question, do we add localization keys now or punt to Phase 8 polish. Default, render the English browser names in this phase, add the localization keys in Phase 8.

Q7. Identity rejection severity. Debug panic plus release warn matches the spec 30 section 2.2 invariant. Some teams prefer to never panic. Default, keep the debug panic, it surfaces invariant breaks loudly during integration.

Q8. Watchdog disable env. Should `CODEXBAR_DISABLE_CLAUDE_WATCHDOG=1` work in release builds. Default, ignored in release, the env only has effect in debug builds. Risk, makes a debug only escape hatch in production but the user is unaware.

Q9. Per browser cookie order overrides. Spec 30 section 7.2 mentions a per provider override for cookie source order. Claude uses the default order. Should we wire the per provider override in this phase or punt. Default, ship the framework support but Claude uses the default order, no override needed yet.

Q10. Token refresh background loop. The `OAuthRefreshDaemon` from spec 30 section 8.4 runs once per logged in provider, never prompts. Should this daemon run in Phase 4 or Phase 5. Default, the daemon ships in Phase 4 with one Claude tenant, Phase 5 adds Codex as a second tenant. The daemon is generic.

## 11. Final notes

Phase 4 is the riskiest phase in the porting plan because it sets the contract for every later provider. The framework code is small but the surface is wide. The Claude provider exercises every flex point, OAuth tokens, browser cookies, PTY scraping, multi account routing, watchdog lifetime, identity siloing.

Two principles to keep in mind during execution. First, every commit must compile and pass tests on its own, no half landed types, no temporarily disabled tests. Second, the spec is the contract, deviations are intentional and noted in the commit message body. The fix during port list in spec 00 index is being drained, not preserved.

When this phase ships, the tray icon shows live Claude data on every refresh tick, the popup card shows the real account email and reset timers, and the Settings pane gives the user the manual override they need when an automatic path fails. That is the bar. Phase 5 begins with Codex, the same framework, a different set of strategies, and a tighter time budget because the hard work was done here.
