import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ProviderDescriptorDto,
  UsageEventPayload,
} from "../bindings";
import { EVENTS } from "../bindings";

interface UsageState {
  descriptors: ProviderDescriptorDto[];
  lastUpdate: UsageEventPayload | null;
}

export function useUsageEvents(): UsageState {
  const [state, setState] = useState<UsageState>({
    descriptors: [],
    lastUpdate: null,
  });

  useEffect(() => {
    let cancelled = false;
    void invoke<ProviderDescriptorDto[]>("provider_descriptors").then(
      (descriptors) => {
        if (!cancelled) {
          setState((prev) => ({ ...prev, descriptors }));
        }
      },
    );

    const unlisten = listen<UsageEventPayload>(EVENTS.USAGE_UPDATED, (event) => {
      setState((prev) => ({ ...prev, lastUpdate: event.payload }));
    });

    return () => {
      cancelled = true;
      void unlisten.then((f) => f());
    };
  }, []);

  return state;
}
