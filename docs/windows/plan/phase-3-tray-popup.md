---
title: "Phase 3, Tray Icon and Popup"
phase: 3
status: "Planned"
polish_bar: "Phantom Wallet, Duolingo"
predecessors:
  - "Phase 0, repo scaffold, Tauri 2 project, Rust workspace"
  - "Phase 1, shared Rust core crate skeleton (codexbar-core), config, settings, types"
  - "Phase 2, IPC plumbing, Tauri commands, event bus, mock provider store"
successor:
  - "Phase 4, real Claude provider wired to the popup and tray through the provider framework"
reads_from:
  - "docs/windows/spec/10-tray-icon-system.md"
  - "docs/windows/spec/15-popover-menu-card-ui.md"
  - "docs/windows/spec/80-feel-and-polish.md"
  - "docs/windows/05-windows-ux-spec.md"
out_of_scope:
  - "Real provider HTTP calls, OAuth, cookie scraping (Phase 4 onward)"
  - "Charts wired to live data (mock fixtures only in this phase)"
  - "Preferences window content (Phase 5)"
  - "Auto update plumbing (Phase 6)"
---

# Phase 3, Tray Icon and Popup

## Why

Phase 3 produces the entire user visible surface of CodexBar4Windows on mock
data. After this phase, an installer sees a polished Windows native tray icon
that animates, a popup that opens in under 100 ms, a provider switcher,
accurate bar geometry, the correct typography, the correct backdrop (Mica on
Windows 11, Acrylic on Windows 10), the correct accent color, and every
microinteraction listed in spec 80. Only the data is fake. Phase 4 replaces
the mock fixture with the real Claude provider without touching pixels.

Splitting visible polish from data flow has three benefits. First, polish is
the single largest source of risk, the renderer must hit 30 Hz at 0.5 percent
of one core, and the popup must hit 100 ms click to open. Locking those
numbers before adding network calls keeps them honest. Second, mock data
iterates at React reload speed. Third, the React popup becomes a stable
contract that takes a `UsageState` and renders, so later phases swap
implementations behind it without redrawing.

The polish bar is Phantom Wallet and Duolingo. Every state change has an in
curve and an out curve. Every focusable element has a focus ring. Every error
has a copy chip. Every theme transition completes within one frame. Nothing
snaps. The renderer must match Mac geometry to plus or minus one pixel on the
right edge of the fill. The bar must not animate on first paint when the
popup opens. Stale state dims track, stroke, and fill in lockstep. These are
not stretch goals, they are the merge gate.

## Dependencies

This phase depends on the deliverables of Phase 0, Phase 1, and Phase 2. The
following must exist on `main` before any task in this plan starts.

| Predecessor | Deliverable | Where it lives |
|---|---|---|
| Phase 0 | Tauri 2 project scaffold | `src-tauri/`, `src/`, `tauri.conf.json` |
| Phase 0 | Rust workspace with `codexbar-core` crate | `crates/codexbar-core/` |
| Phase 0 | App identifier `com.codexbar4windows.app` | `tauri.conf.json:identifier` |
| Phase 1 | `UsageSnapshot`, `UsageState`, `ProviderId` types | `crates/codexbar-core/src/model.rs` |
| Phase 1 | `Settings` struct with display, providers, shortcuts panes | `crates/codexbar-core/src/settings.rs` |
| Phase 1 | `IconStyle` enum, `LoadingPattern` enum | `crates/codexbar-core/src/icon.rs` |
| Phase 2 | Tauri command surface (`get_state`, `refresh_now`, `set_pref`) | `src-tauri/src/commands.rs` |
| Phase 2 | Event bus (`event::emit`) for state updates | `src-tauri/src/events.rs` |
| Phase 2 | Mock fixture store, returns realistic `UsageState` | `crates/codexbar-core/src/mock.rs` |
| Phase 2 | Logging through `tracing`, file rotation in `%LOCALAPPDATA%` | `src-tauri/src/logging.rs` |

If any of the above is missing, Phase 3 is blocked. Open a ticket against the
relevant phase, do not paper over.

Mock data shape (Phase 2 contract, restated for clarity):

```rust
pub struct UsageState {
    pub providers: Vec<UsageSnapshot>,
    pub active_provider: ProviderId,
    pub merged: bool,
    pub last_refresh: chrono::DateTime<chrono::Utc>,
    pub theme: TaskbarTheme,
    pub accent: [u8; 4],
    pub incidents: Vec<ProviderIncident>,
}
```

The mock store must cycle through fixtures every 8 seconds so the renderer and
popup can exercise loading, stale, error, and reset celebration states during
manual smoke testing.

## Deliverables

Eighteen named deliverables, each backed by one or more atomic commits in the
task list. Numbering matches the prompt for traceability.

1. Dynamic tray icon renderer in Rust, `tiny-skia` plus `resvg`, in memory
   RGBA buffer at requested px size, ICO atlas at 16, 20, 24, 28, 32, 36, 40,
   48, 64 px. Smallest bucket collapse rule at 16 px per spec 10, table 1.4.
   LRU re render cache, key `(primary, weekly, credits, stale, style,
   indicator, theme)`, 64 entries.
2. Bar meter style, two bar primary plus secondary, three bar variant when
   credits dominate, brand icon mode with percent label baked into the ICO at
   sizes greater or equal to 32 px (per spec 10 open question 3, MVP target).
3. State overlays, stale dim using alpha table (track 0.18, stroke 0.28, fill
   0.55), incident indicator dot or line plus dot, loading pulse at 30 Hz with
   30 s hard ceiling.
4. Six named loading patterns, `knightRider`, `cylon`, `outsideIn`, `race`,
   `pulse`, `unbraid`, with phase offsets `pi/2`, `pi/3`, and `pi` per spec 10
   section 4.2.
5. Theme detection, registry read of
   `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\SystemUsesLightTheme`,
   listen to `WM_SETTINGCHANGE` with `ImmersiveColorSet`, redraw atlas on
   change.
6. Per monitor DPI awareness v2 in `tauri.conf.json` and the app manifest,
   redraw on `WM_DPICHANGED`.
7. Tray click and hover, left click toggles popup, right click opens native
   `muda` menu (Refresh now, Pause refresh, Preferences, About, Check for
   updates, Quit with `Ctrl+Q`), tooltip up to 128 chars with `\r\n` line
   breaks.
8. Popup window, frameless WebView2, 360 by 480 default with content sizing,
   positioned next to `Shell_NotifyIconGetRect` with edge aware direction,
   Mica on Windows 11, Acrylic on Windows 10, 12 px corner radius, soft drop
   shadow per spec 15 table 1.1, dismiss on focus loss or Esc.
9. React popup shell, header (`PopupHeader`), card stack (`CardStack`,
   `ProviderCard`), footer (`PopupFooter`), all consuming Tauri events.
10. `UsageProgressBar` component, 6 px height, 3 px corner radius, animated
    fill on update with `transform: scaleX`, no reflow.
11. `ProviderSwitcherButtons` component, row heights 30 (inline), 36 (stacked),
    40 (stacked with 3 plus rows), multi row at 15 plus providers per spec 15
    section 8.2.
