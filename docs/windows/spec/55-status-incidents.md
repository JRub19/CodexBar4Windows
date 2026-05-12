---
title: "55 — Provider Status Polling + Incident System"
audience: "Rust + TypeScript engineer (no Swift)"
target_stack: "Tauri 2 + React + shared Rust crate"
polish_bar: "Phantom-wallet / Duolingo"
status: "Spec (Mac behavior captured; Windows port pending)"
source_refs:
  - "Sources/CodexBar/UsageStore+Status.swift"
  - "Sources/CodexBar/UsageStore.swift (refreshStatus)"
  - "Sources/CodexBar/UsageStoreSupport.swift (ProviderStatus, ProviderStatusIndicator)"
  - "Sources/CodexBar/IconRenderer.swift (drawStatusOverlay)"
  - "Sources/CodexBar/StatusItemController+Animation.swift (aggregation + brand overlay)"
  - "Sources/CodexBar/PreferencesProviderSidebarView.swift (ProviderStatusDot)"
  - "Sources/CodexBar/MenuDescriptor.swift (statusLine, Status Page action)"
  - "Sources/CodexBar/StatusItemController+Actions.swift (openStatusPage)"
  - "Sources/CodexBarCore/Providers/Providers.swift (ProviderMetadata fields)"
  - "Sources/CodexBarCore/Providers/*/?ProviderDescriptor.swift"
  - "Sources/CodexBarCLI/CLIPayloads.swift (StatusFetcher mirror)"
  - "docs/status.md"
---

# 55 — Provider Status Polling + Incident System

This subsystem is a **separate cadence from usage polling**. It does NOT call any vendor account / auth API. It only fetches **public status feeds** (Statuspage.io and Google Workspace `appsstatus`), maps them to a small severity enum, and surfaces incidents in three places:

1. A small badge **overlay on the tray icon** (the morphing CodexBar icon).
2. A **per-provider status line** at the bottom of the popover card for that provider.
3. A **status dot** next to the provider name in Preferences → Providers sidebar.

There is also a "Status Page" menu action that opens the vendor's public status URL in a browser.

**Goal of this doc:** give a Rust/TS engineer everything needed to re-implement on Windows in Tauri 2 + React without reading any Swift.

---

## 1. Concept summary (one screen)

| Aspect                  | Behavior                                                                                       |
| ----------------------- | ---------------------------------------------------------------------------------------------- |
| Cadence                 | Piggybacks on the usage-refresh tick (default 5 min). One status fetch per refresh cycle, per provider with a feed. |
| Transport               | Pure HTTPS JSON. 10-second timeout. No auth, no cookies, no PTY, no platform APIs.            |
| Concurrency             | Status fetches run **in parallel with** usage fetches in the same `TaskGroup` (Rust: `JoinSet`). |
| Severity model          | 6-state enum: `none / minor / major / critical / maintenance / unknown`.                       |
| Persistence             | In-memory only. Last successful snapshot is kept across failures (sticky, no flap).            |
| Failure surface         | If we have **never had** a snapshot for this provider, surface `.unknown` + error text. If we **had** a prior snapshot, keep showing it. Never surface a toast/alert. |
| Aggregation (tray icon) | Walk display-enabled providers in user-configured order; show the first that `hasIssue`. **Order-based, not severity-based.** |
| User control            | One global toggle: Settings → Advanced → "Check provider status" (default ON).                |
| Localization            | Indicator labels are localized strings (`status_operational`, `status_partial_outage`, ...).   |

Out of scope for this subsystem: GitHub Atom RSS feeds, vendor-specific webhooks, push notifications when an incident starts, history view, per-component drill-down.

---

## 2. Per-provider status endpoint catalog

Two endpoint shapes are polled by the app (a third shape, `statusLinkURL`, is **link-only** — the menu offers a browser link but no polling happens).

### 2.1 Polled feeds (returns a `StatusSnapshot`)

