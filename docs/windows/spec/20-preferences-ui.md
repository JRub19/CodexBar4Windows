# 20 — Preferences / Settings UI (Windows port spec)

Target stack: Tauri 2 host + React/TypeScript renderer + shared Rust crate (`codexbar-core`).
Mac source of truth: `Sources/CodexBar/Preferences*.swift`, `Sources/CodexBar/SettingsStore*.swift`,
`Sources/CodexBar/Providers/*/<Name>SettingsStore.swift`, `docs/configuration.md`, `docs/ui.md`.

This spec documents *behavior, layout, defaults, validation, persistence* — it never asks the
Windows implementer to read Swift. Aim for Phantom-wallet / Duolingo levels of polish: every
transition spring-eased, every error inline and friendly, every toggle audited for "what does
this disable downstream?".

---

## 1. Window chrome

| Attribute | macOS (current) | Windows (target) |
|---|---|---|
| Window class | SwiftUI `Settings` scene with `TabView` | Tauri webview window `settings` (label-pinned, single-instance) |
| Default size | Width 546pt, height 638pt | Width 880px, height 640px (DPI-scaled) |
| Providers pane width | 792pt (wider than other panes) | Same window stays 880px wide; sidebar fixed 248px |
| Resizable | No (size is animated per tab via spring `response: 0.32, damping: 0.85`) | Yes (min 720×560, default 880×640); persist last size to `windowState.json` |
| Background material | `NSVisualEffectView` (settings material) | **Mica** (`apply_mica = true`) on Win11; fall back to `acrylic` on Win10; final fallback solid `#1B1B1F`/`#FFFFFF` (theme-aware) |
| Title bar | Standard macOS title bar | Custom (Tauri `decorations: false`) with a 32-px title strip: app icon, title "CodexBar Settings", min/max/close buttons (Segoe Fluent Icons `  `). Snap layout zones must work. |
| Chrome height | Title bar ~28pt | 32 px non-client + 1 px Mica seam |
| Rounded corners | system | 8 px Win11 (system); square on Win10 |
| Drop shadow | system | system (let DWM render) |
| Modal | No (independent window) | No; opens via tray menu "Settings…" or hotkey |
| Frame transition | Spring resize on tab change | Do **not** animate window resize — content area changes; window size is fixed unless user-dragged |
| Multi-monitor | macOS remembers | Persist `(monitor_id, x, y, w, h)` on close; clamp into nearest visible monitor on open |

### Visual language ("Phantom/Duolingo polish")

- 8-pt baseline grid; 12-pt for compact rows.
- Accent: `#7C5CFF` (light) / `#9C84FF` (dark) — same as menu bar.
- Section dividers: 1 px `--border-subtle` (`#00000014` / `#FFFFFF1A`).
- Card radius: 8 px (sections), 6 px (rows that highlight on hover).
- Hover affordance: row `background: var(--surface-hover)` with 120 ms ease.
- Focus ring: 2 px accent with 2 px halo, never the OS dotted ring.
- Switch/toggle: pill 36×20, knob 16×16, 180 ms cubic-bezier(.2,.8,.25,1).
- Microinteractions: success checkmarks fade in 220 ms; error states shake 1 cycle (translateX ±4 px, 280 ms).

### Animation & transition catalog (Windows port must implement)

| Surface | Trigger | Duration | Easing |
|---|---|---|---|
| Toggle knob slide | toggle click | 180 ms | cubic-bezier(.2,.8,.25,1) |
| Toggle track color | toggle click | 220 ms | ease-out |
| Pane cross-fade (sidebar nav) | pane change | 200 ms | ease-in-out, opacity-only (no slide) |
| Provider detail "Saving…" → check | autosave landed | 200 ms fade-in, 1500 ms hold, 400 ms fade-out | ease-in-out |
| Error shake | invalid commit | 280 ms (single cycle, ±4 px X) | bouncy spring (mass 0.5, stiffness 220) |
| Hover surface fill | mouse enter row | 120 ms | ease-out |
| Focus ring appear | tab focus | instant on, 100 ms off | linear |
| About icon hover scale | hover icon | 320 ms (response 0.32, damping 0.78) | spring (matches mac) |
| Confetti burst | weekly limit reset transition | 1800 ms emit, 3200 ms total | physics (gravity 0.6) |
| Threshold field commit feedback | Apply pressed | 220 ms green flash → revert | ease-out |
| Drag insertion line | drag over row | 0 ms appear (instant), 120 ms disappear | linear |
| Tray icon transition (related; not Settings) | refresh complete | uses tray subsystem, separate spec | — |

---

## 2. Sidebar / pane list

The macOS version uses a top tab bar. Windows must instead use the **left sidebar pattern** that
matches Windows 11 Settings: 220-px nav rail with grouped items, search field at the top, content
to the right. This is essential to support 30+ provider tiles and per-provider panels comfortably.

| Order | Pane | Icon (Segoe Fluent / Lucide) | Title key | Visibility |
|---|---|---|---|---|
| 1 | General | `Settings` (``) / `gear` | `tab_general` | Always |
| 2 | Providers | `AppsListDetail` (``) / `layout-grid` | `tab_providers` | Always |
| 3 | Display | `View` (``) / `eye` | `tab_display` | Always |
| 4 | Keyboard | `Keyboard` (``) / `keyboard` | `tab_keyboard` | Always (split from Advanced) |
| 5 | Advanced | `DeveloperTools` (``) / `sliders-horizontal` | `tab_advanced` | Always |
| 6 | About | `Info` (``) / `info` | `tab_about` | Always |
| 7 | Debug | `Bug` (``) / `bug` | `tab_debug` | Only when `debugMenuEnabled == true` |

### Sidebar behavior
- Selection highlight: 4-px accent bar on the left edge + `--surface-selected` background.
- Keyboard: Up/Down moves selection, Enter/Space activates, Ctrl+F focuses search.
- Search filters pane list and surfaces matching setting rows under a "Found in …" header.
- When Debug toggles off and user is on Debug pane, route to General with a 200 ms cross-fade.
- Bottom of sidebar: a small "Quit CodexBar" button (Phantom-style ghost button) to mirror the
  prominent macOS "Quit" button in General.

---

## 3. General pane

Layout: three sections separated by 1 px dividers and 24 px vertical gaps.

### 3.1 System

| Control | Type | Default | Range / Options | Notes |
|---|---|---|---|---|
| Language | dropdown | `system` | `system`, `en`, `zh-Hans`, `pt-BR` (Windows port adds `de`, `es-ES`, `ja`, `fr` if shipped) | Live apply — re-render whole window. Key `appLanguage` (empty = system). |
| Launch at sign-in | switch | `false` | bool | Calls `LaunchAtLoginManager` analogue. On Windows: Registry `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\CodexBar` with full exe path + `--hidden`. |

### 3.2 Usage / cost

| Control | Type | Default | Notes |
|---|---|---|---|
| Show cost summary | checkbox | `false` (`tokenCostUsageEnabled`) | When ON, expands inline section "Cost data status" with Claude + Codex sub-rows showing relative-time status: "Refreshing (12s)", "Updated 4m ago — $1.23", "Last attempt 1h ago", or error preview (max 120 chars). Mirrors `costStatusLine`. |
| Auto-refresh hint | static text | — | "Cost data refreshes automatically every 5 minutes." Shown only when switch is ON. |

### 3.3 Automation