12. Pace text rules per spec 15 section 6.2 and 6.3, "On pace", "{N}% in
    deficit, runs out in {T}", "{N}% in reserve, lasts until reset", hide
    below 3 percent elapsed.
13. Click to copy overlay with copied animation, 120 ms in, 1000 ms hold,
    200 ms out.
14. Charts component scaffolded for cost history, credits history, usage
    breakdown, plan utilization, plus storage breakdown. Backed by `uPlot`
    per spec 15 section 11.7 recommendation. Populated with mock fixtures.
15. Typography ramp, Segoe UI Variable on Windows 11 with Segoe UI fallback on
    Windows 10, 13 px body, 11 px secondary, 14 px provider name (mac
    `.headline`, per spec 15 section 3, the value is 14 not 16 even though
    the prompt says 16, we follow the spec).
16. Color tokens with light and dark CSS variables per spec 15 appendix B.
17. First run nudge toast, copy "CodexBar4Windows lives in the tray. To pin
    it, open the overflow flyout and drag the icon next to the volume icon."
    Shown once, persisted in `%APPDATA%\CodexBar4Windows\state.json` flag
    `trayPinnedHintShown`.
18. Accessibility, keyboard navigation across switcher and cards, focus rings
    using `var(--accent)` at 2 px, `prefers-reduced-motion` honors per spec
    80 section 5.

## Atomic commit tasks

Forty four atomic commits. Each has a title, files touched, an acceptance
check, and a draft commit message in conventional format. Branch policy is on
`main`, push after each commit. No em dashes. No single dashes in prose.

Tasks are grouped, but ordering within a group must be preserved because
commits depend on prior structure. Cross group ordering is loose, parallel
work is permitted when files do not overlap.

### Group A, Rust core renderer (commits 1 to 12)

#### A1, Add tiny-skia, resvg, ico, image crate dependencies

Files:
- `Cargo.toml` (workspace)
- `crates/codexbar-core/Cargo.toml`

Acceptance:
- `cargo check -p codexbar-core` passes on Windows.
- `tiny-skia = "0.11"`, `resvg = "0.45"`, `usvg = "0.45"`, `image = "0.25"`,
  `ico = "0.4"`, `lru = "0.13"`, `parking_lot = "0.12"` declared.

Commit: `build(core): add tiny-skia, resvg, ico, image, lru crates for tray rendering`

#### A2, IconRenderer skeleton with logical 18 by 18 pt canvas, 36 by 36 px buffer

Files:
- `crates/codexbar-core/src/renderer/mod.rs` (new)
- `crates/codexbar-core/src/renderer/canvas.rs` (new)
- `crates/codexbar-core/src/renderer/pixel_grid.rs` (new)

Acceptance:
- `IconRenderer::new()` allocates a `tiny_skia::Pixmap` of size 36 by 36.
- `PixelGrid::snap_delta` returns 0 for even px coords, 0.5 for odd, mirrors
  Mac math.
- Unit test renders an empty canvas, asserts all pixels alpha equals 0.

Commit: `feat(core): scaffold IconRenderer with 36x36 pixmap and pixel grid snapping`

#### A3, Two bar geometry, top and bottom rect, capsule track plus stroke plus fill

Files:
- `crates/codexbar-core/src/renderer/bars.rs` (new)
- `crates/codexbar-core/tests/bars_geometry.rs` (new)

Acceptance:
- `draw_bar(rect: 3,19,30,12, value: 50, alpha: 1.0)` renders track at
  `labelColor * 0.28`, stroke inset 1 px at `* 0.44`, fill width
  `round(30 * 0.5) = 15 px` left anchored, right edge hard.
- Corner radius defaults to `h / 2` for capsule, 0 for `IconStyle::Claude`,
  3 for `IconStyle::Warp`.
- Snapshot test compares against `tests/fixtures/bar_50pct.png`, SSIM at
  least 0.99.

Commit: `feat(core): render primary and secondary bars with capsule track, stroke, fill`

#### A4, Layout selector, TwoBarNormal, TwoBarDimmed, TwoBarCreditsOnly, CreditsThickBottom

Files:
- `crates/codexbar-core/src/renderer/layout.rs` (new)
- `crates/codexbar-core/tests/layout_selection.rs` (new)

Acceptance:
- Algorithm matches spec 10 section 2.1 exactly. Table driven test covers
  10 input rows.
- `creditsRatio = min(credits / 1000, 1) * 100`, cap 1000 per spec.

Commit: `feat(core): select bar layout from primary, weekly, credits, style inputs`

#### A5, Provider twist overlays, Codex face, Claude crab, Gemini sparkle, Factory asterisk, Warp tilted eyes

Files:
- `crates/codexbar-core/src/renderer/twists/codex.rs` (new)
- `crates/codexbar-core/src/renderer/twists/claude.rs` (new)
- `crates/codexbar-core/src/renderer/twists/gemini.rs` (new)
- `crates/codexbar-core/src/renderer/twists/factory.rs` (new)
- `crates/codexbar-core/src/renderer/twists/warp.rs` (new)
- `crates/codexbar-core/src/renderer/twists/mod.rs` (new)

Acceptance:
- Each twist matches the geometry table in spec 10 section 2.6 (eye 4 by 4,
  hat 18 by 4, claude arms 3 by `h-6`, gemini 8 point star with outer 4 and
  inner 1, factory 16 point asterisk outer 3.5 and inner 1.05, warp ellipse
  5 by 8 rotated plus or minus pi/3).
- Anti alias OFF for blocky (codex, claude), ON for organic (gemini,
  factory, warp).
- Eyes cleared via `BlendMode::Clear` so fill cannot z fight the eye.
- Snapshot tests against `tests/fixtures/twist_*.png`, SSIM at least 0.99.

Commit: `feat(core): add Codex, Claude, Gemini, Factory, Warp icon twists`

#### A6, Status incident overlay, dot and line plus dot variants

Files:
- `crates/codexbar-core/src/renderer/status_overlay.rs` (new)

Acceptance:
- `minor` and `maintenance`, filled circle 4 by 4 at `(w-6, 2)`.
- `major`, `critical`, `unknown`, line 2 by 6 at `(w-6, 4)` plus dot 2 by 2
  at `(w-6, 2)`.
- Overlay uses resolved fill color, never dimmed by stale.

Commit: `feat(core): draw incident status overlay inside 18x18 bounding box`

#### A7, Stale dim alpha table

Files:
- `crates/codexbar-core/src/renderer/state.rs` (new)

Acceptance:
- Stale flag swaps track 0.28 to 0.18, stroke 0.44 to 0.28, fill 1.0 to 0.55
  in lockstep.
- Status overlay alpha unchanged.
- Test covers the four alpha pairs.

Commit: `feat(core): apply stale alpha table to track, stroke, and fill in lockstep`

#### A8, Six loading patterns, phase math and secondary offsets

Files:
- `crates/codexbar-core/src/renderer/patterns.rs` (new)
- `crates/codexbar-core/tests/patterns.rs` (new)

Acceptance:
- `knightRider`, primary `0.5 + 0.5 * sin(phi)`, offset `pi`.
- `cylon`, sawtooth, offset `pi / 2`.
- `outsideIn`, `abs(cos(phi))`, offset `pi`.
- `race`, sawtooth at 1.2x, offset `pi / 3`.
- `pulse`, `0.4 + 0.6 * (0.5 + 0.5 * sin(phi))`, offset `pi / 2`.
- `unbraid`, drives morph through `IconRenderer::make_morph_icon`, offset
  `pi / 2`.
