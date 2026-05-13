import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProviderCostSnapshot } from "./useCostHistory";

// The macOS "Cost" section that sits inside the provider card:
//
//   Cost                                  Hover for 30-day chart
//   Today          $0.42 · 18.4K tokens
//   Last 30 days   $4.21 · 184K tokens
//
// On hover, invoke `show_cost_popover` (Rust) which positions the
// floating cost-popover Tauri window beside the main popup (left
// edge, falling back to right if no room) and reveals it. On
// mouseleave we invoke `schedule_cost_popover_close` so the Rust
// side runs a 360 ms cancellable close timer — the popover's own
// content invokes `cancel_cost_popover_close` while the cursor is
// over it, keeping it alive across the gap between trigger and
// panel.

const HOVER_OPEN_DELAY_MS = 180;

interface Props {
  providerId: string;
  snapshot: ProviderCostSnapshot | null;
}

function formatUsd(n: number | undefined): string {
  if (n == null || n === 0) return "$0";
  if (n < 0.01) return "<$0.01";
  if (n < 10) return `$${n.toFixed(2)}`;
  if (n < 100) return `$${n.toFixed(2)}`;
  return `$${n.toFixed(0)}`;
}

function formatTokens(n: number | undefined): string {
  if (n == null) return "0";
  if (n < 1000) return `${n}`;
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}K`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

export function CostOverviewRow({ providerId, snapshot }: Props) {
  const [active, setActive] = useState(false);
  const openTimer = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (openTimer.current) window.clearTimeout(openTimer.current);
    };
  }, []);

  const today = snapshot?.daily?.[snapshot.daily.length - 1] ?? null;
  const todayCost = today?.cost_usd ?? 0;
  const todayTokens = today?.total_tokens ?? 0;
  const monthCost = snapshot?.total_window_usd ?? 0;
  const monthTokens =
    snapshot?.daily?.reduce((acc, d) => acc + d.total_tokens, 0) ?? 0;

  const handleEnter = () => {
    // Cancel any pending close that the Rust side scheduled.
    void invoke("cancel_cost_popover_close").catch(() => {});
    if (openTimer.current) return;
    openTimer.current = window.setTimeout(() => {
      openTimer.current = null;
      setActive(true);
      void invoke("show_cost_popover", { providerId }).catch(() => {});
    }, HOVER_OPEN_DELAY_MS);
  };

  const handleLeave = () => {
    if (openTimer.current) {
      window.clearTimeout(openTimer.current);
      openTimer.current = null;
    }
    setActive(false);
    void invoke("schedule_cost_popover_close").catch(() => {});
  };

  return (
    <div
      className={
        "cost-overview" + (active ? " cost-overview--active" : "")
      }
      onMouseEnter={handleEnter}
      onMouseLeave={handleLeave}
      onFocus={handleEnter}
      onBlur={handleLeave}
      tabIndex={0}
      role="button"
      aria-label="Cost history — hover to see 30 day chart"
      aria-expanded={active}
    >
      <div className="cost-overview__header">
        <span className="cost-overview__title">Cost</span>
        <span className="cost-overview__hint">Hover for 30-day chart</span>
      </div>
      <div className="cost-overview__line">
        <span className="cost-overview__line-label">Today</span>
        <span className="cost-overview__line-value">
          {formatUsd(todayCost)}
          <span className="cost-overview__line-tokens">
            · {formatTokens(todayTokens)} tokens
          </span>
        </span>
      </div>
      <div className="cost-overview__line">
        <span className="cost-overview__line-label">Last 30 days</span>
        <span className="cost-overview__line-value">
          {formatUsd(monthCost)}
          <span className="cost-overview__line-tokens">
            · {formatTokens(monthTokens)} tokens
          </span>
        </span>
      </div>
    </div>
  );
}
