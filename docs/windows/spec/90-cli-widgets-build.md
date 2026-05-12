---
summary: "Spec for the CLI binary, widget contract, watchdog + web probe helpers, localization, and the full build/release/sign pipeline. Mac surfaces mapped to Windows / Tauri 2 + React + shared Rust crate."
read_when:
  - "Implementing the codexbar.exe CLI"
  - "Considering shipping widgets on Windows (deferred)"
  - "Porting the Claude PTY watchdog or web-probe helper"
  - "Setting up Windows build/sign/release pipeline"
  - "Wiring localization and language picking"
---

# 90 — CLI / Widgets / Watchdog / Web Probe / Build & Release / Localization

This document maps five adjacent macOS subsystems onto the Tauri 2 + React + shared Rust crate Windows target:

1. The standalone `codexbar` CLI (full feature surface).
2. The WidgetKit extension (data contract only — *not shipped on Windows in v1*).
3. The Claude PTY watchdog + Claude web-probe diagnostic helper.
4. Localization (xcstrings → key-string file pairs).
5. The build / package / sign / notarize / appcast / icon pipeline.

Each section ends with concrete Windows mappings and acceptance criteria a Rust/TS engineer can hand-verify without reading Swift.

---

## A. CLI binary (`codexbar`)

The Mac CLI is a SwiftPM `executableTarget` named `CodexBarCLI`, distributed three ways:

1. Bundled inside the app at `CodexBar.app/Contents/Helpers/CodexBarCLI` and symlinked to `/usr/local/bin/codexbar` and `/opt/homebrew/bin/codexbar` via the in-app *Preferences → Advanced → Install CLI* command (delegates to `bin/install-codexbar-cli.sh`).
2. Standalone tarballs on GitHub Releases (`CodexBarCLI-v<tag>-macos-arm64.tar.gz`, `…-macos-x86_64.tar.gz`, plus Linux x86_64 / aarch64 — there is already cross-compilation for Linux).
3. Homebrew formula `steipete/tap/codexbar` (CLI-only, separate from the cask).

The CLI imports `CodexBarCore` and the in-house `Commander` (`steipete/Commander 0.2.x`) argument parser. Logging is `swift-log`. Everything else is `Foundation`.

### A.1 Command tree

Source: `Sources/CodexBarCLI/CLIEntry.swift`, `CLIHelp.swift`, `CLIOptions.swift`, `CLICostCommand.swift`, `CLIConfigCommand.swift`, `CLICacheCommand.swift`.

| Path | Aliases / defaults | What it does |
|---|---|---|
| `codexbar` (no subcommand) | default subcommand is `usage`; arguments that begin with `-` are forwarded to `usage` | Print usage as text or JSON. |
| `codexbar usage` | — | Same as no-subcommand. |
| `codexbar cost` | — | Print local cost usage (Claude + Codex log scan, plus pi sessions). |
| `codexbar config` | default subcommand: `validate` | Container only. |
| `codexbar config validate` | — | Validate `~/.codexbar/config.json`; report issues with severity. |
| `codexbar config dump` | — | Print normalized config JSON. |
| `codexbar cache` | default subcommand: `clear` | Container only. |
| `codexbar cache clear` | requires one of `--cookies` / `--cost` / `--all` | Clear browser-cookie cache (Keychain) and/or cost-usage cache directory. |

There is no separate `status` subcommand — provider status is opt-in via `usage --status`. There is no `cost --provider …` flag in the form the prompt mentions; `cost` does support `--provider <id|all>`, and silently skips providers other than `claude` and `codex` (with a stderr notice unless `--json-only` is set).

### A.2 Flags (full set)

#### A.2.1 Global flags (every subcommand)

| Flag | Type | Default | Description |
|---|---|---|---|
| `-h`, `--help` | flag | — | Print help for the resolved command (root, `usage`, `cost`, `config`, or `cache`) and exit 0. |
| `-V`, `--version` | flag | — | Print `CodexBar <version>` and exit 0. Version resolution: `Bundle.main` `CFBundleShortVersionString` → walk parent dirs for `.app/Contents/Info.plist` → adjacent `VERSION` file next to the executable. |
| `-v`, `--verbose` | flag | `false` | Sets log level to `debug` unless `--log-level` overrides. |
| `--log-level <trace\|verbose\|debug\|info\|warning\|error\|critical>` | option | `error` (or `debug` if `-v`) | Direct log-level override. |
| `--json-output` | flag | `false` | Emit JSONL logs to **stderr**. Does *not* change stdout format. |
| `--format <text\|json>` | option | `text` | Stdout format. |
| `--json` | flag | — | Shortcut for `--format json`. Empty help text (hidden synonym). |
| `--json-only` | flag | `false` | Emit JSON only; suppress all other stdout. Errors become JSON payloads on stdout. Implies `--format json`. |
| `--pretty` | flag | `false` | Pretty-print JSON with sorted keys. |
| `--no-color` | flag | `false` | Disable ANSI in text output. Also honors `TERM=dumb` and non-TTY stdout. |

#### A.2.2 `usage` flags

| Flag | Type | Default | Notes |
|---|---|---|---|
| `--provider <id\|both\|all>` | option | Derived from enabled providers in config (see A.4) | `id` is one of the CLI names in `ProviderDescriptorRegistry.cliNameMap` (codex, claude, gemini, cursor, copilot, kilo, …). `both` = the two providers in `ProviderDescriptorRegistry.all.filter(isPrimaryProvider)` (Codex + Claude). `all` = every registered provider. |
| `--account <label>` | option | — | Token-account label match (case-insensitive). Requires single provider. |
| `--account-index <n>` | option | — | 1-based index. Requires single provider. Errors if `<= 0`. |
| `--all-accounts` | flag | `false` | Fetch every token account for the provider. Cannot combine with `--account`/`--account-index`. |
| `--no-credits` | flag | `false` | Skip the "Credits" line in Codex text output. Ignored for `--format json`. |
| `--status` | flag | `false` | Fetch the provider status page (`statuspage.io` `api/v2/status.json` schema) and include indicator + description. |
| `--source <auto\|web\|cli\|oauth\|api>` | option | from config (`config.providers[*].source`) or `auto` | Per-provider data source. On non-macOS, `web` and `auto` for web-requiring providers exit non-zero. |
| `--web` | flag | — | Alias for `--source web`. |
| `--web-timeout <seconds>` | option | `60` | Codex web fetch timeout. |
| `--web-debug-dump-html` | flag | `false` | Dumps Codex dashboard HTML snapshots to `/tmp` on missing data. |
| `--antigravity-plan-debug` | flag | `false` | Print Antigravity `planInfo` fields to stderr. |
| `--augment-debug` | flag | `false` | Print Augment raw API responses to stderr (macOS only — guarded by `#if os(macOS)`). |

#### A.2.3 `cost` flags

| Flag | Type | Default | Notes |
|---|---|---|---|
| `--provider <id\|both\|all>` | option | Enabled providers | Only `claude` and `codex` actually return data. Other providers are silently skipped with a stderr notice ("Skipping providers without local cost usage: …") unless `--json-only`. |
| `--refresh` | flag | `false` | Ignore cache (`~/Library/Caches/CodexBar/cost-usage/…`) and rescan from logs. |

#### A.2.4 `config validate` / `config dump`

No additional flags beyond the global set.

#### A.2.5 `cache clear`

| Flag | Type | Default | Notes |
|---|---|---|---|
| `--cookies` | flag | `false` | Clear Keychain-cached browser cookie headers (`com.steipete.codexbar.cache`). |
| `--cost` | flag | `false` | Delete cost-usage cache dir. |
| `--all` | flag | `false` | Both. |
| `--provider <id>` | option | — | Scope `--cookies` to one provider. Combining `--provider` with `--cost` or `--all` is an error. |

