---
summary: "How CodexBar should look and feel on Windows. Not the Mac app in a costume."
read_when:
  - Designing or reviewing any user-facing surface
---

# 05 — Windows UX spec

The user’s ask: *"the icon at the bottom right of the screen where you have the ethernet icon, volume icon, keyboard icon — dynamic, updates with the current usage, all features that work on Mac should work on Windows, look and feel exactly the same but optimized for Windows."*

"Exactly the same but optimized for Windows" is a contradiction taken literally. The honest read: **same features, same information density, same level of polish — but native Windows idioms wherever the Mac idiom doesn’t exist on Windows.** Below is the resolution.

## 1. The tray icon

### Where it lives

The Windows taskbar **notification area** (Win11: "system tray," bottom-right by default). Unlike macOS, Windows **hides infrequently used tray icons by default** in the overflow flyout (the chevron `^` to the left of the always-visible icons).

**Action item:** on first run, after creating the icon, surface a one-time toast: *"CodexBar lives in the tray. To pin it to the always-visible area: open the overflow flyout (`^`) and drag CodexBar to the left of the Wi-Fi icon. Settings ▸ Personalization ▸ Taskbar ▸ Other system tray icons."* Include a "Don’t show again" action.

### What it looks like

- 16×16 logical px base; render also at 20×20 (125% DPI), 24×24 (150% DPI), 32×32 (200% DPI), 40×40 (250% DPI). Hand all sizes to `Shell_NotifyIcon` so Windows picks the right one.
- **Two vertical bars** by default — *session* (left, taller) and *weekly* (right), with optional *credits* (third). Fill represents percent remaining, or percent used if the user flips the toggle.
- **Brand-icon mode** (toggle): show the provider logo + a 2-digit `%` label baked into the bitmap.
- **Stale state**: 50% alpha overall.
- **Incident overlay**: a 5×5 red dot in the top-right corner when the provider has an active incident.
- **Animations**:
  - "Loading" pulse — 1.5 Hz, capped at 8 seconds per refresh cycle to prevent hung-providers from chewing CPU.
  - "Reset" celebration — 1.5 second wiggle/morph (no full-screen confetti like Vortex; that doesn’t belong on a Windows taskbar app).

### Light / dark theme

Windows ships a separate **taskbar theme** (light or dark) independent of the system theme. Detect with:

```
HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\SystemUsesLightTheme
```

Render two variants: bars in **#1f1f1f** for light taskbars, **#f0f0f0** for dark. Watch for theme changes via `WM_SETTINGCHANGE` and redraw.

### Hover & tooltip

`Shell_NotifyIcon` tooltip = `"CodexBar — Claude 67% session · 41% week · resets in 3h 12m"`. Multi-line allowed (up to 128 chars).

## 2. The click — popup

### Open behavior

- **Left-click**: open popup, anchored to the tray icon rect. Direction picked dynamically based on which screen edge holds the taskbar (bottom-up is the common case; left-edge taskbars push the popup rightward).
- **Right-click**: native context menu (see §3).
- **Click outside / focus loss**: dismiss.
- **Esc**: dismiss.
- **Re-click the icon while open**: dismiss (toggle behavior, like Volume).

### Window chrome

- Frameless. 360×480 px default, content-sized.
- **Mica** backdrop on Windows 11; **Acrylic** fallback on Windows 10. Both go through Tauri’s `WebviewWindowBuilder::transparent(true)` + `set_effects`. If neither is available, fall back to a flat dark/light surface with the user-selected accent color as the highlight.
- 12 px corner radius. Windows 11 system menus use ~8 px; 12 px keeps the popup feeling like a "panel" rather than a "menu," which is correct — it has card content, not menu rows.
- Subtle drop shadow (`box-shadow` in WebView CSS), darker on light theme, near-invisible on dark theme — to match Windows 11 flyouts.

### Contents (top-to-bottom)

1. **Header row**: app icon (left), provider switcher tabs / Overview tab (center) if Merge Icons mode is on, settings cog (right).
2. **Provider card stack** — one card per enabled provider, or just the selected one in Merge mode:
   - Brand icon + display name (e.g., "Claude Pro").
   - Account/email line (small, secondary color).
   - **Three reset windows** rendered as horizontal bars (session, weekly, monthly) with percent + "Resets in 3h 12m" or absolute clock (`14:42`) per the user preference.
   - Credits / spend / cost line if the provider exposes it.
   - "Pace: on pace" / "12% in deficit · runs out in 1d 4h" / "8% in reserve · lasts until reset" — match upstream pacing semantics.
3. **Status pill** if the provider has an active incident: a colored pill linking to the status page.
4. **Footer**: *Refresh now* (with last-refreshed relative time), *Preferences…*, *Quit CodexBar*.

### Typography & color

