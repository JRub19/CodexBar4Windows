import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProviderDescriptorDto } from "../../bindings";
import { useUsageStore, type ProviderSlot } from "../state/usageStore";
import { CardHeader } from "./CardHeader";
import { HeroMetric } from "./HeroMetric";
import { MetricRow } from "./MetricRow";
import { Icon } from "../../components/Icon";
import type { Metric, ProviderSnapshot } from "./snapshot";

// The popup's per-provider card. Top to bottom:
//
//   1. CardHeader  — provider name + brand swatch + plan + email
//   2. HeroMetric  — the primary metric (session) with big 36px number,
//                    bar, reset countdown, optional pace text
//   3. Divider
//   4. Secondary metrics (weekly, credits, …) as compact MetricRows
//   5. Optional status block (already rendered inside CardHeader)
//
// The card has four states selected on the slot's data:
//   - has data → above layout
//   - loading first refresh → skeleton hero (em-dash + shimmer bar)
//   - no data after refresh → empty hint with "Refresh now" CTA
//   - refresh failed → error icon + message + Retry/Copy buttons

interface Props {
  descriptor: ProviderDescriptorDto;
}

function placeholderSnapshot(d: ProviderDescriptorDto): ProviderSnapshot {
  return {
    id: d.id,
    displayName: d.metadata.display_name,
    brandAccent: d.branding.accent_hex,
    email: null,
    plan: null,
    subtitle: null,
    metrics: [],
    status: null,
  };
}

function metricFromWindow(w: ProviderSlot["snapshot"]["windows"][number]): Metric {
  const { used, allotted, reset_at_unix_secs } = w.window;
  const percent =
    allotted && allotted > 0
      ? Math.max(0, Math.min(100, (used / allotted) * 100))
      : null;
  const resetText = reset_at_unix_secs
    ? formatResetCountdown(reset_at_unix_secs)
    : null;
  const detailRight =
    allotted != null
      ? `${formatNumber(used)} / ${formatNumber(allotted)}`
      : null;
  return {
    title: w.window.label,
    percent,
    detailLeft: null,
    detailRight,
    resetText,
  };
}

function formatNumber(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
  return value.toFixed(0);
}

function formatResetCountdown(unixSecs: number): string {
  const now = Math.floor(Date.now() / 1000);
  const delta = unixSecs - now;
  if (delta <= 0) return "now";
  const hours = Math.floor(delta / 3600);
  const minutes = Math.floor((delta % 3600) / 60);
  if (hours >= 24) {
    const days = Math.floor(hours / 24);
    return `in ${days}d ${hours % 24}h`;
  }
  if (hours > 0) return `in ${hours}h ${minutes}m`;
  return `in ${minutes}m`;
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
  const [retrying, setRetrying] = useState(false);
  const [stillWaitingAfterBoot, setStillWaitingAfterBoot] = useState(false);

  // If we still have no slot 8 seconds after mount, the refresh
  // either errored or produced no snapshot (most likely: no
  // credentials yet). Surface a sign-in hint instead of an infinite
  // shimmer.
  useEffect(() => {
    if (slot) {
      setStillWaitingAfterBoot(false);
      return;
    }
    const t = window.setTimeout(() => setStillWaitingAfterBoot(true), 8000);
    return () => window.clearTimeout(t);
  }, [slot]);

  const onRetry = async () => {
    setRetrying(true);
    try {
      await invoke("refresh_now");
    } catch {
      /* ignore */
    } finally {
      setRetrying(false);
    }
  };

  const onOpenSettings = async () => {
    try {
      await invoke("open_preferences", { providerId: descriptor.id });
    } catch {
      /* ignore */
    }
  };

  // No slot at all — distinguish two cases by elapsed time since mount.
  if (!slot) {
    const snapshot = placeholderSnapshot(descriptor);
    return (
      <article className="provider-card">
        <CardHeader snapshot={snapshot} />
        {stillWaitingAfterBoot ? (
          <CardNoCredentials
            onOpenSettings={() => void onOpenSettings()}
            onRetry={() => void onRetry()}
            retrying={retrying}
          />
        ) : (
          <CardLoading />
        )}
      </article>
    );
  }

  const snapshot = snapshotFromSlot(descriptor, slot);

  // Slot exists but every metric lacks a percent — treat as empty.
  const hasAnyPercent = snapshot.metrics.some((m) => m.percent != null);
  if (!hasAnyPercent) {
    return (
      <article className="provider-card">
        <CardHeader snapshot={snapshot} />
        <CardEmpty onRefresh={() => void onRetry()} loading={retrying} />
      </article>
    );
  }

  const primary = snapshot.metrics[0];
  const secondary = snapshot.metrics.slice(1);

  return (
    <article className="provider-card">
      <CardHeader snapshot={snapshot} />
      <HeroMetric metric={primary} />
      {secondary.length > 0 ? (
        <div className="provider-card__metrics">
          {secondary.map((metric, idx) => (
            <MetricRow key={`${snapshot.id}-${idx}`} metric={metric} />
          ))}
        </div>
      ) : null}
    </article>
  );
}

function CardLoading() {
  return (
    <div className="card-state">
      <div className="hero-metric__value" style={{ color: "var(--text-tertiary)" }}>
        —
      </div>
      <div className="skeleton-bar" />
      <div className="card-state__body">Fetching latest usage…</div>
    </div>
  );
}

function CardEmpty({
  onRefresh,
  loading,
}: {
  onRefresh: () => void;
  loading: boolean;
}) {
  return (
    <div className="card-state">
      <div className="card-state__icon card-state__icon--accent">
        <Icon name="sparkles" size={24} />
      </div>
      <div className="card-state__title">No usage yet</div>
      <div className="card-state__body">
        Start a session — data will appear within a minute.
      </div>
      <div className="card-state__actions">
        <button
          type="button"
          className="btn-secondary"
          onClick={onRefresh}
          disabled={loading}
        >
          <Icon name="refresh" size={14} />
          {loading ? "Refreshing…" : "Refresh now"}
        </button>
      </div>
    </div>
  );
}

// Shown when the refresh attempt clearly couldn't fetch anything —
// typically because no credentials were found on disk. Surfaces a
// clear sign-in path instead of an infinite loading shimmer.
function CardNoCredentials({
  onOpenSettings,
  onRetry,
  retrying,
}: {
  onOpenSettings: () => void;
  onRetry: () => void;
  retrying: boolean;
}) {
  return (
    <div className="card-state">
      <div className="card-state__icon card-state__icon--accent">
        <Icon name="info" size={24} />
      </div>
      <div className="card-state__title">Sign in to fetch your quota</div>
      <div className="card-state__body">
        No credentials found on disk. Open Settings to sign in — or
        retry if you just signed in.
      </div>
      <div className="card-state__actions">
        <button
          type="button"
          className="btn-primary"
          onClick={onOpenSettings}
        >
          <Icon name="settings" size={14} />
          Open Settings
        </button>
        <button
          type="button"
          className="btn-secondary"
          onClick={onRetry}
          disabled={retrying}
        >
          <Icon name="refresh" size={14} />
          {retrying ? "Retrying…" : "Retry"}
        </button>
      </div>
    </div>
  );
}
