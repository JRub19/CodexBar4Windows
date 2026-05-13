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
const MIN_W = 380;
const MAX_W = 1200;
const DELTA_PX = 2;

export function useAutoResize(rootRef: React.RefObject<HTMLElement | null>) {
  const lastHeightRef = useRef<number | null>(null);
  const lastWidthRef = useRef<number | null>(null);
  const rafRef = useRef<number | null>(null);

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;

    // Measures the popup root's natural content size. Returns
    // [width, height]. The root is a flex row (`popup-main` + optional
    // side panel) so total width = sum of column widths and total
    // height = max of column heights. For each column we sum the
    // scrollHeight of its children (not the bounding rect) because
    // flex: 1 elements would otherwise report viewport height.
    const measure = (): { width: number; height: number } => {
      let totalWidth = 0;
      let maxHeight = 0;
      for (const column of Array.from(root.children) as HTMLElement[]) {
        const cs = window.getComputedStyle(column);
        if (cs.position === "absolute" || cs.position === "fixed") continue;
        if (cs.display === "none") continue;
        const colWidth = column.getBoundingClientRect().width;
        let colHeight = 0;
        for (const child of Array.from(column.children) as HTMLElement[]) {
          const ccs = window.getComputedStyle(child);
          if (ccs.position === "absolute" || ccs.position === "fixed")
            continue;
          const mt = parseFloat(ccs.marginTop) || 0;
          const mb = parseFloat(ccs.marginBottom) || 0;
          colHeight += child.scrollHeight + mt + mb;
        }
        // If the column has no children (rare), fall back to its
        // own scrollHeight.
        if (colHeight === 0) colHeight = column.scrollHeight;
        totalWidth += colWidth;
        maxHeight = Math.max(maxHeight, colHeight);
      }
      return { width: Math.ceil(totalWidth), height: Math.ceil(maxHeight) };
    };

    const apply = async () => {
      rafRef.current = null;
      const target = measure();
      if (target.height < MIN_H || target.height > MAX_H + 1000) {
        // Either too small (mid-mount with nothing rendered yet) or
        // suspiciously huge (we'd rather not commit to a window that
        // covers the screen). Skip this tick.
        return;
      }
      const clampedH = Math.min(MAX_H, Math.max(MIN_H, target.height));
      const clampedW = Math.min(MAX_W, Math.max(MIN_W, target.width));
      const heightChanged =
        lastHeightRef.current == null ||
        Math.abs(clampedH - lastHeightRef.current) >= DELTA_PX;
      const widthChanged =
        lastWidthRef.current == null ||
        Math.abs(clampedW - lastWidthRef.current) >= DELTA_PX;
      if (!heightChanged && !widthChanged) return;
      lastHeightRef.current = clampedH;
      lastWidthRef.current = clampedW;
      try {
        const w = getCurrentWindow();
        // Capture current position + size BEFORE resizing so we can
        // pin the popup's BOTTOM-RIGHT corner to its current spot
        // (the popup is anchored above the tray icon — losing
        // height should shrink toward the top, gaining width should
        // grow toward the left).
        const [prevPos, prevSize, dpi] = await Promise.all([
          w.outerPosition(),
          w.outerSize(),
          w.scaleFactor(),
        ]);
        await w.setSize(new LogicalSize(clampedW, clampedH));
        const newPhysicalHeight = Math.round(clampedH * dpi);
        const newPhysicalWidth = Math.round(clampedW * dpi);
        const deltaY = prevSize.height - newPhysicalHeight;
        const deltaX = prevSize.width - newPhysicalWidth;
        if (deltaX !== 0 || deltaY !== 0) {
          await w.setPosition(
            new PhysicalPosition(prevPos.x + deltaX, prevPos.y + deltaY),
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
