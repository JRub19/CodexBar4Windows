import { useUsageStore, useEnabledDescriptors } from "../state/usageStore";
import { SwitcherTab } from "./SwitcherTab";

// Row of provider tabs that hosts the popup-wide provider selection.
// Only renders when ≥2 providers are enabled — single-provider users
// see no chrome at all. The switcher is a pill-segmented control;
// active segment shows a thin accent underline that slides between
// tabs on switch (handled by SwitcherTab + the ::after rule in CSS).
//
// For typical users (2-4 providers) tabs share the row evenly. With
// 5+ providers the container wraps to a CSS-grid stacked layout via
// the `switcher--stacked` modifier.

export function ProviderSwitcherButtons() {
  const enabled = useEnabledDescriptors();
  const selectedId = useUsageStore((s) => s.selectedProviderId);
  const selectProvider = useUsageStore((s) => s.selectProvider);

  if (enabled.length < 2) return null;

  // Inline single-row layout works cleanly up to 3 tabs; 4+ tabs go
  // to the stacked 2-column grid so labels never truncate.
  const stacked = enabled.length > 3;

  return (
    <div
      className={`switcher${stacked ? " switcher--stacked" : ""}`}
      role="tablist"
    >
      {enabled.map((d) => (
        <SwitcherTab
          key={d.id}
          descriptor={d}
          selected={d.id === selectedId}
          onSelect={() => selectProvider(d.id)}
        />
      ))}
    </div>
  );
}
