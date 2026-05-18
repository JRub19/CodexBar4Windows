// View model that the popup uses to render a provider card. Keep the shape
// close to the Rust provider snapshots so the React layer can stay stable.

export type StatusSeverity =
  | "operational"
  | "degraded"
  | "partial_outage"
  | "major_outage"
  | "investigating";

export interface Metric {
  title: string;
  // Percent 0..100, or null if the provider does not expose a quota.
  percent: number | null;
  // Free-form right side detail, e.g. "12.4M / 20M tokens".
  detailRight: string | null;
  // Free-form left side detail, e.g. "$0.42 / $20".
  detailLeft: string | null;
  // Reset hint shown above the bar on the right.
  resetText: string | null;
  // Raw absolute reset time (unix seconds, UTC). Used by the pace
  // calculator to project ideal vs actual usage; null when the
  // provider doesn't expose a reset clock.
  resetAtUnixSecs: number | null;
  // Original window label, used by the pace calculator to infer the
  // window's nominal duration (5h, week, day, …). The `title` field
  // is the display version which may be transformed; we keep the
  // unmodified label separately.
  windowLabel: string;
}

export interface ProviderSnapshot {
  id: string;
  displayName: string;
  brandAccent: string;
  email: string | null;
  plan: string | null;
  subtitle: string | null;
  metrics: Metric[];
  status: { severity: StatusSeverity; title: string | null; statusPageUrl: string | null } | null;
}
