import type { ProviderDescriptorDto } from "../../bindings";
import { CardHeader } from "./CardHeader";
import { MetricRow } from "./MetricRow";
import type { ProviderSnapshot } from "./snapshot";

// Phase 3 D5: ProviderCard renders header + metric rows + status pill.
// Phase 4 swaps `snapshotFromDescriptor` for real usage snapshots wired
// through `provider_snapshots`. Today we emit a placeholder snapshot so
// the layout is visible during Phase 3 manual QA.

interface Props {
  descriptor: ProviderDescriptorDto;
}

function snapshotFromDescriptor(d: ProviderDescriptorDto): ProviderSnapshot {
  return {
    id: d.id,
    displayName: d.metadata.display_name,
    brandAccent: d.branding.accent_hex,
    email: null,
    plan: null,
    subtitle: "Awaiting first refresh",
    metrics: [
      {
        title: "Session",
        percent: null,
        detailLeft: null,
        detailRight: "No data yet",
        resetText: null,
      },
    ],
    status: null,
  };
}

export function ProviderCard({ descriptor }: Props) {
  const snapshot = snapshotFromDescriptor(descriptor);
  return (
    <article
      className="provider-card"
      style={
        { "--card-accent": snapshot.brandAccent } as React.CSSProperties
      }
    >
      <CardHeader snapshot={snapshot} />
      <div className="provider-card__metrics">
        {snapshot.metrics.map((metric, idx) => (
          <MetricRow
            key={`${snapshot.id}-${idx}`}
            metric={metric}
            brandAccent={snapshot.brandAccent}
          />
        ))}
      </div>
    </article>
  );
}