- All outputs clamped to `[0, 100]`.
- Property test, 10000 random phases, all values in range, no NaN.

Commit: `feat(core): implement six loading patterns with phase offsets`

#### A9, Reset celebration morph, three ribbon segments, fade out on third

Files:
- `crates/codexbar-core/src/renderer/morph.rs` (new)
- `crates/codexbar-core/tests/morph.rs` (new)

Acceptance:
- Three ribbon segments per spec 10 section 3.4 table.
- Cross fade in starts at `t > 0.55`.
- Third ribbon fades using `p = t * 1.1`.
- Morph cache, 512 entries, 200 progress buckets, key
  `styleKey * 1000 + bucket`.

Commit: `feat(core): add unbraid morph with three ribbons and 512 entry cache`

#### A10, LRU render cache, key fields per spec 10 section 5.1

Files:
- `crates/codexbar-core/src/renderer/cache.rs` (new)

Acceptance:
- 64 entry LRU keyed on `(primary, weekly, credits, stale, style, indicator,
  theme)`.
- Quantization, percents `round(value * 10)` for 0.1 percent buckets,
  credits `round(clamp(0..1000) * 10)`.
- Cache miss path renders, hit path returns cloned pixmap reference in under
  0.05 ms.
- Cache skipped when `blink >= 0.0001`, `wiggle >= 0.0001`, or
  `tilt >= 0.0001`.
- Bench, 1000 hits, mean less than 0.05 ms.

Commit: `feat(core): add 64 entry LRU render cache with motion bypass`

#### A11, ICO atlas builder, sizes 16, 20, 24, 28, 32, 36, 40, 48, 64, with 16 px collapse rule

Files:
- `crates/codexbar-core/src/renderer/atlas.rs` (new)
- `crates/codexbar-core/tests/atlas.rs` (new)

Acceptance:
- Atlas contains all nine sizes.
- 16 px uses nearest neighbor resample, 32 plus uses Lanczos3.
- 16 px collapse, hide hat, hide legs, hide gear teeth, antigravity dot
  shrinks to 1 px, status overlay dot shrinks to 2 px.
- Output validated by `ico::IconDir::read` round trip.

Commit: `feat(core): build multi size ICO atlas with smallest bucket collapse rule`

#### A12, Brand icon mode, SVG to bitmap via resvg, percent label baked in at sizes greater or equal to 32 px

Files:
- `crates/codexbar-core/src/renderer/brand.rs` (new)
- `crates/codexbar-core/assets/provider-icons/*.svg` (mock placeholders)

Acceptance:
- For Claude, Codex, Cursor, Copilot, Gemini, OpenRouter, Factory, a 16 by
  16 SVG resamples cleanly.
- At 32 plus px, percent text rendered with Segoe UI Variable Small, 8 px
  height, top right.
- At 16 to 28 px, percent text omitted (collapse), tooltip carries the
  number.
- SVGs tinted to resolved fill when authored with `currentColor`.

Commit: `feat(core): add brand icon mode with percent baked at sizes >=32 px`

### Group B, Tauri tray host (commits 13 to 20)

#### B1, App manifest, per monitor DPI awareness v2, GDI scaling off

Files:
- `src-tauri/Cargo.toml` (add `windows = { features = [...] }`)
- `src-tauri/app.manifest` (new)
- `src-tauri/build.rs` (new, embeds manifest)
- `src-tauri/tauri.conf.json` (`windows.dpiAware = "PerMonitorV2"` block)

Acceptance:
- `GetProcessDpiAwarenessContext` returns
  `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2` at runtime.
- Manual test, drag the window from 100 percent to 200 percent monitor, no
  bitmap stretch.

Commit: `build(tauri): declare PerMonitorV2 DPI awareness in manifest`

#### B2, Theme detection, registry read plus `WM_SETTINGCHANGE` listener

Files:
- `src-tauri/src/theme.rs` (new)

