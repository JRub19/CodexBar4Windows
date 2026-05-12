---
title: "Tray Icon System — Windows Spec"
status: "Authoring (sourced from macOS CodexBar reference implementation)"
audience: "Rust/TS engineer implementing CodexBar on Windows (Tauri 2 + React + shared Rust crate). No Swift background required."
mac_sources:
  - "Sources/CodexBar/IconRenderer.swift"
  - "Sources/CodexBar/StatusItemController.swift"
  - "Sources/CodexBar/StatusItemController+Animation.swift"
  - "Sources/CodexBar/StatusItemController+MenuPresentation.swift"
  - "Sources/CodexBar/MenuBarDisplayMode.swift"
  - "Sources/CodexBar/MenuBarDisplayText.swift"
  - "Sources/CodexBar/MenuBarMetricWindowResolver.swift"
  - "Sources/CodexBar/LoadingPattern.swift"
  - "Sources/CodexBar/DisplayLink.swift"
  - "Sources/CodexBar/MenuHighlightStyle.swift"
  - "Sources/CodexBar/IconRemainingResolver.swift"
  - "Sources/CodexBar/ProviderBrandIcon.swift"
  - "Sources/CodexBar/UsageStoreSupport.swift"
  - "Sources/CodexBar/Resources/ProviderIcon-*.svg"
---

# 10 — Tray Icon System

This document is the implementation spec for the **Windows tray-icon subsystem**. The goal is not to reproduce
Swift; it is to reproduce the **polish**. Phantom-wallet and Duolingo set the bar — every pixel snap, every
easing curve, every hover state matters. Where the macOS renderer encodes a "magic number," we surface it
verbatim. Where there is room for tasteful adaptation to Windows conventions, we say so explicitly.

The Mac app draws the tray icon procedurally (vector → bitmap, never raster assets) on a tiny **18 pt ×
18 pt** template canvas, with sub-pixel snapping and an LRU cache. The Windows port keeps the **same logical
canvas** but rasterises into a multi-resolution ICO at each common DPI scale, refreshing the icon via
`Shell_NotifyIcon` (`NIM_MODIFY` with `NIF_ICON | NIF_TIP`). A shared **Rust core crate** owns the renderer
(via `tiny-skia` / `resvg`), so the same code feeds (a) Win32 tray and (b) the menu-card SwiftUI-equivalent
React surface.

---

## 0. Glossary

| Term                | Meaning |
|---------------------|---------|
| **Critter / Meter** | The Mac-rendered procedural icon — two horizontal capsule bars (primary + secondary) with provider-specific "personality" overlays (Codex face, Claude crab, Gemini sparkle, etc.). |
| **Brand mode**      | When `menuBarShowsBrandIconWithPercent = true`, the icon becomes the provider SVG logo and a percent label appears as a tray title. Critter overlays are bypassed. |
| **Merge-icons mode**| `settings.mergeIcons && enabledProvidersForDisplay().count > 1` → one tray icon represents the highest-priority/highest-usage provider; clicking opens a unified menu that switches between providers. |
| **Loading pattern** | Procedural animation drawn into the bars while waiting for a refresh. Six patterns: `knightRider`, `cylon`, `outsideIn`, `race`, `pulse`, `unbraid`. |
| **Stale**           | `UsageStore.isStale(provider:)` returns `true` when the most recent refresh produced an error. The icon dims (alpha ↓). |
| **Status indicator**| 5-level provider health (`minor`, `major`, `critical`, `maintenance`, `unknown`) overlaid as a small dot or dot-with-line in the bottom-right corner. |
| **Phase**           | Loading-animation drive value in radians; incremented by `2.7 / fps` per tick. |

---

## 1. Canvas + Render Targets

### 1.1 Mac source canvas (verbatim)

| Property                | Value                              | Source |
|-------------------------|------------------------------------|--------|
| Logical canvas          | 18 × 18 pt                         | `baseSize` |
| Output canvas           | 18 × 18 pt                         | `outputSize` |
| Output backing scale    | 2× (Retina)                        | `outputScale` |
| Canvas px (drawing buffer) | 36 × 36 px                      | `canvasPx` |
| Stroke width            | 1 pt → 2 px @2×                    | `strokeWidthPx = 2` |
| Pixel-grid snap         | All rect coords are integer-px at 2× scale (PixelGrid.snapDelta) | `IconRenderer.swift:24` |
| Interpolation quality   | `.none` (nearest-neighbor for blocky shapes) | `withScaledContext` |
| Anti-alias              | OFF by default; turned ON for star/asterisk/ellipse eyes and for Gemini/Factory/Warp overlays | `setShouldAntialias(true)` per overlay |
| Template flag           | `NSImage.isTemplate = true` (system tints from menu bar appearance) | `renderImage` |
| Image scaling on button | `imageScaling = .scaleNone` (no resampling, crisp edges) | `StatusItemController.swift:264` |

> **Single source of truth:** The Mac canvas is **18×18 in *points*** rendered to a **36×36 bitmap**.
> Drawing math is performed in **px space** (`PixelGrid`) and converted back to pt. Everything aligns to integer
> px at 2×.

### 1.2 Windows output targets

Windows tray icons must ship as a multi-size ICO (or as multiple `HICON` resources). Microsoft's tray uses
`SM_CXSMICON` / `SM_CYSMICON` plus DPI scaling — the runtime selects the best match.

| Scale | Logical px (system DPI) | Required size | Notes |
|------:|------------------------:|--------------:|-------|
| 100%  | 16                      | 16 × 16       | Smallest Win11 tray bucket; details collapse — see §1.4 |
| 125%  | 20                      | 20 × 20       | Default Surface / many laptops |
| 150%  | 24                      | 24 × 24       | High-DPI ultraportables |
| 175%  | 28                      | 28 × 28       | Optional but recommended; falls back to 32 if missing |
| 200%  | 32                      | 32 × 32       | **Primary high-DPI target** (matches Mac 36 px buffer closely) |
| 225%  | 36                      | 36 × 36       | Direct match to Mac buffer |
| 250%  | 40                      | 40 × 40       | |
| 300%  | 48                      | 48 × 48       | |
| 400%  | 64                      | 64 × 64       | Some 8K monitors |

**Pack ICO with all sizes ≥ 16 and ≤ 64.** The icon is regenerated **per DPI change** (per-monitor DPI v2)
without restarting the process.

### 1.3 Logical coordinate system (used by all sizes)

The renderer reasons in a **logical 18×18 pt canvas** (origin bottom-left, +y up — mirror Mac convention).
Every size on Windows is produced by:

1. Render to an offscreen 36×36 `tiny_skia::Pixmap` (the "Mac-parity buffer").
2. Down/upsample to each target ICO size with **Lanczos3** for sizes ≥ 32 and **nearest-neighbor** for 16/20
   (preserves the blocky critter eyes).
3. For 16 px, also reduce stroke width from 2 → 1 (since 2 logical px no longer maps to an integer pixel) and
   drop secondary overlays (legs, gear teeth, sparkles) that would collapse to a single pixel.

### 1.4 Smallest-bucket (16 px) graceful degradation

At 16 px the Mac critter would smear. The Windows renderer follows this collapse table:

| Element                 | ≥ 32 px        | 24 px         | 20 px         | 16 px         |
|-------------------------|----------------|---------------|---------------|---------------|
| Two bars (capsule)      | full           | full          | full, 1 px stroke | full, 1 px stroke, smaller corner radius |
| Claude legs/arms        | yes            | yes           | arms only     | hidden        |
| Codex hat               | yes            | yes           | hidden        | hidden        |
| Gemini sparkle points   | yes            | reduced (no side points) | top/bottom only | hidden        |
| Factory gear teeth      | yes            | reduced       | hidden        | hidden        |
| Antigravity dot         | yes            | yes           | yes           | yes (1 px)    |
| Warp tilted-ellipse eyes| yes            | yes           | rectangles    | rectangles    |
| Status indicator overlay| 4 px dot       | 3 px dot      | 2 px dot      | 2 px dot      |