At least one of `--cookies` / `--cost` / `--all` is required; otherwise the CLI exits non-zero with "Specify --cookies, --cost, or --all."

### A.3 Output formats

**Text** (`--format text`, default): A header line (`== <ProviderDisplayName> <version> (<source>) ==`), then 1–3 rate-window rows with usage bars (`[========----]`), reset lines (`Resets …`), optional pace line ("On pace" / "X% in deficit" / "X% in reserve"), optional credits, account, plan, notes. ANSI 38-color uses 31 (red, <10% remaining), 33 (yellow, <25%), 32 (green); cyan-bold (1;36) for cost header, magenta (95) for labels, gray (90) for reset/subtle lines.

**JSON** (`--format json` or `--json` or `--json-only`): An array of `ProviderPayload` objects. Each payload has:

| Field | Type | Notes |
|---|---|---|
| `provider` | string | Provider raw id (`"codex"`, `"claude"`, …) or `"cli"` for CLI-level errors. |
| `account` | string? | Token-account label, when one was used. |
| `version` | string? | Provider CLI version, normalized to first `\d+(\.\d+)+` match. |
| `source` | string | One of `openai-web`, `web`, `oauth`, `api`, `local`, `cli`, or a provider-specific label. |
| `status` | object? | When `--status`. `{indicator, description, updatedAt, url}`. Indicator enum: `none|minor|major|critical|maintenance|unknown`. |
| `usage` | object? | `UsageSnapshot` — `primary`, `secondary`, `tertiary` (each a `RateWindow{usedPercent, windowMinutes, resetsAt, resetDescription}`), `identity`, `providerCost`, etc. See provider docs. |
| `credits` | object? | `{remaining, updatedAt}` (Codex only). |
| `antigravityPlanInfo` | object? | When Antigravity + `--antigravity-plan-debug`. |
| `openaiDashboard` | object? | Cached OpenAI web dashboard (signed-in email, code-review remaining %, credit events, daily breakdown, usage breakdown). |
| `error` | object? | `{code, message, kind}` where `kind ∈ {args, config, provider, runtime}` and `code` mirrors the process exit code. |

`cost --format json` emits a different payload (`CostPayload`):

```
{ provider, source, updatedAt,
  sessionTokens, sessionCostUSD,
  last30DaysTokens, last30DaysCostUSD,
  daily: [{date, inputTokens, outputTokens,
           cacheReadTokens, cacheCreationTokens,
           totalTokens, totalCost,
           modelsUsed, modelBreakdowns: [{modelName, cost, totalTokens}]}],
  totals: {inputTokens, outputTokens,
           cacheReadTokens, cacheCreationTokens,
           totalTokens, totalCost},
  error }
```

`config validate --format json` emits the issues array directly (no wrapper). `cache clear --format json` emits a `[{cache, provider, cleared, error?}]` array.

JSON encoding: `JSONEncoder` with `dateEncodingStrategy = .iso8601`; pretty mode uses `[.prettyPrinted, .sortedKeys]`.

There is no `--format summary` or table mode. The text renderer always emits the verbose multi-line block; the JSON form is the only machine-readable shape.

### A.4 Provider-selection defaults

`CodexBarCLI.providerSelection(rawOverride:, enabled:)` resolves the default `--provider` when none is given:

1. Explicit `--provider <x>` always wins.
2. If exactly 2 enabled providers and both are `metadata.isPrimaryProvider` (Codex + Claude) → `.both`.
3. Else if 2 enabled → custom list.
4. Else if 3+ enabled → custom list (do *not* expand to `.all`).
5. Else if 1 enabled → single provider.
6. Else (no config / empty enabled) → `.single(.codex)`.

### A.5 Exit codes

`Sources/CodexBarCLI/CLIExitCode.swift`:

| Code | Meaning |
|---|---|
| `0` | Success. |
| `1` | Generic failure (default for unmapped errors). |
| `2` | `binaryNotFound`: provider CLI not installed (Codex/Claude/Gemini not on PATH). |
| `3` | `parseError`: parse/format failure — `ClaudeUsageError.parseFailed`, `UsageError.decodeFailed`, etc. |
| `4` | `timeout`: `TTYCommandRunner.Error.timedOut`, `CostUsageError.timedOut`, etc. |

`mapError(_:)` (in `CLIHelpers.swift`) is the canonical mapping.

### A.6 Config sharing with the GUI

- Single source of truth: `~/.codexbar/config.json` (loaded via `CodexBarConfigStore.load()`).
- Both the app and the CLI read the same file; the CLI is read-only with respect to the config (only `cache clear` writes — and only to caches, not config).
- Token accounts live in the same file under `providers[*].tokenAccounts[]` and are resolved identically.
- Cookie sources: in addition to `config.providers[*].cookieSource`, the CLI also reads cached browser-cookie headers from the Keychain (`com.steipete.codexbar.cache`).

### A.7 CLI-runtime vs app-runtime source selection (Claude)

This is a deliberate divergence from the GUI app. From `docs/CLAUDE.md`:

- App runtime main pipeline: **OAuth API → CLI PTY → Web API.**
- CLI runtime main pipeline: **Web API → CLI PTY.** (No OAuth fallback in pure CLI; OAuth is only used when a token-account `sk-ant-oat...` token is explicitly selected.)
- Explicit picker (`--source oauth|web|cli|api`) bypasses fallback in either runtime.

`ProviderFetchContext(runtime: .cli, …)` is what flips the in-core decision tree. Windows must preserve this distinction — the CLI is *not* allowed to silently borrow OAuth tokens from the desktop app's secret store unless a token account explicitly references them.

### A.8 Logging bootstrap

`CodexBarCLI.bootstrapLogging(values:)` chooses:

- Destination: `stderr`.
- JSON mode: `--json-output` or `--json-only` set.
- Level: `--log-level` if parseable, else `verbose ? .debug : .error`.

`CodexBarLog.bootstrapIfNeeded(…)` is the single entry point — no `print()` for log output.

### A.9 Windows mapping (CLI)

| Concern | Mac (current) | Windows (target) |
|---|---|---|
| Workspace | SwiftPM `executableTarget("CodexBarCLI")` | Cargo workspace bin target `codexbar` (in `rust/`); same crate as the core, gated by `[[bin]]`. |
| Binary location | `CodexBar.app/Contents/Helpers/CodexBarCLI` | `%ProgramFiles%\CodexBar\codexbar.exe` (next to the desktop app); installer adds `%ProgramFiles%\CodexBar` to `PATH`. Also shipped as standalone `codexbar-windows-x86_64.zip` and `…-arm64.zip` on GitHub Releases. |
| Argument parser | Vendored `Commander` (steipete fork) | `clap 4` with derive macros. |
| Config path | `~/.codexbar/config.json` | `%APPDATA%\CodexBar\config.json` (i.e. `dirs::config_dir().join("CodexBar/config.json")`). |
| Cache path | `~/Library/Caches/CodexBar/cost-usage/` and `~/Library/Caches/CodexBar/` | `%LOCALAPPDATA%\CodexBar\cache\cost-usage\`. |
| Cookie cache | macOS Keychain `com.steipete.codexbar.cache` | DPAPI-wrapped blob at `%LOCALAPPDATA%\CodexBar\cookies.enc` (see `secure_file.rs` in the core crate). |
| Claude CLI PTY | `posix_spawn` + macOS pty | ConPTY (`CreatePseudoConsole` via the `conpty` crate or `windows-sys`); same JSON-streaming parser. |
| Status fetch | `URLSession` + `URLSession.shared.data(for:)` | `reqwest` with `rustls-tls`. |
| Date encoding | `JSONEncoder(.iso8601)` | `serde_json` + `chrono::DateTime<Utc>` with `serde(with = "chrono::serde::ts_seconds")` or default ISO 8601. |
| ANSI detection | `isatty(STDOUT_FILENO)` + `TERM=dumb` | `atty::is(Stream::Stdout)`; also honor `NO_COLOR` env. Enable VT processing via `SetConsoleMode(STD_OUTPUT_HANDLE, ENABLE_VIRTUAL_TERMINAL_PROCESSING)` on legacy `cmd.exe`. |
| Exit codes | `Darwin.exit` | `std::process::exit`. Same numeric mapping (0/1/2/3/4). |
| Subprocess discovery (`claude`, `codex`, `gemini`) | `which`, explicit overrides, `PATH`, login-shell probe, hard-coded `/usr/local/bin`, `/opt/homebrew/bin` | `which::which` + `%LOCALAPPDATA%\Programs\<tool>\<tool>.exe` + `%ProgramFiles%\<tool>` + `cmd.exe /c where <tool>` fallback. |
| `--source auto` web behavior | Allowed | Same on Windows (WebView2-backed). Linux exits non-zero (already in `#if !os(macOS)` guard); on Windows we keep `auto`/`web` enabled because WebView2 ships with the desktop app and the headless CLI can fall back to `reqwest` with pre-imported cookies. |
| Install entry | `bin/install-codexbar-cli.sh` + in-app menu | The MSI installer always places `codexbar.exe`; no separate symlink step. A `winget install CodexBar.CodexBar` user gets both. |

