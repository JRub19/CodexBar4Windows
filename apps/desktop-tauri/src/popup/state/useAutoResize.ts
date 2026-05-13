import { useEffect, useRef } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

// Auto-resize the tray popup window to fit its content.
//
// Width stays fixed at 360 px. Height grows to fit the **natural**
// content size — no artificial low cap, only the screen-height cap
// the OS imposes (so the popup never extends past the visible area
// of the user's primary monitor).
//
// Implementation notes:
//
//   - `.popup-root` uses intrinsic sizing (`flex: 0 0 auto` on the
//     body; `overflow-y: visible` on the root). That means
//     `root.scrollHeight` reports the true natural content height
//     regardless of the current window size — no internal clipping
//     to fight, no need to sum children manually.
//
//   - A single `ResizeObserver` on the root catches any reflow.
//
//   - A `MutationObserver` catches subtree changes that don't
//     trigger a resize (e.g. swapping out the active provider's
//     card content via React).

const WIDTH = 360;
const MIN_HEIGHT = 220;
// Generous fallback if `window.screen.availHeight` is unavailable —
// roughly fits a portrait 1080p with a little breathing room.
const SCREEN_CAP_FALLBACK = 980;

export function useAutoResize() {
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const root = ref.current;
    if (!root) return;

    let raf: number | null = null;
    let lastApplied = -1;

    const screenCap = (): number => {
      const avail = (window.screen as Screen | undefined)?.availHeight;
      if (typeof avail === "number" && avail > 200) {
        // Leave 80 px of headroom so the tray popup doesn't extend
        // past the visible monitor (Windows taskbar + safety margin).
        return Math.max(MIN_HEIGHT, avail - 80);
      }
      return SCREEN_CAP_FALLBACK;
    };

    const apply = () => {
      const cap = screenCap();
      const natural = root.scrollHeight;
      const clamped = Math.min(cap, Math.max(MIN_HEIGHT, Math.ceil(natural)));
      if (clamped === lastApplied) return;
      lastApplied = clamped;
      void getCurrentWindow()
        .setSize(new LogicalSize(WIDTH, clamped))
        .catch(() => {
          // Best-effort — Tauri rejection (window closed / wrong
          // context) is non-fatal; we just don't resize.
        });
    };

    const schedule = () => {
      if (raf != null) cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        raf = null;
        apply();
      });
    };

    const resizeObserver = new ResizeObserver(schedule);
    resizeObserver.observe(root);

    const mutationObserver = new MutationObserver(schedule);
    mutationObserver.observe(root, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["class", "style"],
      characterData: false,
    });

    // Initial apply on mount.
    apply();
    // A second pass on the next frame after the initial render —
    // images / async fonts / icons can shift layout in the first
    // few frames and we want the window settled by then.
    const initial = window.setTimeout(apply, 120);

    return () => {
      if (raf != null) cancelAnimationFrame(raf);
      window.clearTimeout(initial);
      resizeObserver.disconnect();
      mutationObserver.disconnect();
    };
  }, []);

  return ref;
}
