---
title: "Feel & Polish — Windows Spec"
status: "Authoring (sourced from macOS CodexBar reference implementation)"
audience: "Rust/TS engineer implementing CodexBar on Windows (Tauri 2 + React + shared Rust crate). No Swift background required."
mac_sources:
  - "Sources/CodexBar/DisplayLink.swift"
  - "Sources/CodexBar/LoadingPattern.swift"
  - "Sources/CodexBar/ScreenConfettiOverlayController.swift"
  - "Sources/CodexBar/AppNotifications.swift"
  - "Sources/CodexBar/Notifications+CodexBar.swift"
  - "Sources/CodexBar/SessionQuotaNotifications.swift"
  - "Sources/CodexBar/MenuHighlightStyle.swift"
  - "Sources/CodexBar/ClickToCopyOverlay.swift"
  - "Sources/CodexBar/StatusItemController+Animation.swift"
  - "Sources/CodexBar/StatusItemController.swift"
  - "Sources/CodexBar/IconRenderer.swift"
  - "Sources/CodexBar/MenuCardView.swift"
  - "Sources/CodexBar/UsageProgressBar.swift"
  - "Sources/CodexBar/UsagePaceText.swift"
  - "Sources/CodexBar/PreferencesView.swift"
  - "Sources/CodexBar/PreferencesAboutPane.swift"
  - "Sources/CodexBar/StorageBreakdownMenuView.swift"
  - "Sources/CodexBar/StatusItemMenu.swift"
  - "Sources/CodexBar/Resources/en.lproj/Localizable.strings"
related_specs:
  - "docs/windows/spec/10-tray-icon-system.md"
  - "docs/windows/05-windows-ux-spec.md"
  - "docs/ui.md"
  - "docs/icon.md"
---

# 80 — Feel, Animations, Notifications, Sounds, Confetti

This subsystem is **the app's personality**. CodexBar is a passive utility — users glance at it dozens of
times a day. Every microinteraction has been tuned so the app feels *alive but not pushy*: critters that
blink at unpredictable intervals, a tray icon that morphs softly when refreshing, confetti when the week
resets, a copy-affordance that flashes a checkmark and recovers.

The Phantom / Duolingo bar means: **no instantaneous state changes, no jarring color jumps, no flat
loading spinners**. Everything has an in-curve, a hold, an out-curve, a reasonable cap, and an
opt-out.

The Mac reference uses procedural icon rendering, `CADisplayLink` for animation ticks, Vortex for
confetti, `UNUserNotificationCenter` for toasts, and `NSSound` for the quota-warning ping. On Windows the
mapping is: a `tokio::time::interval`-driven Rust frame source piping events over IPC to the React popup,
`Shell_NotifyIcon` redraws for the tray, `windows-rs` `ToastNotification` for system toasts, and either a
tray-icon morph + toast for the celebration (preferred) or a tasteful canvas-confetti burst inside the
popup if it happens to be open.

---

## 0. Glossary

| Term              | Meaning |
|-------------------|---------|
| **Tick**          | A single frame on the animation driver. |
| **Phase**         | A monotonically increasing radian counter (`Double`) consumed by `LoadingPattern.value(phase:)`. |
| **Pattern**       | One of six named loading shapes (`knightRider`, `cylon`, `outsideIn`, `race`, `pulse`, `unbraid`). |
| **Morph**         | The "unbraid" pattern — icon shapeshifts from brand logo to bars. |
| **Critter**       | The little face/legs/eyes drawn on top of the bars when the user has the *Surprise me* (random blink) toggle on. |
| **Stale**         | A snapshot whose `updatedAt` is older than the staleness threshold. Icon dims to ~55% alpha. |
| **Quota flash**   | A 60-second tinted-red icon overlay that follows a quota-warning notification. |
| **Reduced motion**| Honor the OS preference. Windows: `UISettings.AnimationsEnabled` + `SPI_GETCLIENTAREAANIMATION`. |
| **DND**           | "Do not disturb." macOS Focus / Windows Focus Assist. The app silences sound but keeps visual cues. |

---

## 1. DisplayLink semantics

### What the Mac does

`DisplayLinkDriver` (Sources/CodexBar/DisplayLink.swift) is a minimal frame-tick source. On macOS 15+ it
uses `NSScreen.displayLink`; on macOS 14 it falls back to `CVDisplayLink`. Ticks land on the main thread.
Each tick increments a `tick: Int` counter and calls an optional `onTick` closure.

| Parameter                                | Value                                   |
|------------------------------------------|-----------------------------------------|
| Target FPS for tray-icon loading anim    | **30 Hz** (`loadingAnimationFPS = 30`)  |
| Phase increment per tick                 | `2.7 / 30 ≈ 0.09` rad                   |
| Effective oscillator period              | `2π / 2.7 ≈ 2.33 s` per loading cycle   |
| Hard ceiling per loading session         | **30 s** continuous, then stop          |
| Blink-active tick interval               | **75 ms**                               |
| Blink-idle fallback poll                 | **1 s**                                 |

The Mac caps the displayLink to exactly the requested FPS using `CAFrameRateRange(min, max, preferred)`.
The `step` callback drops frames that arrive faster than `1/targetInterval`. This is critical: on a 120 Hz
ProMotion display the panel will fire at 120 Hz but the icon redraw must stay at 30 Hz, both for visual
calm and for battery.

### Windows mapping

The Windows tray icon is a static bitmap; it cannot be driven by `requestAnimationFrame` directly. Drive
ticks from the Rust core:

```rust
let mut interval = tokio::time::interval(Duration::from_millis(33)); // 30 Hz
interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
loop {
    interval.tick().await;
    if !needs_animation() { break; }
    let bitmap = renderer.render_frame(phase);
    shell_notify_icon::modify(hicon_from(bitmap));
    phase += 2.7 / 30.0;
    if started_at.elapsed() > Duration::from_secs(30) { break; }
}
```

The **React popup** uses real `requestAnimationFrame` for its own UI animations (bar fills, hover, the
copy-flash, the tab indicator). It does **not** drive the tray icon — the popup may not even be open. The
Rust core is the single source of truth for tray-icon frames.

Two independent loops therefore exist:

| Loop                | Driver                          | Cadence | Lives in   |
|---------------------|---------------------------------|---------|------------|
| Tray icon animation | `tokio::time::interval` (30 Hz) | 33 ms   | Rust core  |
| Popup UI animation  | `requestAnimationFrame`         | 60+ Hz  | React WV   |
| Blink (critter)     | `tokio::time::sleep` adaptive   | 75 ms active / 1 s idle | Rust core |

The Rust core emits a `TrayFrameTick` event to the popup over IPC **only when the popup is open** so
React can mirror the morph state in the header icon. While the popup is closed, the event stream is
suppressed.

### Why 30 Hz, not 60

The icon is 16–40 px wide. At 60 Hz the difference is imperceptible and the redraw cost (a full
`Shell_NotifyIcon NIM_MODIFY` + ICO upload) is non-trivial. 30 Hz is the documented sweet spot.

---

## 2. Loading animation

### The six loading patterns

`LoadingPattern` (Sources/CodexBar/LoadingPattern.swift) defines six named ways the bars can dance while
a refresh is in flight. Each maps a `phase: Double` to a `0…100` percent value.

