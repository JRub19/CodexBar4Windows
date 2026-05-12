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

interface UsageStoreState {
  descriptors: ProviderDescriptorDto[];
  lastUsageEvent: UsageEventPayload | null;
  lastStatusEvent: StatusEventPayload | null;
  theme: Theme;
  selectedProviderId: string | null;
  setDescriptors: (next: ProviderDescriptorDto[]) => void;
  applyUsageEvent: (event: UsageEventPayload) => void;
  applyStatusEvent: (event: StatusEventPayload) => void;
  setTheme: (theme: Theme) => void;
  selectProvider: (id: string | null) => void;
}

export const useUsageStore = create<UsageStoreState>((set) => ({
  descriptors: [],
  lastUsageEvent: null,
  lastStatusEvent: null,
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
  applyUsageEvent: (event) => set({ lastUsageEvent: event }),
  applyStatusEvent: (event) => set({ lastStatusEvent: event }),
  setTheme: (theme) => set({ theme }),
  selectProvider: (id) => set({ selectedProviderId: id }),
}));
