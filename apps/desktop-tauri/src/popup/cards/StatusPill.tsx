import type { StatusSeverity } from "./snapshot";

// Phase 3 D5: status pill per spec 15 section 4.5. Hidden when severity
// is operational. Color tokens come from the provider's brand accent for
// neutral states, and from a fixed warning ramp for incidents.

interface Props {
  severity: StatusSeverity;
  title: string | null;
  statusPageUrl: string | null;
}

const LABELS: Record<StatusSeverity, string> = {
  operational: "Operational",
  degraded: "Degraded",
  partial_outage: "Partial outage",
  major_outage: "Major outage",
  investigating: "Investigating",
};

export function StatusPill({ severity, title, statusPageUrl }: Props) {
  if (severity === "operational") return null;
  const label = title ?? LABELS[severity];
  return (
    <button
      type="button"
      className={`status-pill status-pill--${severity}`}
      onClick={() => {
        if (statusPageUrl) void window.open(statusPageUrl, "_blank");
      }}
    >
      {label}
    </button>
  );
}