| Provider     | Feed shape       | Polled URL                                                                                                                | Note                                  |
| ------------ | ---------------- | ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- |
| Codex (OpenAI)  | Statuspage.io | `https://status.openai.com/api/v2/status.json`                                                                            | Same upstream as OpenAI API           |
| OpenAI API   | Statuspage.io    | `https://status.openai.com/api/v2/status.json`                                                                            | Shares status with Codex              |
| Claude       | Statuspage.io    | `https://status.claude.com/api/v2/status.json`                                                                            | Note: `claude.com`, not `anthropic.com` |
| Cursor       | Statuspage.io    | `https://status.cursor.com/api/v2/status.json`                                                                            |                                       |
| Factory (Droid) | Statuspage.io | `https://status.factory.ai/api/v2/status.json`                                                                            |                                       |
| Copilot (GitHub) | Statuspage.io | `https://www.githubstatus.com/api/v2/status.json`                                                                         | GitHub uses Statuspage too            |
| Gemini       | Google Workspace | `https://www.google.com/appsstatus/dashboard/incidents.json` filtered by product ID `npdyhgECDJ6tB66MxXyo`                | Returns array of all GWS incidents; we filter locally |
| Antigravity  | Google Workspace | same URL, same product ID `npdyhgECDJ6tB66MxXyo`                                                                          | Antigravity rides on Gemini's GWS product |

URL construction rule (Statuspage): `${statusPageURL}/api/v2/status.json`. The base URL stored in metadata may or may not have a trailing slash; the path appender must handle both.

### 2.2 Link-only providers (no polling — just a menu link)

These providers expose a `statusLinkURL` but no machine-readable feed. The "Status Page" menu entry opens the link in the user's browser; **no incident state is ever produced**.

| Provider             | `statusLinkURL`                                                          |
| -------------------- | ------------------------------------------------------------------------ |
| Alibaba Coding Plan  | `https://status.aliyun.com`                                              |
| Antigravity (link)   | `https://www.google.com/appsstatus/dashboard/products/npdyhgECDJ6tB66MxXyo/history` |
| DeepSeek             | `https://status.deepseek.com`                                            |
| Gemini (link)        | `https://www.google.com/appsstatus/dashboard/products/npdyhgECDJ6tB66MxXyo/history` |
| Kiro (AWS)           | `https://health.aws.amazon.com/health/status`                            |
| Mistral              | `https://status.mistral.ai`                                              |
| OpenRouter           | `https://status.openrouter.ai`                                           |
| Perplexity           | `https://status.perplexity.com/`                                         |
| Vertex AI (GCP)      | `https://status.cloud.google.com`                                        |

### 2.3 No status surface at all

The following providers expose neither `statusPageURL` nor `statusLinkURL`. They never show an indicator and never get a "Status Page" menu entry: Abacus, Amp, Augment, Codebuff, CommandCode, Crof, Doubao, JetBrains, Kilo, Kimi, KimiK2, Manus, MiMo, MiniMax, Moonshot, Ollama, OpenCode, OpenCodeGo, StepFun, Synthetic, Venice, Warp, Windsurf, Zai. The Windows port should keep this gracefully unobtrusive: no UI affordance for these.

### 2.4 Fields extracted

#### Statuspage.io (`api/v2/status.json`)

Schema is well-defined and stable across Atlassian Statuspage tenants.

| JSON path             | Mapped to                          |
| --------------------- | ---------------------------------- |
| `status.indicator`    | `StatusSeverity` (see 3.1 below)   |
| `status.description`  | `incident_title` (human summary)   |
| `page.updated_at`     | `updated_at` (ISO 8601)            |

Indicator values returned by Statuspage and how they map:

| Statuspage `indicator` | App severity     |
| ---------------------- | ---------------- |
| `none`                 | `none`           |
| `minor`                | `minor`          |
| `major`                | `major`          |
| `critical`             | `critical`       |
| `maintenance`          | `maintenance`    |
| *unknown / missing*    | `unknown`        |

The component list and per-component status are **not** consumed by this app. We deliberately stay at the page-level rollup.

#### Google Workspace (`appsstatus/dashboard/incidents.json`)

Returns an array of incident objects. Each incident has:

| Field                          | Used for                                                              |
| ------------------------------ | --------------------------------------------------------------------- |
| `begin`, `modified`, `end`     | Active filter (`end == null`) and `updated_at`                        |
| `external_desc`                | Fallback summary text                                                 |
| `status_impact`                | Primary severity signal (`AVAILABLE / SERVICE_INFORMATION / SERVICE_DISRUPTION / SERVICE_OUTAGE / SERVICE_MAINTENANCE / SCHEDULED_MAINTENANCE`) |
| `severity`                     | Fallback severity (`low / medium / high`)                             |
| `affected_products[].id`       | Filter by provider's product ID                                       |
| `currently_affected_products[].id` | Preferred filter when present                                     |
| `most_recent_update.{when,status,text}` | Used for updated-at and summary text                          |
| `updates[]` (last entry)       | Fallback when `most_recent_update` missing                            |

Workspace status → app severity mapping (priority order):

```
AVAILABLE                                        -> none
SERVICE_INFORMATION                              -> minor
SERVICE_DISRUPTION                               -> major
SERVICE_OUTAGE                                   -> critical
SERVICE_MAINTENANCE | SCHEDULED_MAINTENANCE      -> maintenance
fallback by severity field: low/medium/high      -> minor/major/critical
```

When multiple incidents are active for the same product, pick the **most severe** (rank: `critical > major > minor > maintenance | unknown > none`). The `description` is derived from the chosen incident's most-recent update text, with markdown bullets / link syntax stripped.

---

## 3. Status models

### 3.1 `StatusSeverity` enum (was `ProviderStatusIndicator`)

| Variant       | Color (mac)    | Has issue? | UI label key            | English label    |
| ------------- | -------------- | ---------- | ----------------------- | ---------------- |
| `none`        | green          | no         | `status_operational`    | Operational      |
| `minor`       | yellow         | yes        | `status_partial_outage` | Partial outage   |
| `major`       | orange         | yes        | `status_major_outage`   | Major outage     |
| `critical`    | red            | yes        | `status_critical_issue` | Critical issue   |
| `maintenance` | gray           | yes        | `status_maintenance`    | Maintenance      |
| `unknown`     | gray           | yes        | `status_unknown`        | Status unknown   |

Serde wire form: lowercase string, no synonyms, exactly the six variants. **Persist this enum as a string** in any cache — it's the cross-process contract used by the CLI helper too.

### 3.2 `StatusSnapshot`

```rust
pub struct StatusSnapshot {
    pub provider_id: ProviderId,
    pub severity:    StatusSeverity,
    pub title:       Option<String>, // human-readable incident summary
    pub updated_at:  Option<DateTime<Utc>>,
    pub source:      StatusSource,   // Statuspage | GoogleWorkspace
    pub fetched_at:  DateTime<Utc>,  // when *we* fetched (for freshness display)
}
```

Notes:
- `title` is the source's `description` (Statuspage) or the cleaned `update.text` (Workspace).
- `updated_at` is the vendor's reported update time (Statuspage `page.updated_at`, Workspace `update.when ?? incident.modified ?? incident.begin`).
- `fetched_at` is separate so the UI can show "12s ago" even if vendor `updated_at` is stale.

### 3.3 Incident object (Workspace only — internal)

Workspace gives us per-incident detail, but we squash it to a `StatusSnapshot` before storing. Don't leak the multi-incident structure into the UI; the spec is "show the worst current incident".

---

## 4. Cadence

| Aspect             | Value                                                                  |
| ------------------ | ---------------------------------------------------------------------- |
| Trigger            | Every usage-refresh tick when global toggle is ON                      |
| Default interval   | 5 minutes (`RefreshFrequency.fiveMinutes`)                             |
| Allowed intervals  | manual, 1m, 2m, 5m, 15m, 30m                                           |
| HTTP timeout       | 10 seconds (per request)                                               |
| Retry policy       | **None.** A failure keeps the previous snapshot and waits for the next tick. |
| Backoff            | None. The natural floor is 60s (shortest refresh interval).            |
| Manual refresh     | Status refresh participates in the user-triggered "Refresh" menu action |
| First-paint        | Status fetch is kicked off during the first refresh after launch. Until it returns, the icon overlay is hidden (severity = `.none` by default). |

**Why no separate timer?** Polling status on the same cadence as usage keeps the system simple and avoids two competing schedulers. The downside is that `manual` refresh mode disables status polling too — document this and consider a Windows improvement (see §9 below).

