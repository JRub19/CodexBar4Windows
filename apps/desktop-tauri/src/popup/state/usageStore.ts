// Phase 3 D1: Zustand store that mirrors the live usage event stream the
// Rust core publishes via Tauri. The popup subscribes once at mount in
// `PopupShell` and components read slices via selectors. Phase 4 fills in
// real provider snapshots; for now we hold the descriptor list plus the
// most recent `UsageEventPayload` so the UI can render skeleton states.

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
  lastUsageEvent: UsageEventPayload | null;
  lastStatusEvent: StatusEventPayload | null;
  snapshots: Record<string, ProviderSlot>;
  theme: Theme;
  selectedProviderId: string | null;
  setDescriptors: (next: ProviderDescriptorDto[]) => void;
  setSnapshots: (next: Record<string, ProviderSlot>) => void;
  applyUsageEvent: (event: UsageEventPayload) => void;
  applyStatusEvent: (event: StatusEventPayload) => void;
  setTheme: (theme: Theme) => void;
  selectProvider: (id: string | null) => void;
}

export const useUsageStore = create<UsageStoreState>((set) => ({
  descriptors: [],
  lastUsageEvent: null,
  lastStatusEvent: null,
  snapshots: {},
  theme: "dark",
  selectedProviderId: null,
  setDescriptors: (next) =>
    set((s) => ({
      descriptors: next,
      // Preserve a selection when the descriptor still exists, otherwise
      // pick the first one so the popup always has something to display.
      selectedProviderId:
        s.selectedProviderId &&
        next.some((d) => d.id === s.selectedProviderId)
          ? s.selectedProviderId
          : (next[0]?.id ?? null),
    })),
  setSnapshots: (next) => set({ snapshots: next }),
  applyUsageEvent: (event) => set({ lastUsageEvent: event }),
  applyStatusEvent: (event) => set({ lastStatusEvent: event }),
  setTheme: (theme) => set({ theme }),
  selectProvider: (id) => set({ selectedProviderId: id }),
}));
