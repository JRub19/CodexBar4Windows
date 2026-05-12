---
summary: "Provider system architecture: the framework that hosts the 39 providers. Windows refactor spec for Tauri 2 + React + a shared `codexbar` Rust crate."
audience: "Rust/TS engineer porting CodexBar to Windows."
status: "Authoritative blueprint."
length_target: "700-1300 lines."
---

# 30 — Provider system architecture (framework)

This document specifies the **framework** that hosts all CodexBar providers. It is the contract every provider plugs into, not the contract of any one provider. Per-provider specs (Codex, Claude, Cursor, …) live in sibling documents and define only the *plan + parsing + endpoints*; the lifecycle, registry, fetch loop, error shape, and settings shape are all owned here.

The goal is that, after reading this doc plus one per-provider spec, an engineer can add a new provider to the Windows app by:

1. creating `rust/src/providers/<name>/` with a `descriptor.rs`, one or more `strategies.rs` files, and `models.rs`,
2. dropping a `<Name>ProviderImplementation` UI binding into `apps/desktop-tauri/src/providers/<name>/`,
3. registering the descriptor in one place,
4. adding strings + icons + a docs page.

No other touchpoints. No grep-and-add lists.

Polish target: **Phantom-wallet for clarity, Duolingo for delight**. Provider cards should feel handcrafted but be 100% data-driven from the descriptor.

---

## 0. TL;DR — the one diagram

```
┌── Tauri shell (Rust, src-tauri) ─────────────────────────────┐
│  Tray + popup window + IPC commands + native menu            │
└────────────┬─────────────────────────────────────────────────┘
             │ invoke("provider_descriptors") / event("usage:updated")
┌────────────▼─────────────────────────────────────────────────┐
│  React popup (apps/desktop-tauri/src)                        │
│  ProviderCard <- descriptor + UsageSnapshot                  │
│  ProviderSettingsPanel <- SettingsDescriptor[]               │
└────────────┬─────────────────────────────────────────────────┘
             │ Tauri commands
┌────────────▼─────────────────────────────────────────────────┐
│  codexbar core (rust/src)                                    │
│                                                              │
│   providers::registry  (inventory! static set)               │
│        │                                                     │
│        ▼                                                     │
│   FetchScheduler ──► ProviderFetchPlan::run(ctx)             │
│        │                  └─► Strategy 1.. (CLI/Web/OAuth/…) │
│        ▼                                                     │
│   UsageStore (Arc<RwLock>)  ─► tray::redraw + popup event    │
│                                                              │
│   host services: http, cookies, dpapi, conpty, keyring,     │
│                  jsonl-scanner, status, locale, log         │
└──────────────────────────────────────────────────────────────┘
```

The dashed line between Tauri shell and React popup is the **only** place provider-specific code in the UI layer is allowed to know provider IDs by name. Everything else iterates descriptors.

---

## 1. Module layout

### 1.1 Mac source-of-truth (today)

| Mac module | Concern | Windows equivalent |
| --- | --- | --- |
| `Sources/CodexBarCore/Providers/*` | descriptors, fetch plan types, strategy protocol, cookie source, branding, CLI config, version detection, candidate retry helper | `rust/src/providers/` (framework module) |
| `Sources/CodexBarCore/Providers/<Id>/` | each provider's descriptor + strategies + parser + models | `rust/src/providers/<id>/` |
| `Sources/CodexBar/Providers/Shared/*` | app-side `ProviderImplementation` protocol + registry + settings UI descriptors + menu context + login flow + presentation | `rust/src/providers/ui.rs` (descriptor-side helpers) **and** `apps/desktop-tauri/src/providers/shared/` (TS-side descriptors + components) |
| `Sources/CodexBar/Providers/<Id>/` | provider's UI hooks (Swift): settings toggles, login flow, settings store keys | `apps/desktop-tauri/src/providers/<id>/` (TS component overrides, only when descriptor-driven UI is not enough) |
| `Sources/CodexBarMacros/*` | SwiftSyntax compiler plugin generating registry side-effects | **deleted** — replaced by `inventory!` (or hand-written `register!` macro) in Rust |
| `Sources/CodexBarMacroSupport/*` | macro attribute shims | **deleted** |

### 1.2 Windows target layout

```
rust/src/providers/
  mod.rs                        // re-exports + registry bootstrap
  registry.rs                   // inventory!-driven static catalog
  descriptor.rs                 // ProviderDescriptor, ProviderMetadata
  branding.rs                   // ProviderBranding, ProviderColor
  cli_config.rs                 // ProviderCLIConfig
  cookie_source.rs              // CookieSource enum + order types
  fetch_plan.rs                 // ProviderFetchPlan, Pipeline, Strategy trait
  fetch_context.rs              // ProviderFetchContext, runtime, source_mode
  fetch_outcome.rs              // ProviderFetchOutcome, ProviderFetchAttempt
  runtime.rs                    // ProviderRuntime trait (lifecycle hooks)
  presentation.rs               // ProviderPresentation (detail line)
  settings_descriptor.rs        // settings descriptors: Toggle/Field/Picker/Action/TokenAccounts
  settings_snapshot.rs          // ProviderSettingsSnapshot + Builder + Contribution
  token_account.rs              // ProviderTokenAccount, store, support catalog
  token_resolver.rs             // ProviderTokenResolver (env vars per provider)
  version_detector.rs           // ProviderVersionDetector
  cookie_negotiation.rs         // browser import order on Windows
  candidate_retry.rs            // ProviderCandidateRetryRunner
  errors.rs                     // ProviderError + ProviderFetchError
  models/                       // canonical result models
    rate_window.rs
    usage_snapshot.rs
    credits.rs
    provider_cost.rs
    identity.rs
    storage_footprint.rs
  codex/      claude/    cursor/    copilot/    gemini/    ...  // per-provider
```

```
apps/desktop-tauri/src/providers/
  index.ts                      // imports all <id>/index.tsx for tree-shaking and registers TS-side UI overrides
  shared/
    ProviderCard.tsx            // descriptor-driven default card
    ProviderSettingsPanel.tsx   // renders SettingsDescriptors generically
    ProviderIcon.tsx            // SVG <-> brand asset resolver
    types.ts                    // TS mirrors of descriptor + snapshot shapes (codegen target)
  codex/index.tsx               // (optional) bespoke MenuActions, dashboard chips
  claude/index.tsx
  ...
```

Rule mirroring `docs/provider.md` line 84-92: **a new provider adds exactly one Rust folder and at most one TS folder.** Anything else is a smell.

### 1.3 IPC boundary

The TS side never reaches into Rust internals. All provider state crosses the boundary as:

- `invoke("provider_descriptors") -> ProviderDescriptorDTO[]` (one-shot at popup mount, includes metadata + branding + capability flags + settings schema)
- `invoke("provider_snapshots") -> Record<ProviderId, UsageSnapshotDTO>` (initial pull)
- `event("usage:updated", payload: { provider, snapshot, attempts })` (push on every refresh)
- `invoke("provider_refresh", { provider })`, `invoke("provider_login", { provider })`, etc.

DTO shapes are owned by `rust/src/providers/host/dto.rs` and codegenned to TS via `ts-rs` (or `specta`). Never hand-roll the TS types.

---

## 2. Provider identity model

A provider is identified at compile time by its `ProviderId` enum variant (Rust) / string literal (TS). Everything else is data in a `ProviderDescriptor`.

### 2.1 Canonical descriptor field table

The descriptor is the **single source of truth**. The Mac descriptor on this fork composes five sub-structs: `ProviderMetadata`, `ProviderBranding`, `ProviderTokenCostConfig`, `ProviderFetchPlan`, `ProviderCLIConfig`. Windows keeps the same composition.

