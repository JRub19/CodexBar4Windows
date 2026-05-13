import { useEffect, useRef } from "react";
import { useUsageStore } from "../state/usageStore";
import type { ProviderCostSnapshot } from "./useCostHistory";

// The macOS "Cost" section that sits inside the provider card:
//
//   Cost
//   Today: $0.42 · 18.4K tokens
//   Last 30 days: $4.21 · 184K tokens
//
// Hovering the row pops a side panel (see PopupShell + CostSidePanel)
// with the full 30-day chart. The submenu hover delay matches the
// macOS AppKit cascade: ~200 ms to open, ~400 ms grace before closing
// so the cursor has time to bridge the gap from the trigger to the
// panel content.

const HOVER_OPEN_DELAY_MS = 180;
const HOVER_CLOSE_DELAY_MS = 360;

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
  const showCostPanel = useUsageStore((s) => s.showCostPanel);
  const hideCostPanel = useUsageStore((s) => s.hideCostPanel);
  const activePanelId = useUsageStore((s) => s.costPanelProviderId);
  const openTimer = useRef<number | null>(null);
  const closeTimer = useRef<number | null>(null);

  // Cancel any pending timers on unmount so a stale callback can't
  // try to update store state after the card has gone away.
  useEffect(() => {
    return () => {
      if (openTimer.current) window.clearTimeout(openTimer.current);
      if (closeTimer.current) window.clearTimeout(closeTimer.current);
    };
  }, []);

  const isActive = activePanelId === providerId;

  // Pull today/30d from snapshot. `daily` is oldest→newest, so the
  // last element is today.
  const today = snapshot?.daily?.[snapshot.daily.length - 1] ?? null;
  const todayCost = today?.cost_usd ?? 0;
  const todayTokens = today?.total_tokens ?? 0;
  const monthCost = snapshot?.total_window_usd ?? 0;
  const monthTokens =
    snapshot?.daily?.reduce((acc, d) => acc + d.total_tokens, 0) ?? 0;

  const handleEnter = () => {
    if (closeTimer.current) {
      window.clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
    if (openTimer.current) return;
    openTimer.current = window.setTimeout(() => {
      openTimer.current = null;
      showCostPanel(providerId);
    }, HOVER_OPEN_DELAY_MS);
  };

  const handleLeave = () => {
    if (openTimer.current) {
      window.clearTimeout(openTimer.current);
      openTimer.current = null;
    }
    if (closeTimer.current) return;
    closeTimer.current = window.setTimeout(() => {
      closeTimer.current = null;
      hideCostPanel();
    }, HOVER_CLOSE_DELAY_MS);
  };

  return (
    <div
      className={
        "cost-overview" + (isActive ? " cost-overview--active" : "")
      }
      onMouseEnter={handleEnter}
      onMouseLeave={handleLeave}
      onFocus={handleEnter}
      onBlur={handleLeave}
      tabIndex={0}
      role="button"
      aria-label="Cost history — hover to see 30 day chart"
      aria-expanded={isActive}
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