| Control | Type | Default | Range / Options |
|---|---|---|---|
| Refresh cadence | dropdown | `5 minutes` (`fiveMinutes`) | `Manual`, `1 min`, `2 min`, `5 min`, `15 min`, `30 min` (`RefreshFrequency` enum; seconds = 60/120/300/900/1800) |
| Manual hint | static text | — | "Refresh manually with Ctrl+R or via the tray menu." Shown only when cadence == Manual. |
| Check provider status | switch | `true` (`statusChecksEnabled`) | Polls vendor statuspages; drives sidebar status dots. |
| Session quota notifications | switch | `true` (`sessionQuotaNotificationsEnabled`) | Toast on depleted/restored transitions. |
| Quota warning notifications | switch | `false` (`quotaWarningNotificationsEnabled`) | When ON, expands the **Quota warnings** sub-panel (see §11). |

### 3.4 Footer
Right-aligned "Quit CodexBar" button (accent prominent), 28 px tall, opens a confirmation popover
"Quit CodexBar? Tray icon and background fetchers will stop." [Quit] [Cancel].

---

## 4. Providers pane

Split layout. Sidebar list on the left, detail panel on the right.

### 4.1 Provider sidebar (left)

| Element | Spec |
|---|---|
| Width | 280 px fixed |
| Container | Rounded card (8 px), 1 px `--border-subtle` outline, `--surface-elevated` fill |
| Row height | 56 px (icon 20×20, two-line text, drag handle 12 px) |
| Order | User-reorderable via drag handle (six-dot grid on the left). Persists to `providers[]` array order in `config.json`. |
| Search | Top-of-list filter input (Ctrl+F focuses); filters by display name and CLI name. |
| Selection | Single-row; arrow keys navigate. Click selects; tap-on-toggle does not select. |
| Status dot | Right side of name: green/yellow/orange/red/gray = `none/minor/major/critical/maintenance|unknown` (only visible when `statusChecksEnabled` is ON). |
| Refresh spinner | Mini indeterminate spinner next to status dot when the row is currently refreshing. |
| Subtitle | Two lines, 2-line clamp: provider plan/source detail + "Updated 12m ago" / "Last fetch failed" / "Usage not fetched yet" / "Disabled — <reason>". |
| Enable toggle | Trailing checkbox (mac uses checkbox; on Windows use the same to preserve density — small switch is fine if checkbox feels clunky). Disabling currently-selected menu provider clears the selectedMenuProvider. |
| Drag UX | While dragging, show 6-px tinted insertion line above target row; reorder commits on drop. |

### 4.2 Provider detail (right)

Vertical scroll, max content width 640 px (centered if window is wider).

1. **Header card** (sticky on scroll)
   - Brand icon (28×28) — uses bundled provider SVG; fallback "dotted circle".
   - Display name (title3 / 16 px semibold).
   - Subtitle line: detail line • "Updated 4m ago".
   - Refresh icon button (`arrow-clockwise`), 28×28, tooltip "Refresh".
   - Enable switch (small), labels hidden.

2. **Info grid** (label/value, 12 px column gap, 6 px row gap, footnote secondary):
   - State: Enabled / Disabled.
   - Source: source label (CLI / Web / OAuth / API).
   - Version: detected CLI version or "not detected".
   - Updated: relative or "Refreshing" / "Unavailable" / "Not fetched yet".
   - Status: optional (status description).
   - Account: optional (email).
   - Plan / Balance: optional (OpenRouter, MiMo, Moonshot remap "Balance").
   - Label column width = `max(longestLabel.width)` (auto).

3. **Usage block** ("Usage" h3, 14 px spacing)
   - Per-metric row: title + horizontal usage bar (percent), percent label + reset text.
   - Optional Credits, Cost, Extra Usage subrows.
   - Pace badge: "On pace", "12% in deficit · Runs out in 2d", "8% in reserve · Lasts until reset" (`docs/ui.md` §Pace).
   - Empty: "Disabled — no recent data" (when off) or "No usage yet" (when on but no snapshot).

4. **Error card** (collapsible; only shown when last fetch failed)
   - Header: "Last fetch failed (<provider>)", footnote semibold secondary.
   - Preview: 3-line clamp of `userFacingError` (truncated at 160 chars + "…").
   - "Show details" link expands the full error.
   - Copy icon button (top right) copies full error to clipboard.

5. **Settings section** (h3 "Settings") — see per-provider catalog §5.
   - Picker rows: 92-px label column, dropdown right; optional trailing dynamic text (e.g. "Auto-detected: OAuth"); optional 2-line subtitle.
   - Field rows: title + subtitle, then text field (plain or secure). Optional footer text and inline action buttons.
   - Action rows: title + subtitle + horizontal button strip (`bordered` and `link` styles).

6. **Token accounts row** (if provider supports multiple accounts) — see §12.

7. **Codex accounts subsection** (only when `provider == .codex`) — see §4.4.

8. **Quota warnings** (always) — see §11.

9. **Options section** (h3 "Options") — toggle list with subtitle + optional status text and inline actions, exposed only when ON.

### 4.3 Provider toggle UX

| Behavior | Spec |
|---|---|
| Default enablement | Per-provider `defaultEnabled` from `ProviderMetadata`. |
| Toggle action | Writes `providers[].enabled` in config.json; emits change notification; clears menu selection if disabling the currently selected provider. |
| Auto-enable | Alibaba: auto-enables once if a coding-plan API token is found in env or config; gated by `alibabaCodingPlanAutoEnableApplied` UserDefaults flag (Windows: `HKCU\Software\CodexBar\Flags`). Setting an Alibaba token in UI also auto-enables. |
| Search | Filter sidebar list by `displayName`, `cliName`, aliases. |
| Sort | Always follows user-defined order; "Sort A–Z" command lives in sidebar overflow menu. |

### 4.4 Codex Accounts subsection (provider == codex only)

| Row | Behavior |
|---|---|
| Active picker | Dropdown of `visibleAccounts` (managed + live system). Subtitle: "Choose which Codex account CodexBar should follow." |
| System picker | Promotes a managed account to be the default system (`~/.codex` on macOS; on Windows: `%USERPROFILE%\.codex`). Disabled while authenticating/removing/promoting. |
| Account row | Email/displayName + "(System)" badge. Right-side buttons: `Re-auth`, `Remove`. |
| Add Account | Bordered button. Launches managed-account OAuth flow. Label flips to "Adding Account…" during flow. |
| Remove confirm | Modal alert: title "Remove Codex Account", body "Remove `email`?", buttons `Remove` (destructive) / `Cancel`. |
| Notice band | Warning (red) or secondary text below list for errors: missing email, workspace cancelled, unsafe managed home, store unreadable. |
| Storage unreadable | Persistent warning text "Managed account store unreadable. Re-auth or fix permissions." Disables System picker and Add. |

### 4.5 Error display rules

- `ProviderErrorDisplay.preview` = user-facing summary, max 160 chars + "…".
- `full` = raw error string; selectable + copyable.
- Expand state persists per provider in component memory only (resets on tab change).
- For copy: clipboard write + 1.5 s toast "Copied".

### 4.6 Auth source picker semantics

Most providers expose a "Usage source" dropdown via the per-provider settings catalog. Options
visible depend on what's supported. See per-provider table §5 for exact options.

| Source value | Meaning |
|---|---|
| `auto` | Provider-specific cascade (e.g. Claude: OAuth → CLI → Web). |
| `oauth` | Force OAuth API path. |
| `cli` | Force CLI PTY probe. |
| `web` | Force web-cookie path. |
| `api` | Force API-token path. |

---

## 5. Per-provider settings catalog

Master table. Each row is one persisted setting. Storage column:
- **config** = `%APPDATA%\CodexBar\config.json` → `providers[id].<field>`.
- **defaults** = `HKCU\Software\CodexBar\Defaults` (registry; replaces macOS `UserDefaults`).
- **cred** = Windows Credential Manager (DPAPI-wrapped) when secret.