| Field | Type | Optionality | Semantics |
| --- | --- | --- | --- |
| `id` | `ProviderId` (enum) | required | Stable persistence key. Never reordered; new providers append. Used in `~/.codexbar/config.json`, IPC payloads, log scopes. |
| `metadata.id` | `ProviderId` | required | Mirror of `id`; kept for embeddable metadata blobs. |
| `metadata.display_name` | `&'static str` | required | User-facing brand name. Localized via `locale.rs`. |
| `metadata.session_label` | `&'static str` | required | Primary window label (e.g. "Session", "5h", "Rate limit"). |
| `metadata.weekly_label` | `&'static str` | required | Secondary window label. |
| `metadata.opus_label` | `Option<&'static str>` | optional | Model-specific window label (Claude Opus). |
| `metadata.supports_opus` | `bool` | required | Show third window if true. |
| `metadata.supports_credits` | `bool` | required | Enables credits row in card. |
| `metadata.credits_hint` | `&'static str` | required | Tooltip under credits gauge. |
| `metadata.toggle_title` | `&'static str` | required | Settings toggle string. |
| `metadata.cli_name` | `&'static str` | required | CLI subcommand identity (`codexbar usage --provider <cli_name>`). |
| `metadata.default_enabled` | `bool` | required | First-run default for this provider. |
| `metadata.is_primary_provider` | `bool` | default false | Sorts to the top of the icon stack; affects merged-icon decisions. |
| `metadata.uses_account_fallback` | `bool` | default false | Provider uses local `auth.json`-style multi-account file (Codex). |
| `metadata.browser_cookie_order` | `Option<BrowserCookieImportOrder>` | optional | Override of the default Windows browser import sequence. |
| `metadata.dashboard_url` | `Option<&'static str>` | optional | URL opened by "Dashboard" menu entry. |
| `metadata.subscription_dashboard_url` | `Option<&'static str>` | optional | URL opened by "Buy" CTA. |
| `metadata.status_page_url` | `Option<&'static str>` | optional | Statuspage.io base; polled if present. |
| `metadata.status_link_url` | `Option<&'static str>` | optional | Browser-only status link (no polling). |
| `metadata.status_workspace_product_id` | `Option<&'static str>` | optional | Google Workspace status product id. |
| `branding.icon_style` | `IconStyle` (enum) | required | Picks which built-in tray icon family to use. |
| `branding.icon_resource_name` | `&'static str` | required | SVG asset key under `apps/desktop-tauri/src/assets/icons/`. |
| `branding.color` | `ProviderColor` (rgb 0..1) | required | Brand color for accents, indicator dots, switcher pills. |
| `token_cost.supports_token_cost` | `bool` | required | Enables local cost-scanner card. |
| `token_cost.no_data_message` | `fn() -> String` | required | Localized fallback string in the cost card. |
| `fetch_plan.source_modes` | `BitFlags<SourceMode>` | required | Which `source_mode` values the settings picker offers. |
| `fetch_plan.pipeline` | `ProviderFetchPipeline` | required | Resolves an ordered list of strategies given a fetch context. |
| `cli.name` | `&'static str` | required | Canonical CLI alias key (for `cli_name_map`). |
| `cli.aliases` | `&'static [&'static str]` | required (may be empty) | Extra accepted CLI aliases. |
| `cli.version_detector` | `Option<fn(&BrowserDetection) -> Option<String>>` | optional | Detects installed CLI version for the menu chip. |

Plus three "registered separately" facets that travel **alongside** the descriptor in the same provider folder:

- `TokenAccountSupport` (see §11) — declares how the provider supports multi-account.
- `ProviderImplementation` trait impl (Rust side) and `ProviderUIOverrides` (TS side) — see §6 and §10.
- `ProviderSettingsSnapshotContribution` builder hook — see §9.

### 2.2 Hard rule: no identity leakage

Mac enforces this with a `ProviderIdentitySnapshot.scoped(to:)` helper (`Sources/CodexBarCore/UsageFetcher.swift:72-80`). The Windows port keeps the same rule:

> Identity fields (`account_email`, `account_organization`, `login_method`) are siloed per provider. They MAY only be displayed inside the matching provider card. The `UsageStore` must reject a snapshot whose `identity.provider_id` does not match the slot it is being written into, or scope it before storing. This is non-negotiable; provider A must never display provider B's email.

### 2.3 Icon styles

`IconStyle` is an enum (one per family) that maps to either a built-in tray meter style or a brand SVG. Mac enumerates ~40 styles (`Providers.swift:49-90`). Windows mirrors the same enum verbatim plus the synthetic `IconStyle::Combined` for the "Merge Icons" mode. The icon renderer (`rust/src/tray/icon.rs`) is the only consumer; everywhere else uses `branding.icon_resource_name`.

---

## 3. Macro registration → Rust equivalent

### 3.1 What Mac does today

Three Swift macros, defined in `Sources/CodexBarMacros/ProviderRegistrationMacros.swift`:

| Macro | Attached to | Generates |
| --- | --- | --- |
| `@ProviderDescriptorDefinition` | descriptor type (e.g. `CodexProviderDescriptor`) | `public static let descriptor: ProviderDescriptor = Self.makeDescriptor()` — eliminates one line of boilerplate. |
| `@ProviderDescriptorRegistration` | descriptor type | A private file-scope let: `_CodexBarDescriptorRegistration_<TypeName> = ProviderDescriptorRegistry.register(<TypeName>.descriptor)`. This is what populates `ProviderDescriptorRegistry`. |
| `@ProviderImplementationRegistration` | UI implementation type | Same pattern, but for `ProviderImplementationRegistry`: `_CodexBarImplementationRegistration_<TypeName> = ProviderImplementationRegistry.register(<TypeName>())`. |

Compile-time diagnostics also fire:

- `unsupportedTarget` — wrong decl kind.
- `missingDescriptor` — must have `static let descriptor` or `static func makeDescriptor()`.
- `missingMakeDescriptor` — when using `@ProviderDescriptorDefinition` without the factory.
- `duplicateDescriptor` — both `static let descriptor` and the macro present.
- `missingInit` — `@ProviderImplementationRegistration` needs a zero-arg init.

But — and this is the subtle bit — `ProviderDescriptorRegistry.descriptorsByID` (`ProviderDescriptor.swift:55-95`) still lists every provider manually. And `ProviderImplementationRegistry.makeImplementation(for:)` (`ProviderImplementationRegistry.swift:14-56`) is a giant `switch`. The macros run *peer* side-effect registrations, but the canonical seed is the hand-written map; the macro registrations dedupe by id, so the map keeps the order stable. This is a known smell — the refactor notes (`docs/refactor/macros.md`) flag exactly this. **The Windows port fixes it.**

### 3.2 Windows: registry = `inventory!`

We replace the macro plumbing with the `inventory` crate (compile-time plugin registry, zero runtime cost).

Each provider folder declares:

```rust
// rust/src/providers/codex/registration.rs
use crate::providers::{descriptor::ProviderDescriptor, ProviderRegistration};
use crate::providers::codex::descriptor::descriptor;

inventory::submit! {
    ProviderRegistration {
        descriptor: descriptor as fn() -> ProviderDescriptor,
        ui_factory: super::ui::factory,
        token_account_support: super::tokens::support,
    }
}
```

The registry is a thin iterator (`rust/src/providers/registry.rs`):

```rust
pub struct ProviderRegistration {
    pub descriptor: fn() -> ProviderDescriptor,
    pub ui_factory: fn() -> Box<dyn ProviderImplementation>,
    pub token_account_support: Option<fn() -> TokenAccountSupport>,
}
inventory::collect!(ProviderRegistration);
```

At startup we materialize once:

```rust
pub static REGISTRY: Lazy<ProviderCatalog> = Lazy::new(|| {
    ProviderCatalog::build(inventory::iter::<ProviderRegistration>())
});
```

`ProviderCatalog::build` validates: no duplicate `id`, no missing icon resource, no metadata gaps. Validation failures panic at startup with a clear message — this is the Rust analog of the Swift `preconditionFailure` in `ProviderDescriptor.swift:99`.

### 3.3 Why `inventory!` and not `linkme` or a hand-written `Vec`?

| Approach | Pros | Cons | Verdict |
| --- | --- | --- | --- |
| `inventory` | dead-simple, zero macro learning curve, ships static slice, works on Windows MSVC | uses link-time `static` so dead-code-eliminating builds need a `#[used]` shim (handled by the crate) | **recommended** |
| `linkme` | newer, slightly faster startup, ergonomic | requires `linker` cfg, has had MSVC sharp edges | acceptable alternative; fall back if `inventory` ever breaks on a target |
| `build.rs` codegen | fully explicit; no link tricks; easy to audit | another build step; harder to add a provider in one PR; slower compile | **rejected for v1** but kept as escape hatch |
| Hand-written `Vec<&'static ProviderDescriptor>` | dead obvious | grep-and-add every time = exactly the smell we are removing from Mac | rejected |

Decision: **inventory** with one validation pass at startup. The registry is then immutable for the process lifetime.

### 3.4 Compile-time diagnostics on Windows

Where the Swift macro errored with `missing_init`, the Rust port uses a plain function pointer signature — if the impl is missing, the `inventory::submit!` call fails to compile. We do not need a compiler plugin; the type system is the diagnostic.