| Pattern        | Bar 1 expression                          | Bar 2 offset  | Feel              |
|----------------|-------------------------------------------|---------------|-------------------|
| `knightRider`  | `0.5 + 0.5 * sin(phase)` (ping-pong)      | `+π`          | Smooth ⇄         |
| `cylon`        | sawtooth `0→1`                            | `+π/2`        | Strict scan       |
| `outsideIn`    | `abs(cos(phase))` (peaks at edges)        | `+π`          | Inhale/exhale     |
| `race`         | sawtooth at 1.2× speed                    | `+π/3`        | Drag race         |
| `pulse`        | `0.4 + 0.6 * (0.5 + 0.5 * sin(phase))`    | `+π/2`        | 40–100% heartbeat |
| `unbraid`      | `0.5 + 0.5 * sin(phase)` (morph progress) | `+π/2`        | Logo → bars       |

Default is `knightRider`. When the user does not have *Surprise me* on, this is what they see. The Debug
pane (hidden by default) exposes a "Replay selected animation" notification that picks a specific pattern;
otherwise the app cycles to the next pattern when a debug shortcut fires.

### When the animation runs

The animation runs **only** while a provider has no snapshot yet AND is not stale AND refresh is in
progress. The shouldAnimate check (Sources/CodexBar/StatusItemController+Animation.swift:754) is:

```text
animate if:
  debugForceAnimation == true                          // QA flag
  OR (provider visible AND not fallback-only AND
      snapshot == nil AND !stale)
  OR (provider is Warp AND no data AND refreshing)     // Warp is slow
```

This means: once data lands, the animation stops. If the provider hangs longer than **30 s** the
animation stops anyway and the icon falls back to the brand image. This is the *fallback safety net* —
critical to avoid forever-spinning bars when a provider is dead.

### Frame budget & guarantees

| Guarantee                                | Detail |
|------------------------------------------|--------|
| Layout stability                         | `weeklyRemaining` is clamped to `max(value, 0.0001)` so the IconRenderer never flips layouts mid-animation. |
| Stale state suppressed during anim       | `stale = false` while phase is animating (else dim + animation = visual noise). |
| Credits balance suppressed during anim   | Render with bars only; no `$X.XX` text. |
| Animation never starts for fallback-only providers | Saves battery (#269, #139). |
| No tray-icon flash on launch             | Initial icon rendered synchronously *before* the status item appears (`applyIcon(phase: nil)` called inline). |

### Card-side skeleton

The popup card uses a tiny subtitle change to indicate loading, not a spinner:

| State              | Card subtitle                                | Style class |
|--------------------|----------------------------------------------|-------------|
| Snapshot present   | `Updated <relative time>`                    | `.info`     |
| Refreshing, no data| `Refreshing...`                              | `.loading`  |
| Has error          | `<error text>`                               | `.error`    |
| Never fetched      | `Not fetched yet`                            | `.info`     |

The card has **no** skeleton rectangles. The bars simply don't render until data arrives. Calm. Quiet.

### Stale-but-not-loading

A snapshot older than the stale threshold renders the icon at:

- Track fill alpha **0.18** (vs 0.28 fresh)
- Track stroke alpha **0.28** (vs 0.44 fresh)
- Fill color alpha **0.55** (vs 1.0 fresh)

No animation. The card still shows the last known values, with the timestamp moving to relative-form
("12m ago", "1h ago"). On Windows, replicate exactly: a stale tray icon is at 55% opacity, drawn into the
ICO with the dim values baked in.

---

## 3. Icon morph animations

| Transition               | Trigger                          | Duration       | Curve / pattern                              |
|--------------------------|----------------------------------|----------------|----------------------------------------------|
| Idle → Loading           | `needsMenuBarIconAnimation()` flips true | Immediate, but **phase starts at 0** so the bars rise from low values | Phase = 0; on subsequent ticks `phase += 0.09 rad/frame` at 30 Hz |
| Loading → Idle           | Snapshot arrives                 | One frame      | Last computed bar values are discarded; `applyIcon(phase: nil)` paints actual data; **no cross-fade** (the values are usually close) |
| Idle → Celebration       | `codexbarWeeklyLimitReset` event | 5 s overlay    | Confetti overlay above tray (see §4). Tray icon itself does not morph in mac. Windows: 1.5 s tray-icon morph + toast. |
| Idle → Incident-on       | `statusIndicator.hasIssue` true  | One frame      | Adds a 4 px label-colored dot (minor/maintenance) or 2 × 6 px bar+dot (major/critical) into the rendered bitmap. **No animation.** |
| Incident-on → Incident-off | issue clears                    | One frame      | Re-render without overlay. **No animation.** |
| Quota warning posted     | `codexbarQuotaWarningDidPost`    | **60 s**       | Red tint overlay (`systemRed @ 22%` rounded fill + `systemRed @ 28%` flat fill on top of base). Constant — no flicker. |
| Forced blink             | Cmd-click / debug `Blink Now`    | 360 ms total   | Symmetric easing: `progress < 0.5 ? 2p : 2(1-p)`, raised to 2.2 (punchier than smoothstep). |

The quota-flash duration is `quotaWarningFlashDuration = 60.0` seconds
(Sources/CodexBar/StatusItemController.swift:27). On Windows port it must be the same — long enough that a
user returning from a meeting still sees the red.

---

## 4. Reset celebration (Vortex confetti on Mac)

### What macOS does

When the `.codexbarWeeklyLimitReset` notification fires AND
`settings.confettiOnWeeklyLimitResetsEnabled == true` (default **off**, opt-in), the app spawns a
borderless, click-through `NSPanel` per screen at `.statusBar` level and renders the
`ScreenConfettiOverlayView` for **5 s** before dismissing.

The setting copy is **"Weekly limit confetti — Play full-screen confetti when weekly usage resets."** and
it is in the Advanced pane. There is no per-provider toggle and no time-of-day rule. Triggering events:

- A weekly window for any provider resets while the user has had ≥ 1% utilization in the past 24 h
  (`UsageStore+PlanUtilization.swift` is the source of truth for the "deserves celebration" filter).
- Manual debug trigger from the debug menu (`codexbarDebugReplayAllAnimations`).

### Vortex parameters (Mac)

The overlay launches 12 fireworks in two staggered fans of 6, plus tracer streamers. Each shell has these
parameters (see `makeFireworkConfettiSystem`):

| Property                | Tracer (parent)            | Explosion (secondary)          |
|-------------------------|----------------------------|--------------------------------|
| Birth rate              | 18 / s                     | 24000 / s                      |
| Emission limit          | 4                          | 42                             |
| Emission duration       | 0.22 s                     | 0.08 s                         |
| Idle duration           | 10 s                       | 10 s                           |
| Lifespan                | 0.58 s + (phase × 0.03)    | 4.2 s                          |
| Speed                   | 1.36 + (phase × 0.04)      | 0.72                           |
| Speed variation         | 0.12                       | 0.44                           |
| Launch angle            | 270°/234°/198°/162°/126°/90° (CW); mirror (CCW) | 360° spread       |
| Angle range             | ±12°                       | 360°                           |
| Acceleration            | `[0, 0.12]` (gravity-up)   | `[0, 0.32]`                    |
| Damping factor          | 0.06                       | 0.18                           |
| Angular speed           | `[0, 0, 6]`                | `[0, 0, 3]`                    |
| Stretch factor          | 1.3 (motion-blurred)       | 0.82                           |
| Color palette           | white                      | 6 random hues seeded per fire  |
| Shapes                  | small dot                  | bar / dot / pill / tracer mix  |

The palette is randomized per invocation: pick a base hue, then offsets `[0, 0.08, 0.16, 0.5, 0.66,
0.83]` of the color wheel with saturation `0.55–0.95` and brightness `0.85–1.0`.

The 6 launch phases are staggered with a **60 ms delay** between phases — *not 0 ms*. This staggered
launch is what makes it feel like real fireworks instead of one explosion. The overlay lifetime is **5
seconds** total, then panels are torn down.

The origin point is the tray icon's screen position (passed via
`statusController.celebrationOriginPoint(for: provider)`), inset 8 px from screen edges. If unavailable,
fall back to `(screen.maxX − 28, screen.maxY − 8)` — i.e., upper right.