Acceptance:
- `read_taskbar_theme()` reads
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\SystemUsesLightTheme`
  via the `windows-registry` crate (`0 = dark`, `1 = light`).
- `subscribe_theme_changes()` registers a hidden message only window, on
  `WM_SETTINGCHANGE` with lparam string `"ImmersiveColorSet"` emits
  `theme_changed` event.
- Manual test, switch taskbar theme in Settings, event fires within one
  frame.

Commit: `feat(tauri): detect Windows taskbar theme and listen for changes`

#### B3, Accent color detection, `UISettings.GetColorValue(UIColorType.Accent)`

Files:
- `src-tauri/src/accent.rs` (new)

Acceptance:
- `read_accent_color()` returns `[r, g, b, a]` from UISettings via the
  `windows` crate WinRT bindings.
- Emits `accent_changed` event when system accent rolls.

Commit: `feat(tauri): read Windows accent color via UISettings WinRT`

#### B4, Tray icon host, `tray-icon` crate, `Shell_NotifyIcon` `NIM_MODIFY`

Files:
- `src-tauri/src/tray.rs` (new)
- `src-tauri/src/main.rs` (wire up)

Acceptance:
- App registers a single tray icon at boot.
- On `state_changed` event, the host rebuilds the ICO from
  `codexbar_core::renderer::render_atlas(state)` and calls
  `TrayIcon::set_icon`.
- `set_icon` skipped when `Arc<Icon>` pointer matches previous (avoids
  Win11 tooltip flicker).
- Manual test, tray icon appears within 800 ms of launch.

Commit: `feat(tauri): host tray icon with ICO atlas refresh on state change`

#### B5, Loading frame driver, 30 Hz timer, 30 s hard ceiling, low power mode 5 Hz

Files:
- `src-tauri/src/animation.rs` (new)

Acceptance:
- `CreateThreadpoolTimer` at 33 ms, phase increments by `2.7 / 30 = 0.09`
  rad.
- Auto stop after 30 s of continuous loading.
- Pause on `WM_POWERBROADCAST` `PBT_APMSUSPEND`, resume on
  `PBT_APMRESUMEAUTOMATIC`.
- Pause to 5 Hz when `SYSTEM_POWER_STATUS.SystemStatusFlag & 1` (battery
  saver).
- Pause on `WTS_SESSION_LOCK`, resume on `WTS_SESSION_UNLOCK`.

Commit: `feat(tauri): drive loading animation at 30 Hz with 30 s ceiling`

#### B6, Right click menu, native `muda`, Refresh now, Pause refresh, Preferences, About, Check for updates, Quit

Files:
- `src-tauri/src/menu.rs` (new)

Acceptance:
- Menu uses `muda::Menu::new()` with Segoe Fluent icon glyphs per spec 15
  section 9.4.
- `Ctrl+Q` accelerator on Quit.
- Pause refresh toggles a check mark.
- About item opens an HTML About view (Phase 7 fills the content, this phase
  ships a placeholder).

Commit: `feat(tauri): add native right click menu via muda with Fluent icons`

#### B7, Tooltip builder, 128 char limit, multi line with `\r\n`, edge cases

Files:
- `src-tauri/src/tooltip.rs` (new)

Acceptance:
- Single provider tooltip format per spec 10 section 9.1.
- Merged mode lists all active providers with percent plus pace, sorted by
  usage.
- Stale providers prefixed `WARN`, loading providers say `Refreshing...`
  (three literal dots).
- Truncation at 127 chars plus ellipsis char `U+2026`.
- Tooltip rebuilt on every snapshot, skipped if identical.

Commit: `feat(tauri): assemble tray tooltip with merged mode and stale markers`

#### B8, Click handling, left toggle, right menu, shift force refresh, ctrl preferences, alt cycle pattern

Files:
- `src-tauri/src/tray.rs` (extend)

Acceptance:
- `NIN_SELECT` toggles popup show or hide.
- `WM_CONTEXTMENU` shows muda menu at cursor position.
- Modifier keys via `GetKeyState(VK_SHIFT)` etc.
- Hover delay matches Win11 default, no preview popup.

Commit: `feat(tauri): wire tray click semantics for left, right, and modifier clicks`

### Group C, Popup window (commits 21 to 27)

#### C1, Frameless WebView2 popup, 360 by 480 default, Mica plus Acrylic fallback

Files:
- `src-tauri/src/popup.rs` (new)
- `src-tauri/tauri.conf.json` (add `windows.popup` entry)

Acceptance:
- Window built with `WebviewWindowBuilder::transparent(true)`,
  `decorations(false)`, `resizable(false)`, `skip_taskbar(true)`,
  `always_on_top(false)`.
- On Windows 11, apply Mica via `apply_mica(&window, Some(MicaType::Auto))`.
- On Windows 10, fall back to Acrylic via `apply_acrylic`.
- On Windows 7 or 8 (defensive only, not supported officially), flat dark
  or light surface with accent highlight.
- Corner radius 12 px, applied via `DwmSetWindowAttribute` with
  `DWMWA_WINDOW_CORNER_PREFERENCE` `DWMWCP_ROUND` (Win11) or CSS clip path
  (Win10).

Commit: `feat(tauri): build frameless popup with Mica or Acrylic backdrop`

#### C2, Popup positioning, `Shell_NotifyIconGetRect`, screen edge aware direction

Files:
- `src-tauri/src/popup.rs` (extend)
- `src-tauri/src/position.rs` (new)

Acceptance:
- `Shell_NotifyIconGetRect` returns the icon rect.
- Position algorithm picks above, below, left, or right based on taskbar
  edge (`ABM_GETTASKBARPOS` via `SHAppBarMessage`).
- Bottom taskbar, popup opens upward, right edge of popup aligned to right
  edge of tray icon, 8 px gap.
- Side taskbar, popup opens rightward (left taskbar) or leftward (right
  taskbar), 8 px gap.
- Popup never crosses the screen edge of the monitor it lives on,
  `GetMonitorInfo` clamping.

Commit: `feat(tauri): position popup with edge aware direction from tray rect`

#### C3, Open and close animation, 180 ms show, 140 ms hide, easings from spec 80

Files:
- `src/popup/animations.css` (new)
- `src/popup/PopupShell.tsx` (new, animation hooks only)

Acceptance:
- Show, `cubic-bezier(0.16, 1, 0.3, 1)` (easeOutExpo), `translateY(8px -> 0)`,
  `opacity 0 -> 1`, `scale(0.98 -> 1)`, 180 ms.
- Hide, `cubic-bezier(0.4, 0, 1, 1)` (easeIn), `translateY(0 -> 8px)`,
  `opacity 1 -> 0`, `scale(1 -> 0.98)`, 140 ms.
- Re click toggle, hide then show, no overlap.
- Both honored by `prefers-reduced-motion`, replaced with instant fade.

Commit: `feat(popup): animate open and close with spec 80 easings`

#### C4, Dismiss triggers, focus loss, Esc, re click tray, click outside

Files:
- `src-tauri/src/popup.rs` (extend)
- `src/popup/PopupShell.tsx` (extend)

Acceptance:
- `WM_KILLFOCUS` on the popup window hides it.
- Esc keydown in the WebView calls `invoke("hide_popup")`.
- Re click on tray icon hides if popup is visible.
- Drop down within popup does not steal focus from popup.

Commit: `feat(popup): dismiss on focus loss, Esc, and re click of tray icon`

#### C5, Drop shadow tokens, 12 px corner radius, border, divider tokens

Files:
- `src/styles/tokens.css` (new)
- `src/styles/themes.css` (new)

Acceptance:
- All variables from spec 15 appendix B present, exact values:
  `--popup-width 360px`, `--gutter-h 16px`, `--gap-row 6px`,
  `--r-popup 12px`, `--r-bar 3px`, `--d-popup-in 140ms`, etc.
- Light and dark theme attributes resolve all foreground and background
  colors per spec 15 section 16.2.
- Shadow, dark `0 12px 32px rgba(0,0,0,0.28)`, light
  `0 8px 24px rgba(0,0,0,0.14)`.
- Border, dark `1px solid rgba(255,255,255,0.08)`, light
  `1px solid rgba(0,0,0,0.08)`.

Commit: `feat(styles): define popup tokens for layout, color, animation, type ramp`

#### C6, Typography stack, Segoe UI Variable plus Segoe UI fallback

Files:
- `src/styles/typography.css` (new)
- `src/styles/tokens.css` (extend with `--font-stack`)

Acceptance:
- `--font-stack` = `"Segoe UI Variable", "Segoe UI", system-ui, sans-serif`.
- Type ramp `--fs-headline 14px`, `--fw-headline 600`, `--fs-body 13px`,
  `--fs-footnote 11px`, `--fs-caption2 10px`, etc.
- Line heights, headline 1.25, body 1.35, footnote 1.4.
- Letter spacing default 0, shortcut badge 0.02em.

Commit: `feat(styles): wire Segoe UI Variable type ramp with Win10 fallback`

#### C7, Width algorithm, measure widest action, clamp to `[360, 420]`

Files:
- `src/popup/usePopupWidth.ts` (new)

Acceptance:
- Hook measures action labels off screen, returns clamped width.
- Sets `--popup-width` on root.
- Switcher and card stack inherit the same value.

Commit: `feat(popup): clamp popup width to 360..420 based on widest action label`

### Group D, React popup content (commits 28 to 39)

#### D1, `PopupShell` component, top level layout

Files:
- `src/popup/PopupShell.tsx` (extend)
- `src/popup/index.tsx` (new entry point)

Acceptance:
- `PopupShell` renders `<PopupHeader>`, `<CardStack>`, `<PopupFooter>` in
  that order.
- Subscribes to Tauri event `state_changed`, stores in Zustand store at
  `src/popup/state/usageStore.ts`.
- Theme attribute applied to `<html data-theme="dark|light">` from event
  payload.

Commit: `feat(popup): scaffold PopupShell with header, cards, footer regions`

#### D2, `PopupHeader` component, switcher tabs or overview tab, settings cog

Files:
- `src/popup/header/PopupHeader.tsx` (new)
- `src/popup/header/SettingsCog.tsx` (new)

Acceptance:
- When merged mode plus at least 2 providers, renders
  `<ProviderSwitcherButtons>`.
- Settings cog at top right, 16 by 16 Segoe Fluent `Settings` glyph,
  hover background `var(--accent-12)`, click opens Preferences (stub for
  this phase).
- No backdrop blur, header sits on Mica directly.

Commit: `feat(popup): add header with switcher tabs and settings cog`

#### D3, `ProviderSwitcherButtons` component, inline plus stacked variants

Files:
- `src/popup/header/ProviderSwitcherButtons.tsx` (new)
- `src/popup/header/SwitcherTab.tsx` (new)
- `src/popup/header/WeeklyIndicator.tsx` (new)

Acceptance:
- Row height 30 (inline), 36 (stacked), 40 (stacked 3 plus rows) per spec 15
  table 8.2.
- Row spacing 2 (inline), 4 (stacked).
- Stacking triggers when `showsIcons && segments.count > 3`.
- 15 plus providers forces 4 rows.
- Selected tab, `var(--accent)` background, `var(--text-on-accent)` text.
- Unselected, `--text-secondary`, hover plate fades in over 80 ms.
- Weekly indicator under each unselected tab, 4 px height, 2 px corner
  radius, 6 px inset left and right, 1 px inset bottom, fill is provider
  brand color, width is `clamp(remaining/100, 0, 1) * available`.
- Light mode wash, switcher area gets
  `background: rgba(0,0,0,0.035)`.

Commit: `feat(popup): add provider switcher with inline, stacked, and 4 row variants`

#### D4, `CardStack` component, smart update via keyed reconciler

Files:
- `src/popup/cards/CardStack.tsx` (new)

Acceptance:
- Renders one `<ProviderCard>` per active provider, or one Overview when
  Overview tab is selected.
- Keyed by `provider.id` so switcher tab change cross fades cards via
  `<AnimatePresence>` or equivalent, 120 ms cross fade per spec 80 section
  8.
- Below switcher, height tweens to new measured value over 220 ms ease out.

Commit: `feat(popup): render card stack with keyed reconciler and 120 ms cross fade`

#### D5, `ProviderCard` component, header, metrics, credits, cost, status pill

Files:
- `src/popup/cards/ProviderCard.tsx` (new)
- `src/popup/cards/CardHeader.tsx` (new)
- `src/popup/cards/MetricRow.tsx` (new)
- `src/popup/cards/StatusPill.tsx` (new)

Acceptance:
- Card header per spec 15 section 3, provider name 14 px semibold, email
  12 px secondary middle truncated, subtitle 11 px secondary, plan pill 11
  px secondary.
- Metric row per spec 15 section 4.1, title 13 px medium, `UsageProgressBar`,
  percent label left 11 px primary, reset text right 11 px secondary, detail
  left 11 px primary, detail right 11 px secondary.
- Status pill per spec 15 section 4.5, 11 px 500 weight, 9999 px radius,
  6 px horizontal padding, 3 px vertical, click opens
  `provider.statusPageURL` in default browser.
- Card hover background fade 80 ms in, 120 ms out.
- Card press, scale 0.985 revert in 120 ms.

Commit: `feat(popup): render provider card with header, metrics, and status pill`

#### D6, `UsageProgressBar` component, 6 px height, animated fill on update

Files:
- `src/popup/components/UsageProgressBar.tsx` (new)
- `src/popup/components/UsageProgressBar.module.css` (new)

Acceptance:
- 6 px height, 3 px corner radius, full width of parent.
- Track `var(--bar-track)`, fill provider brand color from
  `provider-brand-colors.json`.
- On data update, `transform: scaleX` tween over 200 ms ease out per spec 15
  section 5.1.
- First paint after popup open skipped (no tween), gated by a
  `useFirstPaint` hook.
- Quota warning markers at 50 percent and 20 percent remaining, 2 px wide,
  hidden when `prefs.display.hideQuotaWarningMarkers` is true.
- Pace tip stripe per spec 15 section 5.2 when `pacePercent` is set, three
  vertical stripes 2 px wide, tip width `max(25, height * 6.5) = 25 px`,
  center stripe green on reserve, red on deficit, white on highlight.
- Highlighted card flips track and fill to white per spec 15 section 5.1.

Commit: `feat(popup): render UsageProgressBar with pace tip and quota markers`

#### D7, `PaceText` component, exact strings per spec 15 section 6

Files:
- `src/popup/components/PaceText.tsx` (new)
- `src/popup/format/pace.ts` (new)
- `src/popup/format/pace.test.ts` (new)

Acceptance:
- Left label, "On pace" when on track, "{N}% in deficit" ahead,
  "{N}% in reserve" behind, where `N = round(abs(deltaPercent))`.
- Right label, "Lasts until reset", "Runs out now", "Runs out in {dur}",
  optional `" * ~{R}% run out risk"` rounded to nearest 5.
- Hides below 3 percent elapsed.
- Hides when `remainingPercent <= 0`.
- Unit tests cover 20 combinations.

Commit: `feat(popup): compose pace text per spec rules with unit tests`

#### D8, `ResetCountdown` component, countdown and absolute styles

Files:
- `src/popup/format/reset.ts` (new)
- `src/popup/format/reset.test.ts` (new)
- `src/popup/components/ResetCountdown.tsx` (new)

Acceptance:
- Countdown, "Resets now", "Resets in {d}d {h}h", "Resets in {d}d",
  "Resets in {h}h {m}m", "Resets in {h}h", "Resets in {m}m" per spec 15
  section 7.1.
- Absolute, "Resets HH:MM", "Resets tomorrow, HH:MM", "Resets MMM d, HH:MM"
  per spec 15 section 7.2, locale aware via `Intl.DateTimeFormat`.
- Minimum minute granularity, no seconds, `max(1, ceil(seconds / 60))`.
- Updated string, "Updated just now" first 60 s, then "Updated 3m ago"
  abbreviated, then "Updated HH:MM" after 24 h, per spec 15 section 7.4.

Commit: `feat(popup): render reset countdown with countdown and absolute styles`

#### D9, `ClickToCopyOverlay` component plus copied chip animation

Files:
- `src/popup/components/ClickToCopyOverlay.tsx` (new)
- `src/popup/components/CopiedChip.tsx` (new)

Acceptance:
- Click on wrapped element copies `copyText` via `clipboard-manager` plugin.
- Chip appears 120 ms opacity fade plus 4 px slide up, holds 1000 ms, fades
  out 200 ms.
- Mouse cursor changes to pointer on hover.
- Copy icon button in error subtitle, 18 by 18 hit target, swaps to
  checkmark for 900 ms then fades 200 ms.

Commit: `feat(popup): add click to copy overlay with copied chip animation`

#### D10, `PopupFooter`, Refresh now plus last refreshed, Preferences, Quit

Files:
- `src/popup/footer/PopupFooter.tsx` (new)
- `src/popup/footer/ActionRow.tsx` (new)

Acceptance:
- Refresh row 28 px height, 12 px inner padding, 18 px icon column, 8 px
  icon to title gap, 13 px title, 11 px subtitle "Last refreshed 14m ago".
- Refresh icon spins at 1.2 s linear during refresh.
- After failure, title "Refresh failed", subtitle in red with short error,
  row stays enabled.
- Preferences row, accelerator badge "Ctrl+," 11 px secondary with 0.02em
  letter spacing.
- Quit row, accelerator badge "Ctrl+Q".

Commit: `feat(popup): add footer with Refresh, Preferences, Quit and accelerators`

#### D11, `Charts` scaffolding, uPlot, cost, credits, breakdown, plan utilization

Files:
- `src/popup/charts/CostHistoryChart.tsx` (new)
- `src/popup/charts/CreditsHistoryChart.tsx` (new)
- `src/popup/charts/UsageBreakdownChart.tsx` (new)
- `src/popup/charts/PlanUtilizationChart.tsx` (new)
- `src/popup/charts/ChartCard.tsx` (new common skeleton)
- `package.json` (add `uplot ^1.6`)

Acceptance:
- Common skeleton per spec 15 section 11.6, 130 px chart canvas, two detail
  lines, optional legend, optional footer.
- Cost history, brand color bar plus peak overlay `#FFD60A` at top 5 percent
  of peak bar, breakdown rows with 2 px accent strip and opacity ramp
  `[0.75, 0.6, 0.45, 0.3]`.