Optionally we can ship a tiny `#[provider]` proc-macro that generates the `inventory::submit!` block from a `Descriptor` impl, but the marginal ergonomics are not worth the maintenance cost — this is the kind of macro that has historically caused pain on Mac (and is what `docs/refactor/macros.md` is unwinding). Skip the macro in v1.

---

## 4. Provider lifecycle (state machine)

### 4.1 Phases

```
[boot]
   │
   ▼
[load] ── registry materialized (ProviderCatalog::build)
   │
   ▼
[wire] ── for each provider: ProviderImplementation::observe_settings(&settings)
   │            (subscribes to provider-relevant settings keys)
   │
   ▼
[ready] ── refresh loop tick @ cadence
   │
   │   ┌─────────────────────────────────────────────┐
   │   │  for each enabled provider in parallel:     │
   │   │   1. impl.is_available(&ctx)?               │
   │   │   2. build ProviderFetchContext             │
   │   │   3. descriptor.fetch_outcome(ctx).await    │
   │   │   4. fold outcome into UsageStore           │
   │   │   5. impl.provider_did_refresh / did_fail   │
   │   │   6. emit "usage:updated" event             │
   │   └─────────────────────────────────────────────┘
   │
   ▼
[render] ── tray::redraw_if_changed; popup pulls via Tauri command
   │
   ▼ (user clicks tray)
[present] ── React reads ProviderDescriptor + UsageSnapshot
              → ProviderCard renders detail_line (impl.presentation(...))
              → ProviderSettingsPanel renders settings_toggles/fields/pickers/actions
```

### 4.2 Hook timing

`ProviderImplementation` hooks (Mac trait in `Sources/CodexBar/Providers/Shared/ProviderImplementation.swift:9-80`, ported as a Rust trait) fire at these phases:

| Hook | Phase | Threading | Default |
| --- | --- | --- | --- |
| `observe_settings(settings)` | wire | main task | no-op |
| `is_available(ctx)` | ready (per tick) | scheduler | `true` |
| `default_source_label(ctx)` | render | main task | `None` |
| `decorate_source_label(ctx, base)` | render | main task | passthrough |
| `source_mode(ctx)` | ready | main task | `Auto` |
| `detect_version(ctx)` | wire + on-demand | background | falls back to `descriptor.cli.version_detector` |
| `make_runtime()` | wire | main task | `None` |
| `settings_toggles(ctx)` | settings open | main task | `[]` |
| `settings_fields(ctx)` | settings open | main task | `[]` |
| `settings_pickers(ctx)` | settings open | main task | `[]` |
| `settings_actions(ctx)` | settings open | main task | `[]` |
| `token_accounts_visibility(ctx, support)` | settings open | main task | derived rule |
| `settings_snapshot(ctx)` | every fetch context build | main task | `None` |
| `apply_token_account_cookie_source(settings)` | account switch | main task | no-op |
| `append_usage_menu_entries(ctx, &mut entries)` | menu build | main task | no-op |
| `append_action_menu_entries(ctx, &mut entries)` | menu build | main task | no-op |
| `login_menu_action(ctx)` | menu build | main task | `None` |
| `run_login_flow(ctx)` | user-initiated | task | `false` |
| `presentation(ctx).detail_line` | render | main task | `"<cli_name> <version>"` |

Threading: in Rust, "main task" means the Tauri-managed UI task (we annotate the trait functions accordingly with `async fn` where needed). Background work happens in spawned `tokio::task`s.

### 4.3 `ProviderRuntime` (optional long-lived helper)

For providers that need a persistent background actor (PTY session, websocket, ADC token refresh), the impl returns `Some(Arc<dyn ProviderRuntime>)` from `make_runtime`. The runtime implements:

| Method | Called when |
| --- | --- |
| `start(ctx)` | provider becomes enabled |
| `stop(ctx)` | provider becomes disabled, or app shutdown |
| `settings_did_change(ctx)` | any setting the provider observes changed |
| `provider_did_refresh(ctx, provider)` | refresh succeeded — runtime may pre-warm |
| `provider_did_fail(ctx, provider, error)` | refresh failed — runtime may reset cookies/session |
| `perform(action, ctx)` | UI-initiated action (`ForceSessionRefresh`, `OpenAIWebAccessToggled(bool)`) |

This is the Mac `ProviderRuntime` (`Sources/CodexBar/Providers/Shared/ProviderRuntime.swift:1-34`) ported 1:1.

---

## 5. `ProviderFetchPlan` — the planning step

### 5.1 Algorithm (canonical)

The pipeline (`ProviderFetchPlan.swift:154-204` on Mac) runs strategies in declared order, with availability gating and per-strategy fallback decisions:

```text
fn run(ctx) -> ProviderFetchOutcome:
    strategies := plan.pipeline.resolve_strategies(ctx)
    attempts := []
    last_err := None
    for strategy in strategies:
        if not strategy.is_available(ctx).await:
            attempts.push(Attempt{ id, kind, was_available: false, error: None })
            continue
        match strategy.fetch(ctx).await:
            Ok(result):
                attempts.push(Attempt{ id, kind, was_available: true, error: None })
                return Outcome{ result: Ok(result), attempts }
            Err(e):
                last_err := Some(e)
                attempts.push(Attempt{ id, kind, was_available: true, error: Some(e.into()) })
                if strategy.should_fallback(&e, ctx):
                    continue
                else:
                    return Outcome{ result: Err(e), attempts }
    return Outcome{
        result: Err(last_err.unwrap_or(ProviderFetchError::NoAvailableStrategy(id))),
        attempts,
    }
```

Properties:

- **Deterministic** for a given context. No retries, no randomness, no time-based decisions.
- **Observable**: every strategy attempt is recorded in `attempts` for `--verbose` CLI output and the in-app debug pane. This is `docs/refactor/macros.md` point 3 already done.
- **Bounded**: total work = sum of per-strategy timeouts. The scheduler enforces an outer wall-clock too.
- **Fallback is per-strategy and per-error**: `should_fallback(error, ctx)` is the only place a strategy gets to say "no, this error is terminal, stop the pipeline". This is how Codex's CLI strategy refuses to mask a real auth error by silently dropping to Web.

### 5.2 Source modes

```rust
pub enum SourceMode { Auto, Web, Cli, OAuth, Api }
```

`source_modes: BitFlags<SourceMode>` on the plan tells the settings picker which choices to expose. `Auto` is the default and is the only mode that runs the full pipeline — explicit picks (`Web`, `Cli`, `OAuth`, `Api`) constrain the pipeline to strategies matching that kind. `ProviderFetchPipeline::resolve_strategies` is responsible for honoring `ctx.source_mode`. This is how Claude offers Auto/OAuth/Web/CLI and Codex offers Auto/Cli; the descriptor declares it once and the UI binds to it.

### 5.3 Candidate retry runner

`Sources/CodexBarCore/Providers/ProviderCandidateRetryRunner.swift:1-32` is a small utility for *within-strategy* retry across N candidates (e.g. multiple cookie sources, multiple OAuth endpoints, multiple regions). It accepts a list of candidates and a `should_retry(error) -> bool` predicate and short-circuits on the first success.

Windows port: `rust/src/providers/candidate_retry.rs`. Signature:

```rust
pub async fn run_candidates<C, T, Fut>(
    candidates: impl IntoIterator<Item = C>,
    should_retry: impl Fn(&anyhow::Error) -> bool,
    on_retry: impl Fn(&C, &anyhow::Error),
    mut attempt: impl FnMut(C) -> Fut,
) -> Result<T, anyhow::Error>
where Fut: Future<Output = Result<T, anyhow::Error>>;
```

Strategies are not allowed to invent their own retry loops. Either compose with `run_candidates` or surface the error to the pipeline.

### 5.4 Timeouts

Wall-clock budgets are layered:

