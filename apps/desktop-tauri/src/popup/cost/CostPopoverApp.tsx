import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CostHistoryChart } from "../cards/CostHistoryChart";
import type { ProviderCostSnapshot } from "../cards/useCostHistory";

// Render target for the `cost-popover` Tauri window. The window is
// created at app start (visible: false) and shown on demand from
// the main popup's per-card Cost row hover.
//
// State delivery — IMPORTANT: we PULL the active provider via a
// Tauri command on mount AND on window-shown, NOT just listen for
// an emit event. The one-shot emit can fire before the popover
// WebView's listener is attached the very first time, leaving the
// component with `providerId = null` and rendering nothing — which
// looked like "the panel is empty" + made the hover bridge no-op
// (no DOM elements to attach handlers to).
//
// Hover bridge: as long as the cursor is over this window's
// content, we invoke `cancel_cost_popover_close` on the Rust side
// so the close timer (started when the cursor leaves the trigger
// row in the main popup) never fires.

const PROVIDER_BRAND_HEX: Record<string, string> = {
  claude: "#cc7c5e",
  codex: "#10a37f",
};

export function CostPopoverApp() {
  const [providerId, setProviderId] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<ProviderCostSnapshot | null>(null);
  const [animateIn, setAnimateIn] = useState(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  // Fetch helper: pull active provider + its snapshot from Rust.
  const refresh = async () => {
    try {
      const pid = await invoke<string | null>(
        "get_active_cost_popover_provider",
      );
      if (!mounted.current) return;
      if (!pid) {
        setProviderId(null);
        setSnapshot(null);
        return;
      }
      setProviderId(pid);
      const all = await invoke<Record<string, ProviderCostSnapshot>>(
        "cost_snapshots",
      );
      if (!mounted.current) return;
      setSnapshot(all?.[pid] ?? null);
    } catch {
      /* ignore — empty render is fine */
    }
  };

  // Initial pull on mount — covers the case where the Rust emit
  // fires before this listener was attached.
  useEffect(() => {
    void refresh();
  }, []);

  // Re-pull whenever the window becomes visible. Tauri emits a
  // window-event we can subscribe to.
  useEffect(() => {
    let unlistenFocus: (() => void) | null = null;
    void getCurrentWindow()
      .onFocusChanged((focused) => {
        if (focused) void refresh();
      })
      .then((unfn) => {
        unlistenFocus = unfn;
      });
    return () => {
      unlistenFocus?.();
    };
  }, []);

  // Also listen for push updates so a fast-second-hover doesn't
  // need a full Tauri command roundtrip.
  useEffect(() => {
    const unlisten = listen<{ provider_id: string }>(
      "cost-popover:set-provider",
      async (event) => {
        const pid = event.payload?.provider_id;
        if (!pid) return;
        if (!mounted.current) return;
        setProviderId(pid);
        try {
          const all = await invoke<Record<string, ProviderCostSnapshot>>(
            "cost_snapshots",
          );
          if (!mounted.current) return;
          setSnapshot(all?.[pid] ?? null);
        } catch {
          /* ignore */
        }
      },
    );
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

  // Trigger the slide-in animation when a provider is set.
  useEffect(() => {
    if (!providerId) {
      setAnimateIn(false);
      return;
    }
    const id = window.requestAnimationFrame(() => {
      if (mounted.current) setAnimateIn(true);
    });
    return () => window.cancelAnimationFrame(id);
  }, [providerId]);

  // Hover bridge — keep the popover alive while cursor is over it.
  const handleEnter = () => {
    void invoke("cancel_cost_popover_close").catch(() => {});
  };
  const handleLeave = () => {
    void invoke("schedule_cost_popover_close").catch(() => {});
  };

  // We render the chrome unconditionally (transparent until
  // animateIn flips) so the popover window always has DOM elements
  // with hover handlers attached. If providerId is null we show a
  // brief loading state inside; if it stays null the window itself
  // is hidden by Rust so the user never sees this fallback.
  const brand = providerId
    ? (PROVIDER_BRAND_HEX[providerId] ?? "#0078d4")
    : "#0078d4";

  return (
    <div
      className={"cost-popover" + (animateIn ? " cost-popover--in" : "")}
      onMouseEnter={handleEnter}
      onMouseLeave={handleLeave}
    >
      <div className="cost-popover__chrome">
        <header className="cost-popover__header">
          <div className="cost-popover__title">
            {providerId === "claude"
              ? "Claude"
              : providerId === "codex"
                ? "Codex"
                : providerId
                  ? providerId
                  : "Loading…"}{" "}
            · Usage history
          </div>
          <div className="cost-popover__subtitle">Last 30 days</div>
        </header>
        <div className="cost-popover__body">
          <CostHistoryChart
            snapshot={snapshot}
            brandColor={brand}
            visible={animateIn}
          />
        </div>
      </div>
    </div>
  );
}