### 1.5 Output pipeline (Rust)

```rust
fn render_tray_icon(state: IconState, dpi: u32) -> Vec<RgbaImage> {
    let base = render_to_pixmap(state, 36); // master 36×36
    [16, 20, 24, 28, 32, 36, 40, 48, 64]
        .iter()
        .map(|&size| resample(&base, size, ResampleHint::for_size(size)))
        .collect()
}
```

Constructing a Windows ICO from these RGBA images and applying it via the
`tray-icon` crate's `TrayIcon::set_icon` (which wraps `Shell_NotifyIcon` `NIM_MODIFY`) gives the OS the
correct asset for the current DPI.

### 1.6 Mac-side template-image semantics → Windows mapping

| Mac (NSImage)            | Windows mapping |
|--------------------------|-----------------|
| `image.isTemplate = true`<br/>System auto-tints monochrome based on menu-bar appearance (light/dark/blurred wallpaper). | Tray themes are static at icon paint. We must detect taskbar theme via registry (`HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\SystemUsesLightTheme`) **and** listen to `WM_SETTINGCHANGE` with `ImmersiveColorSet`. Re-render with the appropriate fill color. See §6. |
| Status overlay rendered with `NSColor.labelColor` | Always rendered with the "fill" color resolved for the current taskbar theme. |
| `image.isTemplate = false` on quota-warning flash | Windows port draws explicit ARGB pixels with the system red (#FF3B30 @ alpha-22%) — no template path required. |

---

## 2. Visual Styles

The Mac renderer exposes a single entry point, `IconRenderer.makeIcon(...)`, that draws one of **three layouts**
based on the data shape, then layers a provider-specific "twist" on top of the **primary** bar. A fourth mode
(**brand**) bypasses procedural drawing entirely.

### 2.1 Layout selection (algorithm)

```text
Input:  topValue (primaryRemaining), bottomValue (weeklyRemaining), creditsRatio
Output: layout ∈ { TwoBarNormal, TwoBarDimmed, TwoBarCreditsOnly, CreditsThick+Bottom }

if weeklyAvailable (bottomValue != nil && bottomValue > 0):
    layout = TwoBarNormal       (top=primary, bottom=secondary, both active)
elif !hasWeekly OR (style==warp && bonus==0):
    if style == warp && bonusExhausted:
        layout = TwoBarDimmed   (top=monthly w/ warp twist, bottom=dimmed-track alpha 0.45)
    elif topValue == nil && creditsRatio != nil:
        layout = TwoBarCreditsOnly (CreditsThickBar at top, dimmed-track bottom)
    else:
        layout = TwoBarDimmed   (top=primary, bottom=dimmed track alpha 0.45)
else:  # weekly exhausted/missing, credits present
    layout = CreditsThick+Bottom (CreditsThickBar at top, bottom=bottomValue)
```

`creditsRatio = min(creditsRemaining / 1000, 1) * 100` — the **credits cap is 1000** (`creditsCap`).

### 2.2 Bar geometry (all units in **px @ 2×**, derived from PixelGrid)

| Rect            | Used in              | x  | y  | w  | h  | Notes |
|-----------------|----------------------|----|----|----|----|-------|
| `topRectPx`     | TwoBarNormal/Dimmed primary | 3  | 19 | 30 | 12 | Top bar — fat |
| `bottomRectPx`  | TwoBarNormal/Dimmed secondary | 3  | 5  | 30 | 8  | Bottom bar — thinner |
| `creditsRectPx` | CreditsThickBar      | 3  | 14 | 30 | 16 | Single fat bar in 3-bar variant |
| `creditsBottomRectPx` | Companion bottom for credits | 3 | 4 | 30 | 6 | Thin bottom companion |
| `barXPx`        | All                  | `(36 - 30) / 2 = 3` | — | — | — | Center horizontally |
| `barWidthPx`    | All                  | 30 (15 pt) | — | — | — | "Uses the slot better without touching edges." |

Vertical spacing between top and bottom bars: `19 - (5+8) = 6` px (3 pt).

### 2.3 Per-bar drawing rule

For each bar:

1. **Track** (always drawn unless overridden) — rounded-rect filled at `labelColor` × `trackFillAlpha`.
2. **Stroke** — inset by `1` px (= half stroke width), `labelColor` × `trackStrokeAlpha`, stroked.
3. **Fill** — clip to capsule, paint a left-anchored rectangle whose width = `round(barWidthPx × remaining/100)`.
   The right edge is a **hard straight line** (intentional — looks like a battery, not a pill).

### 2.4 Corner radii

| Twist     | Radius                  | Source |
|-----------|-------------------------|--------|
| Claude (`addNotches`) | **0 px** (sharp blocky critter) | `cornerRadiusPx = 0` |
| Warp (`addWarpTwist`) | **3 px** (rounded-square logo style) | `cornerRadiusPx = 3` |
| Default   | `h / 2` (pill / capsule) | else branch |

> Stroke path uses `cornerRadius - insetPx` so the stroke stays *inside* the visual radius. Critical for crisp
> edges at 2×; do the same at every Windows DPI.

### 2.5 Alpha values (memorise these)

| State                                | trackFillAlpha | trackStrokeAlpha | fillColorAlpha |
|--------------------------------------|---------------:|-----------------:|---------------:|
| Normal                               | 0.28           | 0.44             | 1.00           |
| Stale (most recent refresh failed)   | 0.18           | 0.28             | 0.55           |
| Dimmed bottom track ("N/A" lane)     | 0.28 × 0.45    | 0.44 × 0.45      | n/a (no fill)  |
| Per-overlay alpha multiplier         | passes through | passes through   | passes through |

(Each non-default-track bar accepts an `alpha` parameter that scales **track fill, stroke, and value fill**
uniformly — see Warp dimmed bottom and CreditsThickBar dimmed companion.)

### 2.6 Provider "twists" (overlay catalog)

| Style key      | IconStyle case   | Twist on primary bar                            | Special blink target |
|----------------|------------------|--------------------------------------------------|----------------------|
| codex          | `.codex`         | **Face** — 2 eye cutouts (4×4 px each, offset ±7 px from center), 18×4 px hat above. | Eyes refill from top |
| claude         | `.claude`        | **Crab critter** — `addNotches` → 0-radius rect, 3×(h−6) arms protruding left/right, 4 legs (2×3 px) below bar, two **vertical** eye cutouts (2×5 px, offset ±6 px). | Vertical eyes refill |
| gemini, antigravity | `.gemini`, `.antigravity` | **Sparkle** — 8-pointed star cutouts (radius 4 px, inner 1 px), top+bottom triangular crown points (4×4 px), left+right triangle accents (3×3 px). | Star eyes refill |
| antigravity (extra) | `.antigravity` | Plus a **3 px circular dot** at top-right corner of the bar. | n/a |
| factory        | `.factory`       | **Gear / asterisk** — 16-pointed asterisk eye cutouts (radius 3.5 px, inner 1 px), 2 gear-teeth rects (3×2 px) on top edge, 2 on bottom edge. | Asterisk eyes refill |
| warp           | `.warp`          | **Tilted-ellipse eyes** — 5×8 px ellipses rotated ±60° (= ±π/3 rad), drawn either as clear cutouts or filled (`warpEyesFilled = true`) during loading. Corner radius = 3 px. | "Filled" eyes pulse via `sin(phase·3)` |
| All other styles | many             | Plain pill bars; no overlay. | Standard blink (handled at root) |

#### 2.6.1 Codex face details
- Eye size: **4 × 4 px** (cutouts via `ctx.clear`, anti-alias OFF for crisp pixel-edges).
- Eye horizontal offset: **±7 px** from bar center.
- Hat: **18 × 4 px** rect at `y = barTop - 4`, drawn in fill color.
- Hat tilts during the "tilt" motion (see §4) — rotates around face center, anti-aliased.
- Blink: eyes refill **from top down** at amount `blinkHeightPx = round(eyeSize * clamp(blink, 0..1))`.

#### 2.6.2 Claude crab details
- Arms: 3 px wide × `(barHeight - 6)` tall, x = `barX - 3` (left) / `barX + barW` (right).
- Legs: 4 legs, each 2 × 3 px, evenly distributed across `barW / 5` (step = `barW / (legCount+1)`).
- Wiggle offset: `wiggleOffset = snap(wiggle * 0.6)`, then `wigglePx = round(wiggleOffset * 2)`. Arms +
  `wigglePx/6`, eyes + `wigglePx/8` — subtle, sub-pixel-friendly motion.

#### 2.6.3 Gemini sparkle details
- 8-point star: 8 vertices alternating outer-radius `sr = 4` and inner-radius `innerR = sr * 0.25 = 1`.
- Vertex angle: `i * π/4 − π/2` (start at top).
- Crown points: top + bottom triangles, base 4 px, height 4 px.
- Side accents: 3×3 px triangles.

#### 2.6.4 Factory asterisk details
- 16-point asterisk: 16 vertices alternating outer 3.5 px / inner 1.05 px. Angle: `i * π/8 − π/2`.
- Gear teeth: 2 teeth on each of the top and bottom edges, offset ±5 px from center, 3 × 2 px each.

#### 2.6.5 Warp tilted-eye details
- Eye rect: width 5 px, height 8 px, drawn around origin with `CGContext` rotation:
  `translate(cx,cy) → rotate(±π/3) → addEllipse(rect)`.
- The right eye tilts in the **opposite direction** from the left eye (mirror).
- Anti-alias ON for ellipses.
- During Warp **loading**, eyes are drawn **filled** (not cutout) and pulse via `(sin(phase*3) + 1) / 2`.

### 2.7 Brand mode (`menuBarShowsBrandIconWithPercent`)

When enabled:

1. Procedural rendering is bypassed. The icon = provider SVG resampled to **16 × 16 pt** (mac `ProviderBrandIcon`).
2. `NSStatusItem.button.title` is set to the percent label; the brand image sits to its left.
3. The percent label is computed by `MenuBarDisplayText.displayText(mode:percentWindow:pace:showUsed:)`:
   - `mode == .percent` → `"42%"` (rounded, clamped 0–100)
   - `mode == .pace` → `"+12%"` or `"-9%"` from `UsagePace.deltaPercent`
   - `mode == .both` → `"42% · +12%"`, with mid-dot separator `·`
4. Special-cases per provider (override the percent label):
   - **OpenRouter** + automatic preference + `balance` available → USD string (`UsageFormatter.usdString`).
   - **DeepSeek** → balance from `resetDescription` (`$X.XX` or `¥X.XX`).
   - **Moonshot** → `"Balance: …"` parsed from `loginMethod`, dropped at `·` separator.
   - **Mistral** → `"API spend: … this month"` → `"$0.12"`.
   - **KimiK2** → `"Credits: … left"` → `"123"`.

### 2.8 Merge-icons mode

Activated when `settings.mergeIcons == true` && `enabledProvidersForDisplay().count > 1`.

- A single tray icon represents the **primary** provider chosen by `primaryProviderForUnifiedIcon()`:
  1. If `menuBarShowsHighestUsage`: provider with highest "used %" via `providerWithHighestUsage()`.
  2. Else if last-selected merged tab was the Overview tab: the first provider in the resolved Overview list.
  3. Else if a previously selected merged provider exists and is still enabled: that one.
  4. Else: first enabled provider with a snapshot. Otherwise first enabled. Otherwise `.codex` fallback.
- Each (non-merged) provider otherwise gets its **own** tray icon. The Mac stores them in
  `statusItems: [UsageProvider: NSStatusItem]` and reuses them across enable/disable so the OS preserves
  positions. Windows port keeps **one** tray icon always (Win11 hides them by default — see §10) but exposes
  per-provider tooltips via the unified menu.

### 2.9 Status overlay (incident indicator)

Layered **last** in the canvas; lives **inside** the 18×18 pt bounding box (no bleed).

| Indicator | Glyph                                  | Position (origin, pt) | Size |
|-----------|----------------------------------------|----------------------:|------|
| `none`    | Hidden                                 | —                     | —    |
| `minor`, `maintenance` | Filled circle              | `(w-6, 2)`            | 4 × 4 |
| `major`, `critical`, `unknown` | Line + dot ("!") | line `(w-6, 4)` 2×6 rounded(1) + dot `(w-6, 2)` 2×2 oval | composite |

Color: `labelColor` (theme-aware on Mac; on Windows = the resolved fill color). The overlay does **not** dim
when `stale = true` — incident state is more important than freshness.

---

## 3. State Overlays

### 3.1 Stale (refresh failed)

Triggered when `UsageStore.isStale(provider:)` returns true (the provider's last fetch errored). The renderer
takes a single boolean parameter `stale: Bool` which switches **two** alpha pairs in lockstep:

| Layer                  | Normal | Stale |
|------------------------|-------:|------:|
| Track fill alpha       | 0.28   | 0.18  |
| Track stroke alpha     | 0.44   | 0.28  |
| Fill color alpha       | 1.00   | 0.55  |
| Status overlay alpha   | 1.00   | 1.00 (unchanged) |

Stale state suppresses **all** loading animation (`shouldAnimate` returns false when `isStale && hasData`).
It does **not** suppress blink/wiggle/tilt — those continue at dimmed alpha for personality.

### 3.2 Loading animation

See §4. While loading, `stale` is **forced to false** (we never dim during animation), the layout is locked to
`TwoBarNormal` (or `MorphLayout` for the `unbraid` pattern) by using `loadingPercentEpsilon = 0.0001` so the
weekly value never hits 0 (which would flip layouts).

### 3.3 Quota warning flash (60 s after a quota warning)

When a quota warning is posted (`Notification.codexbarQuotaWarningDidPost`), the icon flashes red for
**60 s** (`StatusItemController.quotaWarningFlashDuration`).

Implementation: `quotaWarningFlashImage(base:)` composites:

| Layer       | Color                    | Geometry |
|-------------|--------------------------|----------|
| Background  | `systemRed @ alpha 0.22` | rounded rect (corner 4 pt), inset 1 pt from image |
| Base icon   | The base NSImage         | drawn at full opacity |
| Top wash    | `systemRed @ alpha 0.28` | full-image rect       |
| Template    | `isTemplate = false`     | colors are explicit ARGB |

> Implementation note: the flash image is **not** a template — it commits the red as actual pixels rather than
> relying on system tinting. Windows port draws the same composite explicitly.

### 3.4 Unbraid / Reset celebration morph

The `unbraid` loading pattern is also used as a **reset celebration** morph: three rotating "ribbons"
unfurl into the two-bar critter as `progress: 0 → 1`. Cached in `MorphCache`.

| Property                | Value      |
|-------------------------|------------|
| Cache size              | 512 entries |
| Cache key buckets       | 200 (progress quantised to 0/200..200/200) |
| Cache key formula       | `styleKey * 1000 + bucket` |
| Morph entry point       | `IconRenderer.makeMorphIcon(progress: Double, style: IconStyle)` |
| Cross-fade-in threshold | `t > 0.55` → final bar icon faded in over `(t-0.55)/0.45` |
| Skipped for             | `.combined` icon style (uses `.cylon` fallback when unbraid would be chosen) |

Ribbon segments (3 total, units = logical pt, origin = center 9,9):

| Segment | startCenter | endCenter | startAngle° | endAngle° | startLen | endLen | startThick | endThick | fadeOut |
|---------|-------------|-----------|------------:|----------:|--------:|------:|-----------:|---------:|---------|
| Upper   | (9, 11)     | (9, 9)    | −30         | 0         | 16      | 14    | 3.4        | 3.0      | no      |
| Lower   | (9, 7)      | (9, 4)    | 210         | 0         | 16      | 12    | 3.4        | 2.4      | no      |
| Side    | (9, 9)      | (9, 15)   | 90          | 0         | 16      | 8     | 3.4        | 1.8      | **yes** (alpha = 1 − p) |

For the fade-out segment, `p = t * 1.1` (slightly accelerated so it disappears before t = 1.0).

---

## 4. Animation System

### 4.1 Loading driver

| Property                                  | Value | Source |
|-------------------------------------------|------:|--------|
| Frame rate                                | **30 FPS** | `loadingAnimationFPS` |
| Phase increment per tick                  | `2.7 / 30 = 0.09 rad` | `loadingAnimationPhaseIncrement` |
| Continuous-duration ceiling               | **30 s** | `loadingAnimationMaxContinuousDuration` |
| Driver type (Mac 15+)                     | `CADisplayLink` via `NSScreen.displayLink` | `DisplayLink.swift:29` |
| Driver type (Mac 14)                      | `CVDisplayLink` (callback hops to main actor) | fallback |
| Windows replacement                       | High-resolution timer (`SetTimer` or `CreateThreadpoolTimer`) at 33 ms, debounced against `WM_PAINT` so the tray re-icon happens at most every frame. Pause when window is hidden / system in low-power mode. | — |

> The 30 s ceiling exists to guarantee that a hung provider can never keep the menu bar redrawing forever
> (battery & CPU concern — see Mac issues #269, #139). Windows port enforces the same cap; on timeout it
> stops the timer, sets the icon to the last static state, and resumes only when new data arrives.

### 4.2 Loading patterns

`phase` advances from 0; each pattern maps phase → `value ∈ [0, 100]` for the **primary** bar. The
**secondary** bar is driven by `value(phase + secondaryOffset)`.

| Pattern       | Formula (primary)                                       | secondaryOffset | Visual feel |
|---------------|---------------------------------------------------------|-----------------|-------------|
| `knightRider` | `0.5 + 0.5*sin(phase)` → ping-pong                      | π               | Smooth back-and-forth, primary & secondary anti-phase |
| `cylon`       | `((phase mod 2π)/2π)` → sawtooth 0→1                    | π/2             | Linear sweep, secondary 90° ahead |
| `outsideIn`   | `abs(cos(phase))` → high at edges, dip center           | π               | Wave-out / wave-in |
| `race`        | `((phase*1.2 mod 2π)/2π)` → sawtooth, faster            | π/3             | Two-lane race |
| `pulse`       | `0.4 + 0.6*(0.5 + 0.5*sin(phase))` → range 40–100 %     | π/2             | Breathing |
| `unbraid`     | `0.5 + 0.5*sin(phase)` (drives morph alpha)             | π/2             | Logo → bars morph (uses `makeMorphIcon`) |

All values are clamped to `[0, 100]`.

### 4.3 Idle "personality" motion (blink / wiggle / tilt)

Runs only when **not** loading. Random per-provider effect.

| Property                        | Value                          | Source |
|---------------------------------|--------------------------------|--------|
| Random blink enabled            | `settings.randomBlinkEnabled`  | gate |
| Per-blink duration              | **0.36 s**                     | `blinkDuration` |
| Double-blink chance             | **18 %** (blink only)          | `doubleBlinkChance` |
| Double-blink delay              | random in **[0.22, 0.34] s**   | `doubleDelayRange` |
| Inter-blink delay               | random in **[3, 12] s**        | `BlinkState.randomDelay()` |
| Active tick interval (during blink) | **75 ms**                  | `blinkActiveTickInterval` |
| Idle fallback tick              | **1.0 s**                      | `blinkIdleFallbackInterval` |
| Force-blink hold                | **0.6 s** after debug trigger  | `blinkForceUntil` |
| Quota-warning flash duration    | **60 s**                       | `quotaWarningFlashDuration` |

#### 4.3.1 Easing curve for blink amount

Symmetric triangle, then power 2.2:

```text
elapsed = now - blinkStart                      ∈ [0, 0.36]
progress = clamp(elapsed / 0.36, 0..1)
symmetric = progress < 0.5 ? progress*2 : (1-progress)*2
amount = symmetric ^ 2.2     # "slightly punchier than smoothstep"
```

The exponent `2.2` is intentional (a smoothstep / cubic would feel too sleepy). Windows port must use the
same curve to preserve the eye-blink character.

#### 4.3.2 Effect dispatch per provider

```text
randomEffect(provider) =
  if provider == .claude: 50/50 { .blink, .wiggle }
  else:                   50/50 { .blink, .tilt }
```

- **`.blink`** — drives `blinkAmounts[provider]`, used by face/critter/star/asterisk overlays to refill eyes.
- **`.wiggle`** — drives `wiggleAmounts[provider]`, Claude crab uses it to nudge arms/legs/eyes by `wigglePx/6, wigglePx/8`.
- **`.tilt`** — drives `tiltAmounts[provider]`, Codex hat rotates by `tiltAmount * π / 28 ≈ ±6.4° max`. The
  hat translation is `-abs(tilt) * 1.2` pt so it doesn't slide off the head.

Only one motion effect is active at a time — assigning one zeros the others.

#### 4.3.3 Warp loading pulse

When the active provider is Warp **and** it's loading, the blink amount is overridden by:

```text
blink = clamp((sin(phase * 3) + 1) / 2, 0..1)
```

This drives the **filled-eye pulse** (eyes turn on/off three times per `2π` of phase, ~9 Hz at 30 FPS).

### 4.4 Combined-icon style suppresses overlays

When `iconStyle == .combined` (merged icon mode), the renderer **ignores** `blink/wiggle/tilt` — the merged
icon has no personality (it's a neutral two-bar meter). Source: lines 299-301 of
`StatusItemController+Animation.swift`.

### 4.5 When does animation start / stop?

Animation is **on** when any displayed provider returns `true` from `shouldAnimate(provider:)`:

```text
shouldAnimate(provider) =
  debugForceAnimation                              -> true
  not visible (or merged-not-enabled)              -> false
  fallback-only (no real enablement)               -> false
  warp + no data + currently refreshing            -> true
  no snapshot && not stale                         -> true   (initial fetch in flight)
  otherwise                                        -> false
```

Why "no data && not stale": stale providers stay static (they already errored — don't burn CPU re-spinning).

### 4.6 Coordination between loading & blink

- If **loading is active**, the blink task is **stopped** and `blinkAmounts` are cleared (would otherwise
  overwrite the loading frame on its 75 ms tick and cause flicker).
- The loading driver calls `applyIcon(phase:)` on each tick; the blink ticker uses a separate `Task` loop with
  `Task.sleep`.

---

## 5. Caching

Two caches; both thread-safe.

### 5.1 Static icon LRU (`IconCacheStore`)

| Property         | Value |
|------------------|------:|
| Capacity         | **64** entries (`iconCacheLimit`) |
| Eviction         | LRU (touched on get, moved-to-back on put, oldest dropped at overflow) |
| Skipped when     | Any of `blink ≥ 0.0001`, `wiggle ≥ 0.0001`, `tilt ≥ 0.0001` (motion is animated, not cached) |
| Lock             | `NSLock` (Rust port → `parking_lot::Mutex<LruCache<…>>`) |

Cache key (`IconCacheKey`):

```text
struct IconCacheKey {
    primary:   Int   // quantizedPercent(value): -1 if nil, else round(value * 10)   → 1000 buckets
    weekly:    Int   // quantizedPercent
    credits:   Int   // quantizedCredits: -1 if nil, else round(clamp(0..1000) * 10) → 10000 buckets
    stale:     Bool
    style:     Int   // index into IconStyle.allCases
    indicator: Int   // 0..5 per ProviderStatusIndicator
}
```

The percent quantisation to **0.1 % buckets** means a 100% → 0% transition produces ≤ 1000 cache entries,
well within the 64-entry cap (LRU evicts cold ones).

### 5.2 Morph cache

| Property         | Value |
|------------------|------:|
| Capacity         | **512** entries (`MorphCache.limit`, backed by `NSCache`) |
| Eviction         | OS-managed (mac `NSCache`); Windows port uses `lru::LruCache::new(512)` |
| Bucket count     | **200** progress steps |
| Bucket math      | `bucket = round(progress * 200)`, then key = `styleKey * 1000 + bucket` |

### 5.3 Merge-icon render-skip signature

Independent of pixel cache: a string signature captures all inputs that affect the final image (mode, provider,
style, percents, stale, indicator, blink, text). If the signature matches the last applied, the renderer
returns early (`shouldSkipMergedIconRender`). Reduces `Shell_NotifyIcon` traffic.

Signature shape (`"mode=icon|provider=codex|style=combined|primary=0.420|…"`) is documented in
`StatusItemController+Animation.swift` lines 318-371. Port verbatim; this is the cheapest correctness check we
have against unnecessary redraws.

### 5.4 Cache hit-rate expectations

Steady state (no motion, no loading): **>99 %**. After a settings change or a percentage tick: brief miss
spike, then warm again within ~10 ticks.

---

## 6. Color & Theming

### 6.1 Mac (template image)

| Element             | Mac source            | Effective color (light menu bar / dark menu bar) |
|---------------------|-----------------------|---------------------------------------------------|
| Bar fill            | `NSColor.labelColor` × alpha | Black @ 1.0 / White @ 1.0 (system inverts) |
| Bar track fill      | `labelColor × 0.28`   | Black @ 0.28 / White @ 0.28 |
| Bar stroke          | `labelColor × 0.44`   | Black @ 0.44 / White @ 0.44 |
| Stale fill          | `labelColor × 0.55`   | Black @ 0.55 / White @ 0.55 |
| Status overlay      | `labelColor`          | Black @ 1.0 / White @ 1.0 |
| Quota warning wash  | `systemRed @ 0.22, 0.28` | system-red, explicit ARGB (not template) |
| Brand mode title    | `labelColor` (system menu-bar text) | Black / White |

### 6.2 Windows theming

There is no Win32 "template" path; we must render colors explicitly. The taskbar can be light or dark,
independent of app theme, and either side can change mid-session.

| Resolved color | Light taskbar          | Dark taskbar           |
|----------------|------------------------|------------------------|
| `fill`         | `#1F1F1F` (`labelColor` mac equivalent at 1.0) | `#F2F2F2` |
| `trackFill`    | `fill @ 0.28`          | `fill @ 0.28`          |
| `trackStroke`  | `fill @ 0.44`          | `fill @ 0.44`          |
| `staleFill`    | `fill @ 0.55`          | `fill @ 0.55`          |
| `staleTrackFill`   | `fill @ 0.18`      | `fill @ 0.18`          |
| `staleTrackStroke` | `fill @ 0.28`      | `fill @ 0.28`          |
| `quotaWashTop` | `#FF3B30 @ 0.22`       | `#FF453A @ 0.22`       |
| `quotaWashBottom` | `#FF3B30 @ 0.28`    | `#FF453A @ 0.28`       |
| Status overlay | `fill` (1.0)           | `fill` (1.0)           |

### 6.3 Theme detection on Windows

1. Read `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\SystemUsesLightTheme`
   (DWORD 0 = dark, 1 = light) at boot.
2. Listen to `WM_SETTINGCHANGE` with `lParam` string `"ImmersiveColorSet"` → re-read and rebuild the icon
   atlas. Also re-render on `WM_DPICHANGED`.
3. Listen to `WM_THEMECHANGED` for older Win10 builds. Same handler.

> **Do not** assume the app's `apps_theme` matches the taskbar — Win11 lets the taskbar follow a separate
> setting (`SystemUsesLightTheme`) from window chrome (`AppsUseLightTheme`). The **tray uses taskbar
> theme**, not app theme.

### 6.4 Provider brand colors

The procedural icon is monochrome — brand colors only show up when the user enables **brand mode**. The SVGs
in `Sources/CodexBar/Resources/ProviderIcon-*.svg` are pre-tinted; they render at 16 × 16 pt and are marked
`isTemplate = true` on Mac. On Windows we render the SVG with `resvg`, then re-tint to the resolved `fill`
color to preserve theme parity. Where a provider's brand is **inherently colored** (e.g. Gemini gradient,
Claude red), keep the SVG color; the override applies only for SVGs that were authored with `currentColor`.

---

## 7. Provider-specific Behavior

### 7.1 Bar window resolution (`IconRemainingResolver.resolvedPercents`)

For most providers: `primary = snapshot.primary.remainingPercent`, `secondary = snapshot.secondary.remainingPercent`.

| Provider style    | primary lane resolution                                   | secondary lane resolution      |
|-------------------|-----------------------------------------------------------|--------------------------------|
| `.perplexity`     | `snapshot.orderedPerplexityDisplayWindows()[0]`            | `[1]`                          |
| `.antigravity`    | First of [primary, secondary, tertiary] that is non-nil    | Second of same list            |
| `.codex`          | First of `codexConsumerProjection.visibleRateLanes`        | Second of same                 |
| default           | `snapshot.primary`                                         | `snapshot.secondary`           |

### 7.2 `showUsed` flip

Setting: `usageBarsShowUsed`. When **true**, the renderer is fed `usedPercent` instead of `remainingPercent`,
so the bar fills **left → right with usage** (battery-charging metaphor) rather than draining.

Special-case for Warp:
- Bonus exhausted (`remaining ≤ 0`) — still treat as the "no bonus" layout (top=monthly + dimmed bottom).
- Bonus active and `used == 0` — promote to `loadingPercentEpsilon` so the layout stays in "TwoBarNormal."

### 7.3 When credits become the third bar

Only for Codex when:
- `codexConsumerProjection.menuBarFallback == .creditsBalance`, **and**
- Credits are loaded and positive.

The fallback `.creditsBalance` happens when:
- No 5h session window AND no weekly window have meaningful data (e.g. user purchased credits but hasn't used
  any plan minutes yet).

When triggered, `creditsRemaining = store.codexMenuBarCreditsRemaining(...)` (in dollars), the renderer
computes `creditsRatio = min(creds / 1000, 1) * 100`, and layouts:
- **TwoBarCreditsOnly** if there is no primary value at all → CreditsThickBar on top, dimmed empty companion below.
- **CreditsThick+Bottom** if weekly is exhausted but credits are present → CreditsThickBar on top, weekly (=0) below.

### 7.4 Multi-account stacking

When a provider has multiple token accounts (Claude, Codex), each gets its own snapshot but they all feed
**the same tray icon** (not multiple icons). The tray reflects the **active** account; the menu's switcher
shows all accounts stacked.

On Windows the same model applies — one tray icon per provider (or one merged tray icon in merged mode); the
account switcher lives in the popup menu (see spec `11-menu-system.md`).

---

## 8. Mac → Windows Mapping Table

| Mac concept                                                | Windows equivalent                                                                                                  |
|------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------|
| `NSStatusItem.button.image`                                | `tray_icon::TrayIcon::set_icon(Some(Icon::from_rgba(...)?))` → `Shell_NotifyIcon(NIM_MODIFY)` with `NIF_ICON`       |
| `NSStatusItem.button.title`                                | Tray tooltip carries the percent text (no native "title" in tray); the merged-menu header label shows it.           |
| `NSStatusItem.button.toolTip`                              | `Shell_NotifyIconW.szTip` — **128 char limit** (or `szInfoTip` 256 chars). Multi-line via `\r\n`.                   |
| `image.isTemplate = true`                                  | Render the icon with the **resolved theme color** explicitly. Re-render on `WM_SETTINGCHANGE/ImmersiveColorSet`.    |
| `NSScreen.displayLink` / `CVDisplayLink`                   | `CreateThreadpoolTimer` at 33 ms (30 FPS). On `WM_POWERBROADCAST/PBT_APMSUSPEND` pause; resume on `PBT_APMRESUMEAUTOMATIC`. |
| `NSImage.lockFocus` + `NSBezierPath`                       | `tiny_skia::Pixmap` + `tiny_skia::Path` (or `skia-safe` if higher quality glyphs are needed).                       |
| `setShouldAntialias(false)` / `interpolationQuality = .none` | `tiny_skia::PixmapPaint.quality = FilterQuality::Nearest` and use rect/path APIs without AA where needed.         |
| `LSUIElement = true` (no Dock)                              | Tauri config `bundle.windows.app_kind = "Tool"` and disable taskbar entry on the main window.                       |
| `NSStatusBar.statusItem(withLength: variableLength)`        | Single `TrayIconBuilder`. Width is auto-sized by Windows.                                                            |
| `NSMenu` attached to status item                            | Use a Tauri WebView popup (see `11-menu-system.md`) for rich content; fall back to a native `MENUPOPUP` for the right-click context menu hook. |
| `NSColor.labelColor`                                       | Resolved per theme — see §6.2.                                                                                       |
| `Notification.codexbarQuotaWarningDidPost`                  | Internal pub-sub event (`event::emit("quota-warning", ...)`).                                                       |
| Per-provider `NSStatusItem` with autosave name             | **Windows ports do not support multiple tray icons** in the same way (each adds clutter, hidden by default). Always merge into one tray icon on Windows. |

---

## 9. Click & Hover Behavior at the Icon Level

### 9.1 Tooltip

Windows tray icons surface a tooltip (`szTip`, 128 chars) on hover. Format (line breaks = `\r\n`):

```
CodexBar
{Primary provider name}: {percent} {pace?}
{Secondary provider name}: {percent} {pace?}
...
↑ Updated {relative time}
```

Examples (single-provider, merged mode off):

```
CodexBar
Claude: 42% session · -9%
Resets at 5:00 PM
```

Examples (merged mode):

```
CodexBar — Codex (highest usage)
Codex: 91% used · +14%
Claude: 42% used · -9%
Updated 2 min ago
```

Edge cases:
- **Stale** → append `\r\n⚠ Last refresh failed` (no error detail — that's in the menu).
- **Loading** → omit per-provider lines; show `Refreshing…`.
- **Status incident** → first line becomes `CodexBar — {indicator label}` (e.g., "Partial outage").
- Truncate to 127 chars + `…` if exceeded; the menu always has the full data.

### 9.2 Click semantics

| Action                                | Windows behavior                                                            | Mac source |
|---------------------------------------|------------------------------------------------------------------------------|------------|
| Left-click                            | Open the unified menu popup (Tauri WebView). Same shortcut as `Cmd+B` on Mac → `Ctrl+B` global hotkey toggles it. | `openMenuFromShortcut` |
| Right-click                           | Open the native Win32 context menu (Preferences, Refresh, About, Quit, Open Logs, Quit). Mirrors Mac right-click. | menu attached to status item |
| Double-click                          | **No double-click handler.** Mac doesn't have one; Windows users may expect "open last-used view." Reserve for future; ignore by default. | — |
| `Shift + click`                       | Force-refresh active provider (debug shortcut). Mirrors Mac `Shift+click → reload`. | option in debug menu |
| `Ctrl + click`                        | Open Preferences directly. | — |
| `Alt + click`                         | Cycle the loading-pattern animation (debug only when `settings.debugLoadingPattern == nil`). | `handleDebugReplayNotification` |
| Hover (no click)                      | Tooltip after Windows default delay (~500 ms). No "preview popup" — Tauri popup only opens on click. | — |

### 9.3 Animation/idle interplay

- Hovering the tray does **not** pause idle animations. Personality blinks continue.
- Clicking the tray opens the menu and **does not** halt the loading animation. The menu reads from the same
  store and the tray keeps animating until data lands.

---

## 10. First-run / Overflow Story

Windows 11 hides new tray icons in the **overflow flyout** by default. Phantom-wallet level polish demands we
fix this **once**, gracefully.

### 10.1 First-run detection

State file: `%APPDATA%\CodexBar\state.json` → `"trayPinnedHintShown": bool` (default false).

On first run after install (or first run on a profile where the hint hasn't been shown), wait 3 seconds after
the tray icon registers, then show a one-time **info bubble** anchored to the tray icon:

```
Pin CodexBar for one-click access
Drag the icon out of the chevron menu and onto your taskbar.
                                              [Got it]
```

Use `Shell_NotifyIcon(NIM_MODIFY)` with `NIF_INFO`:
- `dwInfoFlags = NIIF_USER` + custom icon (use the 32 px critter).
- `uTimeout` = 12 s (Windows ignores values < 10 s on Win11; balloon disappears automatically).
- Persist `trayPinnedHintShown = true` after presenting once. Never re-show.

### 10.2 Detection of "still hidden after N days"

Optional (post-MVP): poll `Shell_NotifyIcon(NIM_QUERY_ICON_OVERFLOW_STATE)` — not standardized, but the registry
key `HKCU\Control Panel\NotifyIconSettings\<hash>\IsPromoted` reveals state. If still demoted after 7 days,
show the hint once more with copy `"You can drag CodexBar onto the taskbar — just click the chevron, then
drag the gauge icon out."` Then **never again**.

### 10.3 Power-user override

In **Settings → Display**, expose `"Show tray hint again"` button so the user can replay the bubble.

---

## 11. DPI Handling

### 11.1 Mac (current behavior)

- Mac status bar is always ~22 pt tall; the icon is 18 × 18 pt regardless of screen.
- Retina = `outputScale = 2`, hence the 36 × 36 px backing buffer.
- Pro Display XDR / 3× screens: AppKit auto-upscales the template image — sufficient because the renderer is
  vector under the hood. No code path for 3× rendering exists.

### 11.2 Windows (per-monitor DPI v2)

1. Declare per-monitor DPI awareness v2 in the app manifest (`<dpiAwareness>PerMonitorV2</dpiAwareness>`).
2. On startup, query `GetDpiForWindow(taskbarHwnd)`; pick the matching ICO size.
3. Subscribe to `WM_DPICHANGED` — re-render the icon atlas, call `NIM_MODIFY`.
4. Multi-monitor: the taskbar's DPI is the **per-display** DPI of the monitor hosting that taskbar (each
   monitor's taskbar can have a different DPI on Win11). When the system tray moves between monitors (rare),
   we will get a `WM_DPICHANGED` for our hidden tray host window — re-render then.
5. **Always re-render at every DPI change**, do not interpolate from cache (caches index by `IconCacheKey`
   which does **not** include DPI — they live behind the renderer at the logical-canvas layer).

### 11.3 Test matrix (mandatory)

| Display setup                       | Expected behavior |
|-------------------------------------|-------------------|
| 100% (16 px target)                 | All collapse rules from §1.4 apply; no smearing. |
| 125% / 150% (20 / 24 px)            | Full critter visible; stroke remains 2 logical px (= 2 or 3 actual px depending on scale). |
| 200% (32 px)                        | Mac parity — pixel-perfect critter. |
| 250% / 300%                         | Lanczos resample of the 36 px master is acceptable. |
| Dual-monitor 100% + 200%            | Different ICO sizes per monitor as user drags. |
| DPI change mid-session              | Icon swaps within one frame; no flicker. |

---

## 12. Performance Budget

| Metric                                          | Target |
|-------------------------------------------------|-------:|
| Tray render call (cold, fully procedural)       | ≤ 2.0 ms on a Surface Laptop 5 (i5) |
| Tray render call (cache hit)                    | ≤ 0.05 ms |
| Loading frame (cache miss, all overlays)        | ≤ 1.5 ms |
| Cache hit-rate, steady state                    | ≥ 99 % |
| Cache hit-rate, 1 % usage change (single tick)  | ≥ 80 % |
| `Shell_NotifyIcon(NIM_MODIFY)` calls per second | ≤ 30 (matches FPS cap) |
| CPU usage during 30 s loading                   | ≤ 0.5 % of one core |
| CPU usage idle (no animation)                   | ≤ 0.05 % of one core |
| Memory (icon caches combined)                   | ≤ 5 MB (64 static + 512 morph @ ~16 KB max each) |

### 12.1 CPU-saving heuristics (mirror Mac)

- Stop animation when **all** providers are stale (no point spinning if data is dead). Mac: `shouldAnimate`
  returns false for stale providers; port this rule verbatim.
- Stop animation when the system reports low-power mode (`SYSTEM_POWER_STATUS.SystemStatusFlag & 1`) — pause
  to 5 FPS to keep visual feedback without burning battery.
- Re-render only when something *changes*: the `lastAppliedMergedIconRenderSignature` short-circuit (§5.3) cuts
  ~30 % of redundant calls in merged mode.
- Coalesce settings-change cascades: a single settings update can touch (a) visibility, (b) icon assignment,
  (c) menu refresh. Batch into one render at the end of the run-loop tick (`Task.yield` on Mac → use a
  `pending: AtomicBool` + `WM_APP+1` self-post on Windows).

---

## 13. Acceptance Checklist

The Windows implementation passes when **all** of these hold.

### 13.1 Geometry & rendering

- [ ] At 200 % DPI, the icon is pixel-identical to a screenshot of the Mac status bar icon at the same percent
  value and provider style, allowing for OS-specific rounding (±1 px on the right edge of the fill).
- [ ] All seven loading patterns are present and visually distinct.
- [ ] Bar coordinates match the rect table in §2.2 exactly.
- [ ] Stroke is exactly 2 logical px wide; the inset path keeps the stroke inside the visual radius.
- [ ] Codex hat is centered horizontally on the face; tilts when the tilt motion is active; eyes do not tilt.
- [ ] Claude crab arms reach the canvas edge without clipping (3 px arms × bar at x=3).
- [ ] Gemini star eyes are 4-pointed (not 8-pointed star; the 8 vertices alternate radii).
- [ ] Factory asterisk has 8 "spokes" (16 vertices alternating).
- [ ] Warp ellipses are mirrored (left tilts +60°, right tilts −60°).
- [ ] Status overlay sits inside the 18×18 bounding box and never overlaps the bar fill.

### 13.2 Animation

- [ ] Loading pattern advances at **30 FPS** exactly; under load it never exceeds 30 FPS.
- [ ] Animation **auto-terminates** after 30 s of continuous run; the last rendered frame stays as the static
  icon until new data arrives.
- [ ] Blink curve = `pow(triangle(progress), 2.2)`; visually punchier than smoothstep.
- [ ] Blink duration = 360 ms; double-blink occurs ~18 % of the time with 220-340 ms gap.
- [ ] Idle blink intervals are randomised 3–12 s per provider, independently.
- [ ] Combined-icon style suppresses blink/wiggle/tilt entirely.
- [ ] During loading, blink ticks do not flicker against the loading frame (the blink task is stopped).

### 13.3 State overlays

- [ ] Stale state dims **all** layers (track/stroke/fill) to {0.18, 0.28, 0.55} respectively.
- [ ] Stale state suppresses loading animation but **not** blink/wiggle/tilt.
- [ ] Quota warning flash lasts exactly **60 s** and clears automatically.
- [ ] Quota warning composite = red @ 0.22 inset 1px (radius 4 pt) → base → red @ 0.28 full wash.
- [ ] Status overlay color = resolved theme `fill`; not dimmed by stale.

### 13.4 Caching

- [ ] Static cache holds ≤ 64 entries; LRU evicts on overflow.
- [ ] Cache is skipped whenever blink/wiggle/tilt > 0.0001.
- [ ] Morph cache holds ≤ 512 entries keyed on style × 200 progress buckets.
- [ ] `lastAppliedMergedIconRenderSignature` short-circuits identical merged-mode renders.

### 13.5 Theming & DPI

- [ ] Light taskbar shows black-resolved fills; dark taskbar shows white-resolved fills.
- [ ] Changing taskbar theme mid-session updates the tray icon **without restart**, within one frame of the
  notification.
- [ ] DPI change (drag to different-DPI monitor) swaps to the appropriate ICO size, no flicker.
- [ ] PerMonitorV2 declared in manifest; no virtualization fallback.

### 13.6 Tooltip & click

- [ ] Tooltip honours the 128-char limit (truncate + `…`).
- [ ] Tooltip updates on every render (no stale cached tip).
- [ ] Left-click opens the menu popup; right-click opens the native context menu; `Shift+click` force-refreshes.
- [ ] Tooltip in merged mode lists all active providers with percent + pace; sorted by usage.
- [ ] Stale providers in tooltip are marked with `⚠`; loading providers say `Refreshing…`.

### 13.7 First-run

- [ ] Pin-hint balloon shows exactly once on first install, 3 s after tray registers.
- [ ] User can replay the hint from Settings → Display.
- [ ] If the user has hidden the icon, the app does not aggressively re-show itself (no flashing icon).

### 13.8 Performance

- [ ] During 30 s of continuous loading, CPU stays ≤ 0.5 % of a single core on reference hardware.
- [ ] Memory of the icon subsystem is < 5 MB.
- [ ] No measurable battery drop in 10 min of idle (compare to baseline without CodexBar running).

---

## 14. Microinteractions & Polish Notes (don't skip)

These are the "Phantom / Duolingo" touches that separate a good port from a great one.

1. **Sub-pixel snap on wiggle**: Mac uses `wigglePx = round(wiggleOffset * 2)` to keep arms on the integer-px
   grid even during motion — the arms don't smear. Port verbatim.

2. **Hat tilt translates downward by `|tilt| * 1.2`**: gives the impression the hat is "sitting" on a tilting
   head rather than levitating. Subtle but felt.

3. **Eyes drawn via `clear` blend mode** (not just painted background): when the bar fill grows past the eye
   position, the eye remains visible as a true hole through the bar — no z-fighting with the fill.

4. **Anti-alias is OFF for blocky elements** (Codex face, Claude critter) but **ON** for organic ones
   (Gemini stars, Factory asterisks, Warp ellipses, ribbon morph). This is per-overlay, not global.

5. **The morph's third ribbon fades out before the others complete** (`p = t * 1.1` vs `t`). It vanishes
   ~10 % of progress before the others, so the final state is two ribbons → two bars (no awkward third bar).

6. **Cross-fade-in starts at 55 %**, not 50 %: the bar shape is given more time to fully form before the
   final-state fill emphasis takes over. Tuning: do not move this earlier — it looks rushed.

7. **`loadingPercentEpsilon = 0.0001`** is used to prevent layout flips at exact-zero values. Without this,
   the renderer's "weekly available > 0?" branch can toggle layouts mid-animation, producing a visible
   jitter. Always feed `max(value, epsilon)`.

8. **`shouldMergeIcons` is dynamic** — it depends on `enabledProvidersForDisplay().count > 1`. As the user
   toggles providers, the icon either splits into multiple (Mac) or stays merged with a different active
   provider (Windows-by-design choice).

9. **The Warp loading pulse runs 3× faster than the carrier** (`sin(phase * 3)`): gives Warp a distinct
   "scanning" look during refresh; do not unify with other styles' blink path.

10. **Force-blink hold is 600 ms** (`blinkForceUntil = now + 0.6`) — long enough that the user *sees* the
    blink they triggered via the debug menu, but short enough not to feel sticky.

11. **The 75 ms active tick** (during a blink) is faster than the visible 30 FPS animation cap because the
    blink is **off the loading path** — it doesn't compete with the loading frame, and we want the blink
    motion to land smoothly even when the OS schedules the timer late.

12. **Cache hit short-circuit at the application layer**: even if `IconCacheKey` matches, the tray
    `set_icon` call must also be skipped (it's a system call). Mac uses `if button.image === image { return }`.
    Windows: compare `Arc<Icon>` by pointer before calling `NIM_MODIFY`.

13. **Per-provider tray-icon position is preserved on Mac** via `NSStatusItem` autosave names. Windows lacks
    this — when we merge to a single icon, the position is whatever the user pinned. **Do not** auto-create
    multiple tray icons even when the user enables individual icons in settings (clutter; Windows convention
    is one tray icon per app).

14. **Settings observer batching**: a single user toggle can fire 3–5 observer notifications (config revision,
    provider order, merge icons, switcher icons, used-vs-remaining). Mac coalesces in
    `handleSettingsChange()`; Windows port must do the same — set a `dirty` flag and process once on the next
    tick.

15. **Tooltip rebuilds on every snapshot update**, not on a timer. If nothing changed, `Shell_NotifyIcon` is
    skipped (compare tip string before calling). This avoids the Win11 tooltip flicker bug (the popup
    momentarily disappears if `NIM_MODIFY` is called with the same `szTip` while the tooltip is showing).

---

## 15. Reference Tables (quick lookup)

### 15.1 All magic numbers — one place

| Constant                                | Value      |
|-----------------------------------------|-----------:|
| Canvas (pt)                             | 18 × 18    |
| Canvas (px @ 2×)                        | 36 × 36    |
| Bar width                               | 30 px (15 pt) |
| Bar x                                   | 3 px (1.5 pt) |
| Top bar y, h                            | 19, 12 px  |
| Bottom bar y, h                         | 5, 8 px    |
| Credits thick bar y, h                  | 14, 16 px  |
| Credits thin companion y, h             | 4, 6 px    |
| Stroke width                            | 2 px (1 pt) |
| Credits cap (dollars)                   | 1000       |
| Track fill alpha                        | 0.28       |
| Track stroke alpha                      | 0.44       |
| Fill alpha (normal)                     | 1.00       |
| Stale: track fill / stroke / fill       | 0.18 / 0.28 / 0.55 |
| Dimmed-track alpha (Warp/credits empty) | 0.45       |
| Quota wash inner / outer                | 0.22 / 0.28 |
| Quota wash corner radius / inset        | 4 pt / 1 pt |
| Quota flash duration                    | 60 s       |
| Loading FPS                             | 30         |
| Phase increment per tick                | 2.7 / 30   |
| Loading max continuous duration         | 30 s       |
| Blink duration                          | 0.36 s     |
| Double-blink chance                     | 0.18       |
| Double-blink delay range                | 0.22–0.34 s |
| Inter-blink range                       | 3–12 s     |
| Blink active tick                       | 75 ms      |
| Blink idle tick                         | 1.0 s      |
| Force-blink hold                        | 0.6 s      |
| Blink easing exponent                   | 2.2        |
| Tilt range                              | π / 28 ≈ 6.4° |
| Hat tilt translate-y                    | −\|tilt\| × 1.2 pt |
| Wiggle scale                            | × 0.6      |
| Static icon cache size                  | 64         |
| Morph cache size                        | 512        |
| Morph progress buckets                  | 200        |
| Brand icon size                         | 16 × 16 pt |
| Codex eye size / offset                 | 4 px / ±7 px |
| Codex hat size                          | 18 × 4 px  |
| Claude arm size                         | 3 × (h-6) px |
| Claude leg count / size / step          | 4 / 2×3 px / barW÷5 |
| Claude eye size / offset                | 2 × 5 px / ±6 px |
| Gemini star outer / inner radius        | 4 px / 1 px (sr × 0.25) |
| Gemini eye offset                       | ±8 px      |
| Gemini crown points                     | 4 × 4 px   |
| Gemini side accents                     | 3 × 3 px   |
| Factory asterisk outer / inner          | 3.5 px / 1.05 px (sr × 0.3) |
| Factory gear teeth                      | 3 × 2 px, offset ±5 px |
| Warp eye size / offset / tilt           | 5 × 8 px / ±7 px / ±π/3 (60°) |
| Antigravity dot size / position         | 3 px / (barRight+2, barTop−2) |
| Status overlay dot (minor/maint.)       | 4 × 4 px, at (w−6, 2) |
| Status overlay line+dot (major/etc)     | line 2×6 @ (w−6, 4), dot 2×2 @ (w−6, 2) |

### 15.2 Loading patterns

| Pattern       | Primary formula                          | Secondary offset |
|---------------|------------------------------------------|------------------|
| knightRider   | 0.5 + 0.5 · sin(φ)                       | π                |
| cylon         | (φ mod 2π) / 2π                          | π/2              |
| outsideIn     | \|cos(φ)\|                               | π                |
| race          | (1.2·φ mod 2π) / 2π                      | π/3              |
| pulse         | 0.4 + 0.6 · (0.5 + 0.5 · sin(φ))         | π/2              |
| unbraid       | 0.5 + 0.5 · sin(φ)  → drives morph       | π/2              |

### 15.3 Cache keys

```text
IconCacheKey {
    primary:   round(value * 10) | -1
    weekly:    round(value * 10) | -1
    credits:   round(clamp(0..1000) * 10) | -1
    stale:     bool
    style:     IconStyle.allCases index
    indicator: 0..5 (none, minor, major, critical, maintenance, unknown)
}

MorphKey = styleKey * 1000 + round(progress * 200)
```

---

## 16. Open Questions (call these out before shipping)

1. **Per-monitor tray icon visibility on multi-monitor Win11**: Tauri's `tray-icon` crate currently creates a
   single tray icon associated with the primary taskbar. Does Win11 mirror it to secondary monitors that have
   "Show taskbar on all displays" enabled? **Test required.** If not, we may need to spawn per-monitor host
   windows.

2. **`Shell_NotifyIcon` Win11 cooldown**: rapid `NIM_MODIFY` calls (>30/s) get coalesced. This matches our
   30 FPS cap exactly, but during DPI change + theme change combos we may call `NIM_MODIFY` twice within
   33 ms — verify no visible flicker.

3. **Brand icon mode title text**: Windows tray icons don't have a "title to the right of the icon." Phantom
   would handle this by drawing the percent **into the tray ICO itself** at small sizes, sacrificing the
   critter when brand mode is on. Decision needed: (a) burn percent into the icon, (b) drop title and rely on
   tooltip + popup, or (c) ship a "side label" by hosting a thin taskbar-anchored Tauri window. **MVP: (b);
   target: (a) at sizes ≥ 32.**

4. **Animation pause on user inactivity**: macOS doesn't pause the loading animation when the user is idle —
   should Windows? Microsoft's WinUI guidelines suggest "no visible animation in tray when system is locked."
   Recommend pausing on `WTS_SESSION_LOCK` and resuming on `WTS_SESSION_UNLOCK`.

5. **Provider SVG brand icons need a Windows-tinted variant**: Some SVGs use `currentColor`, some are
   pre-tinted, some are full-color gradients. Audit the 38 SVGs in `Resources/` and tag each with the
   intended treatment (tinted vs verbatim). **Not in scope for this spec.**

---

## 17. Test Plan (high level)

| Test                                                  | How |
|-------------------------------------------------------|-----|
| Geometry parity vs Mac                                | Snapshot-test the 36×36 master pixmap at 100 % primary, 50 % weekly, no stale, codex style. Compare via SSIM ≥ 0.99 against a captured Mac screenshot. |
| All 6 loading patterns                                | Frame-grab at phase = 0, π/4, π/2 …, 2π for each pattern; visual diff vs golden. |
| Stale dim                                             | Test that exactly three alpha values are swapped. |
| Cache LRU                                             | Inject 65 distinct keys, verify oldest is evicted. |
| Quota flash auto-clear                                | Trigger flash, wait 60.5 s, assert `lastAppliedSignature` no longer contains `warningFlash=1`. |
| Blink curve                                           | Sample amount at 50 % progress → expect `1.0^2.2 = 1.0`; at 25 % → `0.5^2.2 ≈ 0.218`. |
| Theme switch mid-session                              | Programmatically flip registry value → fire `WM_SETTINGCHANGE` → assert atlas regenerated within 100 ms. |
| DPI change                                            | Synthesize `WM_DPICHANGED` → assert new size selected, no `Shell_NotifyIcon` errors. |
| Tooltip truncation                                    | Construct a tooltip > 128 chars → assert truncated + `…` and tray accepts it. |
| First-run hint shows once                             | New profile → tray registers → assert balloon appears within 3.5 s → restart → no balloon. |
