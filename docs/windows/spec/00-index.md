---
summary: "Master index for the CodexBar Windows blueprint. Read this first."
read_when:
  - Onboarding to the Windows port
  - Picking the next subsystem to implement
  - Looking up cross-cutting decisions
---

# 00 — Master Blueprint Index

This folder is the **complete behavioral spec** for porting `steipete/CodexBar` (macOS, Swift, AppKit + SwiftUI) to Windows on **Tauri 2 + React + shared Rust crate**, derived from a deep, parallel read of the entire Swift source tree.

It is *not* a Swift-to-Rust translation guide. It is a contract that says: *"after the port, this is the behavior, these are the timings, these are the exact endpoints, this is how it feels."* If you implement against this spec faithfully, the Windows app behaves like the Mac app — and where it deviates, the deviations are documented and intentional.

Target polish bar: **Phantom Wallet / Duolingo**. Every magic number, every animation curve, every microcopy line in the Mac app is captured here.

## How the blueprint is organized

| File | Lines | Owner subsystem | Read when |
|---|---:|---|---|
| [10-tray-icon-system.md](10-tray-icon-system.md) | 1,080 | Dynamic tray icon: canvas, bars, brand twists, theming, animation, DPI, ICO atlas | Building the Rust tray-icon renderer |
| [15-popover-menu-card-ui.md](15-popover-menu-card-ui.md) | 1,195 | Popover chrome, provider cards, switcher, charts, hover, click-to-copy, typography | Building the React popup UI |
| [20-preferences-ui.md](20-preferences-ui.md) | 803 | Preferences window: 7 panes, settings store, 60+ row per-provider catalog | Building the React Preferences window |
| [30-provider-system-architecture.md](30-provider-system-architecture.md) | 1,255 | Provider framework: descriptor, lifecycle, registry, fetch plan, result models, error taxonomy | Before writing any provider |
| [40-provider-claude.md](40-provider-claude.md) | 1,150 | Claude provider: OAuth → Web → CLI PTY pipeline, watchdog, web probe, cost scan | Implementing Claude |
| [41-provider-codex.md](41-provider-codex.md) | 1,034 | Codex provider: OAuth, CLI RPC, web dashboard scrape, account promotion state machine, history ownership | Implementing Codex |
| [42-providers-tier1.md](42-providers-tier1.md) | 1,139 | Cursor, Copilot, Gemini, Vertex AI, Factory, OpenRouter — full per-provider deep-dives | Implementing tier-1 providers |
| [43-providers-catalog.md](43-providers-catalog.md) | 1,002 | 31 long-tail providers, uniform card template + 5 cross-cutting matrices | Implementing long-tail or scoping v1 set |
| [50-refresh-state-pace.md](50-refresh-state-pace.md) | 1,172 | Refresh loop, UsageStore, pace algorithm, plan-utilization history, concurrency model | Building the core data flow |
| [55-status-incidents.md](55-status-incidents.md) | 458 | Status feed catalog (8 polled + link-only), icon badge, menu pill, aggregation rule | Implementing status overlay |
| [60-auth-cookies-secrets.md](60-auth-cookies-secrets.md) | 949 | Keychain → DPAPI/Credential Manager, browser cookies (v10/v20), token-account model, logging discipline | Implementing any auth path |
| [70-cost-scanning.md](70-cost-scanning.md) | 1,018 | JSONL log scanner, pricing tables, dedup, cache, projection, storage footprint | Implementing cost scan |
| [80-feel-and-polish.md](80-feel-and-polish.md) | 1,064 | Animations, sounds, confetti, microcopy, accessibility, reduced-motion, 64-item polish checklist | Continuously — this is the "feel" north star |
| [90-cli-widgets-build.md](90-cli-widgets-build.md) | 806 | CLI subcommands, widget snapshot contract (drop at v1), watchdog binary, locale, Inno + Authenticode + updater | Shipping a release |

**14 documents, 14,125 lines.** Each is self-contained — you do not need to read them in order. Read this index, pick the subsystem you're implementing, jump there.

## Read-in-this-order if you're starting fresh

