import { useEffect, useRef } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

// Auto-resize the tray popup window to fit its content.
//
// Width stays fixed at 360 px. Height grows to fit the natural
// content, clamped to `[MIN_HEIGHT, MAX_HEIGHT]`.
//
// **Why we don't just read `root.scrollHeight`:** the popup-body has
// `flex: 1` so it eats whatever vertical space remains in the window.
// When the window starts at 480 px and the body content actually
// needs 600 px, `root.scrollHeight` still reports 480 (the body just
// becomes internally scrollable and reports its own larger
// `scrollHeight` separately). The root never grows on its own.
//
// To get the *true* natural content height we sum the three known
// children explicitly:
//
//   header.scrollHeight + body.scrollHeight + footer.scrollHeight + 2
//
// (the `+ 2` is the 1 px border above and below the popup root.)
//
// A `ResizeObserver` on each child keeps us in sync as content
// changes (e.g. a refresh result expands the card).

const WIDTH = 360;
const MIN_HEIGHT = 220;
const MAX_HEIGHT = 720;

export function useAutoResize() {
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const root = ref.current;
    if (!root) return;

    let raf: number | null = null;
    let lastApplied = -1;

    const measure = (): number => {
      // Sum the three top-level popup regions. `scrollHeight` on each
      // returns the natural content size regardless of any overflow
      // clipping its CSS has applied.
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
      const clamped = Math.min(
        MAX_HEIGHT,
        Math.max(MIN_HEIGHT, Math.ceil(target)),
      );
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

    // Observe each known child. Adding more children would require
    // also adding them here — but that's the trade-off for getting
    // a correct measurement without fighting CSS layout.
    const observer = new ResizeObserver(schedule);
    for (const sel of [".popup-header", ".popup-body", ".popup-footer"]) {
      const el = root.querySelector(sel);
      if (el) observer.observe(el);
    }
    // Also observe the root itself so the very first measurement
    // (before children have laid out) still fires.
    observer.observe(root);

    // Mutations inside the body — e.g. switching cards — can change
    // height without resizing existing children. Watch for them and
    // re-measure on the next frame.
    const mutationObserver = new MutationObserver(schedule);
    mutationObserver.observe(root, {
      childList: true,
      subtree: true,
      attributes: false,
      characterData: false,
    });

    // Initial apply on mount.
    apply();

    return () => {
      if (raf != null) cancelAnimationFrame(raf);
      observer.disconnect();
      mutationObserver.disconnect();
    };
  }, []);

  return ref;
}