**Battery saver:** the existing OpenAI-web batter-saver knob is *unrelated*; status polling never runs OpenAI web scrapes. Don't reuse that gate.

---

## 5. Surfaces

### 5.1 Tray-icon overlay (the badge)

The morphing tray icon gets a tiny mark drawn into the bottom-right corner when the aggregated severity is an issue.

| Severity                | Glyph                                              | Position (in 16×16 logical icon) | Color           |
| ----------------------- | -------------------------------------------------- | -------------------------------- | --------------- |
| `none`                  | (none — overlay skipped)                           | —                                | —               |
| `minor`, `maintenance`  | 4×4 filled circle                                  | x = w-6, y = 2                   | system label color |
| `major`, `critical`, `unknown` | 2×6 vertical pill + 2×2 dot below (mini "!" mark) | pill at (w-6, 4); dot at (w-6, 2) | system label color |

Mac uses `NSColor.labelColor` so the badge contrasts the menubar regardless of light/dark mode. **Windows port:** use the system text color from `GetSysColor(COLOR_WINDOWTEXT)` / Tauri's `theme()` so the badge stays visible against the user's taskbar background; do **not** hardcode RGB.

Rendering rules:
- Snap to nearest device pixel (mac multiplies by `outputScale` then rounds). On Windows, snap to the DPI-scaled pixel grid; subpixel-soft badges look broken at the tray's small size.
- Antialias the dot, NOT the pill (the pill is a 2px sliver — AA muddies it). On mac the pill is drawn with antialias on, which is fine because the corners are rounded; on Windows test both because the rasterizer differs.
- The overlay is drawn **on top of** the brand icon image when "merged icons" mode is off, and on top of the unified CodexBar morph icon when merged mode is on.

### 5.2 Menu pill (status line in the provider card)

At the bottom of each provider's section in the popover, after the "Status Page" action, a secondary text row appears IFF the provider has an issue:

```
{label} — {freshness}
```

- `label` = `status.description` if non-empty, else the indicator's localized label.
- `freshness` = e.g. "12s ago", "3m ago" (existing `UsageFormatter.updatedString(from:)` helper). On Windows use `dayjs.fromNow()` or an equivalent.

Visual treatment (mac):
- Rendered as `.text(string, .secondary)` — secondary-text color, no clickable affordance.
- No background pill, no border. The "pill" name is conceptual; the polish target says: small caps optional, 10-11pt, secondary-text color, tabular numbers for the freshness.

For Phantom/Duolingo polish, the Windows version should:
- Wrap the text in a soft chip background tinted by severity (10% opacity of severity color over the popover surface).
- Severity dot (6px) to the left of the text, full color (not desaturated).
- Truncate `label` to one line with ellipsis at ~36 chars; the full text goes into the chip's hover tooltip.
- Tap target: clicking the chip opens the vendor's status page (same target as the "Status Page" menu action).

### 5.3 Tooltip

The macOS popover items get a `toolTip` set to the same text shown in the row, including the freshness. The Windows port should mirror this for the chip's tooltip but also add the vendor `updated_at` in a second line:

```
Major outage — 1m ago
Vendor updated 2 min before that
```

### 5.4 Preferences sidebar dot

Inside Preferences → Providers, each provider row has a 6×6 colored circle next to its name when `statusChecksEnabled` is true:

| Severity              | Color  |
| --------------------- | ------ |
| `none`                | green  |
| `minor`               | yellow |
| `major`               | orange |
| `critical`            | red    |
| `maintenance`         | gray   |
| `unknown`             | gray   |

`accessibilityHidden = true` on mac — screen readers read the textual label that follows. Mirror this on Windows: the dot is decorative; `aria-hidden="true"` on the chip's color swatch, but the chip itself must be focusable and announce the severity label.

---

## 6. Aggregation (when multiple providers have incidents)

The tray-icon overlay shows **at most one** badge. The picker rule is:

```
for provider in store.enabledProvidersForDisplay():     # user's configured order
    indicator = store.statusIndicator(for: provider)
    if indicator.hasIssue:
        return indicator
return .none
```

