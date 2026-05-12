import { useUsageStore } from "../state/usageStore";
import { SwitcherTab } from "./SwitcherTab";

// Phase 3 D3: row of provider tabs that lives directly under the header
// title. Layout switches between inline (single row), stacked 2 row
// (>3 visible providers), and stacked 4 row (>=15 providers) per
// spec 15 table 8.2. We only render when there is more than one
// provider so single-provider users see no extra chrome.

export function ProviderSwitcherButtons() {
  const descriptors = useUsageStore((s) => s.descriptors);
  const selectedId = useUsageStore((s) => s.selectedProviderId);
  const selectProvider = useUsageStore((s) => s.selectProvider);

  if (descriptors.length < 2) return null;

  const stacked = descriptors.length > 3;
  const fourRows = descriptors.length >= 15;
  const variant = fourRows
    ? "switcher--4rows"
    : stacked
      ? "switcher--stacked"
      : "switcher--inline";

  return (
    <div className={`switcher ${variant}`} role="tablist">
      {descriptors.map((d) => (
        <SwitcherTab
          key={d.id}
          descriptor={d}
          selected={d.id === selectedId}
          // Phase 4 wires real weekly snapshots; for now show "full"
          // so the indicator demonstrates the layout.
          weeklyRemainingPercent={100}
          onSelect={() => selectProvider(d.id)}
        />
      ))}
    </div>
  );
}
