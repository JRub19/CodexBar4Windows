import { useEffect, useRef } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

// Auto-resize the tray popup window to fit its content. Width stays
// fixed at 360 px; height grows to fit, capped at the visible screen
// height (so the popup never extends past the user's monitor).
//
// Measurement strategy: sum the natural heights of the three top-
// level regions (header, body, footer). We can't use root.scrollHeight
// because the body has `overflow-y: auto` to handle the rare case
// where content genuinely exceeds the screen — that overflow setting
// makes root.scrollHeight equal to the current window height, never
// the natural content. scrollHeight on each *child* still reports
// natural content size regardless of overflow on its parent, so the
// sum approach works.

const WIDTH = 360;
const MIN_HEIGHT = 220;
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
        // 60 px headroom: taskbar (~40 px on Win 11) + a little
        // breathing room so the popup never touches the screen edge.
        return Math.max(MIN_HEIGHT, avail - 60);
      }
      return SCREEN_CAP_FALLBACK;
    };

    const measure = (): number => {
      const header = root.querySelector(".popup-header") as HTMLElement | null;
      const body = root.querySelector(".popup-body") as HTMLElement | null;
      const footer = root.querySelector(".popup-footer") as HTMLElement | null;
      const sum =
        (header?.scrollHeight ?? 0) +
        (body?.scrollHeight ?? 0) +
        (footer?.scrollHeight ?? 0);
      // 1 px border top + bottom on .popup-root.
      return sum + 2;
    };

    const apply = () => {
      const target = measure();
      const cap = screenCap();
      const clamped = Math.min(cap, Math.max(MIN_HEIGHT, Math.ceil(target)));
      if (clamped === lastApplied) return;
      lastApplied = clamped;
      void getCurrentWindow()
        .setSize(new LogicalSize(WIDTH, clamped))
        .catch(() => {
          /* best-effort */
        });
    };

    const schedule = () => {
      if (raf != null) cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        raf = null;
        apply();
      });
    };

    // Observe each region — body changes (card swap), header changes
    // (switcher tab count), footer changes (update row appears).
    const resizeObserver = new ResizeObserver(schedule);
    for (const sel of [".popup-header", ".popup-body", ".popup-footer"]) {
      const el = root.querySelector(sel);
      if (el) resizeObserver.observe(el);
    }
    resizeObserver.observe(root);

    // Subtree mutations (e.g. React re-rendering a card with different
    // content) don't always change an observed element's box size, so
    // also watch DOM mutations and re-measure.
    const mutationObserver = new MutationObserver(schedule);
    mutationObserver.observe(root, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["class", "style"],
    });

    // Multi-pass initial measurement. Layout settles over a few
    // frames as fonts / icons / async data land; one synchronous
    // measure on mount isn't enough.
    apply();
    const t1 = window.setTimeout(apply, 120);
    const t2 = window.setTimeout(apply, 400);
    const t3 = window.setTimeout(apply, 1000);

    return () => {
      if (raf != null) cancelAnimationFrame(raf);
      window.clearTimeout(t1);
      window.clearTimeout(t2);
      window.clearTimeout(t3);
      resizeObserver.disconnect();
      mutationObserver.disconnect();
    };
  }, []);

  return ref;
}
