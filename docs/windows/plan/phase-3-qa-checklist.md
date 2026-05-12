# Phase 3 manual QA checklist

This walks a human reviewer through every acceptance row from
[`docs/windows/spec/10-tray-icon-system.md`](../spec/10-tray-icon-system.md)
section 13 and
[`docs/windows/spec/15-popover-menu-card-ui.md`](../spec/15-popover-menu-card-ui.md)
section 17. Run on a fresh Windows 11 install with English (US) regional
settings; rerun on a Surface Laptop (125% DPI) and a 4K external
display (200% DPI) before promoting to Phase 4.

## Setup

1. `cargo run --manifest-path apps/desktop-tauri/src-tauri/Cargo.toml --features dev`
   Starts the desktop shell with `Fixture::cycle()` rotating every 8 s.
2. `./scripts/screenshot.ps1 -OutDir tests/golden/win -Variant dark`
   then toggle Windows theme and rerun with `-Variant light`.

## Tray icon (spec 10 section 13)

- [ ] Default theme detection picks the correct light/dark palette on
      both Windows themes.
- [ ] DPI scaling preserves the 18 by 18 pt logical canvas at 100, 125,
      150, 200, 300%.
- [ ] Loading pattern animates at 30 Hz, idles at 5 Hz on low power.
- [ ] Stale state dims the bars per `STALE_ALPHAS`.
- [ ] Quota flash twists tilt and untilt smoothly without snap.
- [ ] Status overlay (dot, dot+line) renders within the 18x18 bounding
      box.
- [ ] Reset celebration morph completes within 1.2 s and clears.
- [ ] ICO atlas contains 16, 20, 24, 28, 32, 36, 40, 48, 64 sizes.

## Popover (spec 15 section 17)

- [ ] Popup opens within 180 ms of a left click, closes within 140 ms on
      focus loss or Esc.
- [ ] Mica/Acrylic backdrop applied, not the fallback solid color.
- [ ] Header shows provider name on single account, "Overview" on
      merged mode.
- [ ] Switcher tabs render inline (≤3 providers), stacked (≥4), 4-row
      (≥15).
- [ ] Weekly indicator under each unselected tab.
- [ ] Provider card header shows display name, plan pill, email (middle
      truncated), subtitle.
- [ ] UsageProgressBar tween runs once per data update at 200 ms,
      skipped on first paint.
- [ ] Quota markers at 50% and 20% remaining, hidden when settings flag
      enabled.
- [ ] Pace text matches spec 15 section 6 strings.
- [ ] Reset countdown rotates between countdown and absolute styles.
- [ ] Click-to-copy shows the "Copied" chip for 1.32 s.
- [ ] Footer Refresh row spins icon during refresh and shows "Refresh
      failed" subtitle on error.
- [ ] Footer accelerators show `Ctrl+,` and `Ctrl+Q` with 0.02em letter
      spacing.
- [ ] First-run toast appears 3 s after first popup mount, never again
      after dismiss.

## Accessibility

- [ ] Tab moves through switcher → cards → footer in DOM order.
- [ ] ArrowLeft / ArrowRight cycle the switcher.
- [ ] Esc dismisses the popup.
- [ ] Every focusable element shows the 2 px accent ring.
- [ ] System "Animation effects: off" collapses all transitions to 1
      ms; no jank, no missing data.

## Multi-display

- [ ] Popup correctly anchors to the tray icon on the primary monitor.
- [ ] Popup also anchors correctly when the tray sits on a secondary
      monitor with a different DPI.
- [ ] Taskbar at the top, left, or right of the screen positions the
      popup on the opposite side.
