import { useEffect, useRef } from "react";
import {
  getCurrentWindow,
  LogicalSize,
  PhysicalPosition,
} from "@tauri-apps/api/window";

// Auto-resize the popup window to fit its content. The popup is fixed
// width (380 px); only the height changes.
//
// Why a ref to the popup root and not `document.body`? `body` includes
// the FirstRunToast that's pinned bottom-right and would inflate the
// measurement. We measure the explicit shell wrapper instead.
//
// Implementation notes (every one earned by a previous regression):
//
//  1. **Sum of children, not scrollHeight.** `scrollHeight` reads the
//     clipped value when the parent already has `overflow: hidden`;
//     summing the wrapper children gives the true natural height.
//
//  2. **Min/max bounds.** We refuse to size below MIN_H (avoids the
//     popup snapping to 40 px when the tree is briefly empty during
//     mount) and clamp at MAX_H (avoids covering half the screen if
//     a metric ever explodes).
//
//  3. **rAF-throttled.** ResizeObserver can fire multiple times per
//     frame; we coalesce into one setSize call per animation frame.
//
//  4. **Delta gate.** Don't issue a setSize if the new height is
//     within DELTA_PX of the last one. Saves IPC chatter when
//     sub-pixel sub-rounding bounces around.
//
//  5. **No feedback loop.** setSize fires WindowEvent::Resized on the
//     Rust side, which can trigger another layout pass. The delta
//     gate prevents this from oscillating; the rAF throttle prevents
//     it from spinning the loop.

const MIN_H = 240;
const MAX_H = 900;
const DELTA_PX = 2;

export function useAutoResize(rootRef: React.RefObject<HTMLElement | null>) {
  const lastHeightRef = useRef<number | null>(null);
  const rafRef = useRef<number | null>(null);

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;

    const measure = (): number => {
      let total = 0;
      for (const child of Array.from(root.children) as HTMLElement[]) {
        // Skip pinned-overlay-style children (FirstRunToast etc.).
        // They have position:absolute/fixed and shouldn't contribute
        // to the flow height.
        const cs = window.getComputedStyle(child);
        if (cs.position === "absolute" || cs.position === "fixed") continue;
        // Use `scrollHeight` instead of the bounding rect so we get
        // the child's CONTENT height, not its current flex-stretched
        // box. The popup body has `flex: 1` which would otherwise
        // make the measurement track the window height instead of
        // the cards inside it.
        const mt = parseFloat(cs.marginTop) || 0;
        const mb = parseFloat(cs.marginBottom) || 0;
        total += child.scrollHeight + mt + mb;
      }
      return Math.ceil(total);
    };

    const apply = async () => {
      rafRef.current = null;
      const target = measure();
      if (target < MIN_H || target > MAX_H + 1000) {
        // Either too small (mid-mount with nothing rendered yet) or
        // suspiciously huge (we'd rather not commit to a window that
        // covers the screen). Skip this tick.
        return;
      }
      const clamped = Math.min(MAX_H, Math.max(MIN_H, target));
      if (
        lastHeightRef.current != null &&
        Math.abs(clamped - lastHeightRef.current) < DELTA_PX
      ) {
        return;
      }
      lastHeightRef.current = clamped;
      try {
        const w = getCurrentWindow();
        // Capture current position + size BEFORE resizing so we can
        // pin the popup's BOTTOM edge to its current spot (it's
        // anchored above the tray icon — losing height should shrink
        // toward the top, not push the bottom away from the icon).
        const [prevPos, prevSize, dpi] = await Promise.all([
          w.outerPosition(),
          w.outerSize(),
          w.scaleFactor(),
        ]);
        await w.setSize(new LogicalSize(380, clamped));
        const newPhysicalHeight = Math.round(clamped * dpi);
        const deltaY = prevSize.height - newPhysicalHeight;
        if (deltaY !== 0) {
          await w.setPosition(
            new PhysicalPosition(prevPos.x, prevPos.y + deltaY),
          );
        }
      } catch {
        /* swallow — failing to resize is non-fatal. */
      }
    };

    const schedule = () => {
      if (rafRef.current != null) return;
      rafRef.current = window.requestAnimationFrame(() => {
        void apply();
      });
    };

    // Initial measurement.
    schedule();

    // `ResizeObserver` catches box-size changes on the wrapper itself.
    // Inside the popup body (`flex: 1`) the body's BOX is locked to
    // the viewport regardless of card content, so a content-only
    // reflow would not trigger this observer. We also attach a
    // MutationObserver to catch subtree changes (cards appearing /
    // disappearing, onboarding ↔ card-stack swap, etc.) and re-run
    // the measure pass.
    const ro = new ResizeObserver(() => schedule());
    ro.observe(root);
    for (const child of Array.from(root.children) as HTMLElement[]) {
      ro.observe(child);
    }
    const mo = new MutationObserver(() => schedule());
    mo.observe(root, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["class", "style"],
      characterData: true,
    });

    return () => {
      ro.disconnect();
      mo.disconnect();
      if (rafRef.current != null) {
        window.cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
    };
  }, [rootRef]);
}