### A.10 Subprocess interactivity policy

The Mac CLI wraps every provider fetch in `ProviderInteractionContext.$current.withValue(.background)`:

```swift
let output = await ProviderInteractionContext.$current.withValue(.background) {
    await Self.fetchUsageOutputs(provider: p, status: status,
                                 tokenContext: tokenContext, command: command)
}
```

That `.background` value tells the provider layer:

- Do **not** trigger interactive Keychain prompts.
- Do **not** clear Keychain cooldowns.
- Do **not** open browser windows for OAuth login.
- Do **not** wait on UI confirmation for repair flows.

The CLI is a pure read-from-cache + read-from-config probe; if credentials are stale or missing, it reports the error and exits non-zero rather than escalating to an interactive prompt.

On Windows, the equivalent contract is:

- DPAPI cookie reads do not prompt (DPAPI is silent unless the user account profile is unloaded).
- WebView2 is *not* spawned from the CLI; cookie-source web fetches go through `reqwest` only.
- Credential Manager reads use `CredReadW` with `CRED_TYPE_GENERIC` and respect the same "background" gate.
- The CLI does not bring up the Tauri window; if `--source oauth` requires login, exit code 3 (`parseError`) is returned with a message pointing to `codexbar.exe` *cannot* run interactive OAuth — open the app.

### A.11 Help-text examples (verbatim)

The root help block:

```
CodexBar <version>

Usage:
  codexbar [--format text|json]
          [--json]
          [--json-only]
          [--json-output] [--log-level <…>] [-v|--verbose]
          [--provider <…|both|all>]
          [--account <label>] [--account-index <index>] [--all-accounts]
          [--no-credits] [--no-color] [--pretty] [--status]
          [--source <auto|web|cli|oauth|api>]
          [--web-timeout <seconds>] [--web-debug-dump-html]
          [--antigravity-plan-debug] [--augment-debug]
  codexbar cost   …
  codexbar config <validate|dump> …
  codexbar cache clear <--cookies|--cost|--all> [--provider <name>]
```

Source: `Sources/CodexBarCLI/CLIHelp.swift`. The Windows port must keep these examples runnable; the integration test asserts on each one.

### A.12 Acceptance (CLI)