1. [04-recommended-architecture.md](../04-recommended-architecture.md) (parent folder) — the Tauri + Rust shape.
2. [30-provider-system-architecture.md](30-provider-system-architecture.md) — the framework every provider plugs into.
3. [50-refresh-state-pace.md](50-refresh-state-pace.md) — the data flow that owns the UI.
4. [60-auth-cookies-secrets.md](60-auth-cookies-secrets.md) — the security boundary.
5. [10-tray-icon-system.md](10-tray-icon-system.md) + [15-popover-menu-card-ui.md](15-popover-menu-card-ui.md) — the user-facing surface.
6. [80-feel-and-polish.md](80-feel-and-polish.md) — the bar we're holding the rest of the build to.
7. The provider specs (40, 41, 42, 43) on a per-implementation basis.

## Cross-cutting findings — Mac source inconsistencies surfaced during the read

The deep read across 14 subsystems surfaced **real inconsistencies in the Mac source** that we should *not* faithfully port. They go on the "fix during port" list, not the "preserve" list.

### Architectural

1. **Dual `.auto` selection for Claude** — `ClaudeProviderDescriptor` and `ClaudeUsageFetcher.executeAuto` have separate, drifting ordering. Consolidate to one source of truth. *(specs 30, 40)*
2. **Dual provider registry** — `ProviderDescriptor.swift:55-95` seed map *and* `ProviderImplementationRegistry.swift:14-56` macro registration. Use a single `inventory!`-style Rust crate registry. *(spec 30)*
3. **Identity siloing is documented but not enforced** — invariant exists in `docs/`; nothing fails statically when provider A writes to provider B's snapshot. Make `UsageStore` reject mismatched `identity.provider_id`. *(spec 30)*

### Concurrency / refresh-loop

4. **No per-strategy timeout** — slow Claude OAuth Keychain probes can stall the JoinSet up to URLSession's 60 s default. Wrap each strategy in `tokio::time::timeout(45 s)`. *(spec 50)*
5. **Serial cost-usage scans with 10-min per-step ceiling** — worst case ~30 min stall before fresh data. Tighten to 60–90 s. *(spec 50)*
6. **`UserDefaults` write inside refresh path** — `prepareRefreshState` mutates persistent state during the hot loop. Move to a separate task. *(spec 50)*
7. **Unbounded widget-snapshot task chaining** — replace `_ = await previousTask?.result` with `tokio::sync::watch` + single writer. *(spec 50)*
8. **Two `failure_gates` maps with identical semantics** (usage vs. cost) — easy to drift. Unify. *(spec 50)*
9. **OpenAI dashboard 5× cadence multiplier** — can stretch to 2.5 h between scrapes on the 30 min preset. Re-evaluate. *(spec 50)*
10. **Codex account-scoped refresh guard silently discards results** — preserve exactly or cross-account data leaks. *(spec 50)*

### UX / data-flow

11. **"Sonnet" label even when data is `seven_day_opus`** — Claude card mislabels in some paths. *(spec 40)*
12. **`docs/claude.md` says web extras are internal-only, but the code exposes the toggle.** Reconcile in docs + UI. *(spec 40)*
13. **Codex `openAIWebAccessEnabled` is a second writer for `cookieSource`** — settings layer has implicit cross-key writes. *(spec 20, 41)*
14. **StepFun repurposes `apiKey/cookieHeader/region` as username/password/token** — confusing schema reuse. Either rename or drop password mode in v1. *(spec 20, 43)*
15. **Claude `tokenAccounts` mixes `sessionKey` cookies and OAuth `sk-ant-oat...` bearers** — same field, two formats, routed by prefix. Surface the routing rule in the UI. *(spec 20, 40, 60)*
16. **Quota-marker switch label ("Show") doesn't match PR title ("Hide")** — fresh-from-upstream commit `009420a7`. Confirm intended copy. *(spec 20)*
17. **Status aggregation is first-match-wins in provider order, not severity-based** — surprising; either keep deliberately or change to "highest severity first." *(spec 55)*
18. **`Account:` org-equals-email-prefix suppression in Claude CLI parser** — heuristic that can mis-fire. *(spec 40)*
19. **`utilization` Int-vs-Double polymorphism** in API responses — parsers must accept both. *(spec 40)*
20. **`cents → dollars` conversion lives in two paths** (OAuth `extra_usage` + Web `overage_spend_limit`) — same logic duplicated. *(spec 40)*

