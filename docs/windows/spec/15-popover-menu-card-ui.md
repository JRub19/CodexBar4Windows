---
title: "15 — Popover / Menu / Card UI"
subsystem: "popover-menu-card"
target: "Tauri 2 + React + shared Rust crate"
polish_bar: "Phantom-wallet / Duolingo"
reads_from:
  - "docs/windows/05-windows-ux-spec.md (base palette + Mica chrome)"
  - "Sources/CodexBar/MenuCardView*.swift (canonical visuals)"
  - "Sources/CodexBar/StatusItemController+*.swift (containers, switcher, smart-update)"
  - "Sources/CodexBar/*ChartMenuView.swift (charts)"
written_for: "A Rust/TS engineer with no Swift exposure. Re-implement, don't transpile."
---

# 15 — Popover / Menu / Card UI

This is the spec for the popup that appears when the user clicks the tray icon. The
Mac is canonical for *behavior, density, microinteractions*. The Windows
implementation must feel **native to Windows 11** (Mica, Segoe UI Variable, system
accent) while preserving every information cue the Mac shows.

Every magic number in this document is real — they are extracted directly from
the AppKit/SwiftUI code paths in `Sources/CodexBar/*.swift`. When in doubt, copy
the number. Do not "round to a nice value" or invent your own.

---

## 0. Reading guide

- "**Mac:**" lines describe the source-of-truth measurement we are matching.
- "**Win:**" lines are the Windows port. If a Win line is missing, the Mac value
  is used verbatim with logical-pixel semantics.
- All sizes are **CSS logical pixels** in the WebView. The browser DPR handles
  the Hi-DPI math; do not hand-multiply.
- Colors are written hex for Windows; the Mac variants in parentheses are the
  `NSColor.semanticName` they came from so you can audit the mapping.
- "Card" means the rich provider panel. "Row" means a single hosted SwiftUI
  view inside a Mac `NSMenuItem`. On Windows there is no `NSMenuItem` — the
  popup is a single Tauri WebView, and "row" maps to a focusable React div.

---

## 1. Container chrome (the popup itself)

### 1.1 Window

| Property | Mac (NSMenu hosting SwiftUI) | Win (Tauri WebView) |
|---|---|---|
| Anchor | Tray status item rect | Tray icon rect via `Shell_NotifyIcon` → `NIN_SELECT` |
| Default width | 310 px (`menuCardBaseWidth`) — auto-expands if the longest action label exceeds it | 360 px (Win baseline from §05); content may grow to 420 px |
| Default height | content-sized, no max | content-sized, max = 85 vh, then internal `overflow: auto` |
| Corner radius | NSMenu native (~6 px) | **12 px** (per §05 — panel, not menu) |
| Backdrop | Vibrancy / `NSVisualEffectView` via `allowsVibrancy = true` on host views | **Mica (Win 11)** → **Acrylic (Win 10)** → flat surface fallback |
| Shadow | NSMenu native | `0 12px 32px rgba(0,0,0,0.28)` dark / `0 8px 24px rgba(0,0,0,0.14)` light |
| Border | None | 1 px `rgba(255,255,255,0.08)` dark, `rgba(0,0,0,0.08)` light |
| Focus ring | OS | Internal — accent-tinted 2 px outline, 4 px offset |
| Animation in | NSMenu fade (~80 ms) | **140 ms** ease-out, opacity 0→1 + translateY(4px → 0) |
| Animation out | NSMenu fade | **90 ms** ease-in, opacity 1→0 only |
| Dismiss triggers | click outside / Esc / re-click tray | identical + WM_KILLFOCUS |

### 1.2 Width algorithm (load-bearing)

Mac measures every actionable item with a hidden `NSMenu`, picks the larger of
`310` or `ceil(measuringMenu.size.width)`, then applies that to every hosted
SwiftUI row so they all align (`menuCardWidth(for:sections:)`).

**Win port:**
1. Render the popup off-screen at width `auto`, `max-width: 420px`.
2. Measure the widest line of action text + 14 px leading icon + 18 px trailing.
3. Clamp to `[360, 420]`.
4. Apply that width to every card row via CSS variable `--popup-width`.
5. The provider switcher (header tabs) uses the same width.

### 1.3 Stacking inside the popup (top → bottom)

```
[ProviderSwitcherView]         ← only if Merge Icons + ≥2 providers
[separator]
[CodexAccountSwitcherView]     ← only if Codex multi-account
[separator]
[TokenAccountSwitcherView]     ← only if provider has token-account stacking
[separator]
[Overview rows]   OR   [Provider card (one per active provider)]
[separator]
[Storage card]                 ← if storageText for provider
[separator]
[Submenu rows: Usage breakdown, Credits history, Cost history,
                Plan utilization, Buy credits]
[separator]
[Action rows: Refresh now (persistent), Settings…, About, Quit]
```

`NSMenu` uses real separators between every section. The Windows port uses a
**1 px** `rgba(W,W,W,0.08)` divider with **10 px** vertical padding above/below.

---

## 2. Layout grid

Everything inside a card sits on a 16-px horizontal gutter and a 6-px vertical
rhythm. There are exactly two "first-class" paddings: section margin and row
gap.

| Token | Value | Where it's used |
|---|---|---|
| `--gutter-h` | **16 px** | Left/right of every card body (`padding(.horizontal, 16)` everywhere) |
| `--card-top` | **2 px** | `padding(.top, 2)` on `UsageMenuCardView` |
| `--card-bottom` | **2 px** | `padding(.bottom, 2)` — increases to **6 px** when credits row absent |
| `--gap-tight` | **3 px** | Provider name → email/subtitle |
| `--gap-row` | **6 px** | Metric title → bar → percent line |
| `--gap-section` | **10 px** | Between metric rows and credits block |
| `--gap-block` | **12 px** | Between metric stack and providerCost stack |
| `--gap-list` | **4 px** | Between usage-note lines |
| Row min-height | **28 px** | Persistent action row (`PersistentMenuActionItemView`) |
| Switcher row | **30 px** (inline) / **36 px** (stacked) / **40 px** (3+ rows stacked) | `rowHeight` in `ProviderSwitcherView` |
| Switcher row spacing | **2 px** (inline) / **4 px** (stacked) | `rowSpacing` |
| Submenu indicator (chevron) | trailing, top-padding 0–4 px | `MenuCardSectionContainerView` |

Alignment columns:
- Card title (left edge): **16 px** from popup left.
- Card right text (email, reset countdown): **16 px** from popup right, right-aligned.
- The "Refresh now" persistent row uses **6 px** outer padding for its highlight
  plate and **12 px** inner padding (`PersistentMenuActionItemView`).
  - Icon column: 18 × 18 px.
  - Stack spacing icon→title: **8 px**.

---

## 3. Header row

Order, left-to-right, inside the card header (`UsageMenuCardHeaderView`):

```
[ProviderName headline | semibold | 1 line | truncate tail]   …  [email | subheadline | secondary | truncate middle]
[subtitleText | footnote | secondary]                         …  [plan pill | footnote | secondary]
```

| Element | Mac size | Win mapping |
|---|---|---|
| `providerName` | `.headline` (≈ 13 pt) + `.semibold` | **14 px / 600**, Segoe UI Variable Display Semibold on Win11 |
| `email` | `.subheadline` (≈ 11 pt) | **12 px / 400**, secondary color |
| `subtitleText` | `.footnote` (≈ 10 pt) | **11 px / 400** |
| `planText` (e.g. "Pro") | `.footnote` | **11 px / 400**, secondary |
| Brand icon | (not in header — appears in tray + switcher only) | If used in header, **16 × 16 px**, `mask: <provider>.svg`, fills `currentColor` (template) |
| Settings cog | (not in card header — settings lives in the Actions section) | Optional 16 × 16 cog top-right of popup, accent on hover |

