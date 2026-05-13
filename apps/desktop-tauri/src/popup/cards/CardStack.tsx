import { useUsageStore, useEnabledDescriptors } from "../state/usageStore";
import { ProviderCard } from "./ProviderCard";

// Renders the currently-selected provider's card. Filters descriptors
// to only those the user has enabled — disabled providers never
// surface in the popup body. The selection state is owned by the
// store; the switcher (when ≥2 providers are enabled) drives it.

export function CardStack() {
  const enabled = useEnabledDescriptors();
  const selectedId = useUsageStore((s) => s.selectedProviderId);
  const active =
    enabled.find((d) => d.id === selectedId) ?? enabled[0] ?? null;

  if (!active) {
    return (
      <div className="card-stack card-stack--empty">
        <p className="card-stack__empty-text">No provider configured yet.</p>
      </div>
    );
  }

  return (
    <div className="card-stack">
      <ProviderCard key={active.id} descriptor={active} />
    </div>
  );
}