Key properties:
- **First match wins, not most-severe.** A `.minor` on the top provider beats a `.critical` on a lower-ordered provider.
- Order comes from the user's drag-to-reorder list in Preferences.
- Disabled providers and providers with no feed never contribute.

For merge-icons mode (single CodexBar icon instead of stacked per-provider icons), the same rule applies: aggregate down to one severity, draw overlay once.

**Windows port:** keep this contract. The "first match wins" rule lets users prioritize the provider they care about most. Document it in the Settings tooltip ("Status badge follows your provider order").

---

## 7. Failure modes

| Scenario                                 | Behavior                                                                 |
| ---------------------------------------- | ------------------------------------------------------------------------ |
| Feed unreachable, no prior snapshot      | Store `severity=unknown`, `title = error.localizedDescription`. UI shows the gray dot + a "Status unknown" chip in the menu, but **no toast**. |
| Feed unreachable, had a prior snapshot   | Silently keep the prior snapshot. Don't even update `fetched_at`. The freshness display will quietly age — that's the only user-visible signal. |
| Feed returns 200 with garbage JSON       | Treated like unreachable. Same fail-silent path.                          |
| Feed returns 200 with `indicator` = unknown value | Map to `severity.unknown`. Description still surfaced if present.   |
| Workspace feed returns 200 but no incidents for our product ID | `severity = none`. This is the steady state — most of the time. |
| Workspace feed has multiple active incidents | Pick the most severe (see §2.4). Title comes from that incident.       |
| Toggle is OFF in settings                | `refreshStatus` returns immediately. All accessors return `nil` / `.none`. The Settings sidebar hides its dots. The tray icon overlay is suppressed. |
| User flips toggle ON                     | The next refresh tick will populate. There is no eager fetch — the first paint is delayed up to one refresh interval. (Consider a kick-on-toggle improvement.) |

Logging policy: status fetch errors should log at debug level only. Don't put them in the provider's error pane — that pane is for usage errors. Mixing them confuses users.

---

## 8. Mac → Windows mapping

The whole subsystem is **pure HTTP + JSON + a small enum + ~30 lines of rendering math**. There is nothing platform-specific in the data layer.

| Mac layer                                       | Windows equivalent                                                                |
| ----------------------------------------------- | --------------------------------------------------------------------------------- |
| `URLSession.shared.data(for:)`                  | `reqwest::Client` with `tokio` runtime in the shared Rust crate                   |
| `JSONDecoder` + custom `dateDecodingStrategy`   | `serde_json` + `chrono::DateTime::parse_from_rfc3339` (fall back to `%Y-%m-%dT%H:%M:%S%.3fZ` and `%Y-%m-%dT%H:%M:%SZ`) |
| `ISO8601DateFormatter` with `.withFractionalSeconds` | Try fractional first, then non-fractional. Same fallback chain as the Mac code |
| `URLSession` timeout 10s                        | `Client::builder().timeout(Duration::from_secs(10))`                              |
| `Task { ... }` inside `withTaskGroup`           | `JoinSet<Result<StatusSnapshot, StatusError>>` in tokio                           |
| `@MainActor` writes to `statuses[provider]`     | `Arc<RwLock<HashMap<ProviderId, StatusSnapshot>>>` updated in a single critical section per fetch; UI subscribes via a Tauri event |
| `NSColor.labelColor`                            | CSS `var(--system-text)` derived from `window.matchMedia('(prefers-color-scheme: dark)')` and `@media (forced-colors: active)`; for the native tray badge, use `GetSysColor(COLOR_WINDOWTEXT)` |
| Drawing the icon overlay in Core Graphics       | Tauri's tray icon API (`tauri::tray::TrayIconBuilder::icon`) consumes an RGBA buffer. Use `tiny-skia` or `image` crate to composite badge over base PNG. |
| `NSWorkspace.shared.open(url)` for Status Page  | `tauri::api::shell::open` (or `webbrowser::open` from a Rust command)              |
| Localized strings via `L("status_...")`         | `react-i18next` with the same keys                                                 |

**Color codes for the menu chip (recommended):**