### Windows mapping

Full-screen confetti is **not idiomatic** on a Windows tray app. From `docs/windows/05-windows-ux-spec.md`
§8: *"Vortex full-screen confetti — replaced by a 1.5s tray-icon morph + a toast with a hero image."*

Implement **both** flavors and let the user pick (default to "Mini"):

#### Mini (default for Windows)

| Step                  | Detail |
|-----------------------|--------|
| Tray morph            | 1.5 s `unbraid` pattern playing brand-logo → bars and back. Driven by the Rust core's normal animation loop, just hard-scheduled to last 1500 ms. |
| Toast                 | `ToastGeneric` with hero image (the same dynamic icon at 96×96, rendered via `tiny-skia`), title `"Weekly limit reset"`, body `"<Provider> · You have a fresh week. May your tokens never run out."` |
| Toast group           | `"codexbar-celebrate"` so multiple resets coalesce. |
| Sound                 | `ms-winsoundevent:Notification.Default` unless DND/Focus Assist is on. |
| In-popup confetti     | If the popup happens to be open at celebration time, ALSO render a 1.2 s canvas-confetti burst inside the popup, anchored to the provider card header. (Phantom-grade detail.) |

#### Full (opt-in for users who want it)

A WebView2 transparent overlay window, click-through (`WS_EX_TRANSPARENT | WS_EX_LAYERED`,
`SetWindowDisplayAffinity(WDA_NONE)`, do not steal focus), drawing the same canvas-confetti as the popup
but at fullscreen. Same 5 s lifetime, same staggered 60 ms launches, same palette algorithm. Use a
JS-side particle system (`canvas-confetti` library or hand-rolled with `requestAnimationFrame`) tuned to
the parameters above. **Always click-through.**

In-popup canvas-confetti library: use a small custom system rather than pulling in a 50 kB dependency.
Particles are simple — gravity, damping, angular velocity, lifespan.

---

## 5. Bar fill animation

| Question                                    | Mac answer |
|---------------------------------------------|------------|
| Does the menu-card progress bar animate on value change? | **No.** UsageProgressBar uses a single `Canvas { context, size in ... }` with no implicit animations. The fill width updates instantly when the bound value changes. |
| Does the tray icon bar animate on value change? | **No.** The bar fill is the IconRenderer's `clip + paint rect` — recomputed each render call. |
| Does color animate between thresholds?       | **No.** Color is the provider brand color (or `MenuHighlightStyle.selectionText` when the row is highlighted). Static. |

**Why static?** Because the bar value normally changes **once every 30+ seconds** (the refresh cadence)
and animating would create false motion that competes with the user's eye. The values just snap to truth.

### What *does* animate around the bar

- **Pace stripe (the small triangle):** static position; punched out of the fill with `destinationOut`
  blend. No motion.
- **Pace tip color:** **green** when in reserve / on track; **red** when in deficit (and not highlighted);
  **white** when row is highlighted.
- **Warning markers:** thin 1-px vertical lines at the configured thresholds. Static.
- **Hover:** menu rows highlight via macOS native menu highlight, no app-side animation.

### Windows mapping

| Surface          | Behavior |
|------------------|----------|
| Tray bitmap bar  | Snaps to value. No tween. |
| Popup card bar   | **Tween** the width over **240 ms ease-out** when the value changes (Windows users are accustomed to micro-animations in flyouts). Use `requestAnimationFrame` and a cubic-bezier `(0.16, 1, 0.3, 1)` (i.e., `easeOutExpo`). Skip the tween if `prefers-reduced-motion` is true OR `UISettings.AnimationsEnabled == false`. |
| Color transition | None. Brand color is static. |
| Pace tip         | Reposition with the same 240 ms ease-out tween. |

Rationale for the asymmetry: the tray icon is too small to read motion (16 px), and adding motion creates
flicker when DPI scaling rounds the fill width to the nearest integer pixel. The popup card is large
enough that a clean tween is satisfying.

---

## 6. Pace text transitions

The pace text is in three states (Sources/CodexBar/UsagePaceText.swift):

| Stage class                       | Left label              | Right label patterns                                     |
|-----------------------------------|-------------------------|-----------------------------------------------------------|
| `onTrack`                         | `On pace`               | (optional) `Lasts until reset` / `Runs out in <time>` / `≈ 35% run-out risk` |
| `slightlyAhead`/`ahead`/`farAhead`| `<X>% in deficit`       | `Runs out in 1d 4h · ≈ 60% run-out risk` |
| `slightlyBehind`/`behind`/`farBehind` | `<X>% in reserve`   | `Lasts until reset` |

The two strings are joined by `" · "` (middle dot with thin spaces) — never `,` and never `|`. The pace
text is hidden when less than 3% of the window has elapsed (avoid noisy guessing early on).

### Reflow without jank

- The two-line label is a horizontal `HStack` with `firstTextBaseline` alignment.
- The width is **content-driven**, not fixed.
- When the value flips between stages (e.g., `On pace` → `12% in deficit`), the **menu card width does
  not jump** because the card has a fixed width per-provider; the text just rerenders.

### Windows mapping

- Use a CSS grid: `grid-template-columns: auto auto`, gap `4px`, baseline alignment via
  `align-items: baseline`.
- When the pace stage changes, **fade-cross** the inner text (`opacity 0 → 1` over 180 ms ease-out)
  rather than letting the new string snap in. Use React's `key` change on the wrapper to trigger CSS
  transitions. Skip the fade if reduced-motion is on.
- The middle dot uses `· ` (U+00B7 + space). Stick to this — the Mac uses it everywhere.

---

## 7. Menu open/close

### macOS — `NSMenu` (no custom popover for the main menu)

The main menu is a native `NSMenu` populated with `NSMenuItem`s, some of which host SwiftUI views via
`NSHostingView`. The opening animation is **the system's** — there is no custom spring or fade-in. Mac
does not let third parties retime menu open.

The hidden detail: **closing** the menu after a refresh-key press is handled inside `StatusItemMenu`
(Sources/CodexBar/StatusItemMenu.swift): pressing `Cmd-R` while the menu is open triggers a refresh and
the menu stays open. This is the *one* keyboard shortcut the app hijacks while a menu is open. Cmd-clicks
on bars open the provider dashboard in the default browser.

Settings/Preferences uses a **`spring(response: 0.32, dampingFraction: 0.85)`** for tab content size
changes — soft, settled bounce. The about-pane icon uses
`spring(response: 0.32, dampingFraction: 0.78)` for hover scale.

### Windows mapping

The Windows popup is a **`WebviewWindow` with frameless, transparent, Mica-or-Acrylic backdrop** (see
`docs/windows/05-windows-ux-spec.md` §2). The open/close motion is *all yours*:

| Phase            | Duration | Curve                              | Transform                                  |
|------------------|----------|------------------------------------|--------------------------------------------|
| Show             | 180 ms   | cubic-bezier(0.16, 1, 0.3, 1) (easeOutExpo) | `translateY(8px) → 0`, `opacity 0 → 1`, `scale(0.98) → 1` |
| Hide             | 140 ms   | cubic-bezier(0.4, 0, 1, 1) (easeIn) | `translateY(0 → 8px)`, `opacity 1 → 0`, `scale(1 → 0.98)` |
| Re-click toggle  | 140 ms   | (same as hide)                     | Then 180 ms re-show |

The translateY direction depends on the taskbar edge — slide *up* from the tray (`+8 → 0`) when the
taskbar is on the bottom; mirror for top/left/right.

### Focus loss

- Click outside → dismiss with the *Hide* animation.
- `Esc` → dismiss.
- Re-click the tray icon while open → dismiss (toggle).
- Window loses focus to another **modal** (alert dialog) → keep popup open; close when modal closes.
- Window loses focus because the user opened Notification Center via Win+N → dismiss popup (it's now
  visually behind).

Listen for: `WM_ACTIVATE` with `WA_INACTIVE`, the window-blur event in WebView2, and the global mouse
hook for clicks outside the window.

---

## 8. Switcher tab transitions

In "Merge Icons" mode the popup header shows tabs for each enabled provider plus an optional Overview
tab. Switching tabs:

| Element             | Behavior |
|---------------------|----------|
| Tab indicator bar   | A 2 px underline that **slides** to the new tab's x-position. Duration **220 ms**, cubic-bezier `(0.2, 0, 0, 1)` (Material standard). |
| Content cross-fade  | Outgoing card opacity `1 → 0` over 120 ms ease-out; incoming `0 → 1` over 180 ms ease-out, **30 ms overlap**. |
| Slide               | Optional `translateX(±6px)` paired with the fade gives a "left/right" feel based on tab index direction. |
| Keyboard nav        | `←` / `→` cycles tabs; `Home` jumps to first; `End` to last. |
| Long-press / right-click on tab | Quick menu: *Refresh just this provider*, *Open auth settings*, *Disable*. |

The Mac uses `withAnimation(.spring(response: 0.32, dampingFraction: 0.85))` for the **Preferences**
window's tab size change. Reuse those numbers in the Windows port for the Preferences pane (a real window,
not the popup): JavaScript spring or `react-spring` with `tension: 200, friction: 26` is a close match.

---

## 9. Hover & pressed states

| Element                 | Hover-in       | Hover-out       | Pressed                            | Focus ring |
|-------------------------|----------------|-----------------|-------------------------------------|------------|
| Menu row (popup card)   | 120 ms ease-out background tint `surface.secondary.opacity(0.18)` | 120 ms ease-out | Scale 0.94, 120 ms ease-out (`CopyIconButtonStyle`) | 2 px accent-color ring, 2 px offset |
| Copy-icon button        | (parent row hover) | (parent row hover) | **Scale 0.94 over 120 ms ease-out** + `backgroundColor.opacity 0 → 0.18` | accent-color ring |
| About-pane app icon     | **80 ms hover delay** before scale begins | — | — | — |
| About-pane app icon (after delay) | `scale 1 → 1.05`, `shadow 0 → accent@25%`, `spring(0.32, 0.78)` (≈ 320 ms) | reverse via same spring | — | — |
| Tray icon hover (Mac)   | No effect | — | (Click toggles menu) | — |
| Tab indicator           | (see §8) | — | — | accent-color outline |

### Hover delay (Phantom detail)

