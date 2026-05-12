// Phase 3 D5: view model that the popup uses to render a provider card.
// Phase 4 fills these in from real provider snapshots; for the scaffold
// we keep the shape close to the Mac source so the React layer is stable
// across both renderers.

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
}

export interface ProviderSnapshot {
  id: string;
  displayName: string;
  brandAccent: string;
  email: string | null;
  plan: string | null;
  subtitle: string | null;
  metrics: Metric[];
  status: { severity: StatusSeverity; title: string | null } | null;
}