Validation column:
- `nonblank` = trimmed, must not be empty (else field rejected).
- `trim` = leading/trailing whitespace stripped; empty becomes null.
- `cookie` = accepts either `Cookie: name=value; …` or `name=value; …`; strip leading `Cookie:`; reject Netscape exports with friendly hint.
- `url` = optional, parse via `Url::parse`; reject if scheme not http/https.
- `enum` = must match listed values.
- `pwd` = secure field; not echoed.

| Provider | Setting key | Type | Default | Validation | When hidden | Depends on | Storage |
|---|---|---|---|---|---|---|---|
| **abacus** | `cookieSource` | enum picker `auto/manual/off` | `auto` | enum | — | — | config |
| abacus | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (secret marker; encrypted at rest with DPAPI in Windows port) |
| abacus | `tokenAccounts` | multi-account | empty | — | only when support is registered | — | config |
| **alibaba** | `region` | picker `international/china` | `international` | enum | — | — | config |
| alibaba | `apiKey` | secure text | — | trim | — | — | config (DPAPI) |
| alibaba | `cookieSource` | picker `auto/manual/off` | `auto` | enum | — | — | config |
| alibaba | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| **amp** | `cookieSource` | picker | `auto` | enum | — | — | config |
| amp | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| amp | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **antigravity** | `source` | picker `auto/oauth/cli` | `auto` | enum | — | — | config |
| **augment** | `cookieSource` | picker | `auto` | enum | — | — | config |
| augment | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| augment | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **claude** | `source` (`usageDataSource`) | picker `auto/oauth/web/cli` | `auto` | enum | — | — | config |
| claude | `webExtrasEnabled` | hidden (debug-only) | `false` | bool | always in UI; force-cleared when `source != cli` | source | defaults |
| claude | `peakHoursEnabled` | toggle | `true` | bool | — | — | defaults |
| claude | `cookieSource` | picker | `auto` | enum | — | source | config |
| claude | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | source, cookieSource | config (DPAPI) |
| claude | `oauthKeychainPromptMode` | picker `never/onlyOnUserAction/always` | `onlyOnUserAction` | enum | hidden when `disableKeychainAccess`; Windows: only visible when DPAPI-backed prompt is in play (rare) | disableKeychainAccess | defaults |
| claude | `oauthKeychainReadStrategy` | picker `securityFramework/securityCLIExperimental` | `securityCLIExperimental` | enum | hidden unless Security.framework path is active — on Windows this row is hidden entirely (no equivalent) | platform | defaults (n/a Win) |
| claude | `tokenAccounts` | multi-account (special: accepts `sessionKey` cookies or `sk-ant-oat…` OAuth tokens) | empty | trim+token-shape | — | — | config |
| **codebuff** | `apiKey` | secure text | — | trim | — | — | config (DPAPI) |
| **codex** | `source` (`usageDataSource`) | picker `auto/oauth/cli` | `auto` | enum | — | — | config |
| codex | `codexActiveSource` | composite (live system / managed account UUID) | `liveSystem` | resolver | — | — | config |
| codex | `cookieSource` | picker | `auto` (forced to `off` when `openAIWebAccessEnabled == false`) | enum | when web access disabled | openAIWebAccessEnabled | config |
| codex | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| codex | `openAIWebAccessEnabled` | toggle (in Options) | inferred from history (true if existing config had cookies, else false) | bool | — | — | defaults |
| codex | `openAIWebBatterySaverEnabled` | toggle | `false` | bool | only when web access ON | openAIWebAccessEnabled | defaults |
| codex | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **commandcode** | `cookieSource` | picker | `auto` | enum | — | — | config |
| commandcode | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| commandcode | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **copilot** | `apiKey` | secure text (auto-cleared when token accounts added) | — | trim | — | tokenAccounts | config (DPAPI) |
| copilot | `enterpriseHost` | text | — | url-or-host | — | — | config |
| copilot | `tokenAccounts` | multi-account, **primary "Add Account" button runs device-flow login** instead of inline form | empty | device flow | — | — | config |
| **crof** | `apiKey` | secure text | — | trim | — | — | config (DPAPI) |
| **cursor** | `cookieSource` | picker | `auto` | enum | — | — | config |
| cursor | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| cursor | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **doubao** | `apiKey` | secure text | — | trim | — | — | config (DPAPI) |
| **factory** | `cookieSource` | picker | `auto` | enum | — | — | config |
| factory | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| factory | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **jetbrains** | `ideBasePath` | folder picker | empty (auto-detect) | path-must-exist | — | — | defaults (`jetbrainsIDEBasePath`); Win mapping: `%LOCALAPPDATA%\JetBrains` or `%APPDATA%\JetBrains` |
| **kilo** | `source` (`usageDataSource`) | picker `auto/api/cli` | `auto` | enum | — | — | config |
| kilo | `extrasEnabled` | toggle | `false` | bool | hidden when `source != auto` (forced false) | source | config |
| kilo | `apiKey` | secure text | — | trim | only when `source == api` (visible but optional otherwise) | source | config (DPAPI) |
| **kimi** | `cookieSource` | picker | `auto` | enum | — | — | config |
| kimi | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| **kimik2** | `apiKey` | secure text | — | trim | — | — | config (DPAPI) |
| **manus** | `cookieSource` | picker | `auto` | enum | — | — | config |
| manus | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| manus | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **mimo** | `cookieSource` | picker | `auto` | enum | — | — | config |
| mimo | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| **minimax** | `region` | picker `global/china` | `global` | enum | — | — | config |
| minimax | `apiKey` | secure text | — | trim | — | — | config (DPAPI) |
| minimax | `cookieSource` | picker | `auto` | enum | — | — | config |
| minimax | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| minimax | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **mistral** | `cookieSource` | picker | `auto` | enum | — | — | config |
| mistral | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| mistral | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **moonshot** | `region` | picker `international/china` | `international` | enum | — | — | config |
| moonshot | `apiKey` | secure text | — | trim | — | — | config (DPAPI) |
| **ollama** | `cookieSource` | picker | `auto` | enum | — | — | config |
| ollama | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| ollama | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **openai** (raw API) | `apiKey` | secure text | — | trim | — | — | config (DPAPI) |
| **opencode** | `workspaceID` | text | empty | trim | — | — | config |
| opencode | `cookieSource` | picker | `auto` | enum | — | — | config |
| opencode | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| opencode | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **opencodego** | `workspaceID` | text | empty | trim | — | — | config |
| opencodego | `cookieSource` | picker | `auto` | enum | — | — | config |
| opencodego | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| opencodego | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **openrouter** | `apiKey` | secure text | — | trim | — | — | config (DPAPI) |
| **perplexity** | `cookieSource` | picker | `auto` | enum | — | — | config |
| perplexity | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| **stepfun** | `username` | text (stored in `apiKey` field) | empty | nonblank | — | — | config |
| stepfun | `password` | secure text (stored in `cookieHeader` field) | empty | pwd | — | — | config (DPAPI) |
| stepfun | `token` | secure text (stored in `region` field — repurposed!) | empty | trim | — | — | config (DPAPI) |
| stepfun | `cookieSource` | picker | `auto` | enum | — | — | config |
| stepfun | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **synthetic** | `apiKey` | secure text | — | trim | — | — | config (DPAPI) |
| **warp** | `apiKey` | secure text | — | trim | — | — | config (DPAPI) |
| **windsurf** | `source` (`usageDataSource`) | picker `auto/web/cli` | `auto` | enum | — | — | config |
| windsurf | `cookieSource` | picker | `auto` | enum | — | — | config |
| windsurf | `cookieHeader` | secure text | — | cookie | when `cookieSource != manual` | cookieSource | config (DPAPI) |
| windsurf | `tokenAccounts` | multi-account | empty | — | — | — | config |
| **zai** | `region` | picker `global/china` | `global` | enum | — | — | config |
| zai | `apiKey` | secure text | — | trim | — | — | config (DPAPI) |

