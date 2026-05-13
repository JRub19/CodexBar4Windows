import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CostHistoryChart } from "../cards/CostHistoryChart";
import type { ProviderCostSnapshot } from "../cards/useCostHistory";

// Render target for the `cost-popover` Tauri window. The window
// itself is created via tauri.conf.json with visible: false; the
// Rust side flips it visible/hidden, positions it beside the main
// popup, and emits `cost-popover:set-provider` events to tell us
// which provider's chart to render.
//
// Hover bridge: the popover keeps itself alive while the cursor is
// inside its own DOM by invoking `cancel_cost_popover_close` on
// mouseenter and `schedule_cost_popover_close` on mouseleave. The
// main-popup trigger row does the symmetric thing. Either side
// staying hovered prevents the close.

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

  // Subscribe to provider changes from Rust. The Rust side emits
  // this event right before / when showing the window.
  useEffect(() => {
    const unlisten = listen<{ provider_id: string }>(
      "cost-popover:set-provider",
      async (event) => {
        const pid = event.payload?.provider_id;
        if (!pid) return;
        setProviderId(pid);
        setAnimateIn(false);
        // Fetch snapshots fresh — the main popup may have already
        // populated the cache, in which case this is O(1).
        try {
          const all = await invoke<Record<string, ProviderCostSnapshot>>(
            "cost_snapshots",
          );
          if (!mounted.current) return;
          setSnapshot(all?.[pid] ?? null);
        } catch {
          setSnapshot(null);
        }
        // Trigger CSS enter animation on the next frame so the
        // browser sees the from→to state transition.
        requestAnimationFrame(() => {
          if (mounted.current) setAnimateIn(true);
        });
      },
    );
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

  // Hover bridge — keep the popover alive while cursor is over it.
  const handleEnter = () => {
    void invoke("cancel_cost_popover_close").catch(() => {});
  };
  const handleLeave = () => {
    void invoke("schedule_cost_popover_close").catch(() => {});
  };

  if (!providerId) return null;
  const brand = PROVIDER_BRAND_HEX[providerId] ?? "#0078d4";

  return (
    <div
      className={
        "cost-popover" + (animateIn ? " cost-popover--in" : "")
      }
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
                : providerId}{" "}
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