1. Per-HTTP-request: `reqwest` connect/read timeouts (configured by the strategy's HTTP client).
2. Per-strategy: optional inner `tokio::time::timeout` if the strategy mixes HTTP + PTY + parsing.
3. Pipeline-level: the scheduler's `--web-timeout` (default 60s, surfaced as `ctx.web_timeout`).
4. Refresh-tick: the outer refresh loop bounds the parallel fan-out to "one tick interval minus a 10% buffer".

If a strategy exceeds budgets it MUST throw a typed `ProviderError::Timeout` so the pipeline can decide whether to fall back (see §13).

---

## 6. `ProviderRuntime` + `ProviderContext`

### 6.1 What gets injected (fetch time)

`ProviderFetchContext` (Mac: `ProviderFetchPlan.swift:20-58`) is the contract handed to every strategy. Windows ports it as a `struct ProviderFetchContext<'a>` with the same fields:

| Field | Type | Notes |
| --- | --- | --- |
| `runtime` | `Runtime` | `App` or `Cli`. Drives mode selection (e.g. Claude reorders strategies). |
| `source_mode` | `SourceMode` | User-selected mode (Auto/Web/Cli/OAuth/Api). |
| `include_credits` | `bool` | Whether to fetch the (slower) credits section. |
| `web_timeout` | `Duration` | Outer HTTP/WebView budget. |
| `web_debug_dump_html` | `bool` | Debug toggle. Strategies write captured HTML to `%LOCALAPPDATA%\CodexBar\dumps\` when true. |
| `verbose` | `bool` | Drives `tracing` filter at strategy level. |
| `env` | `BTreeMap<String, String>` | Effective environment, including token-account overrides (see §11). |
| `settings` | `Option<&ProviderSettingsSnapshot>` | Read-only snapshot of all provider-specific settings. |
| `http` | `Arc<HttpClient>` | Pre-configured reqwest client. |
| `cookies` | `Arc<CookieJar>` | Browser-imported + Keychain-cached cookies. |
| `claude_fetcher` | `Arc<dyn ClaudeUsageFetching>` | Shared Claude OAuth/PTY/Web façade (used by Codex when Claude OAuth credentials are needed; not the cleanest dependency but mirrors Mac). |
| `browser_detection` | `BrowserDetection` | Installed browsers + default order. |
| `pty` | `Arc<dyn PtyHost>` | ConPTY wrapper. |
| `keyring` | `Arc<dyn SecretStore>` | Windows Credential Manager. |
| `logger` | `tracing::Span` | Pre-scoped to `provider=<id> strategy=<id>`. |

The `ProviderInteractionContext` (Mac: task-local enum, `ProviderInteractionContext.swift:8-19`) is preserved as a `task_local!` `ProviderInteraction = { Background, UserInitiated }` plus `ProviderRefreshContext = { Regular, Startup }`. These flow through async tasks without polluting the context struct.

### 6.2 What gets injected (UI / lifecycle time)

`ProviderImplementation` callbacks receive smaller context structs (Mac: `ProviderContext.swift`):

| Context struct | Fields | Used by |
| --- | --- | --- |
| `ProviderPresentationContext` | `provider`, `settings`, `store`, `metadata` | `presentation()` |
| `ProviderAvailabilityContext` | `provider`, `settings`, `env` | `is_available()` |
| `ProviderSourceLabelContext` | `provider`, `settings`, `store`, `descriptor` | `default_source_label`, `decorate_source_label` |
| `ProviderSourceModeContext` | `provider`, `settings` | `source_mode()` |
| `ProviderVersionContext` | `provider`, `browser_detection` | `detect_version()` |
| `ProviderSettingsContext` | settings store + bindings + status text + confirmation + login flow | settings tab UI |
| `ProviderSettingsSnapshotContext` | `settings`, `token_override` | `settings_snapshot()` |
| `ProviderMenuUsageContext` | `provider`, `store`, `settings`, `metadata`, `snapshot` | menu builder |
| `ProviderMenuActionContext` | `provider`, `store`, `settings`, `account` | menu builder |
| `ProviderMenuLoginContext` | `provider`, `store`, `settings`, `account` | menu builder |
| `ProviderRuntimeContext` | `provider`, `settings`, `store` | `ProviderRuntime` hooks |

Keep them tiny and use them as the trait surface; do not pass `Settings` directly to every hook.

### 6.3 Threading

| Layer | Executor | Rule |
| --- | --- | --- |
| ProviderImplementation hooks | UI task (`tauri::async_runtime::spawn` on `main`) | `Send`, but assume serialization |
| Strategy `is_available` / `fetch` | tokio multi-thread runtime | must be `Send`, must not block |
| ProviderRuntime hooks | UI task (`MainActor` analogue) | same as ProviderImplementation |
| Cookie import / DPAPI | dedicated `tokio::task::spawn_blocking` | never on UI task |
| ConPTY | dedicated blocking task | one per session |

The trait declarations: `async fn` on the strategy + runtime, plain `fn` (sometimes `async fn` returning the future) on UI hooks.

---

## 7. Cookie source negotiation

### 7.1 Cookie source enum

```rust
pub enum CookieSource { Auto, Manual, Off }
```

`Auto` = let the framework import cookies from installed browsers in `BrowserCookieImportOrder`. `Manual` = read a Cookie header that the user pasted into settings. `Off` = do not attempt cookie-based fetches (degrades the provider to API-token-only).

`Sources/CodexBar/Providers/Shared/ProviderCookieSourceUI.swift:1-44` defines the UI options surface: `auto` is hidden when Keychain access is disabled by the user; `off` is offered only when the descriptor opts in (some providers refuse to allow disabling cookies). Windows mirrors this — `dpapi_disabled` is the analog of `keychain_disabled`.

### 7.2 Browser import order (Mac → Windows mapping)

Mac importer order in `ProviderBrowserCookieDefaults` (`Providers.swift:165-193`):

| Mac default | Browsers tried |
| --- | --- |
| `defaultImportOrder` | Safari → Chrome → Firefox → Chromium variants |
| `codexCookieImportOrder` | Safari, Chrome, Firefox first, then other Chromiums |
| `cursorCookieImportOrder` | Safari first (sessions live there), then Chromium |

Windows replacements (`rust/src/browser/import_order.rs`):

| Windows default | Browsers tried | Notes |
| --- | --- | --- |
| `default_import_order` | Edge → Chrome → Brave → Vivaldi → Opera → Firefox | DPAPI cookies for Chromium, SQLite for Firefox |
| `cursor_import_order` | Firefox first (Cursor users often pinned to it), then Chromium chain | Heuristic; revisit if telemetry says otherwise |
| `codex_import_order` | Edge → Chrome → Firefox, then remainder | Mirrors the "minimize Safe Storage prompts" intent — on Windows, minimize "Chromium key access" UAC-style nags |

Safari is gone. Manual cookie paste remains the universal escape hatch.

### 7.3 Negotiation flow

```text
ctx.source_mode == Web (or Auto resolving to Web):
   1. If settings.cookie_source == Off: skip web strategies entirely.
   2. If settings.cookie_source == Manual and settings.manual_cookie_header set:
        → strategy.fetch with that header.
   3. Else (Auto):
        a. If DPAPI is disabled:
             → return error with "Manual cookie required" hint.
        b. Check Keychain/CredMan cache (key: cookie.<provider>):
             if hit and not expired → use it.
        c. Else: iterate import_order, take first browser that yields a non-empty cookie for the cookie name(s) the strategy declared.
        d. On success: write back to CredMan cache (with timestamp + browser source label).
        e. On 401/403 from the strategy: invalidate cache, fall back to next browser.
```

The cache key + source label match Mac (`docs/CLAUDE.md` line 75: `com.steipete.codexbar.cache`, account `cookie.claude`). Windows uses CredMan service `CodexBar.cookie-cache`, target `<provider>.cookie`.

### 7.4 Cookie API surface

```rust
pub trait CookieApi: Send + Sync {
    async fn header_for(
        &self,
        domains: &[&str],
        cookie_names: &[&str],
        order: BrowserCookieImportOrder,
    ) -> Result<CookieResult, CookieError>;

    async fn cached(&self, provider: ProviderId) -> Option<CachedCookies>;
    async fn store(&self, provider: ProviderId, cookies: &CookieResult);
    async fn invalidate(&self, provider: ProviderId);
}
```

Strategies never reach into `rust/src/browser/` directly. The `CookieApi` is what they consume; the importers (`dpapi.rs`, `firefox.rs`) implement it.

---

## 8. OAuth / device-flow shared helpers

### 8.1 Where the shared logic lives

`rust/src/providers/oauth/` is a sub-module with:

| File | Owns |
| --- | --- |
| `device_flow.rs` | Generic PKCE + device-flow state machine. |
| `token_store.rs` | Encrypted token persistence (DPAPI blob in `%APPDATA%\CodexBar\oauth\<provider>.json.bin`). |
| `refresh.rs` | Background refresh task; one per logged-in provider. |
| `errors.rs` | Typed errors (`InvalidGrant`, `ExpiredRefresh`, `NeedsReauth`, …). |

### 8.2 Token model

```rust
pub struct OAuthCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub scope: Vec<String>,
    pub provider: ProviderId,
    pub source: TokenSource,  // CodexBarStored | ProviderCli | Environment
}
```

- `expires_at` triggers a proactive refresh 60s before expiry.
- `refresh_token` may be absent for short-lived web-cookie-derived tokens.
- `source` matters for prompt policies — Mac has Claude-specific keychain prompt cooldowns (`docs/refactor/claude-current-baseline.md`). Windows simplifies to a single rule: refresh tasks never prompt; user-initiated refresh may prompt; UI clearly distinguishes "needs re-auth" from "refreshing".

### 8.3 Per-provider OAuth providers (today)

| Provider | Flow | Notes |
| --- | --- | --- |
| Claude | OAuth code flow with proxy origin | `~/.claude/.credentials.json` fallback; CLI bootstrap; usage requires `user:profile` scope. |
| Codex | OAuth (ChatGPT-derived) | Auth file plus CodexBar-stored credential. |
| Gemini | OAuth via Gemini CLI credentials | Refresh-only; CodexBar does not initiate. |
| Vertex AI | gcloud ADC | Read-only; refresh delegated to gcloud. |
| Copilot | GitHub device flow | Multi-account; tokens injected via `COPILOT_API_TOKEN`. |
| Augment | Browser OAuth | Falls back to cookies. |

Each provider folder has an `oauth.rs` if it participates in OAuth; the shared `device_flow.rs` is generic enough to power all of them.

### 8.4 Expiry handling

A single `OAuthRefreshDaemon` runs in `tokio` and is the only caller of `device_flow::refresh`. Strategies request a `OAuthCredentials` snapshot through `ctx.keyring` / a `OAuthStore` handle; if the snapshot is `expires_within(60s)`, the strategy awaits a refresh and retries once. After the retry, errors are surfaced normally.

---

## 9. Settings descriptors

The Providers settings tab is **descriptor-driven**. No bespoke React per provider unless absolutely necessary.

### 9.1 Descriptor types (mirrors `ProviderSettingsDescriptors.swift:1-203`)

| Descriptor | Renders as | Key fields |
| --- | --- | --- |
| `Toggle` | Switch row with optional sub-actions | `id`, `title`, `subtitle`, `binding`, `status_text`, `actions`, `is_visible`, `on_change`, `on_app_did_become_active`, `on_appear_when_enabled` |
| `Field` | Text input (plain or secure) with optional footer + actions | `id`, `title`, `subtitle`, `footer_text`, `kind: Plain | Secure`, `placeholder`, `binding`, `actions`, `is_visible`, `on_activate` |
| `Picker` | Radio/dropdown with options + dynamic subtitle | `id`, `title`, `subtitle`, `dynamic_subtitle`, `binding`, `options`, `is_visible`, `is_enabled`, `on_change`, `trailing_text` |
| `ActionsRow` | Title/subtitle + N inline buttons | `id`, `title`, `subtitle`, `actions`, `is_visible` |
| `Action` | A single button (used inside Toggle/Field/ActionsRow) | `id`, `title`, `style: Bordered | Link`, `is_visible`, `perform` |
| `TokenAccounts` | Account picker + add/remove + per-account secret field | `id`, `title`, `subtitle`, `placeholder`, `provider`, `is_visible`, `accounts`, `active_index`, `set_active_index`, `add_account`, `remove_account`, `primary_add_action_title`, `primary_add_action`, `open_config_file`, `reload_from_disk` |
| `Confirmation` | Modal alert dispatched from a setting | `title`, `message`, `confirm_title`, `on_confirm` |

### 9.2 Rust shape

```rust
pub enum SettingsDescriptor {
    Toggle(ToggleDescriptor),
    Field(FieldDescriptor),
    Picker(PickerDescriptor),
    ActionsRow(ActionsRowDescriptor),
    TokenAccounts(TokenAccountsDescriptor),
}