- Credits history, single bar color `#49A3B0`, footer
  "Total (30d): {N} credits".
- Usage breakdown, stacked bars per service, service palette per spec 15
  section 11.3, legend grid `min 110 px` adaptive.
- Plan utilization, 6 px bar width, segmented picker when multi series,
  synthetic points at 0.45 opacity, empty state at fixed 146 px.
- Hover bands, `rgba(255,255,255,0.10)` dark, `rgba(0,0,0,0.08)` light.
- All charts populated with mock fixtures from
  `src/popup/mock/chartFixtures.ts`.

Commit: `feat(popup): scaffold cost, credits, breakdown, plan utilization charts`

#### D12, `FirstRunToast` component, tray pin hint

Files:
- `src/popup/firstRun/FirstRunToast.tsx` (new)
- `src-tauri/src/firstRun.rs` (new, persists `trayPinnedHintShown`)

Acceptance:
- On first launch, 3 s after tray registers, `Shell_NotifyIcon` `NIF_INFO`
  balloon with text:
  "CodexBar4Windows lives in the tray. To pin it, open the overflow flyout
  and drag the icon next to the volume icon."
- Title "Pin CodexBar4Windows for one click access".
- `uTimeout` 12000 (clamped to 10 by Win11).
- Flag persisted to `%APPDATA%\CodexBar4Windows\state.json`.
- Settings has "Show tray hint again" button to clear the flag.

