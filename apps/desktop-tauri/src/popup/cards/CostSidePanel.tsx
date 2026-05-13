import { useEffect, useRef } from "react";
import { useUsageStore } from "../state/usageStore";
import { useCostHistory } from "./useCostHistory";
import { CostHistoryChart } from "./CostHistoryChart";

// The side panel that opens when the user hovers a provider card's
// Cost overview row. Mirrors the macOS "Usage history (30 days)"
// submenu: a panel that sits visually beside the main popup with the
// 30-day bar chart + footer total. Implementation note: we don't
// open a second OS window — `PopupShell` lays out a fixed-position
// element to the left of the popup content and `useAutoResize`
// widens the OS window so the panel fits. Single window = no IPC,
// no focus issues, no positioning math.
//
// Hover bridge: as long as the cursor is over EITHER the trigger
// row OR this panel, the panel stays open. Each side cancels the
// other's close timer.

const HOVER_CLOSE_DELAY_MS = 360;

const PROVIDER_BRAND_HEX: Record<string, string> = {
  claude: "#cc7c5e",
  codex: "#10a37f",
};

export function CostSidePanel() {
  const providerId = useUsageStore((s) => s.costPanelProviderId);
  const hide = useUsageStore((s) => s.hideCostPanel);
  const cost = useCostHistory();
  const closeTimer = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (closeTimer.current) window.clearTimeout(closeTimer.current);
    };
  }, []);

  if (!providerId) return null;

  const snapshot = cost.byProvider[providerId] ?? null;
  const brand = PROVIDER_BRAND_HEX[providerId] ?? "var(--accent)";

  const handleEnter = () => {
    if (closeTimer.current) {
      window.clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
  };
  const handleLeave = () => {
    if (closeTimer.current) return;
    closeTimer.current = window.setTimeout(() => {
      closeTimer.current = null;
      hide();
    }, HOVER_CLOSE_DELAY_MS);
  };

  return (
    <aside
      className="cost-side-panel"
      onMouseEnter={handleEnter}
      onMouseLeave={handleLeave}
      role="dialog"
      aria-label="Cost history for last 30 days"
    >
      <header className="cost-side-panel__header">
        <span className="cost-side-panel__title">
          {providerId === "claude"
            ? "Claude"
            : providerId === "codex"
              ? "Codex"
              : providerId}{" "}
          · Usage history
        </span>
        <span className="cost-side-panel__subtitle">Last 30 days</span>
      </header>
      <div className="cost-side-panel__body">
        <CostHistoryChart
          snapshot={snapshot}
          brandColor={brand}
          visible={true}
        />
      </div>
    </aside>
  );
}