### Per-provider edge cases worth knowing before writing code

21. **Antigravity uses `lsof`/`ps` for a local LSP probe** — Windows needs `Get-NetTCPConnection` or netstat parsing. Hardest single provider port. *(spec 43)*
22. **Windsurf reads Chromium localStorage leveldb (not cookies)** + ConnectRPC over protobuf. Needs `leveldb` reader + `prost`. *(spec 43)*
23. **StepFun stores raw username + password** and runs a 3-step login flow. DPAPI-wrap on Windows; consider dropping password mode in v1. *(spec 43, 60)*
24. **Doubao consumes a real chat request per probe** (`max_tokens: 1` "hi") — costs the user. Warn or rate-limit. *(spec 43)*
25. **Warp rejects anything that isn't literally `User-Agent: Warp/1.0`** — 429 edge limiter. *(spec 43)*
26. **Alibaba has a hand-rolled Chromium cookie decryptor.** Replace with the shared DPAPI path. *(spec 43, 60)*
27. **Kiro is CLI-only** via `kiro-cli chat /usage` with regex on ANSI output. Hard to test without a Windows `kiro-cli.exe`. *(spec 43)*
28. **OpenCode/Go parse TanStack Start JS responses with regex against hard-coded server-function IDs** that will rotate. Fragile. *(spec 43)*
29. **MiniMax has dual auth + localStorage tokens + mode-aware UI visibility logic.** `sk-cp-*` coding-plan keys take precedence over `sk-api-*` standard keys. *(spec 43)*
30. **Copilot's GitHub device flow uses VS Code client ID `Iv1.b507a08c87ecfe98`** — preserve, do not regenerate. *(spec 42)*
31. **Cursor's headline percent has a 6-rule precedence ladder** — `plan.totalPercentUsed` → averaged Auto/API → individual `overall` → `teamUsage.pooled`. Don't simplify. *(spec 42)*

### Security

32. **V20 (App-Bound Encryption) Chrome cookies are a real cliff** — `chatgpt.com` and `claude.ai` are early adopters of `Strict`. Manual-paste fallback is mandatory polish, not optional. *(spec 60)*
33. **Token-account `token` field is plaintext in `config.json` on macOS** (file perms only). On Windows the file is Explorer-browsable, so we **must** DPAPI-wrap on Windows even though Mac doesn't. *(spec 60)*
34. **Credential Manager mirror trade-off** — file canonical, Credential Manager as convenience copy doubles attack surface. Confirm trade-off vs dropping the mirror. *(spec 60)*

## Magic-number / "polish" highlights worth knowing globally

Pulled from across the specs — bookmark these:

- **Tray icon**: 18×18 pt base, render @2× = 36×36 px → produce ICO atlas at 16/20/24/28/32/36/40/48/64 px. Theme via `WM_SETTINGCHANGE`/`ImmersiveColorSet`. Animation cap **30 Hz**, hard duration ceiling **30 s**. *(spec 10, 80)*
- **Popup window**: 360 × 480 px default, content-sized. Corner radius **12 px** for the panel, **6 px** for hover highlight. Mica on Win 11, Acrylic fallback on Win 10. *(spec 15)*
- **Bars**: 6 px height, 4 px corner radius. Pace tip = 3 × 2 px stripes. Warning markers default `[50, 20]`, range 0–99. *(spec 15)*
- **Switcher**: row heights 30/36/40 px; multi-row threshold at 15 providers. *(spec 15)*
- **Token-account stack**: max **6** entries before forcing a switcher bar. *(spec 15, 60)*
- **Critter blink**: 3–12 s random interval per provider (intentionally desynced), 360 ms duration, `pow(symmetric, 2.2)` curve, 18 % double-blink chance with 220–340 ms inter-blink. *(spec 10, 80)*
- **Confetti**: 6 fireworks fans staggered 60 ms apart, hue offsets `[0, 0.08, 0.16, 0.5, 0.66, 0.83]`, 5 s lifetime. *(spec 80)*
- **Quota flash**: held a full **60 seconds** so users back from coffee still catch it. *(spec 80)*
- **Press-scale everywhere**: 0.94 / 120 ms ease-out. Copy-flash 900 ms then 200 ms fade. *(spec 80)*
- **Loading patterns**: six named — knightRider / cylon / outsideIn / race / pulse / unbraid — with `π/2`, `π/3`, `π` secondary-bar phase offsets. *(spec 10, 80)*
- **Sound rule**: `NSSound("Glass")` plays **before** the silent toast so muting notifications never silences the quota warning. Only the master toggle does. *(spec 80)*
- **Threshold re-arm**: lower thresholds re-arm on recovery; higher thresholds stay armed. Prevents "quota low" spam. *(spec 80)*
- **Voice**: no exclamation marks except the tagline. `·` separator with thin spaces. `≈` for approximate. Em-dashes. Lowercase compact units (`1d 4h`). *(spec 80)*
- **Tagline (preserve)**: *"May your tokens never run out — keep agent limits in view."* *(spec 80)*
- **Refresh cadence presets**: Manual / 1 m / 2 m / 5 m (default) / 15 m / 30 m. Status feed rides the same tick. *(spec 50, 55)*
- **Per-monitor DPI**: opt into V2 awareness; redraw on `WM_DPICHANGED`. *(spec 10)*
- **Smallest-icon collapse rule**: 16 px ICO entry uses a simplified single-bar to stay legible. *(spec 10)*