| Severity      | Light mode chip background | Dark mode chip background | Dot/text color           |
| ------------- | -------------------------- | ------------------------- | ------------------------ |
| `none`        | n/a (chip hidden)          | n/a                       | n/a                      |
| `minor`       | `#FFF4D6` (10% on `#F5B100`) | `#3A2C00`               | `#F5B100`                |
| `major`       | `#FFE2C2` (10% on `#FF8A1F`) | `#3A1F00`               | `#FF8A1F`                |
| `critical`    | `#FFD6D6` (10% on `#E5484D`) | `#3A0C0C`               | `#E5484D`                |
| `maintenance` | `#E2E2E2`                  | `#2A2A2A`                 | `#888888`                |
| `unknown`     | `#E2E2E2`                  | `#2A2A2A`                 | `#888888`                |

When the user has Windows in **high-contrast mode** (`@media (forced-colors: active)`), drop chip backgrounds and use a 1px solid border in the severity color instead — system colors will override anyway.

When the user has set a Windows **accent color**, do NOT override severity colors with the accent. Severity is semantic, not stylistic. Accent color may be used for the *chip's outline on hover* if you want a polish touch.

---

## 9. Implementation sketch (Rust side)

A reasonable Rust-crate shape (the React side just subscribes):

```rust
pub trait StatusFeed: Send + Sync {
    async fn fetch(&self, client: &reqwest::Client) -> Result<StatusSnapshot, StatusError>;
}

pub struct StatuspageFeed { pub provider: ProviderId, pub base_url: String }
pub struct WorkspaceFeed  { pub provider: ProviderId, pub product_id: String }

pub struct StatusPoller {
    client:    reqwest::Client,
    feeds:     Vec<Box<dyn StatusFeed>>,
    snapshots: Arc<RwLock<HashMap<ProviderId, StatusSnapshot>>>,
    events:    tauri::AppHandle,
}
```

Notable details:
- `feeds` is built once from the static descriptor table. It does NOT mutate at runtime.
- One `reqwest::Client` reused across all feeds for HTTP/2 connection pooling.
- `StatusError` carries the previous snapshot reference so the poll task can decide "keep prior vs. surface unknown" without touching the store from outside.
- After each tick, fire a Tauri event (`status-updated`) with the changed provider IDs. The React side debounces 100 ms and re-renders only affected chips.

Suggested Windows improvement: when the user flips "Check provider status" ON, fire a one-shot fetch for every feed immediately instead of waiting up to a full refresh interval. This is a 5-line change in Mac too but isn't present today.

---

## 10. Acceptance checklist

For the Windows port to be considered **at parity** with mac:

- [ ] Setting toggle persists to local config; ON by default.
- [ ] Statuspage feeds polled for: Codex/OpenAI, OpenAI API, Claude, Cursor, Factory, Copilot (GitHub).
- [ ] Workspace feed polled (one request) and filtered for: Gemini, Antigravity (both share product ID `npdyhgECDJ6tB66MxXyo`).
- [ ] Link-only providers show a "Status Page" menu entry but never produce a status snapshot.
- [ ] Severity enum is exactly the 6 variants in §3.1; wire form is lowercase string.
- [ ] Statuspage `indicator` values map per §2.4 table; unrecognized values map to `unknown`.
- [ ] Workspace `status_impact` and `severity` map per §2.4; most-severe-wins per provider.
- [ ] Description text strips markdown bullets and `[label](url)` link syntax to plain `label`.
- [ ] 10-second per-request timeout; one fetch per refresh tick per provider.
- [ ] HTTP failure with a prior snapshot **silently keeps** the prior snapshot.
- [ ] HTTP failure with no prior snapshot surfaces `severity=unknown` with the error string as title.
- [ ] Status state is in-memory only — no disk cache persisted across app restarts. (Mac parity. Optional improvement: persist last-known so the first paint after relaunch isn't blank.)
- [ ] Tray icon overlay drawn at correct DPI; pixel-snapped; uses system text color.
- [ ] Tray icon overlay aggregates **first-match-wins** in user's display order.
- [ ] Menu chip shows `{label} — {freshness}` only when `severity.has_issue()`.
- [ ] Menu chip background respects light/dark/high-contrast Windows themes.
- [ ] Preferences sidebar dot (6×6) shows correct color per severity.
- [ ] Clicking "Status Page" opens the vendor URL in the user's default browser (`statusPageURL` if set, else `statusLinkURL`).
- [ ] All severity labels are localized; same keys as mac (`status_operational`, etc.).
- [ ] Status polling does not run when toggle is OFF; accessors return `none`.
- [ ] Status polling does not introduce a second timer thread; it rides the usage refresh loop.
- [ ] Tests cover: Statuspage parse, Workspace multi-incident merge, severity-wins ordering, fail-silent on prior snapshot.

---

## 11. Edge cases worth testing explicitly

| Case                                                                                  | Expected                          |
| ------------------------------------------------------------------------------------- | --------------------------------- |
| Statuspage returns `status.indicator: ""` (empty string)                              | `unknown`                         |
| Statuspage returns 200 with `page.updated_at` missing                                 | snapshot with `updated_at = None`; UI shows label without freshness |
| Workspace feed returns `[]`                                                           | All workspace providers → `none`  |
| Workspace incident with `end != null` (resolved)                                      | Filtered out                      |
| Workspace incident with both `affected_products` and `currently_affected_products`    | `currently_affected_products` wins |
| Network offline for 60 minutes, then back online                                      | Last good snapshot stays. On reconnect, next tick refreshes silently. No catch-up storm. |
| User toggles status OFF while a fetch is in flight                                    | Result is discarded; no state mutation; no log line surfaced to UI |
| Two providers (Gemini + Antigravity) share one Workspace fetch                        | Either cache the HTTP response for 30 s OR dedupe the request in-flight. Avoid two identical HTTP requests per tick. |
| Provider with `statusPageURL` AND `statusLinkURL` set (e.g., Cursor + ...)            | `statusPageURL` is used for polling. The menu's "Status Page" action prefers `statusPageURL`, falling back to `statusLinkURL`. |
| Statuspage feed returns 302 redirect                                                  | `reqwest` follows by default — fine. Confirm timeout still respected end-to-end. |

---

## 12. Telemetry / debug surface (recommended for Windows)

Mac doesn't expose anything special. For Windows polish, add a hidden **Debug → Status Probes** panel showing:

- Per-provider: last severity, last `updated_at`, last `fetched_at`, last error string (or `OK`).
- A "Refetch all" button.
- A toggle to log raw HTTP responses to a ring buffer for debugging (off by default).

Not required for parity. Useful when a vendor changes their feed shape and someone needs to reproduce.

---

## 13. Glossary (terminology used in this doc)

| Term              | Meaning                                                                                |
| ----------------- | -------------------------------------------------------------------------------------- |
| Feed              | One pollable HTTPS endpoint that returns a parseable status payload                    |
| Snapshot          | The latest result we have for a provider, stored in memory                             |
| Severity          | One of 6 enum values that drives every UI surface                                      |
| Pill / chip       | The colored text row in the popover that shows incident summary                        |
| Dot               | The 6×6 colored circle in Preferences sidebar                                          |
| Badge / overlay   | The tiny mark drawn in the corner of the tray icon                                     |
| Link-only         | Provider with no feed; we only show a browser link in the menu                         |
| Workspace         | Google Workspace Status Dashboard, used for Gemini + Antigravity                       |
| Statuspage        | Atlassian Statuspage.io — the standard `api/v2/status.json` shape used by most vendors |

---

## 14. Open questions for the Windows port

1. Should the menu chip be **clickable** (open status page) or **decorative**? Mac is decorative; "Status Page" is a separate menu row. Phantom-level polish argues for clickable, with the dedicated menu row removed when chip is clickable to avoid two affordances.
2. Should we **persist** the last good snapshot to disk so first launch after reboot shows known state immediately? Mac doesn't. Trivial to add; helps perceived snappiness.
3. Should we **dedupe** the shared Gemini + Antigravity Workspace request, or just fetch twice? Trivial dedupe; recommend yes.
4. Should we surface a **toast** when severity transitions `none → critical` for a user-favorited provider? Mac does not. Out of scope for parity; a great v2 polish item, tie it to existing quota-warning notification permission.
5. Should the **Settings toggle** kick off an immediate fetch on flip-ON? Mac doesn't; recommend yes.

These are explicit "polish opportunities the Mac app didn't ship" — flag them but don't block parity on them.