- `codexbar.exe --version` prints `CodexBar <semver>` (read from the executable's version resource or an adjacent `VERSION` file).
- `codexbar.exe usage --provider claude --json --pretty` on a Windows host with the same `config.json` and Claude `sessionKey` cookie as a Mac host produces a byte-for-byte equal JSON payload modulo `updatedAt` timestamps and the `source` label.
- `codexbar.exe cost --provider claude --json` scans `%USERPROFILE%\.claude\projects\**\*.jsonl` (and `$CLAUDE_CONFIG_DIR\projects\…`) and emits the same `CostPayload` shape as Mac.
- Exit codes match the table in A.5 across `binary-missing`, `timeout`, `parse-failure`, `success`.
- `codexbar.exe --help`, `codexbar.exe usage --help`, `codexbar.exe cost --help`, `codexbar.exe config --help`, `codexbar.exe cache --help` each emit a help block whose example commands all run successfully (smoke-tested by an integration script).

---

## B. Widgets (drop on v1, document contract)

Widgets are macOS-only on technical grounds (`WidgetKit` / `AppIntents` / `App Group` sandbox / `widgetkit-extension` plug-in point) and on UX grounds (Windows 11's Widget Board is gated behind MSIX + Adaptive Cards JSON, with different lifecycle semantics). They are not in scope for the Windows v1 ship.

This section documents the contract well enough that a Windows engineer can:

- Mock the data path so that a future `WindowsWidgetsHost` (or a third-party tray-widget surface like Rainmeter / DesktopWidgets / WPF tile) can drop in without changing the core.
- Keep the snapshot writer in the desktop app so cross-platform parity is preserved when widgets ship later.

### B.1 Widget families

Source: `Sources/CodexBarWidget/CodexBarWidgetBundle.swift`.

| Widget kind | `WidgetConfiguration` | Intent | Families |
|---|---|---|---|
| `CodexBarSwitcherWidget` | `StaticConfiguration` (no per-widget config) | none (uses `WidgetSelectionStore`) | small, medium, large |
| `CodexBarUsageWidget` | `AppIntentConfiguration<ProviderSelectionIntent>` | `ProviderSelectionIntent` (provider picker) | small, medium, large |
| `CodexBarHistoryWidget` | `AppIntentConfiguration<ProviderSelectionIntent>` | `ProviderSelectionIntent` | medium, large |
| `CodexBarCompactWidget` | `AppIntentConfiguration<CompactMetricSelectionIntent>` | provider + metric (credits / todayCost / last30DaysCost) | small only |

There is no `accessory*` (Lock Screen / Watch) family. The bundle id pattern is `<app-bundle-id>.widget` (release: `com.steipete.codexbar.widget`; debug: `com.steipete.codexbar.debug.widget`). `NSExtensionPointIdentifier = com.apple.widgetkit-extension`. `NSExtensionPrincipalClass = CodexBarWidget.CodexBarWidgetBundle`.

### B.2 Snapshot data shape (the App Group payload)

The widget reads `widget-snapshot.json` from the App Group container. App Group resolution (see `Sources/CodexBarCore/AppGroupSupport.swift`):

1. Read `kSecCodeInfoTeamIdentifier` from the bundle's static code signature.
2. Else `CodexBarTeamID` from `Info.plist`.
3. Else hard-coded `Y5PE65HELJ`.
4. Group id = `<teamID>.com.steipete.codexbar` (or `.debug` for debug builds).
5. Container URL = `containerURL(forSecurityApplicationGroupIdentifier: <id>)`.
6. Fallback (Linux / unsigned): `~/Library/Application Support/CodexBar/widget-snapshot.json`.

A `migrateLegacyDataIfNeeded(…)` routine copies from the old group id (`group.com.steipete.codexbar`) and a small set of `UserDefaults` keys (`debugDisableKeychainAccess`, `widgetSelectedProvider`) on first launch.

Payload (`WidgetSnapshot`):

```
WidgetSnapshot {
  entries: [ProviderEntry],            // one per provider with usage
  enabledProviders: [UsageProvider],   // subset shown in the switcher
  generatedAt: Date,
}
ProviderEntry {
  provider: UsageProvider,             // "codex", "claude", …
  updatedAt: Date,
  primary, secondary, tertiary: RateWindow?,
  creditsRemaining: Double?,
  codeReviewRemainingPercent: Double?,
  tokenUsage: TokenUsageSummary?       // sessionCostUSD, sessionTokens, last30DaysCostUSD, last30DaysTokens
  dailyUsage: [DailyUsagePoint]        // {dayKey "YYYY-MM-DD", totalTokens, costUSD}
}
RateWindow { usedPercent: Double, windowMinutes: Int?, resetsAt: Date?, resetDescription: String? }
```

### B.3 Intent configuration

`ProviderSelectionIntent` exposes 11 providers in `ProviderChoice` (codex, claude, gemini, alibaba, antigravity, zai, copilot, minimax, kilo, opencode, opencodego). Every other provider in `UsageProvider` returns `nil` from `ProviderChoice(provider:)` and is therefore *not* selectable from the widget configuration UI even if it's present in the snapshot.

`CompactMetricSelectionIntent` exposes two parameters: `provider: ProviderChoice` and `metric ∈ {credits, todayCost, last30DaysCost}`.

`SwitchWidgetProviderIntent` is an interactive `AppIntent` (not a configuration intent) — tapping a provider chip in the Switcher widget calls `WidgetSelectionStore.saveSelectedProvider(.codex)` and `WidgetCenter.shared.reloadAllTimelines()`.

### B.4 Refresh cadence

`getTimeline(…)` / `timeline(for: in:)` returns a single `Timeline(entries: [entry], policy: .after(Date() + 30 * 60))` — i.e. **every 30 minutes**. The app side rewrites `widget-snapshot.json` after every successful main refresh and after token-cost refreshes, so the actual freshness ceiling is the user's app polling cadence.

Placeholder data comes from `WidgetPreviewData.snapshot()`; missing snapshots use `WidgetPreviewData.emptySnapshot()`.

### B.5 Decision for Windows v1: **drop**

Reasons:

- Windows 11 widgets require MSIX packaging, Adaptive Cards JSON, Widget Provider COM contract, and registration through `WindowsAppSDK.Widgets`. That doubles the packaging surface for ~2% of users (estimated from Mac widget telemetry in 2025 maintainer notes).
- Tauri 2 has no first-class widget surface; the workaround is a separate WinAppSDK C++/C# project, which kills the shared-Rust-crate story.
- The compact widget overlaps in intent with the tray icon and tray flyout, which are already first-class on Windows.

What we *do* ship in v1 to preserve the contract:

- Keep `WidgetSnapshot`, `WidgetSnapshotStore`, and `WidgetSelectionStore` in the shared Rust core, writing `widget-snapshot.json` to `%LOCALAPPDATA%\CodexBar\widget-snapshot.json` even though nothing reads it on Windows.
- Keep `ProviderChoice` / `CompactMetric` as Rust enums in `host/widget_contract.rs` so any future Windows widget host (MSIX, Rainmeter skin, Stream Deck plugin, third-party widget board) can deserialize the same JSON.

Re-evaluate at the v2 milestone once either (a) Windows 11 widgets API matures and gains a non-MSIX path, or (b) a community port surfaces demand.

### B.6 Why the snapshot is filesystem-mediated (not XPC / shared memory)

Two reasons drove the file-based design that the Windows port should preserve:

1. WidgetKit extensions run in their own sandbox under `chronod` (Mac) — they cannot reach into the host app's memory or call IPC into a non-running process. A persistent file in the App Group container is the only contract WidgetKit guarantees.
2. The snapshot is small (low-kilobyte JSON), changes infrequently (every refresh cycle, typically ≥1 min), and consumers always want the latest value. A polled-file design beats event-driven IPC for this load.

On Windows, the equivalent is `%LOCALAPPDATA%\CodexBar\widget-snapshot.json` written atomically (`tempfile` + `rename`). Any future widget host — a Rainmeter skin, a Stream Deck plugin, a Microsoft Store MSIX widget — reads the same file. No IPC server in the desktop app is needed.

### B.7 Acceptance (widgets)

- `WidgetSnapshot` JSON written on Windows is byte-compatible with the Mac payload (verified by `serde_json::from_str::<WidgetSnapshot>` round-trip against a fixture exported from Mac).
- The Tauri tray flyout reads from the same snapshot file as the widgets would, exercising the contract.
- No Windows-side code creates a "widget" UI surface in v1.
- The snapshot writer is exercised in CI: the Tauri build runs once headless, exits, and a Rust integration test asserts the snapshot file shape against a versioned JSON schema.

---

## C. Watchdog + web probe

These are two separate executable targets, both macOS-only (`#if os(macOS)` in `Package.swift`):

- `CodexBarClaudeWatchdog` — a tiny standalone binary that babysits the `claude` PTY.
- `CodexBarClaudeWebProbe` — a diagnostic CLI for the Claude web API.

### C.1 Claude watchdog (`CodexBarClaudeWatchdog`)

Single Swift file: `Sources/CodexBarClaudeWatchdog/main.swift` (~120 lines). Linked into the app bundle at `CodexBar.app/Contents/Helpers/CodexBarClaudeWatchdog`.

**Why it exists**

When CodexBar starts `claude /usage` or `claude /status` inside a pseudo-terminal, the Claude CLI spawns a child Node process that does not die cleanly if its parent (CodexBar) is force-killed (e.g. `kill -9 CodexBar`, IDE restart, macOS crash). Orphaned `claude` PTYs accumulate, hold cookies open, and can spam usage probes. The watchdog is a tiny intermediate parent that:

1. `posix_spawnp`'s the real binary (`claude …`).
2. Calls `setpgid(child, child)` to put the child in its own process group.
3. Polls in a 200 ms loop:
   - If `waitpid(child, WNOHANG) == child` → exit with the child's exit code.
   - If `globalShouldTerminate != 0` (SIGTERM / SIGINT / SIGHUP) → `killProcessTree`: send SIGTERM to the group, wait 500 ms, escalate to SIGKILL.
   - If `getppid() == 1` (CodexBar died and we reparented to launchd) → same teardown.
4. Exit code encoding follows the standard `wait(2)` macros (Swift can't import them as function-like macros, so the code re-encodes manually): low 7 bits → signal, high byte → status. Signal-terminated children exit with `128 + sig`.
5. CLI: `CodexBarClaudeWatchdog -- <binary> [args...]`. Exits 64 on usage error, 70 on spawn failure.

**Lifecycle**

CodexBar invokes the watchdog for every `claude` probe (rather than `claude` directly). The watchdog stays alive for the lifetime of the probe. When CodexBar is killed, `kill_claude_probes` in `compile_and_run.sh` separately pkills any orphan `claude /status` / `claude /usage` processes — the watchdog handles the same case at runtime.

**IPC to main app**: none. Pure stdin/stdout/stderr passthrough — the watchdog is transparent except for SIGTERM forwarding.

### C.2 Claude web probe (`CodexBarClaudeWebProbe`)

Source: `Sources/CodexBarClaudeWebProbe/ClaudeWebProbeEntry.swift`.

A diagnostic CLI that hits a curated list of `claude.ai` endpoints with the user's currently-cached cookies and prints status, content-type, top-level JSON keys, detected emails, plan hints, and notable fields for each.

Default endpoint list:

```
https://claude.ai/api/organizations
https://claude.ai/api/organizations/{orgId}/usage
https://claude.ai/api/organizations/{orgId}/overage_spend_limit
https://claude.ai/api/organizations/{orgId}/members
https://claude.ai/api/organizations/{orgId}/me
https://claude.ai/api/organizations/{orgId}/billing
https://claude.ai/api/me
https://claude.ai/api/user
https://claude.ai/api/session
https://claude.ai/api/account
https://claude.ai/settings/billing
https://claude.ai/settings/account
https://claude.ai/settings/usage
```

Custom endpoint args override the default list. `CLAUDE_WEB_PROBE_PREVIEW=1` includes a body preview.

It is **not** wired into the main CLI or the app — it's a developer tool, invoked manually from the build directory: `swift run CodexBarClaudeWebProbe`. Not shipped in the app bundle.

### C.3 Windows mapping (watchdog + probe)

| Concern | Mac | Windows |
|---|---|---|
| Watchdog crate | `executableTarget("CodexBarClaudeWatchdog")` | Same `Cargo.toml` workspace, separate `[[bin]]` `codexbar-claude-watchdog` (or `codexbar-claude-watchdog.exe` after build). |
| Child spawn | `posix_spawnp` | `CreateProcessW` with `CREATE_NEW_PROCESS_GROUP` (so we can `GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT)` on the group). |
| Process group / tree kill | `kill(-pgid, SIGTERM/SIGKILL)` | Create a **Job Object** (`CreateJobObjectW` + `AssignProcessToJobObject` + `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) so when the watchdog dies, the kernel terminates the entire descendant tree atomically. This is strictly better than the Mac process-group approach and Windows-idiomatic. |
| Orphan detection | `getppid() == 1` | Compare `GetCurrentProcessId()` parent (via `NtQueryInformationProcess` or by passing the parent PID on the command line) and exit if the parent process handle signals. Simpler approach: open a handle to the parent, `WaitForSingleObject(parent, 0)` each tick. |
| Signals | SIGTERM/SIGINT/SIGHUP | `SetConsoleCtrlHandler` for CTRL_C/CTRL_BREAK/CLOSE/LOGOFF/SHUTDOWN. |
| Exit codes | 64 (usage), 70 (spawn fail), 128+sig | Use the same numeric scheme. Windows `ExitProcess(128 + signal_kind)` where `signal_kind` is the integer enum from `CTRL_C_EVENT` etc. |
| PTY backend | macOS PTY | ConPTY (`CreatePseudoConsole`) — already in scope for the core (CLI PTY-source providers). |
| Web probe | `swift run CodexBarClaudeWebProbe` | `cargo run --bin codexbar-claude-web-probe`. Same endpoint list, same env (`CLAUDE_WEB_PROBE_PREVIEW=1`). HTTP via `reqwest`; cookie reuse goes through the core's `browser/` module (DPAPI cookie cache). Not shipped in the Tauri installer — dev-only. |
| Bundle location | `CodexBar.app/Contents/Helpers/CodexBarClaudeWatchdog` | `%ProgramFiles%\CodexBar\codexbar-claude-watchdog.exe` (next to `codexbar.exe` and the desktop binary). |

### C.4 Watchdog crash-recovery semantics

Concrete failure modes and the watchdog's response:

| Scenario | Mac watchdog response | Windows mapping |
|---|---|---|
| Tauri/desktop app gracefully exits | SIGTERM propagates → child receives SIGTERM → 500 ms grace → SIGKILL on group | Job Object with `KILL_ON_JOB_CLOSE` flag: as soon as the watchdog handle closes, the kernel terminates every process in the job. |
| Tauri app force-killed (`kill -9`) | `getppid() == 1` detected within 200 ms tick → cleanup as above | Parent process handle becomes signalled → `WaitForSingleObject(parent, 0) == WAIT_OBJECT_0` → cleanup. |
| Watchdog itself crashes | Child orphans (kernel reparents to launchd) | Job Object guarantees child dies with the watchdog. |
| Child hangs (no progress, no exit) | No special handling — watchdog only forwards signals | Same. Per-probe timeouts live in the CLI dispatcher (`TTYCommandRunner.Error.timedOut`), not the watchdog. |
| Child exits with non-zero | Watchdog exits with the same code | Same. |
| User sends Ctrl+C to the CLI | SIGINT forwarded to child group | `CTRL_C_EVENT` via `GenerateConsoleCtrlEvent(CTRL_C_EVENT, child_pgid)`. |

### C.5 Acceptance (watchdog + probe)

- Killing the Tauri desktop process with Task Manager → Force Quit terminates every `claude.exe` child within 1 s (verified by `Get-Process claude`).
- A spawn failure for a non-existent binary exits 70 with a stderr message.
- `codexbar-claude-web-probe.exe` prints the same field set (status, content-type, top-level keys, emails, plan-hints, notable fields) as the Mac binary against the same cookie state.

---

## D. Localization

### D.1 Files shipped

| Locale | Path | Status |
|---|---|---|
| `en` | `Sources/CodexBar/Resources/en.lproj/Localizable.strings` | Base / fallback. |
| `pt-BR` | `Sources/CodexBar/Resources/pt-BR.lproj/Localizable.strings` | Brazilian Portuguese — added in commit `22c44848` "Add Brazilian Portuguese localization (#902)". |
| `zh-Hans` | `Sources/CodexBar/Resources/zh-Hans.lproj/Localizable.strings` | Simplified Chinese — added in v0.25 (#819). |

No `.xcstrings` catalog file is present — the project uses the classic `Localizable.strings` plain-text format (key/value pairs, one per line, e.g. `"Add" = "Adicionar";`). This is simpler to port: the keys are the English strings themselves.

Total: 3 locales. The CLI is English-only (no localization on `--help`/output).

### D.2 Key structure

Keys are the literal English text. Examples:

```
"About" = "Sobre";
"Account" = "Conta";
"Auto-refresh: hourly · Timeout: 10m" = "Atualização automática: por hora · Tempo limite: 10m";
```

This means:

- New strings drop in by adding `"<text>" = "<text>";` to `en.lproj` and translations to each other locale.
- Missing translations fall through to the English key (NSLocalizedString default).
- Plural-rule files (`.stringsdict`) are not currently used.

### D.3 Language picking

`Sources/CodexBar/Localization.swift` is the lookup path:

1. `appLanguageDefaults().string(forKey: "appLanguage")` — user's selected language from Preferences. If empty/nil → **System mode**.
2. System mode: take `resourceBundle.preferredLocalizations.first` (macOS picks from the user's system language preferences, intersected with bundled locales).
3. Override mode: locate the matching `<lang>.lproj` bundle (case-insensitive fallback for casing differences like `pt-BR` vs `pt-br` — fixed in commit `a01bf8c9`).
4. Fallback chain: matched bundle → `en.lproj` → resource bundle root.

The bundle is resolved from `Bundle.main` for packaged builds (`*.app`) and `Bundle.module` for `swift run` dev builds. The packaged path looks up `CodexBar_CodexBar.bundle` first (SwiftPM resource bundle naming), then falls back to scanning `resourceURL`.

User-facing setting: **Preferences → General → Language** with options `(System)` plus each bundled locale.

### D.4 Windows mapping (localization)

| Concern | Mac | Windows |
|---|---|---|
| Format | `Localizable.strings` (UTF-8 key/value pairs) | **i18next** for the React UI (`src/locales/<lang>/translation.json`); **fluent-rs** (`@fluent/fluent`) for the Rust core's user-visible strings (errors, toast text). |
| Key naming | Literal English string ("Add", "Account") | Same convention for the React side — `t("Add")`, `t("Auto-refresh: hourly · Timeout: 10m")` — to keep `.strings` migration zero-touch. The build script converts each `Localizable.strings` to `translation.json` (one-shot import). |
| Language picking | `appLanguage` in `UserDefaults` | `appLanguage` in config (same key) → i18next backend uses it; `null/empty` means `navigator.language` → Windows display language. |
| System probe | `Bundle.preferredLocalizations` | `GetUserPreferredUILanguages` (Win32) for the Rust side; `navigator.language` + `navigator.languages` for React. |
| Casing fix | Lowercased fallback after `a01bf8c9` | i18next is already case-insensitive on lookup keys but case-sensitive on locale codes — normalize to lowercase BCP-47 (`pt-br`, `zh-hans`) in the config layer. |
| New locale flow | Add `<lang>.lproj/Localizable.strings`, rebuild | Drop a `src/locales/<lang>/translation.json`; the build script regenerates the locale manifest. The Rust core uses Fluent `.ftl` files mirrored from the JSON via a `build.rs` step. |
| Plural rules | Not used | Not used initially; if needed later, prefer Fluent's `select` over English-key-as-literal. |

### D.5 Migration script (one-shot Strings → JSON)

A trivial Rust or Node script converts the existing `Localizable.strings` files to the i18next JSON shape:

```js
// strings-to-json.js
const fs = require("fs");
const text = fs.readFileSync(process.argv[2], "utf8");
const out = {};
for (const line of text.split(/\r?\n/)) {
  const m = line.match(/^\s*"((?:[^"\\]|\\.)*)"\s*=\s*"((?:[^"\\]|\\.)*)"\s*;\s*$/);
  if (m) out[m[1].replace(/\\"/g, '"')] = m[2].replace(/\\"/g, '"');
}
console.log(JSON.stringify(out, null, 2));
```

Run once at port time:

```
node strings-to-json.js Sources/CodexBar/Resources/en.lproj/Localizable.strings \
  > apps/desktop-tauri/src/locales/en/translation.json
node strings-to-json.js Sources/CodexBar/Resources/pt-BR.lproj/Localizable.strings \
  > apps/desktop-tauri/src/locales/pt-BR/translation.json
node strings-to-json.js Sources/CodexBar/Resources/zh-Hans.lproj/Localizable.strings \
  > apps/desktop-tauri/src/locales/zh-Hans/translation.json
```

After migration, both the Mac and Windows sides update strings independently. We do *not* keep `.strings` as the source of truth for Windows; each platform owns its translations going forward.

### D.6 Acceptance (localization)

- `pt-BR` and `zh-Hans` strings render correctly in the Tauri preferences UI (verified by switching `appLanguage` and reloading).
- Missing keys fall back to English (no JS console errors, no `key.not.found` strings).
- A round-trip script: `Localizable.strings` → `translation.json` → `Localizable.strings` is idempotent on a known fixture.

---

## E. Build / packaging / release

### E.1 Toolchain (current, Mac)

| Tool | Pin | Why |
|---|---|---|
| Swift | 6.2+ (`swift-tools-version: 6.2`) | StrictConcurrency feature flag; macro support. |
| Platform | macOS 14+ (`.macOS(.v14)`) | `LSMinimumSystemVersion = 14.0`; WidgetKit / AppIntents APIs. |
| Xcode | 26+ (per `docs/RELEASING.md`) | `ictool` / `iconutil` / `appintentsmetadataprocessor`. |
| Sparkle | 2.9.1 (`066e75a8`) | Auto-update. |
| Commander | steipete fork 0.2.x (`ae2ce746`) | CLI parser. |
| swift-log | 1.12.0+ | Logging. |
| swift-syntax | 600.0.1+ | Macros. |
| KeyboardShortcuts | sindresorhus 2.4.0 | Hotkey recorder. |
| Vortex | zats `ef539208` | Confetti. |
| SweetCookieKit | steipete 0.4.1 | Browser cookie import. |

`Package.resolved` pins everything.

### E.2 Build flow (current, Mac)

| Script | Job |
|---|---|
| `Makefile` | Thin shims: `make start` → `compile_and_run.sh`; `make release` → `package_app.sh release`. |
| `Scripts/compile_and_run.sh` | Kills running app + orphan `claude` probes, resolves signing identity (Developer ID > Apple Dev > ad-hoc), acquires a per-repo lock under `$TMPDIR`, packages, launches. Optional `--test`, `--release-universal`, `--debug-lldb`, `--clear-adhoc-keychain`. |
| `Scripts/package_app.sh` | Multi-arch SwiftPM build (`arm64` and/or `x86_64`), patches `KeyboardShortcuts` bundle lookup, assembles `CodexBar.app/Contents/{MacOS, Resources, Frameworks, Helpers, PlugIns}`, generates entitlements plists, writes `Info.plist`, builds `CodexBarWidget.appex` with `appintentsmetadataprocessor`-generated `Metadata.appintents`, copies Sparkle.framework, fixes rpath, codesigns everything in topological order. |
| `Scripts/sign-and-notarize.sh` | Builds universal release, signs with Developer ID, ditto-zips, `xcrun notarytool submit … --wait`, `xcrun stapler staple`, validates with `spctl -a -t exec -vv`, packages dSYM. Requires `APP_STORE_CONNECT_API_KEY_P8`, `APP_STORE_CONNECT_KEY_ID`, `APP_STORE_CONNECT_ISSUER_ID`, `SPARKLE_PRIVATE_KEY_FILE`. |
| `Scripts/make_appcast.sh` | Calls Sparkle's `generate_appcast` with the Ed25519 private key; embeds HTML release notes (rendered from `CHANGELOG.md` via `changelog-to-html.sh`); optionally tags with `sparkle:channel="beta"`. |
| `Scripts/build_icon.sh` | Converts the macOS 14 `Icon.icon` bundle (IconStudio export) → master 824 px PNG → padded 1024 px → all required iconset sizes → `Icon.icns` via `iconutil`. |
| `Scripts/release.sh` | Top-level orchestrator: lint, test, sign-and-notarize, generate appcast, GitHub release upload via `gh release create`, push tag, push `appcast.xml` commit. Sources `~/Projects/agent-scripts/release/sparkle_lib.sh` for shared helpers. |
| `Scripts/check-release-assets.sh` | Post-publish check: zip + dSYM zip exist on the GitHub release. |
| `Scripts/verify_appcast.sh` | Verify the enclosure signature + size in `appcast.xml`. |
| `bin/install-codexbar-cli.sh` | Asks for sudo via `osascript`, symlinks `CodexBar.app/Contents/Helpers/CodexBarCLI` to `/usr/local/bin/codexbar` and `/opt/homebrew/bin/codexbar`. |

### E.3 Distribution (current, Mac)

| Channel | Mechanism |
|---|---|
| GitHub Releases | Notarized `CodexBar-<ver>.zip` + `…-<ver>.dSYM.zip` per release tag (`v<ver>`). |
| Sparkle appcast | `appcast.xml` in repo root, served via `https://raw.githubusercontent.com/steipete/CodexBar/main/appcast.xml`. Public Ed25519 key `AGCY8w5vHirVfGGDGc8Szc5iuOqupZSh9pMj/Qs67XI=` is baked into `Info.plist`. |
| Sparkle channels | `stable` (default, no tag) + `beta` (tagged `sparkle:channel="beta"`); About → Update Channel toggles `allowedChannels`. |
| Homebrew Cask | `steipete/tap/codexbar` (the macOS `.app`). Sparkle is disabled in Homebrew installs (`brew upgrade` updates instead). |
| Homebrew Formula | `steipete/tap/codexbar` (CLI-only, separate from the cask). Pulls the standalone tarballs. |

### E.4 Sparkle appcast format

```xml
<item>
  <title>0.25.1</title>
  <pubDate>Mon, 11 May 2026 03:41:45 +0100</pubDate>
  <link>https://raw.githubusercontent.com/steipete/CodexBar/main/appcast.xml</link>
  <sparkle:version>61</sparkle:version>
  <sparkle:shortVersionString>0.25.1</sparkle:shortVersionString>
  <sparkle:minimumSystemVersion>14.0</sparkle:minimumSystemVersion>
  <description><![CDATA[<h2>CodexBar 0.25.1</h2>…]]></description>
  <enclosure url="https://github.com/steipete/CodexBar/releases/download/v0.25.1/CodexBar-0.25.1.zip"
             length="36140734" type="application/octet-stream"
             sparkle:edSignature="…ed25519…"/>
</item>
```

Beta items add `sparkle:channel="beta"` to `<item>`.

### E.5 Icon pipeline (current, Mac)

`Icon.icon` (IconStudio bundle) → `ictool --export-preview macOS Default 824 824 1 -45` → 824 px PNG → `sips --padToHeightWidth 1024` → padded master → multi-size iconset (16/32/64/128/256/512/1024 + @2x) → `iconutil -c icns` → `Icon.icns` placed in `CodexBar.app/Contents/Resources/Icon.icns`. A classic-style fallback `Icon-classic.icns` is shipped from `Sources/CodexBar/Resources/` (verified to exist by `package_app.sh`, errors out otherwise).

### E.6 Windows mapping (build + release)

**Drop**: every line in this table goes away on Windows.

| Mac artifact | Status |
|---|---|
| `Package.swift`, `Package.resolved` | Drop. |
| `Scripts/compile_and_run.sh`, `package_app.sh`, `sign-and-notarize.sh`, `make_appcast.sh`, `build_icon.sh`, `release.sh`, `changelog-to-html.sh`, `verify_appcast.sh`, `check-release-assets.sh` | Drop (rewrite in PowerShell or Cargo xtask). |
| `bin/install-codexbar-cli.sh` | Drop. The MSI installer always places `codexbar.exe`. |
| `Makefile` | Drop. Replace with `cargo xtask`. |
| `appcast.xml` | Drop. Replaced by Tauri updater JSON manifest. |
| `Icon.icon`, `Icon.icns`, `Icon-classic.icns`, `Sources/.../*.svg` icons | Keep the SVGs and `Icon.icon` *only* as input art; output is ICO. |
| Sparkle framework / signing of Sparkle XPCs | Drop entirely. |
| `xcrun notarytool` / `stapler` | Drop. |
| `appintentsmetadataprocessor` widget metadata generation | Drop. |

**Adopt** for Windows:

| Concern | Windows |
|---|---|
| Build entry | `npm run tauri build` (release) / `npm run tauri dev` (debug). Behind the scenes Tauri invokes `cargo build` for the shell + core, then `tauri-bundler` for the installer. CI uses a `cargo xtask release` thin orchestrator that wraps the same commands. |
| Toolchain | Rust stable (MSRV pinned in `rust-toolchain.toml`, e.g. 1.78+); Node 20+ for the React frontend; pnpm or npm; Tauri CLI 2.x; Windows SDK ≥ 10.0.22621 for SignTool / makemsix. |
| Frontend | React 18 + Vite + TypeScript 5.x. Bundled by Tauri (`tauri.conf.json` `frontendDist`). |
| Targets | `x86_64-pc-windows-msvc` (primary); `aarch64-pc-windows-msvc` (secondary, Windows on ARM). Both produced in one CI matrix. |
| Installer | **Inno Setup** for the user-facing `.exe` installer (per-user install by default, optional per-machine, adds `%LOCALAPPDATA%\Programs\CodexBar` and writes `HKCU\Software\CodexBar`); **portable ZIP** alongside (no installer, just `codexbar.exe` + `CodexBar.exe`); **optional MSIX** for Microsoft Store distribution (skip in v1, plan for v2). |
| Auto-update | Tauri Updater (`tauri-plugin-updater`). Manifest URL `https://github.com/<org>/CodexBar/releases/latest/download/latest.json`; manifest is signed with Tauri's minisign key (analogous to Sparkle ed25519). Replaces appcast.xml. |
| Updater manifest format | `latest.json` per Tauri convention: `{ "version": "0.26.0", "notes": "…markdown…", "pub_date": "ISO 8601", "platforms": { "windows-x86_64": { "signature": "<base64>", "url": "https://…/CodexBar-0.26.0-x64.msi" } } }`. |
| Update channels | Two manifests: `latest.json` (stable) and `beta.json` (beta). The settings UI toggle writes the chosen channel into config and the updater reads from the matching URL. (Tauri 2's updater supports per-channel URLs out of the box.) |
| Distribution | GitHub Releases (same as Mac); **Winget** manifest in `microsoft/winget-pkgs` (`CodexBar.CodexBar.yaml`); optional **Microsoft Store** (MSIX) in v2; optional **Chocolatey** community package — not first-party. |
| Icon | `Icon.icon` / source PNG → ICO via the `image` crate at build time (or `cargo xtask icons`). Sizes: 16, 20, 24, 32, 40, 48, 64, 96, 128, 256 → multi-image `Icon.ico`. Tray uses a separate dynamic-rendered RGBA buffer (the core's `tray::render` already does this on Mac for the bar-meter; same code, no platform shim needed). |
| Code signing | **Authenticode** via `signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /a <file>`. Cert sources, in order of preference: 1) EV cert on YubiKey HSM (avoids SmartScreen reputation gating); 2) OV cert via Sectigo/DigiCert (still triggers SmartScreen warnings until reputation builds). Sign in topological order: `codexbar-claude-watchdog.exe` → `codexbar.exe` → `CodexBar.exe` (desktop) → the installer (`.exe`) and any nested DLLs. |
| Notarization | Not required on Windows. Optional: Microsoft Store certification if shipping MSIX. |
| dSYM / debug symbols | Ship `.pdb` files alongside binaries in the GitHub release (`CodexBar-<ver>-pdb.zip`). |
| Version source | `version.env` (`MARKETING_VERSION=…`, `BUILD_NUMBER=…`) — keep the same file format. The Rust build reads it via `build.rs`; Tauri reads it from `tauri.conf.json` (which is templated from `version.env` by a `cargo xtask sync-version` step). |
| CHANGELOG | Same `CHANGELOG.md` format; the release script extracts the top section for the GitHub release body and embeds it as Markdown (or rendered HTML) into the Tauri updater manifest's `notes`. |

### E.7 CI/CD

| Pipeline stage | Mac (current) | Windows (target) |
|---|---|---|
| Runner | `macos-14` (and `macos-15` for Xcode 26) | `windows-2022` (primary) + `windows-11-arm` (for aarch64 if/when GitHub adds it; until then, cross-compile aarch64 from x64). |
| Lint | `swiftformat Sources Tests`, `swiftlint --strict` | `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `pnpm lint` (ESLint + Prettier), `tsc --noEmit`. |
| Test | `swift test`, `swift test --filter LiveAccountTests` | `cargo test --workspace`, `pnpm test` (Vitest), integration tests under `tests/` (spawn the built binary, hit fixture endpoints). |
| Build | `swift build -c release --arch arm64 && swift build -c release --arch x86_64` | `cargo build --release --target x86_64-pc-windows-msvc` (and aarch64). |
| Bundle | `Scripts/package_app.sh` | `npm run tauri build -- --target x86_64-pc-windows-msvc`. |
| Sign | `codesign --options runtime --timestamp --sign "Developer ID Application: …"` | `signtool sign /tr http://timestamp.digicert.com /td SHA256 /fd SHA256` against secret-store-injected PFX (via Azure Trusted Signing or `SignTool` with a HSM cert). |
| Notarize | `xcrun notarytool submit --wait` | Not applicable. |
| Staple | `xcrun stapler staple` | Not applicable. |
| Verify | `spctl -a -t exec -vv`, `stapler validate` | `signtool verify /pa /v`. |
| Update manifest | `Scripts/make_appcast.sh` → `appcast.xml` commit | `cargo xtask appcast` → emits `latest.json` + `beta.json`, signs with Tauri minisign key, uploads to the release. |
| Release publish | `gh release create v<tag> CodexBar-<ver>.zip CodexBar-<ver>.dSYM.zip --notes-file …` | `gh release create v<tag> CodexBar-<ver>-x64.exe CodexBar-<ver>-arm64.exe CodexBar-<ver>-portable-x64.zip CodexBar-<ver>-pdb.zip latest.json --notes-file …`. |
| Winget submission | Not applicable | Automated via `wingetcreate update CodexBar.CodexBar --version <ver> --url <installer-url> --submit`. |

### E.8 Suggested Inno Setup script outline

The actual file lives at `installer/codexbar.iss`. Key sections:

```
[Setup]
AppName=CodexBar
AppVersion={#MarketingVersion}
AppPublisher=<org>
DefaultDirName={localappdata}\Programs\CodexBar
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesInstallIn64BitMode=x64 arm64
OutputBaseFilename=CodexBar-{#MarketingVersion}-{#Arch}
SetupIconFile=..\assets\Icon.ico
SignTool=signtool sign /tr http://timestamp.digicert.com /td SHA256 /fd SHA256 /a $f
WizardStyle=modern
```

```
[Files]
Source: "..\target\release\CodexBar.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\codexbar.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\codexbar-claude-watchdog.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\Icon.ico"; DestDir: "{app}"
```

```
[Tasks]
Name: "addtopath"; Description: "Add CodexBar to PATH"; Flags: checkedonce
Name: "launchatlogin"; Description: "Launch at sign-in"; Flags: checkedonce
[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path";
   ValueData: "{olddata};{app}"; Check: NeedsAddPath('{app}'); Tasks: addtopath
```

`SignTool=` runs Authenticode against every output the wizard produces — both the unwrapped binaries and the installer itself.

### E.9 Winget manifest outline

`microsoft/winget-pkgs/manifests/c/CodexBar/CodexBar/<ver>/CodexBar.CodexBar.installer.yaml`:

```yaml
PackageIdentifier: CodexBar.CodexBar
PackageVersion: 0.26.0
InstallerType: inno
Architectures:
  - x64
  - arm64
Installers:
  - Architecture: x64
    InstallerUrl: https://github.com/<org>/CodexBar/releases/download/v0.26.0/CodexBar-0.26.0-x64.exe
    InstallerSha256: <sha256>
  - Architecture: arm64
    InstallerUrl: https://github.com/<org>/CodexBar/releases/download/v0.26.0/CodexBar-0.26.0-arm64.exe
    InstallerSha256: <sha256>
ManifestType: installer
ManifestVersion: 1.6.0
```

Submission is automated via `wingetcreate update` from the release workflow.

### E.10 Acceptance (build + release)

- A fresh Windows 11 24H2 machine with no developer tools installed can run `winget install CodexBar.CodexBar` and end up with a tray icon, a working `codexbar.exe` on PATH, and the desktop app starting on user login (if "Launch at login" is enabled in settings).
- Equivalently, double-clicking the downloaded `.exe` installer from GitHub Releases produces the same result — no SmartScreen "Don't run" block for users with the EV cert path; OV-cert path shows the warning but allows "Run anyway".
- Running the portable ZIP variant: extracting and running `CodexBar.exe` from any directory works (no per-user install state required).
- Updating: launching an older signed build triggers an in-app update prompt, downloads the new installer (verified by minisign signature), and installs in-place.
- `codexbar.exe usage --json` output is byte-equal to the macOS CLI output (modulo timestamps and source labels) for the same `config.json` and the same provider credentials.
- `signtool verify /pa /v` succeeds on `CodexBar.exe`, `codexbar.exe`, `codexbar-claude-watchdog.exe`, and the installer.
- All four CI matrix legs (x64 build, arm64 build, x64 sign, arm64 sign) complete in under 20 minutes on `windows-2022`.

---

## Cross-section: file-by-file pointer index (Mac → reader's mental model)

| Mac path | Subject of this doc |
|---|---|
| `Sources/CodexBarCLI/CLIEntry.swift` | CLI section A (main, dispatch). |
| `Sources/CodexBarCLI/CLIUsageCommand.swift` | A — `usage` subcommand. |
| `Sources/CodexBarCLI/CLICostCommand.swift` | A — `cost` subcommand. |
| `Sources/CodexBarCLI/CLIConfigCommand.swift` | A — `config validate`/`dump`. |
| `Sources/CodexBarCLI/CLICacheCommand.swift` | A — `cache clear`. |
| `Sources/CodexBarCLI/CLIOptions.swift` | A — flag schema (`UsageOptions`, `ProviderSelection`, `OutputFormat`). |
| `Sources/CodexBarCLI/CLIHelp.swift` | A — help text. |
| `Sources/CodexBarCLI/CLIHelpers.swift` | A — provider resolution, status fetch, error mapping. |
| `Sources/CodexBarCLI/CLIErrorReporting.swift` | A — JSON error payloads + `exit(...)`. |
| `Sources/CodexBarCLI/CLIExitCode.swift` | A — `ExitCode` enum. |
| `Sources/CodexBarCLI/CLIIO.swift` | A — stderr writer, version detection, `platformExit`. |
| `Sources/CodexBarCLI/CLIOutputPreferences.swift` | A — JSON-only / pretty / format parsing. |
| `Sources/CodexBarCLI/CLIPayloads.swift` | A — JSON shapes (`ProviderPayload`, `ProviderStatusPayload`, `StatusFetcher`). |
| `Sources/CodexBarCLI/CLIRenderer.swift` | A — text-mode renderer. |
| `Sources/CodexBarCLI/TokenAccountCLI.swift` | A — token-account resolution. |
| `Sources/CodexBarWidget/CodexBarWidgetBundle.swift` | B — widget kinds and supported families. |
| `Sources/CodexBarWidget/CodexBarWidgetProvider.swift` | B — timeline providers, intents, snapshot loading. |
| `Sources/CodexBarWidget/CodexBarWidgetViews.swift` | B — view code (read once for the data shape it consumes, then ignore). |
| `Sources/CodexBarCore/AppGroupSupport.swift` | B — snapshot file location + migration. |
| `Sources/CodexBarClaudeWatchdog/main.swift` | C.1 — watchdog. |
| `Sources/CodexBarClaudeWebProbe/ClaudeWebProbeEntry.swift` | C.2 — web probe. |
| `Sources/CodexBar/Localization.swift` | D — language picking. |
| `Sources/CodexBar/Resources/{en,pt-BR,zh-Hans}.lproj/Localizable.strings` | D — strings. |
| `Package.swift`, `Package.resolved` | E.1 — toolchain. |
| `Scripts/*.sh` | E.2 — build flow. |
| `bin/install-codexbar-cli.sh` | E.2 — CLI install (no Windows equivalent). |
| `appcast.xml` | E.4 — appcast format (replaced by `latest.json` on Windows). |
| `Makefile` | E.2 — make targets (replaced by `cargo xtask`). |
| `version.env` | E.6 — version source. |
| `CHANGELOG.md` | E.6 — changelog format (kept verbatim). |

---

## Open questions for the Windows engineer

1. Which cert path for Authenticode signing — Azure Trusted Signing, Sectigo OV, DigiCert EV? Affects SmartScreen UX and CI secret management. Recommend EV-on-HSM if budget allows.
2. Inno Setup or WiX Toolset for the installer? Inno is friendlier and what every healthy Tauri-on-Windows app uses (`Win-CodexBar` included). WiX gives MSI for enterprise GPO deployment.
3. Whether to ship the standalone CLI on Linux as well — the current Mac CLI already cross-compiles for Linux. The shared Rust crate makes Linux essentially free; suggest publishing `codexbar-linux-x86_64.tar.gz` and `codexbar-linux-aarch64.tar.gz` from the same CI matrix.
4. Whether the Claude watchdog needs to ship at v1, or whether ConPTY + Job Object + `CREATE_NEW_PROCESS_GROUP` in the core dispatcher are enough. The Mac watchdog exists because spawning Claude in-process through Foundation leaks orphans; if the Rust dispatcher already uses Job Objects, the watchdog is redundant. Recommend wiring Job Objects directly and dropping the watchdog binary.
5. Whether widgets are ever in scope. Track in `06-roadmap.md` as a v2/v3 candidate gated by Windows Widgets API improvements.