Subtitle states:
- `.info` → `"Updated just now"` / `"Updated 12m ago"` / `"Updated 3h ago"` /
  `"Updated 14:42"` — generated by `UsageFormatter.updatedString`. Color: secondary.
- `.loading` → `"Refreshing..."`. Color: secondary.
- `.error` → first line of the error trimmed; multi-line allowed, **lineLimit: 4**,
  fixed-size vertical. Color: `systemRed` (Win: `#FF453A` dark, `#D70015` light).
  When in error state, a **copy icon button** (`doc.on.doc`, swaps to `checkmark`
  on tap) appears to the right of the subtitle:
  - 18 × 18 hit target, 4 px corner radius
  - pressed: scale 0.94, background opacity 0.18 of secondary
  - copy animation: opacity 1 of checkmark for **900 ms**, then fade back over **200 ms**
  - SwiftUI source: `CopyIconButtonStyle`, `CopyIconButton` in `MenuCardView.swift`

Email **redaction** (Preferences toggle `hidePersonalInfo`):
- Replace the whole email with literal `"Hidden"` (capital H).
- Inside multi-line text (e.g. error messages), regex
  `[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}` → `"Hidden"`, case-insensitive.
- Source: `PersonalInfoRedactor.swift`.

---

## 4. Provider card anatomy

A card has up to five vertical sections, separated by 1 px dividers:

1. **Header** — name, email, subtitle, plan.
2. **Usage metrics** — 1 to 4 progress bars: primary / secondary / tertiary /
   extras / code-review. Order is fixed per provider (see §4.4).
3. **Usage notes** — small grey lines (e.g. `"No limit set for the API key"`,
   `"Off-peak hours · Claude is consuming less"`).
4. **Credits bar** — for providers with `metadata.supportsCredits`.
5. **Provider cost / Extra usage** — `cost.limit > 0` (e.g. Codex monthly,
   Factory extra spend).
6. **Token cost** — for Codex / Claude / VertexAI when `tokenCostUsageEnabled`.

### 4.1 Metric row (one of 1..N)

```
[Title | body | medium]
[UsageProgressBar — height 6 px, full width, pill ends]
[percent label · LEFT]                       [resetText · RIGHT secondary]
[detailLeftText (primary) · LEFT]            [detailRightText · RIGHT secondary]
[detailText · LEFT secondary]
```

Source: `MetricRow` in `MenuCardView.swift`.

| Slot | Font | Color |
|---|---|---|
| Title | body (13 px) / medium | primary |
| Status text (if no bar) | footnote (11 px) | secondary |
| Percent label (`"42% left"` or `"58% used"`) | footnote | primary |
| Reset text (`"Resets in 3h 12m"`) | footnote | secondary |
| Detail left | footnote | primary |
| Detail right | footnote | secondary |
| Detail (full row, e.g. Z.ai limit detail) | footnote | secondary, lineLimit 1 |

### 4.2 Card hover/press/highlight states

Cards are **focusable rows** in the Mac NSMenu — they highlight on arrow-key
navigation and mouse hover. Windows replication:

| State | Mac | Win port |
|---|---|---|
| Idle | no background | transparent |
| Hover (mouse) | drives `menuItemHighlighted = true` via `MenuCardItemHostingView` | background `rgba(255,255,255,0.06)` dark / `rgba(0,0,0,0.05)` light; **80 ms** ease-out fade |
| Highlighted (keyboard / sub-row focus) | `selectedContentBackgroundColor` rounded rect | accent-tinted background `var(--accent-12)` with **6 px** corner radius, inset `6 px` horizontal / `2 px` vertical (matches `MenuCardSectionContainerView`) |
| Pressed | brief darken | scale 0.985 + background `var(--accent-18)`; revert in **120 ms** |
| Focus ring | system | 2 px outer outline using `var(--accent)`, 0 px offset on rounded rect |

When highlighted, all text inside the card flips to `selectedMenuItemTextColor`
(white on Mac, **#ffffff** Win). Source: `MenuHighlightStyle.swift`.

### 4.3 Click targets

- Single click on a card row: opens the provider's dashboard URL (same as Mac
  `selectOverviewProvider`).
- Right-click (or long-press): per-card quick menu — see §9.
- Click on the progress bar specifically: same as card click. No tooltip on
  hover of the bar itself (Mac doesn't have one; Win can add a tooltip with
  `"Resets at 14:42 local · in 3h 12m"` — explicitly approved in §05).

### 4.4 Per-provider metric ordering

`MenuCardView.Model.metrics(input:)` builds the list. Rules:

| Provider | Order |
|---|---|
| Codex | `primary` (session), `secondary` (weekly), optional `code-review` |
| Claude / generic | `primary`, `secondary`, optional `tertiary` (Opus / Sonnet) |
| Factory (when tertiary present) | `5-hour`, `Weekly`, `Monthly` (labels overridden) |
| Kilo | reorder so `secondary` precedes `primary` |
| MiniMax | service-by-service list, no fixed primary/secondary |
| Antigravity | always three (primary, secondary, tertiary), even if data missing → placeholder bar |
| Extras | appended after tertiary, one per `extraRateWindows` entry |

### 4.5 Status pill (incident)

When a provider has an active status indicator:
- Show as a one-line `text(.secondary)` entry in the actions section, **not** inside the card body.
- Format: `"⚠️ degraded — Updated 5m ago"` (Mac uses status emoji from indicator).
- Win port: pill chip below header
  - 11 px / 500 weight, 6 px horizontal / 3 px vertical padding, 9999 px corner radius
  - Color: `rgba(yellow.30%) on yellow.15%` for degraded; red variants for outage; cyan for maintenance
  - Click → open `statusPageURL`

---

## 5. UsageProgressBar (the bar)

Source: `Sources/CodexBar/UsageProgressBar.swift`. Drawn with a single Canvas
(Core Graphics) — explicitly **not** SwiftUI compositing — to dodge a macOS 26
Metal-shader bug. Replicate exactly the geometry on Windows via SVG or `<canvas>`.

### 5.1 Geometry

| Property | Value |
|---|---|
| Height | **6 px** (`.frame(height: 6)`) |
| Width | 100 % of parent (popup width − 32 px gutter) |
| Corner radius | `height / 2` = **3 px** (pill) |
| Track fill | `tertiaryLabelColor.opacity(0.22)` → Win: `rgba(255,255,255,0.18)` dark / `rgba(0,0,0,0.14)` light |
| Track fill (highlighted card) | `selectedContentBackgroundColor.opacity(0.22)` → Win: `accent.opacity(0.22)` |
| Bar fill (normal) | **provider brand color** (see §5.4); solid, no gradient |
| Bar fill (highlighted card) | `selectedMenuItemTextColor` (white) → Win: `#ffffff` |
| Fill animation | **none** — bars snap to the new percent on data update. Mac comment: "Static progress fill with no implicit animations" |

**Win polish opportunity:** apply a one-time `200 ms` ease-out tween when the
bar's percent changes by more than 1.5 percentage points. Use
`transform: scaleX(...)` on the inner fill, GPU-only, never reflow. Skip the
tween for the *first* paint after the popup opens — initial values are not "a
change."

### 5.2 Pace tip ("punch-out arrow")

When `pacePercent` is set (Claude / Codex weekly pace), the bar gets a small
chevron-shaped punch in its fill that points at the "expected used by now"
position. Geometry:

