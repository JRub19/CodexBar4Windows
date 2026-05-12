import type { ProviderDescriptorDto } from "../../bindings";
import { useUsageStore, type ProviderSlot } from "../state/usageStore";
import { CardHeader } from "./CardHeader";
import { MetricRow } from "./MetricRow";
import type { Metric, ProviderSnapshot } from "./snapshot";

// Phase 4 P4-20: ProviderCard renders the live `ProviderSlot` from the
// store when available, falling back to a placeholder when no refresh
// has completed yet.

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

function metricFromWindow(window: ProviderSlot["snapshot"]["windows"][number]): Metric {
  const { used, allotted, reset_at_unix_secs } = window.window;
  const percent =
    allotted && allotted > 0
      ? Math.max(0, Math.min(100, (used / allotted) * 100))
      : null;
  const resetText = reset_at_unix_secs
    ? new Date(reset_at_unix_secs * 1000).toLocaleString(undefined, {
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      })
    : null;
  const detailRight =
    allotted != null
      ? `${used.toFixed(1)} / ${allotted.toFixed(0)}`
      : `${used.toFixed(1)} used`;
  return {
    title: window.window.label,
    percent,
    detailLeft: null,
    detailRight,
    resetText,
  };
}

function snapshotFromSlot(
  d: ProviderDescriptorDto,
  slot: ProviderSlot,
): ProviderSnapshot {
  return {
    id: d.id,
    displayName: d.metadata.display_name,
    brandAccent: d.branding.accent_hex,
    email: slot.snapshot.account_email,
    plan: slot.snapshot.plan_name,
    subtitle: slot.snapshot.account_display_name,
    metrics: slot.snapshot.windows.map(metricFromWindow),
    status: null,
  };
}

export function ProviderCard({ descriptor }: Props) {
  const slot = useUsageStore((s) => s.snapshots[descriptor.id] ?? null);
  const snapshot = slot
    ? snapshotFromSlot(descriptor, slot)
    : snapshotFromDescriptor(descriptor);
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
