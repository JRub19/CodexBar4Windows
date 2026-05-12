import { useUsageStore } from "../state/usageStore";
import { ProviderCard } from "./ProviderCard";

// Phase 3 D4: container that renders one ProviderCard per active
// provider. The card is keyed by `provider.id` so switching tabs cross
// fades via the CSS `popup-card-fade` animation defined in popup.css
// (120 ms per spec 80 section 8). Layout height tweens via the
// container's `transition: height 220ms ease-out`.

export function CardStack() {
  const descriptors = useUsageStore((s) => s.descriptors);
  const selectedId = useUsageStore((s) => s.selectedProviderId);
  const active = descriptors.find((d) => d.id === selectedId) ?? descriptors[0];

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