- Three vertical stripes, **2 px** wide each → **6 px** total span.
- Tip width = `max(25, height * 6.5)` = **25 px**.
- Center stripe color: **green** when ahead-of-schedule, **red** when in deficit, **white** when card highlighted.
- Drawn via `.destinationOut` blend so the stripes "cut" through the fill — pixel-aligned to display scale.
- `paceOnTop = true` means the user has a *reserve* (left of expected); `false` = deficit.

**Win port:** SVG mask. Layer order: track → fill → tip (cut). One `<svg>` per
bar; do not nest filters.

### 5.3 Quota warning markers

Source: `MenuCardQuotaWarningMarkers.swift`, `QuotaWarningThresholds`.

- Default thresholds: `[50, 20]` — vertical tick marks on the bar at 50 % and 20 %
  remaining (or at 50 % / 80 % used, if `usageBarsShowUsed = true`).
- Marker width: `max(1/displayScale, 2 px)` → use **2 px** on Win.
- Color: **white** when highlighted, otherwise `primary.opacity(0.72)` →
  Win: `rgba(230,230,230,0.72)` dark / `rgba(31,31,31,0.72)` light.
- Filter rule: markers at 0 % or 100 % are hidden.
- Hide entirely when Preferences → Display → "Hide quota warning markers" is on
  (issue #918 / commit `009420a7`).
- Allowed range: 0..99; user can configure two thresholds upper/lower; window
  config supports per-window overrides (`session`, `weekly`).

### 5.4 Bar colors per provider

Each provider has a brand color in `ProviderDescriptorRegistry.descriptor(for:).branding.color`.
Map to Windows by storing the RGB tuple in a JSON manifest at build time. When
the card row is highlighted, the fill is **always** white regardless of
provider — do not tint.

---

## 6. Pace text — exact strings

Source: `UsagePaceText.swift`.

### 6.1 When pace shows

- Only computed for Claude and Codex (`sessionPace` gates on
  `provider == .codex || provider == .claude`).
- Hides when `expectedUsedPercent < 3` — the 3-percent-elapsed rule. Below that,
  the data is too noisy to make a useful claim.
- Hides when window has `remainingPercent <= 0`.

### 6.2 Left label (relative to expected pace)

| Stage | Text |
|---|---|
| `.onTrack` | `"On pace"` |
| `.slightlyAhead`, `.ahead`, `.farAhead` | `"{N}% in deficit"` |
| `.slightlyBehind`, `.behind`, `.farBehind` | `"{N}% in reserve"` |

`{N}` is `Int(abs(deltaPercent).rounded())`.

### 6.3 Right label (ETA + risk)

| Condition | Text |
|---|---|
| `willLastToReset` true | `"Lasts until reset"` |
| `etaSeconds` set, countdown == "now" | `"Runs out now"` |
| `etaSeconds` set, countdown otherwise (`"in 3h 12m"`) | `"Runs out in 3h 12m"` |
| `runOutProbability` set | append ` · ≈ {R}% run-out risk` where `R = round(probability*100 / 5) * 5` |
| neither ETA nor risk | label hidden |

### 6.4 Composed summary line (NSMenu fallback rendering)

```
Pace: {leftLabel} · {rightLabel}
Pace: {leftLabel}                            ← when no right label
```

In the **rich card**, left/right go into the two separate detail slots
(`detailLeftText` / `detailRightText`), not glued.

### 6.5 Win port

- Left label: **11 px / 600** primary.
- Right label: **11 px / 400** secondary.
- Color of "deficit" copy: optional accent-red tint (`#FF453A`) on the *first*
  word ("X% in deficit") for the first 800 ms after data update; then revert to
  primary. (Polish, not required — but cheap and recognized as Duolingo-style.)

---

## 7. Reset countdown vs absolute clock

Source: `UsageFormatter.swift` (`resetLine`, `resetCountdownDescription`,
`resetDescription`). User toggles via Preferences → Display → "Reset times" →
`countdown` or `absolute`.

### 7.1 Countdown style (default)

Always prefixed with `"Resets "`. Computed from `resetsAt`:

```
seconds < 1                          → "Resets now"
seconds ≥ 1d                         → "Resets in {d}d {h}h"   (no minutes shown)
                                       "Resets in {d}d"        (when hours = 0)
seconds ≥ 1h                         → "Resets in {h}h {m}m"
                                       "Resets in {h}h"        (when minutes = 0)
seconds < 1h                         → "Resets in {m}m"        (rounded up)
```

Total minutes is `max(1, ceil(seconds / 60))`. There is no "{s}s" tail —
sub-minute is bucketed to "1m".

### 7.2 Absolute style

```
same day                             → "Resets HH:MM"             (en_US: 2:42 PM; locale-aware)
tomorrow                             → "Resets tomorrow, HH:MM"
otherwise                            → "Resets MMM d, HH:MM"      (e.g. "Mar 14, 2:42 PM")
```

### 7.3 Fallback from `resetDescription`

If `resetsAt` is missing but a provider supplies a string `resetDescription`
(e.g. JetBrains: `"Resets in 4h 13m"`), pass through trimmed:
- If string already starts with `"resets"` (case-insensitive), use verbatim.
- Otherwise prefix with `"Resets "`.

### 7.4 "Updated" line

Source: `UsageFormatter.updatedString`. Visible in the card subtitle:
- `< 60 s` → `"Updated just now"`
- `< 24 h` → `"Updated {rel}"` where `{rel}` is `RelativeDateTimeFormatter.localizedString` in `.abbreviated` style. On Win port emit the English abbreviated forms: `"Updated 3m ago"`, `"Updated 1h ago"`.
- `≥ 24 h` → `"Updated HH:MM"`.

### 7.5 Win locale handling

- Time formatting uses the user's Windows region/locale (`Intl.DateTimeFormat`).
- Day abbreviations (`MMM d`) also locale-aware.
- The phrase `"Resets in 3h 12m"` is **deliberately English-only** in upstream
  for parity — keep it English-only in the Win port until i18n strings ship.
  Source strings: `UsageFormatter.resetCountdownDescription`.

---

## 8. Switcher tabs (top of popup)

Source: `ProviderSwitcherView` in `StatusItemController+SwitcherViews.swift`.
Appears only when **Merge Icons** mode is on and ≥ 2 providers are active.

### 8.1 Tab order

```
[Overview] [Provider1] [Provider2] [Provider3] …
```

- Overview tab leads if `includesOverview` is true (Merge mode + > 1 provider).
- Provider order follows `enabledProvidersForDisplay()` — same as the list in
  Preferences → Providers.
- Last selection is sticky (`mergedMenuLastSelectedWasOverview`,
  `selectedMenuProvider`).

### 8.2 Geometry

| Property | Value | Notes |
|---|---|---|
| Overall row height | 30 px (inline) / 36 px (stacked icons) / 40 px (3+ rows stacked) | `rowHeight` |
| Row spacing | 2 px (inline) / 4 px (stacked) | `rowSpacing` |
| Outer padding | 16 → 10 → 6 px (clamps down to keep button ≥ 50–54 px) | `switcherOuterPadding` |
| Minimum gap between tabs | 1 px | `minimumGap` |
| Tab corner radius | 6 px | `layer?.cornerRadius = 6` |
| Inline tab content padding | top 4, leading 7, bottom 4 (+4 indicator space), trailing 7 | `InlineIconToggleButton.contentPadding` |
| Stacked tab content padding | top 2, leading 4, bottom 2 (+4 indicator space), trailing 4 | `StackedToggleButton.contentPadding` |
| Tab icon size | 16 × 16 px, template (recolored) | `iconView.imageScaling = .scaleNone` |
| Inline tab font | smallSystemFontSize (~11 pt) | `setTitleFontSize` |
| Stacked tab font | smallSystemFontSize − 2 (~9 pt), or − 3 when 4 rows | `StackedToggleButton.configure` |
| Stacking trigger | `showsIcons && segments.count > 3` | `stackedIcons` |
| Max rows | min(4, segments.count); auto-multi-row when single row can't fit ≥ 50 px tabs | `switcherRowCount` |
| 4-row threshold | `count >= 15` always uses 4 rows | `fourRowThreshold` |

### 8.3 Tab state

| State | Background | Text/icon |
|---|---|---|
| Idle | clear | `secondaryLabelColor` (Win: `#a0a0a0` dark / `#5d5d5d` light) |
| Hover | `labelColor.opacity(0.06)` dark / `black.opacity(0.095)` light — `hoverPlateColor` | secondary |
| Selected (`.on`) | `controlAccentColor` (Win: `var(--accent)`) | **white** (`#ffffff`) |
| Pressed | (same as hover; mouseDown→mouseUp guarded by hit-testing) | secondary |

Hover comes from `NSTrackingArea(.activeAlways, .mouseMoved, .mouseEnteredAndExited)` and is invalidated when the menu closes.

### 8.4 Per-tab "weekly remaining" pill (under each tab)

A thin colored line under each provider tab shows that provider's weekly
remaining percent — like a mini-bar.

| Property | Value |
|---|---|
| Track height | 4 px |
| Track corner radius | 2 px |
| Inset from tab left/right | 6 px |
| Inset from tab bottom | 1 px |
| Track color | `tertiaryLabelColor.opacity(0.22)` (Win: same as bar track) |
| Fill color | provider brand color |
| Width | `ratio = remainingPercent / 100`, clamped 0..1 |
| Hidden on selected tab | yes (`updateWeeklyIndicatorVisibility`) |
| Hidden when no data | yes |

### 8.5 Light-mode wash

In light mode only, the entire switcher area gets a `black.opacity(0.035)` background overlay layer to "ground it" against the bright Mica surface. On Win, replicate with a CSS `background: rgba(0,0,0,0.035)` on the switcher container only in light theme.

### 8.6 Tab interaction

- **Click**: switches the active card stack; `applyIcon(phase: nil)` triggers a tray icon redraw.
- **Smart update**: when the user clicks a different tab and the new layout is compatible (same provider list, same width, same usage-bar mode), the Mac code keeps the switcher view intact and only rebuilds rows below it. Source: `updateMenuContentPreservingSwitcher`. **Win port:** keyed React reconciler — the `<ProviderSwitcher>` component receives a stable key; below it, the card stack is rebuilt with a `100 ms` cross-fade.
- **Keyboard**: arrow-left / arrow-right; Enter activates the focused tab.
- **Animation between tabs**: Mac is instant (CATransaction disables actions). **Win port:** **120 ms** ease-out opacity cross-fade between the two card stacks. Heights animate over the same window using `height: auto` → measured value.

### 8.7 Token-account & Codex-account switchers (below provider switcher)

Both reuse the same visual style as the provider switcher:

- `TokenAccountSwitcherView`: rowHeight 26 px, rowSpacing 4 px, max 6 accounts, 2-row layout when count > 3.
- `CodexAccountSwitcherView`: same geometry; titles are `email|workspace` with smart middle-truncation; tooltip = full menu display name.

Selected style: `controlAccentColor` background, white text — identical to provider tabs but **without** the under-tab weekly pill.

---

## 9. Menu actions (footer)

Order (when not in Overview):

```
[Switch Account… / Add Account…]    (provider-aware; subtitled when missing OAuth perms)
[Usage Dashboard]                    (if provider has dashboardURL)
[Status Page]                        (if provider has statusPageURL / statusLinkURL)
[Status line · secondary]            (incident text + freshness)
---
[Refresh]                            ← persistent custom view (always visible)
[Settings…]
[About CodexBar]                     ← Win port: rename to "About CodexBar4Windows"
[Quit]
```

### 9.1 Persistent Refresh row

Source: `PersistentMenuActionItemView`. Stays visible even while a refresh
is in flight; the rest of the menu still rebuilds.

| Property | Value |
|---|---|
| Row height | 28 px |
| Inner stack horizontal padding | 12 px |
| Icon column width | 18 px |
| Icon size | 16 × 16 |
| Icon → title spacing | 8 px |
| Title font | `NSFont.menuFont(systemFontSize)` → Win 13 px / 400 |
| Shortcut font | `NSFont.menuFont(smallSystemFontSize)` → Win 11 px / 400 secondary |
| Highlight background | `selectedContentBackgroundColor` rounded rect; padding 6 px h, 2 px v; radius 6 px |
| Title color (idle / highlighted) | controlText / selectedMenuItemText |
| Subtitle ("last refreshed") | optional second line — Win port should add it: `"Last refreshed 14m ago"` 11 px secondary |

### 9.2 Keyboard shortcuts

| Action | Mac | Win |
|---|---|---|
| Refresh | `⌘R` | `Ctrl+R` |
| Settings | `⌘,` | `Ctrl+,` |
| Quit | `⌘Q` | `Ctrl+Q` |

Source: `StatusItemController+Menu.swift` `shortcut(for:)`. Display badge in
the right gutter of each row: 11 px secondary, e.g. `"Ctrl+R"`.

### 9.3 Right-click quick menu (per card)

On macOS this surfaces as the contextual NSMenu attached to each provider
icon. On Windows the in-popup right-click context shows:

```
Refresh just this provider
Open dashboard…
Open status page…           (if available)
─────
Switch account…
Open auth settings…
─────
Disable this provider
```

Use a native `muda` floating menu (not HTML). Styled by Win11 system theme.

### 9.4 Action icons (Fluent / Segoe MDL2 mapping)

| Mac SF Symbol | Win11 Segoe Fluent | Fallback (codepoint) |
|---|---|---|
| `arrow.clockwise` (refresh) | `` Refresh | |
| `chart.bar` (dashboard) | `` BarChart4 | |
| `waveform.path.ecg` (status) | `` Diagnostic | |
| `plus` (add account) | `` Add | |
| `person.crop.circle` (system account) | `` Contact | |
| `key` (switch account) | `` Permissions | |
| `terminal` | `` CommandPrompt | |
| `arrow.right.square` (login) | `` SignIn | |
| `gearshape` (settings) | `` Settings | |
| `info.circle` (about) | `` Info | |
| `xmark.rectangle` (quit) | `` ChromeClose | |
| `doc.on.doc` (copy) | `` Copy | |
| `chevron.right` (submenu) | `` ChevronRight | |

Icon color: secondary idle, primary on hover, `#ffffff` on highlighted row.

---

## 10. Click-to-copy overlay & microinteractions

### 10.1 ClickToCopyOverlay

Source: `ClickToCopyOverlay.swift`. Mounted on every error text and on the
`creditsHintText` line. The whole bounding rect of the text is the hit zone.

Mac behavior:
- `mouseDown` → write `copyText` to `NSPasteboard`. No visual feedback (just the
  copy, silently).

**Win port (polish-bar upgrade):**
1. Click → copy to clipboard via Tauri `clipboard_manager`.
2. Show a 1-line floating chip below the text: `"Copied ✓"`.
   - 11 px / 600, white-on-accent background, 9999 px corner radius, 8 px horizontal padding, 4 px vertical.
   - Appears with `120 ms` opacity 0→1 + translateY(4px → 0).
   - Holds for `1000 ms`, then fades out over `200 ms`.
3. Mouse cursor changes to `pointer` on hover — Mac doesn't, but Windows users
   expect it for copyable text.

### 10.2 The dedicated "copy icon" button (error subtitle)

Already documented in §3 (subtitle states). Same `120 ms` / `900 ms` timing
applies. Use the Segoe Fluent `Copy` () glyph at 11 px.

### 10.3 General microinteractions checklist

| Interaction | Trigger | Animation |
|---|---|---|
| Open popup | tray click | 140 ms ease-out fade + 4 px slide up |
| Close popup | click outside | 90 ms ease-in fade |
| Tab switch | switcher click | 120 ms cross-fade card stack |
| Card hover-in | mouse enter | 80 ms ease-out background fade |
| Card hover-out | mouse leave | 120 ms ease-in fade |
| Card press | mouseDown | scale 0.985, no fade |
| Bar percent change | data update | 200 ms ease-out scaleX on inner fill; skip on first paint |
| Pace stripe color flash | new "deficit" state | 600 ms color hold then revert |
| Copy chip | click copyable | 120 ms in, 1000 ms hold, 200 ms out |
| Refresh row | active refresh | spinner-rotate (1 turn / 1.2 s, linear, paused when no refresh) |
| Reset celebration | window resets while popup open | 1500 ms tray-icon morph; popup card flashes the bar fill **#34C759** for 400 ms then crossfades back |
| Account switch | tab change | 220 ms ease-out height tween + 120 ms text crossfade |

Easing tokens:
- `--ease-out: cubic-bezier(0.22, 1, 0.36, 1)`
- `--ease-in: cubic-bezier(0.32, 0, 0.67, 0)`
- `--ease-in-out: cubic-bezier(0.65, 0, 0.35, 1)`

---

## 11. Charts (rich submenus / inline blocks)

Five chart surfaces. All share a common skeleton (see §11.6).

### 11.1 Cost history chart (`CostHistoryChartMenuView`)

| Property | Value |
|---|---|
| Bars | one BarMark per day; provider brand color |
| Peak callout | top **5 %** of the peak bar is overlaid with `systemYellow` (mac) → Win `#FFD60A` |
| Height | **130 px** chart canvas |
| Y axis | hidden |
| X axis | `AxisGridLine.clear`, `AxisTick.clear`, **caption2** value labels (10 px, tertiary); only `[first, last]` shown |
| Selection band | `labelColor.opacity(0.1)` translucent rectangle covering the hovered day column. Win: `rgba(255,255,255,0.10)` dark / `rgba(0,0,0,0.08)` light |
| Hover trigger | `MouseLocationReader` — true mouse-move tracking, not SwiftUI `onHover` |
| Idle detail | `"Hover a bar for details"` (footnote secondary) |
| Selected primary line | `"{Mar 14}: $0.42 · 12,345 tokens"` (caption secondary, lineLimit 1) |
| Breakdown rows | up to **4**; each row = 2 px vertical accent strip (brand color, opacity 0.75 → 0.3 by index) + title (11 px secondary) + subtitle (10 px tertiary). Row height 24 px. |
| Padding inside chart card | 16 px horizontal, 10 px vertical |
| Footer line | `"Total (30d): $X.XX"` (caption secondary) |

### 11.2 Credits history chart (`CreditsHistoryChartMenuView`)

Same skeleton. Differences:
- Single bar color: `rgb(73, 163, 176)` (the "credits teal" — Mac literal). Win: `#49A3B0`.
- No breakdown rows; instead two-line detail: primary day total + optional first-service name.
- Footer: `"Total (30d): {N} credits"`.

### 11.3 Usage breakdown chart (`UsageBreakdownChartMenuView`)

Stacked bars per day, one segment per service. Differences from §11.1:
- `chartForegroundStyleScale(domain:range:)` for per-service colors.
- Service palette (ordered by total credits desc):
  - `cli` → `rgb(0.26, 0.55, 0.96)` → **#4290F5**
  - `github review*` → `rgb(0.94, 0.53, 0.18)` → **#F0872E**
  - rotating palette (`hash % 4`):
    - `#76BF5C`, `#CC73EB`, `#43C7DB`, `#F0BD43`
- **Legend grid** below the chart: `LazyVGrid(adaptive: minimum 110 px, alignment leading, spacing 6 px)`. Each cell: 7 × 7 px filled circle + 10 px caption2 secondary service name (truncate tail).
- Hover detail bottom rows show top **3** services for that day: `"{service} {credits}"` joined with ` · `.

### 11.4 Plan utilization history chart (`PlanUtilizationHistoryChartMenuView`)

| Property | Value |
|---|---|
| Chart height | 130 px |
| Detail line height | 16 px |
| Empty state height | 130 + 16 = 146 px |
| Max points | 30 |
| Max axis labels | 4 |
| Bar width | **6 px** explicit (others use auto) |
| Series picker | when multiple series (e.g. session vs weekly), `Picker(.segmented)` appears above the chart |
| Observed vs synthetic | observed points use brand color; synthetic (inferred) points use brand color at 0.45 opacity |
| Track behind bars | none (no full track) |
| Selection | same `selectionBandColor` as cost history |

### 11.5 Storage breakdown (`StorageBreakdownMenuView`)

A scrollable list, not a chart.

| Property | Value |
|---|---|
| Outer container | ScrollView vertical, `maxHeight: 560 px`, scrollIndicators visible |
| Header | "Storage" (body medium) + `"Total: 1.2 GB"` (caption secondary) |
| Body row spacing | 8 px between components |
| Component bar fill | `min(1, totalBytes / maxBytes)` proportional fraction |
| Bar height | (not specified; use 6 px to match UsageProgressBar) |
| Visible components | max **8** (`prefix(8)`) |
| Overflow line | `"{N} more items"` caption secondary |
| Cleanup recommendations | divider with `padding(.vertical, 2)`, then "Cleanup ideas" header (body medium) + per-recommendation row |
| Unreadable line | `"{N} unreadable item(s) skipped"` caption secondary |
| Per-component padding | 16 px horizontal, 10 px vertical (whole block) |

Each storage row is also click-to-copy (the file path).

### 11.6 Common chart skeleton

```tsx
<ChartCard>
  <ChartCanvas height={130}>
    <BarMark color={brand}/>
    <PeakOverlay color="#FFD60A" capHeightPct={5}/>
    <SelectionBand show={selected} color={hoverFill}/>
    <MouseMoveLayer onMove={updateSelection}/>
  </ChartCanvas>
  <DetailLine fontSize={11}>{detailPrimary}</DetailLine>
  <DetailLine fontSize={11} opacity={detail.secondary ? 1 : 0}>{detail.secondary || ' '}</DetailLine>
  <Optional><Legend/></Optional>
  <Optional><Footer/></Optional>
</ChartCard>
```

Common interactive behavior:
- Mouse enters plot → set `selectedDateKey` to nearest day by `proxy.value(atX:)`.
- Mouse leaves → clear selection (back to "Hover a bar for details").
- Touch/keyboard not supported on Mac; **Win port** must add arrow-left / arrow-right navigation + Tab focus.

### 11.7 Win port: chart library

- Use **uPlot** or **Recharts** for the BarMark equivalent. Recharts has cleaner
  composition but is heavier (~110 KB gzipped); uPlot is leaner (~50 KB) but
  needs more glue.
- **Recommendation: uPlot.** Tray-app perf budget is < 70 MB resident; every KB
  in the bundle matters.

---

## 12. Quota warning markers — recap

See §5.3 for visual. Additional Preferences semantics (mac → win):

| Setting | Mac storage | Win port |
|---|---|---|
| Master toggle | `SettingsStore.menuPreferences.hideQuotaWarningMarkers` | `prefs.display.hideQuotaWarningMarkers` |
| Thresholds (global) | `QuotaWarningThresholds.defaults = [50, 20]` | identical |
| Per-window override | `QuotaWarningRule.session/weekly/.thresholds` | identical, same schema |
| Per-window enable | `QuotaWarningRule.isEnabled(for:)` | identical |
| Allowed range | `0...99` | identical (anything < 1 or > 99 is dropped) |

Marker percent flip when `usageBarsShowUsed = true`: `[50, 20]` (remaining) →
`[50, 80]` (used). The bar still shows them in the *same* spatial positions —
the math is `showUsed ? 100 - t : t`.

---

## 13. Token-account stacking + switcher bar

Source: `StatusItemController+AccountMenuDisplay.swift` (and the switcher view).

### 13.1 Mode A — Stacked cards

For Claude / Codex with multiple `tokenAccounts`: the popup renders **one card
per account**, separated by full dividers. Cards are taller because each repeats
the header (provider name + account email).

- Max **6** accounts shown.
- Order: `tokenAccounts` array order (config file order); fallback to OS account list.
- An account with no fresh data uses the empty card variant (header + "No usage yet").

### 13.2 Mode B — Switcher bar

For ≥ 4 accounts (Mac uses `useTwoRows = accounts.count > 3`), the switcher
appears just below the provider switcher (or at the top if no provider switcher).

- Up to 2 rows.
- Row height 26 px, row spacing 4 px.
- Buttons distribute equally (`distribution = .fillEqually`).
- Active tab → `controlAccentColor` background, white text.
- Each card switch triggers a refresh of just that account (`refreshProvider`),
  with the popup showing a *loading* state on the active card for the duration.

### 13.3 Tooltip for truncated names

When middle-truncated, the tooltip shows the full `account.displayName` or
`account.menuDisplayName`.

### 13.4 Win port: how to decide

| Account count | Display |
|---|---|
| 1 | single card, no switcher |
| 2–3 | switcher bar, single-row |
| 4–6 | switcher bar, two-row |
| 7+ | switcher bar, two-row, last cell becomes a `"+N more…"` action that opens a list popup |

---

## 14. Codex-specific extras

Codex has the richest card. Beyond the standard 3 metrics, it shows:

| Section | Source | When visible |
|---|---|---|
| Code review metric (4th bar) | `codexProjection.supplementalMetrics.contains(.codeReview)` and `remainingPercent(for: .codeReview)` non-nil | always for Codex Pro+ accounts |
| Credits bar | scaled to 1000 tokens fullScale; shows when `metadata.supportsCredits` and `creditsRemaining` known | always |
| Token cost (Today + Last 30d) | `tokenCostUsageEnabled` and `costUsage` data present | always if enabled |
| Usage breakdown submenu | `projection.hasUsageBreakdown` | when daily usage history available |
| Credits history submenu | `projection.hasCreditsHistory` | when dashboard returned breakdown |
| Cost history submenu | `tokenSnapshot?.daily.isEmpty == false` and `isCostUsageEffectivelyEnabled` | always if cost usage active |
| Plan utilization submenu | `store.supportsPlanUtilizationHistory(for: .codex)` | always for Codex |
| **Buy Credits** action | `settings.showOptionalCreditsAndExtraUsage` AND `projection.canShowBuyCredits` | for low-balance Codex |

### 14.1 Credits bar specifics (`CreditsBarContent`)

| Property | Value |
|---|---|
| Full scale | **1000 tokens** (constant `fullScaleTokens`) |
| Percent | `clamp(remaining / 1000 * 100, 0, 100)` |
| Bar | UsageProgressBar with `accessibilityLabel: "Credits remaining"` |
| Left label | `"{credits} left"` (caption) |
| Right label | `"1K tokens"` (caption secondary, the scale) |
| Hint line | optional secondary text below; copyable when `creditsHintCopyText` set |

### 14.2 Buy Credits action

Mac: `makeBuyCreditsItem()` creates a primary-looking row that opens a hosted
purchase window (`OpenAICreditsPurchaseWindowController`).

**Win port:**
- Promote to a primary CTA button.
- Full-width card, 36 px tall, 8 px corner radius, accent background, white text 13 px / 600.
- Opens the OpenAI billing URL in the default browser (no in-app web view —
  cookie / OAuth boundary is cleaner that way).

### 14.3 Cost section (`UsageMenuCardCostSectionView`)

```
Cost                                 ← body / medium
Today: $0.42 · 12,345 tokens         ← caption
Last 30 days: $13.18 · 521K tokens   ← caption
{hint}                               ← footnote secondary, lineLimit 4
{error}                              ← footnote red (or selectionText if highlighted)
```

- Vertical spacing **6 px** between every line.
- Token counts are `UsageFormatter.tokenCountString`:
  - `< 10k` → `"9,876"`
  - `< 1M`  → `"123K"`
  - `≥ 1M`  → `"1.2M"`
- USD is `UsageFormatter.usdString` (`.currency(code:"USD").locale(en_US)`) for stability across locales.

---

## 15. Loading / empty / error states (per surface)

### 15.1 Provider card

| State | Body |
|---|---|
| Loading (refreshing, no prior snapshot) | header only; subtitle `"Refreshing..."`; no bars; no placeholder |
| Empty (snapshot nil, no error, not refreshing) | header + `"No usage yet"` (subheadline, secondary, single line) |
| Error | header subtitle in red, multi-line up to 4; copy-icon button shows; bars hidden |
| Loaded | full content |
| Stale data (older than refresh window) | bars + 50 % opacity overlay on the bar fill; subtitle adds `· stale` |

The `placeholder: "No usage yet"` text appears only when `snapshot == nil && !isRefreshing && lastError == nil`.

### 15.2 Charts

| State | Body |
|---|---|
| Empty | center text `"No cost history data."` / `"No credits history data."` / `"No usage breakdown data."` (footnote secondary, single line, accessibility label spelled out) |
| Loaded, no selection | detail line `"Hover a bar for details"` |
| Loaded, hovered | detail lines populated |
| Plan utilization, empty per series | single-line empty state at fixed 146 px height to prevent popup-height jitter |

### 15.3 Storage breakdown

| State | Body |
|---|---|
| Empty | `"No local data found"` (footnote secondary) |
| Some unreadable | `"{N} unreadable item(s) skipped"` (caption secondary) appended |
| Cleanup ideas present | divider + dedicated section |

### 15.4 Switcher

| State | Body |
|---|---|
| Single provider | switcher hidden |
| Provider switch in flight | active tab keeps its background; card area below cross-fades to new content (no spinner) |
| Account switch in flight | switcher tab gets a `4 × 4 px` accent dot pulsing at 1.5 Hz in its top-right corner; card area shows refreshing subtitle |

### 15.5 Refresh row

| State | Body |
|---|---|
| Idle | icon static, title `"Refresh"`, subtitle `"Last refreshed 14m ago"` (Win port add) |
| Active | icon spins (1.2 s linear, 360°), title `"Refreshing…"`, subtitle hidden |
| After failure | title `"Refresh failed"`, subtitle in red `"{short error}"`, row stays enabled |

---

## 16. Typography & color

### 16.1 Type ramp

| Token | Mac (SF Pro) | Win (Segoe UI Variable / Segoe UI) | Size · weight |
|---|---|---|---|
| `--font-headline` | `.headline` + `.semibold` | Segoe UI Variable Display Semibold | **14 px · 600** |
| `--font-title` | `.body` + `.medium` | Segoe UI Variable Text Semibold | **13 px · 600** |
| `--font-body` | `.body` | Segoe UI Variable Text Regular | **13 px · 400** |
| `--font-subheadline` | `.subheadline` | Segoe UI Variable Text Regular | **12 px · 400** |
| `--font-footnote` | `.footnote` | Segoe UI Variable Text Regular | **11 px · 400** |
| `--font-caption` | `.caption` | Segoe UI Variable Small Regular | **11 px · 400** |
| `--font-caption2` | `.caption2` | Segoe UI Variable Small Regular | **10 px · 400** |
| `--font-shortcut` | `NSFont.menuFont(smallSystemFontSize)` | Segoe UI Variable Text Regular | **11 px · 400** + letter-spacing 0.02em |

On Windows 10, fall back to **Segoe UI** (no Variable axis). The numbers stay the same; the visual weight ramp is slightly less subtle.

### 16.2 Color tokens (Win)

| Token | Dark | Light | Mac equivalent |
|---|---|---|---|
| `--surface-popup` | `#202020` over Mica | `#f9f9f9` over Mica | NSMenu vibrancy |
| `--surface-card` | `#2b2b2b` (rare; usually inherits Mica) | `#ffffff` | — |
| `--text-primary` | `#e6e6e6` | `#1f1f1f` | `controlTextColor` |
| `--text-secondary` | `#a0a0a0` | `#5d5d5d` | `secondaryLabelColor` |
| `--text-tertiary` | `#7a7a7a` | `#8a8a8a` | `tertiaryLabelColor` |
| `--text-on-accent` | `#ffffff` | `#ffffff` | `selectedMenuItemTextColor` |
| `--text-error` | `#FF453A` | `#D70015` | `systemRed` |
| `--text-warning` | `#FFD60A` | `#B25000` | `systemYellow` |
| `--text-success` | `#34C759` | `#248A3D` | `systemGreen` |
| `--accent` | `var(--win-accent)` (system) | `var(--win-accent)` | `controlAccentColor` |
| `--accent-12` | `accent at 12% alpha` | `accent at 12% alpha` | `selectedContentBackgroundColor` highlight |
| `--accent-18` | `accent at 18% alpha` | `accent at 18% alpha` | pressed states |
| `--divider` | `rgba(255,255,255,0.08)` | `rgba(0,0,0,0.08)` | `NSDivider` |
| `--bar-track` | `rgba(255,255,255,0.18)` | `rgba(0,0,0,0.14)` | `tertiaryLabelColor.opacity(0.22)` |
| `--hover-plate` | `rgba(255,255,255,0.06)` | `rgba(0,0,0,0.095)` | `labelColor.opacity(0.06)` / `black.opacity(0.095)` |
| `--switcher-light-wash` | n/a | `rgba(0,0,0,0.035)` | `black.opacity(0.035)` overlay |

Provider brand colors come from the descriptor registry; convert each
`(red, green, blue)` Double in 0..1 to hex once at build time and write to
`assets/provider-brand-colors.json`.

### 16.3 Letter spacing & line height

- All body/title text: line-height `1.35`.
- Headline (provider name): line-height `1.25`.
- Footnote captions: line-height `1.4` (more reading room).
- Letter spacing: default `0` everywhere except the keyboard shortcut badge (`0.02em`).

### 16.4 Disabled / dim

Disabled action row → `alphaValue = 0.7` on Mac. Win port: `opacity: 0.55` on the icon column + `opacity: 0.7` on the title. Subtitle of a disabled "Switch Account" row (e.g. when re-auth needed) is rendered in `--text-secondary`.

---

## 17. Acceptance checklist (visual parity)

Build a side-by-side screenshot harness that opens the Mac popup at the same DPR
as Win and checks the following. **Each row must pass before shipping.**

### 17.1 Container

- [ ] Popup width is `>= 360px` and matches the measured action width.
- [ ] Corner radius is `12 px` (Win) — never `8 px` or `6 px`.
- [ ] Mica is enabled and visible behind translucent surfaces; no opaque black backdrop on Win11.
- [ ] Popup shadow is visible on light theme, near-invisible on dark.
- [ ] Open animation < 200 ms; close < 100 ms.
- [ ] `Esc` dismisses; clicking the tray while open toggles dismiss.

### 17.2 Header

- [ ] Provider name is **14 px / 600**, truncates with `…` not `...`.
- [ ] Email truncates **middle** (e.g. `verylo…@example.com`), not tail.
- [ ] Subtitle says `"Updated just now"` for the first 60 s after a refresh.
- [ ] Error subtitle is red, wraps to up to 4 lines, has a working copy button.
- [ ] Copy button: tap → glyph swaps to checkmark, holds 900 ms, fades over 200 ms.
- [ ] When `hidePersonalInfo` is on, every email becomes `Hidden` (exact capitalization).

### 17.3 Bars

- [ ] Bar height is exactly **6 px**, ends are perfect semicircles.
- [ ] Track and fill colors flip when the card is highlighted (white on accent).
- [ ] Warning markers at 50 % and 20 % remaining are visible by default.
- [ ] Markers disappear when "Hide quota warning markers" is enabled.
- [ ] Pace tip stripes appear on Codex/Claude weekly bar when expected ≥ 3 %.
- [ ] Pace tip color: green = reserve, red = deficit, white when highlighted.
- [ ] No bar animates on first paint after popup open.
- [ ] On data update, bar tweens 200 ms ease-out; no layout reflow.

### 17.4 Text — exact strings

- [ ] `"Resets in 3h 12m"` format renders for between-1h-and-1d intervals.
- [ ] `"Resets in 1d"` (no hours) when hours == 0 and days > 0.
- [ ] `"Resets in 1m"` minimum (no `"0m"` or seconds).
- [ ] `"On pace"` shown when stage is `.onTrack`.
- [ ] `"12% in deficit"` shown when ahead-of-schedule (using deltaPercent int).
- [ ] `"8% in reserve · lasts until reset"` composed when behind + willLastToReset.
- [ ] `"Runs out in 3h 12m"` for ETA; `"Runs out now"` when countdown == "now".
- [ ] `"≈ 25% run-out risk"` rounded to nearest 5.
- [ ] `"Total (30d): $13.18"` footer on cost history.
- [ ] `"Hover a bar for details"` idle text on every chart.
- [ ] `"No usage yet"` empty-card placeholder.
- [ ] `"Refreshing..."` (three literal dots, not ellipsis char) during loading.

### 17.5 Switcher

- [ ] When merged + 2 providers + Overview enabled, tabs are: `[Overview, P1, P2]`.
- [ ] ≥ 4 providers with icons → tabs become stacked (icon over label).
- [ ] ≥ 15 providers → exactly 4 rows.
- [ ] Selected tab has accent background, white text.
- [ ] Unselected tabs show a 4 px brand-colored weekly remaining pill at the bottom.
- [ ] Hover plate appears on non-selected, non-pressed tab; selected hides the pill.
- [ ] In light theme, switcher area has a subtle dark wash.
- [ ] Tab click cross-fades 120 ms; never jumps.

### 17.6 Footer

- [ ] Refresh row is always present, persistent (does not blink during rebuild).
- [ ] Refresh row shows last-refreshed time as a sub-line.
- [ ] `Ctrl+R` triggers refresh; `Ctrl+,` opens settings; `Ctrl+Q` quits.
- [ ] Right-click on a card shows the contextual menu.

### 17.7 Charts

- [ ] Chart height is exactly 130 px.
- [ ] Peak bar shows a 5 %-of-max yellow cap at the top.
- [ ] Hover over a day → selection band appears + detail line(s) update.
- [ ] X-axis shows only first + last labels (10 px tertiary).
- [ ] Y-axis hidden.
- [ ] Cost history breakdown rows: 2 px accent strip + 11 px / 10 px text + brand color opacity ramp.
- [ ] Usage breakdown legend: 7 × 7 px circle + 10 px secondary label.

### 17.8 Microinteractions

- [ ] Copy chip appears on every copy interaction, 120 ms in, 1 s hold, 200 ms out.
- [ ] Card hover plate fades over 80 ms (in) / 120 ms (out).
- [ ] No state changes have a 0 ms snap unless the user explicitly switches view mode.
- [ ] Reset celebration triggers when a window resets while popup is open (400 ms green flash on the bar, then revert).
- [ ] Account switch shows a refreshing dot on the active tab.

### 17.9 Performance

- [ ] Click-to-popup-visible ≤ 100 ms.
- [ ] Tab switch ≤ 16 ms layout cost (one frame at 60 Hz).
- [ ] Bar update ≤ 16 ms; uses transform, not width.
- [ ] No layout reflow when chart tooltip updates (hover is paint-only).

### 17.10 Theming

- [ ] All colors react to Windows app theme change within one frame.
- [ ] System accent change reflects in active tab + bar tint instantly.
- [ ] Light/dark switch never leaves a hover plate stuck on.

---

## Appendix A — Source map

| Visual concern | Mac source of truth |
|---|---|
| Card layout & sections | `Sources/CodexBar/MenuCardView.swift` |
| Card variants (sectioned for OpenAI web menu items) | `UsageMenuCardHeaderSectionView`, `UsageMenuCardUsageSectionView`, `UsageMenuCardCreditsSectionView`, `UsageMenuCardCostSectionView`, `UsageMenuCardExtraUsageSectionView` |
| Per-provider metric construction | `MenuCardView.swift` `Model.metrics(input:)` + `MenuCardView+MiniMax.swift` + `MenuCardView+ModelHelpers.swift` |
| Quota warning markers | `MenuCardQuotaWarningMarkers.swift`, `QuotaWarningThresholds` in `CodexBarCore/Config/CodexBarConfig.swift` |
| Highlight semantics | `MenuHighlightStyle.swift` (`menuItemHighlighted` environment) |
| Container hosting (NSMenu ↔ SwiftUI) | `StatusItemController+MenuPresentation.swift` (`MenuCardItemHostingView`, `MenuCardSectionContainerView`, `PersistentMenuActionItemView`) |
| Menu assembly + smart updates | `StatusItemController+Menu.swift` |
| Action mapping | `StatusItemController+MenuActionMapping.swift`, `MenuDescriptor.swift` |
| Switcher | `StatusItemController+SwitcherViews.swift` (ProviderSwitcherView, TokenAccountSwitcherView, CodexAccountSwitcherView) + `ProviderSwitcherButtons.swift` (`PaddedToggleButton`, `InlineIconToggleButton`, `StackedToggleButton`) |
| Hosted submenus (chart placeholders) | `StatusItemController+HostedSubmenus.swift` |
| Usage history submenu | `StatusItemController+UsageHistoryMenu.swift` |
| Brand icon | `ProviderBrandIcon.swift` |
| Bar | `UsageProgressBar.swift` |
| Pace text | `UsagePaceText.swift`, `HistoricalUsagePace.swift` |
| Reset / updated formatting | `UsageFormatter.swift` (`ResetTimeDisplayStyle`, `resetLine`, `resetCountdownDescription`, `updatedString`) |
| Relative time | `Date+RelativeDescription.swift` |
| Click-to-copy | `ClickToCopyOverlay.swift` |
| Mouse hover for charts | `MouseLocationReader.swift` |
| Charts | `CostHistoryChartMenuView.swift`, `CreditsHistoryChartMenuView.swift`, `UsageBreakdownChartMenuView.swift`, `PlanUtilizationHistoryChartMenuView.swift`, `StorageBreakdownMenuView.swift` |
| PII redaction | `PersonalInfoRedactor.swift` |
| Menu redraw notifications | `Notifications+CodexBar.swift` (search `.codexbarMenuDidUpdate`, `.codexbarMenuShouldRebuild`) |

---

## Appendix B — Win port cheat-sheet (CSS variables)

```css
:root {
  /* layout */
  --popup-width: 360px;
  --gutter-h: 16px;
  --card-top: 2px;
  --card-bottom-no-credits: 6px;
  --card-bottom-with-credits: 2px;
  --gap-tight: 3px;
  --gap-row: 6px;
  --gap-section: 10px;
  --gap-block: 12px;
  --gap-list: 4px;
  --row-action: 28px;
  --row-switcher-inline: 30px;
  --row-switcher-stacked: 36px;
  --row-switcher-stacked-3plus: 40px;
  --switcher-row-gap-inline: 2px;
  --switcher-row-gap-stacked: 4px;
  --switcher-pad-pref: 16px;
  --switcher-pad-reduced: 10px;
  --switcher-pad-minimal: 6px;

  /* radius */
  --r-popup: 12px;
  --r-card-highlight: 6px;
  --r-tab: 6px;
  --r-bar: 3px;
  --r-pill: 9999px;

  /* animation */
  --d-popup-in: 140ms;
  --d-popup-out: 90ms;
  --d-tab-switch: 120ms;
  --d-hover-in: 80ms;
  --d-hover-out: 120ms;
  --d-bar-update: 200ms;
  --d-copy-in: 120ms;
  --d-copy-hold: 1000ms;
  --d-copy-out: 200ms;
  --ease-out: cubic-bezier(0.22, 1, 0.36, 1);
  --ease-in: cubic-bezier(0.32, 0, 0.67, 0);

  /* type */
  --font-stack: "Segoe UI Variable", "Segoe UI", system-ui, sans-serif;
  --fs-headline: 14px; --fw-headline: 600;
  --fs-title: 13px;    --fw-title: 600;
  --fs-body: 13px;     --fw-body: 400;
  --fs-subheadline: 12px;
  --fs-footnote: 11px;
  --fs-caption: 11px;
  --fs-caption2: 10px;
}

[data-theme="dark"] {
  --text-primary: #e6e6e6;
  --text-secondary: #a0a0a0;
  --text-tertiary: #7a7a7a;
  --text-on-accent: #ffffff;
  --text-error: #FF453A;
  --text-warning: #FFD60A;
  --text-success: #34C759;
  --divider: rgba(255,255,255,0.08);
  --bar-track: rgba(255,255,255,0.18);
  --hover-plate: rgba(255,255,255,0.06);
}

[data-theme="light"] {
  --text-primary: #1f1f1f;
  --text-secondary: #5d5d5d;
  --text-tertiary: #8a8a8a;
  --text-on-accent: #ffffff;
  --text-error: #D70015;
  --text-warning: #B25000;
  --text-success: #248A3D;
  --divider: rgba(0,0,0,0.08);
  --bar-track: rgba(0,0,0,0.14);
  --hover-plate: rgba(0,0,0,0.095);
  --switcher-wash: rgba(0,0,0,0.035);
}
```

---

## Appendix C — What we explicitly are *not* porting

- **NSMenu vibrancy via `allowsVibrancy = true`** — Windows uses Mica; no
  equivalent setting needed on the WebView.
- **Smart-update flicker prevention via `CATransaction`** — React's reconciler
  + a single keyed `<ProviderSwitcher>` element solves this for free.
- **Hosted SwiftUI inside an NSMenu** — there is no menu host on Windows; the
  whole popup is a flat WebView, and `muda` handles only the right-click
  context menu (which is HTML-free per §05).
- **Per-display screen-confetti reset celebration** — replaced by a tray-icon
  morph + a single 400 ms bar flash inside the popup.
- **Per-button `NSTrackingArea`** — DOM `mouseenter` / `mouseleave` is enough.
  Don't poll `requestAnimationFrame` for hover.