## v1 decisions to make explicit before code lands

Carried forward from the parent docs and refined by deep research:

1. **Project name** — "CodexBar" or distinct. Drives every other identifier.
2. **Signing org + cert budget** — OV ≥ $200/yr minimum; EV for instant SmartScreen reputation.
3. **Path 1 (rebase on Win-CodexBar) vs Path 2 (rebuild in place using Win-CodexBar as reference)** — see `../04-recommended-architecture.md`. **Default: Path 2**, per user direction, with permission to copy Tauri+Rust scaffolding from Win-CodexBar.
4. **Provider set at v1** — recommend trimming to **Claude / Codex / Cursor / Copilot / Gemini / OpenRouter / Factory** for v1, with the rest in v1.1.
5. **Drop in v1 explicitly** — widgets, Safari support, Sparkle macOS appcast, `ictool` icon pipeline, full-screen Vortex confetti (replace with tray-icon morph + toast + in-popup confetti if popup is open).
6. **Telemetry** — opt-in error reports only; no analytics. Matches Mac.
7. **WSL stance** — Win-CodexBar supports it. Default: yes for the CLI path (Claude CLI runs natively on Windows but some users will already have it under WSL).
8. **MSIX/Store vs Inno + Winget** — recommend Inno + Winget at v1; revisit MSIX after first release.

## Open items the deep read could not resolve

- **Vertex AI Windows cost-tracking path** — the `@`-in-model-name separator detection is documented but Claude Code's Windows logging shape was not verifiable from inside this repo. Flagged in [42](42-providers-tier1.md). Test on a real Windows install before shipping Vertex AI.
- **Gemini `bundle/`-walk JS extraction hit rate** in real-world installs (vs. legacy/fnm paths) wasn't testable. Flagged in [42](42-providers-tier1.md). Likely needs telemetry-during-beta to confirm.
- **CLI parsers for Kiro and OpenCode/Go** depend on output stability (ANSI sequences, hard-coded server-function IDs respectively). Treat as "ship with test fixtures and pin the parser version" — plan for breakage. *(spec 43)*

## How to use this blueprint in PRs

For any non-trivial PR touching a subsystem:

1. Reference the matching spec doc in the PR description (e.g., *"Implements [50-refresh-state-pace.md §6-7](spec/50-refresh-state-pace.md)"*).
2. Tick the relevant items from that spec's acceptance checklist.
3. If you intentionally deviate, note it in the PR and update the spec.
4. If you discover the spec is wrong, fix the spec in the same PR.

The spec is **living documentation**. The "fix during port" list above (items 1–34) is meant to be drained, not preserved.

## Provenance

Generated 2026-05-12 from a deep parallel read of the macOS Swift sources in this fork (commit `009420a7`). 14 subsystem agents, ~14k lines of behavioral spec, no Swift translation. Sources are pointed to with `file:line` references inside each chapter.
