import { useEffect } from "react";
import { useUsageStore } from "../state/usageStore";

// Phase 3 E1: popup-wide keyboard navigation.
//
// - ArrowLeft / ArrowRight cycle the selected provider tab.
// - Esc is handled by `PopupShell` to hide the window (spec 80).
// - Tab and Enter rely on native focus semantics; we do not override
//   them so the platform conventions stay intact.
//
// Focus on the initial active tab is the responsibility of the
// SwitcherTab when it mounts — we provide the selection coordination
// here so any number of switchers can subscribe.

export function useKeyboardNav() {
  const descriptors = useUsageStore((s) => s.descriptors);
  const selectedId = useUsageStore((s) => s.selectedProviderId);
  const selectProvider = useUsageStore((s) => s.selectProvider);

  useEffect(() => {
    if (descriptors.length < 2) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      const idx = descriptors.findIndex((d) => d.id === selectedId);
      const next =
        event.key === "ArrowRight"
          ? (idx + 1) % descriptors.length
          : (idx - 1 + descriptors.length) % descriptors.length;
      event.preventDefault();
      selectProvider(descriptors[next].id);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [descriptors, selectedId, selectProvider]);
}