Cross-cutting per-provider rows always present (from the shared engine):

| Row | Behavior |
|---|---|
| Menu bar metric (picker `menuBarMetric`) | Per-provider preference (`MenuBarMetricPreference`). Options: `automatic`, `primary` (session), `secondary` (weekly), `tertiary` (Opus etc., when supported), `extraUsage` (when supported), `average`. Special cases: `openrouter` only `automatic` + primary API key limit; `abacus` only `automatic` + primary; balance-only providers only `automatic`. |
| Quota warnings section | See §11. |

### Inconsistencies / smells flagged for the Windows port

These are kept verbatim so the Rust/TS team can fix during port:

1. **StepFun field repurposing**: `apiKey` holds username, `cookieHeader` holds password, `region` holds an opaque Oasis-Token. Define `username/password/token` fields in the new config schema and migrate.
2. **Token-account shape variance**: Claude's token field accepts *either* a cookie header *or* an OAuth bearer (`sk-ant-oat…`); Copilot's token accounts use a device-flow login button instead of inline label+token. Standardize per-provider account-shape descriptors.
3. **Codex special-casing of `openAIWebAccessEnabled`**: legacy global toggle short-circuits `cookieSource` to `off`. Replace with a single per-provider `cookieSource` source-of-truth.
4. **Claude `webExtrasEnabled`** is force-cleared whenever source is anything but CLI but the toggle is in defaults rather than per-provider config. Co-locate.
5. **Region semantics differ**: zai/minimax use `global/china`; alibaba/moonshot use `international/china`. Normalize labels and translation keys.
6. **Mac-only Keychain prompt rows** (`oauthKeychainPromptMode`, `oauthKeychainReadStrategy`) leak into Claude settings — must be hidden on Windows (no Security.framework analogue).
7. **JetBrains base path** field is a stringly-typed text rather than a folder picker; should be a `Browse…`-backed control on Windows.
8. **OpenCode / OpenCodeGo / OpenAI** all expose nearly identical pickers — could share a generic "Workspace ID + Cookie source" component.

### 5.1 Settings row primitives (apply consistently)

| Primitive | Where used | Component shape |
|---|---|---|
| `PreferenceToggleRow` | General, Display, Advanced, Debug | Title + 2-line subtitle on the left, switch right. 5.4-px gap between title and subtitle. Subtitle uses tertiary text color. |
| `ProviderSettingsPickerRowView` | Per-provider Settings | 92-px label column + dropdown + trailing dynamic text + optional 2-line subtitle below. Disabled state dims to 50%. |
| `ProviderSettingsFieldRowView` | Per-provider Settings | Title + subtitle, then plain or secure `TextField` (`.roundedBorder`), then horizontal action button strip, then optional footer text. Activating focus emits `onActivate` (used by some providers to clear a status). |
| `ProviderSettingsActionsRowView` | Per-provider Settings | Title + subtitle + horizontal buttons (mixed `.bordered` and `.link` button styles). |
| `ProviderSettingsToggleRowView` | Per-provider Options | Title + subtitle left, switch right; when ON, exposes a status text line and an action button row. Calls `onAppearWhenEnabled` async hook each time the toggle becomes true. |
| `ProviderSettingsTokenAccountsRowView` | Per-provider Settings | Header (title + optional primary "Add Account" button) + subtitle + account list + inline form (label + secure token + Add) + "Open token file" + "Reload" link buttons. |

### 5.2 Per-provider settings catalog cross-cuts

Every provider that supports a *secure* field must:
- Mask input by default; never echo to logs.
- On commit (focus loss / blur), call `logSecretUpdate(provider, field, value)` which records only `(cleared)` / `(updated)` — never the value.
- On Windows the underlying storage encrypts the value with `CryptProtectData` and stores a `"$dpapi:<base64>"` blob in the same JSON field, so an unencrypted backup never leaves disk.

Every provider that supports `cookieSource = manual` must:
- Show the cookie header field only when `cookieSource == manual` (live conditional render).
- Accept either `Cookie: name=value; …` or `name=value; …` — strip the leading `Cookie:` and any whitespace.
- Reject Netscape cookie file dumps with a friendly inline error: "That looks like a Netscape cookie file. Convert each row to `name=value` and join them with `; `."

---

## 6. Display pane

Two sections.

### 6.1 Menu bar (tray icon on Windows)

| Control | Type | Default | Notes |
|---|---|---|---|
| Merge icons into one tray button | switch | `true` (`mergeIcons`) | When ON, a single tray icon represents all providers; built-in switcher pops out on click. When OFF, each enabled provider gets its own tray icon (Windows: register each via `tray_handle.create_icon`). |
| Switcher shows brand icons | switch | `true` (`switcherShowsIcons`) | Only enabled when Merge is ON. |
| Auto-pick highest-usage provider | switch | `false` (`menuBarShowsHighestUsage`) | Only enabled when Merge is ON. Drives which provider's percent renders in the tray. |
| Show brand icon + percent (instead of bars) | switch | `false` (`menuBarShowsBrandIconWithPercent`) | When ON, exposes the Display Mode picker below. |
| Display mode | picker | `percent` | `MenuBarDisplayMode`: `percent`, `percentDimmedBars`, `barsOnly`, `iconOnly`. Disabled unless brand-icon-with-percent is ON. |

### 6.2 Menu content (popup tray panel)

