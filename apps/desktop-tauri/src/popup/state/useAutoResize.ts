import { useEffect, useRef } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

// Auto-resize the tray popup window to fit its content. Width stays
// fixed; height grows to whatever the body needs, clamped to a sane
// minimum (so brief empty/loading states don't collapse the window)
// and maximum (so a misbehaving provider list never goes off-screen).
//
// Implementation:
//   - ResizeObserver watches the root element.
//   - On size change we call `window.setSize(LogicalSize(W, H))`.
//   - We use logical pixels because Tauri's setSize on a high-DPI
//     monitor would otherwise stomp on the OS scale factor.
//
// The hook returns a ref to attach to the outer popup element.

const WIDTH = 360;
const MIN_HEIGHT = 220;
const MAX_HEIGHT = 600;

export function useAutoResize() {
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    let raf: number | null = null;
    let lastApplied = -1;

    const applyHeight = (target: number) => {
      // Clamp + round to avoid sub-pixel flapping.
      const clamped = Math.min(
        MAX_HEIGHT,
        Math.max(MIN_HEIGHT, Math.ceil(target)),
      );
      if (clamped === lastApplied) return;
      lastApplied = clamped;
      void getCurrentWindow()
        .setSize(new LogicalSize(WIDTH, clamped))
        .catch(() => {
          // Best-effort — if Tauri rejects (window already closed,
          // wrong context, etc.) we silently drop the resize.
        });
    };

    const observer = new ResizeObserver((entries) => {
      if (raf != null) cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        raf = null;
        const entry = entries[entries.length - 1];
        if (!entry) return;
        // Use scrollHeight so we measure the actual content not the
        // visible viewport (which may already be clipped to the
        // previous window size).
        const target = (entry.target as HTMLElement).scrollHeight;
        applyHeight(target);
      });
    });

    observer.observe(el);

    // Initial apply on mount in case the content is already laid out
    // before the observer fires.
    applyHeight(el.scrollHeight);

    return () => {
      if (raf != null) cancelAnimationFrame(raf);
      observer.disconnect();
    };
  }, []);

  return ref;
}