Commit: `feat(firstRun): show one time tray pin hint with persisted flag`

### Group E, accessibility, polish, manual QA (commits 40 to 44)

#### E1, Keyboard navigation, arrow keys, Tab, Enter, Esc

Files:
- `src/popup/a11y/useKeyboardNav.ts` (new)
- `src/popup/cards/CardStack.tsx` (extend)
- `src/popup/header/ProviderSwitcherButtons.tsx` (extend)

Acceptance:
- Arrow left and arrow right switch tabs.
- Arrow up and arrow down move focus across cards and rows.
- Tab moves to footer actions.
- Enter activates the focused control (opens dashboard URL on card click).
- Esc dismisses popup.
- Initial focus on the active switcher tab or, if no switcher, the first
  card.

Commit: `feat(a11y): add keyboard navigation across tabs, cards, and footer`

#### E2, Focus rings, 2 px accent outline at 4 px offset

Files:
- `src/styles/focus.css` (new)

Acceptance:
- All focusable elements show a 2 px outline using `var(--accent)` with
  4 px offset (popup chrome) or 0 px offset (cards).
- Outline never clipped by overflow hidden ancestors.

Commit: `feat(a11y): add accent colored focus rings on all focusable elements`

#### E3, Reduced motion fallback

Files:
- `src/styles/reduced-motion.css` (new)
- `src/popup/hooks/useReducedMotion.ts` (new)

Acceptance:
- `@media (prefers-reduced-motion: reduce)` overrides all transitions to
  `duration: 1ms` and `transform: none`.
- Bar tween skipped, snap to new value.
- Open and close animation replaced with instant fade.
- Pace stripe color flash skipped.
- Reset celebration tray morph still plays but at single frame (no
  intermediate animation).

Commit: `feat(a11y): honor prefers reduced motion across popup and tray`

#### E4, Manual QA harness, golden screenshots, fixture cycling

Files:
- `src-tauri/src/dev/fixtures.rs` (new, gated behind `dev` cargo feature)
- `scripts/screenshot.ps1` (new, Windows screenshot capture)
- `docs/windows/plan/phase-3-qa-checklist.md` (new)

Acceptance:
- `cargo run --features dev` cycles fixtures every 8 s, hitting all states
  (normal, loading, stale, error, reset celebration, quota flash).
- `screenshot.ps1` captures the tray icon at 100, 125, 150, 200, 300 percent
  DPI and the popup in light and dark.
- Goldens stored under `tests/golden/phase-3/*.png` with SSIM check (Rust
  side) at least 0.99.
- Checklist file walks a human through every acceptance row from spec 10
  section 13 and spec 15 section 17.

Commit: `test(qa): add fixture cycling, screenshot script, and golden checklist`

#### E5, Performance budget enforcement, criterion benches, runtime guard

Files:
- `crates/codexbar-core/benches/render.rs` (new)
- `src-tauri/src/perf.rs` (new, runtime sampler)

Acceptance:
- Criterion bench, `render_tray_icon` cold path under 2.0 ms on reference
  hardware (i5 Surface Laptop or equivalent), cache hit under 0.05 ms,
  loading frame under 1.5 ms.
- Runtime sampler logs render time once per second, warns if above budget
  for 5 consecutive samples.
- `cargo bench -p codexbar-core` runs in CI and uploads results as an
  artifact.

Commit: `perf(core): bench renderer paths and add runtime budget sampler`

## Phase acceptance tests

Three test gates, all must pass before Phase 3 can be marked done on the
roadmap.

### Test gate 1, visual parity

Mac side captures, the harness opens the Mac popup at the same scale and the
Mac tray icon at 200 percent backing scale, and saves PNGs into
`tests/golden/mac/*.png`. The Windows harness captures equivalents into
`tests/golden/win/*.png`. SSIM between the two is at least 0.99 for the tray
icon master (36 by 36), and at least 0.97 for the popup card body. Plus or
minus one pixel on the right edge of the bar fill is allowed (OS rounding).

Cases captured for tray icon parity:
- Codex face, primary 80 percent, secondary 40 percent, theme dark.
- Claude crab, primary 12 percent, secondary 60 percent, theme light.
- Gemini sparkle, primary 50 percent, weekly null, theme dark.
- Factory asterisk, primary 30 percent, secondary 30 percent, stale true.
- Warp tilted eyes, loading pulse at phase `pi / 2`, theme dark.
- Brand mode, Codex, percent 42, theme dark, size 32.
- Three bar layout, credits ratio 80 percent, primary null.

Cases captured for popup parity:
- Single provider card, Claude, normal data.
- Two providers, merged mode, switcher visible, Overview tab.
- Codex card with credits bar and cost section.
- Error state card with copy button.
- Loading card, "Refreshing..." subtitle.
- Light theme everything.

### Test gate 2, performance budgets