pub struct ToggleDescriptor {
    pub id: &'static str,
    pub title: SmartStr,           // localized
    pub subtitle: SmartStr,
    pub binding: SettingsKey<bool>, // e.g. SettingsKey::ClaudeWebExtrasEnabled
    pub actions: Vec<ActionDescriptor>,
    pub status_text: Option<fn(&SettingsContext) -> Option<String>>,
    pub is_visible: Option<fn(&SettingsContext) -> bool>,
    pub on_change: Option<AsyncCallback<bool>>,
    pub on_appear_when_enabled: Option<AsyncCallback<()>>,
    pub on_app_did_become_active: Option<AsyncCallback<()>>,
}
```

`SettingsKey<T>` is a typed handle into the central `Settings` store; the React side binds via a `useSettingsKey(key)` hook. Strings are localized; both `serde::Serialize`-able for IPC.

### 9.3 Validators

Validation is per-descriptor:

- `Field` accepts an optional `validator: fn(&str) -> Result<(), ValidationError>`. Errors render under the field; the binding is not written until validation passes.
- `Picker` enforces option-set membership at the store layer.
- `Toggle` cannot be invalid — toggles are always coercible.
- Custom validators that need network IO (e.g. "does this token authenticate?") run via an `action` (e.g. "Test Token") rather than a synchronous validator.

### 9.4 Per-provider settings snapshot

The settings tab is what users edit, but the *fetcher* reads from `ProviderSettingsSnapshot`. Each `ProviderImplementation::settings_snapshot(ctx)` returns a `ProviderSettingsSnapshotContribution`; the builder folds contributions into a single immutable snapshot before each fetch. This is precisely Mac's `ProviderSettingsSnapshotBuilder` (`ProviderSettingsSnapshot.swift:470-565`) — port it as a Rust struct of `Option<<Provider>Settings>` per provider with a `Builder::apply(contribution)` method.

Critical invariant: **the snapshot is built once per fetch context and passed by reference into every strategy.** Strategies never query the live `Settings` mid-fetch — they read from the snapshot. This eliminates races between "user toggles a setting" and "in-flight fetch reads stale config".

---

## 10. Branding & presentation

### 10.1 Brand assets

Per provider, in `apps/desktop-tauri/src/assets/icons/`:

- `ProviderIcon-<id>.svg` — the brand mark. Resolved at runtime via `branding.icon_resource_name`.
- `ProviderIcon-<id>-dark.svg` (optional) — dark-theme variant.
- `ProviderIcon-<id>-mono.svg` — monochrome variant for tray rendering.

Mac uses `Assets.xcassets`; Windows uses raw SVGs imported by Vite.

### 10.2 Colors

`ProviderColor { red, green, blue }` (each 0..1) is the brand accent. Used by:

- Provider card top-border tint.
- Account switcher pill background.
- Tray icon indicator dot when `IconStyle::Combined` is active.
- Confetti/celebration palette (when "reset reached" celebrations are enabled).

Contrast guard: the renderer checks luminance and auto-darkens the color when used on a light surface; otherwise lightens.

### 10.3 Dashboard URL & "Buy" CTA

`metadata.dashboard_url` and `metadata.subscription_dashboard_url` produce two menu items: **Open dashboard** (always shown when set) and **Buy / Upgrade** (shown when set AND the credits gauge has gone red or zero). The "Buy" CTA is the only place the framework prescribes purchase nudging.

### 10.4 Detail line

`ProviderPresentation::detail_line(ctx)` returns the third line under the provider card title. Default: `"<cli_name> <version>"`. Overrides exist for providers without a CLI (e.g. Zai returns `"api"`, web-only providers return e.g. `"cookies · auto"`). This is the *only* per-provider string the implementation is asked to compute at render time — everything else is data.

### 10.5 Custom card variants

Some providers (MiniMax, Cursor request counts, Synthetic) have bespoke fields that don't fit the standard rate-window shape. Two patterns:

1. **Extension fields in `UsageSnapshot`** — preferred. `UsageSnapshot.minimax_usage`, `UsageSnapshot.cursor_requests`, etc. (Mac: `UsageFetcher.swift:86-95`). The default card renders these when present.
2. **Provider UI override** — the TS-side `ProviderUIOverrides` lets a provider replace just the body of its card with a bespoke React component. Use sparingly; this is the escape hatch that earns Phantom-wallet polish.

---

## 11. Token-account multi-account model

### 11.1 Data shape

```rust
pub struct ProviderTokenAccount {
    pub id: Uuid,
    pub label: String,
    pub token: String,              // secret; never logged
    pub added_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub external_identifier: Option<String>, // e.g. GitHub login
    pub organization_id: Option<String>,     // e.g. Anthropic org for Claude sessionKey
}