- System font: **Segoe UI Variable** on Win11, **Segoe UI** on Win10. Sizes: 13 px body, 11 px secondary, 16 px provider name.
- Use the system accent color (`UISettings.GetColorValue(UIColorType.Accent)`) for the active-bar fill and the "Refresh now" link, so the app adopts the user’s personalization.
- Dark mode: `#202020` surface, `#2b2b2b` card, `#e6e6e6` primary text, `#a0a0a0` secondary text.
- Light mode: `#f9f9f9` surface, `#ffffff` card, `#1f1f1f` primary, `#5d5d5d` secondary.

### Interactions

- Cards are focusable with arrow keys.
- Click a bar → open the provider’s dashboard URL in the default browser (same as the Mac app).
- Long-press / right-click a provider card → quick menu: *Refresh just this provider*, *Open auth settings*, *Disable*.
- Mouse hover on a bar shows a richer tooltip ("Resets at 14:42 local · 3h 12m from now").

## 3. Right-click context menu (native)

Native `muda` menu, not HTML. Items:

- **Refresh now** (default)
- **Pause refresh** (toggle)
- ── separator ──
- **Preferences…** (opens a normal `WebviewWindow`, not the popup)
- **About CodexBar**
- **Check for updates…**
- ── separator ──
- **Quit CodexBar** (`Ctrl+Q` accelerator)

Use Segoe MDL2 / Fluent System icons for the entries to match Windows 11.

## 4. Preferences window

A normal resizable window (560×640 px default), Mica-styled, with a left rail and a right pane — matches the *Settings* app pattern.

Panes:

1. **General** — refresh cadence, launch at sign-in, language, hide quota warning markers (already in upstream).
2. **Providers** — toggleable list with per-provider auth panels.
3. **Display** — bar style, percent-used vs percent-remaining, brand-icon mode toggle, merge icons mode, overview tab providers.
4. **Notifications** — toast on threshold, weekly-reset celebration toggle, sound.
5. **Shortcuts** — global hotkeys (default `Win+Shift+U` to toggle popup; user-rebindable).
6. **Advanced** — debug logging, storage scan, credential manager passthrough.
7. **About** — version, "Forked from steipete/CodexBar," check-for-updates button.

## 5. Notifications

- Toast on threshold crossings ("Claude session 90% used"). Hero image = the same dynamic icon at 96×96.
- Weekly reset toast with the celebration icon morph (~1.5s).
- All toasts persist to **Notification Center** (Win11) and are clickable to open the popup.

## 6. Startup behavior

- First run: show the popup centered on the primary monitor with onboarding text ("Choose your providers"); after this, never auto-show — wait for the user to click the tray.
- If "Launch at sign-in" is enabled, start with the popup hidden; just show the tray icon. No splash, no main-window flash. (Windows users hate splashes on tray apps.)

## 7. Performance budgets

Tray apps live in the user’s peripheral attention. Crossing these limits = uninstall:

- **Idle RAM**: < 70 MB resident.
- **Idle CPU**: < 0.2% on a modern laptop.
- **Per-refresh CPU spike**: < 2% sustained for < 500 ms.
- **Disk I/O at idle**: 0 reads, 0 writes.
- **Network at idle**: only the refresh cadence the user picked.
- **Cold start to tray icon visible**: < 800 ms on an SSD.
- **Click-to-popup-open**: < 100 ms (perceptible threshold).

## 8. Things the Mac app does that we deliberately omit on Windows

- **Vortex full-screen confetti** — replaced by a 1.5s tray-icon morph + a toast with a hero image. Full-screen overlays are not Windows-idiomatic for a passive tray app.
- **`NSMenu` hosted SwiftUI inside the menu** — all rich content goes into the popup window. The right-click menu is a small, native, OS-styled menu.
- **Status item drag-to-reorder** — not a thing on the Windows taskbar. Hide in overflow / pin via Settings is the equivalent and we educate the user once.
- **Per-provider menu-bar item by default** — Windows tray real-estate is more constrained. Default to **Merge Icons mode** (single tray icon with a provider switcher in the popup) for new users; expose the per-provider mode as an Advanced setting.

## 9. The Windows-specific bits the Mac app doesn’t need

- **WSL hint banner** when the Claude CLI is detected only under WSL and not under Windows — link to the docs page on enabling it natively or letting CodexBar shell into WSL.
- **Browser closed required** banner when Chromium cookie decryption fails because the browser locked the DB.
- **App-Bound Encryption ("V20") warning** for Chrome ≥ 127 — explain that we can’t decrypt the new format and offer the manual cookie paste path.
- **SmartScreen first-run banner** — if we don’t have an EV cert, give the user a guided "More info → Run anyway" walkthrough on the website.

## 10. Acceptance: the gut-check

A user opens the popup. Within 1 second they can answer:

1. Am I about to run out of Claude / Codex / Cursor? (Yes / No / How much.)
2. When does my next window reset?
3. Is the provider currently up?
4. Do I need to do anything to recover this auth?

If any of those four require more than one glance, the design is wrong.