The Mac about-pane uses `withAnimation(.spring(...)) { hovering = true }` directly, *no* delay. But for
**tooltips** (Mac's `.help(...)` modifier), the system uses an ~800 ms hover delay. The Windows port
should match: **800 ms before tooltip appears**, **0 ms before hover effects begin**. The Phantom-grade
addition is **80 ms for "explanation icons"** (the small `?` icons next to settings) — fast enough that
users discover the affordance, slow enough that they don't trigger on a flyby.

### Pressed scale

The pressed scale is `0.94` (a 6% shrink), not 0.95 or 0.90. The duration is `easeOut(duration: 0.12)`
both for press-down and release. This is uniform — every interactive surface uses these numbers.

### Focus ring

| Surface  | Mac approach | Windows approach |
|----------|--------------|------------------|
| Settings controls | macOS native — system blue ring, 3 px | 2 px accent-color (`UISettings.GetColorValue(UIColorType.Accent)`), 2 px offset, 6 px corner radius |
| Popup tabs / rows | None custom (native menu) | Same accent-color ring; required for arrow-key navigation |
| Tray icon | n/a (no focus concept) | n/a |

Use `outline-offset: 2px` and `outline-style: solid`. Do not use `box-shadow` for focus — accessibility
testing (NVDA + Narrator) needs to detect the native outline.

---

## 10. Click-to-copy overlay

The Mac implementation (Sources/CodexBar/ClickToCopyOverlay.swift) is a transparent `NSView` overlaid on
copyable text. `mouseDown` writes to `NSPasteboard.general` — that's it. No visual feedback in the
overlay itself.

The **`CopyIconButton`** inside `MenuCardView.swift` is where the polish lives:

| Phase    | Duration         | Easing           | Visuals |
|----------|------------------|------------------|---------|
| Press    | 120 ms           | easeOut          | Scale `1 → 0.94`, background `0 → 0.18` |
| Release  | 120 ms           | easeOut          | Scale `0.94 → 1`, background `0.18 → 0` |
| Copied flash | 120 ms ease-out (in) | — | Icon swaps `doc.on.doc` → `checkmark`; `didCopy = true` |
| Hold     | 900 ms           | (no anim)        | Checkmark visible |
| Reset    | 200 ms ease-out  | — | `didCopy = false`; icon swaps back |

Accessibility label flips from `"Copy <thing>"` → `"Copied"` for VoiceOver. No alert, no toast, no sound.

### Windows mapping

| Element                       | Windows |
|-------------------------------|---------|
| Icon swap                     | Use Lucide / Fluent system icons — `Copy` and `Check`. **No** font swap (use the same icon font). |
| Animation                     | Same timings exactly. Use CSS `transition: transform 120ms ease-out, background-color 120ms ease-out`. |
| Hold duration                 | **900 ms** then revert. |
| Sound                         | None. |
| Toast                         | None — the inline flash is the feedback. |
| Tooltip                       | `"Copy <thing>"` / `"Copied"` (the latter only visible while `didCopy === true`). |

### Click-to-copy on a *display* (no button)

For the cases where any clickable label (e.g., the email, the API endpoint) copies on click:

- Underline on hover (`text-decoration: underline; text-underline-offset: 2px`).
- Cursor `pointer`.
- Brief 200 ms flash on click — the underline turns accent-colored and the text background pulses
  `accent @ 12%` then fades.
- Live region announces "Copied" once (`aria-live="polite"`).

---

## 11. System notifications

All notifications go through `AppNotifications.shared.post(...)` (Sources/CodexBar/AppNotifications.swift)
which wraps `UNUserNotificationCenter`. Authorization is requested **on first post**, not on launch
splash (`requestAuthorizationOnStartup` only seeds the task — actual prompt happens lazily). Identifiers
are scoped per notification *kind* with a `codexbar-<prefix>-<uuid>` shape so duplicates are not blocked,
but the prefix carries deduplication intent.

### Surfaces

| Notification kind        | Trigger                              | Title                                  | Body                                                  | Sound | Badge | Dedupe key prefix |
|--------------------------|--------------------------------------|----------------------------------------|-------------------------------------------------------|-------|-------|--------------------|
| Session depleted         | Remaining session quota crosses `≤ 0.0001` from positive | `<Provider> session depleted`          | `0% left. Will notify when it's available again.`     | Default | — | `session-<provider>-depleted` |
| Session restored         | Was depleted, now positive remaining | `<Provider> session restored`          | `Session quota is available again.`                   | Default | — | `session-<provider>-restored` |
| Quota warning (threshold)| `currentRemaining ≤ threshold%` AND not yet fired for this threshold | `<Provider> <window> quota low`        | `<X>% left. Reached your <threshold>% <window> warning threshold.` | **Glass** (or Ping fallback), played *via NSSound before the toast*; toast itself is silent | — | `quota-warning-<provider>-<window>-<threshold>` |
| Weekly reset celebration | Weekly window resets while user has had ≥ 1% utilization recently | (no notification — *confetti instead*) | — | — | — | — |
| Augment session keep-alive | Augment-specific, runs in background  | (from `AugmentSessionKeepalive.swift`)  | — | Default | — | — |

### Threshold dedup

`QuotaWarningNotificationLogic` (Sources/CodexBar/SessionQuotaNotifications.swift:39) tracks fired
thresholds per provider/window. Once a threshold fires, it stays fired until `currentRemaining` rises
back **above** it. This means:

- Crossing 80% fires `80%` and marks `{80, 70, 60, 50, ...}` (`firedThresholdsAfterWarning` clears
  everything at or above the just-crossed threshold).
- Falling further to 60% fires `60%` once.
- Recovery to 75% clears `60%` but not `80%`.

So **lower thresholds re-arm** once recovery happens. Higher thresholds stay armed until full recovery
above them. This is intentional — users don't want "Quota low" spam every refresh.

### Snooze / action buttons

The Mac implementation has **no in-toast action buttons** (the Mac `UNNotificationContent` has no
actions configured). Clicking the notification opens the popup but does not target a specific provider.

### Windows mapping

Use `windows::UI::Notifications::ToastNotification` with `ToastGeneric` template:

```xml
<toast launch="codexbar://celebrate?provider=claude" scenario="reminder">
  <visual>
    <binding template="ToastGeneric">
      <text>Claude session depleted</text>
      <text>0% left. Will notify when it's available again.</text>
      <image placement="appLogoOverride" hint-crop="circle" src="..."/>
    </binding>
  </visual>
  <actions>
    <action content="Open CodexBar" arguments="open" activationType="foreground"/>
    <action content="Snooze 1h" arguments="snooze:3600" activationType="background"/>
  </actions>
</toast>
```

The `launch` URI uses our `codexbar://` custom protocol (registered at install). Two action buttons:

| Button       | Behavior |
|--------------|----------|
| Open CodexBar| Opens the popup centered on primary monitor (so the user can find it even if it's hidden in overflow), focused on the relevant provider's card. |
| Snooze 1h    | Marks the threshold as "snoozed until now+1h" — re-arm even if not yet recovered. Use the same `firedThresholdsAfterWarning` registry but check a `snoozedUntil` timestamp. |

Toast group for quota warnings: `codexbar-quota-<provider>` (so newer toasts replace older). For
celebrations: `codexbar-celebrate-<isoWeek>` (one per week).

---

## 12. Sounds

CodexBar bundles **zero custom sound assets**. All sounds come from the OS sound system:

| Sound name          | Used for                              | When |
|---------------------|---------------------------------------|------|
| `NSSound("Glass")`  | Quota-warning ping                    | Played via `NSSound.play()` directly *before* the silent toast (see SessionQuotaNotifications.swift:130). |
| `NSSound("Ping")`   | Fallback if `Glass` not available     | Same as above. |
| `UNNotificationSound.default` | All other notifications      | macOS default notification sound. |

The reason the quota-warning path plays `NSSound("Glass")` directly and passes `soundEnabled: false` to
the toast is so the sound rings even if the user has disabled the default notification sound.

### User-disable toggle

`settings.quotaWarningSoundEnabled` (Preferences → Notifications → "Play notification sound") gates the
`NSSound` call. The toast still posts visually.

### Windows mapping

| Mac sound          | Windows equivalent                                      |
|--------------------|--------------------------------------------------------|
| `NSSound("Glass")` | `ms-winsoundevent:Notification.IM` (IM sound — chime)   |
| `NSSound("Ping")`  | `ms-winsoundevent:Notification.Default`                |
| Default            | `ms-winsoundevent:Notification.Default`                |
| (disabled)         | `<audio silent="true"/>` in the toast XML              |

Put the `<audio src="..." silent="false|true"/>` inside the toast XML. Don't use `windows::Media::Audio`
— it's overkill and adds startup cost.

### "Do not disturb" / Focus Assist

| State                          | Behavior |
|--------------------------------|----------|
| Focus Assist **off**           | Sound + visible toast |
| Focus Assist **priority only** | Sound + visible toast iff the user has whitelisted CodexBar in priority list. Otherwise: silent toast, queued to Notification Center. |
| Focus Assist **alarms only**   | Silent toast (no sound), queued to Notification Center, banner suppressed. |
| Quiet hours active             | Same as alarms-only. |

Detect via `QueryUserNotificationState()` (returns `QUNS_NOT_PRESENT`, `QUNS_BUSY`,
`QUNS_RUNNING_D3D_FULL_SCREEN`, `QUNS_PRESENTATION_MODE`, `QUNS_ACCEPTS_NOTIFICATIONS`, `QUNS_QUIET_TIME`,
`QUNS_APP`). If `QUNS_QUIET_TIME` or `QUNS_PRESENTATION_MODE` → suppress sound, still post toast (it
silently lands in Notification Center).

### Sound mute toggle scoping

Preferences → Notifications → "Play notification sound" silences **all** CodexBar sounds, not just the
quota warning. This is the user's master switch. Default ON.

---

## 13. Sparkle update flow UI

The Mac uses **Sparkle** (Sources/CodexBar/PreferencesAboutPane.swift). The polish details:

| State                | UI element                                                     |
|----------------------|----------------------------------------------------------------|
| No update            | About pane shows "Version 1.x.x · Built 2026-02-19 14:42" + "Check for Updates…" button. |
| Auto-check toggle    | `Check for updates automatically` checkbox, default ON. |
| Update channel       | `Stable` / `Beta` picker (persisted to UserDefaults under `updateChannel`). |
| Update available     | Sparkle's own modal — sheet with release notes (rendered HTML) + Install / Remind Me Later / Skip This Version. |
| Downloading          | Sparkle's own progress sheet. |
| Ready to install     | "Install and Relaunch" button. |
| Build unavailable    | "Updates unavailable in this build." (e.g. Homebrew formula path). |

### Windows mapping

Use **Tauri's built-in updater** (`tauri-plugin-updater`). Replace Sparkle's sheets with **custom in-app
banners** rendered inside the Preferences window (and as a small banner inside the popup):

#### Popup banner

A 1-line banner at the **top** of the popup card stack:

```
[!] CodexBar 1.4.2 is available · [Update now] [Later] [×]
```

| Behavior              | Detail |
|-----------------------|--------|
| Visible duration      | Until user clicks something or dismisses. Re-appears on next launch if update still pending. |
| Background            | accent-color @ 10% |
| Border-radius         | 8 px |
| Padding               | `8px 12px` |
| Slide-in              | `translateY(-8px) → 0` over 200 ms ease-out on popup open |

#### Preferences > About pane

Three states, no modals:

| State            | Rendered |
|------------------|----------|
| Idle             | "You're up to date · Last checked 12m ago" + "Check Now" button |
| Checking         | "Checking for updates…" + inline spinner (300 ms fade-in to avoid flash) |
| Available        | Card with version, full Markdown-rendered release notes, "Install and Relaunch" primary button, "Skip This Version" secondary, "Remind Me Tomorrow" tertiary |
| Downloading      | Progress bar (`<progress>` styled with accent color), "12% — 4.3 MB / 36 MB" |
| Ready to install | "Update downloaded. Relaunch to apply." + "Relaunch Now" / "Later" |
| Error            | Plain error text with retry button. **No alert dialog.** |

#### Restart prompt

Default to *Tauri's restart()*. Add a custom confirmation dialog only if there are unsaved changes
elsewhere (there aren't in CodexBar — all settings are auto-saved). So: clicking "Relaunch Now" should
restart immediately, no second confirmation.

---

## 14. First-run / onboarding microinteractions

| Touch                             | Detail |
|-----------------------------------|--------|
| Tray icon appears                 | Not animated — appears as a static brand-image-with-bars based on provider defaults (no critter, no animation). The first "real" animation happens on first refresh. |
| Welcome toast                     | Once, on first launch: *"CodexBar lives in the tray. Drag it to the always-visible area: open the overflow flyout (`^`) and drag CodexBar left of the Wi-Fi icon."* Action: *Don't show again*. |
| Popup auto-opens (first run only) | Centered on primary monitor, **not** anchored to tray. The `onboarding` flag is true for the first session. |
| Onboarding card                   | A single dismissable card at the top: *"Welcome to CodexBar — let's pick your providers."* with a primary "Choose providers" button that opens the Providers pane. |
| Empty state copy                  | If no providers enabled: large brand mark, secondary text *"No providers yet."*, primary CTA *"Add a provider"*. Use the providers SVG icons; avoid stock Microsoft Fluent icons. |
| "First provider added" celebration | Tiny scale-pop (1 → 1.06 → 1, 240 ms ease-out cubic-bezier(0.34, 1.56, 0.64, 1)) on the new provider's card, **once**. No sound. |
| Default refresh cadence           | Sensible default (e.g., 60 s) is selected, not "off". |
| First refresh                     | Triggers the loading animation immediately — gives the user instant feedback that *something is happening*. |

### Avoid

- No splash screen.
- No main-window flash on launch (Windows users *hate* tray apps that briefly show a main window).
- No "Tour" carousel — the popup *is* the onboarding.
- No mandatory account creation; CodexBar uses local-only auth.

---

## 15. Empty / zero / error states

### Empty state — no providers

| Element        | Copy / behavior |
|----------------|-----------------|
| Hero           | App icon at 64×64 (procedural, no critter). |
| Title          | `No providers yet.` |
| Body           | `Pick a provider to start tracking your agent quota.` |
| CTA            | `Add a provider` (opens Preferences → Providers) |
| Footer link    | `Why CodexBar?` (opens README in browser) |

### Empty state — provider enabled but no data

| Element        | Copy / behavior |
|----------------|-----------------|
| Card subtitle  | `Not fetched yet` |
| Bars           | Not rendered (no `0%` ghost). |
| CTA            | `Refresh now` |
| Tray icon      | Brand icon only (no bars). |

### Zero state — provider has data but session at 0%

| Element        | Copy / behavior |
|----------------|-----------------|
| Session bar    | Renders at 0% with track only. |
| Reset text     | `Resets in 3h 12m` (preserved). |
| Pace           | Hidden (no positive remaining to pace against). |
| Card subtitle  | `Session depleted` (red color). |
| Tray icon      | Empty bar + (60 s quota-flash tint if recent). |

### Error state — last refresh failed

| Element        | Copy / behavior |
|----------------|-----------------|
| Card subtitle  | `<error text>` in `MenuHighlightStyle.error()` color (red). |
| Copy button    | Visible to the right of the error; copies the *full* error to clipboard. |
| Bars           | Last known values rendered, but stale-styled (55% opacity). |
| Tray icon      | Stale opacity. No animation. |

The error text comes from the provider's last error string verbatim (no friendly rewriting). The "copy
error" button is a key polish item — *makes filing a bug report take one click*.

---

## 16. Accessibility

### macOS today

| Surface         | VoiceOver |
|-----------------|-----------|
| Tray icon       | Status bar item has the provider's display name + current percent as accessibility label. |
| Bars            | `.accessibilityLabel("Usage remaining")` and `.accessibilityValue("\(Int(clamped)) percent")` on each bar. |
| Copy button     | Label flips between `"Copy <thing>"` and `"Copied"`. |
| Reset text      | Read as-is. |
| Pace            | Read as-is. |

Keyboard navigation: native `NSMenu` arrow-key + Return handling. The Open-Menu shortcut is user-bindable
(`KeyboardShortcuts.openMenu`).

### Windows mapping

The WebView2 popup must expose a complete accessibility tree to **Narrator**:

| Element            | Role / Name                                                                  |
|--------------------|------------------------------------------------------------------------------|
| Popup window       | `dialog`, `aria-label="CodexBar usage"` |
| Tabs               | `role="tablist"` / `role="tab"` with `aria-selected`, `tabindex={selected ? 0 : -1}` |
| Provider card      | `role="region"`, `aria-labelledby` pointing to provider name |
| Bars               | `role="progressbar"`, `aria-valuenow`, `aria-valuemin=0`, `aria-valuemax=100`, `aria-label="Session usage"` |
| Reset text         | Read by default (plain text node). |
| Pace               | Plain text. Annotate with `aria-live="polite"` so updates are announced. |
| Copy buttons       | `<button aria-label="Copy error">`; on success change label to `"Copied"`. |
| Quota-warning toast| Native toast — Narrator reads automatically. |

### Tray icon

`Shell_NotifyIcon` exposes the tooltip via Narrator (limited). Tray icons have **no focusable element** in
the accessibility tree — users navigate the tray via Win+B and then arrow keys. The tooltip is the *only*
accessible label; pack the essentials: `"CodexBar — Claude 67% session · 41% week · resets in 3h 12m"`.

### Keyboard nav order in popup (top to bottom)

1. App icon header (focusable, opens dashboard on Enter)
2. Provider tabs (left/right arrow rotates; Enter selects)
3. Each provider card → bars (Enter opens dashboard) → reset times → pace → details
4. Status pill (if incident)
5. Footer: Refresh now, Preferences, Quit

`Esc` always dismisses the popup. `Tab` walks forward, `Shift+Tab` walks back. Focus rings are visible on
all interactive elements.

### High-contrast mode

Detect `prefers-contrast: more` and `forced-colors: active` (Windows High Contrast). When active:

- Remove `Mica`/`Acrylic` backdrops (they tint colors unpredictably).
- Use `Canvas`/`CanvasText`/`Highlight` system colors.
- Bars become solid colors with 2 px borders.
- Critter blink/wiggle is **suppressed** — too much for high-contrast users.

---

## 17. Reduced motion

The Mac codebase does **not** currently honor `NSAccessibility.reduceMotionEnabled`. (Grep confirms: no
`reduce.*motion` references.) This is a gap the Windows port should fix.

### Windows mapping

Detect via:

- `Windows.UI.ViewManagement.UISettings::AnimationsEnabled` (system-wide)
- CSS `@media (prefers-reduced-motion: reduce)` (WebView2)

When **reduced motion is on**:

| Animation                       | Behavior |
|---------------------------------|----------|
| Tray loading animation          | **Disabled.** Show static "loading dot" overlay (a `…` ellipsis or a 50% alpha brand mark) instead. |
| Critter blink/wiggle/tilt       | **Disabled.** Static icon. |
| Quota-warning flash             | Still applied (it's a static red tint, not animated). |
| Reset celebration (Mini)        | Skip morph; play *only* the toast. |
| Reset celebration (Full)        | Suppress overlay entirely. Toast only. |
| Popup open/close                | Instantaneous show/hide (`transition: none`). |
| Bar fill tween                  | Instantaneous. |
| Pace text crossfade             | Instantaneous. |
| Tab indicator slide             | Instantaneous. |
| Hover scale/pressed scale       | **Keep** (these are sub-100 ms, they're feedback not motion). The OS guidelines distinguish *motion* from *transitions of essential state* — pressed-scale is essential feedback. |
| Copy-flash checkmark            | Keep (sub-200 ms instant swap). |
| Confetti (canvas in popup)      | Suppress completely. |

Listen for `UISettings.AnimationsEnabledChanged` and live-update.

---

## 18. Sound mute / "do not disturb"

Already covered in §12. To restate the matrix:

| State                      | Sound | Toast | Tray flash |
|----------------------------|-------|-------|------------|
| Default                    | Play  | Show  | Show       |
| User sound toggle off      | Skip  | Show  | Show       |
| Focus Assist priority      | Conditional on whitelist | Show silently if not whitelisted | Show |
| Focus Assist alarms-only   | Skip  | Queued to Notification Center, no banner | Show |
| QUNS_QUIET_TIME            | Skip  | Queued | Show       |
| QUNS_PRESENTATION_MODE     | Skip  | Queued | Show       |
| Game / fullscreen app      | Skip  | Queued | Show (Windows lets tray flash through) |

The tray icon's quota-flash overlay **is always shown** — it's a visual cue, not an interruption. The
60 s duration means the user sees it even after returning from focus mode.

---

## 19. Tone & voice

CodexBar's voice is **dry, technical, lightly affectionate**. Examples:

| Surface              | Microcopy |
|----------------------|-----------|
| About tagline        | **"May your tokens never run out—keep agent limits in view."** |
| Surprise-me subtitle | *"Check if you like your agents having some fun up there."* |
| Reset celebration    | *"You have a fresh week. May your tokens never run out."* |
| Quota warning body   | *"`<X>%` left. Reached your `<threshold>%` `<window>` warning threshold."* |
| Session depleted     | *"`0%` left. Will notify when it's available again."* |
| Session restored     | *"Session quota is available again."* |
| Empty state          | *"No data yet"* / *"Not fetched yet"* (not "Loading…" — be precise) |
| Cost auto-refresh    | *"Auto-refresh: hourly · Timeout: 10m"* (period-separated, no fluff) |
| Manual refresh hint  | *"Auto-refresh is off; use the menu's Refresh command."* |
| Pace — on track      | *"On pace"* |
| Pace — in deficit    | *"12% in deficit · Runs out in 1d 4h · ≈ 65% run-out risk"* |
| Pace — in reserve    | *"8% in reserve · Lasts until reset"* |
| Copyright            | *"© 2026 Peter Steinberger. MIT License."* |
| Multi-screen quirk   | *"Drag to reorder"* (tooltip on provider list; no exclamation mark) |
| Onboarding hint      | *"Choose your providers."* (period — declarative, not breathless) |

### Voice rules

| Rule                    | Apply |
|-------------------------|-------|
| No "Awesome!" / "Yay!"  | Strict. |
| No exclamation marks    | Outside of the tagline, never. |
| No corporate "we"       | Use second person ("you") or impersonal. |
| No emoji in app strings | The README has emoji (🎚️); the app does not. |
| Use period-separated micro-lists with `·` | E.g., `"hourly · Timeout: 10m"`. |
| Approximation symbol `≈`| Used in `"≈ 65% run-out risk"` — keep it, it's quietly delightful. |
| Em-dash `—` (not `--`)  | The tagline uses it. Keep proper em-dashes everywhere. |
| Lowercase units         | `1d 4h`, `3h 12m` (no spaces inside, no `hr`/`min`/`hrs`). |
| Time format             | Either relative ("in 3h 12m") or absolute clock ("14:42") per user preference. |

The lowercase compact units (`1d 4h`, `3h 12m`) are a Phantom-grade detail. The Mac uses
`UsageFormatter.resetCountdownDescription` to produce these — port the exact algorithm.

---

## 20. Polish checklist

The single most important section of this doc. ~50 individual touches that elevate the feel — pin this on
the wall during implementation.

### Tray icon

1. **No white flash on launch.** Render the icon synchronously before `Shell_NotifyIcon NIM_ADD`. Never let the OS show a default placeholder.
2. **30 Hz loading cadence**, never 60 — calmer and saves battery.
3. **30 s hard cap** on continuous loading animation (then fall back to brand image).
4. **Bars never flash between 0 and small values.** Clamp `weeklyRemaining` to `max(value, 0.0001)` during animation so the IconRenderer's "weeklyRemaining == 0" branch never triggers mid-animation.
5. **Stale state = 55% alpha**, not grayscale. Color is preserved.
6. **Critter blink delay 3–12 s random per provider.** Don't sync — they should feel independent.
7. **Double-blink chance 18%**, 220–340 ms inter-blink. Makes the critter feel "thinking."
8. **Blink curve `pow(symmetric, 2.2)`** — punchier than smoothstep.
9. **Tilt limit ~6.4°** (`.pi/28`). Past that it looks broken.
10. **Wiggle never offsets eyes more than 1 px** — keeps the face readable.
11. **Quota flash is 60 s exact** — long enough that a user returning from coffee still sees it.
12. **Brand-with-percent mode** uses the same `quotaWarningFlashImage` overlay — the red tint is universal.
13. **Cmd-click / right-click** the tray icon opens dashboard URL (not the popup) — direct deep-link.
14. **Mouse over the tray icon** shows the full multi-line tooltip with provider + session + week + reset. Cap at 128 chars (`NIM_TIP` max).
15. **DPI changes** redraw the icon without flicker — pre-render all 5 sizes at app start, swap by `NIM_MODIFY`.

### Popup

16. **Click-to-popup latency < 100 ms.** Below the perceptual threshold. Keep WebView2 pre-warmed.
17. **Open animation 180 ms ease-out-expo**, close 140 ms ease-in — slightly faster close so dismissal feels responsive.
18. **TranslateY direction matches taskbar edge** — slide up from a bottom taskbar, down from top, etc.
19. **Esc, click-outside, re-click tray icon** all dismiss (toggle).
20. **Modal alerts keep the popup open** until the modal closes. Then dismiss-on-blur resumes.
21. **Mica on Win11, Acrylic on Win10**, flat surface on high-contrast. Re-evaluate on theme change (`WM_SETTINGCHANGE`).
22. **12 px corner radius** — wider than menus (8 px), so it reads as a panel.
23. **Subtle drop shadow** — barely visible on dark, more on light. Match Win11 flyouts.
24. **Account email truncates middle**, not end (`p…@example.com`).
25. **Last-refreshed time** in the footer in relative form (`12m ago`), not absolute.
26. **Refresh button** spins its icon 360° over 800 ms ease-in-out when clicked. *Once*. No spinner-while-fetching — the icon already shows loading state.

### Cards

27. **Bars do not animate width changes in the tray** (snap), but **do** in the popup (240 ms ease-out-expo).
28. **Pace text fade-cross on stage change** (180 ms). The width doesn't jump because card width is fixed.
29. **Warning markers are 1–2 px lines**, 72% opacity in normal, white when highlighted.
30. **Pace tip color: green (reserve), red (deficit), white (highlighted).** No yellow middle ground.
31. **Provider card hover** tints background `surface.secondary.opacity(0.18)` over 120 ms.

### Interactions

32. **Copy button: scale 0.94 on press, 120 ms ease-out.** Same numbers everywhere — uniform feel.
33. **Copy flash holds 900 ms**, then 200 ms fade back. Long enough to be seen, short enough not to nag.
34. **Hover delay for tooltips = 800 ms** (Windows default), but **0 ms for hover effects** (instant feedback).
35. **About-pane app icon: spring(0.32, 0.78) on hover** for the gentle scale + accent-color shadow.
36. **Tab indicator slides 220 ms** with material-curve `(0.2, 0, 0, 1)`. Content cross-fades with 30 ms overlap.
37. **Focus ring is 2 px accent**, 2 px offset, 6 px corner radius. Visible on **all** interactive surfaces.
38. **Keyboard nav order matches visual order** top-to-bottom, left-to-right.
39. **Cmd-R / Ctrl-R while menu open** triggers refresh without closing the menu. Mac does this via `StatusItemMenu.performKeyEquivalent`; Windows port should do the same.

### Notifications & sounds

40. **Quota-warning sound plays before the toast**, not as the toast sound — so muting notification sounds doesn't silence it (unless the master sound toggle is off).
41. **Threshold dedup is per-provider × per-window** — Claude session 80% and Codex session 80% are independent.
42. **Lower thresholds re-arm on recovery**, higher thresholds stay armed. No `Quota low` spam.
43. **All toasts are clickable** and open the popup (`launch="codexbar://open?provider=<id>"`).
44. **Toast group key** so newer toasts replace older for the same provider/window/threshold.
45. **Focus Assist respected**: alarms-only and quiet-time skip the sound, still queue the toast to Notification Center.

### Celebration

46. **Confetti opt-in, default off** (so it doesn't surprise users in shared screens).
47. **Tray-morph + toast on Windows**, full-screen confetti only as opt-in.
48. **Celebration triggers only if utilization ≥ 1% in past 24 h** — don't confetti for users who never used the week.
49. **In-popup confetti if popup is open** at celebration time. Extra micro-touch.
50. **Palette is freshly randomized** each time. Never the same colors twice in a row.

### Accessibility

51. **Reduce motion: suppress critter, loading animation, slides, fades.** Keep instantaneous state changes and sub-100 ms feedback (pressed-scale, copy-flash).
52. **Narrator labels all bars** as `role="progressbar"` with `aria-valuenow`.
53. **High-contrast mode strips Mica**, uses system colors, removes critter.
54. **Live regions on pace/subtitle** so screen readers announce updates.

### First-run & emptiness

55. **No splash, no main-window flash on launch.** Tray icon, period.
56. **First-run welcome toast** with overflow-flyout instructions and "Don't show again."
57. **Empty card has a copyable CTA**, not a dead state.
58. **Error text is copyable in one click** — makes bug reports easy.
59. **Onboarding popup centered on primary monitor**, no anchoring to tray (first run only).

### Voice

60. **No exclamation marks**, except in the tagline.
61. **`·` separator with thin spaces** for compound labels (`On pace · Lasts until reset`).
62. **`≈` for approximate percentages**, lowercase compact units (`1d 4h`).
63. **Em-dash `—`** never `--`.
64. **"May your tokens never run out"** is the only "warm" string in the app. Don't dilute by adding others.

---

## Appendix A — Timing reference card

A copy-paste constants block for the React side:

```ts
export const motion = {
  press:          { duration: 120, easing: 'ease-out' },
  release:        { duration: 120, easing: 'ease-out' },
  hoverIn:        { duration: 120, easing: 'ease-out' },
  hoverOut:       { duration: 120, easing: 'ease-out' },
  copyFlashIn:    { duration: 120, easing: 'ease-out' },
  copyFlashHold:  { duration: 900 },
  copyFlashOut:   { duration: 200, easing: 'ease-out' },
  popupShow:      { duration: 180, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' },
  popupHide:      { duration: 140, easing: 'cubic-bezier(0.4, 0, 1, 1)' },
  tabIndicator:   { duration: 220, easing: 'cubic-bezier(0.2, 0, 0, 1)' },
  tabFadeOut:     { duration: 120, easing: 'ease-out' },
  tabFadeIn:      { duration: 180, easing: 'ease-out', delayMs: -30 },
  barWidth:       { duration: 240, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' },
  paceFadeCross:  { duration: 180, easing: 'ease-out' },
  prefsTabSpring: { tension: 200, friction: 26 },
  iconHoverSpring:{ tension: 200, friction: 22 },
};
```

And the Rust side:

```rust
pub const LOADING_FPS: f64 = 30.0;
pub const LOADING_PHASE_INC: f64 = 2.7 / LOADING_FPS;     // ≈ 0.09 rad/frame
pub const LOADING_MAX_DURATION_SEC: u64 = 30;
pub const BLINK_ACTIVE_TICK_MS: u64 = 75;
pub const BLINK_IDLE_FALLBACK_MS: u64 = 1000;
pub const BLINK_DURATION_MS: u64 = 360;
pub const BLINK_DOUBLE_CHANCE: f64 = 0.18;
pub const BLINK_DOUBLE_DELAY: std::ops::RangeInclusive<f64> = 0.22..=0.34;
pub const BLINK_DELAY_RANGE: std::ops::RangeInclusive<f64> = 3.0..=12.0;
pub const QUOTA_FLASH_DURATION_SEC: u64 = 60;
pub const CONFETTI_OVERLAY_LIFETIME_SEC: u64 = 5;
pub const CONFETTI_PHASE_STAGGER_MS: u64 = 60;
```

## Appendix B — Files to read for implementation

Sorted by importance:

1. `Sources/CodexBar/StatusItemController+Animation.swift` — every animation parameter the tray uses.
2. `Sources/CodexBar/LoadingPattern.swift` — the six pattern math expressions.
3. `Sources/CodexBar/DisplayLink.swift` — frame driver semantics.
4. `Sources/CodexBar/ScreenConfettiOverlayController.swift` — confetti parameters & lifecycle.
5. `Sources/CodexBar/SessionQuotaNotifications.swift` — threshold logic & dedup.
6. `Sources/CodexBar/AppNotifications.swift` — auth flow & toast posting.
7. `Sources/CodexBar/MenuCardView.swift` — copy-button animation, subtitle states.
8. `Sources/CodexBar/UsageProgressBar.swift` — bar canvas layout & pace tip.
9. `Sources/CodexBar/UsagePaceText.swift` — pace stage → label mapping.
10. `Sources/CodexBar/PreferencesView.swift` — preferences spring values.
11. `Sources/CodexBar/PreferencesAboutPane.swift` — hover-spring + about layout.
12. `Sources/CodexBar/StatusItemMenu.swift` — Cmd-R while-menu-open behavior.
13. `Sources/CodexBar/Resources/en.lproj/Localizable.strings` — voice & microcopy.
14. `Sources/CodexBar/IconRenderer.swift` — critter rendering (blink/wiggle/tilt application).

— End of 80-feel-and-polish.md —