pub struct ProviderTokenAccountData {
    pub version: u32,
    pub accounts: Vec<ProviderTokenAccount>,
    pub active_index: usize,
}
```

Stored in `%APPDATA%\CodexBar\token-accounts.json` (DPAPI-wrapped if any account is secure). Mac stores at `~/Library/Application Support/CodexBar/token-accounts.json`. File-level perms restrict access.

### 11.2 Opt-in via `TokenAccountSupport`

A provider that wants multi-account adds a `token_account_support()` factory and a `TokenAccountSupport` entry:

| Field | Semantics |
| --- | --- |
| `title` | Section title in settings ("API tokens", "Session tokens", "GitHub accounts", …). |
| `subtitle` | Description below the section header. |
| `placeholder` | Input placeholder for new tokens. |
| `injection: TokenAccountInjection` | `CookieHeader` or `Environment(key)` — controls how the token is wired into `ctx.env`. |
| `requires_manual_cookie_source: bool` | If true, selecting an account forces `cookie_source = Manual`. |
| `cookie_name: Option<&'static str>` | If injection is CookieHeader and the raw token isn't a header, wrap as `<name>=<token>`. |

Catalog lives in `rust/src/providers/token_account_support.rs` and mirrors Mac's `TokenAccountSupportCatalog+Data.swift` literally — same provider set, same labels.

### 11.3 Per-account fetch dispatch

When the active account changes (either via menu switcher or settings picker):

1. `ProviderImplementation::apply_token_account_cookie_source(settings)` fires — may flip `cookie_source` to Manual.
2. `Settings.active_token_account(provider)` is updated.
3. Next fetch's `ProviderSettingsSnapshot` includes the new active account's `external_identifier` / `organization_id`.
4. `ctx.env` is augmented via `TokenAccountSupportCatalog::env_override(provider, token)` (Mac equivalent: `TokenAccountSupport.swift:38-53`).

Important: Claude has a special case — a token may be a `sessionKey` cookie OR an OAuth access token (`sk-ant-oat...`). `ClaudeCredentialRouting::resolve(...)` decides which path to take and the env override changes accordingly. This is **provider-specific routing inside a generic mechanism** and is the most subtle part of the model. Document it in the Claude per-provider spec; the framework only exposes the routing hook.

### 11.4 UI consequences

- Settings: `TokenAccountsDescriptor` renders the section.
- Menu: when multiple accounts exist, the provider's menu gets a "Switch account" submenu listing all accounts plus an "Add account" entry.
- Card: the active account's label appears under the provider name (if more than one account exists).

### 11.5 Threading + storage

Reads are synchronous (file is small). Writes go through a single `TokenAccountStore` actor (mpsc channel) so concurrent updates from multiple providers serialize cleanly.

---

## 12. Result models

Mac canonical models live in `Sources/CodexBarCore/UsageFetcher.swift` and `Sources/CodexBarCore/CreditsModels.swift`, `ProviderCostSnapshot.swift`, `OpenAIDashboardModels.swift`. Windows mirrors them in `rust/src/providers/models/`.

### 12.1 `RateWindow` (the fundamental unit)

```rust
pub struct RateWindow {
    pub used_percent: f64,             // 0..=100
    pub window_minutes: Option<u32>,   // total window duration; None when unknown
    pub resets_at: Option<DateTime<Utc>>,
    pub reset_description: Option<String>,    // for providers that only render text ("Resets tomorrow at 9am")
    pub next_regen_percent: Option<f64>,      // rolling-recovery providers
}

impl RateWindow {
    pub fn remaining_percent(&self) -> f64 { (100.0 - self.used_percent).max(0.0) }

    pub fn backfilling_reset_time(&self, cached: Option<&Self>, now: DateTime<Utc>) -> Self {
        if self.resets_at.is_some() { return self.clone(); }
        // If the live snapshot lacks resets_at, reuse a cached future reset.
        if let Some(c) = cached.filter(|c| c.resets_at.is_some_and(|t| t > now)) {
            return Self { resets_at: c.resets_at, window_minutes: self.window_minutes.or(c.window_minutes),
                          reset_description: self.reset_description.clone().or_else(|| c.reset_description.clone()),
                          ..self.clone() };
        }
        self.clone()
    }
}
```

`NamedRateWindow { id, title, window }` exists for providers that need to return arbitrary additional windows (search-hourly, monthly, custom plans).

### 12.2 `UsageSnapshot` (per provider, per refresh tick)

| Field | Type | Notes |
| --- | --- | --- |
| `primary` | `Option<RateWindow>` | Session / rate-limit / monthly — provider chooses. Drives the big number. |
| `secondary` | `Option<RateWindow>` | Weekly typically. |
| `tertiary` | `Option<RateWindow>` | Model-specific (Claude Opus weekly). |
| `extra_rate_windows` | `Option<Vec<NamedRateWindow>>` | Provider-specific extra gauges. |
| `provider_cost` | `Option<ProviderCostSnapshot>` | "Extra usage" spend/limit (Claude monthly overage etc.). |
| `zai_usage`, `minimax_usage`, `openrouter_usage`, `cursor_requests` | provider-specific structs | Optional bespoke fields. |
| `updated_at` | `DateTime<Utc>` | Time of the *fetch*, not the data freshness from upstream. |
| `identity` | `Option<ProviderIdentitySnapshot>` | Scoped (see §2.2). |

### 12.3 `CreditsSnapshot`

```rust
pub struct CreditEvent { pub id: Uuid, pub date: DateTime<Utc>, pub service: String, pub credits_used: f64 }
pub struct CreditsSnapshot { pub remaining: f64, pub events: Vec<CreditEvent>, pub updated_at: DateTime<Utc> }
```

Used by providers that expose a balance/usage-history view (OpenAI dashboard, OpenRouter, Codebuff, Crof, Venice, …).

### 12.4 `ProviderCostSnapshot`

```rust
pub struct ProviderCostSnapshot {
    pub used: f64,
    pub limit: f64,
    pub currency_code: String,           // ISO-4217
    pub period: Option<String>,          // "Monthly" usually
    pub resets_at: Option<DateTime<Utc>>,
    pub next_regen_amount: Option<f64>,  // rolling recovery
    pub updated_at: DateTime<Utc>,
}
```

### 12.5 `ProviderStorageFootprint`

Local on-disk footprint (Claude logs, Codex sessions). Drives the "Cleanup" section of settings. Mac: `ProviderStorageFootprint.swift:1-499`. Windows ports verbatim, swapping path candidates (`%LOCALAPPDATA%\Anthropic\Claude`, `%USERPROFILE%\.claude\projects`, etc.). The `cleanup_recommendations` heuristic is identical (manual cleanup only, no automated deletion).

### 12.6 `OpenAIDashboardSnapshot`

Codex/OpenAI carry an extra payload from the web dashboard (`OpenAIDashboardModels.swift`): signed-in email, code-review remaining %, credit events history, daily breakdown, usage breakdown, primary/secondary limits, credits remaining, account plan. This is wired through `ProviderFetchResult.dashboard` and surfaced in the Codex card's expandable region.

### 12.7 `ProviderFetchResult`

```rust
pub struct ProviderFetchResult {
    pub usage: UsageSnapshot,
    pub credits: Option<CreditsSnapshot>,
    pub dashboard: Option<OpenAIDashboardSnapshot>,
    pub source_label: String,         // "openai-web", "claude", "oauth", "api", "local", "cli", etc.
    pub strategy_id: String,
    pub strategy_kind: FetchKind,
}
```

`FetchKind = { Cli, Web, OAuth, ApiToken, LocalProbe, WebDashboard }`.

### 12.8 `StatusSnapshot`

```rust
pub struct StatusSnapshot {
    pub state: StatusState,           // Operational | Degraded | Major | Maintenance | Unknown
    pub last_incident: Option<Incident>,
    pub fetched_at: DateTime<Utc>,
    pub source: StatusSource,         // Statuspage | GoogleWorkspace | None
}
```

Status polling is owned by `rust/src/status.rs`, not by the provider strategy pipeline — but the `metadata.status_page_url` field decides whether the status badge renders on the card.

---

## 13. Error model

### 13.1 Typed errors

```rust
pub enum ProviderError {
    Timeout,
    Network(reqwest::Error),
    Unauthorized { reason: String },
    PermissionDenied { reason: String },
    NoCookies { tried: Vec<String> },
    NoToken { hint: String },
    ParseError { context: String, cause: anyhow::Error },
    UpstreamError { code: u16, body_excerpt: String },
    PluginUnavailable { reason: String },
    UserConfigInvalid { field: String, reason: String },
    Cancelled,
}

pub enum ProviderFetchError {
    NoAvailableStrategy(ProviderId),
    StrategyFailed { strategy: String, error: ProviderError },
}
```