| Control | Type | Default | Notes |
|---|---|---|---|
| Show usage as used (vs remaining) | switch | `false` (`usageBarsShowUsed`) | Flips fill direction of all usage bars. |
| **Show quota warning markers** | switch | `true` (`quotaWarningMarkersVisible`) | **Added recently** (PR #918 — "Add option to hide quota warning markers"). When OFF, tick marks on usage bars hide globally (even when thresholds are otherwise configured). Note: row labelled "Show quota warning markers" but commit says "Hide" — pick "Show … markers" to match settings store key. |
| Show reset time as absolute clock | switch | `false` (`resetTimesShowAbsolute`) | OFF = countdown ("Resets in 4h 12m"), ON = clock ("Resets at 16:30"). |
| Show credits & extra usage | switch | `true` (`showOptionalCreditsAndExtraUsage`) | Adds Credits / Extra Usage rows to menu cards when available. |
| Multi-account layout | picker | `segmented` | `MultiAccountMenuLayout`: `segmented` (switcher bar) or `stacked` (up to 6 stacked cards). Legacy `showAllTokenAccountsInMenu==true` → `stacked` (migration). |
| Overview tab providers | configure-button popover | first 3 active in order | Max 3 (`mergedOverviewProviderLimit`). Disabled unless Merge is ON. Popover lists active providers as checkboxes; selection persisted as `mergedOverviewSelectedProviders` array; once user edits, a signature of active-providers-at-edit-time is stored (`mergedOverviewSelectionEditedActiveProviders`) so the selection sticks across enable/disable churn. |
| Hints | static | — | "Enable Merge Icons to configure the Overview tab." / "No active providers yet." |

---

## 7. Advanced pane (Windows split)

The macOS Advanced pane mixes a global keyboard shortcut, CLI installation, debug toggles, privacy
toggles, and Keychain controls. On Windows split into **Keyboard** (§13) and **Advanced**.

### Advanced pane contents

| Group | Control | Type | Default | Notes |
|---|---|---|---|---|
| CLI install | "Install CodexBar CLI" | button | — | Symlinks helper into `/usr/local/bin` on mac. **Windows analogue**: copy/symlink `codexbar.exe` into a writable folder on `PATH`, or add `%LOCALAPPDATA%\Programs\CodexBar` to user `PATH`. Status text shown to right after install attempt. |
| Misc | Show debug settings | switch | `false` (`debugMenuEnabled`) | Reveals Debug pane in sidebar. |
| Misc | Random blink animation ("Surprise me") | switch | `false` (`randomBlinkEnabled`) | Periodic playful blink on tray icon. |
| Misc | Confetti on weekly limit reset | switch | `false` (`confettiOnWeeklyLimitResetsEnabled`) | One-shot confetti burst when weekly window rolls over. |
| Privacy | Hide personal info in menu | switch | `false` (`hidePersonalInfo`) | Masks emails/account names in tray UI (e.g. for screensharing). |
| Privacy | Show provider storage usage | switch | `false` (`providerStorageFootprintsEnabled`) | Enables background scans of known provider-owned paths to show local disk usage. |
| Security | "Disable secret access" | switch | `false` (`debugDisableKeychainAccess`) | **macOS**: stops Keychain access for browser cookie import / OAuth caches. **Windows analogue**: stops DPAPI use + browser-cookie sniffing; user must paste cookies manually. Section caption explains the privacy/perf tradeoff. |

### 7.1 Advanced caption blocks (so users understand consequences)

Every switch in Advanced has a subtitle line; render footnote/tertiary. Example: "Stops CodexBar
from reading any saved cookies or DPAPI-protected tokens. You will have to paste Cookie headers
manually for any provider that uses web mode."

---

## 8. Debug pane (only when `debugMenuEnabled`)

Long form. Sections separated by `SettingsSection` cards with a section title + caption.

| Section | Controls | Notes |
|---|---|---|
| Logging | "Enable file logging" switch (default `false`), with subtitle showing log path `%LOCALAPPDATA%\CodexBar\Logs\codexbar.log`. Verbosity picker (`verbose/info/notice/warn/error`, default `verbose`). "Open log file" button. | Toggles forward to `CodexBarLog.setFileLoggingEnabled`. Bridge `debugFileLoggingEnabled` to renderer via Tauri command. |
| Animations | "Force animation next refresh" switch. | Used to manually replay loading animation. |
| Loading animations | Radio group: `Random (default)` + each `LoadingPattern` case. "Replay selected animation" button (default action). "Blink now" button. | Posts internal event to tray icon controller. |
| Probe logs | Provider segmented control (Codex/Claude/Cursor/Augment/Amp/Ollama). Buttons: Fetch log, Copy, Save to file, (Claude only) "Load parse dump". "Rerun provider auto-detect". Read-only monospace text view 160–220 px. | "Fetch log" runs provider-specific debug probe; shows loading spinner. |
| Fetch strategy | Provider menu picker (all providers). Monospace text view listing recent `FetchAttempts` per strategy: `<strategyID> (cli|web|oauth|api|local) available\|unavailable error=…`. | Read-only. |
| OpenAI cookies (when keychain not disabled) | "Copy" button. Monospace text view of import debug log. | Shows latest OpenAI dashboard cookie import attempt log. |
| Caches | "Clear cost cache" button (disabled while a token refresh is in flight). "Clear cookie cache" button. Status text right of each. | Cookie cache on Windows = our managed sqlite cache; clearing wipes per-provider rows. |
| Notifications | Codex/Claude segmented picker; "Post depleted" / "Post restored" buttons. | Trigger session-quota toasts. |
| **CLI sessions** | "Keep CLI sessions alive between probes" switch (`debugKeepCLISessionsAlive`, default `false`). "Reset CLI sessions" button. | When ON, the CLI PTY/ConPTY session persists between probes (faster but consumes a slot). When OFF (default), session exits after each probe. Windows: ConPTY equivalent. |
| Error simulation (DEBUG build only) | Provider segmented picker; text area for simulated error; buttons Set/Clear menu error, Set/Clear cost error. | Compile out in release. |
| CLI paths | Read-only display: Codex binary, Claude binary, effective PATH, optional login-shell PATH. | Windows: replace "login shell PATH" with "PowerShell profile PATH" if detected. |

### What "Keep CLI sessions alive" actually does
- Default: CodexBar starts a Claude/Codex CLI per probe, sends `/usage`, parses, exits. Faster cold start, lower memory.
- ON: keeps the CLI/ConPTY child running between probes, so a probe is just "send `/usage`, parse" — but holds a session slot and a resident process.
- Windows: implemented via `windows::Win32::System::Console` ConPTY APIs; bound to the Tauri app lifetime.

### Log destinations (Windows)

| Mac path | Windows path |
|---|---|
| `~/Library/Logs/CodexBar/codexbar.log` | `%LOCALAPPDATA%\CodexBar\Logs\codexbar.log` |
| `~/Library/Caches/CodexBar/cost-usage/*.json` | `%LOCALAPPDATA%\CodexBar\Cache\cost-usage\*.json` |
| `~/.codexbar/config.json` | `%APPDATA%\CodexBar\config.json` (chmod 600 equivalent: ACL stripping inheritance, granting RW only to current user SID) |

### 8.1 Debug pane row-level UX rules

- All log buffers in the Debug pane are read-only, monospace, selectable, copyable. Min height 120 px, max 220 px.
- Buttons in the Debug pane never trigger destructive operations without an inline status string ("Cleared 3 providers", "Failed: <reason>").
- Provider segmented controls (Codex / Claude / etc.) auto-clamp to the providers actually compiled in this build; CodexBar mac uses `UsageProvider.allCases` for the menu-style picker and a curated subset for the segmented picker. Mirror that on Windows.
- The "Open log file" button on Windows must call `ShellExecute(open, log_path)` to default-launch Notepad / VS Code.
- "Reset CLI sessions" on Windows kills any persistent ConPTY child processes started for keep-alive mode, then resets their tracker state.

### 8.2 Debug pane non-goals

- Do **not** expose any developer-only setting that is gated by `#if DEBUG` outside debug builds.
- Do **not** include error-simulation rows in release; compile them out via `cfg(debug_assertions)`.

---

## 9. About pane

Centered vertical stack, padded 24/24/24.

| Element | Behavior |
|---|---|
| App icon | 92×92 with 16-px rounded corners. Hover: scale 1.05 + accent shadow. Click: opens project home `https://github.com/steipete/CodexBar` (Windows: `ShellExecute` via `tauri::shell::open`). |
| Title | "CodexBar" (16 px semibold). |
| Version | `Version 0.25.x (build)` — `CFBundleShortVersionString (CFBundleVersion)` analogue; pulled from `Cargo.toml` package version + Tauri build metadata. |
| Build timestamp | `Built Jan 12, 2026, 4:23 PM` if `CodexBuildTimestamp` is present; render in user locale. |
| Tagline | "Track AI coding usage in your tray." (or current `about_tagline`). |
| Link rows (centered, accent color, underline on hover) | GitHub, Website (https://codexbar.app), Twitter (@steipete), Email (peter@steipete.me). Each uses Segoe Fluent icons (`Code`, `World`, `Bird`, `Mail`). |
| Fork attribution | Add a row: "Windows port: <name> · <link>" — leave a placeholder string in localization. |
| Donation | Add a row "Buy me a coffee" with `Coffee` icon → `https://www.buymeacoffee.com/steipete` (or current). |
| Divider | — |
| Auto-update group (when updater available) | Checkbox: "Check for updates automatically" (`autoUpdateEnabled`, default `true`). Picker: "Update channel" (`UpdateChannel` — `stable`/`beta`); shows channel description text below. "Check for updates" button. |
| Auto-update group (when unavailable) | Show `unavailableReason` or fallback "Updates unavailable". |
| Changelog | "View changelog" link → in-app modal or external GitHub releases. |
| Copyright | Footnote at bottom: "© 2026 Peter Steinberger — MIT". |

### Update channel semantics

| Channel | Description |
|---|---|
| `stable` | Stable, production-ready releases only. |
| `beta` | Stable + beta previews. |
| Default | `stable`, unless build is a prerelease (`IS_PRERELEASE_BUILD` info-plist key, or version string contains `beta/alpha/rc/pre/dev`). |
| Switching channel | Triggers an immediate `checkForUpdates()` (Tauri updater). |

---

## 10. Settings persistence

### 10.1 Mac sources

| Store | What it holds |
|---|---|
| `UserDefaults` (standard + app-group) | UI preferences (refresh cadence, launch at login, language, debug flags, quota warning toggles/thresholds, multi-account layout, menu bar metric prefs, OpenAI web access flags, sparkle update channel, last-selected provider, overview selection). |
| `~/.codexbar/config.json` (`CodexBarConfigStore`) | Per-provider enable, source mode, cookie source, cookie header, API key, region, workspace ID, multi-account token data. Single source of truth shared with `codexbar` CLI. Written 0600. |
| Keychain | Legacy: provider tokens/cookies (now migrated to config.json on first launch). Runtime: cookie cache, OAuth caches that require Apple Keychain. |

### 10.2 Migration logic (`CodexBarConfigMigrator`)

On every launch, in order:
1. Load `config.json` (or seed default).
2. **Always**: `applyLegacyCookieSources` — read UserDefaults keys `codexCookieSource`, `claudeCookieSource`, `cursorCookieSource`, `opencodeCookieSource`, `factoryCookieSource`, `minimaxCookieSource`, `kimiCookieSource`, `augmentCookieSource`, `ampCookieSource` and fill `providers[].cookieSource` if absent. Also force `codex.cookieSource = off` if `openAIWebAccessEnabled == false`.
3. If `legacySecretsMigrationCompleted` flag is unset:
   - First-launch only (existing config absent): `applyLegacyOrderAndToggles` reads `providerOrder` (string array) and `providerToggles` (`{cliName: bool}`) and rewrites the config order/enable flags.
   - `migrateLegacySecrets`: pull tokens from per-provider Keychain stores (zai, synthetic, copilot, kimik2) and cookie headers (codex, claude, cursor, factory, augment, amp) into `apiKey`/`cookieHeader` if those fields are empty. Special: minimax (token + cookie + region from `minimaxAPIRegion`), kimi (token then `kimiManualCookieHeader` defaults fallback), opencode (cookie + `opencodeWorkspaceID`).
   - `migrateLegacyAccounts`: copy entries from `FileTokenAccountStore` JSON.
   - On success, set `legacySecretsMigrationCompleted = true` and delete the legacy file/keychain entries.

### 10.3 Windows mapping (target)

| Concept | Windows store |
|---|---|
| UI preferences (`UserDefaults`) | **Registry** `HKCU\Software\CodexBar\Defaults\<key>` (string/dword/binary), mirrored to a `defaults.json` for sync to portable installs. Use registry primarily so Group Policy + roaming profiles still work. |
| `config.json` | `%APPDATA%\CodexBar\config.json`. ACL: remove inherited ACEs, grant only `(NT AUTHORITY\SYSTEM, current user SID)` Modify + Read. |
| Per-secret encryption | DPAPI `CryptProtectData` per-field for any cookie/API key. Stored as base64 string with a `"$dpapi:"` prefix in the same JSON field. The Rust crate transparently decrypts on read. |
| OAuth refresh / device-flow credentials | **Windows Credential Manager** generic credentials, `Target = CodexBar:<provider>:oauth`. Survives roaming; safer than DPAPI files. |
| Cookie cache (browser-imported cookies) | `%LOCALAPPDATA%\CodexBar\Cache\cookies.db` (sqlcipher with DPAPI-wrapped key file). |
| App-group equivalent | Not needed — Windows only has one app, no widget extension yet. If widget shipped: per-user `%APPDATA%\CodexBar\shared\`. |

### 10.4 What each setting maps to (curated subset)

Full Defaults keys: `refreshFrequency`, `launchAtLogin`, `debugMenuEnabled`, `debugDisableKeychainAccess`, `debugFileLoggingEnabled`, `debugLogLevel`, `debugLoadingPattern`, `debugKeepCLISessionsAlive`, `statusChecksEnabled`, `sessionQuotaNotificationsEnabled`, `quotaWarningNotificationsEnabled`, `quotaWarningThresholds`, `quotaWarningSessionEnabled`, `quotaWarningWeeklyEnabled`, `quotaWarningSoundEnabled`, `quotaWarningMarkersVisible`, `usageBarsShowUsed`, `resetTimesShowAbsolute`, `menuBarShowsBrandIconWithPercent`, `menuBarDisplayMode`, `historicalTrackingEnabled`, `multiAccountMenuLayout`, `menuBarMetricPreferences`, `tokenCostUsageEnabled`, `hidePersonalInfo`, `randomBlinkEnabled`, `confettiOnWeeklyLimitResetsEnabled`, `menuBarShowsHighestUsage`, `claudeOAuthKeychainPromptMode` (n/a Win), `claudeOAuthKeychainReadStrategy` (n/a Win), `claudeWebExtrasEnabled`, `claudePeakHoursEnabled`, `showOptionalCreditsAndExtraUsage`, `openAIWebAccessEnabled`, `openAIWebBatterySaverEnabled`, `providerStorageFootprintsEnabled`, `jetbrainsIDEBasePath`, `mergeIcons`, `switcherShowsIcons`, `mergedMenuLastSelectedWasOverview`, `mergedOverviewSelectedProviders`, `selectedMenuProvider`, `providerDetectionCompleted`, `appLanguage`, `updateChannel`, `autoUpdateEnabled`.

---

## 11. Quota warning markers

### 11.1 Defaults & data model

| Key | Default | Notes |
|---|---|---|
| `quotaWarningNotificationsEnabled` | `false` | Master switch (General pane). |
| `quotaWarningThresholds` | `[50, 20]` (typical) | Sanitized via `QuotaWarningThresholds.sanitized()`. Up to 2 active thresholds; values clamped 1–99. |
| `quotaWarningSessionEnabled` | `true` | Toggle session-window warnings. |
| `quotaWarningWeeklyEnabled` | `true` | Toggle weekly-window warnings. |
| `quotaWarningSoundEnabled` | `true` | Play OS notification sound on threshold cross. |
| `quotaWarningMarkersVisible` | `true` | (Display pane) Master switch for *visual markers on bars* — added in PR #918. |
| Per-provider overrides | none | Override flag + thresholds + per-window enable stored under provider entry. |

### 11.2 Global threshold editor (General pane, expanded when notifications ON)

- Two text fields: **Upper** (placeholder `50`), **Lower** (placeholder `20`). Each accepts integer
  digits only, max length 2 chars, filtered live. Apply on blur, Enter, or "Apply" button.
- Session / Weekly checkboxes side-by-side. When both OFF, the threshold field grays out (55% opacity).
- "Play sound on warning" checkbox.

### 11.3 Per-provider overrides (in provider detail under "Quota warnings")

- Section h3 "Quota warnings" with caption "Customize thresholds for this provider, or inherit globals."
- One row per window (Session, Weekly):
  - "Customize <Session/Weekly> thresholds" checkbox.
  - When ON, shows nested:
    - "Enable <Session/Weekly> warnings" checkbox.
    - When that ON, shows Upper/Lower fields (same as global).
    - When OFF, shows "Warnings off" text.
  - When OFF, shows "Inherits globals (50%, 20%)" text or "Warnings off (inherited)".

### 11.4 Marker rendering

- Markers drawn as 2-px wide vertical ticks at threshold% positions on usage bars (in menu cards
  and Settings → Provider detail Usage block).
- When `usageBarsShowUsed == true`, markers are placed at `(100 - threshold)%` (since bar fills in
  the opposite direction).
- Hidden globally when `quotaWarningMarkersVisible == false` (the recently-added Display switch).
- Hidden per window when that window's warnings are disabled.

---

## 12. Token-account management UI

### 12.1 Data shape

```json
{
  "version": 1,
  "activeIndex": 0,
  "accounts": [
    { "id": "uuid", "label": "Work", "token": "secret", "addedAt": 1735123456, "lastUsed": 1735220000,
      "externalIdentifier": null, "organizationID": null }
  ]
}
```

### 12.2 Per-provider Token Accounts row (when provider supports it)

| Element | Behavior |
|---|---|
| Header | Title left, optional "Add Account" button right (Copilot only: runs device-flow OAuth instead of inline form). |
| Subtitle | Help text e.g. "Paste a `Cookie:` header from claude.ai." |
| Account list | Each row: radio dot (✓ filled when active) + label + token-suffix preview + Remove button. Click row sets `activeIndex` and refreshes the provider. |
| Inline add form (when no primary button) | Two fields: Label (text) + Token (secure) + "Add" button. Disabled until both non-empty (trimmed). |
| Footer links | "Open token file" (opens `config.json` in default editor), "Reload" (re-reads from disk; useful when CLI wrote to file). |
| Empty state | "No token accounts yet." |
| Visibility | Row hidden when not supported. Cookie-injection providers hide it unless either (a) provider's `requiresManualCookieSource` is false, or (b) user has added at least one account. |

### 12.3 Multi-account display rules

- `multiAccountMenuLayout = segmented` (default): tray menu shows account switcher bar above the
  card; up to ~6 chips before overflow.
- `multiAccountMenuLayout = stacked`: tray menu shows all accounts as stacked cards. Capped at **6**
  visible cards (`docs/ui.md`); rest reachable via overflow.

### 12.4 Per-provider account picker

- When `accounts.count > 1`, the provider's menu card shows a small switcher with the active
  account label. In settings, the "Accounts" section row toggles the active index.

### 12.5 Validation

- Label trimmed; empty → fallback `"Account <N+1>"`.
- Token trimmed; empty → reject.
- Removing the active account: pick the same index in the remaining list (clamp to count-1).
- Adding a Copilot account auto-clears the legacy `apiKey` field.

---

## 13. Keyboard pane (Windows split)

The only shortcut on macOS today is the global menu opener. Windows must expose it in its own pane
and add per-window shortcuts.

| Name | Default (mac) | Default (Windows) | Rebind | Notes |
|---|---|---|---|---|
| Open CodexBar menu (`openMenu`) | unset (user-bound via `KeyboardShortcuts.Recorder`) | **Win+Shift+U** | `KeyShortcutRecorder` component | Registered via `RegisterHotKey` (Win32) on app start. Conflicts shown inline ("Already in use by …"). Falls back to a backup combo if registration fails. |
| Refresh now | — | `Ctrl+R` (in-app only) | rebind | App-local accelerator. |
| Quick switch provider | — | `Ctrl+1..9` (in-app menu) | rebind | Maps to first 9 active providers. |
| Focus search in Providers pane | — | `Ctrl+F` (in-app) | not rebindable | Standard. |

### Rebind UX
- Click recorder → shows "Recording…" placeholder, captures next keystroke chord. Esc cancels.
- If chord already used by another shortcut, show inline red text "Already used by Refresh now".
- "Reset to default" mini-link beside each shortcut.
- "Disable" trash icon clears the binding.

---

## 14. Update channel selection

- Tauri's built-in updater drives this. Two appcasts: `appcast-stable.xml`, `appcast-beta.xml`.
- Selection persisted in `HKCU\Software\CodexBar\Defaults\updateChannel`.
- Change triggers an immediate `tauri::updater::check()`. If a higher version is found, surface the
  "Update available" inline card with [Download] [Later].
- Pre-release detection at build time (Cargo metadata or env var `CODEXBAR_PRERELEASE=1`) sets
  default channel to `beta`.

---

## 15. Localization

### 15.1 Strategy

- Strings live in `%APP_RESOURCES%\locales\<lang>\messages.json` (Tauri i18n) keyed by the same
  identifiers as macOS `.strings` files (e.g. `tab_general`, `refresh_cadence_title`, etc.).
- Loader: pick `appLanguage` key when set, else system locale (`GetUserPreferredUILanguages`),
  else fall back to `en`.
- React side: lightweight i18n provider (`react-i18next` or simple context). All UI strings
  consumed via `t("key")`.
- Reload on `appLanguage` change: re-mount the settings window (cheap; matches mac `id(self.settings.appLanguage)`).

### 15.2 Shipped locales (initial Windows release)

Mirror what mac ships today:

| Code | Label | Status |
|---|---|---|
| `` (system) | "System" | Default; resolves to user UI lang. |
| `en` | "English" | Primary. |
| `zh-Hans` | "中文 (简体)" | Imported from mac. |
| `pt-BR` | "Português (Brasil)" | Imported from mac (recent PR #902). |

Reserve keys for future: `de`, `es`, `ja`, `fr`. Add only when strings are translated end-to-end.

---

## 16. Validation, error, dirty/save states

### 16.1 Apply model: **live, not save-button-driven**

CodexBar mac applies all settings instantly (no Save button anywhere). The Windows port must do
the same to feel native to Windows 11 Settings, which is also live-apply. Exceptions:

| Setting | Apply timing |
|---|---|
| Quota warning thresholds | On blur / Enter / explicit "Apply" button. |
| Cookie header / API token | On blur (auto-save) + show a transient inline "Saved" check, then trigger one provider refresh. |
| Token account add | On click "Add" — form clears, refresh triggered. |
| All toggles / pickers | On change. |
| Language | On change — re-renders the whole window. |
| Provider order | On drop. |

### 16.2 Persistence pipeline

- UI mutation → Tauri command (`set_setting`) → Rust state update → debounced 350-ms write to disk
  (matches mac `schedulePersistConfig`). Tests can force-flush.
- Failures: surface a non-blocking toast "Couldn't save settings (disk full). Retrying…" and
  retry every 30 s. Never lose state in memory.

### 16.3 Confirmation dialogs

| Action | Dialog |
|---|---|
| Remove Codex managed account | Title "Remove Codex Account", body "Remove `<email>`?", primary "Remove" (destructive accent), "Cancel". |
| Quit CodexBar | Inline popover from Quit button (no modal) — primary "Quit", secondary "Cancel". |
| Clear cost cache | Inline — no confirm; show "Cleared" status text after. |
| Clear cookie cache | Inline — no confirm. |
| Disable secret access (Advanced) | Inline toast warning "Cookies will be cleared. Some providers may stop fetching usage until you paste cookies manually." |
| Switch update channel from beta → stable | No confirm; immediate channel switch + update check. |
| Change language | No confirm; immediate. |

### 16.4 Validation errors

- Inline below the offending field, red text + small alert icon, no toast.
- Field border turns `--border-danger` (`#E5484D`) for 600 ms, then fades.
- Tab-out with invalid text reverts to last good value and shows inline hint for 1.5 s.

### 16.5 Dirty state

- There is no global dirty marker — every change autosaves. Provider detail headers may briefly
  show a "Saving…" pill in the top right that flips to a check (200 ms) then fades after 1.5 s.

---

## 17. Acceptance checklist

A Windows port passes only when **all** of the following hold:

- [ ] Window opens at 880×640, applies Mica (Win11) or acrylic (Win10), with custom 32-px title strip and working snap/min/max/close.
- [ ] Sidebar shows General, Providers, Display, Keyboard, Advanced, About — and Debug only when debug menu is enabled.
- [ ] Switching panes does not animate window size; content cross-fades 200 ms.
- [ ] General pane: language picker live-reapplies; "Launch at sign-in" toggle persists across reboot via `Run` registry key with `--hidden`; refresh cadence picker offers Manual/1/2/5/15/30 minutes; quota warning notifications switch expands the global threshold sub-panel.
- [ ] Providers pane: sidebar shows reorderable list with brand icons, status dots (when status checks ON), refresh spinners, two-line subtitles, and per-row enable toggle. Drag-reorder writes to `config.json` `providers[]` array immediately. Search filters list.
- [ ] Selecting a provider renders detail with sticky header, info grid, usage block (with markers when enabled), error card (collapsible, copyable), settings rows, options rows, and the Codex Accounts subsection for Codex.
- [ ] Per-provider settings catalog (§5) implemented: every key/type/default matches the table; hidden-when conditions enforced.
- [ ] Display pane: merge-icons toggle disables/enables dependent rows; Overview tab popover limits to 3 providers and remembers selection signature; reset-time toggle flips countdown ↔ clock; multi-account layout picker switches stacked ↔ segmented.
- [ ] Quota warning **markers** Display switch hides/shows tick marks globally (PR #918 parity).
- [ ] Per-provider quota warnings inherit globals unless customized; thresholds clamp to 1–99 and accept up to 2 active values; "Apply" commits the field; Session/Weekly toggles work independently.
- [ ] Advanced pane: Show debug settings reveals Debug tab; Disable secret access warns and stops DPAPI use; Show provider storage usage starts background scans; Hide personal info masks emails in tray UI; CLI install button copies/symlinks `codexbar.exe` to a writable PATH dir and shows status text.
- [ ] Keyboard pane: Open menu shortcut defaults to **Win+Shift+U**, rebindable, conflicts detected, fallback if `RegisterHotKey` fails.
- [ ] Debug pane: every section renders read-only PATH info, log levels persist, "Keep CLI sessions alive" toggles ConPTY persistence, fetch-attempt list shows per-strategy availability + last error, error-simulation section gated to debug builds.
- [ ] About pane: version + build timestamp + GitHub/Website/Twitter/Email links + auto-update checkbox + channel picker (Stable/Beta) with description + manual "Check for updates" + BuyMeACoffee row + fork attribution row.
- [ ] Token accounts UI: add/edit/remove, active radio, "Open token file" opens `%APPDATA%\CodexBar\config.json`, "Reload" picks up external edits. Copilot row uses device-flow button.
- [ ] Multi-account display: max 6 stacked cards or segmented switcher, per `multiAccountMenuLayout`.
- [ ] Codex Accounts: managed + system accounts, Add → OAuth flow, Re-auth/Remove buttons with proper disable states, removal confirmed via dialog, unreadable-store warning persisted.
- [ ] Migration: `applyLegacyCookieSources` runs every launch; secrets migration runs once and writes `legacySecretsMigrationCompleted`; legacy stores cleared on success.
- [ ] Persistence: config.json is `0600`-equivalent on Windows (ACL stripped, only current user RW); secrets wrapped with DPAPI; UI prefs in registry + JSON mirror.
- [ ] Update channel: switch from stable→beta triggers immediate updater check; pre-release builds default to beta.
- [ ] Localization: en + zh-Hans + pt-BR ship; system mode follows `GetUserPreferredUILanguages`; appLanguage change re-mounts window without crash.
- [ ] All toggles autosave with a 350-ms debounce; failures show a non-blocking toast and retry; cookie/API fields show "Saved" check on blur.
- [ ] Confirmation dialogs for destructive actions only (account removal); everything else is live + reversible.
- [ ] Inline validation: integer-only quota thresholds (max 2 digits), cookie header strip-leading-`Cookie:`, URL field rejects non-http schemes.
- [ ] Accessibility: 4.5:1 contrast, focus rings visible on every interactive element, screen-reader labels on icon-only buttons (Refresh, Copy, Remove, Add Account), keyboard navigation through every control with no traps.
- [ ] Polish: spring-eased toggles, hover surfaces, 1.05 hover on About icon, 200 ms cross-fades between panes, confetti opt-in on weekly reset works.

---

## 18. Provider settings inconsistencies (collected — for the implementer to normalize)

1. **StepFun field repurposing**: `username → apiKey`, `password → cookieHeader`, `token → region`. Migrate to a dedicated `{username, password, token}` shape in the new config schema; provide a one-shot legacy reader.
2. **Claude `tokenAccounts` polymorphism**: accepts cookie strings *or* OAuth bearer tokens (`sk-ant-oat…`); decision happens in `ClaudeCredentialRouting`. Document this in the schema; consider a per-account `kind: "cookie" | "oauth"` discriminator.
3. **Codex special-case `openAIWebAccessEnabled`** forces `cookieSource = off`. Two writers for the same logical state. Replace with one canonical `cookieSource`.
4. **Claude `webExtrasEnabled`** lives in defaults, not provider config — yet is force-cleared when `claudeUsageDataSource != .cli`. Co-locate with Claude provider settings.
5. **Region label inconsistency**: zai/minimax use `global/china`; alibaba/moonshot use `international/china`. Pick one label set and translate consistently.
6. **Mac-only Claude Keychain rows** (`oauthKeychainPromptMode`, `oauthKeychainReadStrategy`) need explicit `visibleOnPlatform: ["macos"]` markers so the Windows renderer never shows them.
7. **JetBrains `ideBasePath`** is plain text on mac; on Windows must be a Browse… picker (folder dialog) that defaults to `%LOCALAPPDATA%\JetBrains`.
8. **Cookie + workspace combo providers** (OpenCode, OpenCodeGo, OpenAI) duplicate ~90% of their settings code; extract a shared `WorkspaceCookieSettings` descriptor.
9. **Copilot `apiKey` auto-clear** when token accounts are added/updated is an implicit side effect — make it explicit in the schema (mutually exclusive) and surface a one-time notice "Legacy API token replaced by account-based login."
10. **Alibaba auto-enable** uses a one-shot UserDefaults flag (`alibabaCodingPlanAutoEnableApplied`); on Windows mirror with `HKCU\Software\CodexBar\Flags\alibabaAutoEnableApplied` and document the trigger conditions (env var or non-empty token).
11. **Quota warning marker switch label mismatch**: settings key is `quotaWarningMarkersVisible` (positive form, default true), but PR #918 is titled "Add option to **hide** quota warning markers". UI label should be "Show quota warning markers" (positive) to match the key — the hint subtitle can clarify "Turn off to hide tick marks…".
12. **`menuBarMetricPreferences`** is keyed by `UsageProvider.rawValue` but the legacy single-pref key (`menuBarMetricPreference`) migrates the same value across *all* providers — record the migration path in the Rust crate's config loader.

---

End of spec.