Must all hold on a Surface Laptop 5 i5, AC power, balanced power plan.

| Metric | Budget |
|---|---|
| Cold tray render | 2.0 ms |
| Cache hit tray render | 0.05 ms |
| Loading frame, all overlays | 1.5 ms |
| Cache hit rate, steady state | 99 percent |
| Cache hit rate, 1 percent tick | 80 percent |
| `Shell_NotifyIcon` calls per second | 30 |
| CPU during 30 s loading | 0.5 percent of one core |
| Idle CPU | 0.05 percent of one core |
| Idle resident memory | 70 MB |
| Click to popup visible | 100 ms |
| Tab switch layout cost | 16 ms (one frame at 60 Hz) |
| Bar update | 16 ms, transform only, no reflow |
| Cold start to tray visible | 800 ms |

### Test gate 3, polish checklist

Twenty rows, all checked off by a human running the dev build. Source
material is spec 10 section 13 and spec 15 section 17.

- [ ] Popup opens within 100 ms perceived, animation completes within 180 ms.
- [ ] Popup corner radius is exactly 12 px on Win11.
- [ ] Mica visible on Win11, Acrylic on Win10.
- [ ] Shadow visible on light, near invisible on dark.
- [ ] Provider name renders at 14 px 600 weight in Segoe UI Variable.
- [ ] Email middle truncation works, "verylo...@example.com".
- [ ] Bars are exactly 6 px tall with semicircle ends.
- [ ] Quota warning markers at 50 percent and 20 percent remaining.
- [ ] Pace tip green on reserve, red on deficit, white on highlight.
- [ ] No bar animates on first paint after open.
- [ ] On data update, bar tweens 200 ms ease out.
- [ ] "Resets in 3h 12m" format renders correctly.
- [ ] "On pace" shown when on track.
- [ ] Switcher cross fades in 120 ms.
- [ ] 4 providers stacks the switcher with icons.
- [ ] 15 providers forces 4 rows.
- [ ] Light theme switcher has subtle dark wash.
- [ ] Right click menu styled by Win11 theme with Fluent icons.
- [ ] Tray icon swaps within one frame on theme change.
- [ ] Tray icon swaps within one frame on DPI change.
- [ ] First run toast appears once, then never again.
- [ ] Esc dismisses popup.
- [ ] `Ctrl+Q` quits, `Ctrl+,` opens preferences, `Ctrl+R` refreshes.
- [ ] All focusable elements have 2 px accent focus ring.
- [ ] `prefers-reduced-motion` disables transitions.

## CI gates

GitHub Actions pipeline gates this phase. On every push to `main` and on
every pull request that touches `src/`, `src-tauri/`, or `crates/`, the
following must pass.

| Job | Tool | Failure condition |
|---|---|---|
| `cargo fmt` | rustfmt | Any diff |
| `cargo clippy -- -D warnings` | clippy | Any warning |
| `cargo test --workspace` | cargo test | Any test failure |
| `cargo bench --no-run` | criterion | Build failure (bench artifacts uploaded on `main` only) |
| `pnpm lint` | eslint | Any error |
| `pnpm typecheck` | tsc no emit | Any error |
| `pnpm test` | vitest | Any test failure |
| Golden image diff | Custom Rust runner, SSIM check | SSIM under 0.99 for tray, 0.97 for popup |
| Bundle size budget | `bundlesize` config | Popup JS bundle over 320 KB gzipped |
| Tauri build | `tauri build --target x86_64-pc-windows-msvc` | Any failure |
| Manifest check | Custom script | `tauri.conf.json` missing `dpiAware = PerMonitorV2` |
| Identifier check | Custom script | `identifier` not `com.codexbar4windows.app` |
| No em dash check | `grep` (custom rule) | Em dash (`U+2014`) anywhere in source or docs |

The em dash check matches the project house style. Conventional commit format
is enforced by `commitlint` on every push.

## Risks

Eleven risks, scored low (L), medium (M), high (H). Mitigations are listed.

| Risk | Score | Mitigation |
|---|---|---|
| Tauri `tray-icon` crate cannot keep up with 30 Hz `NIM_MODIFY` updates without flicker | M | Skip `set_icon` calls when `Arc<Icon>` pointer matches previous, coalesce tooltip and icon updates into a single tick, throttle to 30 fps in the timer (matches Win11 coalescing). |
| `tiny-skia` ellipse and star paths do not match `Core Graphics` curves pixel exactly | M | Cap parity goal at SSIM 0.99, not pixel exact, accept plus or minus one pixel on right edge of fill. Document any visible divergence in a "known geometry deltas" section. |
| Mica is unavailable on Windows 10 22H2, Acrylic also varies | M | Implement explicit feature detection via `IsWindows11OrGreater` + `RtlGetVersion`, fall back gracefully to flat surface with system accent highlight. |
| `WM_DPICHANGED` fires twice during multi monitor drag | L | Debounce atlas rebuild 16 ms, the second invalidation re uses the cached ICO. |
| Right click menu on tray hidden in overflow flyout has positioning quirks | M | Use `TrackPopupMenuEx` with cursor coords, anchor to the actual icon rect from `Shell_NotifyIconGetRect` when icon is pinned. |
| WebView2 transparent window has a known black flash on first paint | M | Pre warm the WebView during app boot, hide popup window off screen until first frame is painted, then animate in. |
| `Segoe UI Variable` not present on Win10, font weight ramp differs visually | L | Fallback to `Segoe UI` declared first in the stack, snapshot tests run on both fonts. |
| 30 s continuous loading ceiling may cut off legitimate slow refreshes | L | Stop animation but keep refresh task alive, the next state update from the IPC bus re triggers animation. |
| First run balloon is ignored by users with Focus Assist on | L | Show the message inside the popup as a one time banner on first popup open as well. |
| Charts library `uPlot` is canvas based and does not auto handle high DPI WebView2 | M | Use `device-pixel-ratio` to set canvas size, redraw on DPR change, validated by the 200 percent DPI golden screenshots. |
| Mock fixture state machine drifts from real provider semantics, masking Phase 4 bugs | M | Tag every fixture with the provider field it simulates, document the cycle in `crates/codexbar-core/src/mock.rs` header, audit during Phase 4 kick off. |

## Time estimate

Estimates are working days assuming one senior engineer focused on this
phase. Add 30 percent if pairing or context switching, add 50 percent if the
engineer is new to Tauri 2 or `tiny-skia`.

| Group | Tasks | Days |
|---|---|---|
| A, Rust core renderer | 12 | 11 |
| B, Tauri tray host | 8 | 6 |
| C, Popup window | 7 | 5 |
| D, React popup content | 12 | 13 |
| E, a11y, polish, QA | 5 | 5 |
| Visual parity loop, screenshot harness, golden image tuning | included in E4 | 3 |
| Buffer for unexpected (Mica quirks, theme edge cases, integration) | 4 |

Total, **47 working days**, roughly **9.5 calendar weeks** at one engineer
full time. With a second engineer pairing on Group A and Group D in parallel,
the path can compress to about **6 calendar weeks**.

Milestone checkpoints:

- End of week 2, Group A commits 1 through 8 merged, renderer draws all
  twists and bars correctly into PNGs (no tray host yet).