Each provider folder may add a provider-local error enum that converts into `ProviderError` (preserving cause) when surfaced.

### 13.2 Where errors surface

| Surface | Behavior |
| --- | --- |
| Tray icon | Dimmed icon, no badge — single visual for "something is wrong". |
| Provider card | Inline pill ("Cookies expired"), with a "Fix" action linking to the relevant setting or login flow. Color: `branding.color` desaturated. |
| Menu | Last error string with "Copy error" submenu item. |
| Console / log | Full structured `tracing` event (`level=warn`, `provider=<id>`, `strategy=<id>`, `error_kind=<variant>`). Secrets redacted. |
| CLI `--verbose` | Prints the `attempts` list in order with their `error_description`. |

### 13.3 Retry decisions

- `Timeout` → strategy may say `should_fallback = true`.
- `Network` (5xx, connection reset) → fallback.
- `Unauthorized` → **terminal** by default; fallback only if the next strategy has an entirely different auth source.
- `NoCookies` / `NoToken` → fallback (this strategy can't run, but another might).
- `PluginUnavailable` → fallback.
- `ParseError` → terminal (something changed upstream; we want loud failure, not silent fallback). 
- `UserConfigInvalid` → terminal; surface to settings UI.

`shouldFallback` is implemented per-strategy — these are defaults the strategy may override.

---

## 14. CLI integration contract

### 14.1 `codexbar usage --provider <id>`

The CLI peer (`codexbar.exe`) shares the same `codexbar` crate. The contract:

| Sub-command | Output |
| --- | --- |
| `codexbar usage` | JSON: `{ providers: [{ id, snapshot, attempts, source_label }] }` for all enabled providers. |
| `codexbar usage --provider <id>` | Same shape, single entry. |
| `codexbar usage --provider <id> --source <mode>` | Runs with overridden `source_mode`. |
| `codexbar usage --verbose` | Adds `attempts` (every strategy + outcome + error). |
| `codexbar cost --provider <id>` | Local-log cost scan output (Claude/Codex). |
| `codexbar status` | Status snapshots. |
| `codexbar providers` | Lists registered providers (id, display name, cli aliases, source modes). |

The `cli_name_map` (`ProviderDescriptor.swift:138-148`) feeds `--provider` argument parsing; aliases let users type either `codex` or `chatgpt-codex` (etc.). Same on Windows.

### 14.2 Runtime selection

CLI sets `ctx.runtime = Runtime::Cli`. Some providers reorder strategies on CLI (Claude prefers Web > CLI on CLI runtime per `docs/refactor/claude-current-baseline.md`). Strategy ordering is the strategy's responsibility, gated by `ctx.runtime`.

### 14.3 Configuration sharing

CLI reads the same `%APPDATA%\CodexBar\config.json` as the desktop app. Token accounts come from the same `token-accounts.json`. No second source of truth.

---

## 15. Widget snapshot contract (future)

Mac ships a `Sources/CodexBarWidget` extension that consumes a shared snapshot file. Windows v1 drops widgets, but the **data contract** must be preserved so a future Windows-widget shell (e.g. a Windows 11 widget board pinned tile or the new TaskbarExtension API) can adopt it without re-plumbing.

### 15.1 Snapshot file

Path: `%APPDATA%\CodexBar\widget-snapshot.json`.

```json
{
  "version": 1,
  "generated_at": "2026-05-12T14:32:17Z",
  "providers": [
    {
      "id": "claude",
      "display_name": "Claude",
      "primary": { "used_percent": 73.2, "resets_at": "...", "label": "Session" },
      "secondary": { "used_percent": 22.0, "resets_at": "...", "label": "Weekly" },
      "tertiary": null,
      "credits": { "remaining": 12.4, "currency": "USD" },
      "status": "Operational",
      "stale": false,
      "icon_resource": "ProviderIcon-claude"
    }
  ]
}
```

The desktop app writes this file atomically after every successful refresh. Stale detection is `now - generated_at > 2 * refresh_cadence`. Consumers MUST NOT write to the file. The contract is versioned; bumping `version` is a breaking change.

---

## 16. Macro vs hand-written registry: decision recap

Already covered in §3, but a one-pager for reviewers:

| Question | Answer |
| --- | --- |
| Do we need a compile-time macro? | No. The Swift macros existed mostly to add file-scope side-effects to populate a registry. `inventory!` does that natively in Rust. |
| Do we need a build.rs codegen? | No, but it's an acceptable fallback. |
| What if a provider forgets to register? | Validation in `ProviderCatalog::build()` checks the registry against `ProviderId::ALL` and panics with a clear message at startup. We do **not** rely on `enum ProviderId` mirroring the registry; instead, we drop the `ProviderId::ALL` requirement and let the registry itself be the source of truth (closer to Mac's `UsageProvider.allCases`, but discovered, not asserted). |
| Can two providers share an id? | No — validated at startup. |
| Can we add a provider in one PR? | Yes — add one folder under `rust/src/providers/`, one `mod.rs` line, optionally one folder under `apps/desktop-tauri/src/providers/`, one row in `token_account_support.rs` if multi-account, one icon asset, one localized string set, one docs page. No grep-and-add. |
| Recommended crate | `inventory = "0.3"`. |
| Optional second crate | `linkme = "0.3"` if `inventory` ever breaks on a target. |

---

## 17. Provider implementation contract (Rust trait)

The Mac `ProviderImplementation` protocol is ported as:

```rust
#[async_trait]
pub trait ProviderImplementation: Send + Sync {
    fn id(&self) -> ProviderId;
    fn supports_login_flow(&self) -> bool { false }

    fn presentation(&self, ctx: &ProviderPresentationContext) -> ProviderPresentation;
    fn observe_settings(&self, settings: &SettingsStore);
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
    fn apply_token_account_cookie_source(&self, settings: &SettingsStore) {}

    fn append_usage_menu_entries(&self, ctx: &MenuUsageContext, entries: &mut Vec<MenuEntry>) {}
    fn append_action_menu_entries(&self, ctx: &MenuActionContext, entries: &mut Vec<MenuEntry>) {}
    fn login_menu_action(&self, ctx: &MenuLoginContext) -> Option<(String, MenuAction)> { None }

    async fn run_login_flow(&self, ctx: &LoginContext) -> bool { false }
}
```

`ProviderRuntime` is its own trait (see §6.3).

### 17.1 The host services contract

Strategies and implementations both consume the same `Host` services. Mirror `docs/provider.md:71-82`:

| Service | Trait | Implementation |
| --- | --- | --- |
| Settings store | `SettingsStore` | serde-backed JSON + watchers |
| Secret store | `SecretStore` | `keyring` (CredMan) |
| Browser cookies | `CookieApi` | DPAPI + SQLite |
| HTTP | `HttpClient` | reqwest + rustls |
| PTY | `PtyHost` | portable-pty (ConPTY) |
| Tokens | `TokenResolver` | env + auth-file + cookie-derived |
| Status | `StatusApi` | Statuspage + Google Workspace pollers |
| Logger | `tracing::Span` | redacted; never logs raw tokens |

Strategies must declare which services they need by accepting them as `&dyn Trait` from `ProviderFetchContext`. They MUST NOT reach into platform-specific APIs (`windows::Win32::*`) directly — only the host service implementations do.

---

## 18. Provider folder template

The Mac authoring guide (`docs/provider.md:93-154`) lists the structure. Windows equivalent:

```
rust/src/providers/<id>/
  mod.rs                      // pub use {descriptor::descriptor, ui::*}; inventory::submit!
  descriptor.rs               // fn descriptor() -> ProviderDescriptor { ... }
  strategies.rs               // impl Strategy for FooCliStrategy { ... }
  fetcher.rs / probe.rs       // network/PTY/parser logic
  models.rs                   // serde structs for upstream JSON
  parser.rs                   // optional, for HTML/text parsers
  ui.rs                       // impl ProviderImplementation for FooUI
  oauth.rs                    // optional, if provider has OAuth
  tokens.rs                   // optional, for TokenAccountSupport + token resolver
  errors.rs                   // optional, for provider-local error enum
```

```
apps/desktop-tauri/src/providers/<id>/
  index.tsx                   // optional UI overrides
```

### 18.1 Minimal Rust example (the Windows analog of `docs/provider.md:95-154`)

```rust
// rust/src/providers/example/descriptor.rs
pub fn descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: ProviderId::Example,
        metadata: ProviderMetadata {
            display_name: "Example",
            session_label: "Session",
            weekly_label: "Weekly",
            cli_name: "example",
            default_enabled: false,
            ..Default::default()
        },
        branding: ProviderBranding {
            icon_style: IconStyle::Codex,
            icon_resource_name: "ProviderIcon-example",
            color: ProviderColor::new(0.2, 0.6, 0.8),
        },
        fetch_plan: ProviderFetchPlan {
            source_modes: SourceMode::Auto | SourceMode::Cli,
            pipeline: ProviderFetchPipeline::new(|_ctx| vec![Box::new(ExampleCliStrategy)]),
        },
        cli: ProviderCLIConfig { name: "example", aliases: &[], version_detector: None },
        token_cost: ProviderTokenCostConfig::unsupported(|| "Example cost is not available.".into()),
    }
}
```

```rust
// rust/src/providers/example/mod.rs
mod descriptor; mod strategies; mod ui;
inventory::submit! {
    ProviderRegistration {
        descriptor: descriptor::descriptor,
        ui_factory: || Box::new(ui::ExampleUI),
        token_account_support: None,
    }
}
```

```rust
// rust/src/providers/example/strategies.rs
pub struct ExampleCliStrategy;
#[async_trait]
impl Strategy for ExampleCliStrategy {
    fn id(&self) -> &'static str { "example.cli" }
    fn kind(&self) -> FetchKind { FetchKind::Cli }
    async fn is_available(&self, _ctx: &ProviderFetchContext<'_>) -> bool { true }
    async fn fetch(&self, _ctx: &ProviderFetchContext<'_>) -> Result<ProviderFetchResult, ProviderError> {
        let now = Utc::now();
        let usage = UsageSnapshot {
            primary: Some(RateWindow { used_percent: 0.0, ..Default::default() }),
            updated_at: now, ..Default::default()
        };
        Ok(ProviderFetchResult {
            usage, credits: None, dashboard: None,
            source_label: "cli".into(), strategy_id: self.id().into(), strategy_kind: self.kind(),
        })
    }
    fn should_fallback(&self, _err: &ProviderError, _ctx: &ProviderFetchContext<'_>) -> bool { false }
}
```

Twelve files become twelve files; no `mod.rs` chains beyond the provider's own.

---

## 19. Provider catalog: ordering & enumeration

### 19.1 Enumeration order

The Mac `UsageProvider` enum is `Sendable, Codable, allCases` (`Providers.swift:5-45`). The order is stable because `ProviderDescriptorRegistry.ordered` records insertion order from the seed map (`ProviderDescriptor.swift:48-117`).

On Windows, the catalog records inventory iteration order, which is **link-order-dependent**. To stabilize, we sort by `metadata.display_name` after collection, with `is_primary_provider == true` providers floated to the top in a stable secondary sort. This makes the order *deterministic* even if `inventory!` order changes. The result is what powers:

- The settings provider list.
- The icon-stack ordering when `IconStyle::Combined` is on.
- CLI `codexbar providers` output.

### 19.2 Primary provider

`metadata.is_primary_provider = true` is the new home for what used to be hardcoded "Codex/Claude special cases" (per `docs/refactor/macros.md` point 4). Use the flag to drive:

- Default selection in the popup.
- Tray icon when only one provider is visible.
- Onboarding focus.

---

## 20. Acceptance checklist

A new provider is "done" only when *all* of the following are checked. The framework should make these mechanical, not creative.

### Code
- [ ] `rust/src/providers/<id>/` folder created with `descriptor.rs`, `strategies.rs`, `ui.rs`, `mod.rs`, `models.rs`, and any of `fetcher.rs` / `parser.rs` / `oauth.rs` / `tokens.rs` as needed.
- [ ] `descriptor()` returns a fully populated `ProviderDescriptor` with `default_enabled`, `cli_name`, `display_name`, `session_label`, `weekly_label`, `icon_resource_name`, and either `dashboard_url` or a documented reason to omit.
- [ ] At least one `Strategy` impl that returns a valid `UsageSnapshot` from `fetch`.
- [ ] `inventory::submit!` block in `mod.rs`.
- [ ] `ProviderImplementation` impl provided (even if all defaults are kept) so `ProviderImplementationRegistry` can resolve it.

### Settings + UI
- [ ] Settings snapshot contribution declared if the provider has any toggles/fields/pickers.
- [ ] Token-account support entry added if the provider supports multiple accounts.
- [ ] All strings in `apps/desktop-tauri/src/locale/en.json` (and other locales if shipping).
- [ ] Icon SVG at `apps/desktop-tauri/src/assets/icons/ProviderIcon-<id>.svg` (light + optional dark + mono).
- [ ] Brand color chosen and contrast-checked.

### Tests
- [ ] Snapshot mapping unit tests (`cargo test -p codexbar --lib providers::<id>::tests`).
- [ ] Strategy availability + fallback tests.
- [ ] Parser tests with at least one captured fixture under `rust/tests/fixtures/<id>/`.
- [ ] CLI alias resolution test (`codexbar usage --provider <id>` and any alias).
- [ ] Registry completeness test asserting `descriptor()` does not panic and round-trips through serde.

### Docs
- [ ] Per-provider doc at `docs/windows/spec/providers/<id>.md` describing data source, auth, endpoints, parsing.
- [ ] Entry in `docs/providers.md` summary table.
- [ ] Entry in `apps/desktop-tauri/src/providers/index.ts` if TS overrides exist.

### Polish
- [ ] Card renders with no `null`/empty fields in the default state.
- [ ] Login flow (if any) ends with a focused popup + a tray pulse animation.
- [ ] Refresh failure surfaces a one-tap "Fix" action that goes somewhere useful (settings / login).
- [ ] Empty state has a non-generic message — no "no data".
- [ ] Reset-time copy ("Resets in 4h 12m") localizes correctly and never says "in -1m".

### Architectural smells to actively fight
- [ ] No provider id mentioned by name anywhere outside `providers/<id>/`, `locale/`, and `assets/`. Grep for `id == "<id>"` or `ProviderId::<Name>` and explain every hit.
- [ ] No `unwrap()` in strategy code; every error is a typed `ProviderError`.
- [ ] No raw `tokio::time::sleep` inside fetch paths (use deadlines).
- [ ] No `Settings` access mid-fetch — always read from `ProviderSettingsSnapshot`.
- [ ] No identity field reads from another provider's snapshot.

---

## 21. Open questions

These are the spots where the Mac code is intentionally vague and the Windows port needs a product decision before merging:

1. **Per-provider polling cadence.** Mac uses a single global cadence. Some providers (Antigravity local probe, Ollama local) can refresh every 5s with zero cost; others (Vertex AI Cloud Monitoring) cost real money. Decide at v1: keep global, or add `metadata.preferred_min_cadence: Duration`.
2. **Failure backoff.** Today, a failing provider re-runs its full pipeline every tick. Add an opt-in exponential backoff per provider with a manual "Try again" override?
3. **Token-account routing for Claude.** The OAuth-vs-cookie disambiguation is provider-specific but lives in shared code. Document whether to keep it shared (today) or fold it into `claude::tokens`.
4. **`ProviderRuntime` for non-Codex/Claude providers.** Does any other provider need long-lived background state? Cursor's stored WebKit session is a candidate.
5. **Card variant API.** Define the smallest possible TS override surface: full body replacement, or slot-based (header / body / footer)?

These should be resolved either in this doc or in `docs/windows/spec/35-provider-decisions.md` (TBD) before the first Cursor + Claude + Codex providers land.

---

## 22. Glossary

| Term | Meaning |
| --- | --- |
| Descriptor | Single immutable struct describing a provider's identity, branding, fetch plan, CLI metadata, token cost support. |
| Pipeline | Ordered list of strategies that the fetch plan runs. |
| Strategy | A concrete way to obtain usage (CLI, web cookies, OAuth, API token, local probe, web dashboard). |
| Implementation | The UI-side hook bundle for a provider (settings, menu, login). |
| Runtime | Optional long-lived per-provider actor for sessions/refresh daemons. |
| Snapshot | An immutable `UsageSnapshot` written into the `UsageStore` per refresh. |
| Settings snapshot | An immutable per-fetch view of *all* provider-specific settings. |
| Token account | A user-provided credential set; provider declares shape via `TokenAccountSupport`. |
| Source mode | The fetch path the user has selected (Auto, Web, CLI, OAuth, Api). |
| Cookie source | How web-strategy cookies are obtained (Auto from browsers, Manual paste, Off). |
| FetchKind | Categorization of how the strategy works (Cli / Web / OAuth / ApiToken / LocalProbe / WebDashboard). |
