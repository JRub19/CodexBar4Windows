// Phase 3 D1: Zustand store that mirrors the live usage event stream the
// Rust core publishes via Tauri. The popup subscribes once at mount in
// `PopupShell` and components read slices via selectors. Phase 4 fills in
// real provider snapshots; for now we hold the descriptor list plus the
// most recent `UsageEventPayload` so the UI can render skeleton states.

import { useMemo } from "react";
import { create } from "zustand";
import type {
  ProviderDescriptorDto,
  StatusEventPayload,
  UsageEventPayload,
} from "../../bindings";

export type Theme = "dark" | "light";

// Mirrors `providers::models::UsageSnapshot` from the Rust core.
export interface NamedRateWindow {
  key: string;
  window: {
    label: string;
    used: number;
    allotted: number | null;
    reset_at_unix_secs: number | null;
    pace_delta_percent: number | null;
  };
}

export interface UsageSnapshot {
  identity: { provider_id: string; account_token: string };
  windows: NamedRateWindow[];
  credits: unknown | null;
  cost: unknown | null;
  account_display_name: string | null;
  account_email: string | null;
  plan_name: string | null;
  captured_at_unix_secs: number;
}

export interface ProviderSlot {
  snapshot: UsageSnapshot;
  attempts: Array<{
    strategy: string;
    duration_ms: number;
    error_kind: string | null;
    error_detail: string | null;
  }>;
}

interface UsageStoreState {
  descriptors: ProviderDescriptorDto[];
  /** Provider IDs the user has explicitly enabled. `null` means "no
   *  preference set yet" — treat every registered provider as enabled.
   *  This mirrors the refresh-loop's empty-vec semantics so a fresh
   *  install shows every available provider until the user disables
   *  some via Preferences → Providers. */
  enabledProviderIds: string[] | null;
  lastUsageEvent: UsageEventPayload | null;
  lastStatusEvent: StatusEventPayload | null;
  snapshots: Record<string, ProviderSlot>;
  theme: Theme;
  selectedProviderId: string | null;
  setDescriptors: (next: ProviderDescriptorDto[]) => void;
  setEnabledProviderIds: (next: string[] | null) => void;
  setSnapshots: (next: Record<string, ProviderSlot>) => void;
  applyUsageEvent: (event: UsageEventPayload) => void;
  applyStatusEvent: (event: StatusEventPayload) => void;
  setTheme: (theme: Theme) => void;
  selectProvider: (id: string | null) => void;
  /** Provider id whose cost-history side panel is currently shown,
   *  or `null` when no panel is open. Driven by the per-card cost
   *  row's hover state in `ProviderCard`. */
  costPanelProviderId: string | null;
  showCostPanel: (id: string) => void;
  hideCostPanel: () => void;
}

export const useUsageStore = create<UsageStoreState>((set) => ({
  descriptors: [],
  enabledProviderIds: null,
  lastUsageEvent: null,
  lastStatusEvent: null,
  snapshots: {},
  theme: "dark",
  selectedProviderId: null,
  setDescriptors: (next) =>
    set((s) => ({
      descriptors: next,
      selectedProviderId:
        s.selectedProviderId &&
        next.some((d) => d.id === s.selectedProviderId)
          ? s.selectedProviderId
          : (next[0]?.id ?? null),
    })),
  setEnabledProviderIds: (next) =>
    set((s) => {
      // Re-pick selection if the currently-selected provider was
      // just disabled. Fall back to the first enabled descriptor.
      const allowed = next == null ? s.descriptors.map((d) => d.id) : next;
      const selectionStillValid =
        s.selectedProviderId != null && allowed.includes(s.selectedProviderId);
      const firstEnabled = s.descriptors.find((d) => allowed.includes(d.id));
      return {
        enabledProviderIds: next,
        selectedProviderId: selectionStillValid
          ? s.selectedProviderId
          : (firstEnabled?.id ?? null),
      };
    }),
  setSnapshots: (next) => set({ snapshots: next }),
  applyUsageEvent: (event) => set({ lastUsageEvent: event }),
  applyStatusEvent: (event) => set({ lastStatusEvent: event }),
  setTheme: (theme) => set({ theme }),
  selectProvider: (id) => set({ selectedProviderId: id }),
  costPanelProviderId: null,
  showCostPanel: (id) => set({ costPanelProviderId: id }),
  hideCostPanel: () => set({ costPanelProviderId: null }),
}));

/** Helper selector — returns descriptors filtered by the
 *  enabledProviderIds set (or all of them when no preference exists).
 *
 *  Implementation note: the filtered array is derived in a `useMemo`
 *  outside the Zustand selector so we never hand React a fresh array
 *  reference from `getSnapshot`. The previous inline-filter version
 *  produced a new array on every call to the selector, which made
 *  `useSyncExternalStore` see "the snapshot changed" on every render
 *  and triggered "Maximum update depth exceeded" the moment
 *  `enabledProviderIds` became non-null (i.e. as soon as the user
 *  toggled any provider in Preferences). */
export function useEnabledDescriptors(): ProviderDescriptorDto[] {
  const descriptors = useUsageStore((s) => s.descriptors);
  const enabledIds = useUsageStore((s) => s.enabledProviderIds);
  return useMemo(() => {
    if (enabledIds == null) return descriptors;
    const enabled = new Set(enabledIds);
    return descriptors.filter((d) => enabled.has(d.id));
  }, [descriptors, enabledIds]);
}