- End of week 4, Group A and Group B complete, tray icon visible and
  animating on the real Windows taskbar with mock data, right click menu
  works.
- End of week 6, Group C complete, popup opens against tray rect with Mica
  backdrop, empty shell present.
- End of week 8, Group D commits 1 through 8 merged, switcher and cards
  populated with mock data, bars tween cleanly.
- End of week 9, Group D charts plus first run hint plus a11y plus QA,
  polish checklist walked, all green.

## Open questions

Eleven questions, owners assigned. Resolve before declaring Phase 3 done, or
explicitly accept the deferral into Phase 4 or Phase 5.

1. Brand icon mode below 32 px on Windows tray, drop the percent or keep it
   smaller. Spec 10 open question 3 leans MVP option (b) drop title, target
   option (a) bake. Decision needed before A12 lands. Owner, design.
2. Multi monitor tray icon mirror behavior on Win11, does the single tray
   icon appear on secondary taskbars or only primary. Spec 10 open question
   1. Test required during B4 review. Owner, platform.
3. Animation pause on `WTS_SESSION_LOCK`, are we comfortable suspending
   personality blinks on lock screen. Spec 10 open question 4. Recommend
   yes, confirm with product. Owner, product.
4. Provider SVG audit, some SVGs use `currentColor` and tint cleanly, some
   are pre tinted full color. The seven Phase 1 v1 providers must each be
   tagged. Owner, design plus engineering.
5. Reset celebration sound, do we play
   `ms-winsoundevent:Notification.Default` per spec 80 section 4. Risk of
   annoyance, default off, expose toggle. Owner, product.
6. Tooltip line breaks on Win11 22H2, latest insider builds collapse
   `\r\n` in some locales. Need to confirm against current shipping build.
   Owner, platform.
7. `muda` menu styling on Win11 dark mode, the default is fine but the
   accelerator badge font weight differs from system menus. Decide on
   acceptable. Owner, design.
8. WebView2 version pinning, do we bundle Fixed Version or ride Evergreen.
   Phase 0 likely set this, confirm before Phase 3 ships. Owner, build.
9. Accent color follows system, no per app override in this phase. Confirm
   with product. Owner, product.
10. Pace text deficit color flash, 600 ms red tint on first word per spec
    15 section 6.5, is the polish worth the implementation cost. Recommend
    ship in D7, easy to remove later. Owner, design.
11. Charts library final pick, spec 15 section 11.7 recommends `uPlot` but
    Recharts has cleaner composition. Decision driven by bundle size,
    320 KB cap. Owner, engineering.

---

## Appendix A, file tree after Phase 3

New files added by this phase, grouped by area.

```
src-tauri/  app.manifest, build.rs, tauri.conf.json (extended)
src-tauri/src/  accent.rs, animation.rs, firstRun.rs, menu.rs, perf.rs,
                popup.rs, position.rs, theme.rs, tooltip.rs, tray.rs,
                dev/fixtures.rs, commands.rs (extended), events.rs (extended),
                main.rs (extended)

crates/codexbar-core/assets/  provider-icons/*.svg,
                              provider-brand-colors.json
crates/codexbar-core/src/renderer/  mod.rs, canvas.rs, pixel_grid.rs, bars.rs,
                                    layout.rs, cache.rs, atlas.rs, brand.rs,
                                    morph.rs, patterns.rs, state.rs,
                                    status_overlay.rs
crates/codexbar-core/src/renderer/twists/  mod.rs, codex.rs, claude.rs,
                                           gemini.rs, factory.rs, warp.rs
crates/codexbar-core/benches/  render.rs
crates/codexbar-core/tests/  atlas.rs, bars_geometry.rs, layout_selection.rs,
                             morph.rs, patterns.rs, fixtures/*.png

src/popup/  index.tsx, PopupShell.tsx, animations.css, usePopupWidth.ts
src/popup/state/  usageStore.ts
src/popup/hooks/  useReducedMotion.ts
src/popup/header/  PopupHeader.tsx, ProviderSwitcherButtons.tsx,
                   SwitcherTab.tsx, WeeklyIndicator.tsx, SettingsCog.tsx
src/popup/cards/  CardStack.tsx, CardHeader.tsx, ProviderCard.tsx,
                  MetricRow.tsx, StatusPill.tsx
src/popup/components/  UsageProgressBar.tsx (+ .module.css), PaceText.tsx,
                       ResetCountdown.tsx, ClickToCopyOverlay.tsx,
                       CopiedChip.tsx
src/popup/footer/  PopupFooter.tsx, ActionRow.tsx
src/popup/charts/  ChartCard.tsx, CostHistoryChart.tsx,
                   CreditsHistoryChart.tsx, UsageBreakdownChart.tsx,
                   PlanUtilizationChart.tsx
src/popup/firstRun/  FirstRunToast.tsx
src/popup/format/  pace.ts (+ .test.ts), reset.ts (+ .test.ts)
src/popup/mock/  chartFixtures.ts
src/popup/a11y/  useKeyboardNav.ts

src/styles/  tokens.css, themes.css, typography.css, focus.css,
             reduced-motion.css

scripts/  screenshot.ps1
tests/golden/phase-3/  tray and popup goldens, one per parity case
docs/windows/plan/  phase-3-tray-popup.md, phase-3-qa-checklist.md
```

## Appendix B, conventional commit prefixes used

| Prefix | Meaning | Example task |
|---|---|---|
| `feat(core)` | New behavior in `codexbar-core` crate | A1 through A12 |
| `feat(tauri)` | New behavior in the Tauri host | B1 through B8 |
| `feat(popup)` | New React popup behavior | C1 through D12 |
| `feat(styles)` | New CSS tokens or themes | C5, C6 |
| `feat(a11y)` | Accessibility additions | E1, E2, E3 |
| `feat(firstRun)` | First run experience | D12 |
| `build(core)` | Build, dependency, manifest changes for the core | A1 |
| `build(tauri)` | Build, manifest changes for Tauri | B1 |
| `perf(core)` | Performance benches and runtime guards | E5 |
| `test(qa)` | Test harness and goldens | E4 |
| `fix(*)` | Bug fix, scoped to subsystem | reserved |
| `docs(*)` | Documentation changes | reserved |
| `chore(*)` | Refactor or cleanup | reserved |

Every commit message body should reference the task ID, for example
`A11, build ICO atlas, ...`, so the plan and the commit history stay in
lockstep.

## Appendix C, what we explicitly defer to Phase 4

- Real Claude provider implementation, HTTP, OAuth, cookies, refresh, error
  handling beyond mock fixture shapes.
- Real Codex, Cursor, Copilot, Gemini, OpenRouter, Factory providers,
  Phase 4 covers Claude only, the remaining six follow in Phase 5 and 6.
- Preferences window content. The right click menu opens a placeholder
  window in Phase 3. Phase 5 fills in the seven preferences panes.
- Auto update plumbing, signing, SmartScreen guidance. Phase 6.
- WSL detection banner, Chromium V20 warning, browser closed required
  banner. Phase 4 lands Claude detection, the banners ship alongside.
- Per provider tray icons. Spec 10 section 14, item 13, explicitly says
  Windows convention is one tray icon per app, we keep that constraint.

End of plan.
